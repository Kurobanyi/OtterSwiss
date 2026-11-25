//! Handles de Windows con cierre automático (RAII): archivos y procesos, DLL, servicios, Job Objects y COM.

use std::path::Path;
use std::ptr;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, FreeLibrary, HANDLE, HMODULE, INVALID_HANDLE_VALUE};
use windows::Win32::Networking::WinInet::InternetCloseHandle;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows::Win32::System::Services::{CloseServiceHandle, SC_HANDLE};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::wide::to_wide_null;

pub struct SafeHandle(HANDLE);

impl SafeHandle {
    pub fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_invalid() || handle.0.is_null() {
            None
        } else {
            Some(Self(handle))
        }
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for SafeHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() && !self.0 .0.is_null() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
            self.0 = INVALID_HANDLE_VALUE;
        }
    }
}

pub struct SafeLibrary(HMODULE);

impl SafeLibrary {
    pub fn load_system(dll_name: &str) -> CoreResult<Self> {
        let wide = to_wide_null(dll_name);
        let module =
            unsafe { LoadLibraryExW(PCWSTR(wide.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .map_err(|e| CoreError::SystemTool {
                    message: format!("No se pudo cargar DLL del sistema '{}': {}", dll_name, e),
                    source: None,
                })?;
        Ok(Self(module))
    }

    pub fn load_from_verified_path(path: &Path) -> CoreResult<Self> {
        let wide = to_wide_null(path);
        // Las dependencias de la DLL se buscan solo junto a ella y en System32, nunca en el directorio actual.
        let module = unsafe {
            LoadLibraryExW(
                PCWSTR(wide.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("No se pudo cargar DLL '{}': {}", path.display(), e),
            source: None,
        })?;
        Ok(Self(module))
    }

    /// # Safety
    /// `symbol` debe terminar en NUL y el llamador debe convertir el puntero devuelto a la firma real
    /// de la función exportada antes de llamarla.
    pub unsafe fn get_proc(&self, symbol: &[u8]) -> Option<unsafe extern "system" fn() -> isize> {
        debug_assert!(symbol.ends_with(&[0]));
        // SAFETY: el módulo sigue cargado mientras viva `self` y `symbol` termina en NUL (contrato de la función).
        unsafe { GetProcAddress(self.0, windows::core::PCSTR(symbol.as_ptr())) }
    }
}

impl Drop for SafeLibrary {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe {
                let _ = FreeLibrary(self.0);
            }
        }
    }
}

pub struct ScHandle(SC_HANDLE);

impl ScHandle {
    pub fn new(handle: SC_HANDLE) -> Option<Self> {
        if handle.0.is_null() {
            None
        } else {
            Some(Self(handle))
        }
    }

    pub fn raw(&self) -> SC_HANDLE {
        self.0
    }
}

impl Drop for ScHandle {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe {
                let _ = CloseServiceHandle(self.0);
            }
        }
    }
}

pub struct JobObject(SafeHandle);

impl JobObject {
    pub fn new_kill_on_close() -> CoreResult<Self> {
        let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(|e| {
            CoreError::SystemTool {
                message: format!("Error creando Job Object: {}", e),
                source: None,
            }
        })?;
        let handle = SafeHandle::new(raw)
            .ok_or_else(|| CoreError::from_win32("CreateJobObjectW devolvió handle inválido"))?;

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        unsafe {
            SetInformationJobObject(
                handle.raw(),
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("Error configurando límites del Job Object: {}", e),
            source: None,
        })?;

        Ok(Self(handle))
    }

    pub fn assign(&self, process: HANDLE) -> CoreResult<()> {
        unsafe { AssignProcessToJobObject(self.0.raw(), process) }.map_err(|e| {
            CoreError::SystemTool {
                message: format!("Error asignando proceso al Job Object: {}", e),
                source: None,
            }
        })
    }

    pub fn raw(&self) -> HANDLE {
        self.0.raw()
    }

    pub fn terminate_all(&self, exit_code: u32) {
        unsafe {
            let _ = TerminateJobObject(self.0.raw(), exit_code);
        }
    }
}

pub struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    pub fn init_mt() -> Self {
        let hr = unsafe { CoInitializeEx(Some(ptr::null()), COINIT_MULTITHREADED) };
        Self {
            initialized: hr.is_ok(),
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

/// Handle de WinInet (sesión, conexión o petición).
pub struct WinInetHandle(*mut std::ffi::c_void);

impl WinInetHandle {
    pub fn new(ptr: *mut std::ffi::c_void) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self(ptr))
        }
    }
    pub fn raw(&self) -> *mut std::ffi::c_void {
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
