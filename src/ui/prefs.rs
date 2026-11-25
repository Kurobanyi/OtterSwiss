//! Preferencias de la interfaz (tema e idioma) en `%APPDATA%\OtterSwiss`, un archivo JSON por preferencia.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::core::platform::safe_fs::{read_bounded_bytes, write_file_inside};

const PREFS_DIR: &str = "OtterSwiss";
/// Una preferencia es un valor corto; más que esto es un archivo ajeno o dañado.
const MAX_PREF_BYTES: u64 = 4096;

/// Lee una preferencia guardada. `None` si no existe, es un reparse point, es demasiado grande o no se entiende.
pub fn load<T: DeserializeOwned>(file_name: &str) -> Option<T> {
    let path = dirs::config_dir()?.join(PREFS_DIR).join(file_name);
    let bytes = read_bounded_bytes(&path, MAX_PREF_BYTES).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Guarda una preferencia. Un fallo no es grave: la próxima vez se usa el valor por defecto.
pub fn save<T: Serialize>(file_name: &str, value: &T) {
    // El proceso corre elevado: se escribe sin seguir junctions (SEC-19).
    if let (Some(root), Ok(json)) = (dirs::config_dir(), serde_json::to_vec_pretty(value)) {
        let _ = write_file_inside(&root, PREFS_DIR, file_name, &json);
    }
}
