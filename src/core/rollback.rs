//! Revertir todo lo que OtterSwiss cambió en el sistema (lo que hace `--rollback` al desinstalar la app).

use crate::core::debloat::DebloaterManager;
use crate::core::dns::DnsManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::optimize::engine::{OptimizationEngine, RollbackReport};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::safe_fs::delete_tree_verified;
use crate::core::security::data_dir::ensure_admin_only_data_dir;

/// Resultado de `revert_all_changes`.
#[derive(Debug, Default)]
pub struct FullRevertReport {
    pub engine: Option<RollbackReport>,
    pub dns_restored: usize,
    pub debloat_restored: usize,
    pub failures: Vec<String>,
    pub data_dir_deleted: bool,
}

impl FullRevertReport {
    /// `true` si no hubo fallos ni ítems omitidos por cambios del usuario.
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
            && self
                .engine
                .as_ref()
                .is_none_or(|e| e.skipped_user_modified == 0)
    }
}

/// Revierte todo lo que OtterSwiss cambió en el sistema (optimización, DNS y ajustes del Debloater).
/// Lo usa `--rollback`, que llama el desinstalador. La carpeta de datos solo se borra si todo se
/// revirtió y no queda ningún respaldo pendiente; si algo falla, los respaldos se conservan.
pub fn revert_all_changes() -> CoreResult<FullRevertReport> {
    ElevationManager::require_elevated("Revertir todos los cambios de OtterSwiss")?;
    let data_dir = ensure_admin_only_data_dir()?;
    let mut report = FullRevertReport::default();

    let engine = OptimizationEngine::new(&data_dir);
    if engine.has_backup() {
        match engine.rollback(false) {
            Ok(rb) => {
                report.failures.extend(
                    rb.failed_items
                        .iter()
                        .map(|f| format!("[Optimización] {}", f)),
                );
                report.engine = Some(rb);
            }
            Err(e) => report.failures.push(format!("[Optimización] {}", e)),
        }
    }

    let dns = DnsManager::new(&data_dir);
    if dns.has_backup() {
        match dns.restore_dns() {
            Ok(n) => report.dns_restored = n,
            Err(e) => report.failures.push(format!("[DNS] {}", e)),
        }
    }

    if DebloaterManager::has_registry_backup() {
        match DebloaterManager::revert_registry_debloat() {
            Ok(n) => report.debloat_restored = n,
            Err(e) => report.failures.push(format!("[Debloater] {}", e)),
        }
    }

    let pending_backups =
        engine.has_backup() || dns.has_backup() || DebloaterManager::has_registry_backup();
    if report.failures.is_empty() && !pending_backups {
        let parent = data_dir
            .parent()
            .ok_or_else(|| CoreError::Validation("ProgramData sin padre".into()))?;
        match delete_tree_verified(&data_dir, parent, 8) {
            Ok(()) => report.data_dir_deleted = true,
            Err(e) => report.failures.push(format!("[Datos] {}", e)),
        }
    }

    Ok(report)
}
