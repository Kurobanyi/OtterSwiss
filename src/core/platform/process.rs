//
// Lanzamiento de procesos externos (instaladores, desinstaladores, runtimes) desde el proceso elevado.
//
// Reglas que cumple este módulo (ver docs/BUGS.md, patrón 3):
// - El proceso se crea suspendido, se asigna al Job y solo entonces se reanuda: ningún hijo escapa del Job.
// - Se espera a que el Job quede vacío, no solo al proceso inicial: los desinstaladores que se copian a
//   %TEMP% y se relanzan (NSIS, Squirrel) terminan su trabajo en vez de morir al cerrarse el Job.
// - El directorio de trabajo es explícito, nunca el heredado de OtterSwiss.
// - Ante cualquier fallo (asignación, espera, timeout, cancelación) el Job se termina y se informa el error.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, TokenPrimary, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE,
    TOKEN_QUERY,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::JobObjects::{
    JobObjectBasicAccountingInformation, QueryInformationJobObject,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
};
use windows::Win32::System::Threading::{
    CreateProcessW, CreateProcessWithTokenW, GetExitCodeProcess, OpenProcess, OpenProcessToken,
    ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_SUSPENDED,
    CREATE_UNICODE_ENVIRONMENT, LOGON_WITH_PROFILE, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::GetUserProfileDirectoryW;
use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::{JobObject, SafeHandle};
use crate::core::platform::wide::to_wide_null;

/// Carpeta `System32` real, sin depender de variables de entorno.
pub fn system32_dir() -> CoreResult<PathBuf> {
    let mut buf = [0u16; 260];
    let len =
        unsafe { windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut buf)) }
            as usize;
    if len == 0 || len >= buf.len() {
        return Err(CoreError::SystemTool {
            message: "No se pudo localizar System32".into(),
            source: None,
        });
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

/// Token primario del Explorador de escritorio, para ejecutar algo sin elevación como el usuario que
/// tiene la sesión abierta.
pub fn duplicate_desktop_shell_primary_token() -> CoreResult<SafeHandle> {
    let shell_hwnd = unsafe { GetShellWindow() };
    if shell_hwnd.0.is_null() {
        return Err(CoreError::SystemTool {
            message: "No se encontró el Explorador de escritorio para ejecutar sin elevación"
                .into(),
            source: None,
        });
    }
    let mut shell_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(shell_hwnd, Some(&mut shell_pid));
    }
    if shell_pid == 0 {
        return Err(CoreError::SystemTool {
            message: "No se pudo obtener el PID de explorer.exe".into(),
            source: None,
        });
    }

    let proc_raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, shell_pid) }
        .map_err(|e| CoreError::SystemTool {
            message: format!("OpenProcess(explorer.exe) falló: {}", e),
            source: None,
        })?;
    let proc_h = SafeHandle::new(proc_raw)
        .ok_or_else(|| CoreError::from_win32("Handle de explorer.exe inválido"))?;

    let mut tok_raw = HANDLE::default();
    unsafe { OpenProcessToken(proc_h.raw(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut tok_raw) }
        .map_err(|e| CoreError::SystemTool {
            message: format!("OpenProcessToken(explorer.exe) falló: {}", e),
            source: None,
        })?;
    let tok_h = SafeHandle::new(tok_raw)
        .ok_or_else(|| CoreError::from_win32("Token de explorer.exe inválido"))?;

    let mut primary_raw = HANDLE::default();
    unsafe {
        DuplicateTokenEx(
            tok_h.raw(),
            TOKEN_ALL_ACCESS,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut primary_raw,
        )
    }
    .map_err(|e| CoreError::SystemTool {
        message: format!("DuplicateTokenEx falló: {}", e),
        source: None,
    })?;

    SafeHandle::new(primary_raw)
        .ok_or_else(|| CoreError::from_win32("Token primario duplicado inválido"))
}

/// Carpeta de perfil del usuario dueño de `token` (p. ej. `C:\Users\Ana`).
pub fn user_profile_dir(token: &SafeHandle) -> CoreResult<PathBuf> {
    let mut len = 0u32;
    let _ = unsafe { GetUserProfileDirectoryW(token.raw(), PWSTR::null(), &mut len) };
    if len == 0 || len > 4096 {
        return Err(CoreError::SystemTool {
            message: "No se pudo obtener la carpeta de perfil del usuario".into(),
            source: None,
        });
    }
    let mut buf = vec![0u16; len as usize];
    unsafe { GetUserProfileDirectoryW(token.raw(), PWSTR(buf.as_mut_ptr()), &mut len) }.map_err(
        |e| CoreError::SystemTool {
            message: format!("GetUserProfileDirectoryW falló: {}", e),
            source: None,
        },
    )?;
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
}

struct EnvBlockGuard(*mut std::ffi::c_void);
impl Drop for EnvBlockGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = DestroyEnvironmentBlock(self.0);
            }
        }
    }
}

/// Qué ejecutar y cómo.
pub struct Launch<'a> {
    /// Ruta absoluta del ejecutable (se pasa como `lpApplicationName`: no hay búsqueda por PATH).
    pub application: &'a Path,
    /// Línea de comandos completa, con el ejecutable entrecomillado como primer argumento.
    pub command_line: String,
    /// Directorio de trabajo explícito.
    pub working_dir: &'a Path,
    /// Si existe, el proceso corre sin elevación con este token (el del Explorador).
    pub user_token: Option<&'a SafeHandle>,
}

/// Resultado de un proceso que terminó dentro del tiempo.
#[derive(Debug, Clone, Copy)]
pub struct ProcessExit {
    pub exit_code: u32,
}

impl ProcessExit {
    /// Códigos que Windows Installer y la mayoría de instaladores usan para «terminó bien»
    /// (0, 3010 y 1641 = requiere reinicio).
    pub fn is_success(&self) -> bool {
        matches!(self.exit_code, 0 | 3010 | 1641)
    }

    /// Código para mostrar: los HRESULT (p. ej. `0x80040902` de EdgeUpdate) en hexadecimal, que es como se
    /// buscan; los códigos chicos (1603, 3010…) en decimal.
    pub fn code_display(&self) -> String {
        if self.exit_code > 0xFFFF {
            format!("0x{:08X}", self.exit_code)
        } else {
            self.exit_code.to_string()
        }
    }
}

fn active_processes(job: &JobObject) -> CoreResult<u32> {
    let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    unsafe {
        QueryInformationJobObject(
            job.raw(),
            JobObjectBasicAccountingInformation,
            &mut info as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            None,
        )
    }
    .map_err(|e| CoreError::SystemTool {
        message: format!("QueryInformationJobObject falló: {}", e),
        source: None,
    })?;
    Ok(info.ActiveProcesses)
}

/// Ejecuta `launch` dentro de un Job «kill on close» y espera a que el Job completo quede vacío.
/// Devuelve el código de salida del proceso inicial. Con timeout o cancelación, termina el Job.
pub fn run_in_job(
    launch: &Launch<'_>,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> CoreResult<ProcessExit> {
    let job = JobObject::new_kill_on_close()?;
    let app_w = to_wide_null(launch.application);
    let cwd_w = to_wide_null(launch.working_dir);
    let mut cmd_w = to_wide_null(&launch.command_line);
    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();

    if let Some(token) = launch.user_token {
        let mut env_block: *mut std::ffi::c_void = std::ptr::null_mut();
        unsafe { CreateEnvironmentBlock(&mut env_block, token.raw(), false) }.map_err(|e| {
            CoreError::SystemTool {
                message: format!("CreateEnvironmentBlock falló: {}", e),
                source: None,
            }
        })?;
        let _env = EnvBlockGuard(env_block);
        unsafe {
            CreateProcessWithTokenW(
                token.raw(),
                LOGON_WITH_PROFILE,
                PCWSTR(app_w.as_ptr()),
                PWSTR(cmd_w.as_mut_ptr()),
                CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
                Some(env_block),
                PCWSTR(cwd_w.as_ptr()),
                &si,
                &mut pi,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!(
                "No se pudo ejecutar '{}' sin elevación: {}",
                launch.application.display(),
                e
            ),
            source: None,
        })?;
    } else {
        unsafe {
            CreateProcessW(
                PCWSTR(app_w.as_ptr()),
                PWSTR(cmd_w.as_mut_ptr()),
                None,
                None,
                false,
                CREATE_SUSPENDED,
                None,
                PCWSTR(cwd_w.as_ptr()),
                &si,
                &mut pi,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!(
                "No se pudo ejecutar '{}': {}",
                launch.application.display(),
                e
            ),
            source: None,
        })?;
    }

    let process = SafeHandle::new(pi.hProcess)
        .ok_or_else(|| CoreError::from_win32("Handle de proceso inválido"))?;
    let thread = SafeHandle::new(pi.hThread)
        .ok_or_else(|| CoreError::from_win32("Handle de hilo inválido"))?;

    if let Err(e) = job.assign(process.raw()) {
        unsafe {
            let _ = TerminateProcess(process.raw(), 1);
        }
        return Err(e);
    }
    if unsafe { ResumeThread(thread.raw()) } == u32::MAX {
        job.terminate_all(1);
        return Err(CoreError::from_win32("ResumeThread falló"));
    }
    drop(thread);

    let started = Instant::now();
    let mut exit_code: Option<u32> = None;
    loop {
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            job.terminate_all(1);
            return Err(CoreError::Cancelled);
        }
        if started.elapsed() >= timeout {
            job.terminate_all(1);
            return Err(CoreError::SystemTool {
                message: format!(
                    "'{}' excedió el tiempo límite de {} s y se terminó",
                    launch.application.display(),
                    timeout.as_secs()
                ),
                source: None,
            });
        }

        if exit_code.is_none() {
            let w = unsafe { WaitForSingleObject(process.raw(), 250) };
            if w == WAIT_OBJECT_0 {
                let mut code = 0u32;
                if let Err(e) = unsafe { GetExitCodeProcess(process.raw(), &mut code) } {
                    job.terminate_all(1);
                    return Err(CoreError::SystemTool {
                        message: format!("GetExitCodeProcess falló: {}", e),
                        source: None,
                    });
                }
                exit_code = Some(code);
            } else if w != WAIT_TIMEOUT {
                job.terminate_all(1);
                return Err(CoreError::from_win32("WaitForSingleObject falló"));
            }
        } else {
            std::thread::sleep(Duration::from_millis(250));
        }

        if let Some(code) = exit_code {
            if active_processes(&job)? == 0 {
                return Ok(ProcessExit { exit_code: code });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProcessExit;

    #[test]
    fn codigo_de_salida_legible() {
        assert_eq!(ProcessExit { exit_code: 1603 }.code_display(), "1603");
        assert_eq!(
            ProcessExit {
                exit_code: 2_147_748_098
            }
            .code_display(),
            "0x80040902"
        );
        assert!(ProcessExit { exit_code: 3010 }.is_success());
        assert!(!ProcessExit { exit_code: 1602 }.is_success());
    }
}
