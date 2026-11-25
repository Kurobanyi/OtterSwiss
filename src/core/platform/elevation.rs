use std::path::PathBuf;
use windows::Win32::Foundation::{ERROR_CANCELLED, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use crate::core::cli::RELAUNCH_FLAGS;
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::SafeHandle;
use crate::core::platform::wide::to_wide_null;

pub struct ElevationManager;

impl ElevationManager {
    pub fn is_elevated() -> CoreResult<bool> {
        let mut raw_token = HANDLE::default();
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) }.map_err(
            |e| CoreError::SystemTool {
                message: format!("OpenProcessToken falló: {}", e),
                source: None,
            },
        )?;

        let token = SafeHandle::new(raw_token)
            .ok_or_else(|| CoreError::invalid_handle("Token de proceso inválido"))?;

        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned_len = 0u32;
        unsafe {
            GetTokenInformation(
                token.raw(),
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut _),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned_len,
            )
        }
        .map_err(CoreError::sys(
            "GetTokenInformation(TokenElevation) falló: ",
        ))?;

        Ok(elevation.TokenIsElevated != 0)
    }

    pub fn require_elevated(operation: &str) -> CoreResult<()> {
        if Self::is_elevated()? {
            Ok(())
        } else {
            Err(CoreError::NotElevated(operation.to_string()))
        }
    }

    pub fn validate_relaunch_args(extra_args: &[&str]) -> CoreResult<String> {
        let mut sanitized = Vec::with_capacity(extra_args.len());
        for &arg in extra_args {
            let flag = arg.split_whitespace().next().unwrap_or(arg);
            if !RELAUNCH_FLAGS.contains(&flag) {
                return Err(CoreError::Validation(format!(
                    "Argumento no autorizado para relanzamiento elevado: '{}'",
                    arg
                )));
            }
            sanitized.push(arg);
        }
        Ok(sanitized.join(" "))
    }

    pub fn relaunch_as_admin(extra_args: &[&str]) -> CoreResult<()> {
        if Self::is_elevated()? {
            return Ok(());
        }

        let params_str = Self::validate_relaunch_args(extra_args)?;
        let exe_path: PathBuf = std::env::current_exe()?;
        let exe_dir = exe_path.parent().ok_or_else(|| {
            CoreError::Validation("No se pudo determinar el directorio del ejecutable".into())
        })?;

        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

        let verb_w = to_wide_null("runas");
        let file_w = to_wide_null(&exe_path);
        let dir_w = to_wide_null(exe_dir);
        let params_w = if params_str.is_empty() {
            None
        } else {
            Some(to_wide_null(&params_str))
        };

        let mut sei = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC,
            lpVerb: PCWSTR(verb_w.as_ptr()),
            lpFile: PCWSTR(file_w.as_ptr()),
            lpParameters: params_w
                .as_ref()
                .map(|p| PCWSTR(p.as_ptr()))
                .unwrap_or(PCWSTR::null()),
            lpDirectory: PCWSTR(dir_w.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };

        if let Err(err) = unsafe { ShellExecuteExW(&mut sei) } {
            if err.code() == ERROR_CANCELLED.to_hresult() {
                return Err(CoreError::Cancelled);
            }
            return Err(CoreError::SystemTool {
                message: format!("ShellExecuteExW(runas) falló: {}", err),
                source: None,
            });
        }

        Ok(())
    }
}
