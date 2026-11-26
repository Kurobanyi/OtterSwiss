use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::context_menu::ContextMenuManager;
use crate::core::error::CoreError;
use crate::core::platform::elevation::ElevationManager;
use crate::core::store::app_store::{AppCategory, AppStoreManager, InstallSource};
use crate::core::store::dependencies::DependencyManager;
use crate::core::sys_tools::{SysTools, TrimWorkingSetsReport};
use crate::ui::components::{
    AppIconBadge, Card, EmptyState, Modal, ProgressBar, StatusBanner, Tone,
};
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::installer::{
    InstallJobRequest, InstallStatus, InstallerDispatcher, ToolsSharedState,
};

#[component]
pub fn ToolsView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut context_menu_active =
        use_signal(|| ContextMenuManager::is_installed().unwrap_or(false));
    let mut tools_state = use_context::<Signal<ToolsSharedState>>();
    let dispatcher = use_context::<InstallerDispatcher>();
    let mut local_working = use_signal(|| false);
    let mut ram_report_modal = use_signal(|| Option::<TrimWorkingSetsReport>::None);

    let on_trim_ram = move |_| {
        local_working.set(true);
        tools_state.write().tool_log = Some((
            Msg::new(|en| {
                if en {
                    "Trimming RAM working sets…".to_string()
                } else {
                    "Optimizando memoria RAM…".to_string()
                }
            }),
            Tone::Info,
        ));

        spawn(async move {
            let res = tokio::task::spawn_blocking(SysTools::trim_working_sets)
                .await
                .unwrap_or_else(|e| {
                    Err(CoreError::SystemTool {
                        message: e.to_string(),
                        source: None,
                    })
                });

            match res {
                Ok(report) => {
                    let (trimmed, skipped) = (report.processes_trimmed, report.processes_skipped);
                    tools_state.write().tool_log = Some((
                        Msg::new(move |en| {
                            if en {
                                format!("RAM trimmed: {trimmed} processes, {skipped} skipped.")
                            } else {
                                format!("RAM optimizada: {trimmed} procesos, {skipped} omitidos.")
                            }
                        }),
                        Tone::Success,
                    ));
                    ram_report_modal.set(Some(report));
                }
                Err(e) => {
                    tools_state.write().tool_log = Some((Msg::from(e), Tone::Danger));
                }
            }
            local_working.set(false);
        });
    };

    let on_toggle_context_menu = move |_| {
        let currently_active = *context_menu_active.read();
        local_working.set(true);

        spawn(async move {
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
                    source: None,
                })
            });

            match res {
                Ok(()) => {
                    let next = !currently_active;
                    *context_menu_active.write() = next;
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
                                *context_menu_active.write() = installed_now;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tools_state.write().tool_log = Some((Msg::from(e), Tone::Danger));
                }
            }
            local_working.set(false);
        });
    };

    let state = tools_state.read();
    let any_busy = state.is_working || *local_working.read();

    let ram_title = if en {
        "RAM Optimizer"
    } else {
        "Optimización de RAM"
    }
    .to_string();
    let ram_sub = Some(
        (if en {
            "Move idle pages to Windows Standby list"
        } else {
            "Mueve páginas inactivas a Standby"
        })
        .to_string(),
    );
    let ctx_title = if en {
        "Explorer Context Menu"
    } else {
        "Menú contextual"
    }
    .to_string();
    let ctx_sub = Some(
        (if en {
            "Right-click Compress (.zip) & Extract"
        } else {
            "Comprimir (.zip) y extraer en clic derecho"
        })
        .to_string(),
    );
    let runtimes_title = if en {
        "Gaming Runtimes"
    } else {
        "Dependencias para juegos"
    }
    .to_string();
    let runtimes_sub = Some(
        (if en {
            "Essential official Windows libraries"
        } else {
            "Bibliotecas oficiales esenciales"
        })
        .to_string(),
    );

    rsx! {
        div { class: "content-body",
            if let Some(report) = ram_report_modal.read().clone() {
                Modal {
                    title: if en { "RAM Working Sets Trimmed".to_string() } else { "Optimización de RAM Completada".to_string() },
                    subtitle: Some(if en {
                        format!("Trimmed: {} | Skipped: {}", report.processes_trimmed, report.processes_skipped)
                    } else {
                        format!("Procesados: {} | Omitidos: {}", report.processes_trimmed, report.processes_skipped)
                    }),
                    on_close: move |_| ram_report_modal.set(None),
                    div { class: "modal-list",
                        for proc_name in report.trimmed_processes.iter() {
                            div { key: "{proc_name}", "• {proc_name}" }
                        }
                    }
                    div { class: "modal-actions",
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| ram_report_modal.set(None),
                            "{strings(en).close}"
                        }
                    }
                }
            }

            if state.is_working {
                div {
                    class: "card card-warning",
                    div { class: "stack-2",
                        strong { class: "text-warning",
                            if en { "Installing — Please keep OtterSwiss open until finished." } else { "Instalando — No cierres OtterSwiss hasta que finalice." }
                        }
                        if let Some(lbl) = state.progress_label.as_ref() {
                            div { class: "text-sm", "{lbl} ({state.progress_percent}%)" }
                        }
                        ProgressBar { percent: state.progress_percent }
                    }
                }
            }

            if let Some((msg, tone)) = state.tool_log.clone() {
                StatusBanner {
                    tone,
                    text: msg.text(en),
                }
            }

            div { class: "cards-grid",
                Card {
                    title: ram_title,
                    subtitle: ram_sub,
                    button {
                        class: "btn btn-primary",
                        disabled: any_busy,
                        onclick: on_trim_ram,
                        if *local_working.read() {
                            if en { "Trimming…" } else { "Procesando…" }
                        } else if en {
                            "Trim RAM Now"
                        } else {
                            "Optimizar RAM"
                        }
                    }
                }

                Card {
                    title: ctx_title,
                    subtitle: ctx_sub,
                    div { class: "row-between gap-3",
                        span {
                            class: if *context_menu_active.read() { "badge badge-success" } else { "badge badge-warning" },
                            if *context_menu_active.read() {
                                "{strings(en).installed_runtime}"
                            } else if en {
                                "Not Installed"
                            } else {
                                "No Instalado"
                            }
                        }
                        button {
                            class: if *context_menu_active.read() { "btn btn-danger" } else { "btn btn-primary" },
                            disabled: any_busy,
                            onclick: on_toggle_context_menu,
                            if *context_menu_active.read() {
                                "{strings(en).uninstall}"
                            } else if en {
                                "Install"
                            } else {
                                "Instalar"
                            }
                        }
                    }
                }
            }

            Card {
                title: runtimes_title,
                subtitle: runtimes_sub,
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
                                            } else if en {
                                                "Missing"
                                            } else {
                                                "Faltante"
                                            }
                                        }
                                        if !installed {
                                            button {
                                                class: "btn btn-primary btn-sm",
                                                disabled: any_busy,
                                                onclick: move |_| {
                                                    let _ = tx.send(InstallJobRequest::InstallRuntime(dep_id));
                                                },
                                                if en { "Install" } else { "Instalar" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            BatchAppStoreCard {}
        }
    }
}

/// Paquetes del catálogo que se ven con la búsqueda `query` (en minúsculas) y la categoría `cat`.
fn filter_catalog(
    query: &str,
    cat: Option<AppCategory>,
) -> Vec<&'static crate::core::store::app_store::CuratedPackage> {
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
pub fn BatchAppStoreCard() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut selected = use_signal(HashSet::<&'static str>::new);
    let mut search_query = use_signal(String::new);
    let mut selected_cat = use_signal(|| Option::<AppCategory>::None);

    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let dispatcher = use_context::<InstallerDispatcher>();

    let categories = [
        AppCategory::Gaming,
        AppCategory::Communication,
        AppCategory::Utilities,
        AppCategory::Browsers,
        AppCategory::Development,
        AppCategory::UwpRestoration,
    ];

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

    let state = tools_state.read();
    let is_running = state.is_working;

    let q = search_query.read().trim().to_ascii_lowercase();
    let cur_cat = *selected_cat.read();

    let filtered_catalog = filter_catalog(&q, cur_cat);
    let selected_count = filtered_catalog
        .iter()
        .filter(|p| selected.read().contains(p.id))
        .count();

    rsx! {
        Card {
            title: strings(en).store_title.to_string(),
            subtitle: Some(strings(en).store_subtitle.to_string()),
            div { class: "stack-3",
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
                            let cat_label = cat.label(en);
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

                div { class: "row-between gap-3 wrap",
                    div { class: "flex gap-2",
                        button {
                            class: "btn btn-primary",
                            disabled: is_running || selected_count == 0,
                            onclick: on_install_selected,
                            if is_running {
                                "{strings(en).installing}"
                            } else if en {
                                "Install Selected ({selected_count})"
                            } else {
                                "Instalar ({selected_count})"
                            }
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

                if filtered_catalog.is_empty() {
                    EmptyState {
                        title: strings(en).store_empty.to_string(),
                        description: strings(en).store_empty_hint.to_string(),
                    }
                }

                div { class: "modal-list tall",
                    for pkg in filtered_catalog.into_iter() {
                        {
                            let pkg_id = pkg.id;
                            let is_checked = selected.read().contains(pkg_id);
                            let is_installed = state.installed_app_ids.contains(pkg_id);
                            let st = state.app_statuses.get(pkg_id).cloned();
                            let desc = if en { pkg.description_en } else { pkg.description_es };
                            let cat_tag = pkg.category.label(en);
                            let version_label = match pkg.source {
                                InstallSource::Store { .. } => "Microsoft Store".to_string(),
                                InstallSource::Winget { .. } => strings(en).latest_version.to_string(),
                            };

                            rsx! {
                                div {
                                    key: "{pkg_id}",
                                    class: "item-row stack-1",
                                    label { class: "row-between clickable",
                                        div { class: "row gap-3 grow",
                                            input {
                                                r#type: "checkbox",
                                                checked: is_checked,
                                                disabled: is_running,
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
                    }
                }
            }
        }
    }
}
