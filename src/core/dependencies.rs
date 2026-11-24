use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::RegKey;

use crate::core::elevation::ElevationManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::safe_fs::delete_verified_file;
use crate::core::trust::{ensure_admin_only_data_dir, secure_download_and_verify, DownloadPolicy};
use crate::core::win_common::{to_wide_null, JobObject, SafeHandle};

#[derive(Debug, Clone)]
pub struct RuntimeDependency {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub allowed_hosts: &'static [&'static str],
    pub expected_sha256: Option<&'static str>,
    pub signer_cn: Option<&'static str>,
    pub silent_args: &'static str,
    pub max_bytes: u64,
}

static RUNTIME_CATALOG: &[RuntimeDependency] = &[
    RuntimeDependency {
        id: "vcredist_x64",
        name: "Visual C++ 2015–2022 Redistributable (x64)",
        description: "Librerías C/C++ nativas de 64 bits requeridas por juegos modernos",
        url: "https://aka.ms/vs/17/release/vc_redist.x64.exe",
        allowed_hosts: &[
            "aka.ms",
            "download.visualstudio.microsoft.com",
            "download.microsoft.com",
        ],
        expected_sha256: None,
        signer_cn: Some("Microsoft Corporation"),
        silent_args: "/install /quiet /norestart",
        max_bytes: 35 * 1024 * 1024,
    },
    RuntimeDependency {
        id: "vcredist_x86",
        name: "Visual C++ 2015–2022 Redistributable (x86)",
        description: "Librerías C/C++ de 32 bits (necesarias para launchers y juegos DX9/DX11)",
        url: "https://aka.ms/vs/17/release/vc_redist.x86.exe",
        allowed_hosts: &[
            "aka.ms",
            "download.visualstudio.microsoft.com",
            "download.microsoft.com",
        ],
        expected_sha256: None,
        signer_cn: Some("Microsoft Corporation"),
        silent_args: "/install /quiet /norestart",
        max_bytes: 25 * 1024 * 1024,
    },
    RuntimeDependency {
        id: "directx_web",
        name: "DirectX End-User Runtime (DX9/DX10/XInput/XAudio)",
        description: "DLLs heredadas d3dx9_43, xinput1_3 y XAudio2_7 para juegos clásicos y motores DX11",
        url: "https://download.microsoft.com/download/1/7/1/1718ccc4-6315-4d8e-9543-8e28a4e18c4c/dxwebsetup.exe",
        allowed_hosts: &["download.microsoft.com"],
        expected_sha256: None,
        signer_cn: Some("Microsoft Corporation"),
        silent_args: "/Q",
        max_bytes: 5 * 1024 * 1024,
    },
    RuntimeDependency {
        id: "dotnet_desktop_8_x64",
        name: ".NET 8.0 Desktop Runtime (x64)",
        description: "Runtime de escritorio para launchers, overlays y gestores de mods",
        url: "https://aka.ms/dotnet/8.0/windowsdesktop-runtime-win-x64.exe",
        allowed_hosts: &[
            "aka.ms",
            "download.visualstudio.microsoft.com",
            "builds.dotnet.microsoft.com",
            "dotnetcli.azureedge.net",
            "dotnetbuilds.azureedge.net",
        ],
        expected_sha256: None,
        signer_cn: Some(".NET|Microsoft Corporation"),
        silent_args: "/install /quiet /norestart",
        max_bytes: 75 * 1024 * 1024,
    },
    RuntimeDependency {
        id: "dotnet_desktop_8_x86",
        name: ".NET 8.0 Desktop Runtime (x86)",
        description: "Variante de 32 bits para herramientas y utilidades de juegos",
        url: "https://aka.ms/dotnet/8.0/windowsdesktop-runtime-win-x86.exe",
        allowed_hosts: &[
            "aka.ms",
            "download.visualstudio.microsoft.com",
            "builds.dotnet.microsoft.com",
            "dotnetcli.azureedge.net",
            "dotnetbuilds.azureedge.net",
        ],
        expected_sha256: None,
        signer_cn: Some(".NET|Microsoft Corporation"),
        silent_args: "/install /quiet /norestart",
        max_bytes: 70 * 1024 * 1024,
    },
    RuntimeDependency {
        id: "webview2",
        name: "Microsoft Edge WebView2 Runtime",
        description: "Motor web embebido requerido por interfaces y launchers modernos",
        url: "https://go.microsoft.com/fwlink/p/?LinkId=2124703",
        allowed_hosts: &[
            "go.microsoft.com",
            "msedge.sf.dl.delivery.mp.microsoft.com",
            "msedge.b.tlu.dl.delivery.mp.microsoft.com",
            "download.microsoft.com",
        ],
        expected_sha256: None,
        signer_cn: Some("Microsoft Corporation"),
        silent_args: "/silent /install",
        max_bytes: 180 * 1024 * 1024,
    },
];

pub struct DependencyManager;

impl DependencyManager {
    pub fn get_catalog() -> &'static [RuntimeDependency] {
        RUNTIME_CATALOG
    }

    pub fn is_webview2_installed() -> bool {
        let reg_checks = [
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-F500-4197-9A05-C077E71C7328}",
                KEY_READ | KEY_WOW64_64KEY,
            ),
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-F500-4197-9A05-C077E71C7328}",
                KEY_READ | KEY_WOW64_32KEY,
            ),
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-F500-4197-9A05-C077E71C7328}",
                KEY_READ | KEY_WOW64_64KEY,
            ),
            (
                HKEY_CURRENT_USER,
                r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-F500-4197-9A05-C077E71C7328}",
                KEY_READ,
            ),
        ];

        for (hive, subkey, flags) in reg_checks {
            let root = RegKey::predef(hive);
            if let Ok(key) = root.open_subkey_with_flags(subkey, flags) {
                if let Ok(pv) = key.get_value::<String, _>("pv") {
                    if !pv.trim().is_empty() && pv != "0.0.0.0" {
                        return true;
                    }
                }
            }
        }

        // Comprobación física en disco (EdgeWebView\Application\<version>\msedgewebview2.exe)
        let mut app_dirs = Vec::new();
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            app_dirs.push(Path::new(&pf86).join(r"Microsoft\EdgeWebView\Application"));
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            app_dirs.push(Path::new(&pf).join(r"Microsoft\EdgeWebView\Application"));
        }
        if let Ok(local) = std::env::var("LocalAppData") {
            app_dirs.push(Path::new(&local).join(r"Microsoft\EdgeWebView\Application"));
        }

        for dir in app_dirs {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if entry.path().join("msedgewebview2.exe").is_file() {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn is_vcredist_installed() -> bool {
        let root = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64";
        if let Ok(key) = root.open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY) {
            if let Ok(installed) = key.get_value::<u32, _>("Installed") {
                return installed == 1;
            }
        }
        false
    }

    pub fn is_vcredist_x86_installed() -> bool {
        let root = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = r"SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x86";
        if let Ok(key) = root.open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY) {
            if let Ok(installed) = key.get_value::<u32, _>("Installed") {
                return installed == 1;
            }
        }
        false
    }

    pub fn is_dotnet8_desktop_installed(arch: &str) -> bool {
        let env_var = if arch == "x86" {
            "ProgramFiles(x86)"
        } else {
            "ProgramFiles"
        };
        let base_pf = std::env::var(env_var).unwrap_or_else(|_| {
            if arch == "x86" {
                r"C:\Program Files (x86)".to_string()
            } else {
                r"C:\Program Files".to_string()
            }
        });
        let base = Path::new(&base_pf).join(r"dotnet\shared\Microsoft.WindowsDesktop.App");
        std::fs::read_dir(base)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with("8.0."))
            })
            .unwrap_or(false)
    }

    pub fn is_directx_legacy_installed() -> bool {
        if let Ok(sys_root) = std::env::var("SystemRoot") {
            let root = Path::new(&sys_root);
            let dll64 = root.join(r"System32\d3dx9_43.dll");
            let dll32 = root.join(r"SysWOW64\d3dx9_43.dll");
            return dll64.is_file() && dll32.is_file();
        }
        false
    }

    pub fn is_installed(runtime_id: &str) -> bool {
        match runtime_id {
            "vcredist_x64" => Self::is_vcredist_installed(),
            "vcredist_x86" => Self::is_vcredist_x86_installed(),
            "directx_web" => Self::is_directx_legacy_installed(),
            "dotnet_desktop_8_x64" => Self::is_dotnet8_desktop_installed("x64"),
            "dotnet_desktop_8_x86" => Self::is_dotnet8_desktop_installed("x86"),
            "webview2" => Self::is_webview2_installed(),
            _ => false,
        }
    }

    pub fn install_runtime(runtime_id: &str) -> CoreResult<()> {
        ElevationManager::require_elevated("Instalar runtime dependiente de Windows")?;

        let runtime = RUNTIME_CATALOG
            .iter()
            .find(|r| r.id.eq_ignore_ascii_case(runtime_id))
            .ok_or_else(|| {
                CoreError::Validation(format!(
                    "Runtime '{}' no encontrado en el catálogo",
                    runtime_id
                ))
            })?;

        let secure_dir = ensure_admin_only_data_dir()?.join("staging");
        std::fs::create_dir_all(&secure_dir)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let installer_path = secure_dir.join(format!(
            "{}_{}_{}.exe",
            runtime.id,
            std::process::id(),
            nonce
        ));

        let policy = DownloadPolicy {
            url: runtime.url,
            allowed_final_hosts: runtime.allowed_hosts,
            expected_sha256_hex: runtime.expected_sha256,
            expected_cert_thumbprint_sha1: None,
            expected_signer_cn: runtime.signer_cn,
            max_bytes: runtime.max_bytes,
        };

        let file = match secure_download_and_verify(&installer_path, &policy) {
            Ok(f) => f,
            Err(e) => {
                let _ = delete_verified_file(&installer_path);
                return Err(e);
            }
        };
        drop(file);

        let job = JobObject::new_kill_on_close()?;
        let cmd_line = format!("\"{}\" {}", installer_path.display(), runtime.silent_args);
        let mut cmd_wide = to_wide_null(&cmd_line);

        let mut pi = windows::Win32::System::Threading::PROCESS_INFORMATION::default();
        let si = windows::Win32::System::Threading::STARTUPINFOW {
            cb: std::mem::size_of::<windows::Win32::System::Threading::STARTUPINFOW>() as u32,
            ..Default::default()
        };

        let wide_exe = to_wide_null(&installer_path);
        unsafe {
            windows::Win32::System::Threading::CreateProcessW(
                windows::core::PCWSTR(wide_exe.as_ptr()),
                windows::core::PWSTR(cmd_wide.as_mut_ptr()),
                None,
                None,
                false,
                Default::default(),
                None,
                windows::core::PCWSTR::null(),
                &si,
                &mut pi,
            )
        }
        .map_err(|e| {
            let _ = delete_verified_file(&installer_path);
            CoreError::SystemTool {
                message: format!("Error ejecutando instalador runtime: {}", e),
                source: None,
            }
        })?;

        let _proc_handle = SafeHandle::new(pi.hProcess);
        let _thread_handle = SafeHandle::new(pi.hThread);

        if let Some(h) = _proc_handle.as_ref() {
            job.assign(h.raw())?;
        }

        let wait_res =
            unsafe { windows::Win32::System::Threading::WaitForSingleObject(pi.hProcess, 300_000) };

        if wait_res == windows::Win32::Foundation::WAIT_TIMEOUT {
            job.terminate_all(1);
            let _ = delete_verified_file(&installer_path);
            return Err(CoreError::SystemTool {
                message: format!("Tiempo límite excedido instalando {}", runtime.name),
                source: None,
            });
        }

        let mut exit_code = 0u32;
        unsafe {
            windows::Win32::System::Threading::GetExitCodeProcess(pi.hProcess, &mut exit_code)
        }
        .map_err(|e| {
            let _ = delete_verified_file(&installer_path);
            CoreError::SystemTool {
                message: format!("GetExitCodeProcess falló: {}", e),
                source: None,
            }
        })?;

        let _ = delete_verified_file(&installer_path);

        if exit_code != 0 && exit_code != 3010 && exit_code != 1641 && exit_code != 1638 {
            return Err(CoreError::SystemTool {
                message: format!(
                    "Instalador runtime finalizó con código de error {}",
                    exit_code
                ),
                source: None,
            });
        }

        if runtime.id == "webview2" {
            thread::sleep(Duration::from_millis(1200));
        }

        Ok(())
    }
}