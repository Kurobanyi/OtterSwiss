use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
use winreg::RegKey;

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::folders::{program_files_dir, program_files_x86_dir, windows_dir};
use crate::core::platform::process::{run_in_job, Launch, ProcessExit};
use crate::core::platform::safe_fs::delete_verified_file;
use crate::core::security::trust::{
    ensure_admin_only_data_dir, reopen_verified_for_exec, secure_download_and_verify,
    DownloadPolicy,
};

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

    /// `true` si WebView2 Runtime está instalado **y utilizable**. Se pregunta al cargador oficial
    /// (`GetAvailableCoreWebView2BrowserVersionString`), el mismo que usa la ventana de la app: el registro y las
    /// carpetas pueden quedar con restos de una instalación rota y dar un falso «instalado» (ERR-13).
    pub fn is_webview2_installed() -> bool {
        use webview2_com::CoTaskMemPWSTR;
        use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;

        // Sin carpeta propia del navegador (null): se usa el runtime instalado en el sistema.
        let no_browser_dir = CoTaskMemPWSTR::default();
        let mut version = Default::default();
        let ok = unsafe {
            GetAvailableCoreWebView2BrowserVersionString(
                *no_browser_dir.as_ref().as_pcwstr(),
                &mut version,
            )
        }
        .is_ok();
        // `CoTaskMemPWSTR` libera la cadena al soltarse.
        let version = CoTaskMemPWSTR::from(version);
        ok && !version.to_string().trim().is_empty()
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
        let base_pf = if arch == "x86" {
            program_files_x86_dir()
        } else {
            program_files_dir()
        };
        let Some(base_pf) = base_pf else {
            return false;
        };
        let base = base_pf.join(r"dotnet\shared\Microsoft.WindowsDesktop.App");
        std::fs::read_dir(base)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with("8.0."))
            })
            .unwrap_or(false)
    }

    pub fn is_directx_legacy_installed() -> bool {
        if let Some(sys_root) = windows_dir() {
            let root = sys_root.as_path();
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

        match secure_download_and_verify(&installer_path, &policy) {
            Ok(f) => drop(f),
            Err(e) => {
                let _ = delete_verified_file(&installer_path);
                return Err(e);
            }
        }

        let run = (|| -> CoreResult<ProcessExit> {
            // Mientras este handle viva nadie puede reemplazar el instalador (SEC-05).
            let _exec_lock = reopen_verified_for_exec(&installer_path, &policy)?;
            run_in_job(
                &Launch {
                    application: &installer_path,
                    command_line: format!(
                        "\"{}\" {}",
                        installer_path.display(),
                        runtime.silent_args
                    ),
                    working_dir: &secure_dir,
                    user_token: None,
                },
                Duration::from_secs(300),
                None,
            )
        })();
        let _ = delete_verified_file(&installer_path);
        let exit = run?;

        // 1638 = otra versión del producto ya instalada.
        if !exit.is_success() && exit.exit_code != 1638 {
            return Err(CoreError::SystemTool {
                message: format!(
                    "Instalador runtime finalizó con código de error {}",
                    exit.code_display()
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
