//! Cadenas anchas (UTF-16) de las APIs de Windows.

#![warn(clippy::undocumented_unsafe_blocks)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::core::PWSTR;

pub fn to_wide_null(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// CL-15: igual que `wide_ptr_to_string_and_free` pero sin pérdida, para rutas (un nombre de archivo puede no ser
/// Unicode válido).
pub fn wide_ptr_to_path_and_free(ptr: PWSTR) -> std::path::PathBuf {
    use std::os::windows::ffi::OsStringExt;
    if ptr.is_null() {
        return std::path::PathBuf::new();
    }
    // SAFETY: `ptr` es una cadena terminada en NUL que devolvió la API y se libera una sola vez, al final.
    unsafe {
        let mut len = 0usize;
        while *ptr.0.add(len) != 0 {
            len += 1;
        }
        let path = std::ffi::OsString::from_wide(std::slice::from_raw_parts(ptr.0, len));
        windows::Win32::System::Com::CoTaskMemFree(Some(ptr.0 as *const _));
        path.into()
    }
}

/// CL-15: clave para comparar rutas como identidad (sin distinguir mayúsculas, sin barra final), igual en todo el
/// core. NTFS no distingue mayúsculas también fuera de ASCII, así que se usa `to_lowercase`.
pub fn path_key(path: &std::path::Path) -> String {
    path.to_string_lossy().trim_end_matches('\\').to_lowercase()
}

pub fn wide_ptr_to_string_and_free(ptr: PWSTR) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: `ptr` apunta a una cadena terminada en NUL que devolvió la API.
    let result = unsafe {
        let mut len = 0usize;
        while *ptr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr.0, len);
        String::from_utf16_lossy(slice)
    };
    // SAFETY: la memoria la reservó la API con CoTaskMemAlloc y se libera una sola vez.
    unsafe {
        windows::Win32::System::Com::CoTaskMemFree(Some(ptr.0 as *const _));
    }
    result
}

/// Cadena ASCII terminada en NUL dentro de un buffer devuelto por una API (por ejemplo `STORAGE_DEVICE_DESCRIPTOR`).
/// Desplazamiento 0 o fuera del buffer = vacía.
pub fn read_cstr_at(buf: &[u8], offset: u32) -> String {
    let off = offset as usize;
    if offset == 0 || off >= buf.len() {
        return String::new();
    }
    let slice = &buf[off..];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}
