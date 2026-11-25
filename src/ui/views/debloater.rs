use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::debloat::{
    DebloatCategory, DebloatProgress, DebloatReport, DebloatScanResult, DebloaterManager,
    DetectedBloatApp, DEBLOAT_REG_TWEAKS,
};
use crate::ui::components::{Card, ConfirmDialog, StatusBanner, Tone};
use crate::ui::i18n::Lang;

fn apply_recommended_defaults(
    scan: &DebloatScanResult,
    mut selected_apps: Signal<HashSet<&'static str>>,
    mut selected_regs: Signal<HashSet<&'static str>>,
) {
    let (apps, regs) = scan.recommended_selection();
    selected_apps.set(apps);
    selected_regs.set(regs);
}

/// UI-02 / UI-03: el aviso refleja fallos y cancelación, no solo que la tarea terminó.
fn report_banner(rep: &DebloatReport, en: bool) -> (String, Tone) {
    let counts = if en {
        format!(
            "{} apps removed, {} tweaks applied, {} errors",
            rep.removed_apps, rep.applied_reg_tweaks, rep.failed_count
        )
    } else {
        format!(
            "{} apps quitadas, {} ajustes aplicados, {} errores",
            rep.removed_apps, rep.applied_reg_tweaks, rep.failed_count
        )
    };
    if rep.cancelled {
        let text = if en {
            format!("Cancelled. Done before cancelling: {}.", counts)
        } else {
            format!("Cancelado. Hecho antes de cancelar: {}.", counts)
        };
        (text, Tone::Warning)
    } else if rep.failed_count > 0 {
        let text = if en {
            format!("Finished with errors: {}. See the summary.", counts)
        } else {
            format!("Terminado con errores: {}. Mira el resumen.", counts)
        };
        (text, Tone::Danger)
    } else {
        let text = if en {
            format!("Done: {}.", counts)
        } else {
            format!("Listo: {}.", counts)
        };
        (text, Tone::Success)
    }
}

#[component]
pub fn DebloaterView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut scan_result = use_signal(DebloatScanResult::default);
    let mut selected_apps = use_signal(HashSet::<&'static str>::new);
    let mut selected_regs = use_signal(HashSet::<&'static str>::new);
    let mut is_scanning = use_signal(|| true);
    let mut is_running = use_signal(|| false);
    let mut progress_state = use_signal(DebloatProgress::default);
    let mut last_report = use_signal(|| Option::<DebloatReport>::None);
    let mut status_banner = use_signal(|| Option::<(String, Tone)>::None);
    let mut confirm_open = use_signal(|| false);
    let mut error_banner = use_signal(|| Option::<String>::None);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));

    let run_scan = move || {
        is_scanning.set(true);
        error_banner.set(None);
        spawn(async move {
            let res = tokio::task::spawn_blocking(DebloaterManager::scan_system)
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));

            match res {
                Ok(data) => {
                    apply_recommended_defaults(&data, selected_apps, selected_regs);
                    scan_result.set(data);
                }
                Err(e) => error_banner.set(Some(e)),
            }
            is_scanning.set(false);
        });
    };

    use_hook(move || {
        let mut r = run_scan;
        r();
    });

    let cancel_for_exec = Arc::clone(&cancel_flag);
    let mut start_execute = move || {
        let current_scan = scan_result.read().clone();
        let sel_app_ids = selected_apps.read().clone();
        let apps_to_remove: Vec<DetectedBloatApp> = current_scan
            .apps
            .into_iter()
            .filter(|a| a.installed && sel_app_ids.contains(a.id))
            .collect();

        let sel_reg_ids: Vec<&'static str> = DEBLOAT_REG_TWEAKS
            .iter()
            .filter(|r| selected_regs.read().contains(r.id))
            .map(|r| r.id)
            .collect();

        if apps_to_remove.is_empty() && sel_reg_ids.is_empty() {
            return;
        }

        cancel_for_exec.store(false, Ordering::Release);
        is_running.set(true);
        error_banner.set(None);
        status_banner.set(None);
        last_report.set(None);
        progress_state.set(DebloatProgress {
            current_step: 0,
            total_steps: (apps_to_remove.len() + sel_reg_ids.len()).max(1),
            current_label: if en { "Starting..." } else { "Iniciando..." }.to_string(),
            percent: 2,
        });

        let prog_arc = Arc::new(Mutex::new(DebloatProgress::default()));
        let prog_worker = Arc::clone(&prog_arc);
        let cancel_worker = Arc::clone(&cancel_for_exec);

        spawn(async move {
            let handle = tokio::task::spawn_blocking(move || {
                DebloaterManager::execute_debloat(
                    &apps_to_remove,
                    &sel_reg_ids,
                    &cancel_worker,
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
                tokio::time::sleep(Duration::from_millis(150)).await;
            }

            let res = handle.await.unwrap_or_else(|e| Err(e.to_string()));

            match res {
                Ok(rep) => {
                    status_banner.set(Some(report_banner(&rep, en)));
                    last_report.set(Some(rep));
                    if let Ok(Ok(fresh)) =
                        tokio::task::spawn_blocking(DebloaterManager::scan_system).await
                    {
                        apply_recommended_defaults(&fresh, selected_apps, selected_regs);
                        scan_result.set(fresh);
                    }
                }
                Err(e) => error_banner.set(Some(e)),
            }
            is_running.set(false);
        });
    };

    let on_revert_reg = move |_| {
        is_running.set(true);
        error_banner.set(None);
        spawn(async move {
            let res = tokio::task::spawn_blocking(DebloaterManager::revert_registry_debloat)
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));

            match res {
                Ok(count) => {
                    status_banner.set(Some((
                        if en {
                            format!("Reverted {} registry values.", count)
                        } else {
                            format!("Revertidos {} valores de registro.", count)
                        },
                        Tone::Success,
                    )));
                    if let Ok(Ok(fresh)) =
                        tokio::task::spawn_blocking(DebloaterManager::scan_system).await
                    {
                        scan_result.set(fresh);
                    }
                }
                Err(e) => error_banner.set(Some(e)),
            }
            is_running.set(false);
        });
    };

    let scan = scan_result.read();
    let total_selected = selected_apps.read().len() + selected_regs.read().len();
    let installed_count = scan.apps.iter().filter(|a| a.installed).count();
    let prog = progress_state.read();
    let cancel_for_btn = Arc::clone(&cancel_flag);
    let busy = *is_running.read() || *is_scanning.read();

    let categories = [
        DebloatCategory::Recommended,
        DebloatCategory::OptionalApps,
        DebloatCategory::XboxGaming,
    ];

    let debloat_title = if en {
        "Windows Debloater"
    } else {
        "Debloater de Windows"
    }
    .to_string();
    let debloat_sub = Some(
        (if en {
            "Remove preinstalled apps and ads (Store and Winget stay protected)"
        } else {
            "Quita apps preinstaladas y anuncios (la Store y Winget quedan protegidas)"
        })
        .to_string(),
    );
    let prog_title = if en { "Debloat Progress" } else { "Progreso" }.to_string();
    let rep_title = if en { "Summary" } else { "Resumen" }.to_string();
    let tweaks_title = if en {
        "Privacy & Ad-Block Registry Tweaks"
    } else {
        "Ajustes Anti-Publicidad (Reversibles)"
    }
    .to_string();

    // SYS-20: la confirmación muestra la lista concreta y qué significa quitar para todos los usuarios.
    let confirm_apps: Vec<&'static str> = scan
        .apps
        .iter()
        .filter(|a| a.installed && selected_apps.read().contains(a.id))
        .map(|a| a.display_name)
        .collect();
    let confirm_regs: Vec<&'static str> = DEBLOAT_REG_TWEAKS
        .iter()
        .filter(|r| selected_regs.read().contains(r.id))
        .map(|r| r.label)
        .collect();

    rsx! {
        div { class: "content-body",
            if *confirm_open.read() {
                ConfirmDialog {
                    title: if en { "Apply the debloat?".to_string() } else { "¿Aplicar el Debloater?".to_string() },
                    subtitle: Some(if en {
                        "Apps are removed for every user of this PC and from the Windows image. You can reinstall them from Microsoft Store. Registry tweaks can be reverted.".to_string()
                    } else {
                        "Las apps se quitan para todos los usuarios de este equipo y de la imagen de Windows. Se pueden reinstalar desde Microsoft Store. Los ajustes de registro se pueden revertir.".to_string()
                    }),
                    confirm_label: if en { format!("Apply ({})", total_selected) } else { format!("Aplicar ({})", total_selected) },
                    cancel_label: if en { "Cancel".to_string() } else { "Cancelar".to_string() },
                    tone: Tone::Danger,
                    wide: true,
                    on_cancel: move |_| confirm_open.set(false),
                    on_confirm: move |_| {
                        confirm_open.set(false);
                        start_execute();
                    },
                    div { class: "modal-list",
                        for name in confirm_apps.iter() {
                            div { key: "app-{name}", "• {name}" }
                        }
                        for label in confirm_regs.iter() {
                            div { key: "reg-{label}", "• {label}" }
                        }
                    }
                }
            }

            Card {
                title: debloat_title,
                subtitle: debloat_sub,
                div { style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); flex-wrap: wrap;",
                    div { style: "display: flex; gap: var(--space-2); flex-wrap: wrap;",
                        button {
                            class: "btn btn-danger",
                            disabled: busy || total_selected == 0,
                            onclick: move |_| confirm_open.set(true),
                            if *is_running.read() {
                                if en { "Running..." } else { "Ejecutando..." }
                            } else if en {
                                "Apply ({total_selected})"
                            } else {
                                "Aplicar ({total_selected})"
                            }
                        }
                        if *is_running.read() {
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| cancel_for_btn.store(true, Ordering::Release),
                                if en { "Cancel" } else { "Cancelar" }
                            }
                        } else {
                            button {
                                class: "btn btn-secondary",
                                disabled: busy,
                                onclick: move |_| {
                                    let s = scan_result.read().clone();
                                    apply_recommended_defaults(&s, selected_apps, selected_regs);
                                },
                                if en { "Recommended" } else { "Recomendados" }
                            }
                            button {
                                class: "btn btn-secondary",
                                disabled: busy,
                                onclick: move |_| {
                                    selected_apps.write().clear();
                                    selected_regs.write().clear();
                                },
                                if en { "Clear" } else { "Limpiar" }
                            }
                            button {
                                class: "btn btn-secondary",
                                disabled: busy,
                                onclick: move |_| {
                                    let mut r = run_scan;
                                    r();
                                },
                                if en { "Rescan" } else { "Re-escanear" }
                            }
                            if scan.has_reg_backup {
                                button {
                                    class: "btn btn-secondary",
                                    disabled: busy,
                                    onclick: on_revert_reg,
                                    if en { "Revert Registry" } else { "Revertir Registro" }
                                }
                            }
                        }
                    }

                    span { class: "badge badge-info",
                        if *is_scanning.read() {
                            if en { "Scanning..." } else { "Escaneando..." }
                        } else if en {
                            "{installed_count} found"
                        } else {
                            "{installed_count} detectadas"
                        }
                    }
                }
            }

            if *is_running.read() {
                Card {
                    title: prog_title,
                    subtitle: Some(format!("{}/{} — {}", prog.current_step, prog.total_steps.max(1), prog.current_label)),
                    div {
                        style: "width: 100%; height: var(--space-2); background: var(--bg); border-radius: 999px; overflow: hidden; border: 1px solid var(--border);",
                        div { style: "height: 100%; width: {prog.percent}%; background: var(--accent); transition: width 0.2s ease;" }
                    }
                }
            }

            if let Some((msg, tone)) = status_banner.read().clone() {
                StatusBanner { text: msg, tone }
            }

            if let Some(err) = error_banner.read().clone() {
                StatusBanner { text: format!("Error: {}", err), tone: Tone::Danger }
            }

            if let Some(rep) = last_report.read().as_ref() {
                Card {
                    title: rep_title,
                    subtitle: None,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-1);",
                        for item in rep.outcomes.iter() {
                            div {
                                key: "{item.label}",
                                style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-1) 0; border-bottom: 1px solid var(--border); font-size: var(--font-size-xs);",
                                div {
                                    div { style: "font-weight: 600;", "{item.label}" }
                                    if !item.message.is_empty() {
                                        div { class: "text-muted", "{item.message}" }
                                    }
                                }
                                span {
                                    class: if item.success { "badge badge-success" } else { "badge badge-danger" },
                                    if item.success { "OK" } else { "Error" }
                                }
                            }
                        }
                    }
                }
            }

            Card {
                title: tweaks_title,
                subtitle: None,
                div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                    for spec in DEBLOAT_REG_TWEAKS {
                        {
                            let tid = spec.id;
                            let checked = selected_regs.read().contains(tid);
                            rsx! {
                                label {
                                    key: "{tid}",
                                    style: "display: flex; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); cursor: pointer;",
                                    input {
                                        r#type: "checkbox",
                                        checked: checked,
                                        disabled: busy,
                                        onchange: move |evt| {
                                            let mut s = selected_regs.write();
                                            if evt.checked() { s.insert(tid); } else { s.remove(tid); }
                                        },
                                    }
                                    div {
                                        div { style: "font-size: var(--font-size-sm); font-weight: 600;", "{spec.label}" }
                                        div { class: "text-xs text-muted", "{spec.description}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            for cat in categories {
                {
                    let cat_label = match (cat, en) {
                        (DebloatCategory::Recommended, false) => "Bloatware Común y Telemetría (Recomendado)",
                        (DebloatCategory::Recommended, true) => "Common Bloatware & Telemetry (Recommended)",
                        (DebloatCategory::OptionalApps, false) => "Aplicaciones Integradas Opcionales",
                        (DebloatCategory::OptionalApps, true) => "Optional Built-in Apps",
                        (DebloatCategory::XboxGaming, false) => "Xbox y Gaming (NO marcar si usas Game Pass)",
                        (DebloatCategory::XboxGaming, true) => "Xbox & Gaming (DO NOT check if you use Game Pass)",
                    };
                    rsx! {
                        Card {
                            key: "{cat_label}",
                            title: cat_label.to_string(),
                            subtitle: None,
                            div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                                for app in scan.apps.iter().filter(|a| a.category == cat) {
                                    {
                                        let app_id = app.id;
                                        let checked = app.installed && selected_apps.read().contains(app_id);
                                        rsx! {
                                            label {
                                                key: "{app_id}",
                                                style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); cursor: pointer;",
                                                div { style: "display: flex; align-items: center; gap: var(--space-3);",
                                                    input {
                                                        r#type: "checkbox",
                                                        checked: checked,
                                                        disabled: busy || !app.installed,
                                                        onchange: move |evt| {
                                                            let mut s = selected_apps.write();
                                                            if evt.checked() { s.insert(app_id); } else { s.remove(app_id); }
                                                        },
                                                    }
                                                    div {
                                                        span { style: "font-weight: 600; font-size: var(--font-size-sm);", "{app.display_name} " }
                                                        span { class: "text-xs text-muted", "({app.package_name})" }
                                                        div { class: "text-xs text-muted", "{app.description}" }
                                                    }
                                                }
                                                span {
                                                    class: if app.installed { "badge badge-warning" } else { "badge badge-success" },
                                                    if app.installed {
                                                        if en { "Installed" } else { "Instalada" }
                                                    } else if en {
                                                        "Not installed"
                                                    } else {
                                                        "No instalada"
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
