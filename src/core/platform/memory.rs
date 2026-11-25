//! Buffers para estructuras que devuelven las APIs de Windows.

/// Buffer de bytes alineado a 8. Las APIs de Windows devuelven estructuras dentro de un buffer de bytes;
/// reinterpretar un `Vec<u8>` (alineado a 1) como esa estructura es comportamiento indefinido (SEC-28).
pub struct AlignedBuf {
    words: Vec<u64>,
    len: usize,
}

impl AlignedBuf {
    pub fn zeroed(len: usize) -> Self {
        Self {
            words: vec![0u64; len.div_ceil(8).max(1)],
            len,
        }
    }
}

impl std::ops::Deref for AlignedBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        // SAFETY: `words` tiene al menos `len` bytes y vive tanto como el slice prestado.
        unsafe { std::slice::from_raw_parts(self.words.as_ptr() as *const u8, self.len) }
    }
}

impl std::ops::DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY: `words` tiene al menos `len` bytes y el préstamo mutable es exclusivo.
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr() as *mut u8, self.len) }
    }
}

/// CL-02: memoria que una API de Windows reservó con `LocalAlloc` (descriptores, ACL, GUID de energía). Se libera
/// con `LocalFree` al soltarse, también si quien la usa sale antes con `?`.
pub struct LocalMem(*mut std::ffi::c_void);

impl LocalMem {
    /// Toma posesión de `ptr` (puede ser nulo).
    pub fn new(ptr: *mut std::ffi::c_void) -> Self {
        Self(ptr)
    }

    pub fn as_ptr(&self) -> *mut std::ffi::c_void {
        self.0
    }
}

impl Drop for LocalMem {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: el puntero lo reservó una API con LocalAlloc y solo se libera aquí.
            unsafe {
                let _ = windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
                    self.0,
                ));
            }
        }
    }
}

/// CL-02: descriptor de seguridad construido desde SDDL, liberado al soltarse.
pub struct SecurityDescriptor(LocalMem);

impl SecurityDescriptor {
    pub fn from_sddl(sddl: &str) -> crate::core::error::CoreResult<Self> {
        use windows::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        let wide = crate::core::platform::wide::to_wide_null(sddl);
        let mut psd = windows::Win32::Security::PSECURITY_DESCRIPTOR::default();
        // SAFETY: `wide` termina en NUL; el descriptor devuelto pasa a ser de `LocalMem`.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                windows::core::PCWSTR(wide.as_ptr()),
                SDDL_REVISION_1,
                &mut psd,
                None,
            )
        }
        .map_err(crate::core::error::CoreError::sys(
            "No se pudo construir el descriptor de seguridad: ",
        ))?;
        Ok(Self(LocalMem::new(psd.0)))
    }

    /// Atributos para pasar a `CreateDirectoryW`, `CreateMutexW`… Válidos mientras viva `self`.
    pub fn attributes(&self) -> windows::Win32::Security::SECURITY_ATTRIBUTES {
        windows::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<windows::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.as_ptr(),
            bInheritHandle: false.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_desde_sddl() {
        let sd = SecurityDescriptor::from_sddl("D:P(A;;GA;;;BA)(A;;GA;;;SY)").unwrap();
        assert!(!sd.attributes().lpSecurityDescriptor.is_null());
        assert!(SecurityDescriptor::from_sddl("esto no es SDDL").is_err());
    }

    /// El mutex con DACL solo-admin se puede crear y tomar (nombre local, sin `Global\`).
    #[test]
    fn mutex_con_descriptor() {
        let name = format!("OtterSwiss_test_{}", std::process::id());
        let held = crate::core::platform::mutex::NamedMutex::acquire(&name).unwrap();
        drop(held);
    }
}
