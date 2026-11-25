//! Carpetas conocidas de Windows.

use crate::core::platform::wide::wide_ptr_to_string_and_free;

/// Carpeta conocida de Windows por `SHGetKnownFolderPath`. Se usa en lugar de variables de entorno
/// (`%ProgramFiles%`, `%SystemRoot%`…), que un proceso sin elevar puede redefinir en `HKCU\Environment`.
pub fn known_folder(id: &windows::core::GUID) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    let s = wide_ptr_to_string_and_free(raw);
    if s.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(s))
    }
}

pub fn program_files_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_ProgramFiles)
}

pub fn program_files_x86_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_ProgramFilesX86)
}

pub fn windows_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_Windows)
}

pub fn local_app_data_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_LocalAppData)
}
