//! Formatos TAR y TAR.GZ.

use super::count_source_stats;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use tar::{Archive as TarArchive, Builder as TarBuilder, EntryType};

use super::paths::sanitize_archive_rel_path;
use super::progress::{ArchiveProgress, QuotaCopy, TelemetryCtx};
use super::staging::{
    create_staging_dir, discard_staging, finalize_staging_dir, read_zone_identifier,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{create_verified_new_file, open_verified_file_read};

pub(super) fn extract_tar(
    archive_path: &Path,
    dest_dir: &Path,
    max_quota_bytes: u64,
    cancel: &AtomicBool,
    progress: Option<&Arc<Mutex<ArchiveProgress>>>,
) -> CoreResult<u64> {
    let file = File::open(archive_path)?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let zone = read_zone_identifier(archive_path);
    extract_tar_reader(file, len, dest_dir, max_quota_bytes, cancel, progress, zone)
}

pub(super) fn extract_tar_gz(
    archive_path: &Path,
    dest_dir: &Path,
    max_quota_bytes: u64,
    cancel: &AtomicBool,
    progress: Option<&Arc<Mutex<ArchiveProgress>>>,
) -> CoreResult<u64> {
    let file = File::open(archive_path)?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let gz = GzDecoder::new(file);
    let zone = read_zone_identifier(archive_path);
    extract_tar_reader(gz, len, dest_dir, max_quota_bytes, cancel, progress, zone)
}

pub(super) fn extract_tar_reader<R: Read>(
    reader: R,
    approx_bytes: u64,
    dest_dir: &Path,
    max_quota_bytes: u64,
    cancel: &AtomicBool,
    progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    zone: Option<Vec<u8>>,
) -> CoreResult<u64> {
    let mut tar = TarArchive::new(reader);
    let mut tel = TelemetryCtx::new(progress);
    tel.init(0, approx_bytes);

    let (parent, tmp_dir) = create_staging_dir(dest_dir)?;
    let mut quota = QuotaCopy::new(max_quota_bytes, cancel);
    let mut files_done = 0usize;

    let res = (|| -> CoreResult<()> {
        let entries = tar.entries().map_err(|e| CoreError::Archive {
            message: format!("Error leyendo entradas TAR: {}", e),
            source: Some(Box::new(e)),
        })?;

        for entry_res in entries {
            if cancel.load(Ordering::Relaxed) {
                return Err(CoreError::Cancelled);
            }
            let mut entry = entry_res.map_err(|e| CoreError::Archive {
                message: format!("Entrada TAR corrupta: {}", e),
                source: Some(Box::new(e)),
            })?;

            let etype = entry.header().entry_type();
            if matches!(
                etype,
                EntryType::Symlink
                    | EntryType::Link
                    | EntryType::Char
                    | EntryType::Block
                    | EntryType::Fifo
            ) {
                return Err(CoreError::Validation(
                    "Symlink, hardlink o dispositivo rechazado en TAR".into(),
                ));
            }

            let raw_path = entry.path().map_err(|e| CoreError::Archive {
                message: format!("Ruta TAR inválida: {}", e),
                source: Some(Box::new(e)),
            })?;
            let rel = sanitize_archive_rel_path(&raw_path.to_string_lossy())?;
            let rel_display = rel.to_string_lossy().into_owned();
            let out_path = tmp_dir.join(&rel);
            files_done += 1;

            if etype.is_dir() {
                std::fs::create_dir_all(&out_path)?;
            } else if etype.is_file() {
                quota.copy(&mut entry, &out_path, &rel_display, files_done, &mut tel)?;
            }
            tel.tick(Some(&rel_display), quota.written, files_done, false);
        }
        Ok(())
    })();

    if let Err(e) = res {
        discard_staging(&tmp_dir, parent);
        return Err(e);
    }

    finalize_staging_dir(&tmp_dir, dest_dir, parent, zone.as_deref())?;
    tel.finish(quota.written, files_done);
    Ok(quota.written)
}

pub(super) fn compress_to_tar_gz(
    src_path: &Path,
    dest_archive: &Path,
    cancel: &AtomicBool,
    progress: Option<&Arc<Mutex<ArchiveProgress>>>,
) -> CoreResult<u64> {
    if cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    let (total_files, total_bytes_est) = count_source_stats(src_path);
    let mut tel = TelemetryCtx::new(progress);
    tel.init(total_files, total_bytes_est);
    tel.tick(Some(&src_path.to_string_lossy()), 0, 0, true);

    let (file, _) = create_verified_new_file(dest_archive, true)?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar = TarBuilder::new(enc);
    // No seguir symlinks ni junctions: la compresión corre elevada y podría empaquetar archivos de
    // fuera del origen que el usuario no puede leer (SEC-16).
    tar.follow_symlinks(false);

    if src_path.is_file() {
        let (mut f, _) = open_verified_file_read(src_path, false, None)?;
        let name = src_path
            .file_name()
            .ok_or_else(|| CoreError::Validation("Nombre de archivo inválido".into()))?;
        tar.append_file(name, &mut f)
            .map_err(|e| CoreError::Archive {
                message: format!("Error agregando archivo a TAR: {}", e),
                source: Some(Box::new(e)),
            })?;
    } else if src_path.is_dir() {
        tar.append_dir_all(".", src_path)
            .map_err(|e| CoreError::Archive {
                message: format!("Error empaquetando directorio TAR: {}", e),
                source: Some(Box::new(e)),
            })?;
    }

    tar.finish().map_err(|e| CoreError::Archive {
        message: format!("Error finalizando TAR.GZ: {}", e),
        source: Some(Box::new(e)),
    })?;

    let sz = dest_archive.metadata().map(|m| m.len()).unwrap_or(0);
    tel.finish(sz, total_files);
    Ok(sz)
}
