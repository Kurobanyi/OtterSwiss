//! Ajustes de registro del Debloater: respaldo en `debloat_backup.json`, aplicación y reversión.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use winreg::RegValue;
use winreg::enums::{KEY_READ, KEY_WRITE};

use super::{DEBLOAT_MUTEX, DEBLOAT_REG_TWEAKS, DebloatRegEntry, DebloaterManager};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::json_file::{read_backup, write_backup};
use crate::core::platform::mutex::NamedMutex;
use crate::core::platform::registry::{RegHive, RegLocation};
use crate::core::platform::registry::{reg_type_to_u32, u32_to_reg_type};
use crate::core::platform::safe_fs::delete_verified_file;
use crate::core::security::data_dir::ensure_admin_only_data_dir;

const MAX_DEBLOAT_BACKUP_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DebloatRegSnapshot {
    hive: RegHive,
    subkey: String,
    value_name: String,
    existed: bool,
    /// Formato antiguo (solo DWORD). Se conserva para leer respaldos de versiones previas.
    #[serde(default)]
    previous_dword: Option<u32>,
    /// Tipo y bytes exactos del valor original, sea cual sea su tipo.
    #[serde(default)]
    previous_type: Option<u32>,
    #[serde(default)]
    previous_raw: Option<Vec<u8>>,
}

impl DebloatRegSnapshot {
    /// La entrada solo es válida si corresponde a un ajuste del catálogo embebido. Cualquier otra clave
    /// en el respaldo solo puede venir de un archivo manipulado y nunca se escribe en el registro.
    fn catalog_entry(&self) -> Option<&'static DebloatRegEntry> {
        DEBLOAT_REG_TWEAKS
            .iter()
            .flat_map(|t| t.entries.iter())
            .find(|e| {
                e.hive == self.hive
                    && e.subkey.eq_ignore_ascii_case(&self.subkey)
                    && e.value_name.eq_ignore_ascii_case(&self.value_name)
            })
    }

    fn is_valid(&self) -> bool {
        if self.catalog_entry().is_none() {
            return false;
        }
        if !self.existed {
            return self.previous_raw.is_none();
        }
        match (&self.previous_raw, self.previous_type, self.previous_dword) {
            (Some(raw), Some(t), _) => raw.len() <= 1024 && [1, 2, 3, 4, 11].contains(&t),
            (None, None, Some(_)) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DebloatBackupDoc {
    version: u32,
    entries: Vec<DebloatRegSnapshot>,
}

pub(super) fn debloat_backup_path() -> CoreResult<PathBuf> {
    Ok(ensure_admin_only_data_dir()?.join("debloat_backup.json"))
}

impl DebloaterManager {
    /// Lee `debloat_backup.json`. Si existe y no se puede leer, falla (nunca se asume vacío); si trae
    /// entradas que no son del catálogo, falla: el archivo fue manipulado.
    pub(super) fn load_backup_doc(path: &Path) -> CoreResult<DebloatBackupDoc> {
        let doc: DebloatBackupDoc = read_backup(
            path,
            MAX_DEBLOAT_BACKUP_BYTES,
            "no se tocará el registro para no perder el estado original",
        )?
        .unwrap_or(DebloatBackupDoc {
            version: 2,
            entries: Vec::new(),
        });
        if let Some(bad) = doc.entries.iter().find(|e| !e.is_valid()) {
            return Err(CoreError::Backup {
                message: format!(
                    "debloat_backup.json contiene una entrada que no es del catálogo ({:?}\\{}\\{}); posible manipulación",
                    bad.hive, bad.subkey, bad.value_name
                ),
                source: None,
            });
        }
        Ok(doc)
    }

    pub(super) fn backup_reg_entries(entries: &[DebloatRegEntry]) -> CoreResult<()> {
        let backup_file = debloat_backup_path()?;
        let mut doc = Self::load_backup_doc(&backup_file)?;
        doc.version = 2;

        for entry in entries {
            let already = doc.entries.iter().any(|s| {
                s.hive == entry.hive
                    && s.subkey.eq_ignore_ascii_case(entry.subkey)
                    && s.value_name.eq_ignore_ascii_case(entry.value_name)
            });
            if already {
                continue;
            }

            let root = RegLocation::native(entry.hive).root();
            let flags = RegLocation::native(entry.hive).flags(KEY_READ);

            // Distinguir «no existe» de un error real: un error no puede guardarse como «no existía».
            let prev = match root.open_subkey_with_flags(entry.subkey, flags) {
                Ok(key) => match key.get_raw_value(entry.value_name) {
                    Ok(raw) => Some(raw),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => {
                        return Err(CoreError::Registry {
                            path: format!("{}\\{}", entry.subkey, entry.value_name),
                            message: format!("No se pudo leer el valor original: {}", e),
                            source: Some(e),
                        });
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    return Err(CoreError::Registry {
                        path: entry.subkey.to_string(),
                        message: format!("No se pudo abrir la clave original: {}", e),
                        source: Some(e),
                    });
                }
            };

            doc.entries.push(DebloatRegSnapshot {
                hive: entry.hive,
                subkey: entry.subkey.to_string(),
                value_name: entry.value_name.to_string(),
                existed: prev.is_some(),
                previous_dword: None,
                previous_type: prev.as_ref().map(|r| reg_type_to_u32(&r.vtype)),
                previous_raw: prev.map(|r| r.bytes),
            });
        }

        write_backup(&backup_file, &doc)
    }

    pub(super) fn apply_reg_entries(entries: &[DebloatRegEntry]) -> CoreResult<()> {
        for entry in entries {
            let root = RegLocation::native(entry.hive).root();
            let flags = RegLocation::native(entry.hive).flags(KEY_WRITE);
            let (key, _) = root
                .create_subkey_with_flags(entry.subkey, flags)
                .map_err(|e| CoreError::Registry {
                    path: entry.subkey.to_string(),
                    message: format!("Error creando clave debloat: {}", e),
                    source: Some(e),
                })?;
            key.set_value(entry.value_name, &entry.target_dword)?;
        }
        Ok(())
    }

    /// `true` si hay ajustes de registro del Debloater pendientes de revertir.
    pub fn has_registry_backup() -> bool {
        debloat_backup_path().map(|p| p.exists()).unwrap_or(false)
    }

    /// Revierte los ajustes de registro del Debloater. Solo toca claves del catálogo, propaga cada error
    /// y conserva el respaldo si algo no se pudo revertir, para poder reintentar.
    pub fn revert_registry_debloat() -> CoreResult<usize> {
        ElevationManager::require_elevated("Revertir ajustes de registro del Debloater")?;
        let _lock = NamedMutex::acquire(DEBLOAT_MUTEX)?;
        let backup_file = debloat_backup_path()?;
        if !backup_file.exists() {
            return Err(CoreError::Backup {
                message: "No existe respaldo de ajustes del Debloater".into(),
                source: None,
            });
        }

        let doc = Self::load_backup_doc(&backup_file)?;

        let mut restored = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for snap in doc.entries.iter().rev() {
            match Self::restore_snapshot(snap) {
                Ok(()) => restored += 1,
                Err(e) => failures.push(format!("{}\\{}: {}", snap.subkey, snap.value_name, e)),
            }
        }

        if !failures.is_empty() {
            return Err(CoreError::Backup {
                message: format!(
                    "Se revirtieron {} ajustes, pero {} fallaron y el respaldo se conserva: {}",
                    restored,
                    failures.len(),
                    failures.join("; ")
                ),
                source: None,
            });
        }

        delete_verified_file(&backup_file)?;
        Ok(restored)
    }

    pub(super) fn restore_snapshot(snap: &DebloatRegSnapshot) -> CoreResult<()> {
        let entry = snap.catalog_entry().ok_or_else(|| {
            CoreError::Validation("Entrada fuera del catálogo del Debloater".into())
        })?;
        let root = RegLocation::native(entry.hive).root();
        let flags = RegLocation::native(entry.hive).flags(KEY_READ | KEY_WRITE);

        if snap.existed {
            let (key, _) = root.create_subkey_with_flags(entry.subkey, flags)?;
            match (&snap.previous_raw, snap.previous_type, snap.previous_dword) {
                (Some(raw), Some(t), _) => key.set_raw_value(
                    entry.value_name,
                    &RegValue {
                        bytes: raw.clone(),
                        vtype: u32_to_reg_type(t),
                    },
                )?,
                (None, None, Some(dword)) => key.set_value(entry.value_name, &dword)?,
                _ => {
                    return Err(CoreError::Validation(
                        "Snapshot inconsistente: existía sin valor guardado".into(),
                    ));
                }
            }
            Ok(())
        } else {
            match root.open_subkey_with_flags(entry.subkey, flags) {
                Ok(key) => match key.delete_value(entry.value_name) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.into()),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> DebloatRegSnapshot {
        let entry = &DEBLOAT_REG_TWEAKS[0].entries[0];
        DebloatRegSnapshot {
            hive: entry.hive,
            subkey: entry.subkey.to_string(),
            value_name: entry.value_name.to_string(),
            existed: true,
            previous_dword: None,
            previous_type: Some(4),
            previous_raw: Some(vec![1, 0, 0, 0]),
        }
    }

    /// SEC-03 / SYS-11: solo se revierte lo del catálogo, con el tipo exacto o el formato viejo.
    #[test]
    fn snapshot_del_debloater_solo_del_catalogo() {
        assert!(snapshot().is_valid());

        let mut formato_viejo = snapshot();
        formato_viejo.previous_type = None;
        formato_viejo.previous_raw = None;
        formato_viejo.previous_dword = Some(1);
        assert!(formato_viejo.is_valid());

        let mut manipulado = snapshot();
        manipulado.subkey = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run".into();
        assert!(!manipulado.is_valid());

        let mut tipo_raro = snapshot();
        tipo_raro.previous_type = Some(7);
        assert!(!tipo_raro.is_valid());

        let mut no_existia_con_bytes = snapshot();
        no_existia_con_bytes.existed = false;
        assert!(!no_existia_con_bytes.is_valid());
    }
}
