use dioxus::prelude::*;

use crate::core::hardware::profile::{HardwareProfile, HardwareTier};
use crate::core::optimize::engine::{
    EngineExecutionReport, EngineProgress, OptimizationEngine, OptimizationProfile,
    OptimizationStepStatus, PlannedOptimizationItem, RollbackReport,
};
use crate::core::optimize::power_plan::{
    GUID_BALANCED, GUID_HIGH_PERFORMANCE, GUID_ULTIMATE_PERFORMANCE, get_active_scheme_guid,
};
use crate::core::platform::guid::format_guid;
use crate::core::platform::power::reboot_system_now;
use crate::ui::components::{Card, ConfirmDialog, LoadingState, ProgressBar, StatusBanner, Tone};
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::run_with_progress;
use base64::{Engine as _, engine::general_purpose::STANDARD};

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

/// Memoria en GB con un decimal (la detección la da en MB).
fn ram_gb(mb: u64) -> String {
    format!("{:.1} GB", mb as f64 / 1024.0)
}

/// Nombre del plan de energía si es uno conocido.
fn power_plan_name(guid: &str, en: bool) -> &'static str {
    let txt = strings(en).dashboard;
    let lower = guid.to_lowercase();
    if lower.contains(GUID_BALANCED) {
        txt.power_plan_balanced
    } else if lower.contains(GUID_HIGH_PERFORMANCE) {
        txt.power_plan_high
    } else if lower.contains(GUID_ULTIMATE_PERFORMANCE) {
        txt.power_plan_ultimate
    } else {
        txt.power_plan_other
    }
}

/// TE-31: portada de Inicio. La nutria de la marca, el procesador como dato principal, tres datos rápidos y la acción de
/// optimizar con su explicación.
#[component]
fn HomeHero(
    hw: HardwareProfile,
    is_running: Signal<bool>,
    nothing_to_apply: bool,
    on_optimize: EventHandler<()>,
    on_rollback: EventHandler<()>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    let (tier_class, tier_text) = match hw.tier {
        HardwareTier::HighEnd => ("badge badge-success", txt.tier_high),
        HardwareTier::MidRange => ("badge badge-info", txt.tier_mid),
        HardwareTier::LowEnd => ("badge badge-warning", txt.tier_low),
    };
    let gpu = hw.gpu_names.first().cloned().unwrap_or_default();
    // Logo de 192 px (unos 30 KB), no el PNG de 512 px.
    let otter = use_hook(|| {
        format!(
            "data:image/png;base64,{}",
            STANDARD.encode(include_bytes!("../../../assets/brand/logo-192.png"))
        )
    });
    rsx! {
        section { class: "home-hero",
            div { class: "home-hero-main",
                span { class: "home-kicker", "{txt.your_pc}" }
                h2 { class: "home-cpu", "{hw.cpu_name}" }
                div { class: "home-chips",
                    span { class: "home-chip",
                        span { class: "fluent-icon", "aria-hidden": "true", "\u{E964}" }
                        "{ram_gb(hw.total_ram_mb)}"
                    }
                    if !gpu.is_empty() {
                        span { class: "home-chip",
                            span { class: "fluent-icon", "aria-hidden": "true", "\u{E7F4}" }
                            "{gpu}"
                        }
                    }
                    span { class: "home-chip",
                        span { class: "fluent-icon", "aria-hidden": "true", "\u{E950}" }
                        "{(txt.threads)(&hw.logical_cores)}"
                    }
                    span { class: "{tier_class}", "{tier_text}" }
                }
            }
            img { class: "home-otter", src: "{otter}", alt: "" }
            div { class: "home-action",
                div { class: "stack-1 grow-wide",
                    span { class: "fw-semibold", "{txt.optimizer_title}" }
                    // UI-01: sin «transaccional»; el respaldo permite revertir, pero no garantiza todo o nada.
                    span { class: "text-muted text-sm", "{txt.optimizer_desc}" }
                }
                div { class: "row",
                    button {
                        class: "btn",
                        disabled: *is_running.read(),
                        onclick: move |_| on_rollback.call(()),
                        "{txt.revert_rollback}"
                    }
                    button {
                        class: "btn btn-primary btn-lg",
                        disabled: *is_running.read() || nothing_to_apply,
                        onclick: move |_| on_optimize.call(()),
                        span { class: "fluent-icon", "aria-hidden": "true", "\u{E945}" }
                        if *is_running.read() {
                            "{txt.processing}"
                        } else { "{txt.optimize_system}" }
                    }
                }
            }
        }
    }
}

/// Una celda de la ficha técnica: icono en círculo, nombre y valor.
#[component]
fn SpecCell(glyph: &'static str, label: String, value: String, note: Option<String>) -> Element {
    rsx! {
        div { class: "spec-cell",
            span { class: "fluent-icon dash-icon", "aria-hidden": "true", "{glyph}" }
            div { class: "spec-text",
                span { class: "spec-label", "{label}" }
                span { class: "spec-value", "{value}" }
                if let Some(note) = note {
                    span { class: "spec-note", "{note}" }
                }
            }
        }
    }
}

/// Ficha técnica del equipo en dos columnas: placa, memoria, gráficos, discos y plan de energía.
#[component]
fn SpecSheet(hw: HardwareProfile, active_scheme: Signal<String>) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    let guid = active_scheme.read().clone();
    rsx! {
        div { class: "section-title", "{txt.specs}" }
        div { class: "spec-grid",
            SpecCell {
                glyph: "\u{E950}",
                label: txt.cpu.to_string(),
                value: hw.cpu_name.clone(),
                note: Some((txt.threads)(&hw.logical_cores)),
            }
            SpecCell {
                glyph: "\u{E7F8}",
                label: txt.board.to_string(),
                value: tr(&hw.motherboard, en).to_string(),
            }
            SpecCell {
                glyph: "\u{E964}",
                label: txt.ram.to_string(),
                value: ram_gb(hw.total_ram_mb),
            }
            for (i , gpu) in hw.gpu_names.iter().enumerate() {
                SpecCell {
                    key: "gpu{i}",
                    glyph: "\u{E7F4}",
                    label: txt.graphics.to_string(),
                    value: gpu.clone(),
                }
            }
            for (i , drive) in hw.storage_drives.iter().enumerate() {
                SpecCell {
                    key: "drive{i}",
                    glyph: "\u{EDA2}",
                    label: txt.disk.to_string(),
                    value: drive.clone(),
                }
            }
            SpecCell {
                glyph: "\u{E83F}",
                label: txt.active_power_plan.to_string(),
                value: power_plan_name(&guid, en).to_string(),
                note: Some(txt.auto_selects_optimal_plan_for.to_string()),
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

/// Una fila de ajuste: nombre, qué hace, y su estado a la derecha.
#[component]
fn TweakRow(
    label: String,
    detail: String,
    badge_class: &'static str,
    badge_text: String,
) -> Element {
    rsx! {
        div { class: "item-row dash-row",
            div { class: "grow stack-1",
                div { class: "fw-semibold", "{label}" }
                if !detail.is_empty() {
                    div { class: "text-xs text-muted", "{detail}" }
                }
            }
            span { class: "{badge_class}", "{badge_text}" }
        }
    }
}

/// Resultado de aplicar el perfil: fallos, advertencias y cada paso.
#[component]
fn EngineReportCard(report: EngineExecutionReport) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    rsx! {
        div { class: "section-title", "{(txt.results)(&report.profile_name)}" }
        div { class: "text-muted",
            "{(txt.applied_skipped_errors)(&report.applied_count, &report.skipped_count, &report.failed_items.len())}"
        }
        // UI-01: fallos y advertencias del motor, no solo los contadores.
        if !report.failed_items.is_empty() {
            StatusBanner { text: report.failed_items.join("\n"), tone: Tone::Danger }
        }
        if !report.warnings.is_empty() {
            StatusBanner { text: report.warnings.join("\n"), tone: Tone::Warning }
        }
        div { class: "stack-1",
            for step in report.step_results.iter() {
                TweakRow {
                    key: "{step.category}:{step.label}",
                    label: format!("{}: {}", tr(&step.category, en), tr(&step.label, en)),
                    detail: tr(&step.detail, en).to_string(),
                    badge_class: match step.status {
                        OptimizationStepStatus::Applied => "badge badge-success",
                        OptimizationStepStatus::Skipped => "badge badge-info",
                        OptimizationStepStatus::Failed => "badge badge-danger",
                    },
                    badge_text: match step.status {
                        OptimizationStepStatus::Applied => txt.step_applied,
                        OptimizationStepStatus::Skipped => txt.step_skipped,
                        OptimizationStepStatus::Failed => txt.step_failed,
                    }
                        .to_string(),
                }
            }
        }
    }
}

/// Plan que se aplicaría con el perfil detectado, antes de optimizar, agrupado por categoría.
#[component]
fn PlannedTweaksGroups(items: Vec<PlannedOptimizationItem>, tier: HardwareTier) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dashboard;
    let tier_text = match tier {
        HardwareTier::HighEnd => txt.tier_high,
        HardwareTier::MidRange => txt.tier_mid,
        HardwareTier::LowEnd => txt.tier_low,
    };
    let mut groups: Vec<(String, Vec<&PlannedOptimizationItem>)> = Vec::new();
    for item in items.iter() {
        match groups.iter_mut().find(|(c, _)| *c == item.category) {
            Some((_, v)) => v.push(item),
            None => groups.push((item.category.clone(), vec![item])),
        }
    }
    rsx! {
        div { class: "section-title", "{(txt.planned_tweaks)(&tier_text)}" }
        div { class: "text-muted", "{txt.backed_up_before_applying}" }
        for (category , group) in groups.iter() {
            div { key: "{category}", class: "stack-1",
                div { class: "text-xs fw-semibold text-muted dash-group", "{tr(category, en)}" }
                for item in group.iter() {
                    {
                        let (badge_class, badge_text) = if item.eligible {
                            ("badge badge-success", txt.included)
                        } else {
                            ("badge badge-info", txt.skipped)
                        };
                        rsx! {
                            TweakRow {
                        key: "{item.category}:{item.label}",
                        // UI-12: cada ajuste explica qué hace.
                        label: tr(&item.label, en).to_string(),
                        detail: tr(&item.detail, en).to_string(),
                        badge_class,
                        badge_text: badge_text.to_string(),
                            }
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

            HomeHero {
                hw: hw.clone(),
                is_running: sig.is_running,
                nothing_to_apply: eligible_items.is_empty(),
                on_optimize: move |_| confirm_optimize.set(true),
                on_rollback: move |_| start_rollback(sig, en),
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

            SpecSheet { hw: hw.clone(), active_scheme: sig.active_scheme }

            if let Some(report) = sig.last_report.read().clone() {
                EngineReportCard { report }
            } else {
                PlannedTweaksGroups { items: planned_items, tier: hw.tier }
            }
        }
    }
}
