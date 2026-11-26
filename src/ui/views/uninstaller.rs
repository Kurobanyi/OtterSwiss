use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;

use crate::core::store::app_store;
use crate::core::uninstall::{
    self, InstalledApp, LeftoverItem, LeftoverKind, PurgeReport, UninstallOutcome,
};
use crate::ui::components::{Card, ConfirmDialog, LoadingState, Modal, StatusBanner, Tone};
use crate::ui::format::bytes_mb;
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::uninstaller::UninstallerStrings;
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

/// CL-14: señales de la vista que comparten el escaneo y la desinstalación. Son `Copy`.
#[derive(Clone, Copy, PartialEq)]
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
        let computed =
            tokio::task::spawn_blocking(move || uninstall::estimate_missing_sizes(apps_list))
                .await
                .unwrap_or_default();
        if !computed.is_empty() {
            extra_sizes.set(computed);
        }
    });
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
        let res = run_blocking(move || uninstall::scan_leftovers(&app_clone)).await;

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
                selected_leftovers.set(uninstall::recommended_ids(&found));
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
        let res = run_blocking(move || uninstall::uninstall_and_scan(&app_run)).await;

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
                let reboot_note = move |en: bool| {
                    if reboot_required {
                        strings(en).uninstaller.restart_to_finish
                    } else {
                        ""
                    }
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
                        selected_leftovers.set(uninstall::recommended_ids(&found));
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
    let refreshed = tokio::task::spawn_blocking(uninstall::list_installed_apps)
        .await
        .ok()
        .and_then(|r| r.ok());
    if let Some(list) = refreshed {
        apps.set(list.clone());
        spawn_fill_missing_sizes(list, extra_sizes);
    }
    refresh_tools_inventory(tools_state);
}

/// Tamaño efectivo en KB: el que declara la app o, si no lo declara, el calculado aparte.
fn effective_kb(app: &InstalledApp, extra_sizes: &HashMap<String, u64>) -> Option<u64> {
    app.estimated_size_kb
        .or_else(|| extra_sizes.get(&app.id).copied())
}

/// Apps que se muestran: las que coinciden con la búsqueda (nombre o editor), en el orden elegido. Devuelve
/// referencias a la lista, no una copia de todas las apps en cada render (CL-16).
fn visible_apps<'a>(
    apps: &'a [InstalledApp],
    extra_sizes: &HashMap<String, u64>,
    query: &str,
    mode: SortMode,
) -> Vec<&'a InstalledApp> {
    let q = query.trim().to_ascii_lowercase();
    let mut list: Vec<&InstalledApp> = apps
        .iter()
        .filter(|a| {
            q.is_empty()
                || a.display_name.to_ascii_lowercase().contains(&q)
                || a.publisher.to_ascii_lowercase().contains(&q)
        })
        .collect();

    let by_name = |a: &&InstalledApp, b: &&InstalledApp| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    };
    match mode {
        SortMode::NameAsc => list.sort_by(by_name),
        SortMode::NameDesc => list.sort_by(|a, b| by_name(b, a)),
        SortMode::SizeDesc | SortMode::SizeAsc => list.sort_by(|a, b| {
            match (effective_kb(a, extra_sizes), effective_kb(b, extra_sizes)) {
                (Some(sa), Some(sb)) if mode == SortMode::SizeDesc => sb.cmp(&sa),
                (Some(sa), Some(sb)) => sa.cmp(&sb),
                // Las apps sin tamaño conocido van al final, ordenadas por nombre.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => by_name(a, b),
            }
        }),
    }
    list
}

/// Etiqueta del tipo de un resto, en el idioma de la interfaz.
fn leftover_kind_label(kind: LeftoverKind, txt: &UninstallerStrings) -> &'static str {
    match kind {
        LeftoverKind::Directory => txt.kind_folder,
        LeftoverKind::File => txt.kind_file,
        LeftoverKind::RegistryKey => txt.kind_registry,
        LeftoverKind::RegistryValue => txt.kind_startup,
    }
}

/// Borra los restos marcados en el diálogo y deja el resumen del resultado.
fn start_purge(sig: UninstallerSignals) {
    let UninstallerSignals {
        mut is_working,
        mut purge_summary,
        mut status_msg,
        apps,
        extra_sizes,
        selected_leftovers,
        mut leftovers_modal,
        tools_state,
    } = sig;
    let Some((app, items)) = leftovers_modal.read().clone() else {
        return;
    };
    let sel = selected_leftovers.read().clone();
    let to_delete: Vec<LeftoverItem> = items.into_iter().filter(|i| sel.contains(&i.id)).collect();
    if to_delete.is_empty() {
        leftovers_modal.set(None);
        return;
    }

    is_working.set(true);
    leftovers_modal.set(None);

    spawn(async move {
        let report =
            tokio::task::spawn_blocking(move || uninstall::purge_leftover_items(&to_delete))
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
}

/// Diálogo con los restos encontrados: se marcan los que se borran y se confirma.
#[component]
fn LeftoversModal(sig: UninstallerSignals) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).uninstaller;
    let UninstallerSignals {
        mut selected_leftovers,
        mut leftovers_modal,
        ..
    } = sig;

    let guard = leftovers_modal.read();
    let Some((modal_app, found_items)) = guard.as_ref() else {
        return rsx! {};
    };
    let sel_count = selected_leftovers.read().len();
    let total_sel_bytes: u64 = found_items
        .iter()
        .filter(|i| selected_leftovers.read().contains(&i.id))
        .map(|i| i.size_bytes)
        .sum();
    let purge_lbl = (txt.delete_permanently)(&sel_count);

    rsx! {
        Modal {
            title: (txt.leftovers_for)(&modal_app.display_name),
            subtitle: Some(txt.only_the_items_marked_as.to_string()),
            tone: Tone::Danger,
            wide: true,
            on_close: move |_| leftovers_modal.set(None),
            StatusBanner { text: txt.files_and_folders_are_deleted.to_string(), tone: Tone::Warning }

            div { class: "modal-list",
                for item in found_items.iter() {
                    {
                        let item_id = item.id.clone();
                        let checked = selected_leftovers.read().contains(&item_id);
                        let badge_lbl = leftover_kind_label(item.kind, txt);
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
                                                span { class: "badge badge-warning", "{txt.review}" }
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
                span { class: "text-xs text-muted", "{sel_count} • {format_size_mb(total_sel_bytes)}" }
                div { class: "modal-actions",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| leftovers_modal.set(None),
                        "{strings(en).close}"
                    }
                    button {
                        class: "btn btn-danger",
                        disabled: sel_count == 0,
                        onclick: move |_| start_purge(sig),
                        "{purge_lbl}"
                    }
                }
            }
        }
    }
}

/// UX-01: desinstalar pide confirmación y avisa que el desinstalador corre en modo silencioso.
#[component]
fn UninstallConfirm(
    target: InstalledApp,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<InstalledApp>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).uninstaller;
    let app = target.clone();
    rsx! {
        ConfirmDialog {
            title: (txt.uninstall)(&target.display_name),
            subtitle: None,
            confirm_label: strings(en).uninstall.to_string(),
            cancel_label: strings(en).cancel.to_string(),
            tone: Tone::Danger,
            on_cancel: move |_| on_cancel.call(()),
            on_confirm: move |_| on_confirm.call(app.clone()),
            p { class: "text-xs", "{txt.its_uninstaller_will_run_silently}" }
        }
    }
}

/// Buscador, contador, recarga y orden de la lista.
#[component]
fn UninstallerToolbar(
    search_query: Signal<String>,
    sort_mode: Signal<SortMode>,
    shown: usize,
    is_working: Signal<bool>,
    on_refresh: EventHandler<()>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).uninstaller;
    let current = *sort_mode.read();
    let sorts = [
        (SortMode::NameAsc, "A - Z"),
        (SortMode::NameDesc, "Z - A"),
        (SortMode::SizeDesc, txt.size_largest),
        (SortMode::SizeAsc, txt.size_smallest),
    ];
    rsx! {
        Card {
            title: txt.deep_uninstaller.to_string(),
            subtitle: Some(txt.uninstall_apps_and_purge_leftover.to_string()),
            div { class: "stack-3",
                div { class: "row gap-3 wrap",
                    input {
                        r#type: "text",
                        placeholder: "{txt.search_app_or_publisher}",
                        value: "{search_query.read()}",
                        class: "input grow-wide",
                        oninput: move |evt| search_query.set(evt.value()),
                    }
                    span { class: "badge badge-info", "{shown}" }
                    button {
                        class: "btn btn-secondary",
                        disabled: *is_working.read(),
                        onclick: move |_| on_refresh.call(()),
                        "{strings(en).reload_list}"
                    }
                }

                div { class: "row wrap",
                    span { class: "text-xs text-muted", "{txt.sort}" }
                    for (mode, label) in sorts {
                        button {
                            key: "{label}",
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if current == mode { "true" } else { "false" },
                            onclick: move |_| sort_mode.set(mode),
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// Una app instalada con sus acciones: actualizar (si winget conoce el paquete), buscar restos y desinstalar.
#[component]
fn AppRow(
    app: InstalledApp,
    size_label: String,
    sig: UninstallerSignals,
    confirm_uninstall: Signal<Option<InstalledApp>>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).uninstaller;
    let dispatcher = use_context::<InstallerDispatcher>();
    let is_working = sig.is_working;
    let update = app_store::winget_package_for(&app.display_name);
    let (app_for_scan, app_for_uninstall) = (app.clone(), app.clone());
    let mut confirm_uninstall = confirm_uninstall;

    rsx! {
        div { class: "list-row wrap",
            div { class: "grow-wide",
                div { class: "row wrap",
                    span { class: "fw-semibold", "{app.display_name}" }
                    if !app.display_version.is_empty() {
                        span { class: "text-xs text-muted", "v{app.display_version}" }
                    }
                }
                div { class: "text-xs text-muted mt-1",
                    if !app.publisher.is_empty() {
                        "{app.publisher} • "
                    }
                    "{size_label} • "
                    {if app.is_per_user { "HKCU" } else { "HKLM" }}
                }
            }

            div { class: "row",
                if let Some(newer) = update {
                    button {
                        class: "btn btn-secondary btn-sm",
                        // winget actualiza si hay versión nueva o responde «ya está al día».
                        disabled: *is_working.read(),
                        onclick: move |_| {
                            let _ = dispatcher.tx.send(InstallJobRequest::InstallAppBatch(vec![newer.id]));
                        },
                        "{txt.check_for_update}"
                    }
                }
                button {
                    class: "btn btn-secondary",
                    disabled: *is_working.read(),
                    onclick: move |_| start_scan(app_for_scan.clone(), sig),
                    "{txt.scan_leftovers}"
                }
                button {
                    class: "btn btn-danger",
                    disabled: *is_working.read(),
                    onclick: move |_| confirm_uninstall.set(Some(app_for_uninstall.clone())),
                    "{strings(en).uninstall}"
                }
            }
        }
    }
}

#[component]
pub fn UninstallerView() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).uninstaller;

    // UI-05: el inventario se lee fuera del hilo de UI.
    let mut apps = use_signal(Vec::<InstalledApp>::new);
    let mut apps_loaded = use_signal(|| false);
    let extra_sizes = use_signal(HashMap::<String, u64>::new);
    let search_query = use_signal(String::new);
    let sort_mode = use_signal(|| SortMode::NameAsc);
    let mut is_working = use_signal(|| false);
    let mut status_msg = use_signal(|| Status::None);
    let mut confirm_uninstall = use_signal(|| Option::<InstalledApp>::None);
    let leftovers_modal = use_signal(|| Option::<(InstalledApp, Vec<LeftoverItem>)>::None);
    let selected_leftovers = use_signal(HashSet::<String>::new);
    let purge_summary = use_signal(|| Option::<PurgeReport>::None);
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

    let mut load_apps = move || {
        is_working.set(true);
        spawn(async move {
            let res = run_blocking(uninstall::list_installed_apps).await;
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

    let extra_map = extra_sizes.read();
    let apps_guard = apps.read();
    let filtered = visible_apps(
        &apps_guard,
        &extra_map,
        &search_query.read(),
        *sort_mode.read(),
    );

    rsx! {
        div { class: "content-body",
            LeftoversModal { sig }

            if let Some(target) = confirm_uninstall.read().clone() {
                UninstallConfirm {
                    target,
                    on_cancel: move |_| confirm_uninstall.set(None),
                    on_confirm: move |app| {
                        confirm_uninstall.set(None);
                        start_uninstall_and_scan(app, sig);
                    },
                }
            }

            if let Some((msg, tone)) = status_msg.read().clone() {
                StatusBanner { text: msg.text(en), tone }
            }

            if let Some(rep) = purge_summary.read().as_ref() {
                if !rep.errors.is_empty() {
                    Card {
                        title: txt.locked_items.to_string(),
                        subtitle: None,
                        div { class: "stack-1 text-xs text-danger",
                            for err in rep.errors.iter() {
                                div { "{err}" }
                            }
                        }
                    }
                }
            }

            UninstallerToolbar {
                search_query,
                sort_mode,
                shown: filtered.len(),
                is_working,
                on_refresh: move |_| load_apps(),
            }

            Card {
                title: txt.installed_applications.to_string(),
                subtitle: None,
                div { class: "stack-2",
                    if !*apps_loaded.read() {
                        LoadingState { message: txt.reading_installed_apps.to_string() }
                    }
                    for app in filtered.into_iter() {
                        AppRow {
                            key: "{app.id}",
                            app: app.clone(),
                            size_label: effective_kb(app, &extra_map)
                                .map(|kb| format_size_mb(kb.saturating_mul(1024)))
                                .unwrap_or_else(|| "—".to_string()),
                            sig,
                            confirm_uninstall,
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

    fn app(id: &str, name: &str, publisher: &str, kb: Option<u64>) -> InstalledApp {
        InstalledApp {
            id: id.into(),
            display_name: name.into(),
            display_version: String::new(),
            publisher: publisher.into(),
            uninstall_string: String::new(),
            quiet_uninstall_string: None,
            install_location: None,
            display_icon: None,
            estimated_size_kb: kb,
            is_per_user: false,
            is_wow64: false,
            registry_subkey: String::new(),
        }
    }

    fn names(list: &[&InstalledApp]) -> Vec<String> {
        list.iter().map(|a| a.display_name.clone()).collect()
    }

    /// CL-05: la búsqueda mira nombre y editor, y las apps sin tamaño conocido van al final al ordenar por tamaño.
    #[test]
    fn busca_por_nombre_o_editor_y_ordena_por_tamano() {
        let apps = vec![
            app("a", "Zeta", "Acme", Some(10)),
            app("b", "alfa", "Otro", None),
            app("c", "Beta", "Acme", Some(500)),
        ];
        let extra = HashMap::from([("b".to_string(), 50u64)]);

        let todas = visible_apps(&apps, &extra, "", SortMode::NameAsc);
        assert_eq!(names(&todas), ["alfa", "Beta", "Zeta"]);
        assert_eq!(
            names(&visible_apps(&apps, &extra, "", SortMode::NameDesc)),
            ["Zeta", "Beta", "alfa"]
        );
        assert_eq!(
            names(&visible_apps(&apps, &extra, "acme", SortMode::NameAsc)),
            ["Beta", "Zeta"]
        );
        // El tamaño calculado aparte cuenta: alfa (50) queda entre Beta (500) y Zeta (10).
        assert_eq!(
            names(&visible_apps(&apps, &extra, "", SortMode::SizeDesc)),
            ["Beta", "alfa", "Zeta"]
        );
        assert_eq!(
            names(&visible_apps(&apps, &HashMap::new(), "", SortMode::SizeAsc)),
            ["Zeta", "Beta", "alfa"]
        );
    }
}
