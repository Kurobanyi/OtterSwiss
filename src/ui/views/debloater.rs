use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::core::debloat::{
    self, DEBLOAT_REG_TWEAKS, DebloatCategory, DebloatProgress, DebloatReport, DebloatScanResult,
    DetectedBloatApp,
};
use crate::ui::components::{
    Card, CheckRow, ConfirmDialog, LoadingState, ProgressBar, StatusBanner, Tone,
};
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::{run_blocking, run_with_progress};

/// CL-05: las señales que comparten la vista y sus tarjetas. Son `Copy`.
#[derive(Clone, Copy, PartialEq)]
struct DebloaterSignals {
    scan_result: Signal<DebloatScanResult>,
    selected_apps: Signal<HashSet<&'static str>>,
    selected_regs: Signal<HashSet<&'static str>>,
    is_scanning: Signal<bool>,
    is_running: Signal<bool>,
    progress_state: Signal<DebloatProgress>,
    last_report: Signal<Option<DebloatReport>>,
    status_banner: Signal<Option<(Msg, Tone)>>,
    error_banner: Signal<Option<Msg>>,
    /// UI-24: diálogo para confirmar «Revertir registro».
    confirm_revert: Signal<bool>,
}

impl DebloaterSignals {
    fn busy(&self) -> bool {
        *self.is_running.read() || *self.is_scanning.read()
    }

    /// Marca lo que el catálogo recomienda para esta PC.
    fn apply_recommended_defaults(mut self, scan: &DebloatScanResult) {
        let (apps, regs) = scan.recommended_selection();
        self.selected_apps.set(apps);
        self.selected_regs.set(regs);
    }

    fn selected_count(&self) -> usize {
        self.selected_apps.read().len() + self.selected_regs.read().len()
    }
}

/// UI-02 / UI-03: el aviso refleja fallos y cancelación, no solo que la tarea terminó.
fn report_banner(rep: &DebloatReport, en: bool) -> (String, Tone) {
    let counts = (strings(en).debloater.apps_removed_tweaks_applied_errors)(
        &rep.removed_apps,
        &rep.applied_reg_tweaks,
        &rep.failed_count,
    );
    if rep.cancelled {
        let text = (strings(en).debloater.cancelled_done_before_cancelling)(&counts);
        (text, Tone::Warning)
    } else if rep.failed_count > 0 {
        let text = (strings(en).debloater.finished_with_errors_see_the)(&counts);
        (text, Tone::Danger)
    } else {
        let text = (strings(en).debloater.done)(&counts);
        (text, Tone::Success)
    }
}

/// Escanea el sistema (fuera del hilo de UI) y deja la selección recomendada.
fn start_scan(sig: DebloaterSignals) {
    let DebloaterSignals {
        mut scan_result,
        mut is_scanning,
        mut error_banner,
        ..
    } = sig;
    is_scanning.set(true);
    error_banner.set(None);
    spawn(async move {
        match run_blocking(debloat::scan_system).await {
            Ok(data) => {
                sig.apply_recommended_defaults(&data);
                scan_result.set(data);
            }
            Err(e) => error_banner.set(Some(e)),
        }
        is_scanning.set(false);
    });
}

/// Quita las apps y aplica los ajustes marcados, con progreso y cancelación.
fn start_execute(sig: DebloaterSignals, cancel: Arc<AtomicBool>, en: bool) {
    let DebloaterSignals {
        mut scan_result,
        selected_apps,
        selected_regs,
        mut is_running,
        mut progress_state,
        mut last_report,
        mut status_banner,
        mut error_banner,
        ..
    } = sig;
    let sel_app_ids = selected_apps.read().clone();
    let apps_to_remove: Vec<DetectedBloatApp> = scan_result
        .read()
        .apps
        .iter()
        .filter(|a| a.installed && sel_app_ids.contains(a.id))
        .cloned()
        .collect();

    let sel_reg_ids: Vec<&'static str> = DEBLOAT_REG_TWEAKS
        .iter()
        .filter(|r| selected_regs.read().contains(r.id))
        .map(|r| r.id)
        .collect();

    if apps_to_remove.is_empty() && sel_reg_ids.is_empty() {
        return;
    }

    cancel.store(false, Ordering::Release);
    is_running.set(true);
    error_banner.set(None);
    status_banner.set(None);
    last_report.set(None);
    progress_state.set(DebloatProgress {
        current_step: 0,
        total_steps: (apps_to_remove.len() + sel_reg_ids.len()).max(1),
        current_label: strings(en).starting.to_string(),
        percent: 2,
    });

    spawn(async move {
        let res = run_with_progress(progress_state, move |prog| {
            debloat::execute_debloat(&apps_to_remove, &sel_reg_ids, &cancel, Some(prog))
                .map_err(Msg::from)
        })
        .await
        .and_then(|r| r);

        match res {
            Ok(rep) => {
                let tone = report_banner(&rep, false).1;
                let shown = rep.clone();
                status_banner.set(Some((
                    Msg::new(move |en| report_banner(&shown, en).0),
                    tone,
                )));
                last_report.set(Some(rep));
                if let Ok(Ok(fresh)) = tokio::task::spawn_blocking(debloat::scan_system).await {
                    sig.apply_recommended_defaults(&fresh);
                    scan_result.set(fresh);
                }
            }
            Err(e) => error_banner.set(Some(e)),
        }
        is_running.set(false);
    });
}

/// Revierte los ajustes de registro del Debloater desde su respaldo.
fn start_revert_registry(sig: DebloaterSignals) {
    let DebloaterSignals {
        mut scan_result,
        mut is_running,
        mut status_banner,
        mut error_banner,
        ..
    } = sig;
    is_running.set(true);
    error_banner.set(None);
    spawn(async move {
        match run_blocking(debloat::revert_registry_debloat).await {
            Ok(count) => {
                status_banner.set(Some((
                    Msg::new(move |en| (strings(en).debloater.reverted_registry_values)(&count)),
                    Tone::Success,
                )));
                if let Ok(Ok(fresh)) = tokio::task::spawn_blocking(debloat::scan_system).await {
                    scan_result.set(fresh);
                }
            }
            Err(e) => error_banner.set(Some(e)),
        }
        is_running.set(false);
    });
}

/// SYS-20: la confirmación muestra la lista concreta y qué significa quitar para todos los usuarios.
#[component]
fn DebloatConfirm(
    sig: DebloaterSignals,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).debloater;
    let scan = sig.scan_result.read();
    let apps: Vec<&'static str> = scan
        .apps
        .iter()
        .filter(|a| a.installed && sig.selected_apps.read().contains(a.id))
        .map(|a| a.display_name)
        .collect();
    let regs: Vec<&'static str> = DEBLOAT_REG_TWEAKS
        .iter()
        .filter(|r| sig.selected_regs.read().contains(r.id))
        .map(|r| r.label)
        .collect();

    rsx! {
        ConfirmDialog {
            title: txt.apply_the_debloat.to_string(),
            subtitle: Some(txt.apps_are_removed_for_every.to_string()),
            confirm_label: (txt.apply)(&sig.selected_count()),
            cancel_label: strings(en).cancel.to_string(),
            tone: Tone::Danger,
            wide: true,
            on_cancel: move |_| on_cancel.call(()),
            on_confirm: move |_| on_confirm.call(()),
            div { class: "modal-list",
                for name in apps.iter() {
                    div { key: "app-{name}", "• {name}" }
                }
                for label in regs.iter() {
                    div { key: "reg-{label}", "• {label}" }
                }
            }
        }
    }
}

/// Acción principal arriba (aplicar), selección rápida y contador de lo elegido.
#[component]
fn DebloatActions(
    sig: DebloaterSignals,
    on_apply: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).debloater;
    let DebloaterSignals {
        scan_result,
        mut selected_apps,
        mut selected_regs,
        is_scanning,
        is_running,
        ..
    } = sig;
    let busy = sig.busy();
    let total_selected = sig.selected_count();
    let scan = scan_result.read();
    let installed_count = scan.apps.iter().filter(|a| a.installed).count();
    let nothing = total_selected == 0 && !busy;

    rsx! {
        div { class: "card",
            div { class: "row-between gap-4 wrap",
                div { class: "grow-wide stack-1",
                    div { class: "card-title", "{(txt.selected_count)(&total_selected)}" }
                    div { class: "text-muted",
                        if *is_scanning.read() {
                            "{strings(en).scanning}"
                        } else if nothing {
                            "{txt.nothing_selected}"
                        } else { "{(txt.found)(&installed_count)}" }
                    }
                }
                div { class: "row wrap",
                    if !*is_running.read() {
                        button {
                            class: "btn",
                            disabled: busy,
                            onclick: move |_| sig.apply_recommended_defaults(&scan_result.read()),
                            "{txt.recommended}"
                        }
                        button {
                            class: "btn",
                            disabled: busy || total_selected == 0,
                            onclick: move |_| {
                                selected_apps.write().clear();
                                selected_regs.write().clear();
                            },
                            "{strings(en).clear}"
                        }
                        button {
                            class: "btn",
                            disabled: busy,
                            onclick: move |_| start_scan(sig),
                            "{txt.rescan}"
                        }
                        if scan.has_reg_backup {
                            button {
                                class: "btn",
                                disabled: busy,
                                onclick: move |_| {
                                    let mut confirm = sig.confirm_revert;
                                    confirm.set(true);
                                },
                                "{txt.revert_registry}"
                            }
                        }
                    }
                    button {
                        class: "btn btn-primary",
                        disabled: busy || total_selected == 0,
                        title: if nothing { txt.nothing_selected } else { "" },
                        onclick: move |_| on_apply.call(()),
                        if *is_running.read() {
                            "{strings(en).running}"
                        } else { "{(txt.apply)(&total_selected)}" }
                    }
                    if *is_running.read() {
                        button { class: "btn", onclick: move |_| on_cancel.call(()), "{strings(en).cancel}" }
                    }
                }
            }
        }
    }
}

/// Encabezado de un grupo: título, contador de elegidos, etiqueta de reversibilidad y una nota breve.
#[component]
fn GroupHeading(
    title: String,
    note: String,
    count: String,
    tag: String,
    tag_class: &'static str,
) -> Element {
    rsx! {
        div { class: "stack-1",
            div { class: "row-between gap-3 wrap",
                div { class: "section-title", "{title}" }
                div { class: "row",
                    span { class: "badge {tag_class}", "{tag}" }
                    span { class: "badge badge-info", "{count}" }
                }
            }
            div { class: "text-xs text-muted", "{note}" }
        }
    }
}

/// Resultado de la última ejecución, ítem por ítem.
#[component]
fn DebloatReportCard(report: DebloatReport) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    rsx! {
        div { class: "section-title", "{strings(en).debloater.summary}" }
        div { class: "stack-1",
            for item in report.outcomes.iter() {
                div { key: "{item.label}", class: "item-row row-between gap-3",
                    div { class: "grow stack-1",
                        span { class: "fw-semibold", "{tr(&item.label, en)}" }
                        if !item.message.is_empty() {
                            span { class: "text-xs text-muted", "{item.message}" }
                        }
                    }
                    span {
                        class: if item.success { "badge badge-success" } else { "badge badge-danger" },
                        if item.success { "{strings(en).debloater.result_ok}" } else { "{strings(en).debloater.result_failed}" }
                    }
                }
            }
        }
    }
}

/// Ajustes de privacidad y anuncios (registro): una casilla por ajuste.
#[component]
fn RegistryTweaksCard(sig: DebloaterSignals) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).debloater;
    let mut selected_regs = sig.selected_regs;
    let busy = sig.busy();
    let chosen = selected_regs.read().len();
    rsx! {
        GroupHeading {
            title: txt.privacy_ad_block_registry_tweaks.to_string(),
            note: txt.tweaks_group_note.to_string(),
            count: (txt.selected_of)(&chosen, &DEBLOAT_REG_TWEAKS.len()),
            tag: txt.reversible.to_string(),
            tag_class: "badge-success",
        }
        div { class: "stack-1",
            for spec in DEBLOAT_REG_TWEAKS {
                {
                    let tid = spec.id;
                    let checked = selected_regs.read().contains(tid);
                    rsx! {
                        CheckRow {
                            key: "{tid}",
                            class: "item-row gap-3",
                            checked,
                            disabled: busy,
                            on_change: move |on| {
                                let mut s = selected_regs.write();
                                if on { s.insert(tid); } else { s.remove(tid); }
                            },
                            div { class: "grow stack-1",
                                span { "{tr(&spec.label, en)}" }
                                span { class: "text-xs text-muted", "{tr(&spec.description, en)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Las apps de una categoría; solo se pueden marcar las que están instaladas.
#[component]
fn CategoryCard(cat: DebloatCategory, sig: DebloaterSignals) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).debloater;
    let mut selected_apps = sig.selected_apps;
    let busy = sig.busy();
    let scan = sig.scan_result.read();
    let in_cat = scan.apps.iter().filter(|a| a.category == cat);
    let installed = in_cat.clone().filter(|a| a.installed).count();
    let chosen = in_cat
        .clone()
        .filter(|a| a.installed && selected_apps.read().contains(a.id))
        .count();
    rsx! {
        GroupHeading {
            title: tr(cat.label(), en).into_owned(),
            note: txt.apps_group_note.to_string(),
            count: (txt.selected_of)(&chosen, &installed),
            tag: txt.reinstallable.to_string(),
            tag_class: "badge-warning",
        }
        div { class: "stack-1",
            for app in in_cat {
                {
                    let app_id = app.id;
                    let checked = app.installed && selected_apps.read().contains(app_id);
                    rsx! {
                        CheckRow {
                            key: "{app_id}",
                            class: "item-row gap-3",
                            checked,
                            disabled: busy || !app.installed,
                            on_change: move |on| {
                                let mut s = selected_apps.write();
                                if on { s.insert(app_id); } else { s.remove(app_id); }
                            },
                            div { class: "grow stack-1",
                                span { class: if app.installed { "" } else { "text-muted" },
                                    "{tr(&app.display_name, en)}"
                                }
                                span { class: "text-xs text-muted", "{tr(&app.description, en)}" }
                                span { class: "text-xs text-muted", "{app.package_name}" }
                            }
                            span {
                                class: if app.installed { "badge badge-warning" } else { "badge badge-success" },
                                if app.installed {
                                    "{strings(en).installed_app}"
                                } else { "{txt.not_installed}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn DebloaterView() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    // Sin administrador el escaneo de apps falla siempre y el shell ya lo avisa: el error es una advertencia, no un fallo.
    let is_elevated = use_hook(|| crate::core::platform::elevation::is_elevated().unwrap_or(false));

    let sig = DebloaterSignals {
        scan_result: use_signal(DebloatScanResult::default),
        selected_apps: use_signal(HashSet::<&'static str>::new),
        selected_regs: use_signal(HashSet::<&'static str>::new),
        is_scanning: use_signal(|| true),
        is_running: use_signal(|| false),
        progress_state: use_signal(DebloatProgress::default),
        last_report: use_signal(|| Option::<DebloatReport>::None),
        status_banner: use_signal(|| Option::<(Msg, Tone)>::None),
        error_banner: use_signal(|| Option::<Msg>::None),
        confirm_revert: use_signal(|| false),
    };
    let mut confirm_open = use_signal(|| false);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));
    use_hook(move || start_scan(sig));

    let prog = sig.progress_state.read();
    let categories = [
        DebloatCategory::Recommended,
        DebloatCategory::OptionalApps,
        DebloatCategory::XboxGaming,
    ];

    rsx! {
        div { class: "content-body",
            if *sig.confirm_revert.read() {
                ConfirmDialog {
                    title: strings(en).debloater.confirm_revert_title.to_string(),
                    subtitle: Some(strings(en).debloater.confirm_revert_desc.to_string()),
                    confirm_label: strings(en).debloater.revert_registry.to_string(),
                    cancel_label: strings(en).cancel.to_string(),
                    tone: Tone::Warning,
                    on_cancel: move |_| {
                        let mut confirm = sig.confirm_revert;
                        confirm.set(false);
                    },
                    on_confirm: move |_| {
                        let mut confirm = sig.confirm_revert;
                        confirm.set(false);
                        start_revert_registry(sig);
                    },
                }
            }
            if *confirm_open.read() {
                DebloatConfirm {
                    sig,
                    on_cancel: move |_| confirm_open.set(false),
                    on_confirm: {
                        let cancel = Arc::clone(&cancel_flag);
                        move |_| {
                            confirm_open.set(false);
                            start_execute(sig, Arc::clone(&cancel), en);
                        }
                    },
                }
            }

            DebloatActions {
                sig,
                on_apply: move |_| confirm_open.set(true),
                on_cancel: {
                    let cancel = Arc::clone(&cancel_flag);
                    move |_| cancel.store(true, Ordering::Release)
                },
            }

            if *sig.is_running.read() {
                Card {
                    title: strings(en).debloater.debloat_progress.to_string(),
                    subtitle: Some(format!("{}/{} — {}", prog.current_step, prog.total_steps.max(1), tr(&prog.current_label, en))),
                    ProgressBar { percent: prog.percent }
                }
            }

            if let Some((msg, tone)) = sig.status_banner.read().clone().map(|(m, t)| (m.text(en), t)) {
                StatusBanner { text: msg, tone }
            }

            if let Some(err) = sig.error_banner.read().as_ref().map(|m| m.text(en)) {
                StatusBanner { text: err, tone: if is_elevated { Tone::Danger } else { Tone::Warning } }
            }

            if let Some(report) = sig.last_report.read().clone() {
                DebloatReportCard { report }
            }

            if *sig.is_scanning.read() && sig.scan_result.read().apps.is_empty() {
                LoadingState { message: strings(en).scanning.to_string() }
            } else {
                RegistryTweaksCard { sig }

                for cat in categories {
                    CategoryCard { key: "{cat.label()}", cat, sig }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(cancelled: bool, failed: usize) -> DebloatReport {
        DebloatReport {
            cancelled,
            removed_apps: 2,
            applied_reg_tweaks: 1,
            failed_count: failed,
            outcomes: Vec::new(),
        }
    }

    /// UI-02 / UI-03: el aviso no es verde si hubo fallos o se canceló.
    #[test]
    fn tono_del_aviso_sale_del_informe() {
        assert_eq!(report_banner(&report(false, 0), false).1, Tone::Success);
        assert_eq!(report_banner(&report(false, 1), false).1, Tone::Danger);
        assert_eq!(report_banner(&report(true, 0), false).1, Tone::Warning);
        assert!(report_banner(&report(true, 0), false).0.contains("2 apps"));
    }
}
