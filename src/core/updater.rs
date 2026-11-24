// src/core/updater.rs

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinInet::{
    HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile,
    InternetSetOptionW, HTTP_QUERY_CONTENT_LENGTH, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE,
    INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD, INTERNET_FLAG_SECURE,
    INTERNET_OPEN_TYPE_PRECONFIG, INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT,
    INTERNET_OPTION_SEND_TIMEOUT,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::trust::ensure_admin_only_data_dir;
use crate::core::win_common::to_wide_null;

const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Kurobanyi/OtterSwiss/releases/latest";

struct WinInetHandle(*mut std::ffi::c_void);

impl WinInetHandle {
    fn new(ptr: *mut std::ffi::c_void) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self(ptr))
        }
    }
    fn raw(&self) -> *mut std::ffi::c_void {
        self.0
    }
}

impl Drop for WinInetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
            self.0 = ptr::null_mut();
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: Option<String>,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone)]
pub struct UpdateReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub title: String,
    pub notes: String,
    pub download_url: String,
    pub asset_name: String,
    pub asset_size: u64,
}

fn parse_version_tuple(v: &str) -> (u32, u32, u32) {
    let clean = v.trim().trim_start_matches(['v', 'V']);
    let mut parts = clean.split('.');
    let major = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer_version(remote_tag: &str, current_ver: &str) -> bool {
    parse_version_tuple(remote_tag) > parse_version_tuple(current_ver)
}

pub fn check_for_updates() -> CoreResult<Option<UpdateReleaseInfo>> {
    let agent_w = to_wide_null(&format!("OtterSwiss-Updater/{}", env!("CARGO_PKG_VERSION")));
    let h_internet = WinInetHandle::new(unsafe {
        InternetOpenW(
            PCWSTR(agent_w.as_ptr()),
            INTERNET_OPEN_TYPE_PRECONFIG.0,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: "No se pudo inicializar la conexión WinInet para buscar actualizaciones".into(),
        source: None,
    })?;

    let timeout_ms: u32 = 10_000;
    unsafe {
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_CONNECT_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_SEND_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_RECEIVE_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
    }

    let url_w = to_wide_null(GITHUB_LATEST_RELEASE_URL);
    let h_url = WinInetHandle::new(unsafe {
        InternetOpenUrlW(
            h_internet.raw(),
            PCWSTR(url_w.as_ptr()),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: "No se pudo conectar a GitHub Releases API".into(),
        source: None,
    })?;

    let mut status_code: u32 = 0;
    let mut status_len: u32 = std::mem::size_of::<u32>() as u32;
    unsafe {
        HttpQueryInfoW(
            h_url.raw(),
            HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
            Some(&mut status_code as *mut _ as *mut _),
            &mut status_len,
            None,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("Error obteniendo estado HTTP de GitHub: {}", e),
        source: None,
    })?;

    if status_code != 200 {
        return Err(CoreError::Trust {
            message: format!("GitHub API devolvió código HTTP {}", status_code),
            source: None,
        });
    }

    let mut json_bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let mut read_bytes: u32 = 0;
        let ok = unsafe {
            InternetReadFile(
                h_url.raw(),
                chunk.as_mut_ptr() as *mut _,
                chunk.len() as u32,
                &mut read_bytes,
            )
        };
        if ok.is_err() || read_bytes == 0 {
            break;
        }
        json_bytes.extend_from_slice(&chunk[..read_bytes as usize]);
    }

    let release: GitHubRelease = serde_json::from_slice(&json_bytes).map_err(|e| {
        CoreError::Trust {
            message: format!("Error interpretando respuesta JSON de GitHub: {}", e),
            source: None,
        }
    })?;

    let current_ver = env!("CARGO_PKG_VERSION");
    if !is_newer_version(&release.tag_name, current_ver) {
        return Ok(None);
    }

    let asset = release
        .assets
        .iter()
        .find(|a| {
            let n = a.name.to_ascii_lowercase();
            n.ends_with(".exe") && (n.contains("setup") || n.contains("installer"))
        })
        .or_else(|| {
            release.assets.iter().find(|a| a.name.to_ascii_lowercase().ends_with(".exe"))
        })
        .ok_or_else(|| CoreError::Trust {
            message: "La versión nueva en GitHub no incluye un ejecutable (.exe)".into(),
            source: None,
        })?;

    let clean_version = release.tag_name.trim_start_matches(['v', 'V']).to_string();

    Ok(Some(UpdateReleaseInfo {
        version: clean_version,
        tag_name: release.tag_name,
        title: release.name.unwrap_or_else(|| "OtterSwiss Update".into()),
        notes: release.body.unwrap_or_default(),
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        asset_size: asset.size,
    }))
}

pub fn download_and_install_update(
    info: &UpdateReleaseInfo,
    cancel_flag: &Arc<AtomicBool>,
    progress_sig: &Arc<AtomicU8>,
) -> CoreResult<PathBuf> {
    if cancel_flag.load(Ordering::Acquire) {
        return Err(CoreError::Cancelled);
    }

    let target_dir = ensure_admin_only_data_dir()?.join("Updates");
    std::fs::create_dir_all(&target_dir)?;

    let target_file = target_dir.join(&info.asset_name);
    if target_file.exists() {
        let _ = std::fs::remove_file(&target_file);
    }

    let agent_w = to_wide_null(&format!("OtterSwiss-Updater/{}", env!("CARGO_PKG_VERSION")));
    let h_internet = WinInetHandle::new(unsafe {
        InternetOpenW(
            PCWSTR(agent_w.as_ptr()),
            INTERNET_OPEN_TYPE_PRECONFIG.0,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: "No se pudo iniciar WinInet para descargar la actualización".into(),
        source: None,
    })?;

    let timeout_ms: u32 = 30_000;
    unsafe {
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_CONNECT_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_SEND_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_RECEIVE_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
    }

    let url_w = to_wide_null(&info.download_url);
    let h_url = WinInetHandle::new(unsafe {
        InternetOpenUrlW(
            h_internet.raw(),
            PCWSTR(url_w.as_ptr()),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: format!("No se pudo conectar a la URL de descarga: {}", info.download_url),
        source: None,
    })?;

    let mut status_code: u32 = 0;
    let mut status_len: u32 = std::mem::size_of::<u32>() as u32;
    unsafe {
        HttpQueryInfoW(
            h_url.raw(),
            HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
            Some(&mut status_code as *mut _ as *mut _),
            &mut status_len,
            None,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("Error obteniendo código HTTP del asset: {}", e),
        source: None,
    })?;

    if status_code != 200 {
        return Err(CoreError::Trust {
            message: format!("El servidor de descarga devolvió código HTTP {}", status_code),
            source: None,
        });
    }

    let mut content_len: u32 = 0;
    let mut cl_size: u32 = std::mem::size_of::<u32>() as u32;
    let has_content_len = unsafe {
        HttpQueryInfoW(
            h_url.raw(),
            HTTP_QUERY_CONTENT_LENGTH | HTTP_QUERY_FLAG_NUMBER,
            Some(&mut content_len as *mut _ as *mut _),
            &mut cl_size,
            None,
        )
    }
    .is_ok();

    let total_expected = if has_content_len && content_len > 0 {
        content_len as u64
    } else {
        info.asset_size
    };

    let mut out_file = File::create(&target_file)?;
    let mut total_downloaded: u64 = 0;
    let mut chunk = [0u8; 32 * 1024];

    loop {
        if cancel_flag.load(Ordering::Acquire) {
            drop(out_file);
            let _ = std::fs::remove_file(&target_file);
            return Err(CoreError::Cancelled);
        }

        let mut read_bytes: u32 = 0;
        let ok = unsafe {
            InternetReadFile(
                h_url.raw(),
                chunk.as_mut_ptr() as *mut _,
                chunk.len() as u32,
                &mut read_bytes,
            )
        };

        if ok.is_err() || read_bytes == 0 {
            break;
        }

        out_file.write_all(&chunk[..read_bytes as usize])?;
        total_downloaded += read_bytes as u64;

        if total_expected > 0 {
            let pct = ((total_downloaded * 100) / total_expected).min(99) as u8;
            progress_sig.store(pct, Ordering::Release);
        }
    }

    out_file.flush()?;
    out_file.sync_all()?;
    progress_sig.store(100, Ordering::Release);

    Ok(target_file)
}

pub fn launch_installer_and_exit(installer_path: &PathBuf) -> CoreResult<()> {
    std::process::Command::new(installer_path)
        .spawn()
        .map_err(|e| CoreError::SystemTool {
            message: format!("No se pudo ejecutar el instalador actualizado: {}", e),
            source: None,
        })?;

    std::process::exit(0);
}