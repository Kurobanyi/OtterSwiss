//! Mutex con nombre entre procesos, solo para Administradores y SYSTEM.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::SafeHandle;
use crate::core::platform::wide::to_wide_null;

pub struct NamedMutex {
    handle: SafeHandle,
    locked: bool,
}

impl NamedMutex {
    /// Crea (o abre) el mutex con una DACL solo para Administradores y SYSTEM: así un usuario sin privilegios
    /// no puede crearlo antes y retenerlo para bloquear respaldos y rollbacks (SEC-23).
    fn open(name: &str) -> CoreResult<SafeHandle> {
        use windows::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

        let sddl = to_wide_null("D:P(A;;GA;;;BA)(A;;GA;;;SY)");
        let mut psd = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut psd,
                None,
            )
        }
        .map_err(CoreError::sys(
            "No se pudo construir el descriptor del mutex: ",
        ))?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: psd.0,
            bInheritHandle: false.into(),
        };
        let wide = to_wide_null(name);
        let res = unsafe { CreateMutexW(Some(&sa), false, PCWSTR(wide.as_ptr())) };
        unsafe {
            let _ =
                windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(psd.0));
        }
        let raw = res.map_err(|e| CoreError::SystemTool {
            message: format!("Error creando mutex '{}': {}", name, e),
            source: None,
        })?;
        SafeHandle::new(raw)
            .ok_or_else(|| CoreError::invalid_handle("CreateMutexW devolvió handle inválido"))
    }

    /// `WAIT_ABANDONED` significa que el dueño anterior murió sin soltarlo: el mutex ya es nuestro (CON-01).
    fn wait(handle: &SafeHandle, timeout_ms: u32) -> windows::Win32::Foundation::WAIT_EVENT {
        unsafe { WaitForSingleObject(handle.raw(), timeout_ms) }
    }

    pub fn acquire(name: &str) -> CoreResult<Self> {
        let handle = Self::open(name)?;
        let w = Self::wait(&handle, 0);
        if w == WAIT_OBJECT_0 || w == WAIT_ABANDONED {
            return Ok(Self {
                handle,
                locked: true,
            });
        }
        Err(CoreError::AlreadyRunning)
    }

    /// Espera el mutex hasta 60 s; nunca indefinidamente.
    pub fn acquire_blocking(name: &str) -> CoreResult<Self> {
        let handle = Self::open(name)?;
        let w = Self::wait(&handle, 60_000);
        if w == WAIT_OBJECT_0 || w == WAIT_ABANDONED {
            return Ok(Self {
                handle,
                locked: true,
            });
        }
        if w == WAIT_TIMEOUT {
            return Err(CoreError::AlreadyRunning);
        }
        Err(CoreError::from_win32("WaitForSingleObject falló en mutex"))
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        if self.locked {
            unsafe {
                let _ = ReleaseMutex(self.handle.raw());
            }
            self.locked = false;
        }
    }
}
