//! AR-02: respaldos JSON en la carpeta de datos. Una sola implementación para el motor, el Debloater y DNS.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{atomic_write_new, read_bounded_bytes};

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Lee un respaldo. `Ok(None)` si no existe. Si existe y no se puede leer o interpretar, falla (patrón 6 de
/// BUGS.md: nunca se asume «vacío», porque se guardaría como original un estado ya cambiado). `consequence`
/// completa el mensaje: qué no se va a hacer por esa razón.
pub fn read_backup<T: DeserializeOwned>(
    path: &Path,
    max_bytes: u64,
    consequence: &str,
) -> CoreResult<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let name = file_label(path);
    let bytes = read_bounded_bytes(path, max_bytes).map_err(|e| CoreError::Backup {
        message: format!("No se pudo leer {name} ({e}); {consequence}"),
        source: Some(Box::new(e)),
    })?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| CoreError::Backup {
            message: format!("{name} está dañado ({e}); {consequence}"),
            source: Some(Box::new(e)),
        })
}

/// Escribe el respaldo de forma atómica (archivo nuevo y renombre).
pub fn write_backup<T: Serialize>(path: &Path, value: &T) -> CoreResult<()> {
    let json = serde_json::to_vec_pretty(value).map_err(|e| CoreError::Backup {
        message: format!("Error serializando {}: {e}", file_label(path)),
        source: Some(Box::new(e)),
    })?;
    atomic_write_new(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "otter-json-{tag}-{}-{nonce}.json",
            std::process::id()
        ))
    }

    #[test]
    fn ida_y_vuelta_y_archivo_ausente() {
        let path = temp_file("ok");
        assert!(read_backup::<Vec<u32>>(&path, 1024, "x").unwrap().is_none());
        write_backup(&path, &vec![1u32, 2, 3]).unwrap();
        assert_eq!(
            read_backup::<Vec<u32>>(&path, 1024, "x").unwrap(),
            Some(vec![1, 2, 3])
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Patrón 6: un respaldo dañado es un error, no un respaldo vacío.
    #[test]
    fn respaldo_danado_falla() {
        let path = temp_file("roto");
        std::fs::write(&path, b"{ esto no es json").unwrap();
        let err = read_backup::<Vec<u32>>(&path, 1024, "no se toca nada").unwrap_err();
        assert!(err.to_string().contains("dañado"));
        let _ = std::fs::remove_file(&path);
    }
}
