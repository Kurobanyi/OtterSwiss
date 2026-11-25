//! Cadenas anchas (UTF-16) de las APIs de Windows.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::core::PWSTR;

pub fn to_wide_null(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn wide_ptr_to_string_and_free(ptr: PWSTR) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let result = unsafe {
        let mut len = 0usize;
        while *ptr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr.0, len);
        String::from_utf16_lossy(slice)
    };
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
