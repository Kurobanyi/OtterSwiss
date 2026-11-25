//! CL-18: ayudas para los tests unitarios del core.

use std::path::{Path, PathBuf};

/// Carpeta temporal única que se borra al soltarla, también si el test falla a mitad.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("otter_test_{tag}_{}_{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("crear carpeta temporal del test");
        Self(dir)
    }

    pub fn join(&self, rel: impl AsRef<Path>) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
