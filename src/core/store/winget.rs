//! winget: dónde está el binario en que se puede confiar, qué argumentos usa la tienda y qué códigos de salida dejan la
//! app instalada. La tienda instala todo con winget (decisión del autor, 25/11): última versión, SHA-256 del manifiesto
//! revisado en el repositorio de Microsoft (winget no deja saltarlo como administrador) y los argumentos silenciosos
//! que mantiene ese repositorio.

use std::fs::File;
use std::path::PathBuf;

use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
use windows::Win32::Storage::Packaging::Appx::{
    GetPackagePathByFullName, GetPackagesByPackageFamily,
};
use windows::core::{PCWSTR, PWSTR};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::folders::program_files_dir;
use crate::core::platform::handles::SafeHandle;
use crate::core::platform::process::user_profile_dir;
use crate::core::platform::safe_fs::{canonical_dos_path, path_is_strictly_under};
use crate::core::platform::wide::to_wide_null;
use crate::core::security::download::{DownloadPolicy, reopen_verified_for_exec};

/// Familia del paquete App Installer, que trae winget.
const APP_INSTALLER_FAMILY: &str = "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe";
const WINGET_SIGNER: &str = "Microsoft Corporation";

/// `winget install` terminó sin cambios porque la app ya está al día (`APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE`).
pub(super) const WINGET_UPDATE_NOT_APPLICABLE: u32 = 0x8A15_002B;
/// La app ya estaba instalada (`APPINSTALLER_CLI_ERROR_PACKAGE_ALREADY_INSTALLED`).
pub(super) const WINGET_PACKAGE_ALREADY_INSTALLED: u32 = 0x8A15_0061;

/// Para quién se instala una app del repositorio de winget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingetScope {
    /// Para todo el equipo: winget corre como administrador.
    Machine,
    /// Para el usuario de la sesión: winget corre con su token, nunca elevado, para que la app no quede en el perfil
    /// del administrador (Discord, Spotify, Playnite solo tienen instalador por usuario).
    User,
}

/// ERR-15: códigos de `winget install` que dejan la app instalada. «No se encontró la app» (`0x8A150014`) es un fallo: un ID
/// equivocado o la fuente caída no pueden informarse como «Listo».
pub(super) fn winget_exit_is_installed(code: u32) -> bool {
    matches!(
        code,
        0 | WINGET_UPDATE_NOT_APPLICABLE | WINGET_PACKAGE_ALREADY_INSTALLED
    )
}

/// Argumentos de `winget install` para una app del repositorio comunitario. Nunca `--ignore-security-hash`: el SHA-256
/// del manifiesto es lo que garantiza que el instalador es el revisado.
pub(super) fn repository_install_args(id: &str, scope: WingetScope) -> String {
    let scope = match scope {
        WingetScope::Machine => "machine",
        WingetScope::User => "user",
    };
    format!(
        "install --id {id} --exact --source winget --scope {scope} --silent \
         --accept-package-agreements --accept-source-agreements --disable-interactivity"
    )
}

/// Argumentos de `winget install` para una app de Microsoft Store (siempre como el usuario).
pub(super) fn store_install_args(id: &str) -> String {
    format!(
        "install --id {id} --exact --source msstore --silent \
         --accept-package-agreements --accept-source-agreements --disable-interactivity"
    )
}

/// winget del paquete App Installer, en `Program Files\WindowsApps` (solo lo escribe TrustedInstaller), con la firma de
/// Microsoft comprobada sobre el handle. El `File` devuelto impide reemplazarlo mientras viva: hay que mantenerlo hasta
/// que winget termine. Es el único winget que se ejecuta elevado (SEC-31: nunca el alias del perfil del usuario).
pub(super) fn machine_winget() -> CoreResult<(PathBuf, File)> {
    // La raíz de `WindowsApps` no se puede abrir ni como administrador: se normaliza `Program Files` y se le suma.
    let program_files =
        program_files_dir().ok_or_else(|| missing("no se encontró Program Files"))?;
    let windows_apps = canonical_dos_path(&program_files)?.join("WindowsApps");
    let mut candidates: Vec<(Vec<u64>, PathBuf)> = package_paths(APP_INSTALLER_FAMILY)?
        .into_iter()
        .filter_map(|(full_name, dir)| {
            let exe = canonical_dos_path(&dir.join("winget.exe")).ok()?;
            path_is_strictly_under(&exe, &windows_apps).then(|| (package_version(&full_name), exe))
        })
        .collect();
    candidates.sort();
    let (_, exe) = candidates
        .pop()
        .ok_or_else(|| missing("App Installer no tiene winget.exe"))?;
    let policy = DownloadPolicy {
        url: "",
        allowed_final_hosts: &[],
        expected_sha256_hex: None,
        expected_cert_thumbprint_sha1: None,
        expected_signer_cn: Some(WINGET_SIGNER),
        max_bytes: 0,
    };
    let lock = reopen_verified_for_exec(&exe, &policy)?;
    Ok((exe, lock))
}

/// SEC-31: el alias `winget.exe` del perfil del usuario dueño del escritorio. Esa carpeta es del usuario, así que el
/// binario solo se ejecuta con su propio token (nunca elevado): quien pueda reemplazarlo no gana nada que no tuviera.
pub(super) fn user_winget(user_token: &SafeHandle) -> CoreResult<PathBuf> {
    let candidate =
        user_profile_dir(user_token)?.join(r"AppData\Local\Microsoft\WindowsApps\winget.exe");
    // El alias es un reparse point de tipo AppExecLink: no se sigue, basta con que exista.
    if std::fs::symlink_metadata(&candidate).is_ok() {
        Ok(candidate)
    } else {
        Err(missing("winget no está instalado para este usuario"))
    }
}

fn missing(detail: &str) -> CoreError {
    CoreError::SystemTool {
        message: format!(
            "{detail}. Instala «App Installer» desde Microsoft Store (ms-windows-store://pdp/?productid=9NBLGGH4NNS1)."
        ),
        source: None,
    }
}

/// Versión de un nombre completo de paquete (`Microsoft.DesktopAppInstaller_1.29.379.0_x64__8wekyb3d8bbwe`).
fn package_version(full_name: &str) -> Vec<u64> {
    full_name
        .split('_')
        .nth(1)
        .unwrap_or_default()
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Nombres completos y carpetas instaladas de los paquetes de una familia.
fn package_paths(family: &str) -> CoreResult<Vec<(String, PathBuf)>> {
    let family_w = to_wide_null(family);
    let mut count = 0u32;
    let mut buffer_len = 0u32;
    // SAFETY: consulta de tamaños sin búferes; la familia termina en NUL.
    let first = unsafe {
        GetPackagesByPackageFamily(
            PCWSTR(family_w.as_ptr()),
            &mut count,
            None,
            &mut buffer_len,
            None,
        )
    };
    if first == ERROR_SUCCESS && count == 0 {
        return Ok(Vec::new());
    }
    if first != ERROR_INSUFFICIENT_BUFFER {
        return Err(missing(&format!(
            "No se pudo consultar App Installer (código {})",
            first.0
        )));
    }
    let mut names = vec![PWSTR::null(); count as usize];
    let mut buffer = vec![0u16; buffer_len as usize];
    // SAFETY: los búferes miden lo que pidió la primera llamada; la API escribe punteros dentro de `buffer`.
    let second = unsafe {
        GetPackagesByPackageFamily(
            PCWSTR(family_w.as_ptr()),
            &mut count,
            Some(names.as_mut_ptr()),
            &mut buffer_len,
            Some(PWSTR(buffer.as_mut_ptr())),
        )
    };
    if second != ERROR_SUCCESS {
        return Err(missing(&format!(
            "No se pudo consultar App Installer (código {})",
            second.0
        )));
    }
    let mut out = Vec::new();
    for name in names.iter().take(count as usize) {
        // SAFETY: cada puntero apunta a una cadena terminada en NUL dentro de `buffer`, que sigue vivo.
        let Ok(full_name) = (unsafe { name.to_string() }) else {
            continue;
        };
        if let Some(path) = package_path(&full_name) {
            out.push((full_name, path));
        }
    }
    Ok(out)
}

fn package_path(full_name: &str) -> Option<PathBuf> {
    let name_w = to_wide_null(full_name);
    let mut len = 0u32;
    // SAFETY: consulta de tamaño sin búfer; el nombre termina en NUL.
    let first = unsafe { GetPackagePathByFullName(PCWSTR(name_w.as_ptr()), &mut len, None) };
    if first != ERROR_INSUFFICIENT_BUFFER || len == 0 {
        return None;
    }
    let mut path = vec![0u16; len as usize];
    // SAFETY: el búfer mide lo que pidió la primera llamada.
    let second = unsafe {
        GetPackagePathByFullName(
            PCWSTR(name_w.as_ptr()),
            &mut len,
            Some(PWSTR(path.as_mut_ptr())),
        )
    };
    if second != ERROR_SUCCESS {
        return None;
    }
    let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    Some(PathBuf::from(String::from_utf16_lossy(&path[..end])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigos_de_winget() {
        assert!(winget_exit_is_installed(0));
        assert!(winget_exit_is_installed(WINGET_UPDATE_NOT_APPLICABLE));
        assert!(winget_exit_is_installed(WINGET_PACKAGE_ALREADY_INSTALLED));
        // «No se encontró la app» no es éxito (ERR-15).
        assert!(!winget_exit_is_installed(0x8A15_0014));
        assert!(!winget_exit_is_installed(1));
    }

    /// La tienda nunca pide saltar el SHA-256 y siempre fija fuente, ID exacto y alcance.
    #[test]
    fn argumentos_de_instalacion() {
        let machine = repository_install_args("7zip.7zip", WingetScope::Machine);
        for must in [
            "--id 7zip.7zip",
            "--exact",
            "--source winget",
            "--scope machine",
            "--silent",
            "--disable-interactivity",
        ] {
            assert!(machine.contains(must), "falta {must}: {machine}");
        }
        assert!(
            repository_install_args("Discord.Discord", WingetScope::User).contains("--scope user")
        );
        let store = store_install_args("9WZDNCRFHVN5");
        assert!(store.contains("--source msstore") && store.contains("--exact"));
        for args in [machine, store] {
            assert!(!args.contains("ignore-security-hash"));
        }
    }

    #[test]
    fn version_del_paquete() {
        assert_eq!(
            package_version("Microsoft.DesktopAppInstaller_1.29.379.0_x64__8wekyb3d8bbwe"),
            vec![1, 29, 379, 0]
        );
    }

    /// Encuentra el winget de App Installer y su firma de Microsoft (necesita App Installer instalado).
    #[test]
    #[ignore = "depende de que App Installer esté instalado"]
    fn winget_de_la_maquina() {
        let (exe, _lock) = machine_winget().expect("winget");
        assert!(exe.to_string_lossy().contains("WindowsApps"));
    }
}
