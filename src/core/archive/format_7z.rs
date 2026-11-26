//! Formato 7z.

use super::count_source_stats;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sevenz_rust2::{ArchiveReader, Password};

use super::paths::sanitize_archive_rel_path;
use super::progress::{ArchiveProgress, QuotaCopy, TelemetryCtx};
use super::staging::{
    create_staging_dir, discard_staging, finalize_staging_dir, read_zone_identifier,
};
use crate::core::error::{CoreError, CoreResult};

pub(super) fn extract_7z(
    archive_path: &Path,
    dest_dir: &Path,
    max_quota_bytes: u64,
    cancel: &AtomicBool,
    progress: Option<&Arc<Mutex<ArchiveProgress>>>,
) -> CoreResult<u64> {
    let zone = read_zone_identifier(archive_path);
    let file = File::open(archive_path)?;
    let mut reader =
        ArchiveReader::new(file, Password::empty()).map_err(|e| CoreError::Archive {
            message: format!("Error abriendo archivo 7Z: {}", e),
            source: Some(Box::new(e)),
        })?;

    let total_files = reader.archive().files.len();
    let total_uncompressed: u64 = reader.archive().files.iter().map(|f| f.size).sum();

    let mut tel = TelemetryCtx::new(progress);
    tel.init(total_files, total_uncompressed);

    let (parent, tmp_dir) = create_staging_dir(dest_dir)?;
    let mut quota = QuotaCopy::new(max_quota_bytes, cancel);
    let mut files_done = 0usize;
    let mut inner_err: Option<CoreError> = None;

    let sz_res = reader.for_each_entries(|entry, data_reader| {
        if cancel.load(Ordering::Relaxed) {
            inner_err = Some(CoreError::Cancelled);
            return Ok(false);
        }

        let rel_path = match sanitize_archive_rel_path(entry.name()) {
            Ok(p) => p,
            Err(e) => {
                inner_err = Some(e);
                return Ok(false);
            }
        };
        let rel_display = rel_path.to_string_lossy().into_owned();
        let out_path = tmp_dir.join(&rel_path);
        files_done += 1;

        if entry.is_directory() {
            if let Err(e) = std::fs::create_dir_all(&out_path) {
                inner_err = Some(CoreError::Io(e));
                return Ok(false);
            }
            tel.tick(Some(&rel_display), quota.written, files_done, false);
            return Ok(true);
        }

        if let Err(e) = quota.copy(data_reader, &out_path, &rel_display, files_done, &mut tel) {
            inner_err = Some(e);
            return Ok(false);
        }
        tel.tick(Some(&rel_display), quota.written, files_done, false);
        Ok(true)
    });

    if let Some(e) = inner_err {
        discard_staging(&tmp_dir, parent);
        return Err(e);
    }
    if let Err(e) = sz_res {
        discard_staging(&tmp_dir, parent);
        return Err(CoreError::Archive {
            message: format!("Error extrayendo 7Z: {}", e),
            source: Some(Box::new(e)),
        });
    }

    finalize_staging_dir(&tmp_dir, dest_dir, parent, zone.as_deref())?;
    tel.finish(quota.written, files_done);
    Ok(quota.written)
}

pub(super) fn compress_to_7z(
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

    sevenz_rust2::compress_to_path(src_path, dest_archive).map_err(|e| CoreError::Archive {
        message: format!("Error comprimiendo en 7Z: {}", e),
        source: Some(Box::new(e)),
    })?;
    let sz = dest_archive.metadata().map(|m| m.len()).unwrap_or(0);
    tel.finish(sz, total_files);
    Ok(sz)
}
