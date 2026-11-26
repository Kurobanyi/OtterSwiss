//! Archivador: extraer y comprimir ZIP, 7z, TAR y TAR.GZ sin salirse del destino ni sobrescribir nada.

mod format_7z;
mod format_tar;
mod format_zip;
mod paths;
mod progress;
mod staging;
use format_7z::{compress_to_7z, extract_7z};
use format_tar::{compress_to_tar_gz, extract_tar, extract_tar_gz};
use format_zip::{compress_to_zip, extract_zip};

pub use paths::{is_reserved_windows_name, resolve_extract_destination, validate_path_components};
pub use progress::ArchiveProgress;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::delete_verified_file;
use paths::validate_cli_paths;

/// AR-16: tope de bytes de una extracción (bomba de descompresión), igual en la ventana principal y en la mini.
pub const DEFAULT_EXTRACT_QUOTA: u64 = 50 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZip,
    TarGz,
}

/// CL-13: qué hace un trabajo del archivador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveMode {
    Extract,
    Compress,
}

pub fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    max_quota_bytes: u64,
    cancel: &AtomicBool,
) -> CoreResult<u64> {
    extract_archive_with_progress(archive_path, dest_dir, max_quota_bytes, cancel, None)
}

pub fn extract_archive_with_progress(
    archive_path: &Path,
    dest_dir: &Path,
    max_quota_bytes: u64,
    cancel: &AtomicBool,
    progress: Option<&Arc<Mutex<ArchiveProgress>>>,
) -> CoreResult<u64> {
    validate_cli_paths(archive_path, dest_dir)?;

    let lower = archive_path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if lower.ends_with(".zip") {
        extract_zip(archive_path, dest_dir, max_quota_bytes, cancel, progress)
    } else if lower.ends_with(".7z") {
        extract_7z(archive_path, dest_dir, max_quota_bytes, cancel, progress)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        extract_tar_gz(archive_path, dest_dir, max_quota_bytes, cancel, progress)
    } else if lower.ends_with(".tar") {
        extract_tar(archive_path, dest_dir, max_quota_bytes, cancel, progress)
    } else {
        Err(CoreError::Validation(format!(
            "Formato no soportado (ZIP, 7Z, TAR/TAR.GZ): '{}'",
            archive_path.display()
        )))
    }
}

pub fn compress_path(
    src_path: &Path,
    dest_archive: &Path,
    format: ArchiveFormat,
    cancel: &AtomicBool,
) -> CoreResult<u64> {
    compress_path_with_progress(src_path, dest_archive, format, cancel, None)
}

pub fn compress_path_with_progress(
    src_path: &Path,
    dest_archive: &Path,
    format: ArchiveFormat,
    cancel: &AtomicBool,
    progress: Option<&Arc<Mutex<ArchiveProgress>>>,
) -> CoreResult<u64> {
    validate_cli_paths(src_path, dest_archive)?;
    if dest_archive.exists() {
        return Err(CoreError::Validation(format!(
            "El archivo '{}' ya existe; no se sobrescribe",
            dest_archive.display()
        )));
    }

    // Se escribe en un temporal y solo se renombra al terminar bien: cancelar o fallar nunca deja
    // un .zip truncado con apariencia de válido (SYS-22).
    let name = dest_archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archivo".into());
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let partial = dest_archive.with_file_name(format!(".{}.{}.partial", name, nonce));

    let res = match format {
        ArchiveFormat::Zip => compress_to_zip(src_path, &partial, cancel, progress),
        ArchiveFormat::SevenZip => compress_to_7z(src_path, &partial, cancel, progress),
        ArchiveFormat::TarGz => compress_to_tar_gz(src_path, &partial, cancel, progress),
    };
    let res = res.and_then(|n| {
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        if dest_archive.exists() {
            return Err(CoreError::Validation(format!(
                "El archivo '{}' apareció mientras se comprimía; no se sobrescribe",
                dest_archive.display()
            )));
        }
        std::fs::rename(&partial, dest_archive)?;
        Ok(n)
    });
    if res.is_err() && partial.exists() {
        let _ = delete_verified_file(&partial);
    }
    res
}

fn count_source_stats(src: &Path) -> (usize, u64) {
    if src.is_file() {
        let sz = src.metadata().map(|m| m.len()).unwrap_or(0);
        return (1, sz);
    }
    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![src.to_path_buf()];
    while let Some(cur) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&cur) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_symlink() {
                        continue;
                    }
                    if ft.is_dir() {
                        stack.push(entry.path());
                    } else if ft.is_file() {
                        count += 1;
                        bytes =
                            bytes.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
                    }
                }
            }
        }
    }
    (count, bytes)
}

/// Carpeta junto al archivo con su nombre sin extensión (`datos.tar.gz` → `datos`). No comprueba si existe:
/// para eso está `resolve_extract_destination`.
pub fn default_extract_dir(archive: &Path) -> PathBuf {
    let parent = archive.parent().unwrap_or(archive);
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let low = name.to_ascii_lowercase();
    let stem = if low.ends_with(".tar.gz") {
        &name[..name.len() - ".tar.gz".len()]
    } else {
        archive.file_stem().and_then(|s| s.to_str()).unwrap_or("")
    };
    parent.join(if stem.is_empty() { "extracted" } else { stem })
}

/// `.zip` junto a la ruta, con su nombre completo (UI-15: `mi.proyecto\` da `mi.proyecto.zip`, no `mi.zip`).
pub fn default_zip_path(src: &Path) -> PathBuf {
    let name = src.file_name().unwrap_or_default().to_string_lossy();
    src.parent().unwrap_or(src).join(format!("{name}.zip"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carpeta_de_extraccion_quita_la_extension_compuesta() {
        let dir = |p: &str| default_extract_dir(Path::new(p));
        assert_eq!(dir(r"C:\d\datos.zip"), PathBuf::from(r"C:\d\datos"));
        assert_eq!(dir(r"C:\d\datos.tar.gz"), PathBuf::from(r"C:\d\datos"));
        assert_eq!(dir(r"C:\d\datos.tgz"), PathBuf::from(r"C:\d\datos"));
        // Sin nombre aparte de la extensión, la carpeta coincide con el archivo: resolve_extract_destination usa «(2)».
        assert_eq!(dir(r"C:\d\.zip"), PathBuf::from(r"C:\d\.zip"));
    }

    #[test]
    fn zip_conserva_el_nombre_completo() {
        assert_eq!(
            default_zip_path(Path::new(r"C:\d\mi.proyecto")),
            PathBuf::from(r"C:\d\mi.proyecto.zip")
        );
    }
}
