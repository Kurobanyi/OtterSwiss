use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sevenz_rust::{Password, SevenZReader};
use tar::{Archive as TarArchive, Builder as TarBuilder, EntryType};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::core::error::{CoreError, CoreResult};
use crate::core::safe_fs::{
    canonical_dos_path, create_verified_new_file, delete_tree_verified, is_safe_non_root_dir,
    write_zone_identifier,
};

const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5",
    "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
    "LPT8", "LPT9",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArchiveProgress {
    pub current_file: String,
    pub files_done: usize,
    pub total_files: usize,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub percent: u8,
    pub speed_mbps: f64,
    pub eta_seconds: Option<u64>,
    pub finished: bool,
}

impl ArchiveProgress {
    fn update_telemetry(&mut self, started: Instant) {
        let elapsed = started.elapsed().as_secs_f64();
        if elapsed > 0.15 {
            self.speed_mbps = (self.bytes_processed as f64 / (1024.0 * 1024.0)) / elapsed;
        }
        if self.total_bytes > 0 && self.bytes_processed > 0 && self.speed_mbps > 0.01 {
            let remaining_mb =
                self.total_bytes.saturating_sub(self.bytes_processed) as f64 / (1024.0 * 1024.0);
            self.eta_seconds = Some((remaining_mb / self.speed_mbps).ceil() as u64);
            self.percent = ((self.bytes_processed * 100) / self.total_bytes).min(99) as u8;
        } else if self.total_files > 0 {
            self.percent = ((self.files_done * 100) / self.total_files).min(99) as u8;
        }
    }
}

pub fn is_reserved_windows_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim();
    RESERVED_NAMES.iter().any(|&r| stem.eq_ignore_ascii_case(r))
}

pub fn validate_path_components(p: &Path) -> CoreResult<()> {
    for comp in p.components() {
        match comp {
            Component::Normal(os_name) => {
                let s = os_name.to_string_lossy();
                if s.contains('\0') || s.ends_with(' ') || s.ends_with('.') {
                    return Err(CoreError::Validation(format!(
                        "Nombre de archivo inválido para Windows: '{}'",
                        s
                    )));
                }
                if is_reserved_windows_name(&s) {
                    return Err(CoreError::Validation(format!(
                        "Nombre reservado de Windows detectado: '{}'",
                        s
                    )));
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::Validation(format!(
                    "Ruta insegura o fuera de raíz detectada: '{}'",
                    p.display()
                )));
            }
        }
    }
    Ok(())
}

fn sanitize_archive_rel_path(raw: &str) -> CoreResult<PathBuf> {
    if raw.contains('\0') {
        return Err(CoreError::Validation("Ruta con byte NUL en archivo".into()));
    }
    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/') || normalized.starts_with("//") {
        return Err(CoreError::Validation(format!(
            "Ruta absoluta rechazada en archivo: '{}'",
            raw
        )));
    }

    let mut clean = PathBuf::new();
    for seg in normalized.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." || seg.contains(':') {
            return Err(CoreError::Validation(format!(
                "Traversal o prefijo de unidad rechazado: '{}'",
                raw
            )));
        }
        if is_reserved_windows_name(seg) || seg.ends_with(' ') || seg.ends_with('.') {
            return Err(CoreError::Validation(format!(
                "Componente inválido o reservado en archivo: '{}'",
                seg
            )));
        }
        clean.push(seg);
    }

    if clean.as_os_str().is_empty() {
        return Err(CoreError::Validation("Entrada de archivo con ruta vacía".into()));
    }
    validate_path_components(&clean)?;
    Ok(clean)
}

fn validate_cli_paths(src: &Path, dst: &Path) -> CoreResult<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        return Err(CoreError::Validation("Origen symlink rechazado".into()));
    }
    let _ = canonical_dos_path(src)?;
    if dst.is_dir() && !is_safe_non_root_dir(dst, 1) {
        return Err(CoreError::Validation(
            "No se permite escribir directamente en raíz de unidad".into(),
        ));
    }
    Ok(())
}

fn create_staging_dir(dest_dir: &Path) -> CoreResult<(&Path, PathBuf)> {
    let parent = dest_dir
        .parent()
        .ok_or_else(|| CoreError::Validation("Directorio destino sin padre".into()))?;
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_dir = parent.join(format!(".extract.{}.{}.tmp", pid, nonce));
    std::fs::create_dir_all(&tmp_dir)?;
    Ok((parent, tmp_dir))
}

fn finalize_staging_dir(tmp_dir: &Path, dest_dir: &Path, parent: &Path) -> CoreResult<()> {
    if dest_dir.exists() {
        delete_tree_verified(dest_dir, parent, 16)?;
    }
    std::fs::rename(tmp_dir, dest_dir)?;
    let _ = write_zone_identifier(dest_dir, "archive://extracted");
    Ok(())
}

struct TelemetryCtx<'a> {
    progress: Option<&'a Arc<Mutex<ArchiveProgress>>>,
    started: Instant,
    last_emit: Instant,
}

impl<'a> TelemetryCtx<'a> {
    fn new(progress: Option<&'a Arc<Mutex<ArchiveProgress>>>) -> Self {
        Self {
            progress,
            started: Instant::now(),
            last_emit: Instant::now() - Duration::from_secs(1),
        }
    }

    fn init(&self, total_files: usize, total_bytes: u64) {
        if let Some(p_arc) = self.progress {
            if let Ok(mut p) = p_arc.lock() {
                p.total_files = total_files;
                p.total_bytes = total_bytes;
                p.files_done = 0;
                p.bytes_processed = 0;
                p.percent = 1;
                p.finished = false;
            }
        }
    }

    fn tick(&mut self, current_file: Option<&str>, bytes_processed: u64, files_done: usize, force: bool) {
        let Some(p_arc) = self.progress else {
            return;
        };
        if !force && self.last_emit.elapsed() < Duration::from_millis(120) {
            return;
        }
        self.last_emit = Instant::now();
        if let Ok(mut p) = p_arc.lock() {
            if let Some(cf) = current_file {
                p.current_file = cf.to_string();
            }
            p.bytes_processed = bytes_processed;
            p.files_done = files_done;
            p.update_telemetry(self.started);
        }
    }

    fn finish(&self, total_bytes: u64, files_done: usize) {
        if let Some(p_arc) = self.progress {
            if let Ok(mut p) = p_arc.lock() {
                p.bytes_processed = total_bytes;
                p.files_done = files_done;
                p.percent = 100;
                p.eta_seconds = Some(0);
                p.finished = true;
            }
        }
    }
}

fn copy_with_quota<R: Read + ?Sized>(
    reader: &mut R,
    out_path: &Path,
    rel_name: &str,
    bytes_written: &mut u64,
    files_done: usize,
    max_quota_bytes: u64,
    cancel: &AtomicBool,
    tel: &mut TelemetryCtx<'_>,
) -> CoreResult<()> {
    if let Some(p) = out_path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let (mut out_file, _) = create_verified_new_file(out_path, true)?;
    let mut chunk = [0u8; 64 * 1024];

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        let read_len = reader.read(&mut chunk)?;
        if read_len == 0 {
            break;
        }
        *bytes_written = bytes_written.saturating_add(read_len as u64);
        if *bytes_written > max_quota_bytes {
            return Err(CoreError::Archive {
                message: format!("Límite de cuota ({} bytes) excedido", max_quota_bytes),
                source: None,
            });
        }
        out_file.write_all(&chunk[..read_len])?;
        tel.tick(Some(rel_name), *bytes_written, files_done, false);
    }

    out_file.sync_all()?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZip,
    TarGz,
}

pub struct ArchiveManager;

impl ArchiveManager {
    pub fn extract_archive(
        archive_path: &Path,
        dest_dir: &Path,
        max_quota_bytes: u64,
        cancel: &AtomicBool,
    ) -> CoreResult<u64> {
        Self::extract_archive_with_progress(archive_path, dest_dir, max_quota_bytes, cancel, None)
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
            Self::extract_zip(archive_path, dest_dir, max_quota_bytes, cancel, progress)
        } else if lower.ends_with(".7z") {
            Self::extract_7z(archive_path, dest_dir, max_quota_bytes, cancel, progress)
        } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            Self::extract_tar_gz(archive_path, dest_dir, max_quota_bytes, cancel, progress)
        } else if lower.ends_with(".tar") {
            Self::extract_tar(archive_path, dest_dir, max_quota_bytes, cancel, progress)
        } else {
            Err(CoreError::Validation(format!(
                "Formato no soportado (ZIP, 7Z, TAR/TAR.GZ): '{}'",
                archive_path.display()
            )))
        }
    }

    pub fn extract_zip(
        archive_path: &Path,
        dest_dir: &Path,
        max_quota_bytes: u64,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
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
        let mut bytes_written = 0u64;

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

                let raw_path = match entry.enclosed_name() {
                    Some(p) => p.to_path_buf(),
                    None => {
                        return Err(CoreError::Validation(format!(
                            "Ruta insegura en archivo ZIP: {}",
                            entry.name()
                        )));
                    }
                };

                validate_path_components(&raw_path)?;
                let rel_display = raw_path.to_string_lossy().into_owned();
                let out_path = tmp_dir.join(&raw_path);

                if entry.is_dir() {
                    std::fs::create_dir_all(&out_path)?;
                } else {
                    copy_with_quota(
                        &mut entry,
                        &out_path,
                        &rel_display,
                        &mut bytes_written,
                        i + 1,
                        max_quota_bytes,
                        cancel,
                        &mut tel,
                    )?;
                }
                tel.tick(Some(&rel_display), bytes_written, i + 1, false);
            }
            Ok(())
        })();

        if let Err(e) = extract_res {
            let _ = delete_tree_verified(&tmp_dir, parent, 16);
            return Err(e);
        }

        finalize_staging_dir(&tmp_dir, dest_dir, parent)?;
        tel.finish(bytes_written, total_files);
        Ok(bytes_written)
    }

    pub fn extract_7z(
        archive_path: &Path,
        dest_dir: &Path,
        max_quota_bytes: u64,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        let file = File::open(archive_path)?;
        let len = file.metadata()?.len();
        let mut reader = SevenZReader::new(file, len, Password::empty()).map_err(|e| {
            CoreError::Archive {
                message: format!("Error abriendo archivo 7Z: {}", e),
                source: None,
            }
        })?;

        let total_files = reader.archive().files.len();
        let total_uncompressed: u64 = reader.archive().files.iter().map(|f| f.size).sum();

        let mut tel = TelemetryCtx::new(progress);
        tel.init(total_files, total_uncompressed);

        let (parent, tmp_dir) = create_staging_dir(dest_dir)?;
        let mut bytes_written = 0u64;
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
                tel.tick(Some(&rel_display), bytes_written, files_done, false);
                return Ok(true);
            }

            if let Err(e) = copy_with_quota(
                data_reader,
                &out_path,
                &rel_display,
                &mut bytes_written,
                files_done,
                max_quota_bytes,
                cancel,
                &mut tel,
            ) {
                inner_err = Some(e);
                return Ok(false);
            }
            tel.tick(Some(&rel_display), bytes_written, files_done, false);
            Ok(true)
        });

        if let Some(e) = inner_err {
            let _ = delete_tree_verified(&tmp_dir, parent, 16);
            return Err(e);
        }
        if let Err(e) = sz_res {
            let _ = delete_tree_verified(&tmp_dir, parent, 16);
            return Err(CoreError::Archive {
                message: format!("Error extrayendo 7Z: {}", e),
                source: None,
            });
        }

        finalize_staging_dir(&tmp_dir, dest_dir, parent)?;
        tel.finish(bytes_written, files_done);
        Ok(bytes_written)
    }

    pub fn extract_tar(
        archive_path: &Path,
        dest_dir: &Path,
        max_quota_bytes: u64,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        let file = File::open(archive_path)?;
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        Self::extract_tar_reader(file, len, dest_dir, max_quota_bytes, cancel, progress)
    }

    pub fn extract_tar_gz(
        archive_path: &Path,
        dest_dir: &Path,
        max_quota_bytes: u64,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        let file = File::open(archive_path)?;
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let gz = GzDecoder::new(file);
        Self::extract_tar_reader(gz, len, dest_dir, max_quota_bytes, cancel, progress)
    }

    fn extract_tar_reader<R: Read>(
        reader: R,
        approx_bytes: u64,
        dest_dir: &Path,
        max_quota_bytes: u64,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        let mut tar = TarArchive::new(reader);
        let mut tel = TelemetryCtx::new(progress);
        tel.init(0, approx_bytes);

        let (parent, tmp_dir) = create_staging_dir(dest_dir)?;
        let mut bytes_written = 0u64;
        let mut files_done = 0usize;

        let res = (|| -> CoreResult<()> {
            let entries = tar.entries().map_err(|e| CoreError::Archive {
                message: format!("Error leyendo entradas TAR: {}", e),
                source: None,
            })?;

            for entry_res in entries {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CoreError::Cancelled);
                }
                let mut entry = entry_res.map_err(|e| CoreError::Archive {
                    message: format!("Entrada TAR corrupta: {}", e),
                    source: None,
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
                    source: None,
                })?;
                let rel = sanitize_archive_rel_path(&raw_path.to_string_lossy())?;
                let rel_display = rel.to_string_lossy().into_owned();
                let out_path = tmp_dir.join(&rel);
                files_done += 1;

                if etype.is_dir() {
                    std::fs::create_dir_all(&out_path)?;
                } else if etype.is_file() {
                    copy_with_quota(
                        &mut entry,
                        &out_path,
                        &rel_display,
                        &mut bytes_written,
                        files_done,
                        max_quota_bytes,
                        cancel,
                        &mut tel,
                    )?;
                }
                tel.tick(Some(&rel_display), bytes_written, files_done, false);
            }
            Ok(())
        })();

        if let Err(e) = res {
            let _ = delete_tree_verified(&tmp_dir, parent, 16);
            return Err(e);
        }

        finalize_staging_dir(&tmp_dir, dest_dir, parent)?;
        tel.finish(bytes_written, files_done);
        Ok(bytes_written)
    }

    pub fn compress_path(
        src_path: &Path,
        dest_archive: &Path,
        format: ArchiveFormat,
        cancel: &AtomicBool,
    ) -> CoreResult<u64> {
        Self::compress_path_with_progress(src_path, dest_archive, format, cancel, None)
    }

    pub fn compress_path_with_progress(
        src_path: &Path,
        dest_archive: &Path,
        format: ArchiveFormat,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        validate_cli_paths(src_path, dest_archive)?;

        match format {
            ArchiveFormat::Zip => Self::compress_to_zip(src_path, dest_archive, cancel, progress),
            ArchiveFormat::SevenZip => {
                Self::compress_to_7z(src_path, dest_archive, cancel, progress)
            }
            ArchiveFormat::TarGz => {
                Self::compress_to_tar_gz(src_path, dest_archive, cancel, progress)
            }
        }
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
                            bytes = bytes.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
                        }
                    }
                }
            }
        }
        (count, bytes)
    }

    pub fn compress_to_zip(
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
                zip_writer.start_file(&rel_str, options).map_err(|e| {
                    CoreError::Archive {
                        message: format!("Error iniciando archivo ZIP: {}", e),
                        source: None,
                    }
                })?;
                let mut f = File::open(&current)?;
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
                        let mut f = File::open(&p)?;
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

    pub fn compress_to_7z(
        src_path: &Path,
        dest_archive: &Path,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        let (total_files, total_bytes_est) = Self::count_source_stats(src_path);
        let mut tel = TelemetryCtx::new(progress);
        tel.init(total_files, total_bytes_est);
        tel.tick(Some(&src_path.to_string_lossy()), 0, 0, true);

        sevenz_rust::compress_to_path(src_path, dest_archive).map_err(|e| CoreError::Archive {
            message: format!("Error comprimiendo con sevenz-rust (7Z): {}", e),
            source: None,
        })?;
        let sz = dest_archive.metadata().map(|m| m.len()).unwrap_or(0);
        tel.finish(sz, total_files);
        Ok(sz)
    }

    pub fn compress_to_tar_gz(
        src_path: &Path,
        dest_archive: &Path,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<ArchiveProgress>>>,
    ) -> CoreResult<u64> {
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        let (total_files, total_bytes_est) = Self::count_source_stats(src_path);
        let mut tel = TelemetryCtx::new(progress);
        tel.init(total_files, total_bytes_est);
        tel.tick(Some(&src_path.to_string_lossy()), 0, 0, true);

        let (file, _) = create_verified_new_file(dest_archive, true)?;
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = TarBuilder::new(enc);

        if src_path.is_file() {
            let mut f = File::open(src_path)?;
            let name = src_path
                .file_name()
                .ok_or_else(|| CoreError::Validation("Nombre de archivo inválido".into()))?;
            tar.append_file(name, &mut f).map_err(|e| CoreError::Archive {
                message: format!("Error agregando archivo a TAR: {}", e),
                source: None,
            })?;
        } else if src_path.is_dir() {
            tar.append_dir_all(".", src_path).map_err(|e| CoreError::Archive {
                message: format!("Error empaquetando directorio TAR: {}", e),
                source: None,
            })?;
        }

        tar.finish().map_err(|e| CoreError::Archive {
            message: format!("Error finalizando TAR.GZ: {}", e),
            source: None,
        })?;

        let sz = dest_archive.metadata().map(|m| m.len()).unwrap_or(0);
        tel.finish(sz, total_files);
        Ok(sz)
    }
}