use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::context_menu::ContextMenuManager;
use crate::core::error::CoreError;
use crate::core::platform::elevation::ElevationManager;
use crate::core::store::app_store::{AppCategory, AppStoreManager, CuratedPackage, InstallSource};
use crate::core::store::dependencies::DependencyManager;
use crate::ui::components::{AppIconBadge, Card, EmptyState, ProgressBar, StatusBanner, Tone};
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, pick, strings};
use crate::ui::installer::{
    InstallJobRequest, InstallStatus, InstallerDispatcher, ToolsSharedState,
};

/// Instala o quita el menú contextual; sin permisos, relanza la app como administrador y espera el cambio.
async fn toggle_context_menu(
    currently_active: bool,
    mut active: Signal<bool>,
    mut tools_state: Signal<ToolsSharedState>,
) {
    let res = tokio::task::spawn_blocking(move || {
        if currently_active {
            ContextMenuManager::uninstall()
        } else {
            ContextMenuManager::install()
        }
    })
    .await
    .unwrap_or_else(|e| {
        Err(CoreError::SystemTool {
            message: e.to_string(),
            source: Some(Box::new(e)),
        })
    });

    match res {
        Ok(()) => {
            let next = !currently_active;
            *active.write() = next;
            tools_state.write().tool_log = Some((
                Msg::new(move |en| {
                    match (next, en) {
                        (true, true) => "Context menu installed.",
                        (true, false) => "Menú contextual instalado.",
                        (false, true) => "Context menu removed.",
                        (false, false) => "Menú contextual desinstalado.",
                    }
                    .to_string()
                }),
                Tone::Success,
            ));
        }
        Err(CoreError::NotElevated(_)) => {
            let flag = if currently_active {
                crate::core::cli::UNINSTALL_CONTEXT_MENU
            } else {
                crate::core::cli::INSTALL_CONTEXT_MENU
            };
            if ElevationManager::relaunch_as_admin(&[flag]).is_ok() {
                for _ in 0..8 {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    let installed_now =
                        ContextMenuManager::is_installed().unwrap_or(currently_active);
                    if installed_now != currently_active {
                        *active.write() = installed_now;
                        break;
                    }
                }
            }
        }
        Err(e) => {
            tools_state.write().tool_log = Some((Msg::from(e), Tone::Danger));
        }
    }
}

/// Paquetes del catálogo que se ven con la búsqueda `query` (en minúsculas) y la categoría `cat`.
fn filter_catalog(query: &str, cat: Option<AppCategory>) -> Vec<&'static CuratedPackage> {
    AppStoreManager::get_catalog()
        .iter()
        .filter(|p| cat.is_none_or(|c| p.category == c))
        .filter(|p| {
            query.is_empty()
                || p.name.to_ascii_lowercase().contains(query)
                || p.copyright_holder.to_ascii_lowercase().contains(query)
                || p.description_es.to_ascii_lowercase().contains(query)
                || p.description_en.to_ascii_lowercase().contains(query)
        })
        .collect()
}

#[component]
pub fn ToolsView() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let local_working = use_signal(|| false);

    let state = tools_state.read();
    let any_busy = state.is_working || *local_working.read();

    rsx! {
        div { class: "content-body",
            if state.is_working {
                InstallProgressCard {}
            }
            if let Some((msg, tone)) = state.tool_log.clone() {
                StatusBanner { tone, text: msg.text(en) }
            }
            ContextMenuCard { any_busy, local_working }
            RuntimesCard { any_busy }
            BatchAppStoreCard {}
        }
    }
}

/// Aviso fijo mientras hay una instalación en curso.
#[component]
fn InstallProgressCard() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).tools;
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let state = tools_state.read();
    rsx! {
        div {
            class: "card card-warning",
            div { class: "stack-2",
                strong { class: "text-warning",
                    "{txt.installing_please_keep_otterswiss_open}"
                }
                if let Some(lbl) = state.progress_label.as_ref() {
                    div { class: "text-sm", "{lbl} ({state.progress_percent}%)" }
                }
                ProgressBar { percent: state.progress_percent }
            }
        }
    }
}

#[component]
fn ContextMenuCard(any_busy: bool, local_working: Signal<bool>) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).tools;
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let active = use_signal(|| ContextMenuManager::is_installed().unwrap_or(false));
    let mut working = local_working;

    let on_toggle = move |_| {
        let currently_active = *active.read();
        working.set(true);
        spawn(async move {
            toggle_context_menu(currently_active, active, tools_state).await;
            working.set(false);
        });
    };

    rsx! {
        div { class: "cards-grid",
            Card {
                title: txt.explorer_context_menu.to_string(),
                subtitle: Some(txt.right_click_compress_zip_extract.to_string()),
                div { class: "row-between gap-3",
                    span {
                        class: if *active.read() { "badge badge-success" } else { "badge badge-warning" },
                        if *active.read() {
                            "{strings(en).installed_runtime}"
                        } else { "{txt.not_installed}" }
                    }
                    button {
                        class: if *active.read() { "btn btn-danger" } else { "btn btn-primary" },
                        disabled: any_busy,
                        onclick: on_toggle,
                        if *active.read() {
                            "{strings(en).uninstall}"
                        } else { "{txt.install}" }
                    }
                }
            }
        }
    }
}

#[component]
fn RuntimesCard(any_busy: bool) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).tools;
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let dispatcher = use_context::<InstallerDispatcher>();
    let state = tools_state.read();

    rsx! {
        Card {
            title: txt.gaming_runtimes.to_string(),
            subtitle: Some(txt.essential_official_windows_libraries.to_string()),
            div { class: "stack-2",
                for dep in DependencyManager::get_catalog() {
                    {
                        let installed = state.dep_states.get(dep.id).copied().unwrap_or(false);
                        let dep_id = dep.id;
                        let tx = dispatcher.tx.clone();
                        rsx! {
                            div {
                                key: "{dep_id}",
                                class: "list-row",
                                div {
                                    div { class: "text-sm fw-semibold", "{dep.name}" }
                                }
                                div { class: "row",
                                    span {
                                        class: if installed { "badge badge-success" } else { "badge badge-warning" },
                                        if installed {
                                            "{strings(en).installed_runtime}"
                                        } else { "{txt.missing}" }
                                    }
                                    if !installed {
                                        button {
                                            class: "btn btn-primary btn-sm",
                                            disabled: any_busy,
                                            onclick: move |_| {
                                                let _ = tx.send(InstallJobRequest::InstallRuntime(dep_id));
                                            },
                                            "{txt.install}"
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

/// Búsqueda y filtro por categoría de la tienda.
#[component]
fn StoreFilters(
    search_query: Signal<String>,
    selected_cat: Signal<Option<AppCategory>>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let mut search_query = search_query;
    let mut selected_cat = selected_cat;
    let cur_cat = *selected_cat.read();
    let categories = [
        AppCategory::Gaming,
        AppCategory::Communication,
        AppCategory::Utilities,
        AppCategory::Browsers,
        AppCategory::Development,
        AppCategory::UwpRestoration,
    ];

    rsx! {
        div { class: "row wrap",
            input {
                r#type: "text",
                class: "input grow-wide",
                placeholder: "{strings(en).store_search}",
                value: "{search_query.read()}",
                oninput: move |evt| search_query.set(evt.value()),
            }
            // TE-28: la categoría elegida se marca con aria-pressed, no alternando btn-primary.
            button {
                class: "btn btn-secondary btn-sm",
                "aria-pressed": if cur_cat.is_none() { "true" } else { "false" },
                onclick: move |_| selected_cat.set(None),
                "{strings(en).all_categories}"
            }
            for cat in categories {
                {
                    let cat_label = tr(cat.label(), en);
                    let is_selected_cat = cur_cat == Some(cat);
                    rsx! {
                        button {
                            key: "{cat_label}",
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if is_selected_cat { "true" } else { "false" },
                            onclick: move |_| selected_cat.set(Some(cat)),
                            "{cat_label}"
                        }
                    }
                }
            }
        }
    }
}

/// Instalar lo marcado, cancelar o limpiar la selección, y el resumen del último lote.
#[component]
fn StoreActions(
    selected: Signal<HashSet<&'static str>>,
    search_query: Signal<String>,
    selected_cat: Signal<Option<AppCategory>>,
    selected_count: usize,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let dispatcher = use_context::<InstallerDispatcher>();
    let mut selected = selected;
    let state = tools_state.read();
    let is_running = state.is_working;

    let on_install_selected = {
        let tx = dispatcher.tx.clone();
        move |_| {
            // UI-21: solo lo que se ve; lo marcado y oculto por el filtro no se instala a ciegas.
            let q = search_query.read().trim().to_ascii_lowercase();
            let ids: Vec<&'static str> = filter_catalog(&q, *selected_cat.read())
                .into_iter()
                .filter(|p| selected.read().contains(p.id))
                .map(|p| p.id)
                .collect();

            if !ids.is_empty() && tx.send(InstallJobRequest::InstallAppBatch(ids)).is_ok() {
                selected.write().clear();
            }
        }
    };

    rsx! {
        div { class: "row-between gap-3 wrap",
            div { class: "flex gap-2",
                button {
                    class: "btn btn-primary",
                    disabled: is_running || selected_count == 0,
                    onclick: on_install_selected,
                    if is_running {
                        "{strings(en).installing}"
                    } else { "{(strings(en).tools.install_selected)(&selected_count)}" }
                }
                if is_running {
                    button {
                        class: "btn btn-danger",
                        onclick: move |_| dispatcher.cancel_flag.store(true, Ordering::Release),
                        "{strings(en).cancel}"
                    }
                } else if selected_count > 0 {
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| selected.write().clear(),
                        "{strings(en).clear}"
                    }
                }
            }

            if let Some(msg) = state.summary_msg.as_ref().map(|m| m.text(en)) {
                span { class: "badge badge-info", "{msg}" }
            }
        }
    }
}

/// Una app del catálogo: casilla, datos y estado de instalación.
#[component]
fn StoreRow(pkg: &'static CuratedPackage, selected: Signal<HashSet<&'static str>>) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let state = tools_state.read();
    let mut selected = selected;

    let pkg_id = pkg.id;
    let is_checked = selected.read().contains(pkg_id);
    let is_installed = state.installed_app_ids.contains(pkg_id);
    let st = state.app_statuses.get(pkg_id).cloned();
    let desc = pick(en, pkg.description_es, pkg.description_en);
    let cat_tag = tr(pkg.category.label(), en);
    let version_label = match pkg.source {
        InstallSource::Store { .. } => "Microsoft Store".to_string(),
        InstallSource::Winget { .. } => strings(en).latest_version.to_string(),
    };

    rsx! {
        div {
            class: "item-row stack-1",
            label { class: "row-between clickable",
                div { class: "row gap-3 grow",
                    input {
                        r#type: "checkbox",
                        checked: is_checked,
                        disabled: state.is_working,
                        onchange: move |evt| {
                            let mut set = selected.write();
                            if evt.checked() {
                                set.insert(pkg_id);
                            } else {
                                set.remove(pkg_id);
                            }
                        },
                    }
                    AppIconBadge { icon: pkg.icon }
                    div { class: "grow",
                        div { class: "row wrap",
                            span { class: "text-sm fw-semibold", "{tr(pkg.name, en)}" }
                            span { class: "badge badge-info", "{cat_tag}" }
                            span { class: "text-xs text-muted",
                                "{version_label} • © {pkg.copyright_holder}"
                            }
                        }
                        div { class: "text-xs text-muted mt-1", "{desc}" }
                    }
                }

                div { class: "row",
                    if is_installed {
                        span { class: "badge badge-success", "{strings(en).installed_app}" }
                    }
                    if let Some(ref status) = st {
                        match status {
                            InstallStatus::Queued => rsx! { span { class: "badge badge-info", "{strings(en).queued}" } },
                            InstallStatus::Installing => rsx! { span { class: "badge badge-warning", "{strings(en).installing}" } },
                            InstallStatus::Done => rsx! { span { class: "badge badge-success", "{strings(en).done}" } },
                            InstallStatus::Failed(_) => rsx! { span { class: "badge badge-danger", "Error" } },
                        }
                    }
                }
            }
            if let Some(InstallStatus::Failed(err_msg)) = st {
                // TE-27 (regresión de UI-18): texto en `--danger-text`, que cumple AA; `--danger` es para fondos.
                div { class: "text-xs text-danger ml-5", "{err_msg.text(en)}" }
            }
        }
    }
}

#[component]
pub fn BatchAppStoreCard() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let selected = use_signal(HashSet::<&'static str>::new);
    let search_query = use_signal(String::new);
    let selected_cat = use_signal(|| Option::<AppCategory>::None);

    let q = search_query.read().trim().to_ascii_lowercase();
    let filtered_catalog = filter_catalog(&q, *selected_cat.read());
    let selected_count = filtered_catalog
        .iter()
        .filter(|p| selected.read().contains(p.id))
        .count();

    rsx! {
        Card {
            title: strings(en).store_title.to_string(),
            subtitle: Some(strings(en).store_subtitle.to_string()),
            div { class: "stack-3",
                StoreFilters { search_query, selected_cat }
                StoreActions { selected, search_query, selected_cat, selected_count }

                if filtered_catalog.is_empty() {
                    EmptyState {
                        title: strings(en).store_empty.to_string(),
                        description: strings(en).store_empty_hint.to_string(),
                    }
                }

                div { class: "modal-list tall",
                    for pkg in filtered_catalog {
                        StoreRow { key: "{pkg.id}", pkg, selected }
                    }
                }
            }
        }
    }
}
