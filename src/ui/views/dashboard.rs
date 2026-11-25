use dioxus::prelude::*;

use crate::core::hardware::profile::{HardwareProfile, HardwareTier};
use crate::core::optimize::engine::{
    EngineExecutionReport, EngineProgress, OptimizationEngine, OptimizationProfile,
    OptimizationStepStatus, RollbackReport,
};
use crate::core::optimize::power_plan::get_active_scheme_guid;
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::guid::format_guid;
use crate::core::platform::power::reboot_system_now;
use crate::ui::components::{Card, ConfirmDialog, LoadingState, ProgressBar, StatusBanner, Tone};
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::run_with_progress;

#[component]
pub fn DashboardView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    // UI-05: la detección de hardware (WMI) corre fuera del hilo de UI.
    let mut hw_profile = use_signal(|| Option::<HardwareProfile>::None);
    use_hook(move || {
        spawn(async move {
            if let Ok(hw) = tokio::task::spawn_blocking(HardwareProfile::detect).await {
                hw_profile.set(Some(hw));
            }
        });
    });
    let is_elevated = use_signal(|| ElevationManager::is_elevated().unwrap_or(false));
    let mut is_running = use_signal(|| false);
    let mut active_action_label = use_signal(|| Msg::plain(""));
    let mut show_reboot_modal = use_signal(|| false);
    let mut confirm_optimize = use_signal(|| false);
    let mut progress_state = use_signal(EngineProgress::default);
    let mut last_report = use_signal(|| Option::<EngineExecutionReport>::None);
    let mut last_rollback = use_signal(|| Option::<RollbackReport>::None);
    let mut error_banner = use_signal(|| Option::<Msg>::None);

    let mut active_scheme = use_signal(|| {
        get_active_scheme_guid()
            .map(|g| format_guid(&g))
            .unwrap_or_else(|_| "N/A".to_string())
    });

    let mut start_optimize = move || {
        is_running.set(true);
        active_action_label.set(Msg::new(|en| {
            if en {
                "Applying system optimizations".to_string()
            } else {
                "Aplicando optimizaciones".to_string()
            }
        }));
        error_banner.set(None);
        last_rollback.set(None);
        progress_state.set(EngineProgress {
            current_step: 0,
            total_steps: 0,
            current_label: if en {
                "Creating backup…"
            } else {
                "Creando respaldo…"
            }
            .to_string(),
            percent: 5,
            completed_steps: Vec::new(),
        });

        spawn(async move {
            let res = run_with_progress(progress_state, move |prog| {
                let engine = OptimizationEngine::open_default().map_err(Msg::from)?;
                engine
                    .apply_profile_with_progress(OptimizationProfile::AutoDetected, Some(prog))
                    .map_err(Msg::from)
            })
            .await
            .and_then(|r| r);

            match res {
                Ok(report) => {
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
        active_action_label.set(Msg::new(|en| {
            if en {
                "Reverting from backup".to_string()
            } else {
                "Revirtiendo desde respaldo".to_string()
            }
        }));
        error_banner.set(None);
        last_report.set(None);
        progress_state.set(EngineProgress {
            current_step: 0,
            total_steps: 0,
            current_label: if en {
                "Reading backup…"
            } else {
                "Leyendo respaldo…"
            }
            .to_string(),
            percent: 5,
            completed_steps: Vec::new(),
        });

        spawn(async move {
            let res = run_with_progress(progress_state, move |prog| {
                let engine = OptimizationEngine::open_default().map_err(Msg::from)?;
                engine
                    .rollback_with_progress(false, Some(prog))
                    .map_err(Msg::from)
            })
            .await
            .and_then(|r| r);

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

    let mut on_reboot_now = move || {
        show_reboot_modal.set(false);
        if let Err(e) = reboot_system_now() {
            error_banner.set(Some(Msg::from(e)));
        }
    };

    let on_relaunch_admin = move |_| {
        let _ = ElevationManager::relaunch_as_admin(&[crate::core::cli::ELEVATED]);
    };

    let hw_guard = hw_profile.read();
    let Some(hw) = hw_guard.as_ref() else {
        return rsx! {
            div { class: "content-body",
                LoadingState {
                    message: if en { "Detecting hardware…".to_string() } else { "Detectando el hardware…".to_string() },
                }
            }
        };
    };
    let planned_items = OptimizationEngine::preview_plan(OptimizationProfile::AutoDetected, hw);
    let eligible_items: Vec<_> = planned_items
        .iter()
        .filter(|i| i.eligible)
        .cloned()
        .collect();
    let prog = progress_state.read();

    let overview_title = if en {
        "System Overview"
    } else {
        "Resumen del equipo"
    }
    .to_string();
    let overview_sub = Some(
        (if en {
            "Native Win32 detection"
        } else {
            "Detección nativa Win32"
        })
        .to_string(),
    );
    let power_title = if en {
        "Active Power Plan"
    } else {
        "Plan de energía"
    }
    .to_string();
    let power_sub = Some(
        (if en {
            "Current Windows scheme"
        } else {
            "Esquema activo Win32"
        })
        .to_string(),
    );
    let opt_title = if en { "Optimizer" } else { "Optimizador" }.to_string();
    // UI-01: sin «transaccional»; el respaldo permite revertir, pero no garantiza todo o nada.
    let opt_sub = Some(
        (if en {
            "Backs up each value before changing it, so it can be reverted"
        } else {
            "Respalda cada valor antes de cambiarlo para poder revertirlo"
        })
        .to_string(),
    );
    let planned_title = if en {
        format!("Planned Tweaks ({:?})", hw.tier)
    } else {
        format!("Ajustes del Perfil ({:?})", hw.tier)
    };
    let planned_sub = Some(
        (if en {
            "Backed up before applying"
        } else {
            "Respaldo previo automático"
        })
        .to_string(),
    );

    rsx! {
        div { class: "content-body",
            // UI-13: basta cerrar sesión o reiniciar; reiniciar ahora cierra las apps abiertas.
            if *show_reboot_modal.read() {
                ConfirmDialog {
                    title: if en { "Sign out or restart to finish".to_string() } else { "Cierra sesión o reinicia para terminar".to_string() },
                    subtitle: Some(if en {
                        "Some changes take effect after signing out or restarting Windows.".to_string()
                    } else {
                        "Algunos cambios se aplican al cerrar sesión o al reiniciar Windows.".to_string()
                    }),
                    confirm_label: if en { "Restart now".to_string() } else { "Reiniciar ahora".to_string() },
                    cancel_label: strings(en).later.to_string(),
                    tone: Tone::Warning,
                    on_cancel: move |_| show_reboot_modal.set(false),
                    on_confirm: move |_| on_reboot_now(),
                    p { class: "text-xs",
                        if en {
                            "Restarting closes every open app; save your work first."
                        } else {
                            "Reiniciar cierra todas las apps abiertas; guarda tu trabajo antes."
                        }
                    }
                }
            }

            // UX-02: optimizar muestra antes el plan concreto que se va a aplicar.
            if *confirm_optimize.read() {
                ConfirmDialog {
                    title: if en { "Apply these optimizations?".to_string() } else { "¿Aplicar estas optimizaciones?".to_string() },
                    subtitle: Some(if en {
                        "Each value is backed up first; «Revert» restores it.".to_string()
                    } else {
                        "Cada valor se respalda antes; «Revertir» lo restaura.".to_string()
                    }),
                    confirm_label: if en { format!("Apply ({})", eligible_items.len()) } else { format!("Aplicar ({})", eligible_items.len()) },
                    cancel_label: strings(en).cancel.to_string(),
                    wide: true,
                    on_cancel: move |_| confirm_optimize.set(false),
                    on_confirm: move |_| {
                        confirm_optimize.set(false);
                        start_optimize();
                    },
                    div { class: "modal-list",
                        for item in eligible_items.iter() {
                            div { key: "{item.category}:{item.label}",
                                div { class: "fw-semibold", "[{item.category}] {item.label}" }
                                if !item.detail.is_empty() {
                                    div { class: "text-muted", "{item.detail}" }
                                }
                            }
                        }
                    }
                }
            }

            if !*is_elevated.read() {
                div { class: "card",
                    div { class: "card-header",
                        div {
                            h3 { class: "card-title", if en { "Admin Rights Required" } else { "Se requiere administrador" } }
                            p { class: "card-subtitle", if en { "Elevate to modify services, registry & power." } else { "Eleva para ajustar registro, servicios y energía." } }
                        }
                        button {
                            class: "btn btn-primary",
                            onclick: on_relaunch_admin,
                            if en { "Relaunch as Admin" } else { "Ejecutar como administrador" }
                        }
                    }
                }
            }

            div { class: "cards-grid",
                Card {
                    title: overview_title,
                    subtitle: overview_sub,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2); font-size: var(--font-size-sm);",
                        p { strong { "CPU: " } "{hw.cpu_name} ({hw.logical_cores}T)" }
                        p { strong { if en { "Board: " } else { "Placa: " } } "{hw.motherboard}" }
                        p { strong { "RAM: " } "{hw.total_ram_mb} MB" }
                        for gpu in hw.gpu_names.iter() {
                            p { strong { "GPU: " } "{gpu}" }
                        }
                        for drive in hw.storage_drives.iter() {
                            p { strong { if en { "Disk: " } else { "Disco: " } } "{drive}" }
                        }
                        div { class: "mt-1",
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
                    div { class: "stack-2",
                        code { style: "font-size: var(--font-size-xs); background: var(--bg); padding: var(--space-2); border-radius: var(--radius-sm); word-break: break-all;",
                            "{active_scheme.read()}"
                        }
                        p { class: "text-xs text-muted",
                            if en { "Auto-selects optimal plan for desktop or laptop." } else { "Selecciona el plan óptimo según PC o portátil." }
                        }
                    }
                }

                Card {
                    title: opt_title,
                    subtitle: opt_sub,
                    div { class: "stack-3",
                        button {
                            class: "btn btn-primary",
                            disabled: *is_running.read() || eligible_items.is_empty(),
                            onclick: move |_| confirm_optimize.set(true),
                            if *is_running.read() {
                                if en { "Processing…" } else { "Procesando…" }
                            } else if en {
                                "Optimize System"
                            } else {
                                "Optimizar sistema"
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

            if let Some(err) = error_banner.read().as_ref().map(|m| m.text(en)) {
                StatusBanner { text: err, tone: Tone::Danger }
            }

            if *is_running.read() {
                Card {
                    title: active_action_label.read().text(en),
                    subtitle: Some(format!("{}/{} — {}", prog.current_step, prog.total_steps.max(1), prog.current_label)),
                    div { class: "stack-2",
                        div { class: "text-xs text-muted", "{prog.percent}%" }
                        ProgressBar { percent: prog.percent }
                    }
                }
            }

            // UI-01: el resultado del rollback muestra también fallos, omisiones y si se conservó el respaldo.
            if let Some(rb) = last_rollback.read().as_ref() {
                {
                    let clean = rb.failed_items.is_empty() && rb.skipped_user_modified == 0;
                    let (title, tone) = match (clean, en) {
                        (true, true) => ("Rollback completed", Tone::Success),
                        (true, false) => ("Rollback completado", Tone::Success),
                        (false, true) => ("Rollback incomplete", Tone::Warning),
                        (false, false) => ("Rollback incompleto", Tone::Warning),
                    };
                    let backup_note = match (rb.backup_deleted, en) {
                        (true, true) => "The backup was deleted: nothing is left to revert.",
                        (true, false) => "Se borró el respaldo: no queda nada por revertir.",
                        (false, true) => "The backup was kept so you can retry what is pending.",
                        (false, false) => "Se conservó el respaldo para reintentar lo pendiente.",
                    };
                    rsx! {
                        Card {
                            title: title.to_string(),
                            subtitle: Some(backup_note.to_string()),
                            div { style: "display: flex; gap: var(--space-3); flex-wrap: wrap;",
                                span { class: "badge badge-success", if en { "Registry: {rb.restored_registry}" } else { "Registro: {rb.restored_registry}" } }
                                span { class: "badge badge-success", if en { "Services: {rb.restored_services}" } else { "Servicios: {rb.restored_services}" } }
                                span { class: "badge badge-success", if en { "Power: {rb.restored_power}" } else { "Energía: {rb.restored_power}" } }
                                if rb.skipped_user_modified > 0 {
                                    span { class: "badge badge-warning",
                                        if en { "Kept (changed by you later): {rb.skipped_user_modified}" } else { "Conservados (los cambiaste después): {rb.skipped_user_modified}" }
                                    }
                                }
                                if !rb.failed_items.is_empty() {
                                    span { class: "badge badge-danger",
                                        if en { "Failed: {rb.failed_items.len()}" } else { "Con error: {rb.failed_items.len()}" }
                                    }
                                }
                            }
                            if !rb.failed_items.is_empty() {
                                StatusBanner { text: rb.failed_items.join("\n"), tone }
                            }
                        }
                    }
                }
            }

            if let Some(rep) = last_report.read().as_ref() {
                Card {
                    title: if en { format!("Results — {}", rep.profile_name) } else { format!("Resultados — {}", rep.profile_name) },
                    subtitle: Some(if en {
                        format!("Applied: {} • Skipped: {} • Errors: {}", rep.applied_count, rep.skipped_count, rep.failed_items.len())
                    } else {
                        format!("Aplicados: {} • Omitidos: {} • Errores: {}", rep.applied_count, rep.skipped_count, rep.failed_items.len())
                    }),
                    div { class: "stack-2",
                        // UI-01: fallos y advertencias del motor, no solo los contadores.
                        if !rep.failed_items.is_empty() {
                            StatusBanner { text: rep.failed_items.join("\n"), tone: Tone::Danger }
                        }
                        if !rep.warnings.is_empty() {
                            StatusBanner { text: rep.warnings.join("\n"), tone: Tone::Warning }
                        }
                        for step in rep.step_results.iter() {
                            div {
                                key: "{step.category}:{step.label}",
                                class: "list-row",
                                div {
                                    div { class: "text-sm fw-semibold", "[{step.category}] {step.label}" }
                                    if !step.detail.is_empty() {
                                        div { class: "text-xs text-muted", "{step.detail}" }
                                    }
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
                    div { class: "stack-2",
                        for item in planned_items.iter() {
                            div {
                                key: "{item.category}:{item.label}",
                                class: "list-row",
                                // UI-12: cada ajuste explica qué hace.
                                div {
                                    div { class: "text-sm fw-semibold", "[{item.category}] {item.label}" }
                                    if !item.detail.is_empty() {
                                        div { class: "text-xs text-muted", "{item.detail}" }
                                    }
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
