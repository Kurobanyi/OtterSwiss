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

use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE, TOKEN_QUERY,
    TokenPrimary,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::JobObjects::{
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
    QueryInformationJobObject,
};
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, CreateProcessWithTokenW,
    GetExitCodeProcess, LOGON_WITH_PROFILE, OpenProcess, OpenProcessToken, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, ResumeThread, STARTUPINFOW, TerminateProcess,
    WaitForSingleObject,
};
use windows::Win32::UI::Shell::GetUserProfileDirectoryW;
use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};
use windows::core::{PCWSTR, PWSTR};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::{JobObject, SafeHandle};
use crate::core::platform::wide::to_wide_null;

/// Carpeta `System32` real, sin depender de variables de entorno.
pub fn system32_dir() -> CoreResult<PathBuf> {
    let mut buf = [0u16; 260];
    let len =
        // SAFETY: el búfer es nuestro y la API escribe como mucho su longitud.
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
    // SAFETY: función sin parámetros; devuelve un HWND o nulo.
    let shell_hwnd = unsafe { GetShellWindow() };
    if shell_hwnd.0.is_null() {
        return Err(CoreError::SystemTool {
            message: "No se encontró el Explorador de escritorio para ejecutar sin elevación"
                .into(),
            source: None,
        });
    }
    let mut shell_pid = 0u32;
    // SAFETY: `hwnd` viene de GetShellWindow y `pid` es una variable local válida.
    unsafe {
        GetWindowThreadProcessId(shell_hwnd, Some(&mut shell_pid));
    }
    if shell_pid == 0 {
        return Err(CoreError::SystemTool {
            message: "No se pudo obtener el PID de explorer.exe".into(),
            source: None,
        });
    }

    // SAFETY: solo se pide acceso de consulta; el handle devuelto pasa a SafeHandle.
    let proc_raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, shell_pid) }
        .map_err(CoreError::sys("OpenProcess(explorer.exe) falló: "))?;
    let proc_h = SafeHandle::new(proc_raw)
        .ok_or_else(|| CoreError::invalid_handle("Handle de explorer.exe inválido"))?;

    let mut tok_raw = HANDLE::default();
    // SAFETY: el handle de proceso es válido y el token devuelto pasa a SafeHandle.
    unsafe { OpenProcessToken(proc_h.raw(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut tok_raw) }
        .map_err(CoreError::sys("OpenProcessToken(explorer.exe) falló: "))?;
    let tok_h = SafeHandle::new(tok_raw)
        .ok_or_else(|| CoreError::invalid_handle("Token de explorer.exe inválido"))?;

    let mut primary_raw = HANDLE::default();
    // SAFETY: el token de origen es válido; el duplicado pasa a SafeHandle.
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
    .map_err(CoreError::sys("DuplicateTokenEx falló: "))?;

    SafeHandle::new(primary_raw)
        .ok_or_else(|| CoreError::invalid_handle("Token primario duplicado inválido"))
}

/// Carpeta de perfil del usuario dueño de `token` (p. ej. `C:\Users\Ana`).
pub fn user_profile_dir(token: &SafeHandle) -> CoreResult<PathBuf> {
    let mut len = 0u32;
    // SAFETY: el token es válido y el búfer tiene la longitud que se informa en `len`.
    let _ = unsafe { GetUserProfileDirectoryW(token.raw(), None, &mut len) };
    if len == 0 || len > 4096 {
        return Err(CoreError::SystemTool {
            message: "No se pudo obtener la carpeta de perfil del usuario".into(),
            source: None,
        });
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: el token es válido y el búfer tiene la longitud que se informa en `len`.
    unsafe { GetUserProfileDirectoryW(token.raw(), Some(PWSTR(buf.as_mut_ptr())), &mut len) }
        .map_err(|e| CoreError::SystemTool {
            message: format!("GetUserProfileDirectoryW falló: {}", e),
            source: Some(Box::new(e)),
        })?;
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
}

struct EnvBlockGuard(*mut std::ffi::c_void);
impl Drop for EnvBlockGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: el bloque lo creó CreateEnvironmentBlock y solo se destruye aquí, una vez.
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

/// AR-16: tiempo máximo que se espera a un instalador o desinstalador.
pub const INSTALLER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// `ERROR_PRODUCT_VERSION` de Windows Installer: otra versión del producto ya está instalada.
pub const ERROR_PRODUCT_VERSION: u32 = 1638;

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

    /// Para un instalador: éxito, o que ya había otra versión instalada (`ERROR_PRODUCT_VERSION`).
    pub fn is_installed_ok(&self) -> bool {
        self.is_success() || self.exit_code == ERROR_PRODUCT_VERSION
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
    // SAFETY: el Job es válido y la estructura de salida tiene el tamaño que se informa.
    unsafe {
        QueryInformationJobObject(
            Some(job.raw()),
            JobObjectBasicAccountingInformation,
            &mut info as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            None,
        )
    }
    .map_err(CoreError::sys("QueryInformationJobObject falló: "))?;
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
        // SAFETY: el token es válido; el bloque pasa al guardián que lo destruye.
        unsafe { CreateEnvironmentBlock(&mut env_block, Some(token.raw()), false) }
            .map_err(CoreError::sys("CreateEnvironmentBlock falló: "))?;
        let _env = EnvBlockGuard(env_block);
        // SAFETY: cadenas terminadas en NUL que viven durante la llamada; el proceso nace suspendido y los handles devueltos pasan a SafeHandle.
        unsafe {
            CreateProcessWithTokenW(
                token.raw(),
                LOGON_WITH_PROFILE,
                PCWSTR(app_w.as_ptr()),
                Some(PWSTR(cmd_w.as_mut_ptr())),
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
            source: Some(Box::new(e)),
        })?;
    } else {
        // SAFETY: cadenas terminadas en NUL que viven durante la llamada; el proceso nace suspendido y los handles devueltos pasan a SafeHandle.
        unsafe {
            CreateProcessW(
                PCWSTR(app_w.as_ptr()),
                Some(PWSTR(cmd_w.as_mut_ptr())),
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
            source: Some(Box::new(e)),
        })?;
    }

    let process = SafeHandle::new(pi.hProcess)
        .ok_or_else(|| CoreError::invalid_handle("Handle de proceso inválido"))?;
    let thread = SafeHandle::new(pi.hThread)
        .ok_or_else(|| CoreError::invalid_handle("Handle de hilo inválido"))?;

    if let Err(e) = job.assign(process.raw()) {
        // SAFETY: el handle del proceso es válido; se termina porque no se pudo meter en el Job.
        unsafe {
            let _ = TerminateProcess(process.raw(), 1);
        }
        return Err(e);
    }
    // SAFETY: el hilo principal es válido y el proceso ya está dentro del Job.
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
            // SAFETY: el handle es válido mientras vive su dueño.
            let w = unsafe { WaitForSingleObject(process.raw(), 250) };
            if w == WAIT_OBJECT_0 {
                let mut code = 0u32;
                // SAFETY: el proceso es válido y `code` es una variable local.
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

        if let Some(code) = exit_code
            && active_processes(&job)? == 0
        {
            return Ok(ProcessExit { exit_code: code });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ERROR_PRODUCT_VERSION, ProcessExit, run_captured, system32_dir};

    /// SYS-02: solo 0, 3010 y 1641 cuentan como éxito de un (des)instalador.
    #[test]
    fn codigos_de_salida_de_exito() {
        for ok in [0, 3010, 1641] {
            assert!(ProcessExit { exit_code: ok }.is_success());
        }
        for fail in [1, 1602, 1603, 5, u32::MAX] {
            assert!(!ProcessExit { exit_code: fail }.is_success());
        }
        assert!(
            ProcessExit {
                exit_code: ERROR_PRODUCT_VERSION
            }
            .is_installed_ok()
        );
    }

    #[test]
    fn captura_la_salida_de_una_herramienta() {
        let cmd = system32_dir().unwrap().join("cmd.exe");
        let out = run_captured(
            &cmd,
            &["/c", "echo hola"],
            std::time::Duration::from_secs(10),
        )
        .unwrap();
        assert!(out.success);
        assert_eq!(out.stdout.trim(), "hola");
    }

    #[test]
    fn corta_la_herramienta_que_pasa_el_tiempo() {
        let cmd = system32_dir().unwrap().join("cmd.exe");
        let started = std::time::Instant::now();
        let res = run_captured(
            &cmd,
            &["/c", "ping -n 30 127.0.0.1 > nul"],
            std::time::Duration::from_millis(300),
        );
        assert!(res.is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

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

/// `CREATE_NO_WINDOW`: el proceso hijo de consola no abre ventana.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Salida de un proceso de consola ya terminado.
#[derive(Debug, Clone)]
pub struct CapturedOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// AR-10: ejecuta una herramienta de consola de Windows (PowerShell, reg.exe…) sin ventana, leyendo stdout y
/// stderr en hilos propios mientras corre (patrón 3 de BUGS.md) y matándola si pasa `timeout`. `program` tiene
/// que ser una ruta absoluta (de `system32_dir`), nunca un nombre que se busque en `PATH`.
pub fn run_captured<S: AsRef<std::ffi::OsStr>>(
    program: &Path,
    args: &[S],
    timeout: std::time::Duration,
) -> CoreResult<CapturedOutput> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut child = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let spawn_reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_reader = spawn_reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err_reader = spawn_reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );

    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CoreError::SystemTool {
                message: format!(
                    "Tiempo límite excedido ejecutando '{}'",
                    program.file_name().unwrap_or_default().to_string_lossy()
                ),
                source: None,
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    let text = |reader: std::thread::JoinHandle<Vec<u8>>| {
        String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned()
    };
    Ok(CapturedOutput {
        success: status.success(),
        code: status.code(),
        stdout: text(out_reader),
        stderr: text(err_reader).trim().to_string(),
    })
}
