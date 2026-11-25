//
// Auto-actualización desde GitHub Releases. El instalador se ejecuta como administrador, así que pasa por la
// misma cadena de confianza que la tienda (SEC-02):
// - Descarga con `trust::secure_download_and_verify_with_progress`: HTTPS, host final de GitHub en lista
//   blanca, tope de tamaño y SHA-256 obligatorio (el `digest` que GitHub publica para cada asset).
// - Si `UPDATE_SIGNER_CN` está definido, además exige firma Authenticode de ese firmante.
// - Se ejecuta reabriendo el archivo con `reopen_verified_for_exec` (nadie puede reemplazarlo entre la
//   verificación y la ejecución) desde la carpeta de datos solo-admin.

use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinInet::{
    HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile,
    InternetSetOptionW, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE,
    INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD, INTERNET_FLAG_SECURE,
    INTERNET_OPEN_TYPE_PRECONFIG, INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT,
    INTERNET_OPTION_SEND_TIMEOUT,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::delete_verified_file;
use crate::core::platform::wide::to_wide_null;
use crate::core::security::trust::{
    ensure_admin_only_data_dir, reopen_verified_for_exec, secure_download_and_verify_with_progress,
    DownloadPolicy,
};

const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Kurobanyi/OtterSwiss/releases/latest";

/// Hosts a los que puede redirigir la descarga de un asset de GitHub Releases.
const GITHUB_DOWNLOAD_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// CN del certificado Authenticode con el que se firman las releases. Mientras sea `None` solo se exige el
/// SHA-256; al firmar las releases, poner aquí el CN para que el updater rechace binarios sin esa firma.
const UPDATE_SIGNER_CN: Option<&str> = None;

const MAX_RELEASE_JSON_BYTES: usize = 1024 * 1024;
const MAX_INSTALLER_BYTES: u64 = 200 * 1024 * 1024;

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
    /// `sha256:<hex>` que GitHub calcula para cada asset.
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
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
    pub sha256: String,
}

/// Versión `MAJOR.MINOR.PATCH`, con la parte de pre-release (`-rc1`) aparte. Una pre-release es menor que
/// la versión final del mismo número (semver).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    core: (u32, u32, u32),
    pre: Option<String>,
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match self.core.cmp(&other.core) {
            Equal => match (&self.pre, &other.pre) {
                (None, None) => Equal,
                (None, Some(_)) => Greater,
                (Some(_), None) => Less,
                (Some(a), Some(b)) => a.cmp(b),
            },
            o => o,
        }
    }
}

fn parse_version(v: &str) -> Version {
    let clean = v.trim().trim_start_matches(['v', 'V']);
    let clean = clean.split('+').next().unwrap_or(clean);
    let (core_str, pre) = match clean.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (clean, None),
    };
    let mut parts = core_str.split('.');
    let mut next = || {
        parts
            .next()
            .and_then(|x| x.trim().parse::<u32>().ok())
            .unwrap_or(0)
    };
    Version {
        core: (next(), next(), next()),
        pre,
    }
}

pub fn is_newer_version(remote_tag: &str, current_ver: &str) -> bool {
    parse_version(remote_tag) > parse_version(current_ver)
}

/// Solo letras, dígitos, `.`, `-` y `_`, terminado en `.exe`: el nombre remoto se usa como nombre de archivo.
fn is_safe_asset_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && name.to_ascii_lowercase().ends_with(".exe")
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn parse_sha256_digest(digest: Option<&str>) -> Option<String> {
    let hex = digest?.trim().strip_prefix("sha256:")?;
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex.to_ascii_lowercase())
    } else {
        None
    }
}

fn open_internet(timeout_ms: u32) -> CoreResult<WinInetHandle> {
    let agent_w = to_wide_null(format!("OtterSwiss-Updater/{}", env!("CARGO_PKG_VERSION")));
    let h = WinInetHandle::new(unsafe {
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
    for opt in [
        INTERNET_OPTION_CONNECT_TIMEOUT,
        INTERNET_OPTION_SEND_TIMEOUT,
        INTERNET_OPTION_RECEIVE_TIMEOUT,
    ] {
        unsafe {
            let _ = InternetSetOptionW(
                Some(h.raw()),
                opt,
                Some(&timeout_ms as *const _ as *const _),
                4,
            );
        }
    }
    Ok(h)
}

pub fn check_for_updates() -> CoreResult<Option<UpdateReleaseInfo>> {
    let h_internet = open_internet(10_000)?;
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
        if json_bytes.len() > MAX_RELEASE_JSON_BYTES {
            return Err(CoreError::Trust {
                message: "La respuesta de GitHub excede el tamaño máximo esperado".into(),
                source: None,
            });
        }
    }

    let release: GitHubRelease =
        serde_json::from_slice(&json_bytes).map_err(|e| CoreError::Trust {
            message: format!("Error interpretando respuesta JSON de GitHub: {}", e),
            source: None,
        })?;

    if release.draft || release.prerelease {
        return Ok(None);
    }
    if !is_newer_version(&release.tag_name, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let candidates = || {
        release
            .assets
            .iter()
            .filter(|a| is_safe_asset_name(&a.name))
    };
    let asset = candidates()
        .find(|a| {
            let n = a.name.to_ascii_lowercase();
            n.contains("setup") || n.contains("installer")
        })
        .or_else(|| candidates().next())
        .ok_or_else(|| CoreError::Trust {
            message: "La versión nueva en GitHub no incluye un instalador (.exe) válido".into(),
            source: None,
        })?;

    let sha256 = parse_sha256_digest(asset.digest.as_deref()).ok_or_else(|| CoreError::Trust {
        message: format!(
            "GitHub no publicó el SHA-256 del instalador '{}'; no se puede verificar la actualización",
            asset.name
        ),
        source: None,
    })?;

    Ok(Some(UpdateReleaseInfo {
        version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
        tag_name: release.tag_name.clone(),
        title: release
            .name
            .clone()
            .unwrap_or_else(|| "OtterSwiss Update".into()),
        notes: release.body.clone().unwrap_or_default(),
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        asset_size: asset.size,
        sha256,
    }))
}

fn update_policy(info: &UpdateReleaseInfo) -> DownloadPolicy<'_> {
    DownloadPolicy {
        url: &info.download_url,
        allowed_final_hosts: GITHUB_DOWNLOAD_HOSTS,
        expected_sha256_hex: Some(&info.sha256),
        expected_cert_thumbprint_sha1: None,
        expected_signer_cn: UPDATE_SIGNER_CN,
        max_bytes: MAX_INSTALLER_BYTES,
    }
}

/// Descarga y verifica el instalador en `ProgramData\OtterSwiss\Updates`. Devuelve su ruta.
pub fn download_and_install_update(
    info: &UpdateReleaseInfo,
    cancel_flag: &Arc<AtomicBool>,
    progress_sig: &Arc<AtomicU8>,
) -> CoreResult<PathBuf> {
    if cancel_flag.load(Ordering::Acquire) {
        return Err(CoreError::Cancelled);
    }
    if !is_safe_asset_name(&info.asset_name) {
        return Err(CoreError::Validation(format!(
            "Nombre de instalador no permitido: '{}'",
            info.asset_name
        )));
    }

    let target_dir = ensure_admin_only_data_dir()?.join("Updates");
    std::fs::create_dir_all(&target_dir)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let target_file = target_dir.join(format!("{}_{}", nonce, info.asset_name));

    let policy = update_policy(info);
    match secure_download_and_verify_with_progress(
        &target_file,
        &policy,
        Some(progress_sig.as_ref()),
        Some(cancel_flag.as_ref()),
    ) {
        Ok(f) => drop(f),
        Err(e) => {
            let _ = delete_verified_file(&target_file);
            return Err(e);
        }
    }

    progress_sig.store(100, Ordering::Release);
    Ok(target_file)
}

/// Lanza el instalador verificado y cierra OtterSwiss. El archivo se reabre verificado y se mantiene
/// abierto (sin permitir escritura ni borrado) hasta que el proceso del instalador ya arrancó.
pub fn launch_installer_and_exit(
    installer_path: &Path,
    info: &UpdateReleaseInfo,
) -> CoreResult<()> {
    let policy = update_policy(info);
    let _exec_lock = reopen_verified_for_exec(installer_path, &policy)?;
    let working_dir = installer_path.parent().unwrap_or(installer_path);

    std::process::Command::new(installer_path)
        .current_dir(working_dir)
        .spawn()
        .map_err(|e| CoreError::SystemTool {
            message: format!("No se pudo ejecutar el instalador actualizado: {}", e),
            source: None,
        })?;

    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compara_versiones_semver() {
        assert!(is_newer_version("v3.5.2", "3.5.1"));
        assert!(is_newer_version("3.6.0", "3.5.9"));
        assert!(!is_newer_version("3.5.1", "3.5.1"));
        assert!(!is_newer_version("3.5.0", "3.5.1"));
        // ERR-02: una pre-release de un número mayor sí es más nueva…
        assert!(is_newer_version("3.5.2-rc1", "3.5.1"));
        // …pero es menor que la final del mismo número.
        assert!(!is_newer_version("3.5.2-rc1", "3.5.2"));
        assert!(is_newer_version("3.5.2", "3.5.2-rc1"));
    }

    #[test]
    fn nombres_de_asset_seguros() {
        assert!(is_safe_asset_name("OtterSwiss-Setup-v3.5.2.exe"));
        assert!(!is_safe_asset_name("..\\..\\evil.exe"));
        assert!(!is_safe_asset_name("setup.msi"));
        assert!(!is_safe_asset_name("a b.exe"));
        assert!(!is_safe_asset_name(".exe"));
    }

    #[test]
    fn digest_sha256() {
        let hex = "a".repeat(64);
        assert_eq!(
            parse_sha256_digest(Some(&format!("sha256:{}", hex))),
            Some(hex)
        );
        assert_eq!(parse_sha256_digest(Some("sha1:abcd")), None);
        assert_eq!(parse_sha256_digest(Some("sha256:xyz")), None);
        assert_eq!(parse_sha256_digest(None), None);
    }
}
