//! Debloater: quitar apps Appx del catálogo y aplicar o revertir ajustes de privacidad en el registro.

mod appx;
mod catalog;
mod registry_backup;

pub use catalog::{
    DEBLOAT_APP_CATALOG, DEBLOAT_REG_TWEAKS, DebloatAppSpec, DebloatCategory, DebloatRegEntry,
    DebloatRegTweakSpec,
};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::core::error::CoreResult;
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::mutex::NamedMutex;
use crate::core::progress::{StepProgress, report_step};
use appx::{get_powershell_path, is_valid_package_ident, run_ps};

pub(crate) const DEBLOAT_MUTEX: &str = r"Global\OtterSwiss_Debloat_Lock";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedBloatApp {
    pub id: &'static str,
    pub package_name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub category: DebloatCategory,
    pub default_selected: bool,
    pub installed: bool,
    pub full_names: Vec<String>,
    pub provisioned_names: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DebloatScanResult {
    pub apps: Vec<DetectedBloatApp>,
    pub has_reg_backup: bool,
}

impl DebloatScanResult {
    /// AR-15: lo que viene marcado al abrir: apps instaladas marcadas como recomendadas en el catálogo (SYS-20) y
    /// los ajustes de registro recomendados.
    pub fn recommended_selection(
        &self,
    ) -> (
        std::collections::HashSet<&'static str>,
        std::collections::HashSet<&'static str>,
    ) {
        let apps = self
            .apps
            .iter()
            .filter(|a| a.installed && a.default_selected)
            .map(|a| a.id)
            .collect();
        let regs = DEBLOAT_REG_TWEAKS
            .iter()
            .filter(|r| r.default_selected)
            .map(|r| r.id)
            .collect();
        (apps, regs)
    }
}

#[derive(Debug, Clone, Default)]
pub struct DebloatProgress {
    pub current_step: usize,
    pub total_steps: usize,
    pub current_label: String,
    pub percent: u8,
}

impl StepProgress for DebloatProgress {
    fn set_step(&mut self, step: usize, total: usize, label: &str, percent: u8) {
        self.current_step = step;
        self.total_steps = total;
        self.current_label = label.to_string();
        self.percent = percent;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebloatItemOutcome {
    pub label: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebloatReport {
    /// `true` si el usuario canceló: el informe incluye lo que ya se hizo antes de cancelar.
    pub cancelled: bool,
    pub removed_apps: usize,
    pub applied_reg_tweaks: usize,
    pub failed_count: usize,
    pub outcomes: Vec<DebloatItemOutcome>,
}

pub struct DebloaterManager;

impl DebloaterManager {
    pub fn execute_debloat(
        apps_to_remove: &[DetectedBloatApp],
        reg_tweak_ids: &[&'static str],
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<DebloatProgress>>>,
    ) -> CoreResult<DebloatReport> {
        ElevationManager::require_elevated("Ejecutar Debloater de Windows")?;
        let _lock = NamedMutex::acquire(DEBLOAT_MUTEX)?;
        let ps_path = get_powershell_path()?;

        let mut report = DebloatReport::default();
        let total_steps = (apps_to_remove.len() + reg_tweak_ids.len()).max(1);
        let mut step_idx = 0usize;

        let push_prog = |idx: usize, label: &str| report_step(progress, idx, total_steps, label);

        for app in apps_to_remove {
            if cancel.load(Ordering::Acquire) {
                report.cancelled = true;
                return Ok(report);
            }

            step_idx += 1;
            push_prog(
                step_idx.saturating_sub(1),
                &format!("Eliminando {}…", app.display_name),
            );

            if !DEBLOAT_APP_CATALOG
                .iter()
                .any(|s| s.id == app.id && s.package_name == app.package_name)
            {
                report.failed_count += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: app.display_name.to_string(),
                    success: false,
                    message: "Rechazado: paquete fuera del catálogo autorizado".into(),
                });
                continue;
            }

            let mut app_ok = true;
            let mut last_err = String::new();
            let mut only_current_user = false;

            for full_name in &app.full_names {
                if !is_valid_package_ident(full_name) {
                    continue;
                }
                // Cada paquete se deja terminar: cancelar a mitad de Remove-AppxPackage puede dejarlo roto.
                let cmd = format!(
                    "try {{ Remove-AppxPackage -Package '{}' -AllUsers -ErrorAction Stop; 'SCOPE=ALL' }} catch {{ Remove-AppxPackage -Package '{}' -ErrorAction Stop; 'SCOPE=USER' }}",
                    full_name, full_name
                );
                match run_ps(&ps_path, &cmd, 90_000) {
                    Ok(out) if out.contains("SCOPE=USER") => only_current_user = true,
                    Ok(_) => {}
                    Err(e) => {
                        app_ok = false;
                        last_err = e.to_string();
                    }
                }
            }

            for prov_name in &app.provisioned_names {
                if !is_valid_package_ident(prov_name) {
                    continue;
                }
                let cmd = format!(
                    "Remove-AppxProvisionedPackage -Online -PackageName '{}' -ErrorAction Stop | Out-Null",
                    prov_name
                );
                if let Err(e) = run_ps(&ps_path, &cmd, 90_000) {
                    app_ok = false;
                    last_err = e.to_string();
                }
            }

            if app_ok {
                report.removed_apps += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: app.display_name.to_string(),
                    success: true,
                    message: if only_current_user {
                        "Eliminada solo para el usuario actual (otros perfiles la conservan)".into()
                    } else if app.provisioned_names.is_empty() {
                        "Eliminada para todos los usuarios".into()
                    } else {
                        "Eliminada para todos los usuarios y desaprovisionada".into()
                    },
                });
            } else {
                report.failed_count += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: app.display_name.to_string(),
                    success: false,
                    message: last_err,
                });
            }
            push_prog(step_idx, &format!("Procesado {}", app.display_name));
        }

        for &tweak_id in reg_tweak_ids {
            if cancel.load(Ordering::Acquire) {
                report.cancelled = true;
                return Ok(report);
            }
            step_idx += 1;

            let Some(spec) = DEBLOAT_REG_TWEAKS.iter().find(|t| t.id == tweak_id) else {
                continue;
            };
            push_prog(step_idx.saturating_sub(1), spec.label);

            if let Err(e) = Self::backup_reg_entries(spec.entries) {
                report.failed_count += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: spec.label.to_string(),
                    success: false,
                    message: format!("Error guardando respaldo: {}", e),
                });
                continue;
            }

            match Self::apply_reg_entries(spec.entries) {
                Ok(()) => {
                    report.applied_reg_tweaks += 1;
                    report.outcomes.push(DebloatItemOutcome {
                        label: spec.label.to_string(),
                        success: true,
                        message: "Aplicado en Registro con respaldo".into(),
                    });
                }
                Err(e) => {
                    report.failed_count += 1;
                    report.outcomes.push(DebloatItemOutcome {
                        label: spec.label.to_string(),
                        success: false,
                        message: e.to_string(),
                    });
                }
            }
            push_prog(step_idx, spec.label);
        }

        push_prog(total_steps, "Debloat finalizado");
        Ok(report)
    }
}
