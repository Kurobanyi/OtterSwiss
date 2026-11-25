use std::path::Path;

use windows::Win32::System::ProcessStatus::{EnumProcesses, K32EmptyWorkingSet};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION,
    PROCESS_SET_QUOTA, QueryFullProcessImageNameW,
};
use windows::core::PWSTR;

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::folders::windows_dir;
use crate::core::platform::handles::SafeHandle;

#[derive(Debug, Clone, Default)]
pub struct TrimWorkingSetsReport {
    pub processes_trimmed: usize,
    pub processes_skipped: usize,
    pub errors_encountered: usize,
    pub trimmed_processes: Vec<String>,
}

/// Abre el Explorador en `target` (o en su carpeta si es un archivo). El Explorador se toma de la carpeta de Windows
/// por `SHGetKnownFolderPath`, no de `PATH` ni de `%SystemRoot%`.
pub fn open_folder_in_explorer(target: &Path) {
    let folder = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    };
    if let Some(windows) = windows_dir() {
        let _ = std::process::Command::new(windows.join("explorer.exe"))
            .arg(folder)
            .spawn();
    }
}

pub struct SysTools;

impl SysTools {
    pub fn get_process_ids() -> CoreResult<Vec<u32>> {
        let mut capacity = 1024usize;
        loop {
            let mut pids = vec![0u32; capacity];
            let mut bytes_returned = 0u32;

            // SAFETY: el búfer de PIDs tiene la capacidad en bytes que se pasa.
            let ok = unsafe {
                EnumProcesses(
                    pids.as_mut_ptr(),
                    (pids.len() * std::mem::size_of::<u32>()) as u32,
                    &mut bytes_returned,
                )
            };

            if ok.is_err() {
                return Err(CoreError::SystemTool {
                    message: "EnumProcesses falló".into(),
                    source: None,
                });
            }

            let num_returned = (bytes_returned as usize) / std::mem::size_of::<u32>();
            if bytes_returned == (pids.len() * std::mem::size_of::<u32>()) as u32 {
                capacity *= 2;
                continue;
            }

            pids.truncate(num_returned);
            return Ok(pids);
        }
    }

    pub fn trim_working_sets() -> CoreResult<TrimWorkingSetsReport> {
        ElevationManager::require_elevated("Optimizar conjuntos de trabajo de memoria (RAM)")?;

        // SAFETY: función sin parámetros.
        let own_pid = unsafe { GetCurrentProcessId() };
        let mut current_session = 0u32;
        // SAFETY: el PID es un número y la sesión de salida es una variable local.
        let _ = unsafe { ProcessIdToSessionId(own_pid, &mut current_session) };

        let pids = Self::get_process_ids()?;

        let mut report = TrimWorkingSetsReport {
            processes_trimmed: 0,
            processes_skipped: 0,
            errors_encountered: 0,
            trimmed_processes: Vec::new(),
        };

        for pid in pids {
            if pid <= 4 || pid == own_pid {
                report.processes_skipped += 1;
                continue;
            }

            let mut proc_session = 0u32;
            // SAFETY: el PID es un número y la sesión de salida es una variable local.
            if unsafe { ProcessIdToSessionId(pid, &mut proc_session) }.is_err()
                || proc_session != current_session
            {
                report.processes_skipped += 1;
                continue;
            }

            let raw_handle =
                // SAFETY: solo se pide acceso de consulta y de cuota; el handle devuelto pasa a SafeHandle.
                unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, false, pid) };

            let handle = match raw_handle {
                Ok(h) => match SafeHandle::new(h) {
                    Some(sh) => sh,
                    None => {
                        report.errors_encountered += 1;
                        continue;
                    }
                },
                Err(_) => {
                    report.processes_skipped += 1;
                    continue;
                }
            };

            // SAFETY: el handle del proceso es válido y tiene PROCESS_SET_QUOTA.
            let ok = unsafe { K32EmptyWorkingSet(handle.raw()) };
            if ok.as_bool() {
                report.processes_trimmed += 1;

                let mut buf = [0u16; 1024];
                let mut size = buf.len() as u32;
                // SAFETY: el handle del proceso es válido y el búfer tiene la longitud que se informa.
                let name = if unsafe {
                    QueryFullProcessImageNameW(
                        handle.raw(),
                        PROCESS_NAME_WIN32,
                        PWSTR(buf.as_mut_ptr()),
                        &mut size,
                    )
                }
                .is_ok()
                {
                    let full = String::from_utf16_lossy(&buf[..size as usize]);
                    Path::new(&full)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("PID {}", pid))
                } else {
                    format!("PID {}", pid)
                };

                report
                    .trimmed_processes
                    .push(format!("{} (PID {})", name, pid));
            } else {
                report.errors_encountered += 1;
            }
        }

        report
            .trimmed_processes
            .sort_by_key(|a| a.to_ascii_lowercase());

        Ok(report)
    }
}
