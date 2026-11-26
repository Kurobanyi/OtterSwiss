//! Ejecución del desinstalador propio de cada app (UninstallString) dentro de un Job.

use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::Win32::System::Environment::ExpandEnvironmentStringsForUserW;
use windows::core::PCWSTR;

use super::{InstalledApp, UninstallerManager};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::handles::SafeHandle;
use crate::core::platform::process::ProcessExit;
use crate::core::platform::process::{
    Launch, duplicate_desktop_shell_primary_token, run_in_job, system32_dir,
};
use crate::core::platform::wide::to_wide_null;

impl UninstallerManager {
    /// Expande `%VAR%` con el entorno del **sistema** (sin token) o del usuario dueño de `token`, nunca con
    /// el del propio proceso: un proceso sin elevar puede redefinir `%ProgramFiles%` en `HKCU\Environment`
    /// y hacer que OtterSwiss ejecute como administrador un binario suyo.
    pub fn expand_environment_string(raw: &str, token: Option<&SafeHandle>) -> CoreResult<String> {
        if !raw.contains('%') {
            return Ok(raw.to_string());
        }
        let wide = to_wide_null(raw);
        let mut buf = vec![0u16; 32_768];
        let h = token.map(|t| t.raw());
        // SAFETY: el token y las cadenas son válidos; el búfer tiene la longitud que se pasa.
        unsafe { ExpandEnvironmentStringsForUserW(h, PCWSTR(wide.as_ptr()), &mut buf) }.map_err(
            |e| CoreError::Uninstaller {
                message: format!("No se pudo expandir '{}': {}", raw, e),
                source: Some(Box::new(e)),
            },
        )?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(String::from_utf16_lossy(&buf[..end]))
    }

    /// Ejecuta el desinstalador de `app` y espera a que terminen él y todos sus hijos.
    /// Devuelve el código de salida; el llamador decide si es éxito (`is_uninstall_success`).
    pub fn run_uninstaller(app: &InstalledApp, timeout: Duration) -> CoreResult<ProcessExit> {
        let raw_cmd = app
            .quiet_uninstall_string
            .as_deref()
            .unwrap_or(&app.uninstall_string);

        // Las entradas de HKCU se ejecutan sin elevación, como el usuario del escritorio.
        let user_token = if app.is_per_user && ElevationManager::is_elevated().unwrap_or(false) {
            Some(duplicate_desktop_shell_primary_token()?)
        } else {
            None
        };

        let expanded = Self::expand_environment_string(raw_cmd, user_token.as_ref())?;
        let trimmed = expanded.trim();

        let (exe, raw_args) = parse_cmd_exe_and_args(trimmed);
        let exe_path = PathBuf::from(exe);
        let is_msi = exe_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("msiexec.exe") || n.eq_ignore_ascii_case("msiexec"))
            .unwrap_or(false);

        let sys32 = system32_dir()?;
        let application = if is_msi {
            sys32.join("msiexec.exe")
        } else {
            let is_unc = exe.starts_with(r"\\") || exe.starts_with("//");
            if !exe_path.is_absolute() || is_unc || !exe_path.is_file() {
                return Err(CoreError::Uninstaller {
                    message: format!(
                        "Ruta de desinstalador no permitida (debe ser un archivo local con ruta absoluta): {}",
                        exe
                    ),
                    source: None,
                });
            }
            exe_path
        };

        let mut args = raw_args.to_string();
        if is_msi {
            args = args.replace("/I{", "/X{").replace("/i{", "/X{");
            let low_args = args.to_ascii_lowercase();
            if !low_args.contains("/q") {
                args.push_str(" /qn /norestart");
            }
        }

        let command_line = if args.trim().is_empty() {
            format!("\"{}\"", application.display())
        } else {
            format!("\"{}\" {}", application.display(), args.trim())
        };
        let working_dir = if is_msi {
            sys32
        } else {
            application
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| sys32.clone())
        };

        run_in_job(
            &Launch {
                application: &application,
                command_line,
                working_dir: &working_dir,
                user_token: user_token.as_ref(),
            },
            timeout,
            None,
        )
    }
}

pub(super) fn parse_cmd_exe_and_args(trimmed: &str) -> (&str, &str) {
    if let Some(stripped) = trimmed.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            return (&stripped[..end], stripped[end + 1..].trim());
        }
        return (trimmed, "");
    }

    let lower = trimmed.to_ascii_lowercase();
    if let Some(pos) = lower.find(".exe") {
        let split_at = pos + 4;
        if split_at == trimmed.len() || trimmed.as_bytes()[split_at].is_ascii_whitespace() {
            return (&trimmed[..split_at], trimmed[split_at..].trim());
        }
    }

    let mut parts = trimmed.splitn(2, ' ');
    (
        parts.next().unwrap_or(trimmed),
        parts.next().unwrap_or("").trim(),
    )
}
