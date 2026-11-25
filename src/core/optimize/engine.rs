use serde::{Deserialize, Serialize};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::core::error::{CoreError, CoreResult};
use crate::core::hardware::profile::{HardwareProfile, HardwareTier};
use crate::core::optimize::backup::BackupManager;
use crate::core::optimize::power_plan::{
    activate_prepared_power_scheme, capture_and_prepare_power_scheme, discard_unused_scheme,
    restore_power_snapshot, GUID_BALANCED, GUID_HIGH_PERFORMANCE, GUID_ULTIMATE_PERFORMANCE,
};
use crate::core::optimize::tweaks_catalog::{REGISTRY_TWEAKS, SERVICE_TWEAKS};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::mutex::NamedMutex;
use crate::core::platform::registry::{
    apply_registry_tweak, capture_registry_tweak, restore_registry_snapshot,
};
use crate::core::platform::services::{
    apply_service_tweak, capture_service_tweak, restore_service_snapshot, ServiceOpOutcome,
};
use crate::core::progress::step_percent;

const ENGINE_MUTEX_NAME: &str = r"Global\OtterSwiss_Engine_Execution";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationProfile {
    AutoDetected,
    LowEnd,
    MidRange,
    HighEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationStepStatus {
    Applied,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizationStepResult {
    pub category: String,
    pub label: String,
    pub detail: String,
    pub status: OptimizationStepStatus,
}

#[derive(Debug, Clone, Default)]
pub struct EngineProgress {
    pub current_step: usize,
    pub total_steps: usize,
    pub current_label: String,
    pub percent: u8,
    pub completed_steps: Vec<OptimizationStepResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedOptimizationItem {
    pub category: String,
    pub label: String,
    pub detail: String,
    pub eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineExecutionReport {
    pub profile_name: String,
    pub hardware: HardwareProfile,
    pub applied_count: usize,
    pub skipped_count: usize,
    pub failed_items: Vec<String>,
    pub warnings: Vec<String>,
    pub requires_relogon: bool,
    #[serde(default)]
    pub step_results: Vec<OptimizationStepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackReport {
    pub restored_registry: usize,
    pub restored_services: usize,
    pub restored_power: usize,
    pub skipped_user_modified: usize,
    pub failed_items: Vec<String>,
    pub backup_deleted: bool,
}

/// Decide si un ajuste entra en el perfil para el nivel de hardware efectivo (una sola regla para el
/// plan que se muestra y para lo que se aplica).
pub fn tweak_is_eligible(
    tier: HardwareTier,
    min_tier: HardwareTier,
    applies_to_low_end: bool,
) -> bool {
    match tier {
        HardwareTier::LowEnd => applies_to_low_end,
        HardwareTier::MidRange => matches!(min_tier, HardwareTier::LowEnd | HardwareTier::MidRange),
        HardwareTier::HighEnd => true,
    }
}

pub struct OptimizationEngine {
    backup_mgr: BackupManager,
}

impl OptimizationEngine {
    pub fn new(secure_data_dir: &Path) -> Self {
        Self {
            backup_mgr: BackupManager::new(secure_data_dir),
        }
    }

    pub fn preview_plan(
        requested: OptimizationProfile,
        hw: &HardwareProfile,
    ) -> Vec<PlannedOptimizationItem> {
        let effective_tier = match requested {
            OptimizationProfile::AutoDetected => hw.tier,
            OptimizationProfile::LowEnd => HardwareTier::LowEnd,
            OptimizationProfile::MidRange => HardwareTier::MidRange,
            OptimizationProfile::HighEnd => HardwareTier::HighEnd,
        };

        let mut plan = Vec::new();
        for spec in REGISTRY_TWEAKS {
            let eligible =
                tweak_is_eligible(effective_tier, spec.min_tier, spec.applies_to_low_end);
            plan.push(PlannedOptimizationItem {
                category: format!("Registro ({})", spec.hive.as_str()),
                label: spec.label.to_string(),
                detail: spec.description.to_string(),
                eligible,
            });
        }

        for spec in SERVICE_TWEAKS {
            let eligible =
                tweak_is_eligible(effective_tier, spec.min_tier, spec.applies_to_low_end);
            plan.push(PlannedOptimizationItem {
                category: "Servicio Windows".to_string(),
                label: spec.label.to_string(),
                detail: format!("Servicio '{}': deshabilitar y detener", spec.service_name),
                eligible,
            });
        }

        let power_desc = match effective_tier {
            HardwareTier::LowEnd => "Plan Equilibrado de Windows (estabilidad térmica)",
            HardwareTier::MidRange => "Plan Alto Rendimiento de Windows",
            HardwareTier::HighEnd => {
                if hw.is_laptop_or_battery {
                    "Plan Alto Rendimiento (portátil/batería detectado)"
                } else {
                    "Plan Máximo Rendimiento (Ultimate Performance)"
                }
            }
        };
        plan.push(PlannedOptimizationItem {
            category: "Plan de Energía".to_string(),
            label: "Activar esquema energético óptimo".to_string(),
            detail: power_desc.to_string(),
            eligible: true,
        });

        plan
    }

    pub fn apply_profile(
        &self,
        requested: OptimizationProfile,
    ) -> CoreResult<EngineExecutionReport> {
        self.apply_profile_with_progress(requested, None)
    }

    pub fn apply_profile_with_progress(
        &self,
        requested: OptimizationProfile,
        progress: Option<&Arc<Mutex<EngineProgress>>>,
    ) -> CoreResult<EngineExecutionReport> {
        ElevationManager::require_elevated("Aplicar optimizaciones del sistema")?;
        let _guard = NamedMutex::acquire(ENGINE_MUTEX_NAME)?;

        let result = catch_unwind(AssertUnwindSafe(|| {
            self.apply_profile_inner(requested, progress)
        }));
        match result {
            Ok(inner) => inner,
            Err(_) => Err(CoreError::Panic(
                "Pánico capturado durante apply_profile; el respaldo previo permanece intacto"
                    .into(),
            )),
        }
    }

    fn apply_profile_inner(
        &self,
        requested: OptimizationProfile,
        progress: Option<&Arc<Mutex<EngineProgress>>>,
    ) -> CoreResult<EngineExecutionReport> {
        let hw = HardwareProfile::detect();
        let effective_tier = match requested {
            OptimizationProfile::AutoDetected => hw.tier,
            OptimizationProfile::LowEnd => HardwareTier::LowEnd,
            OptimizationProfile::MidRange => HardwareTier::MidRange,
            OptimizationProfile::HighEnd => HardwareTier::HighEnd,
        };

        let profile_name = format!("{:?}", effective_tier);
        let mut applied_count = 0usize;
        let mut skipped_count = 0usize;
        let mut failed_items = Vec::new();
        let mut warnings = Vec::new();
        let mut requires_relogon = false;
        let mut step_results = Vec::new();
        // Si un respaldo no se puede guardar, no se aplica nada más: un cambio sin respaldo no se
        // podría revertir. El informe parcial se devuelve igual.
        let mut backup_failed = false;

        let total_steps = (REGISTRY_TWEAKS.len() + SERVICE_TWEAKS.len() + 1).max(1);
        let mut step_idx = 0usize;

        let push_progress = |idx: usize, label: &str, pushed: Option<OptimizationStepResult>| {
            if let Some(p_arc) = progress {
                if let Ok(mut p) = p_arc.lock() {
                    p.current_step = idx;
                    p.total_steps = total_steps;
                    p.current_label = label.to_string();
                    p.percent = step_percent(idx, total_steps);
                    if let Some(item) = pushed {
                        p.completed_steps.push(item);
                    }
                }
            }
        };

        for spec in REGISTRY_TWEAKS {
            if backup_failed {
                break;
            }
            step_idx += 1;
            push_progress(step_idx.saturating_sub(1), spec.label, None);

            let eligible =
                tweak_is_eligible(effective_tier, spec.min_tier, spec.applies_to_low_end);

            if !eligible {
                skipped_count += 1;
                let res = OptimizationStepResult {
                    category: format!("Registro ({})", spec.hive.as_str()),
                    label: spec.label.to_string(),
                    detail: "Omitido por perfil de hardware".into(),
                    status: OptimizationStepStatus::Skipped,
                };
                step_results.push(res.clone());
                push_progress(step_idx, spec.label, Some(res));
                continue;
            }

            let snap = match capture_registry_tweak(spec) {
                Ok(s) => s,
                Err(e) => {
                    let err_txt = format!("{}", e);
                    failed_items.push(format!("[Registro: {}] {}", spec.id, err_txt));
                    let res = OptimizationStepResult {
                        category: format!("Registro ({})", spec.hive.as_str()),
                        label: spec.label.to_string(),
                        detail: err_txt,
                        status: OptimizationStepStatus::Failed,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                    continue;
                }
            };

            if let Err(e) = self
                .backup_mgr
                .record_registry_before_apply(&profile_name, snap)
            {
                failed_items.push(format!(
                    "[Respaldo] {} — se detuvo la optimización para no aplicar cambios sin respaldo",
                    e
                ));
                backup_failed = true;
                continue;
            }

            match apply_registry_tweak(spec) {
                Ok(()) => {
                    applied_count += 1;
                    if spec.requires_relogon {
                        requires_relogon = true;
                    }
                    let res = OptimizationStepResult {
                        category: format!("Registro ({})", spec.hive.as_str()),
                        label: spec.label.to_string(),
                        detail: spec.description.to_string(),
                        status: OptimizationStepStatus::Applied,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                }
                Err(e) => {
                    let err_txt = format!("{}", e);
                    failed_items.push(format!("[Registro: {}] {}", spec.id, err_txt));
                    let res = OptimizationStepResult {
                        category: format!("Registro ({})", spec.hive.as_str()),
                        label: spec.label.to_string(),
                        detail: err_txt,
                        status: OptimizationStepStatus::Failed,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                }
            }
        }

        for spec in SERVICE_TWEAKS {
            if backup_failed {
                break;
            }
            step_idx += 1;
            push_progress(step_idx.saturating_sub(1), spec.label, None);

            let eligible =
                tweak_is_eligible(effective_tier, spec.min_tier, spec.applies_to_low_end);

            if !eligible {
                skipped_count += 1;
                let res = OptimizationStepResult {
                    category: "Servicio Windows".into(),
                    label: spec.label.to_string(),
                    detail: format!("Servicio '{}' omitido por perfil", spec.service_name),
                    status: OptimizationStepStatus::Skipped,
                };
                step_results.push(res.clone());
                push_progress(step_idx, spec.label, Some(res));
                continue;
            }

            let snap_opt = match capture_service_tweak(spec) {
                Ok(s) => s,
                Err(e) => {
                    let err_txt = format!("{}", e);
                    failed_items.push(format!("[Servicio: {}] {}", spec.service_name, err_txt));
                    let res = OptimizationStepResult {
                        category: "Servicio Windows".into(),
                        label: spec.label.to_string(),
                        detail: err_txt,
                        status: OptimizationStepStatus::Failed,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                    continue;
                }
            };

            let Some(snap) = snap_opt else {
                skipped_count += 1;
                let res = OptimizationStepResult {
                    category: "Servicio Windows".into(),
                    label: spec.label.to_string(),
                    detail: format!(
                        "Servicio '{}' no instalado en este sistema",
                        spec.service_name
                    ),
                    status: OptimizationStepStatus::Skipped,
                };
                step_results.push(res.clone());
                push_progress(step_idx, spec.label, Some(res));
                continue;
            };

            if let Err(e) = self
                .backup_mgr
                .record_service_before_apply(&profile_name, snap)
            {
                failed_items.push(format!(
                    "[Respaldo] {} — se detuvo la optimización para no aplicar cambios sin respaldo",
                    e
                ));
                backup_failed = true;
                continue;
            }

            match apply_service_tweak(spec) {
                Ok(ServiceOpOutcome::Applied) => {
                    applied_count += 1;
                    let res = OptimizationStepResult {
                        category: "Servicio Windows".into(),
                        label: spec.label.to_string(),
                        detail: format!(
                            "Servicio '{}' detenido y deshabilitado",
                            spec.service_name
                        ),
                        status: OptimizationStepStatus::Applied,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                }
                Ok(ServiceOpOutcome::AppliedPendingStop(why)) => {
                    applied_count += 1;
                    warnings.push(format!(
                        "El servicio '{}' quedó deshabilitado pero no se pudo detener ahora ({}); se detendrá al reiniciar.",
                        spec.service_name, why
                    ));
                    let res = OptimizationStepResult {
                        category: "Servicio Windows".into(),
                        label: spec.label.to_string(),
                        detail: format!(
                            "Servicio '{}' deshabilitado; se detendrá al reiniciar",
                            spec.service_name
                        ),
                        status: OptimizationStepStatus::Applied,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                }
                Ok(_) => {
                    skipped_count += 1;
                    let res = OptimizationStepResult {
                        category: "Servicio Windows".into(),
                        label: spec.label.to_string(),
                        detail: format!("Servicio '{}' omitido", spec.service_name),
                        status: OptimizationStepStatus::Skipped,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                }
                Err(e) => {
                    let err_txt = format!("{}", e);
                    failed_items.push(format!("[Servicio: {}] {}", spec.service_name, err_txt));
                    let res = OptimizationStepResult {
                        category: "Servicio Windows".into(),
                        label: spec.label.to_string(),
                        detail: err_txt,
                        status: OptimizationStepStatus::Failed,
                    };
                    step_results.push(res.clone());
                    push_progress(step_idx, spec.label, Some(res));
                }
            }
        }

        step_idx += 1;
        push_progress(
            step_idx.saturating_sub(1),
            "Configurando Plan de Energía",
            None,
        );

        let (target_guid, fallback_guid) = match effective_tier {
            HardwareTier::LowEnd => (GUID_BALANCED, None),
            HardwareTier::MidRange if hw.is_laptop_or_battery => {
                warnings.push(
                    "Equipo portátil/con batería detectado: se mantiene el plan Equilibrado para proteger autonomía y térmicas."
                        .into(),
                );
                (GUID_BALANCED, None)
            }
            HardwareTier::MidRange => (GUID_HIGH_PERFORMANCE, Some(GUID_BALANCED)),
            HardwareTier::HighEnd => {
                if hw.is_laptop_or_battery {
                    warnings.push(
                        "Equipo portátil/con batería detectado: usando Alto Rendimiento en lugar de Máximo Rendimiento para proteger autonomía y térmicas."
                            .into(),
                    );
                    (GUID_HIGH_PERFORMANCE, Some(GUID_BALANCED))
                } else {
                    (GUID_ULTIMATE_PERFORMANCE, Some(GUID_HIGH_PERFORMANCE))
                }
            }
        };

        let existing_created: CoreResult<Vec<String>> = if backup_failed {
            Err(CoreError::Backup {
                message: "respaldo no disponible".into(),
                source: None,
            })
        } else {
            self.backup_mgr.load_tolerant().map(|(b, _)| {
                b.map(|b| {
                    b.power_history
                        .into_iter()
                        .filter_map(|p| p.created_scheme_guid)
                        .collect()
                })
                .unwrap_or_default()
            })
        };

        match existing_created.and_then(|created| {
            capture_and_prepare_power_scheme(target_guid, fallback_guid, &created)
        }) {
            Ok(prep) if prep.already_managed => {
                skipped_count += 1;
                let res = OptimizationStepResult {
                    category: "Plan de Energía".into(),
                    label: "Esquema energético de Windows".into(),
                    detail: "Ya hay un plan creado por OtterSwiss activo; revierte primero para cambiar de perfil".into(),
                    status: OptimizationStepStatus::Skipped,
                };
                step_results.push(res.clone());
                push_progress(step_idx, "Optimización completada", Some(res));
            }
            Ok(prep) => {
                if prep.used_fallback_scheme {
                    warnings.push(
                        "El plan de energía primario no estaba disponible; se aplicó el plan alternativo."
                            .into(),
                    );
                }
                let recorded = self
                    .backup_mgr
                    .record_power_before_apply(&profile_name, prep.snapshot.clone())
                    .map_err(|e| CoreError::Backup {
                        message: e.to_string(),
                        source: None,
                    });

                match recorded.and_then(|()| activate_prepared_power_scheme(&prep.snapshot)) {
                    Err(e @ CoreError::Backup { .. }) => {
                        // Sin respaldo no se activa: se borra el esquema duplicado que no se usará.
                        if let Some(ref created) = prep.snapshot.created_scheme_guid {
                            discard_unused_scheme(created);
                        }
                        failed_items.push(format!(
                            "[Respaldo] {} — no se cambió el plan de energía",
                            e
                        ));
                    }
                    Ok(_) => {
                        applied_count += 1;
                        let res = OptimizationStepResult {
                            category: "Plan de Energía".into(),
                            label: "Esquema energético de Windows".into(),
                            detail: format!("Activado para perfil {}", profile_name),
                            status: OptimizationStepStatus::Applied,
                        };
                        step_results.push(res.clone());
                        push_progress(step_idx, "Optimización completada", Some(res));
                    }
                    Err(e) => {
                        let msg = format!("Plan de energía no aplicado (no fatal): {}", e);
                        warnings.push(msg.clone());
                        let res = OptimizationStepResult {
                            category: "Plan de Energía".into(),
                            label: "Esquema energético de Windows".into(),
                            detail: msg,
                            status: OptimizationStepStatus::Skipped,
                        };
                        step_results.push(res.clone());
                        push_progress(step_idx, "Optimización completada", Some(res));
                    }
                }
            }
            Err(e) => {
                let msg = format!("No se pudo preparar el plan de energía (no fatal): {}", e);
                warnings.push(msg.clone());
                let res = OptimizationStepResult {
                    category: "Plan de Energía".into(),
                    label: "Esquema energético de Windows".into(),
                    detail: msg,
                    status: OptimizationStepStatus::Skipped,
                };
                step_results.push(res.clone());
                push_progress(step_idx, "Optimización completada", Some(res));
            }
        }

        Ok(EngineExecutionReport {
            profile_name,
            hardware: hw,
            applied_count,
            skipped_count,
            failed_items,
            warnings,
            requires_relogon,
            step_results,
        })
    }

    /// `true` si hay un respaldo de optimización pendiente de revertir.
    pub fn has_backup(&self) -> bool {
        self.backup_mgr.exists()
    }

    pub fn rollback(&self, force_overwrite_user_changes: bool) -> CoreResult<RollbackReport> {
        self.rollback_with_progress(force_overwrite_user_changes, None)
    }

    pub fn rollback_with_progress(
        &self,
        force_overwrite_user_changes: bool,
        progress: Option<&Arc<Mutex<EngineProgress>>>,
    ) -> CoreResult<RollbackReport> {
        ElevationManager::require_elevated("Revertir optimizaciones del sistema")?;
        let _guard = NamedMutex::acquire(ENGINE_MUTEX_NAME)?;

        let (backup_opt, load_warnings) = self.backup_mgr.load_tolerant()?;
        let Some(backup) = backup_opt else {
            return Err(CoreError::Backup {
                message: "No existe ningún respaldo activo para revertir".into(),
                source: None,
            });
        };

        let mut restored_registry = 0usize;
        let mut restored_services = 0usize;
        let mut restored_power = 0usize;
        let mut skipped_user_modified = 0usize;
        let mut failed_items = load_warnings;

        let total_steps =
            (backup.power_history.len() + backup.services.len() + backup.registry.len()).max(1);
        let mut step_idx = 0usize;

        let push_rb = |idx: usize, label: &str| {
            if let Some(p_arc) = progress {
                if let Ok(mut p) = p_arc.lock() {
                    p.current_step = idx;
                    p.total_steps = total_steps;
                    p.current_label = label.to_string();
                    p.percent = step_percent(idx, total_steps);
                }
            }
        };

        for power_snap in backup.power_history.iter().rev() {
            step_idx += 1;
            push_rb(step_idx, "Restaurando Plan de Energía original...");
            match restore_power_snapshot(power_snap, force_overwrite_user_changes) {
                Ok(true) => restored_power += 1,
                Ok(false) => skipped_user_modified += 1,
                Err(e) => failed_items.push(format!("[Energía] {}", e)),
            }
        }

        for svc_snap in backup.services.iter().rev() {
            step_idx += 1;
            push_rb(
                step_idx,
                &format!("Restaurando servicio '{}'...", svc_snap.service_name),
            );
            match restore_service_snapshot(svc_snap, force_overwrite_user_changes) {
                Ok(ServiceOpOutcome::Applied | ServiceOpOutcome::AppliedPendingStop(_)) => {
                    restored_services += 1
                }
                Ok(ServiceOpOutcome::SkippedUserModified) => skipped_user_modified += 1,
                Ok(ServiceOpOutcome::SkippedNotInstalled) => {}
                Err(e) => failed_items.push(format!("[Servicio: {}] {}", svc_snap.service_name, e)),
            }
        }

        for reg_snap in backup.registry.iter().rev() {
            step_idx += 1;
            push_rb(
                step_idx,
                &format!("Restaurando clave '{}'...", reg_snap.value_name),
            );
            match restore_registry_snapshot(reg_snap, force_overwrite_user_changes) {
                Ok(true) => restored_registry += 1,
                Ok(false) => skipped_user_modified += 1,
                Err(e) => failed_items.push(format!(
                    "[Registro: {:?}\\{}\\{}] {}",
                    reg_snap.hive, reg_snap.subkey, reg_snap.value_name, e
                )),
            }
        }

        // Con ítems omitidos el respaldo se conserva: el usuario aún puede forzar su restauración.
        let backup_deleted = if failed_items.is_empty() && skipped_user_modified == 0 {
            self.backup_mgr.delete_if_clean()?;
            true
        } else {
            false
        };

        push_rb(total_steps, "Reversión completada");

        Ok(RollbackReport {
            restored_registry,
            restored_services,
            restored_power,
            skipped_user_modified,
            failed_items,
            backup_deleted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elegibilidad_por_nivel() {
        assert!(tweak_is_eligible(
            HardwareTier::LowEnd,
            HardwareTier::LowEnd,
            true
        ));
        assert!(!tweak_is_eligible(
            HardwareTier::LowEnd,
            HardwareTier::LowEnd,
            false
        ));
        assert!(tweak_is_eligible(
            HardwareTier::MidRange,
            HardwareTier::MidRange,
            false
        ));
        assert!(!tweak_is_eligible(
            HardwareTier::MidRange,
            HardwareTier::HighEnd,
            false
        ));
        assert!(tweak_is_eligible(
            HardwareTier::HighEnd,
            HardwareTier::HighEnd,
            false
        ));
    }
}
