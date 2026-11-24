// src/ui/views/uninstaller.rs

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::app_store::AppStoreManager;
use crate::core::uninstaller::{
    InstalledApp, LeftoverItem, LeftoverKind, PurgeReport, UninstallerManager,
};
use crate::ui::components::Card;
use crate::ui::i18n::Lang;
use crate::ui::{refresh_tools_inventory, InstallJobRequest, InstallerDispatcher, ToolsSharedState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortMode {
    NameAsc,
    NameDesc,
    SizeDesc,
    SizeAsc,
}

fn format_size_mb(bytes: u64) -> String {
    if bytes == 0 {
        return "—".to_string();
    }
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 0.01 {
        "< 0.01 MB".to_string()
    } else if mb >= 100.0 {
        format!("{:.0} MB", mb)
    } else {
        format!("{:.2} MB", mb)
    }
}

fn spawn_fill_missing_sizes(apps_list: Vec<InstalledApp>, mut extra_sizes: Signal<HashMap<String, u64>>) {
    spawn(async move {
        let computed = tokio::task::spawn_blocking(move || {
            UninstallerManager::estimate_missing_sizes(apps_list)
        })
        .await
        .unwrap_or_default();
        if !computed.is_empty() {
            extra_sizes.set(computed);
        }
    });
}

fn start_scan(
    app: InstalledApp,
    en: bool,
    mut is_working: Signal<bool>,
    mut status_msg: Signal<Option<String>>,
    mut selected_leftovers: Signal<HashSet<String>>,
    mut leftovers_modal: Signal<Option<(InstalledApp, Vec<LeftoverItem>)>>,
) {
    is_working.set(true);
    status_msg.set(Some(if en {
        format!("Scanning leftovers for '{}'...", app.display_name)
    } else {
        format!("Escaneando rastros de '{}'...", app.display_name)
    }));

    spawn(async move {
        let app_clone = app.clone();
        let found = tokio::task::spawn_blocking(move || {
            UninstallerManager::scan_leftovers(&app_clone)
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();

        let ids: HashSet<String> = found.iter().map(|i| i.id.clone()).collect();
        selected_leftovers.set(ids);
        if found.is_empty() {
            status_msg.set(Some(if en {
                format!("No leftovers found for '{}'.", app.display_name)
            } else {
                format!("Sin rastros residuales de '{}'.", app.display_name)
            }));
        } else {
            status_msg.set(Some(if en {
                format!("Found {} leftover(s) for '{}'.", found.len(), app.display_name)
            } else {
                format!("{} rastro(s) detectados de '{}'.", found.len(), app.display_name)
            }));
            leftovers_modal.set(Some((app, found)));
        }
        is_working.set(false);
    });
}

fn start_uninstall_and_scan(
    app: InstalledApp,
    en: bool,
    mut is_working: Signal<bool>,
    mut purge_summary: Signal<Option<PurgeReport>>,
    mut status_msg: Signal<Option<String>>,
    mut apps: Signal<Vec<InstalledApp>>,
    extra_sizes: Signal<HashMap<String, u64>>,
    mut selected_leftovers: Signal<HashSet<String>>,
    mut leftovers_modal: Signal<Option<(InstalledApp, Vec<LeftoverItem>)>>,
    tools_state: Signal<ToolsSharedState>,
) {
    is_working.set(true);
    purge_summary.set(None);
    status_msg.set(Some(if en {
        format!("Uninstalling '{}'...", app.display_name)
    } else {
        format!("Desinstalando '{}'...", app.display_name)
    }));

    spawn(async move {
        let app_run = app.clone();
        let res = tokio::task::spawn_blocking(move || {
            UninstallerManager::run_uninstaller(&app_run, Duration::from_secs(300))
        })
        .await
        .map_err(|e| format!("Thread error: {}", e))
        .and_then(|r| r.map_err(|e| e.to_string()));

        match res {
            Ok(_) => {
                let app_scan = app.clone();
                let found = tokio::task::spawn_blocking(move || {
                    UninstallerManager::scan_leftovers(&app_scan)
                })
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();

                let refreshed = tokio::task::spawn_blocking(UninstallerManager::list_installed_apps)
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or_default();
                apps.set(refreshed.clone());
                spawn_fill_missing_sizes(refreshed, extra_sizes);
                refresh_tools_inventory(tools_state);

                if found.is_empty() {
                    status_msg.set(Some(if en {
                        format!("'{}' removed cleanly.", app.display_name)
                    } else {
                        format!("'{}' desinstalado sin rastros.", app.display_name)
                    }));
                } else {
                    let ids: HashSet<String> = found.iter().map(|i| i.id.clone()).collect();
                    selected_leftovers.set(ids);
                    leftovers_modal.set(Some((app, found)));
                }
            }
            Err(err) => {
                status_msg.set(Some(format!("Error '{}': {}", app.display_name, err)));
            }
        }
        is_working.set(false);
    });
}

#[component]
pub fn UninstallerView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut apps = use_signal(|| UninstallerManager::list_installed_apps().unwrap_or_default());
    let extra_sizes = use_signal(HashMap::<String, u64>::new);
    let mut search_query = use_signal(String::new);
    let mut sort_mode = use_signal(|| SortMode::NameAsc);
    let mut is_working = use_signal(|| false);
    let mut status_msg = use_signal(|| Option::<String>::None);
    let mut leftovers_modal = use_signal(|| Option::<(InstalledApp, Vec<LeftoverItem>)>::None);
    let mut selected_leftovers = use_signal(HashSet::<String>::new);
    let mut purge_summary = use_signal(|| Option::<PurgeReport>::None);
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let dispatcher = use_context::<InstallerDispatcher>();

    use_hook(move || {
        let initial = apps.read().clone();
        spawn_fill_missing_sizes(initial, extra_sizes);
    });

    let on_refresh = move |_| {
        is_working.set(true);
        spawn(async move {
            let list = tokio::task::spawn_blocking(UninstallerManager::list_installed_apps)
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
            apps.set(list.clone());
            spawn_fill_missing_sizes(list, extra_sizes);
            refresh_tools_inventory(tools_state);
            is_working.set(false);
        });
    };

    let on_purge_selected = move |_| {
        let Some((app, items)) = leftovers_modal.read().clone() else {
            return;
        };
        let sel = selected_leftovers.read().clone();
        let to_delete: Vec<LeftoverItem> =
            items.into_iter().filter(|i| sel.contains(&i.id)).collect();
        if to_delete.is_empty() {
            leftovers_modal.set(None);
            return;
        }

        is_working.set(true);
        leftovers_modal.set(None);

        spawn(async move {
            let report = tokio::task::spawn_blocking(move || {
                UninstallerManager::purge_leftover_items(&to_delete)
            })
            .await
            .unwrap_or_default();

            let refreshed = tokio::task::spawn_blocking(UninstallerManager::list_installed_apps)
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
            apps.set(refreshed);
            refresh_tools_inventory(tools_state);

            status_msg.set(Some(if en {
                format!(
                    "Cleaned '{}': {} removed ({} freed).",
                    app.display_name,
                    report.removed_count,
                    format_size_mb(report.bytes_freed)
                )
            } else {
                format!(
                    "Purga de '{}': {} eliminados ({} liberados).",
                    app.display_name,
                    report.removed_count,
                    format_size_mb(report.bytes_freed)
                )
            }));
            purge_summary.set(Some(report));
            is_working.set(false);
        });
    };

    let extra_map = extra_sizes.read();
    let effective_kb = |a: &InstalledApp| -> Option<u64> {
        a.estimated_size_kb.or_else(|| extra_map.get(&a.id).copied())
    };

    let q = search_query.read().trim().to_ascii_lowercase();
    let mut filtered: Vec<InstalledApp> = apps
        .read()
        .iter()
        .filter(|a| {
            q.is_empty()
                || a.display_name.to_ascii_lowercase().contains(&q)
                || a.publisher.to_ascii_lowercase().contains(&q)
        })
        .cloned()
        .collect();

    match *sort_mode.read() {
        SortMode::NameAsc => {
            filtered.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
        }
        SortMode::NameDesc => {
            filtered.sort_by(|a, b| b.display_name.to_lowercase().cmp(&a.display_name.to_lowercase()));
        }
        SortMode::SizeDesc => {
            filtered.sort_by(|a, b| match (effective_kb(a), effective_kb(b)) {
                (Some(sa), Some(sb)) => sb.cmp(&sa),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()),
            });
        }
        SortMode::SizeAsc => {
            filtered.sort_by(|a, b| match (effective_kb(a), effective_kb(b)) {
                (Some(sa), Some(sb)) => sa.cmp(&sb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()),
            });
        }
    }

    let locked_title = if en { "Locked items" } else { "Elementos en uso" }.to_string();
    let uninstaller_title = if en { "Deep Uninstaller" } else { "Desinstalador Profundo" }.to_string();
    let uninstaller_sub = Some((if en { "Uninstall apps and purge leftover files & registry" } else { "Desinstala y purga archivos y registro residual" }).to_string());
    let installed_title = if en { "Installed Applications" } else { "Aplicaciones Instaladas" }.to_string();

    let search_placeholder = if en { "Search app or publisher..." } else { "Buscar aplicación o fabricante..." };
    let refresh_lbl = if en { "Refresh" } else { "Actualizar" };
    let sort_lbl = if en { "Sort:" } else { "Orden:" };
    let sort_largest_lbl = if en { "Size (Largest)" } else { "Tamaño (Mayor)" };
    let sort_smallest_lbl = if en { "Size (Smallest)" } else { "Tamaño (Menor)" };
    let update_lbl = if en { "Update" } else { "Actualizar" };
    let scan_lbl = if en { "Scan Leftovers" } else { "Escanear Rastros" };
    let uninst_lbl = if en { "Uninstall" } else { "Desinstalar" };

    rsx! {
        div { class: "content-body",
            if let Some((ref modal_app, ref found_items)) = *leftovers_modal.read() {
                {
                    let sel_count = selected_leftovers.read().len();
                    let total_sel_bytes: u64 = found_items
                        .iter()
                        .filter(|i| selected_leftovers.read().contains(&i.id))
                        .map(|i| i.size_bytes)
                        .sum();

                    let modal_title = if en {
                        format!("Leftovers for '{}'", modal_app.display_name)
                    } else {
                        format!("Rastros de '{}'", modal_app.display_name)
                    };
                    let modal_sub = if en { "Select items to purge." } else { "Selecciona los restos a eliminar." };
                    let close_lbl = if en { "Close" } else { "Cerrar" };
                    let purge_lbl = if en {
                        format!("Purge ({})", sel_count)
                    } else {
                        format!("Eliminar ({})", sel_count)
                    };

                    rsx! {
                        div {
                            style: "position: fixed; inset: 0; background: rgba(0, 0, 0, 0.75); display: flex; align-items: center; justify-content: center; z-index: 9999; padding: 24px;",
                            div {
                                class: "card",
                                style: "max-width: 720px; width: 100%; border: 1px solid var(--accent);",
                                div { class: "card-header",
                                    div {
                                        h3 { class: "card-title", "{modal_title}" }
                                        p { class: "card-subtitle", "{modal_sub}" }
                                    }
                                }

                                div {
                                    style: "max-height: 340px; overflow-y: auto; border: 1px solid var(--border); border-radius: var(--radius); padding: var(--space-2); background: var(--bg); display: flex; flex-direction: column; gap: 6px;",
                                    for item in found_items.iter() {
                                        {
                                            let item_id = item.id.clone();
                                            let checked = selected_leftovers.read().contains(&item_id);
                                            let badge_lbl = match (item.kind, en) {
                                                (LeftoverKind::Directory, false) => "Carpeta",
                                                (LeftoverKind::Directory, true) => "Folder",
                                                (LeftoverKind::File, false) => "Archivo",
                                                (LeftoverKind::File, true) => "File",
                                                (LeftoverKind::RegistryKey, false) => "Registro",
                                                (LeftoverKind::RegistryKey, true) => "Registry",
                                                (LeftoverKind::RegistryValue, false) => "Inicio",
                                                (LeftoverKind::RegistryValue, true) => "Startup",
                                            };

                                            rsx! {
                                                label {
                                                    key: "{item_id}",
                                                    style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-2); padding: 6px 8px; border-bottom: 1px solid var(--border); cursor: pointer; font-size: var(--font-size-xs);",
                                                    div { style: "display: flex; align-items: center; gap: var(--space-2); min-width: 0;",
                                                        input {
                                                            r#type: "checkbox",
                                                            checked: checked,
                                                            onchange: move |evt| {
                                                                let mut s = selected_leftovers.write();
                                                                if evt.checked() {
                                                                    s.insert(item_id.clone());
                                                                } else {
                                                                    s.remove(&item_id);
                                                                }
                                                            },
                                                        }
                                                        div { style: "min-width: 0;",
                                                            div { style: "font-weight: 600;", "[{badge_lbl}] {item.category}" }
                                                            div { style: "color: var(--text-muted); word-break: break-all;", "{item.target_display}" }
                                                        }
                                                    }
                                                    span { class: "badge badge-info", "{format_size_mb(item.size_bytes)}" }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { style: "display: flex; justify-content: space-between; align-items: center; margin-top: var(--space-3); gap: var(--space-3); flex-wrap: wrap;",
                                    span { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                                        "{sel_count} • {format_size_mb(total_sel_bytes)}"
                                    }
                                    div { style: "display: flex; gap: var(--space-2);",
                                        button {
                                            class: "btn btn-secondary",
                                            onclick: move |_| leftovers_modal.set(None),
                                            "{close_lbl}"
                                        }
                                        button {
                                            class: "btn btn-danger",
                                            disabled: sel_count == 0,
                                            onclick: on_purge_selected,
                                            "{purge_lbl}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(msg) = status_msg.read().as_ref() {
                div { class: "badge badge-info", style: "padding: var(--space-3); font-size: var(--font-size-sm);",
                    "{msg}"
                }
            }

            if let Some(rep) = purge_summary.read().as_ref() {
                if !rep.errors.is_empty() {
                    Card {
                        title: locked_title,
                        subtitle: None,
                        div { style: "font-size: var(--font-size-xs); color: var(--danger); display: flex; flex-direction: column; gap: 4px;",
                            for err in rep.errors.iter() {
                                div { "{err}" }
                            }
                        }
                    }
                }
            }

            Card {
                title: uninstaller_title,
                subtitle: uninstaller_sub,
                div { style: "display: flex; flex-direction: column; gap: var(--space-3);",
                    div { style: "display: flex; gap: var(--space-3); align-items: center; flex-wrap: wrap;",
                        input {
                            r#type: "text",
                            placeholder: "{search_placeholder}",
                            value: "{search_query.read()}",
                            style: "flex: 1; min-width: 220px; padding: 8px 12px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg); color: var(--text);",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                        span { class: "badge badge-info", "{filtered.len()}" }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_working.read(),
                            onclick: on_refresh,
                            "{refresh_lbl}"
                        }
                    }

                    div { style: "display: flex; gap: var(--space-2); align-items: center; flex-wrap: wrap;",
                        span { style: "font-size: var(--font-size-xs); color: var(--text-muted);", "{sort_lbl}" }
                        button {
                            class: if *sort_mode.read() == SortMode::NameAsc { "btn btn-primary" } else { "btn btn-secondary" },
                            style: "padding: 4px 10px; font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::NameAsc),
                            "A - Z"
                        }
                        button {
                            class: if *sort_mode.read() == SortMode::NameDesc { "btn btn-primary" } else { "btn btn-secondary" },
                            style: "padding: 4px 10px; font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::NameDesc),
                            "Z - A"
                        }
                        button {
                            class: if *sort_mode.read() == SortMode::SizeDesc { "btn btn-primary" } else { "btn btn-secondary" },
                            style: "padding: 4px 10px; font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::SizeDesc),
                            "{sort_largest_lbl}"
                        }
                        button {
                            class: if *sort_mode.read() == SortMode::SizeAsc { "btn btn-primary" } else { "btn btn-secondary" },
                            style: "padding: 4px 10px; font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::SizeAsc),
                            "{sort_smallest_lbl}"
                        }
                    }
                }
            }

            Card {
                title: installed_title,
                subtitle: None,
                div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                    for app in filtered.into_iter() {
                        {
                            let app_for_uninst = app.clone();
                            let app_for_scan = app.clone();
                            let size_lbl = effective_kb(&app)
                                .map(|kb| format_size_mb(kb.saturating_mul(1024)))
                                .unwrap_or_else(|| "—".to_string());

                            let update_opt = AppStoreManager::find_newer_curated_version(&app.display_name, &app.display_version);
                            let tx = dispatcher.tx.clone();

                            rsx! {
                                div {
                                    key: "{app.id}",
                                    style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-2) 0; border-bottom: 1px solid var(--border); flex-wrap: wrap;",
                                    div { style: "min-width: 240px; flex: 1;",
                                        div { style: "display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap;",
                                            span { style: "font-weight: 600;", "{app.display_name}" }
                                            if !app.display_version.is_empty() {
                                                span { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                                                    "v{app.display_version}"
                                                }
                                            }
                                            if let Some(newer) = update_opt {
                                                span { class: "badge badge-warning",
                                                    if en { "Update: v{newer.version}" } else { "Actualización: v{newer.version}" }
                                                }
                                            }
                                        }
                                        div { style: "font-size: var(--font-size-xs); color: var(--text-muted); margin-top: 2px;",
                                            if !app.publisher.is_empty() {
                                                "{app.publisher} • "
                                            }
                                            "{size_lbl} • "
                                            {if app.is_per_user { "HKCU" } else { "HKLM" }}
                                        }
                                    }

                                    div { style: "display: flex; gap: var(--space-2); align-items: center;",
                                        if let Some(newer) = update_opt {
                                            button {
                                                class: "btn btn-primary",
                                                style: "padding: 6px 12px; font-size: var(--font-size-xs);",
                                                disabled: *is_working.read(),
                                                onclick: move |_| {
                                                    let _ = tx.send(InstallJobRequest::InstallAppBatch(vec![newer.id]));
                                                },
                                                "{update_lbl}"
                                            }
                                        }
                                        button {
                                            class: "btn btn-secondary",
                                            disabled: *is_working.read(),
                                            onclick: move |_| start_scan(
                                                app_for_scan.clone(),
                                                en,
                                                is_working,
                                                status_msg,
                                                selected_leftovers,
                                                leftovers_modal,
                                            ),
                                            "{scan_lbl}"
                                        }
                                        button {
                                            class: "btn btn-danger",
                                            disabled: *is_working.read(),
                                            onclick: move |_| start_uninstall_and_scan(
                                                app_for_uninst.clone(),
                                                en,
                                                is_working,
                                                purge_summary,
                                                status_msg,
                                                apps,
                                                extra_sizes,
                                                selected_leftovers,
                                                leftovers_modal,
                                                tools_state,
                                            ),
                                            "{uninst_lbl}"
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