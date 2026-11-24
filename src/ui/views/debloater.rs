use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::debloater::{
    DebloatCategory, DebloatProgress, DebloatReport, DebloatScanResult, DebloaterManager,
    DetectedBloatApp, DEBLOAT_REG_TWEAKS,
};
use crate::ui::components::Card;
use crate::ui::i18n::Lang;

fn apply_recommended_defaults(
    scan: &DebloatScanResult,
    mut selected_apps: Signal<HashSet<&'static str>>,
    mut selected_regs: Signal<HashSet<&'static str>>,
) {
    let apps: HashSet<&'static str> = scan
        .apps
        .iter()
        .filter(|a| a.installed && a.default_selected)
        .map(|a| a.id)
        .collect();
    let regs: HashSet<&'static str> = DEBLOAT_REG_TWEAKS
        .iter()
        .filter(|r| r.default_selected)
        .map(|r| r.id)
        .collect();
    selected_apps.set(apps);
    selected_regs.set(regs);
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
    let mut status_banner = use_signal(|| Option::<String>::None);
    let mut error_banner = use_signal(|| Option::<String>::None);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));

    let run_scan = move || {
        is_scanning.set(true);
        error_banner.set(None);
        spawn(async move {
            let res = tokio::task::spawn_blocking(DebloaterManager::scan_system)
                .await
                .map_err(|e| format!("Thread error: {}", e))
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
    let on_execute = move |_| {
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

            let res = handle
                .await
                .unwrap_or_else(|e| Err(format!("Thread error: {}", e)));

            match res {
                Ok(rep) => {
                    status_banner.set(Some(format!(
                        "OK — Apps: {} | Reg: {} | Err: {}",
                        rep.removed_apps, rep.applied_reg_tweaks, rep.failed_count
                    )));
                    last_report.set(Some(rep));
                    if let Ok(Ok(fresh)) = tokio::task::spawn_blocking(DebloaterManager::scan_system).await {
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
                .map_err(|e| format!("Thread error: {}", e))
                .and_then(|r| r.map_err(|e| e.to_string()));

            match res {
                Ok(count) => {
                    status_banner.set(Some(if en {
                        format!("Reverted {} registry values.", count)
                    } else {
                        format!("Revertidos {} valores de registro.", count)
                    }));
                    if let Ok(Ok(fresh)) = tokio::task::spawn_blocking(DebloaterManager::scan_system).await {
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

    let debloat_title = if en { "Windows Debloater" } else { "Debloater de Windows" }.to_string();
    let debloat_sub = Some((if en { "Remove UWP bloatware & ads (Store/Winget safe)" } else { "Elimina bloatware UWP y anuncios (Store protegida)" }).to_string());
    let prog_title = if en { "Debloat Progress" } else { "Progreso" }.to_string();
    let rep_title = if en { "Summary" } else { "Resumen" }.to_string();
    let tweaks_title = if en { "Privacy & Ad-Block Registry Tweaks" } else { "Ajustes Anti-Publicidad (Reversibles)" }.to_string();

    rsx! {
        div { class: "content-body",
            Card {
                title: debloat_title,
                subtitle: debloat_sub,
                div { style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); flex-wrap: wrap;",
                    div { style: "display: flex; gap: var(--space-2); flex-wrap: wrap;",
                        button {
                            class: "btn btn-danger",
                            disabled: busy || total_selected == 0,
                            onclick: on_execute,
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
                        style: "width: 100%; height: 10px; background: var(--bg); border-radius: 999px; overflow: hidden; border: 1px solid var(--border);",
                        div { style: "height: 100%; width: {prog.percent}%; background: var(--accent); transition: width 0.2s ease;" }
                    }
                }
            }

            if let Some(msg) = status_banner.read().as_ref() {
                div { class: "badge badge-success", style: "padding: var(--space-3);", "{msg}" }
            }

            if let Some(err) = error_banner.read().as_ref() {
                div { class: "badge badge-danger", style: "padding: var(--space-3);", "Error: {err}" }
            }

            if let Some(rep) = last_report.read().as_ref() {
                Card {
                    title: rep_title,
                    subtitle: None,
                    div { style: "display: flex; flex-direction: column; gap: 4px;",
                        for item in rep.outcomes.iter() {
                            div {
                                key: "{item.label}",
                                style: "display: flex; justify-content: space-between; align-items: center; padding: 4px 0; border-bottom: 1px solid var(--border); font-size: var(--font-size-xs);",
                                span { "{item.label}" }
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
                div { style: "display: flex; flex-direction: column; gap: 6px;",
                    for spec in DEBLOAT_REG_TWEAKS {
                        {
                            let tid = spec.id;
                            let checked = selected_regs.read().contains(tid);
                            rsx! {
                                label {
                                    key: "{tid}",
                                    style: "display: flex; align-items: center; gap: var(--space-3); padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--radius); cursor: pointer;",
                                    input {
                                        r#type: "checkbox",
                                        checked: checked,
                                        disabled: busy,
                                        onchange: move |evt| {
                                            let mut s = selected_regs.write();
                                            if evt.checked() { s.insert(tid); } else { s.remove(tid); }
                                        },
                                    }
                                    span { style: "font-size: var(--font-size-sm); font-weight: 600;", "{spec.label}" }
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
                            div { style: "display: flex; flex-direction: column; gap: 6px;",
                                for app in scan.apps.iter().filter(|a| a.category == cat) {
                                    {
                                        let app_id = app.id;
                                        let checked = app.installed && selected_apps.read().contains(app_id);
                                        rsx! {
                                            label {
                                                key: "{app_id}",
                                                style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--radius); cursor: pointer;",
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
                                                        span { style: "font-size: 11px; color: var(--text-muted);", "({app.package_name})" }
                                                    }
                                                }
                                                span {
                                                    class: if app.installed { "badge badge-warning" } else { "badge badge-success" },
                                                    if app.installed {
                                                        if en { "Installed" } else { "Instalada" }
                                                    } else if en {
                                                        "Removed"
                                                    } else {
                                                        "Eliminada"
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