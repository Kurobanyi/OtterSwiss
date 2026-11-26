//! Progreso de una operación (archivos, bytes, velocidad, ETA) y copia con tope de bytes.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::create_verified_new_file;

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
        } else if let Some(pct) = (self.files_done * 100).checked_div(self.total_files) {
            self.percent = pct.min(99) as u8;
        }
    }
}

pub(super) struct TelemetryCtx<'a> {
    progress: Option<&'a Arc<Mutex<ArchiveProgress>>>,
    started: Instant,
    last_emit: Instant,
}

impl<'a> TelemetryCtx<'a> {
    pub(super) fn new(progress: Option<&'a Arc<Mutex<ArchiveProgress>>>) -> Self {
        Self {
            progress,
            started: Instant::now(),
            last_emit: Instant::now() - Duration::from_secs(1),
        }
    }

    pub(super) fn init(&self, total_files: usize, total_bytes: u64) {
        if let Some(p_arc) = self.progress
            && let Ok(mut p) = p_arc.lock()
        {
            p.total_files = total_files;
            p.total_bytes = total_bytes;
            p.files_done = 0;
            p.bytes_processed = 0;
            p.percent = 1;
            p.finished = false;
        }
    }

    pub(super) fn tick(
        &mut self,
        current_file: Option<&str>,
        bytes_processed: u64,
        files_done: usize,
        force: bool,
    ) {
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

    pub(super) fn finish(&self, total_bytes: u64, files_done: usize) {
        if let Some(p_arc) = self.progress
            && let Ok(mut p) = p_arc.lock()
        {
            p.bytes_processed = total_bytes;
            p.files_done = files_done;
            p.percent = 100;
            p.eta_seconds = Some(0);
            p.finished = true;
        }
    }
}

/// CL-14: estado de una extracción que se reparte entre todas sus entradas: bytes escritos, tope y cancelación.
pub(super) struct QuotaCopy<'a> {
    pub(super) written: u64,
    quota: u64,
    cancel: &'a AtomicBool,
}

impl<'a> QuotaCopy<'a> {
    pub(super) fn new(quota: u64, cancel: &'a AtomicBool) -> Self {
        Self {
            written: 0,
            quota,
            cancel,
        }
    }

    /// Copia una entrada a `out_path` (archivo nuevo, sin seguir reparse points) sumando al total y fallando si pasa
    /// el tope (bomba de descompresión) o si se cancela.
    pub(super) fn copy<R: Read + ?Sized>(
        &mut self,
        reader: &mut R,
        out_path: &Path,
        rel_name: &str,
        files_done: usize,
        tel: &mut TelemetryCtx<'_>,
    ) -> CoreResult<()> {
        if let Some(p) = out_path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let (mut out_file, _) = create_verified_new_file(out_path, true)?;
        let mut chunk = [0u8; 64 * 1024];

        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(CoreError::Cancelled);
            }
            let read_len = reader.read(&mut chunk)?;
            if read_len == 0 {
                break;
            }
            self.written = self.written.saturating_add(read_len as u64);
            if self.written > self.quota {
                return Err(CoreError::archive_msg(format!(
                    "Límite de cuota ({} bytes) excedido",
                    self.quota
                )));
            }
            out_file.write_all(&chunk[..read_len])?;
            tel.tick(Some(rel_name), self.written, files_done, false);
        }

        out_file.sync_all()?;
        Ok(())
    }
}
