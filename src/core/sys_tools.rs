use std::path::Path;

use crate::core::platform::folders::windows_dir;

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
