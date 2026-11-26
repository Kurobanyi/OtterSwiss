use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;

use crate::core::store::app_store::AppStoreManager;
use crate::core::uninstall::{
    InstalledApp, LeftoverItem, LeftoverKind, PurgeReport, UninstallOutcome, UninstallerManager,
};
use crate::ui::components::{Card, ConfirmDialog, LoadingState, Modal, StatusBanner, Tone};
use crate::ui::format::bytes_mb;
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, plural, strings};
use crate::ui::installer::{
    InstallJobRequest, InstallerDispatcher, ToolsSharedState, refresh_tools_inventory,
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
type Status = Option<(Msg, Tone)>;

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

/// CL-14: señales de la vista que comparten el escaneo y la desinstalación. Son `Copy`.
#[derive(Clone, Copy)]
struct UninstallerSignals {
    is_working: Signal<bool>,
    purge_summary: Signal<Option<PurgeReport>>,
    status_msg: Signal<Status>,
    apps: Signal<Vec<InstalledApp>>,
    extra_sizes: Signal<HashMap<String, u64>>,
    selected_leftovers: Signal<HashSet<String>>,
    leftovers_modal: Signal<Option<(InstalledApp, Vec<LeftoverItem>)>>,
    tools_state: Signal<ToolsSharedState>,
}

fn start_scan(app: InstalledApp, sig: UninstallerSignals) {
    let UninstallerSignals {
        mut is_working,
        mut status_msg,
        mut selected_leftovers,
        mut leftovers_modal,
        ..
    } = sig;
    is_working.set(true);
    let name = app.display_name.clone();
    status_msg.set(Some((
        Msg::new(move |en| (strings(en).uninstaller.scanning_leftovers_for)(&name)),
        Tone::Info,
    )));

    spawn(async move {
        let name = app.display_name.clone();
        let app_clone = app.clone();
        let res = run_blocking(move || UninstallerManager::scan_leftovers(&app_clone)).await;

        match res {
            Err(err) => status_msg.set(Some((
                Msg::new(move |en| (strings(en).uninstaller.could_not_scan)(&name, &err.text(en))),
                Tone::Danger,
            ))),
            Ok(found) if found.is_empty() => status_msg.set(Some((
                Msg::new(move |en| (strings(en).uninstaller.no_leftovers_found_for)(&name)),
                Tone::Success,
            ))),
            Ok(found) => {
                selected_leftovers.set(UninstallerManager::recommended_ids(&found));
                let count = found.len();
                status_msg.set(Some((
                    Msg::new(move |en| {
                        let leftovers = plural(count, strings(en).leftover);
                        (strings(en).uninstaller.found_for)(&leftovers, &name)
                    }),
                    Tone::Info,
                )));
                leftovers_modal.set(Some((app, found)));
            }
        }
        is_working.set(false);
    });
}

fn start_uninstall_and_scan(app: InstalledApp, sig: UninstallerSignals) {
    let UninstallerSignals {
        mut is_working,
        mut purge_summary,
        mut status_msg,
        apps,
        extra_sizes,
        mut selected_leftovers,
        mut leftovers_modal,
        tools_state,
    } = sig;
    is_working.set(true);
    purge_summary.set(None);
    let name = app.display_name.clone();
    status_msg.set(Some((
        Msg::new(move |en| (strings(en).uninstaller.uninstalling)(&name)),
        Tone::Info,
    )));

    spawn(async move {
        let app_run = app.clone();
        let res = run_blocking(move || UninstallerManager::uninstall_and_scan(&app_run)).await;

        let name = app.display_name.clone();
        let (text, tone) = match res {
            Err(err) => (
                Msg::new(move |en| {
                    (strings(en).uninstaller.could_not_uninstall)(&name, &err.text(en))
                }),
                Tone::Danger,
            ),
            Ok(UninstallOutcome::Failed { code }) => (
                Msg::new(move |en| {
                    (strings(en).uninstaller.the_uninstaller_of_ended_with)(&name, &code)
                }),
                Tone::Danger,
            ),
            Ok(UninstallOutcome::StillRegistered) => (
                Msg::new(move |en| {
                    (strings(en).uninstaller.is_still_installed_its_uninstaller)(&name)
                }),
                Tone::Warning,
            ),
            Ok(UninstallOutcome::Removed {
                reboot_required,
                leftovers,
            }) => {
                let reboot_note = move |en: bool| match (reboot_required, en) {
                    (false, _) => "",
                    (true, true) => " Restart Windows to finish.",
                    (true, false) => " Reinicia Windows para terminar.",
                };
                match leftovers {
                    Err(err) => (
                        {
                            let err = Msg::plain(err);
                            Msg::new(move |en| {
                                (strings(en).uninstaller.uninstalled_but_its_leftovers_could)(
                                    &name,
                                    &err.text(en),
                                )
                            })
                        },
                        Tone::Warning,
                    ),
                    Ok(found) if found.is_empty() => (
                        Msg::new(move |en| {
                            (strings(en).uninstaller.uninstalled_no_leftovers_found)(
                                &name,
                                &reboot_note(en),
                            )
                        }),
                        Tone::Success,
                    ),
                    Ok(found) => {
                        selected_leftovers.set(UninstallerManager::recommended_ids(&found));
                        leftovers_modal.set(Some((app, found)));
                        (
                            Msg::new(move |en| {
                                (strings(en).uninstaller.uninstalled_review_its_leftovers)(
                                    &name,
                                    &reboot_note(en),
                                )
                            }),
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
    let txt = strings(en).uninstaller;

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
    let sig = UninstallerSignals {
        is_working,
        purge_summary,
        status_msg,
        apps,
        extra_sizes,
        selected_leftovers,
        leftovers_modal,
        tools_state,
    };
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
                    Msg::new(move |en| {
                        (strings(en).uninstaller.could_not_read_the_installed)(&err.text(en))
                    }),
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
                Msg::new(move |en| {
                    (strings(en).uninstaller.cleaned_removed_failed_freed)(
                        &app.display_name,
                        &report.removed_count,
                        &report.failed_count,
                        &format_size_mb(report.bytes_freed),
                    )
                }),
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
    // CL-16: referencias a la lista, no una copia de todas las apps en cada render.
    let apps_guard = apps.read();
    let mut filtered: Vec<&InstalledApp> = apps_guard
        .iter()
        .filter(|a| {
            q.is_empty()
                || a.display_name.to_ascii_lowercase().contains(&q)
                || a.publisher.to_ascii_lowercase().contains(&q)
        })
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

    let locked_title = txt.locked_items.to_string();
    let uninstaller_title = txt.deep_uninstaller.to_string();
    let uninstaller_sub = Some(txt.uninstall_apps_and_purge_leftover.to_string());
    let installed_title = txt.installed_applications.to_string();

    let search_placeholder = txt.search_app_or_publisher;
    let refresh_lbl = strings(en).reload_list;
    let sort_lbl = txt.sort;
    let sort_largest_lbl = txt.size_largest;
    let sort_smallest_lbl = txt.size_smallest;
    let update_lbl = txt.check_for_update;
    let scan_lbl = txt.scan_leftovers;
    let uninst_lbl = strings(en).uninstall;

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

                    let modal_title = (strings(en).uninstaller.leftovers_for)(&modal_app.display_name);
                    let modal_sub = txt.only_the_items_marked_as;
                    let warn_lbl = txt.files_and_folders_are_deleted;
                    let review_lbl = txt.review;
                    let close_lbl = strings(en).close;
                    let purge_lbl = (strings(en).uninstaller.delete_permanently)(&sel_count);

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
                                                    class: "list-row padded clickable text-xs",
                                                    div { class: "row min-w-0",
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
                                                                "[{badge_lbl}] {tr(&item.category, en)} "
                                                                if !item.recommended {
                                                                    span { class: "badge badge-warning", "{review_lbl}" }
                                                                }
                                                            }
                                                            div { class: "text-muted break-all", "{item.target_display}" }
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
                    let title = (strings(en).uninstaller.uninstall)(&target.display_name);
                    let body = txt.its_uninstaller_will_run_silently;
                    rsx! {
                        ConfirmDialog {
                            title,
                            subtitle: None,
                            confirm_label: uninst_lbl.to_string(),
                            cancel_label: strings(en).cancel.to_string(),
                            tone: Tone::Danger,
                            on_cancel: move |_| confirm_uninstall.set(None),
                            on_confirm: move |_| {
                                confirm_uninstall.set(None);
                                start_uninstall_and_scan(target.clone(), sig);
                            },
                            p { class: "text-xs", "{body}" }
                        }
                    }
                }
            }

            if let Some((msg, tone)) = status_msg.read().clone() {
                StatusBanner { text: msg.text(en), tone }
            }

            if let Some(rep) = purge_summary.read().as_ref() {
                if !rep.errors.is_empty() {
                    Card {
                        title: locked_title,
                        subtitle: None,
                        div { class: "stack-1 text-xs text-danger",
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
                            class: "input grow-wide",
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
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if *sort_mode.read() == SortMode::NameAsc { "true" } else { "false" },
                            onclick: move |_| sort_mode.set(SortMode::NameAsc),
                            "A - Z"
                        }
                        button {
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if *sort_mode.read() == SortMode::NameDesc { "true" } else { "false" },
                            onclick: move |_| sort_mode.set(SortMode::NameDesc),
                            "Z - A"
                        }
                        button {
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if *sort_mode.read() == SortMode::SizeDesc { "true" } else { "false" },
                            onclick: move |_| sort_mode.set(SortMode::SizeDesc),
                            "{sort_largest_lbl}"
                        }
                        button {
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if *sort_mode.read() == SortMode::SizeAsc { "true" } else { "false" },
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
                            message: txt.reading_installed_apps.to_string(),
                        }
                    }
                    for app in filtered.into_iter() {
                        {
                            let app_for_uninst = app.clone();
                            let app_for_scan = app.clone();
                            let size_lbl = effective_kb(app)
                                .map(|kb| format_size_mb(kb.saturating_mul(1024)))
                                .unwrap_or_else(|| "—".to_string());

                            let update_opt = AppStoreManager::winget_package_for(&app.display_name);
                            let tx = dispatcher.tx.clone();

                            rsx! {
                                div {
                                    key: "{app.id}",
                                    class: "list-row wrap",
                                    div { class: "grow-wide",
                                        div { class: "row wrap",
                                            span { class: "fw-semibold", "{app.display_name}" }
                                            if !app.display_version.is_empty() {
                                                span { class: "text-xs text-muted",
                                                    "v{app.display_version}"
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
                                                class: "btn btn-secondary btn-sm",
                                                // winget actualiza si hay versión nueva o responde «ya está al día».
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
                                            onclick: move |_| start_scan(app_for_scan.clone(), sig),
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
