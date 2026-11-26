use dioxus::prelude::*;

use crate::core::hardware::profile::{HardwareProfile, HardwareTier};
use crate::core::optimize::engine::{
    EngineExecutionReport, EngineProgress, OptimizationEngine, OptimizationProfile,
    OptimizationStepStatus, PlannedOptimizationItem, RollbackReport,
};
use crate::core::optimize::power_plan::get_active_scheme_guid;
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::guid::format_guid;
use crate::core::platform::power::reboot_system_now;
use crate::ui::components::{Card, ConfirmDialog, LoadingState, ProgressBar, StatusBanner, Tone};
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::run_with_progress;

/// CL-05: las señales que comparten la vista y sus tarjetas. Son `Copy`.
#[derive(Clone, Copy, PartialEq)]
struct DashboardSignals {
    is_running: Signal<bool>,
    active_action_label: Signal<Msg>,
    show_reboot_modal: Signal<bool>,
    progress_state: Signal<EngineProgress>,
    last_report: Signal<Option<EngineExecutionReport>>,
    last_rollback: Signal<Option<RollbackReport>>,
    error_banner: Signal<Option<Msg>>,
    active_scheme: Signal<String>,
}

/// Vuelve a leer el plan de energía activo (el motor puede haberlo cambiado).
fn refresh_active_scheme(mut active_scheme: Signal<String>) {
    if let Ok(g) = get_active_scheme_guid() {
        active_scheme.set(format_guid(&g));
    }
}

/// Aplica el perfil de optimización detectado, con respaldo previo y progreso.
fn start_optimize(sig: DashboardSignals, en: bool) {
    let DashboardSignals {
        mut is_running,
        mut active_action_label,
        mut show_reboot_modal,
        mut progress_state,
        mut last_report,
        mut last_rollback,
        mut error_banner,
        active_scheme,
    } = sig;
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
        current_label: strings(en).dashboard.creating_backup.to_string(),
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
                refresh_active_scheme(active_scheme);
                if need_reboot {
                    show_reboot_modal.set(true);
                }
            }
            Err(err) => error_banner.set(Some(err)),
        }
        is_running.set(false);
    });
}

/// Revierte lo aplicado desde el respaldo.
fn start_rollback(sig: DashboardSignals, en: bool) {
    let DashboardSignals {
        mut is_running,
        mut active_action_label,
        mut progress_state,
        mut last_report,
        mut last_rollback,
        mut error_banner,
        active_scheme,
        ..
    } = sig;
    is_running.set(true);
    active_action_label.set(Msg::new(|en| {
        strings(en).dashboard.reverting_from_backup.to_string()
    }));
    error_banner.set(None);
    last_report.set(None);
    progress_state.set(EngineProgress {
        current_step: 0,
        total_steps: 0,
        current_label: strings(en).dashboard.reading_backup.to_string(),
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
                refresh_active_scheme(active_scheme);
            }
            Err(err) => error_banner.set(Some(err)),
        }
        is_running.set(false);
    });
}

/// UI-13: basta cerrar sesión o reiniciar; reiniciar ahora cierra las apps abiertas.
#[component]
fn RebootConfirm(sig: DashboardSignals) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    let mut show_reboot_modal = sig.show_reboot_modal;
    let mut error_banner = sig.error_banner;
    rsx! {
        ConfirmDialog {
            title: txt.sign_out_or_restart_to.to_string(),
            subtitle: Some(txt.some_changes_take_effect_after.to_string()),
            confirm_label: txt.restart_now.to_string(),
            cancel_label: strings(en).later.to_string(),
            tone: Tone::Warning,
            on_cancel: move |_| show_reboot_modal.set(false),
            on_confirm: move |_| {
                show_reboot_modal.set(false);
                if let Err(e) = reboot_system_now() {
                    error_banner.set(Some(Msg::from(e)));
                }
            },
            p { class: "text-xs", "{txt.restarting_closes_every_open_app}" }
        }
    }
}

/// UX-02: optimizar muestra antes el plan concreto que se va a aplicar.
#[component]
fn OptimizeConfirm(
    items: Vec<PlannedOptimizationItem>,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    rsx! {
        ConfirmDialog {
            title: txt.apply_these_optimizations.to_string(),
            subtitle: Some(txt.each_value_is_backed_up.to_string()),
            confirm_label: (txt.apply)(&items.len()),
            cancel_label: strings(en).cancel.to_string(),
            wide: true,
            on_cancel: move |_| on_cancel.call(()),
            on_confirm: move |_| on_confirm.call(()),
            div { class: "modal-list",
                for item in items.iter() {
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
}

/// Aviso de que faltan permisos de administrador, con el botón para relanzar elevado.
#[component]
fn AdminNotice() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    rsx! {
        Card {
            title: txt.admin_rights_required.to_string(),
            subtitle: Some(txt.elevate_to_modify_services_registry.to_string()),
            actions: rsx! {
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        let _ = ElevationManager::relaunch_as_admin(&[crate::core::cli::ELEVATED]);
                    },
                    "{txt.relaunch_as_admin}"
                }
            },
        }
    }
}

/// CPU, placa, RAM, GPU y discos detectados, con el nivel de hardware.
#[component]
fn OverviewCard(hw: HardwareProfile) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    rsx! {
        Card {
            title: txt.system_overview.to_string(),
            subtitle: Some(txt.native_win32_detection.to_string()),
            div { class: "stack-2 text-sm",
                p { strong { "CPU: " } "{hw.cpu_name} ({hw.logical_cores}T)" }
                p { strong { "{txt.board}" } "{tr(&hw.motherboard, en)}" }
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
    }
}

#[component]
fn PowerPlanCard(active_scheme: Signal<String>) -> Element {
    let txt = strings(use_context::<Signal<Lang>>().read().is_en()).dashboard;
    rsx! {
        Card {
            title: txt.active_power_plan.to_string(),
            subtitle: Some(txt.current_windows_scheme.to_string()),
            div { class: "stack-2",
                code { class: "code-box", "{active_scheme.read()}" }
                p { class: "text-xs text-muted", "{txt.auto_selects_optimal_plan_for}" }
            }
        }
    }
}

/// Botones de optimizar y de revertir.
#[component]
fn OptimizerCard(
    is_running: Signal<bool>,
    nothing_to_apply: bool,
    on_optimize: EventHandler<()>,
    on_rollback: EventHandler<()>,
) -> Element {
    let txt = strings(use_context::<Signal<Lang>>().read().is_en()).dashboard;
    rsx! {
        Card {
            title: txt.optimizer.to_string(),
            // UI-01: sin «transaccional»; el respaldo permite revertir, pero no garantiza todo o nada.
            subtitle: Some(txt.backs_up_each_value_before.to_string()),
            div { class: "stack-3",
                button {
                    class: "btn btn-primary",
                    disabled: *is_running.read() || nothing_to_apply,
                    onclick: move |_| on_optimize.call(()),
                    if *is_running.read() {
                        "{txt.processing}"
                    } else { "{txt.optimize_system}" }
                }
                button {
                    class: "btn btn-secondary",
                    disabled: *is_running.read(),
                    onclick: move |_| on_rollback.call(()),
                    "{txt.revert_rollback}"
                }
            }
        }
    }
}

/// Progreso de la acción en curso.
#[component]
fn ActionProgressCard(sig: DashboardSignals) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let prog = sig.progress_state.read();
    rsx! {
        Card {
            title: sig.active_action_label.read().text(en),
            subtitle: Some(format!("{}/{} — {}", prog.current_step, prog.total_steps.max(1), tr(&prog.current_label, en))),
            div { class: "stack-2",
                div { class: "text-xs text-muted", "{prog.percent}%" }
                ProgressBar { percent: prog.percent }
            }
        }
    }
}

/// UI-01: el resultado del rollback muestra también fallos, omisiones y si se conservó el respaldo.
#[component]
fn RollbackResultCard(rb: RollbackReport) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    let clean = rb.failed_items.is_empty() && rb.skipped_user_modified == 0;
    let (title, tone) = if clean {
        (txt.rollback_completed, Tone::Success)
    } else {
        (txt.rollback_incomplete, Tone::Warning)
    };
    let backup_note = if rb.backup_deleted {
        txt.backup_deleted
    } else {
        txt.backup_kept
    };
    rsx! {
        Card {
            title: title.to_string(),
            subtitle: Some(backup_note.to_string()),
            div { class: "flex gap-3 wrap",
                span { class: "badge badge-success", "{(txt.registry)(&rb.restored_registry)}" }
                span { class: "badge badge-success", "{(txt.services)(&rb.restored_services)}" }
                span { class: "badge badge-success", "{(txt.power)(&rb.restored_power)}" }
                if rb.skipped_user_modified > 0 {
                    span { class: "badge badge-warning",
                        "{(txt.kept_changed_by_you_later)(&rb.skipped_user_modified)}"
                    }
                }
                if !rb.failed_items.is_empty() {
                    span { class: "badge badge-danger", "{(txt.failed)(&rb.failed_items.len())}" }
                }
            }
            if !rb.failed_items.is_empty() {
                StatusBanner { text: rb.failed_items.join("\n"), tone }
            }
        }
    }
}

/// Resultado de aplicar el perfil: fallos, advertencias y cada paso.
#[component]
fn EngineReportCard(report: EngineExecutionReport) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    rsx! {
        Card {
            title: (txt.results)(&report.profile_name),
            subtitle: Some((txt.applied_skipped_errors)(&report.applied_count, &report.skipped_count, &report.failed_items.len())),
            div { class: "stack-2",
                // UI-01: fallos y advertencias del motor, no solo los contadores.
                if !report.failed_items.is_empty() {
                    StatusBanner { text: report.failed_items.join("\n"), tone: Tone::Danger }
                }
                if !report.warnings.is_empty() {
                    StatusBanner { text: report.warnings.join("\n"), tone: Tone::Warning }
                }
                for step in report.step_results.iter() {
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
                            match step.status {
                                OptimizationStepStatus::Applied => txt.step_applied,
                                OptimizationStepStatus::Skipped => txt.step_skipped,
                                OptimizationStepStatus::Failed => "Error",
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Plan que se aplicaría con el perfil detectado, antes de optimizar.
#[component]
fn PlannedTweaksCard(items: Vec<PlannedOptimizationItem>, tier: HardwareTier) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    rsx! {
        Card {
            title: (txt.planned_tweaks)(&format!("{tier:?}")),
            subtitle: Some(txt.backed_up_before_applying.to_string()),
            div { class: "stack-2",
                for item in items.iter() {
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

#[component]
pub fn DashboardView() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();

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
    let mut confirm_optimize = use_signal(|| false);
    let sig = DashboardSignals {
        is_running: use_signal(|| false),
        active_action_label: use_signal(|| Msg::plain("")),
        show_reboot_modal: use_signal(|| false),
        progress_state: use_signal(EngineProgress::default),
        last_report: use_signal(|| Option::<EngineExecutionReport>::None),
        last_rollback: use_signal(|| Option::<RollbackReport>::None),
        error_banner: use_signal(|| Option::<Msg>::None),
        active_scheme: use_signal(|| {
            get_active_scheme_guid()
                .map(|g| format_guid(&g))
                .unwrap_or_else(|_| "N/A".to_string())
        }),
    };

    let hw_guard = hw_profile.read();
    let Some(hw) = hw_guard.as_ref() else {
        return rsx! {
            div { class: "content-body",
                LoadingState { message: strings(en).dashboard.detecting_hardware.to_string() }
            }
        };
    };
    let planned_items = OptimizationEngine::preview_plan(OptimizationProfile::AutoDetected, hw);
    let eligible_items: Vec<_> = planned_items
        .iter()
        .filter(|i| i.eligible)
        .cloned()
        .collect();

    rsx! {
        div { class: "content-body",
            if *sig.show_reboot_modal.read() {
                RebootConfirm { sig }
            }

            if *confirm_optimize.read() {
                OptimizeConfirm {
                    items: eligible_items.clone(),
                    on_cancel: move |_| confirm_optimize.set(false),
                    on_confirm: move |_| {
                        confirm_optimize.set(false);
                        start_optimize(sig, en);
                    },
                }
            }

            if !*is_elevated.read() {
                AdminNotice {}
            }

            div { class: "cards-grid",
                OverviewCard { hw: hw.clone() }
                PowerPlanCard { active_scheme: sig.active_scheme }
                OptimizerCard {
                    is_running: sig.is_running,
                    nothing_to_apply: eligible_items.is_empty(),
                    on_optimize: move |_| confirm_optimize.set(true),
                    on_rollback: move |_| start_rollback(sig, en),
                }
            }

            if let Some(err) = sig.error_banner.read().as_ref().map(|m| m.text(en)) {
                StatusBanner { text: err, tone: Tone::Danger }
            }

            if *sig.is_running.read() {
                ActionProgressCard { sig }
            }

            if let Some(rb) = sig.last_rollback.read().clone() {
                RollbackResultCard { rb }
            }

            if let Some(report) = sig.last_report.read().clone() {
                EngineReportCard { report }
            } else {
                PlannedTweaksCard { items: planned_items, tier: hw.tier }
            }
        }
    }
}
