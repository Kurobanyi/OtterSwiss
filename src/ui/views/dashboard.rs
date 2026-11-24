use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::elevation::ElevationManager;
use crate::core::engine::{
    EngineExecutionReport, EngineProgress, OptimizationEngine, OptimizationProfile,
    OptimizationStepStatus, RollbackReport,
};
use crate::core::power::{format_guid, get_active_scheme_guid};
use crate::core::trust::get_program_data_dir;
use crate::core::win_common::{reboot_system_now, HardwareProfile, HardwareTier};
use crate::ui::components::Card;
use crate::ui::i18n::Lang;

#[component]
pub fn DashboardView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let hw_profile = use_signal(HardwareProfile::detect);
    let is_elevated = use_signal(|| ElevationManager::is_elevated().unwrap_or(false));
    let mut is_running = use_signal(|| false);
    let mut active_action_label = use_signal(String::new);
    let mut show_reboot_modal = use_signal(|| false);
    let mut progress_state = use_signal(EngineProgress::default);
    let mut last_report = use_signal(|| Option::<EngineExecutionReport>::None);
    let mut last_rollback = use_signal(|| Option::<RollbackReport>::None);
    let mut error_banner = use_signal(|| Option::<String>::None);

    let mut active_scheme = use_signal(|| {
        get_active_scheme_guid()
            .map(|g| format_guid(&g))
            .unwrap_or_else(|_| "N/A".to_string())
    });

    let on_optimize = move |_| {
        is_running.set(true);
        active_action_label.set(if en {
            "Applying system optimizations".to_string()
        } else {
            "Aplicando optimizaciones".to_string()
        });
        error_banner.set(None);
        last_rollback.set(None);
        progress_state.set(EngineProgress {
            current_step: 0,
            total_steps: 13,
            current_label: if en { "Creating backup..." } else { "Creando respaldo..." }.to_string(),
            percent: 5,
            completed_steps: Vec::new(),
        });

        let prog_arc = Arc::new(Mutex::new(EngineProgress::default()));
        let prog_worker = Arc::clone(&prog_arc);

        spawn(async move {
            let handle = tokio::task::spawn_blocking(move || {
                let data_dir = get_program_data_dir()
                    .map_err(|e| e.to_string())?
                    .join("OtterSwiss");
                std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

                let engine = OptimizationEngine::new(&data_dir);
                engine
                    .apply_profile_with_progress(
                        OptimizationProfile::AutoDetected,
                        Some(&prog_worker),
                    )
                    .map_err(|e| e.to_string())
            });

            while !handle.is_finished() {
                if let Ok(snap) = prog_arc.lock() {
                    if snap.total_steps > 0 {
                        progress_state.set(snap.clone());
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let res = handle
                .await
                .unwrap_or_else(|e| Err(format!("Thread error: {}", e)));

            match res {
                Ok(report) => {
                    if let Ok(snap) = prog_arc.lock() {
                        progress_state.set(snap.clone());
                    }
                    let need_reboot = report.requires_relogon;
                    last_report.set(Some(report));
                    if let Ok(g) = get_active_scheme_guid() {
                        active_scheme.set(format_guid(&g));
                    }
                    if need_reboot {
                        show_reboot_modal.set(true);
                    }
                }
                Err(err) => error_banner.set(Some(err)),
            }
            is_running.set(false);
        });
    };

    let on_rollback = move |_| {
        is_running.set(true);
        active_action_label.set(if en {
            "Reverting from backup".to_string()
        } else {
            "Revirtiendo desde respaldo".to_string()
        });
        error_banner.set(None);
        last_report.set(None);
        progress_state.set(EngineProgress {
            current_step: 0,
            total_steps: 13,
            current_label: if en { "Reading backup..." } else { "Leyendo respaldo..." }.to_string(),
            percent: 5,
            completed_steps: Vec::new(),
        });

        let prog_arc = Arc::new(Mutex::new(EngineProgress::default()));
        let prog_worker = Arc::clone(&prog_arc);

        spawn(async move {
            let handle = tokio::task::spawn_blocking(move || {
                let data_dir = get_program_data_dir()
                    .map_err(|e| e.to_string())?
                    .join("OtterSwiss");

                let engine = OptimizationEngine::new(&data_dir);
                engine
                    .rollback_with_progress(false, Some(&prog_worker))
                    .map_err(|e| e.to_string())
            });

            while !handle.is_finished() {
                if let Ok(snap) = prog_arc.lock() {
                    if snap.total_steps > 0 {
                        progress_state.set(snap.clone());
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let res = handle
                .await
                .unwrap_or_else(|e| Err(format!("Thread error: {}", e)));

            match res {
                Ok(rb) => {
                    last_rollback.set(Some(rb));
                    if let Ok(g) = get_active_scheme_guid() {
                        active_scheme.set(format_guid(&g));
                    }
                }
                Err(err) => error_banner.set(Some(err)),
            }
            is_running.set(false);
        });
    };

    let on_reboot_now = move |_| {
        if let Err(e) = reboot_system_now() {
            show_reboot_modal.set(false);
            error_banner.set(Some(format!("Error: {}", e)));
        } else {
            show_reboot_modal.set(false);
        }
    };

    let on_relaunch_admin = move |_| {
        let _ = ElevationManager::relaunch_as_admin(&["--elevated"]);
    };

    let hw = hw_profile.read();
    let planned_items = OptimizationEngine::preview_plan(OptimizationProfile::AutoDetected, &hw);
    let prog = progress_state.read();

    let overview_title = if en { "System Overview" } else { "Resumen del Equipo" }.to_string();
    let overview_sub = Some((if en { "Native Win32 detection" } else { "Detección nativa Win32" }).to_string());
    let power_title = if en { "Active Power Plan" } else { "Plan de Energía" }.to_string();
    let power_sub = Some((if en { "Current Windows scheme" } else { "Esquema activo Win32" }).to_string());
    let opt_title = if en { "Optimizer" } else { "Optimizador" }.to_string();
    let opt_sub = Some((if en { "Transactional with full rollback" } else { "Transaccional con rollback seguro" }).to_string());
    let rollback_title = if en { "Rollback Completed" } else { "Rollback Completado" }.to_string();
    let planned_title = if en { format!("Planned Tweaks ({:?})", hw.tier) } else { format!("Ajustes del Perfil ({:?})", hw.tier) };
    let planned_sub = Some((if en { "Backed up before applying" } else { "Respaldo previo automático" }).to_string());

    rsx! {
        div { class: "content-body",
            if *show_reboot_modal.read() {
                div {
                    style: "position: fixed; inset: 0; background: rgba(0, 0, 0, 0.72); display: flex; align-items: center; justify-content: center; z-index: 9999; padding: 24px;",
                    div {
                        class: "card",
                        style: "max-width: 440px; width: 100%; border: 1px solid var(--accent);",
                        div { class: "card-header",
                            div {
                                h3 { class: "card-title", if en { "System Reboot Required" } else { "Reinicio Requerido" } }
                                p { class: "card-subtitle", if en { "Some changes require restarting Windows." } else { "Algunos cambios requieren reiniciar Windows." } }
                            }
                        }
                        div { style: "display: flex; justify-content: flex-end; gap: var(--space-3);",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| show_reboot_modal.set(false),
                                if en { "Later" } else { "Más Tarde" }
                            }
                            button {
                                class: "btn btn-primary",
                                onclick: on_reboot_now,
                                if en { "Reboot Now" } else { "Reiniciar Ahora" }
                            }
                        }
                    }
                }
            }

            if !*is_elevated.read() {
                div { class: "card",
                    div { class: "card-header",
                        div {
                            h3 { class: "card-title", if en { "Admin Rights Required" } else { "Se Requiere Administrador" } }
                            p { class: "card-subtitle", if en { "Elevate to modify services, registry & power." } else { "Eleva para ajustar registro, servicios y energía." } }
                        }
                        button {
                            class: "btn btn-primary",
                            onclick: on_relaunch_admin,
                            if en { "Relaunch as Admin" } else { "Ejecutar como Admin" }
                        }
                    }
                }
            }

            div { class: "cards-grid",
                Card {
                    title: overview_title,
                    subtitle: overview_sub,
                    div { style: "display: flex; flex-direction: column; gap: 6px; font-size: var(--font-size-sm);",
                        p { strong { "CPU: " } "{hw.cpu_name} ({hw.logical_cores}T)" }
                        p { strong { if en { "Board: " } else { "Placa: " } } "{hw.motherboard}" }
                        p { strong { "RAM: " } "{hw.total_ram_mb} MB" }
                        for gpu in hw.gpu_names.iter() {
                            p { strong { "GPU: " } "{gpu}" }
                        }
                        for drive in hw.storage_drives.iter() {
                            p { strong { if en { "Disk: " } else { "Disco: " } } "{drive}" }
                        }
                        div { style: "margin-top: 4px;",
                            span {
                                class: match hw.tier {
                                    HardwareTier::HighEnd => "badge badge-success",
                                    HardwareTier::MidRange => "badge badge-info",
                                    HardwareTier::LowEnd => "badge badge-warning",
                                },
                                "{hw.tier:?}"
                            }
                        }
                    }
                }

                Card {
                    title: power_title,
                    subtitle: power_sub,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        code { style: "font-size: var(--font-size-xs); background: var(--bg); padding: 6px; border-radius: var(--radius-sm); word-break: break-all;",
                            "{active_scheme.read()}"
                        }
                        p { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                            if en { "Auto-selects optimal plan for desktop or laptop." } else { "Selecciona el plan óptimo según PC o portátil." }
                        }
                    }
                }

                Card {
                    title: opt_title,
                    subtitle: opt_sub,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-3);",
                        button {
                            class: "btn btn-primary",
                            disabled: *is_running.read(),
                            onclick: on_optimize,
                            if *is_running.read() {
                                if en { "Processing..." } else { "Procesando..." }
                            } else if en {
                                "Optimize System"
                            } else {
                                "Optimizar Sistema"
                            }
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_running.read(),
                            onclick: on_rollback,
                            if en { "Revert (Rollback)" } else { "Revertir (Rollback)" }
                        }
                    }
                }
            }

            if let Some(err) = error_banner.read().as_ref() {
                div { class: "badge badge-danger", style: "padding: var(--space-3); font-size: var(--font-size-sm);",
                    "Error: {err}"
                }
            }

            if *is_running.read() {
                Card {
                    title: active_action_label.read().clone(),
                    subtitle: Some(format!("{}/{} — {}", prog.current_step, prog.total_steps.max(1), prog.current_label)),
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        div { style: "font-size: var(--font-size-xs); color: var(--text-muted);", "{prog.percent}%" }
                        div {
                            style: "width: 100%; height: 10px; background: var(--bg); border-radius: 999px; overflow: hidden; border: 1px solid var(--border);",
                            div { style: "height: 100%; width: {prog.percent}%; background: var(--accent); transition: width 0.2s ease;" }
                        }
                    }
                }
            }

            if let Some(rb) = last_rollback.read().as_ref() {
                Card {
                    title: rollback_title,
                    subtitle: None,
                    div { style: "display: flex; gap: var(--space-3); flex-wrap: wrap;",
                        span { class: "badge badge-success", if en { "Registry: {rb.restored_registry}" } else { "Registro: {rb.restored_registry}" } }
                        span { class: "badge badge-success", if en { "Services: {rb.restored_services}" } else { "Servicios: {rb.restored_services}" } }
                        span { class: "badge badge-success", if en { "Power: {rb.restored_power}" } else { "Energía: {rb.restored_power}" } }
                    }
                }
            }

            if let Some(rep) = last_report.read().as_ref() {
                Card {
                    title: if en { format!("Results — {}", rep.profile_name) } else { format!("Resultados — {}", rep.profile_name) },
                    subtitle: Some(format!("OK: {} • Skip: {} • Err: {}", rep.applied_count, rep.skipped_count, rep.failed_items.len())),
                    div { style: "display: flex; flex-direction: column; gap: 6px;",
                        for step in rep.step_results.iter() {
                            div {
                                key: "{step.category}:{step.label}",
                                style: "display: flex; justify-content: space-between; align-items: center; padding: 6px 0; border-bottom: 1px solid var(--border);",
                                div { style: "font-size: var(--font-size-sm); font-weight: 600;",
                                    "[{step.category}] {step.label}"
                                }
                                span {
                                    class: match step.status {
                                        OptimizationStepStatus::Applied => "badge badge-success",
                                        OptimizationStepStatus::Skipped => "badge badge-info",
                                        OptimizationStepStatus::Failed => "badge badge-danger",
                                    },
                                    match (step.status, en) {
                                        (OptimizationStepStatus::Applied, false) => "Aplicado",
                                        (OptimizationStepStatus::Applied, true) => "Applied",
                                        (OptimizationStepStatus::Skipped, false) => "Omitido",
                                        (OptimizationStepStatus::Skipped, true) => "Skipped",
                                        (OptimizationStepStatus::Failed, _) => "Error",
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                Card {
                    title: planned_title,
                    subtitle: planned_sub,
                    div { style: "display: flex; flex-direction: column; gap: 6px;",
                        for item in planned_items.iter() {
                            div {
                                key: "{item.category}:{item.label}",
                                style: "display: flex; justify-content: space-between; align-items: center; padding: 6px 0; border-bottom: 1px solid var(--border);",
                                div { style: "font-size: var(--font-size-sm); font-weight: 600;",
                                    "[{item.category}] {item.label}"
                                }
                                span {
                                    class: if item.eligible { "badge badge-success" } else { "badge badge-info" },
                                    if item.eligible {
                                        if en { "Included" } else { "Incluido" }
                                    } else if en {
                                        "Skipped"
                                    } else {
                                        "Omitido"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}