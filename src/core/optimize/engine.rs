use serde::{Deserialize, Serialize};
use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::core::error::{CoreError, CoreResult};
use crate::core::hardware::profile::{HardwareProfile, HardwareTier};
use crate::core::optimize::backup::BackupManager;
use crate::core::optimize::power_plan::{
    GUID_BALANCED, GUID_HIGH_PERFORMANCE, GUID_ULTIMATE_PERFORMANCE,
    activate_prepared_power_scheme, capture_and_prepare_power_scheme, discard_unused_scheme,
    restore_power_snapshot,
};
use crate::core::optimize::registry_tweaks::{
    apply_registry_tweak, capture_registry_tweak, restore_registry_snapshot,
};
use crate::core::optimize::service_tweaks::{
    ServiceOpOutcome, apply_service_tweak, capture_service_tweak, restore_service_snapshot,
};
use crate::core::optimize::tweaks_catalog::{profile_registry_tweaks, profile_service_tweaks};
use crate::core::platform::elevation;
use crate::core::platform::mutex::NamedMutex;
use crate::core::progress::{StepProgress, report_step};

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

impl StepProgress for EngineProgress {
    fn set_step(&mut self, step: usize, total: usize, label: &str, percent: u8) {
        self.current_step = step;
        self.total_steps = total;
        self.current_label = label.to_string();
        self.percent = percent;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedOptimizationItem {
    pub category: String,
    pub label: String,
    pub detail: String,
    pub eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackReport {
    pub restored_registry: usize,
    pub restored_services: usize,
    pub restored_power: usize,
    pub skipped_user_modified: usize,
    pub failed_items: Vec<String>,
    pub backup_deleted: bool,
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

    /// AR-07: abre el motor sobre la carpeta de datos solo-admin.
    pub fn open_default() -> CoreResult<Self> {
        Ok(Self::new(
            &crate::core::security::data_dir::ensure_admin_only_data_dir()?,
        ))
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
        for spec in profile_registry_tweaks() {
            let eligible =
                tweak_is_eligible(effective_tier, spec.min_tier, spec.applies_to_low_end);
            plan.push(PlannedOptimizationItem {
                category: format!("Registro ({})", spec.hive.as_str()),
                label: spec.label.to_string(),
                detail: spec.description.to_string(),
                eligible,
            });
        }

        for spec in profile_service_tweaks() {
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
        elevation::require_elevated("Aplicar optimizaciones del sistema")?;
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
        let tier = match requested {
            OptimizationProfile::AutoDetected => hw.tier,
            OptimizationProfile::LowEnd => HardwareTier::LowEnd,
            OptimizationProfile::MidRange => HardwareTier::MidRange,
            OptimizationProfile::HighEnd => HardwareTier::HighEnd,
        };
        let profile_name = format!("{:?}", tier);
        let mut rec = StepRecorder::new(
            progress,
            (profile_registry_tweaks().count() + profile_service_tweaks().count() + 1).max(1),
        );

        // Si un respaldo no se puede guardar, no se aplica nada más: un cambio sin respaldo no se podría
        // revertir. El informe parcial se devuelve igual.
        let backup_ok = self
            .apply_registry_tweaks(&mut rec, tier, &profile_name)
            .is_continue()
            && self
                .apply_service_tweaks(&mut rec, tier, &profile_name)
                .is_continue();
        self.apply_power_plan(&mut rec, tier, &hw, &profile_name, backup_ok);

        Ok(rec.into_report(profile_name, hw))
    }

    /// Ajustes de registro del perfil. `Break` si un respaldo falló.
    fn apply_registry_tweaks(
        &self,
        rec: &mut StepRecorder<'_>,
        tier: HardwareTier,
        profile_name: &str,
    ) -> ControlFlow<()> {
        for spec in profile_registry_tweaks() {
            rec.begin(spec.label);
            let category = format!("Registro ({})", spec.hive.as_str());

            if !tweak_is_eligible(tier, spec.min_tier, spec.applies_to_low_end) {
                rec.skip(
                    &category,
                    spec.label,
                    "Omitido por perfil de hardware".into(),
                );
                continue;
            }

            let snap = match capture_registry_tweak(spec) {
                Ok(s) => s,
                Err(e) => {
                    rec.fail(&category, spec.label, format!("[Registro: {}]", spec.id), e);
                    continue;
                }
            };
            if let Err(e) = self
                .backup_mgr
                .record_registry_before_apply(profile_name, snap)
            {
                rec.backup_failed(e);
                return ControlFlow::Break(());
            }

            match apply_registry_tweak(spec) {
                Ok(()) => {
                    rec.requires_relogon |= spec.requires_relogon;
                    rec.applied(&category, spec.label, spec.description.to_string());
                }
                Err(e) => rec.fail(&category, spec.label, format!("[Registro: {}]", spec.id), e),
            }
        }
        ControlFlow::Continue(())
    }

    /// Servicios del perfil. `Break` si un respaldo falló.
    fn apply_service_tweaks(
        &self,
        rec: &mut StepRecorder<'_>,
        tier: HardwareTier,
        profile_name: &str,
    ) -> ControlFlow<()> {
        const CATEGORY: &str = "Servicio Windows";
        for spec in profile_service_tweaks() {
            rec.begin(spec.label);
            let name = spec.service_name;
            let failed_tag = format!("[Servicio: {}]", name);

            if !tweak_is_eligible(tier, spec.min_tier, spec.applies_to_low_end) {
                rec.skip(
                    CATEGORY,
                    spec.label,
                    format!("Servicio '{}' omitido por perfil", name),
                );
                continue;
            }

            let snap = match capture_service_tweak(spec) {
                Ok(Some(s)) => s,
                Ok(None) => {
                    rec.skip(
                        CATEGORY,
                        spec.label,
                        format!("Servicio '{}' no instalado en este sistema", name),
                    );
                    continue;
                }
                Err(e) => {
                    rec.fail(CATEGORY, spec.label, failed_tag, e);
                    continue;
                }
            };
            if let Err(e) = self
                .backup_mgr
                .record_service_before_apply(profile_name, snap)
            {
                rec.backup_failed(e);
                return ControlFlow::Break(());
            }

            match apply_service_tweak(spec) {
                Ok(ServiceOpOutcome::Applied) => rec.applied(
                    CATEGORY,
                    spec.label,
                    format!("Servicio '{}' detenido y deshabilitado", name),
                ),
                Ok(ServiceOpOutcome::AppliedPendingStop(why)) => {
                    rec.warnings.push(format!(
                        "El servicio '{}' quedó deshabilitado pero no se pudo detener ahora ({}); se detendrá al reiniciar.",
                        name, why
                    ));
                    rec.applied(
                        CATEGORY,
                        spec.label,
                        format!(
                            "Servicio '{}' deshabilitado; se detendrá al reiniciar",
                            name
                        ),
                    );
                }
                Ok(_) => rec.skip(CATEGORY, spec.label, format!("Servicio '{}' omitido", name)),
                Err(e) => rec.fail(CATEGORY, spec.label, failed_tag, e),
            }
        }
        ControlFlow::Continue(())
    }

    /// Plan de energía según el nivel. Un fallo del plan no es fatal (queda como aviso); sin respaldo no se activa.
    fn apply_power_plan(
        &self,
        rec: &mut StepRecorder<'_>,
        tier: HardwareTier,
        hw: &HardwareProfile,
        profile_name: &str,
        backup_ok: bool,
    ) {
        const CATEGORY: &str = "Plan de Energía";
        const LABEL: &str = "Esquema energético de Windows";
        const DONE: &str = "Optimización completada";
        rec.begin("Configurando Plan de Energía");

        let (target_guid, fallback_guid) = match tier {
            HardwareTier::LowEnd => (GUID_BALANCED, None),
            HardwareTier::MidRange if hw.is_laptop_or_battery => {
                rec.warnings.push(
                    "Equipo portátil/con batería detectado: se mantiene el plan Equilibrado para proteger autonomía y térmicas."
                        .into(),
                );
                (GUID_BALANCED, None)
            }
            HardwareTier::MidRange => (GUID_HIGH_PERFORMANCE, Some(GUID_BALANCED)),
            HardwareTier::HighEnd if hw.is_laptop_or_battery => {
                rec.warnings.push(
                    "Equipo portátil/con batería detectado: usando Alto Rendimiento en lugar de Máximo Rendimiento para proteger autonomía y térmicas."
                        .into(),
                );
                (GUID_HIGH_PERFORMANCE, Some(GUID_BALANCED))
            }
            HardwareTier::HighEnd => (GUID_ULTIMATE_PERFORMANCE, Some(GUID_HIGH_PERFORMANCE)),
        };

        let existing_created: CoreResult<Vec<String>> = if !backup_ok {
            Err(CoreError::backup_msg("respaldo no disponible"))
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

        let prep = match existing_created.and_then(|created| {
            capture_and_prepare_power_scheme(target_guid, fallback_guid, &created)
        }) {
            Ok(prep) => prep,
            Err(e) => {
                let msg = format!("No se pudo preparar el plan de energía (no fatal): {}", e);
                rec.power_warning(CATEGORY, LABEL, DONE, msg);
                return;
            }
        };
        if prep.already_managed {
            rec.skip_as(
                CATEGORY,
                LABEL,
                DONE,
                "Ya hay un plan creado por OtterSwiss activo; revierte primero para cambiar de perfil".into(),
            );
            return;
        }
        if prep.used_fallback_scheme {
            rec.warnings.push(
                "El plan de energía primario no estaba disponible; se aplicó el plan alternativo."
                    .into(),
            );
        }

        let recorded = self
            .backup_mgr
            .record_power_before_apply(profile_name, prep.snapshot.clone())
            .map_err(|e| CoreError::Backup {
                message: e.to_string(),
                source: Some(Box::new(e)),
            });
        match recorded.and_then(|()| activate_prepared_power_scheme(&prep.snapshot)) {
            Err(e @ CoreError::Backup { .. }) => {
                // Sin respaldo no se activa: se borra el esquema duplicado que no se usará.
                if let Some(ref created) = prep.snapshot.created_scheme_guid {
                    discard_unused_scheme(created);
                }
                rec.failed_items.push(format!(
                    "[Respaldo] {} — no se cambió el plan de energía",
                    e
                ));
            }
            Ok(_) => rec.applied_as(
                CATEGORY,
                LABEL,
                DONE,
                format!("Activado para perfil {}", profile_name),
            ),
            Err(e) => {
                let msg = format!("Plan de energía no aplicado (no fatal): {}", e);
                rec.power_warning(CATEGORY, LABEL, DONE, msg);
            }
        }
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
        elevation::require_elevated("Revertir optimizaciones del sistema")?;
        let _guard = NamedMutex::acquire(ENGINE_MUTEX_NAME)?;

        let (backup_opt, load_warnings) = self.backup_mgr.load_tolerant()?;
        let Some(backup) = backup_opt else {
            return Err(CoreError::backup_msg(
                "No existe ningún respaldo activo para revertir",
            ));
        };

        let mut restored_registry = 0usize;
        let mut restored_services = 0usize;
        let mut restored_power = 0usize;
        let mut skipped_user_modified = 0usize;
        let mut failed_items = load_warnings;

        let total_steps =
            (backup.power_history.len() + backup.services.len() + backup.registry.len()).max(1);
        let mut step_idx = 0usize;

        let push_rb = |idx: usize, label: &str| report_step(progress, idx, total_steps, label);

        for power_snap in backup.power_history.iter().rev() {
            step_idx += 1;
            push_rb(step_idx, "Restaurando Plan de Energía original…");
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
                &format!("Restaurando servicio '{}'…", svc_snap.service_name),
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
                &format!("Restaurando clave '{}'…", reg_snap.value_name),
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

/// CL-05: lleva la cuenta de un perfil en curso (aplicados, omitidos, fallos y avisos), los resultados por paso y
/// el progreso que ve la UI. Cada paso empieza con `begin` y termina con un solo resultado.
struct StepRecorder<'a> {
    progress: Option<&'a Arc<Mutex<EngineProgress>>>,
    total_steps: usize,
    step: usize,
    label: String,
    applied_count: usize,
    skipped_count: usize,
    failed_items: Vec<String>,
    warnings: Vec<String>,
    requires_relogon: bool,
    step_results: Vec<OptimizationStepResult>,
}

impl<'a> StepRecorder<'a> {
    fn new(progress: Option<&'a Arc<Mutex<EngineProgress>>>, total_steps: usize) -> Self {
        Self {
            progress,
            total_steps,
            step: 0,
            label: String::new(),
            applied_count: 0,
            skipped_count: 0,
            failed_items: Vec::new(),
            warnings: Vec::new(),
            requires_relogon: false,
            step_results: Vec::new(),
        }
    }

    fn publish(&self, step: usize, label: &str, done: Option<&OptimizationStepResult>) {
        report_step(self.progress, step, self.total_steps, label);
        if let (Some(item), Some(Ok(mut p))) = (done, self.progress.map(|p| p.lock())) {
            p.completed_steps.push(item.clone());
        }
    }

    /// Empieza el paso siguiente y lo anuncia.
    fn begin(&mut self, label: &str) {
        self.step += 1;
        self.label = label.to_string();
        self.publish(self.step - 1, label, None);
    }

    fn finish(
        &mut self,
        progress_label: &str,
        category: &str,
        label: &str,
        detail: String,
        status: OptimizationStepStatus,
    ) {
        let res = OptimizationStepResult {
            category: category.to_string(),
            label: label.to_string(),
            detail,
            status,
        };
        self.publish(self.step, progress_label, Some(&res));
        self.step_results.push(res);
    }

    fn applied(&mut self, category: &str, label: &str, detail: String) {
        let progress_label = self.label.clone();
        self.applied_as(category, label, &progress_label, detail);
    }

    fn applied_as(&mut self, category: &str, label: &str, progress_label: &str, detail: String) {
        self.applied_count += 1;
        self.finish(
            progress_label,
            category,
            label,
            detail,
            OptimizationStepStatus::Applied,
        );
    }

    fn skip(&mut self, category: &str, label: &str, detail: String) {
        let progress_label = self.label.clone();
        self.skip_as(category, label, &progress_label, detail);
    }

    fn skip_as(&mut self, category: &str, label: &str, progress_label: &str, detail: String) {
        self.skipped_count += 1;
        self.finish(
            progress_label,
            category,
            label,
            detail,
            OptimizationStepStatus::Skipped,
        );
    }

    /// Un paso falló: va a `failed_items` con su etiqueta (`[Registro: id]`) y al informe como fallido.
    fn fail(&mut self, category: &str, label: &str, tag: String, err: CoreError) {
        let detail = err.to_string();
        self.failed_items.push(format!("{tag} {detail}"));
        let progress_label = self.label.clone();
        self.finish(
            &progress_label,
            category,
            label,
            detail,
            OptimizationStepStatus::Failed,
        );
    }

    /// El plan de energía no se aplicó: es un aviso (no cuenta como omitido ni como fallo).
    fn power_warning(&mut self, category: &str, label: &str, progress_label: &str, msg: String) {
        self.warnings.push(msg.clone());
        self.finish(
            progress_label,
            category,
            label,
            msg,
            OptimizationStepStatus::Skipped,
        );
    }

    fn backup_failed(&mut self, err: CoreError) {
        self.failed_items.push(format!(
            "[Respaldo] {} — se detuvo la optimización para no aplicar cambios sin respaldo",
            err
        ));
    }

    fn into_report(self, profile_name: String, hardware: HardwareProfile) -> EngineExecutionReport {
        EngineExecutionReport {
            profile_name,
            hardware,
            applied_count: self.applied_count,
            skipped_count: self.skipped_count,
            failed_items: self.failed_items,
            warnings: self.warnings,
            requires_relogon: self.requires_relogon,
            step_results: self.step_results,
        }
    }
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
