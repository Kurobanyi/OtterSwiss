//! Handles de Windows con cierre automático (RAII): archivos y procesos, DLL, servicios, Job Objects y COM.

use std::path::Path;
use std::ptr;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SetupDiDestroyDeviceInfoList, SetupDiGetClassDevsW,
};
use windows::Win32::Foundation::{
    CloseHandle, FreeLibrary, HANDLE, HMODULE, INVALID_HANDLE_VALUE, RPC_E_CHANGED_MODE,
};
use windows::Win32::Networking::WinInet::InternetCloseHandle;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::Win32::System::Services::{CloseServiceHandle, SC_HANDLE};
use windows::core::PCWSTR;

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
        if !self.0.is_invalid() && !self.0.0.is_null() {
            // SAFETY: el handle es propio y se cierra una sola vez (luego queda inválido).
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
            // SAFETY: la ruta termina en NUL; la búsqueda de dependencias se limita a System32 o a la carpeta de la DLL.
            unsafe { LoadLibraryExW(PCWSTR(wide.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .map_err(|e| CoreError::SystemTool {
                    message: format!("No se pudo cargar DLL del sistema '{}': {}", dll_name, e),
                    source: Some(Box::new(e)),
                })?;
        Ok(Self(module))
    }

    pub fn load_from_verified_path(path: &Path) -> CoreResult<Self> {
        let wide = to_wide_null(path);
        // Las dependencias de la DLL se buscan solo junto a ella y en System32, nunca en el directorio actual.
        // SAFETY: la ruta termina en NUL; la búsqueda de dependencias se limita a System32 o a la carpeta de la DLL.
        let module = unsafe {
            LoadLibraryExW(
                PCWSTR(wide.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("No se pudo cargar DLL '{}': {}", path.display(), e),
            source: Some(Box::new(e)),
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
        if !self.0.0.is_null() {
            // SAFETY: el módulo lo cargó este SafeLibrary y se libera una sola vez.
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
        if !self.0.0.is_null() {
            // SAFETY: el handle del SCM o del servicio es propio y se cierra una sola vez.
            unsafe {
                let _ = CloseServiceHandle(self.0);
            }
        }
    }
}

pub struct JobObject(SafeHandle);

impl JobObject {
    pub fn new_kill_on_close() -> CoreResult<Self> {
        // SAFETY: sin nombre ni atributos; el handle devuelto pasa a SafeHandle.
        let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .map_err(CoreError::sys("Error creando Job Object: "))?;
        let handle = SafeHandle::new(raw).ok_or_else(|| {
            CoreError::invalid_handle("CreateJobObjectW devolvió handle inválido")
        })?;

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        // SAFETY: el Job es válido y la estructura tiene el tamaño que se informa.
        unsafe {
            SetInformationJobObject(
                handle.raw(),
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(CoreError::sys(
            "Error configurando límites del Job Object: ",
        ))?;

        Ok(Self(handle))
    }

    pub fn assign(&self, process: HANDLE) -> CoreResult<()> {
        // SAFETY: el Job y el proceso son handles válidos.
        unsafe { AssignProcessToJobObject(self.0.raw(), process) }
            .map_err(CoreError::sys("Error asignando proceso al Job Object: "))
    }

    pub fn raw(&self) -> HANDLE {
        self.0.raw()
    }

    pub fn terminate_all(&self, exit_code: u32) {
        // SAFETY: el Job es válido; termina todos sus procesos.
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
        // SAFETY: inicializa COM en este hilo; el resultado se respeta al cerrar.
        let hr = unsafe { CoInitializeEx(Some(ptr::null()), COINIT_MULTITHREADED) };
        Self {
            initialized: hr.is_ok(),
        }
    }

    /// COM en modo STA (Windows Update Agent lo exige). Si el hilo ya tenía COM en otro modo se usa como está y no
    /// se cierra al salir.
    pub fn init_sta() -> CoreResult<Self> {
        // SAFETY: inicializa COM en este hilo; `Drop` solo lo cierra si esta llamada tuvo éxito.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_ok() {
            Ok(Self { initialized: true })
        } else if hr == RPC_E_CHANGED_MODE {
            Ok(Self { initialized: false })
        } else {
            Err(CoreError::SystemTool {
                message: format!("CoInitializeEx(STA) falló: {hr:?}"),
                source: None,
            })
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: se llama solo si CoInitializeEx tuvo éxito en este hilo.
            unsafe {
                CoUninitialize();
            }
        }
    }
}

/// Lista de dispositivos de SetupAPI, liberada con `SetupDiDestroyDeviceInfoList`.
pub struct DevInfoList(HDEVINFO);

impl DevInfoList {
    /// Dispositivos presentes de todas las clases.
    pub fn present_devices() -> CoreResult<Self> {
        // SAFETY: sin punteros de entrada; el handle devuelto queda a cargo de este guardián.
        let handle = unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES | DIGCF_PRESENT)
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("SetupDiGetClassDevsW falló: {e}"),
            source: None,
        })?;
        if handle.is_invalid() {
            return Err(CoreError::SystemTool {
                message: "Handle de dispositivo SetupAPI inválido".into(),
                source: None,
            });
        }
        Ok(Self(handle))
    }

    pub fn raw(&self) -> HDEVINFO {
        self.0
    }
}

impl Drop for DevInfoList {
    fn drop(&mut self) {
        // SAFETY: el handle es válido (se comprobó al crearlo) y se libera una sola vez.
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

/// Handle de WinInet (sesión, conexión o petición).
pub struct WinInetHandle(*mut std::ffi::c_void);

impl WinInetHandle {
    pub fn new(ptr: *mut std::ffi::c_void) -> Option<Self> {
        if ptr.is_null() { None } else { Some(Self(ptr)) }
    }
    pub fn raw(&self) -> *mut std::ffi::c_void {
        self.0
    }
}

impl Drop for WinInetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: el handle de WinInet es propio y se cierra una sola vez.
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
            self.0 = ptr::null_mut();
        }
    }
}
