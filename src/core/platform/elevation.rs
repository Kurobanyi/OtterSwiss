use std::path::PathBuf;
use windows::Win32::Foundation::{ERROR_CANCELLED, HANDLE};
use windows::Win32::Security::{
    EqualSid, GetTokenInformation, PSID, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, TokenElevation,
    TokenUser,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::{
    GetShellWindow, GetWindowThreadProcessId, SW_SHOWNORMAL,
};
use windows::core::PCWSTR;

use crate::core::cli::RELAUNCH_FLAGS;
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::SafeHandle;
use crate::core::platform::wide::to_wide_null;

pub fn is_elevated() -> CoreResult<bool> {
    let mut raw_token = HANDLE::default();
    // SAFETY: el handle de proceso es válido y el token devuelto pasa a SafeHandle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) }.map_err(|e| {
        CoreError::SystemTool {
            message: format!("OpenProcessToken falló: {}", e),
            source: Some(Box::new(e)),
        }
    })?;

    let token = SafeHandle::new(raw_token)
        .ok_or_else(|| CoreError::invalid_handle("Token de proceso inválido"))?;

    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned_len = 0u32;
    // SAFETY: el token es válido y la estructura de salida tiene el tamaño que se informa.
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

/// Cuenta dueña del token `token` (el buffer de `TOKEN_USER`, con el SID dentro).
fn token_user_buffer(token: &SafeHandle) -> Option<Vec<u8>> {
    let mut len = 0u32;
    // SAFETY: primera llamada solo para saber el tamaño; falla con ERROR_INSUFFICIENT_BUFFER y eso es lo esperado.
    let _ = unsafe { GetTokenInformation(token.raw(), TokenUser, None, 0, &mut len) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: el buffer tiene el tamaño que Windows pidió.
    unsafe {
        GetTokenInformation(
            token.raw(),
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            len,
            &mut len,
        )
    }
    .ok()?;
    Some(buf)
}

fn sid_of(buf: &[u8]) -> PSID {
    // SAFETY: `buf` lo llenó GetTokenInformation(TokenUser): empieza con un TOKEN_USER (se lee sin exigir alineación)
    // y el SID vive dentro del mismo buffer.
    unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<TOKEN_USER>()) }
        .User
        .Sid
}

/// SYS-24: ¿este proceso corre con una cuenta distinta de la del usuario del escritorio? Pasa con la elevación
/// «sobre el hombro»: un usuario estándar escribe las credenciales de un administrador, y HKCU del proceso es el del
/// administrador, no el del usuario que ve la pantalla. Sin poder saberlo (sin shell, sin permiso) devuelve `false`.
pub fn shell_user_differs() -> bool {
    shell_user_differs_inner().unwrap_or(false)
}

fn shell_user_differs_inner() -> Option<bool> {
    // SAFETY: GetShellWindow no tiene precondiciones; devuelve nulo si no hay shell.
    let shell = unsafe { GetShellWindow() };
    if shell.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `shell` es un HWND devuelto por el sistema y `pid` es una salida válida.
    unsafe { GetWindowThreadProcessId(shell, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    // SAFETY: el handle devuelto pasa a SafeHandle.
    let process = SafeHandle::new(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?,
    )?;
    let mut raw = HANDLE::default();
    // SAFETY: `process` es válido; el token devuelto pasa a SafeHandle.
    unsafe { OpenProcessToken(process.raw(), TOKEN_QUERY, &mut raw) }.ok()?;
    let shell_token = SafeHandle::new(raw)?;
    let mut raw = HANDLE::default();
    // SAFETY: el pseudo-handle del proceso actual es válido; el token devuelto pasa a SafeHandle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) }.ok()?;
    let own_token = SafeHandle::new(raw)?;

    let shell_buf = token_user_buffer(&shell_token)?;
    let own_buf = token_user_buffer(&own_token)?;
    // SAFETY: ambos SID viven en buffers que siguen vivos durante la llamada.
    Some(unsafe { EqualSid(sid_of(&shell_buf), sid_of(&own_buf)) }.is_err())
}

pub fn require_elevated(operation: &str) -> CoreResult<()> {
    if is_elevated()? {
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
    if is_elevated()? {
        return Ok(());
    }

    let params_str = validate_relaunch_args(extra_args)?;
    let exe_path: PathBuf = std::env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or_else(|| {
        CoreError::Validation("No se pudo determinar el directorio del ejecutable".into())
    })?;

    // SAFETY: inicializa COM en este hilo; el resultado se respeta al cerrar.
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

    // SAFETY: la estructura y sus cadenas viven durante la llamada; el proceso devuelto pasa a SafeHandle.
    if let Err(err) = unsafe { ShellExecuteExW(&mut sei) } {
        if err.code() == ERROR_CANCELLED.to_hresult() {
            return Err(CoreError::Cancelled);
        }
        return Err(CoreError::SystemTool {
            message: format!("ShellExecuteExW(runas) falló: {}", err),
            source: Some(Box::new(err)),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SYS-24: en una sesión normal el proceso y el escritorio son la misma cuenta; si diera «distinta», todo ajuste de
    /// HKCU se bloquearía sin motivo.
    #[test]
    fn el_proceso_y_el_escritorio_son_la_misma_cuenta() {
        assert_eq!(shell_user_differs_inner(), Some(false));
    }
}
