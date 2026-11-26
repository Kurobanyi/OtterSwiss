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
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::run_with_progress;

#[component]
pub fn DashboardView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let txt = strings(en).dashboard;

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
            strings(en)
                .dashboard
                .applying_system_optimizations
                .to_string()
        }));
        error_banner.set(None);
        last_rollback.set(None);
        progress_state.set(EngineProgress {
            current_step: 0,
            total_steps: 0,
            current_label: txt.creating_backup.to_string(),
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
            strings(en).dashboard.reverting_from_backup.to_string()
        }));
        error_banner.set(None);
        last_report.set(None);
        progress_state.set(EngineProgress {
            current_step: 0,
            total_steps: 0,
            current_label: txt.reading_backup.to_string(),
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
                    message: txt.detecting_hardware.to_string(),
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

    let overview_title = txt.system_overview.to_string();
    let overview_sub = Some(txt.native_win32_detection.to_string());
    let power_title = txt.active_power_plan.to_string();
    let power_sub = Some(txt.current_windows_scheme.to_string());
    let opt_title = txt.optimizer.to_string();
    // UI-01: sin «transaccional»; el respaldo permite revertir, pero no garantiza todo o nada.
    let opt_sub = Some(txt.backs_up_each_value_before.to_string());
    let planned_title = (strings(en).dashboard.planned_tweaks)(&format!("{:?}", hw.tier));
    let planned_sub = Some(txt.backed_up_before_applying.to_string());

    rsx! {
        div { class: "content-body",
            // UI-13: basta cerrar sesión o reiniciar; reiniciar ahora cierra las apps abiertas.
            if *show_reboot_modal.read() {
                ConfirmDialog {
                    title: txt.sign_out_or_restart_to.to_string(),
                    subtitle: Some(txt.some_changes_take_effect_after.to_string()),
                    confirm_label: txt.restart_now.to_string(),
                    cancel_label: strings(en).later.to_string(),
                    tone: Tone::Warning,
                    on_cancel: move |_| show_reboot_modal.set(false),
                    on_confirm: move |_| on_reboot_now(),
                    p { class: "text-xs",
                        "{txt.restarting_closes_every_open_app}"
                    }
                }
            }

            // UX-02: optimizar muestra antes el plan concreto que se va a aplicar.
            if *confirm_optimize.read() {
                ConfirmDialog {
                    title: txt.apply_these_optimizations.to_string(),
                    subtitle: Some(txt.each_value_is_backed_up.to_string()),
                    confirm_label: (strings(en).dashboard.apply)(&eligible_items.len()),
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
                                div { class: "fw-semibold", "[{tr(&item.category, en)}] {tr(&item.label, en)}" }
                                if !item.detail.is_empty() {
                                    div { class: "text-muted", "{tr(&item.detail, en)}" }
                                }
                            }
                        }
                    }
                }
            }

            if !*is_elevated.read() {
                Card {
                    title: txt.admin_rights_required.to_string(),
                    subtitle: Some(txt.elevate_to_modify_services_registry.to_string()),
                    actions: rsx! {
                        button {
                            class: "btn btn-primary",
                            onclick: on_relaunch_admin,
                            "{txt.relaunch_as_admin}"
                        }
                    },
                }
            }

            div { class: "cards-grid",
                Card {
                    title: overview_title,
                    subtitle: overview_sub,
                    div { class: "stack-2 text-sm",
                        p { strong { "CPU: " } "{hw.cpu_name} ({hw.logical_cores}T)" }
                        p { strong { "{txt.board}" } "{hw.motherboard}" }
                        p { strong { "RAM: " } "{hw.total_ram_mb} MB" }
                        for gpu in hw.gpu_names.iter() {
                            p { strong { "GPU: " } "{gpu}" }
                        }
                        for drive in hw.storage_drives.iter() {
                            p { strong { "{txt.disk}" } "{drive}" }
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
                        code { class: "code-box",
                            "{active_scheme.read()}"
                        }
                        p { class: "text-xs text-muted",
                            "{txt.auto_selects_optimal_plan_for}"
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
                                "{txt.processing}"
                            } else { "{txt.optimize_system}" }
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_running.read(),
                            onclick: on_rollback,
                            "{txt.revert_rollback}"
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
                    subtitle: Some(format!("{}/{} — {}", prog.current_step, prog.total_steps.max(1), tr(&prog.current_label, en))),
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
                            div { class: "flex gap-3 wrap",
                                span { class: "badge badge-success", "{(strings(en).dashboard.registry)(&rb.restored_registry)}" }
                                span { class: "badge badge-success", "{(strings(en).dashboard.services)(&rb.restored_services)}" }
                                span { class: "badge badge-success", "{(strings(en).dashboard.power)(&rb.restored_power)}" }
                                if rb.skipped_user_modified > 0 {
                                    span { class: "badge badge-warning",
                                        "{(strings(en).dashboard.kept_changed_by_you_later)(&rb.skipped_user_modified)}"
                                    }
                                }
                                if !rb.failed_items.is_empty() {
                                    span { class: "badge badge-danger",
                                        "{(strings(en).dashboard.failed)(&rb.failed_items.len())}"
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
                    title: (strings(en).dashboard.results)(&rep.profile_name),
                    subtitle: Some((strings(en).dashboard.applied_skipped_errors)(&rep.applied_count, &rep.skipped_count, &rep.failed_items.len())),
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
                                    div { class: "text-sm fw-semibold", "[{tr(&step.category, en)}] {tr(&step.label, en)}" }
                                    if !step.detail.is_empty() {
                                        div { class: "text-xs text-muted", "{tr(&step.detail, en)}" }
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
                                    div { class: "text-sm fw-semibold", "[{tr(&item.category, en)}] {tr(&item.label, en)}" }
                                    if !item.detail.is_empty() {
                                        div { class: "text-xs text-muted", "{tr(&item.detail, en)}" }
                                    }
                                }
                                span {
                                    class: if item.eligible { "badge badge-success" } else { "badge badge-info" },
                                    if item.eligible {
                                        "{txt.included}"
                                    } else { "{txt.skipped}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
