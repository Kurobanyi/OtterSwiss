use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::optimize::tweaks_catalog::{
    is_allowed_registry_entry, is_allowed_service_name, is_valid_service_start_type, RegHive,
};
use crate::core::platform::safe_fs::{atomic_write_new, delete_verified_file, read_bounded_bytes};
use crate::core::platform::win32::NamedMutex;

const BACKUP_SCHEMA_VERSION: u32 = 2;
const MAX_BACKUP_BYTES: u64 = 2 * 1024 * 1024;
const BACKUP_MUTEX_NAME: &str = r"Global\OtterSwiss_Backup_Lock";
/// REG_SZ, REG_EXPAND_SZ, REG_BINARY, REG_DWORD, REG_QWORD.
const ALLOWED_SNAPSHOT_TYPES: [u32; 5] = [1, 2, 3, 4, 11];
const MAX_SNAPSHOT_VALUE_BYTES: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySnapshot {
    pub hive: RegHive,
    pub subkey: String,
    pub value_name: String,
    pub existed: bool,
    pub key_created: bool,
    pub value_type: Option<u32>,
    pub raw_bytes: Option<Vec<u8>>,
    pub applied_bytes: Vec<u8>,
}

impl RegistrySnapshot {
    pub fn is_valid(&self) -> bool {
        if !is_allowed_registry_entry(self.hive, &self.subkey, &self.value_name) {
            return false;
        }
        // Los valores del catálogo son DWORD/QWORD/cadenas cortas: tipos y tamaños fuera de eso solo
        // pueden venir de un respaldo manipulado.
        if self.applied_bytes.len() > MAX_SNAPSHOT_VALUE_BYTES {
            return false;
        }
        match (self.existed, self.value_type, &self.raw_bytes) {
            (true, Some(t), Some(raw)) => {
                ALLOWED_SNAPSHOT_TYPES.contains(&t) && raw.len() <= MAX_SNAPSHOT_VALUE_BYTES
            }
            (false, None, None) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub service_name: String,
    pub previous_start_type: u32,
    pub previous_delayed_auto: bool,
    pub was_running: bool,
    pub applied_start_type: u32,
}

impl ServiceSnapshot {
    pub fn is_valid(&self) -> bool {
        is_allowed_service_name(&self.service_name)
            && is_valid_service_start_type(self.previous_start_type)
            && is_valid_service_start_type(self.applied_start_type)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSnapshot {
    pub previous_scheme_guid: String,
    pub created_scheme_guid: Option<String>,
    pub active_after_apply_guid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationBackup {
    pub version: u32,
    pub created_at_epoch: u64,
    pub last_updated_epoch: u64,
    pub last_profile_applied: String,
    pub registry: Vec<RegistrySnapshot>,
    pub services: Vec<ServiceSnapshot>,
    pub power_history: Vec<PowerSnapshot>,
}

impl OptimizationBackup {
    pub fn new(profile_name: &str) -> Self {
        let now = epoch_now();
        Self {
            version: BACKUP_SCHEMA_VERSION,
            created_at_epoch: now,
            last_updated_epoch: now,
            last_profile_applied: profile_name.to_string(),
            registry: Vec::new(),
            services: Vec::new(),
            power_history: Vec::new(),
        }
    }
}

pub struct BackupManager {
    file_path: PathBuf,
}

impl BackupManager {
    pub fn new(secure_data_dir: &Path) -> Self {
        Self {
            file_path: secure_data_dir.join("backup.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// Carga tolerante (Transversal 6): descarta entradas manipuladas o fuera de la tabla única
    /// sin bloquear toda la recuperación del respaldo.
    pub fn load_tolerant(&self) -> CoreResult<(Option<OptimizationBackup>, Vec<String>)> {
        let _lock = NamedMutex::acquire_blocking(BACKUP_MUTEX_NAME)?;
        if !self.file_path.exists() {
            return Ok((None, Vec::new()));
        }

        let bytes = read_bounded_bytes(&self.file_path, MAX_BACKUP_BYTES)?;
        let mut parsed: OptimizationBackup =
            serde_json::from_slice(&bytes).map_err(|e| CoreError::Backup {
                message: format!("Error deserializando backup.json: {}", e),
                source: Some(Box::new(e)),
            })?;

        let mut warnings = Vec::new();
        let before_reg = parsed.registry.len();
        parsed.registry.retain(|entry| entry.is_valid());
        if parsed.registry.len() < before_reg {
            warnings.push(format!(
                "Omitidas {} entradas de registro no autorizadas o incoherentes en backup.json",
                before_reg - parsed.registry.len()
            ));
        }

        let before_svc = parsed.services.len();
        parsed.services.retain(|entry| entry.is_valid());
        if parsed.services.len() < before_svc {
            warnings.push(format!(
                "Omitidas {} entradas de servicios no autorizadas en backup.json",
                before_svc - parsed.services.len()
            ));
        }

        Ok((Some(parsed), warnings))
    }

    /// Persiste un snapshot individual ANTES de que el motor aplique el cambio al sistema.
    /// Conserva siempre el estado original (`previous_*`) si la clave/servicio ya estaba registrado.
    pub fn record_registry_before_apply(
        &self,
        profile_name: &str,
        snap: RegistrySnapshot,
    ) -> CoreResult<()> {
        if !snap.is_valid() {
            return Err(CoreError::Validation(format!(
                "Snapshot de registro rechazado por lista blanca: {:?}\\{}\\{}",
                snap.hive, snap.subkey, snap.value_name
            )));
        }

        let _lock = NamedMutex::acquire_blocking(BACKUP_MUTEX_NAME)?;
        let mut doc = self.load_or_init_unlocked(profile_name)?;
        doc.last_updated_epoch = epoch_now();
        doc.last_profile_applied = profile_name.to_string();

        if let Some(existing) = doc.registry.iter_mut().find(|r| {
            r.hive == snap.hive
                && r.subkey.eq_ignore_ascii_case(&snap.subkey)
                && r.value_name.eq_ignore_ascii_case(&snap.value_name)
        }) {
            // Mantener el estado previo original; actualizar solo el último valor aplicado.
            existing.applied_bytes = snap.applied_bytes;
        } else {
            doc.registry.push(snap);
        }

        self.save_unlocked(&doc)
    }

    pub fn record_service_before_apply(
        &self,
        profile_name: &str,
        snap: ServiceSnapshot,
    ) -> CoreResult<()> {
        if !snap.is_valid() {
            return Err(CoreError::Validation(format!(
                "Snapshot de servicio rechazado por lista blanca: '{}'",
                snap.service_name
            )));
        }

        let _lock = NamedMutex::acquire_blocking(BACKUP_MUTEX_NAME)?;
        let mut doc = self.load_or_init_unlocked(profile_name)?;
        doc.last_updated_epoch = epoch_now();
        doc.last_profile_applied = profile_name.to_string();

        if let Some(existing) = doc
            .services
            .iter_mut()
            .find(|s| s.service_name.eq_ignore_ascii_case(&snap.service_name))
        {
            existing.applied_start_type = snap.applied_start_type;
        } else {
            doc.services.push(snap);
        }

        self.save_unlocked(&doc)
    }

    pub fn record_power_before_apply(
        &self,
        profile_name: &str,
        snap: PowerSnapshot,
    ) -> CoreResult<()> {
        let _lock = NamedMutex::acquire_blocking(BACKUP_MUTEX_NAME)?;
        let mut doc = self.load_or_init_unlocked(profile_name)?;
        doc.last_updated_epoch = epoch_now();
        doc.last_profile_applied = profile_name.to_string();
        doc.power_history.push(snap);
        self.save_unlocked(&doc)
    }

    pub fn delete_if_clean(&self) -> CoreResult<()> {
        let _lock = NamedMutex::acquire_blocking(BACKUP_MUTEX_NAME)?;
        if self.file_path.exists() {
            delete_verified_file(&self.file_path)?;
        }
        Ok(())
    }

    /// Carga el respaldo para añadirle entradas. Si el archivo existe pero no se puede leer o
    /// interpretar, falla: empezar uno vacío guardaría como «original» valores ya optimizados y el
    /// estado de fábrica se perdería para siempre.
    fn load_or_init_unlocked(&self, profile_name: &str) -> CoreResult<OptimizationBackup> {
        if !self.file_path.exists() {
            return Ok(OptimizationBackup::new(profile_name));
        }
        let bytes = read_bounded_bytes(&self.file_path, MAX_BACKUP_BYTES).map_err(|e| {
            CoreError::Backup {
                message: format!(
                    "No se pudo leer el respaldo existente ({}); no se aplicarán cambios para no perder el estado original",
                    e
                ),
                source: None,
            }
        })?;
        let mut parsed = serde_json::from_slice::<OptimizationBackup>(&bytes).map_err(|e| {
            CoreError::Backup {
                message: format!(
                    "El respaldo existente está dañado ({}); no se aplicarán cambios para no perder el estado original",
                    e
                ),
                source: Some(Box::new(e)),
            }
        })?;
        parsed.registry.retain(|r| r.is_valid());
        parsed.services.retain(|s| s.is_valid());
        Ok(parsed)
    }

    fn save_unlocked(&self, doc: &OptimizationBackup) -> CoreResult<()> {
        let data = serde_json::to_vec_pretty(doc).map_err(|e| CoreError::Backup {
            message: format!("Error serializando backup.json: {}", e),
            source: Some(Box::new(e)),
        })?;
        atomic_write_new(&self.file_path, &data)
    }
}

fn epoch_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::optimize::tweaks_catalog::REGISTRY_TWEAKS;

    fn snapshot() -> RegistrySnapshot {
        let spec = &REGISTRY_TWEAKS[0];
        RegistrySnapshot {
            hive: spec.hive,
            subkey: spec.subkey.to_string(),
            value_name: spec.value_name.to_string(),
            existed: true,
            key_created: false,
            value_type: Some(4),
            raw_bytes: Some(vec![0, 0, 0, 0]),
            applied_bytes: vec![1, 0, 0, 0],
        }
    }

    /// SEC-10: solo entradas del catálogo, con tipos y tamaños acotados.
    #[test]
    fn snapshot_valida_catalogo_tipo_y_tamano() {
        assert!(snapshot().is_valid());

        let mut otra_clave = snapshot();
        otra_clave.subkey = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run".into();
        assert!(!otra_clave.is_valid());

        let mut multi_sz = snapshot();
        multi_sz.value_type = Some(7);
        assert!(!multi_sz.is_valid());

        let mut enorme = snapshot();
        enorme.raw_bytes = Some(vec![0; 4096]);
        assert!(!enorme.is_valid());

        let mut incoherente = snapshot();
        incoherente.existed = false;
        assert!(!incoherente.is_valid());

        let mut no_existia = snapshot();
        no_existia.existed = false;
        no_existia.value_type = None;
        no_existia.raw_bytes = None;
        assert!(no_existia.is_valid());
    }
}
