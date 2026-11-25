use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;

use crate::core::store::app_store::AppStoreManager;
use crate::core::uninstall::{
    InstalledApp, LeftoverItem, LeftoverKind, PurgeReport, UninstallOutcome, UninstallerManager,
};
use crate::ui::components::{Card, ConfirmDialog, LoadingState, Modal, StatusBanner, Tone};
use crate::ui::format::bytes_mb;
use crate::ui::i18n::Lang;
use crate::ui::installer::{
    refresh_tools_inventory, InstallJobRequest, InstallerDispatcher, ToolsSharedState,
};
use crate::ui::task::run_blocking;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortMode {
    NameAsc,
    NameDesc,
    SizeDesc,
    SizeAsc,
}

/// Mensaje de estado con su tono (UI-02).
type Status = Option<(String, Tone)>;

/// Tamaño de una app o un resto; 0 significa «desconocido», no «vacío».
fn format_size_mb(bytes: u64) -> String {
    if bytes == 0 {
        "—".to_string()
    } else {
        bytes_mb(bytes)
    }
}

fn spawn_fill_missing_sizes(
    apps_list: Vec<InstalledApp>,
    mut extra_sizes: Signal<HashMap<String, u64>>,
) {
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
    mut status_msg: Signal<Status>,
    mut selected_leftovers: Signal<HashSet<String>>,
    mut leftovers_modal: Signal<Option<(InstalledApp, Vec<LeftoverItem>)>>,
) {
    is_working.set(true);
    status_msg.set(Some((
        if en {
            format!("Scanning leftovers for '{}'...", app.display_name)
        } else {
            format!("Escaneando rastros de '{}'...", app.display_name)
        },
        Tone::Info,
    )));

    spawn(async move {
        let app_clone = app.clone();
        let res = run_blocking(move || UninstallerManager::scan_leftovers(&app_clone)).await;

        match res {
            Err(err) => status_msg.set(Some((
                if en {
                    format!("Could not scan '{}': {}", app.display_name, err)
                } else {
                    format!("No se pudo escanear '{}': {}", app.display_name, err)
                },
                Tone::Danger,
            ))),
            Ok(found) if found.is_empty() => status_msg.set(Some((
                if en {
                    format!("No leftovers found for '{}'.", app.display_name)
                } else {
                    format!("Sin rastros residuales de '{}'.", app.display_name)
                },
                Tone::Success,
            ))),
            Ok(found) => {
                selected_leftovers.set(UninstallerManager::recommended_ids(&found));
                status_msg.set(Some((
                    if en {
                        format!(
                            "Found {} leftover(s) for '{}'.",
                            found.len(),
                            app.display_name
                        )
                    } else {
                        format!(
                            "{} rastro(s) detectados de '{}'.",
                            found.len(),
                            app.display_name
                        )
                    },
                    Tone::Info,
                )));
                leftovers_modal.set(Some((app, found)));
            }
        }
        is_working.set(false);
    });
}

#[allow(
    clippy::too_many_arguments,
    reason = "señales de la vista; se agrupan en CL-14"
)]
fn start_uninstall_and_scan(
    app: InstalledApp,
    en: bool,
    mut is_working: Signal<bool>,
    mut purge_summary: Signal<Option<PurgeReport>>,
    mut status_msg: Signal<Status>,
    apps: Signal<Vec<InstalledApp>>,
    extra_sizes: Signal<HashMap<String, u64>>,
    mut selected_leftovers: Signal<HashSet<String>>,
    mut leftovers_modal: Signal<Option<(InstalledApp, Vec<LeftoverItem>)>>,
    tools_state: Signal<ToolsSharedState>,
) {
    is_working.set(true);
    purge_summary.set(None);
    status_msg.set(Some((
        if en {
            format!("Uninstalling '{}'...", app.display_name)
        } else {
            format!("Desinstalando '{}'...", app.display_name)
        },
        Tone::Info,
    )));

    spawn(async move {
        let app_run = app.clone();
        let res = run_blocking(move || UninstallerManager::uninstall_and_scan(&app_run)).await;

        let name = app.display_name.clone();
        let (text, tone) = match res {
            Err(err) => (
                if en {
                    format!("Could not uninstall '{}': {}", name, err)
                } else {
                    format!("No se pudo desinstalar '{}': {}", name, err)
                },
                Tone::Danger,
            ),
            Ok(UninstallOutcome::Failed { code }) => {
                (
                    if en {
                        format!("The uninstaller of '{}' ended with code {}; leftovers were not scanned.", name, code)
                    } else {
                        format!("El desinstalador de '{}' terminó con el código {}; no se buscaron restos.", name, code)
                    },
                    Tone::Danger,
                )
            }
            Ok(UninstallOutcome::StillRegistered) => (
                if en {
                    format!("'{}' is still installed; its uninstaller may need a restart or your input.", name)
                } else {
                    format!("'{}' sigue instalada; su desinstalador puede necesitar un reinicio o tu intervención.", name)
                },
                Tone::Warning,
            ),
            Ok(UninstallOutcome::Removed {
                reboot_required,
                leftovers,
            }) => {
                let reboot_note = match (reboot_required, en) {
                    (false, _) => "",
                    (true, true) => " Restart Windows to finish.",
                    (true, false) => " Reinicia Windows para terminar.",
                };
                match leftovers {
                    Err(err) => (
                        if en {
                            format!(
                                "'{}' uninstalled, but its leftovers could not be scanned: {}",
                                name, err
                            )
                        } else {
                            format!(
                                "'{}' desinstalada, pero no se pudieron buscar sus restos: {}",
                                name, err
                            )
                        },
                        Tone::Warning,
                    ),
                    Ok(found) if found.is_empty() => (
                        if en {
                            format!("'{}' uninstalled; no leftovers found.{}", name, reboot_note)
                        } else {
                            format!(
                                "'{}' desinstalada; no se encontraron restos.{}",
                                name, reboot_note
                            )
                        },
                        Tone::Success,
                    ),
                    Ok(found) => {
                        selected_leftovers.set(UninstallerManager::recommended_ids(&found));
                        leftovers_modal.set(Some((app, found)));
                        (
                            if en {
                                format!(
                                    "'{}' uninstalled; review its leftovers.{}",
                                    name, reboot_note
                                )
                            } else {
                                format!(
                                    "'{}' desinstalada; revisa sus restos.{}",
                                    name, reboot_note
                                )
                            },
                            Tone::Success,
                        )
                    }
                }
            }
        };

        reload_apps(apps, extra_sizes, tools_state).await;
        status_msg.set(Some((text, tone)));
        is_working.set(false);
    });
}

/// Relee el inventario; si falla, conserva la lista anterior.
async fn reload_apps(
    mut apps: Signal<Vec<InstalledApp>>,
    extra_sizes: Signal<HashMap<String, u64>>,
    tools_state: Signal<ToolsSharedState>,
) {
    let refreshed = tokio::task::spawn_blocking(UninstallerManager::list_installed_apps)
        .await
        .ok()
        .and_then(|r| r.ok());
    if let Some(list) = refreshed {
        apps.set(list.clone());
        spawn_fill_missing_sizes(list, extra_sizes);
    }
    refresh_tools_inventory(tools_state);
}

#[component]
pub fn UninstallerView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    // UI-05: el inventario se lee fuera del hilo de UI.
    let mut apps = use_signal(Vec::<InstalledApp>::new);
    let mut apps_loaded = use_signal(|| false);
    let extra_sizes = use_signal(HashMap::<String, u64>::new);
    let mut search_query = use_signal(String::new);
    let mut sort_mode = use_signal(|| SortMode::NameAsc);
    let mut is_working = use_signal(|| false);
    let mut status_msg = use_signal(|| Status::None);
    let mut confirm_uninstall = use_signal(|| Option::<InstalledApp>::None);
    let mut leftovers_modal = use_signal(|| Option::<(InstalledApp, Vec<LeftoverItem>)>::None);
    let mut selected_leftovers = use_signal(HashSet::<String>::new);
    let mut purge_summary = use_signal(|| Option::<PurgeReport>::None);
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let dispatcher = use_context::<InstallerDispatcher>();

    let mut load_apps = move || {
        is_working.set(true);
        spawn(async move {
            let res = run_blocking(UninstallerManager::list_installed_apps).await;
            match res {
                Ok(list) => {
                    apps.set(list.clone());
                    spawn_fill_missing_sizes(list, extra_sizes);
                }
                Err(err) => status_msg.set(Some((
                    if en {
                        format!("Could not read the installed apps: {}", err)
                    } else {
                        format!("No se pudieron leer las apps instaladas: {}", err)
                    },
                    Tone::Danger,
                ))),
            }
            apps_loaded.set(true);
            refresh_tools_inventory(tools_state);
            is_working.set(false);
        });
    };

    use_hook(load_apps);

    let on_refresh = move |_| load_apps();

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
            .unwrap_or_else(|e| PurgeReport {
                failed_count: 1,
                errors: vec![e.to_string()],
                ..Default::default()
            });

            reload_apps(apps, extra_sizes, tools_state).await;

            let tone = if report.failed_count > 0 {
                Tone::Warning
            } else {
                Tone::Success
            };
            status_msg.set(Some((
                if en {
                    format!(
                        "Cleaned '{}': {} removed, {} failed ({} freed).",
                        app.display_name,
                        report.removed_count,
                        report.failed_count,
                        format_size_mb(report.bytes_freed)
                    )
                } else {
                    format!(
                        "Purga de '{}': {} eliminados, {} con error ({} liberados).",
                        app.display_name,
                        report.removed_count,
                        report.failed_count,
                        format_size_mb(report.bytes_freed)
                    )
                },
                tone,
            )));
            purge_summary.set(Some(report));
            is_working.set(false);
        });
    };

    let extra_map = extra_sizes.read();
    let effective_kb = |a: &InstalledApp| -> Option<u64> {
        a.estimated_size_kb
            .or_else(|| extra_map.get(&a.id).copied())
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
            filtered.sort_by_key(|a| a.display_name.to_lowercase());
        }
        SortMode::NameDesc => {
            filtered.sort_by_key(|a| std::cmp::Reverse(a.display_name.to_lowercase()));
        }
        SortMode::SizeDesc => {
            filtered.sort_by(|a, b| match (effective_kb(a), effective_kb(b)) {
                (Some(sa), Some(sb)) => sb.cmp(&sa),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a
                    .display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase()),
            });
        }
        SortMode::SizeAsc => {
            filtered.sort_by(|a, b| match (effective_kb(a), effective_kb(b)) {
                (Some(sa), Some(sb)) => sa.cmp(&sb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a
                    .display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase()),
            });
        }
    }

    let locked_title = if en {
        "Locked items"
    } else {
        "Elementos en uso"
    }
    .to_string();
    let uninstaller_title = if en {
        "Deep Uninstaller"
    } else {
        "Desinstalador Profundo"
    }
    .to_string();
    let uninstaller_sub = Some(
        (if en {
            "Uninstall apps and purge leftover files & registry"
        } else {
            "Desinstala y purga archivos y registro residual"
        })
        .to_string(),
    );
    let installed_title = if en {
        "Installed Applications"
    } else {
        "Aplicaciones Instaladas"
    }
    .to_string();

    let search_placeholder = if en {
        "Search app or publisher..."
    } else {
        "Buscar aplicación o fabricante..."
    };
    let refresh_lbl = if en { "Refresh" } else { "Actualizar" };
    let sort_lbl = if en { "Sort:" } else { "Orden:" };
    let sort_largest_lbl = if en {
        "Size (Largest)"
    } else {
        "Tamaño (Mayor)"
    };
    let sort_smallest_lbl = if en {
        "Size (Smallest)"
    } else {
        "Tamaño (Menor)"
    };
    let update_lbl = if en { "Update" } else { "Actualizar" };
    let scan_lbl = if en {
        "Scan Leftovers"
    } else {
        "Escanear Rastros"
    };
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
                    let modal_sub = if en {
                        "Only the items marked as safe are preselected. Review the rest before purging."
                    } else {
                        "Solo vienen marcados los restos seguros. Revisa el resto antes de eliminar."
                    };
                    let warn_lbl = if en {
                        "Files and folders are deleted permanently (not to the Recycle Bin). Registry keys are exported first to ProgramData\\OtterSwiss\\RegBackups."
                    } else {
                        "Los archivos y carpetas se borran de forma permanente (no van a la Papelera). Las claves de registro se exportan antes a ProgramData\\OtterSwiss\\RegBackups."
                    };
                    let review_lbl = if en { "Review" } else { "Revisar" };
                    let close_lbl = if en { "Close" } else { "Cerrar" };
                    let purge_lbl = if en {
                        format!("Delete permanently ({})", sel_count)
                    } else {
                        format!("Borrar de forma permanente ({})", sel_count)
                    };

                    rsx! {
                        Modal {
                            title: modal_title,
                            subtitle: Some(modal_sub.to_string()),
                            tone: Tone::Danger,
                            wide: true,
                            on_close: move |_| leftovers_modal.set(None),
                                StatusBanner { text: warn_lbl.to_string(), tone: Tone::Warning }

                                div { class: "modal-list",
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
                                                    style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-2); border-bottom: 1px solid var(--border); cursor: pointer; font-size: var(--font-size-xs);",
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
                                                        div { class: "min-w-0",
                                                            div { class: "fw-semibold",
                                                                "[{badge_lbl}] {item.category} "
                                                                if !item.recommended {
                                                                    span { class: "badge badge-warning", "{review_lbl}" }
                                                                }
                                                            }
                                                            div { style: "color: var(--text-muted); word-break: break-all;", "{item.target_display}" }
                                                        }
                                                    }
                                                    span { class: "badge badge-info", "{format_size_mb(item.size_bytes)}" }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "row-between gap-3 wrap",
                                    span { class: "text-xs text-muted",
                                        "{sel_count} • {format_size_mb(total_sel_bytes)}"
                                    }
                                    div { class: "modal-actions",
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

            // UX-01: desinstalar pide confirmación y avisa que corre en modo silencioso.
            if let Some(target) = confirm_uninstall.read().clone() {
                {
                    let title = if en {
                        format!("Uninstall '{}'?", target.display_name)
                    } else {
                        format!("¿Desinstalar '{}'?", target.display_name)
                    };
                    let body = if en {
                        "Its uninstaller will run silently, without asking for confirmation. Afterwards you can review its leftovers before deleting anything."
                    } else {
                        "Su desinstalador se ejecutará en modo silencioso, sin pedir confirmación. Después podrás revisar sus restos antes de borrar nada."
                    };
                    rsx! {
                        ConfirmDialog {
                            title,
                            subtitle: None,
                            confirm_label: uninst_lbl.to_string(),
                            cancel_label: if en { "Cancel".to_string() } else { "Cancelar".to_string() },
                            tone: Tone::Danger,
                            on_cancel: move |_| confirm_uninstall.set(None),
                            on_confirm: move |_| {
                                confirm_uninstall.set(None);
                                start_uninstall_and_scan(
                                    target.clone(),
                                    en,
                                    is_working,
                                    purge_summary,
                                    status_msg,
                                    apps,
                                    extra_sizes,
                                    selected_leftovers,
                                    leftovers_modal,
                                    tools_state,
                                );
                            },
                            p { class: "text-xs", "{body}" }
                        }
                    }
                }
            }

            if let Some((msg, tone)) = status_msg.read().clone() {
                StatusBanner { text: msg, tone }
            }

            if let Some(rep) = purge_summary.read().as_ref() {
                if !rep.errors.is_empty() {
                    Card {
                        title: locked_title,
                        subtitle: None,
                        div { style: "font-size: var(--font-size-xs); color: var(--danger-text); display: flex; flex-direction: column; gap: var(--space-1);",
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
                div { class: "stack-3",
                    div { class: "row gap-3 wrap",
                        input {
                            r#type: "text",
                            placeholder: "{search_placeholder}",
                            value: "{search_query.read()}",
                            style: "flex: 1; min-width: 220px; padding: var(--space-2) var(--space-3); border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg); color: var(--text);",
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

                    div { class: "row wrap",
                        span { class: "text-xs text-muted", "{sort_lbl}" }
                        button {
                            class: "btn btn-secondary",
                            "aria-pressed": if *sort_mode.read() == SortMode::NameAsc { "true" } else { "false" },
                            style: "padding: var(--space-1) var(--space-3); font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::NameAsc),
                            "A - Z"
                        }
                        button {
                            class: "btn btn-secondary",
                            "aria-pressed": if *sort_mode.read() == SortMode::NameDesc { "true" } else { "false" },
                            style: "padding: var(--space-1) var(--space-3); font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::NameDesc),
                            "Z - A"
                        }
                        button {
                            class: "btn btn-secondary",
                            "aria-pressed": if *sort_mode.read() == SortMode::SizeDesc { "true" } else { "false" },
                            style: "padding: var(--space-1) var(--space-3); font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::SizeDesc),
                            "{sort_largest_lbl}"
                        }
                        button {
                            class: "btn btn-secondary",
                            "aria-pressed": if *sort_mode.read() == SortMode::SizeAsc { "true" } else { "false" },
                            style: "padding: var(--space-1) var(--space-3); font-size: var(--font-size-xs);",
                            onclick: move |_| sort_mode.set(SortMode::SizeAsc),
                            "{sort_smallest_lbl}"
                        }
                    }
                }
            }

            Card {
                title: installed_title,
                subtitle: None,
                div { class: "stack-2",
                    if !*apps_loaded.read() {
                        LoadingState {
                            message: if en { "Reading installed apps...".to_string() } else { "Leyendo las apps instaladas...".to_string() },
                        }
                    }
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
                                            span { class: "fw-semibold", "{app.display_name}" }
                                            if !app.display_version.is_empty() {
                                                span { class: "text-xs text-muted",
                                                    "v{app.display_version}"
                                                }
                                            }
                                            if let Some(newer) = update_opt {
                                                span { class: "badge badge-warning",
                                                    if en { "Update: v{newer.version}" } else { "Actualización: v{newer.version}" }
                                                }
                                            }
                                        }
                                        div { class: "text-xs text-muted mt-1",
                                            if !app.publisher.is_empty() {
                                                "{app.publisher} • "
                                            }
                                            "{size_lbl} • "
                                            {if app.is_per_user { "HKCU" } else { "HKLM" }}
                                        }
                                    }

                                    div { class: "row",
                                        if let Some(newer) = update_opt {
                                            button {
                                                class: "btn btn-primary btn-sm",
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
                                            onclick: move |_| confirm_uninstall.set(Some(app_for_uninst.clone())),
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
