//! Formato ZIP.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::ArchiveManager;
use super::paths::sanitize_archive_rel_path;
use super::progress::{ArchiveProgress, QuotaCopy, TelemetryCtx};
use super::staging::{
    create_staging_dir, discard_staging, finalize_staging_dir, read_zone_identifier,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{create_verified_new_file, open_verified_file_read};

impl ArchiveManager {
    pub(super) fn extract_zip(
        archive_path: &Path,
        dest_dir: &Path,
        max_quota_bytes: u64,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        let zone = read_zone_identifier(archive_path);
        let file = File::open(archive_path)?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| CoreError::Archive {
            message: format!("Error abriendo ZIP: {}", e),
            source: None,
        })?;

        let total_files = archive.len();
        let total_uncompressed: u64 = (0..total_files)
            .filter_map(|i| archive.by_index_raw(i).ok().map(|e| e.size()))
            .sum();

        let mut tel = TelemetryCtx::new(progress);
        tel.init(total_files, total_uncompressed);

        let (parent, tmp_dir) = create_staging_dir(dest_dir)?;
        let mut quota = QuotaCopy::new(max_quota_bytes, cancel);

        let extract_res = (|| -> CoreResult<()> {
            for i in 0..total_files {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CoreError::Cancelled);
                }

                let mut entry = archive.by_index(i).map_err(|e| CoreError::Archive {
                    message: format!("Error leyendo entrada ZIP #{}: {}", i, e),
                    source: None,
                })?;

                if entry
                    .unix_mode()
                    .is_some_and(|m| (m & 0o170000) == 0o120000)
                {
                    return Err(CoreError::Validation(format!(
                        "Symlink rechazado en archivo ZIP: {}",
                        entry.name()
                    )));
                }

                if entry.enclosed_name().is_none() {
                    return Err(CoreError::Validation(format!(
                        "Ruta insegura en archivo ZIP: {}",
                        entry.name()
                    )));
                }
                // Mismo saneo que 7z y tar: rechaza también ':' (flujos alternos NTFS) y rutas profundas.
                let raw_path = sanitize_archive_rel_path(entry.name())?;
                let rel_display = raw_path.to_string_lossy().into_owned();
                let out_path = tmp_dir.join(&raw_path);

                if entry.is_dir() {
                    std::fs::create_dir_all(&out_path)?;
                } else {
                    quota.copy(&mut entry, &out_path, &rel_display, i + 1, &mut tel)?;
                }
                tel.tick(Some(&rel_display), quota.written, i + 1, false);
            }
            Ok(())
        })();

        if let Err(e) = extract_res {
            discard_staging(&tmp_dir, parent);
            return Err(e);
        }

        finalize_staging_dir(&tmp_dir, dest_dir, parent, zone.as_deref())?;
        tel.finish(quota.written, total_files);
        Ok(quota.written)
    }

    pub(super) fn compress_to_zip(
        src_path: &Path,
        dest_archive: &Path,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        let (total_files, total_bytes_est) = Self::count_source_stats(src_path);
        let mut tel = TelemetryCtx::new(progress);
        tel.init(total_files, total_bytes_est);

        let (file, _) = create_verified_new_file(dest_archive, true)?;
        let mut zip_writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().large_file(true);

        let mut total_bytes = 0u64;
        let mut files_done = 0usize;
        let mut stack = vec![src_path.to_path_buf()];

        while let Some(current) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return Err(CoreError::Cancelled);
            }

            if current.is_file() {
                let rel = current
                    .strip_prefix(src_path.parent().unwrap_or(src_path))
                    .unwrap_or(&current);
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                zip_writer
                    .start_file(&rel_str, options)
                    .map_err(|e| CoreError::Archive {
                        message: format!("Error iniciando archivo ZIP: {}", e),
                        source: None,
                    })?;
                let (mut f, _) = open_verified_file_read(&current, false, None)?;
                let mut buf = [0u8; 64 * 1024];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(CoreError::Cancelled);
                    }
                    let n = f.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    zip_writer.write_all(&buf[..n])?;
                    total_bytes += n as u64;
                    tel.tick(Some(&rel_str), total_bytes, files_done, false);
                }
                files_done += 1;
                tel.tick(Some(&rel_str), total_bytes, files_done, false);
            } else if current.is_dir() {
                for entry in std::fs::read_dir(&current)? {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(CoreError::Cancelled);
                    }
                    let entry = entry?;
                    let p = entry.path();
                    let ft = entry.file_type()?;
                    if ft.is_symlink() {
                        continue;
                    }
                    if ft.is_dir() {
                        let rel = p.strip_prefix(src_path).unwrap_or(&p);
                        let rel_str = format!("{}/", rel.to_string_lossy().replace('\\', "/"));
                        let _ = zip_writer.add_directory(&rel_str, options);
                        stack.push(p);
                    } else if ft.is_file() {
                        let rel = p.strip_prefix(src_path).unwrap_or(&p);
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        zip_writer.start_file(&rel_str, options).map_err(|e| {
                            CoreError::Archive {
                                message: format!("Error escribiendo en ZIP: {}", e),
                                source: None,
                            }
                        })?;
                        let (mut f, _) = open_verified_file_read(&p, false, None)?;
                        let mut buf = [0u8; 64 * 1024];
                        loop {
                            if cancel.load(Ordering::Relaxed) {
                                return Err(CoreError::Cancelled);
                            }
                            let n = f.read(&mut buf)?;
                            if n == 0 {
                                break;
                            }
                            zip_writer.write_all(&buf[..n])?;
                            total_bytes += n as u64;
                            tel.tick(Some(&rel_str), total_bytes, files_done, false);
                        }
                        files_done += 1;
                        tel.tick(Some(&rel_str), total_bytes, files_done, false);
                    }
                }
            }
        }

        zip_writer.finish().map_err(|e| CoreError::Archive {
            message: format!("Error finalizando ZIP: {}", e),
            source: None,
        })?;

        tel.finish(total_bytes, files_done);
        Ok(total_bytes)
    }
}
