use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::context_menu::ContextMenuManager;
use crate::core::error::CoreError;
use crate::core::platform::elevation::ElevationManager;
use crate::core::store::app_store::{AppCategory, AppIcon, AppStoreManager};
use crate::core::store::dependencies::DependencyManager;
use crate::core::sys_tools::{SysTools, TrimWorkingSetsReport};
use crate::ui::components::{Card, Modal, ProgressBar, StatusBanner, Tone};
use crate::ui::i18n::{Lang, strings};
use crate::ui::installer::{
    InstallJobRequest, InstallStatus, InstallerDispatcher, ToolsSharedState,
};

fn render_generic_icon(icon: AppIcon) -> Element {
    let (p1, p2) = match icon {
        AppIcon::Gamepad => (
            "M6 12h4m-2-2v4",
            "M15 11h.01M18 13h.01M17.5 7A5.5 5.5 0 0 0 12 12.5a5.5 5.5 0 0 0-5.5-5.5A5.5 5.5 0 0 0 2 12.5C2 16.5 6 20 12 20s10-3.5 10-7.5A5.5 5.5 0 0 0 17.5 7z",
        ),
        AppIcon::Browser => (
            "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2z",
            "M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
        ),
        AppIcon::Chat => (
            "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
            "",
        ),
        AppIcon::Video => (
            "M23 7l-7 5 7 5V7z",
            "M1 5a2 2 0 0 1 2-2h11a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V5z",
        ),
        AppIcon::Code => ("M16 18l6-6-6-6", "M8 6l-6 6 6 6"),
        AppIcon::Terminal => ("M4 17l6-6-6-6", "M12 19h8"),
        AppIcon::Archive => ("M21 8v13H3V8", "M1 3h22v5H1zM10 12h4"),
        AppIcon::Download => (
            "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4",
            "M7 10l5 5 5-5M12 15V3",
        ),
        AppIcon::Search => ("M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16z", "M21 21l-4.35-4.35"),
        AppIcon::Wrench => (
            "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z",
            "",
        ),
        AppIcon::Disk => (
            "M22 12H2",
            "M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11zM6 16h.01M10 16h.01",
        ),
        AppIcon::Camera => (
            "M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z",
            "M12 17a4 4 0 1 0 0-8 4 4 0 0 0 0 8z",
        ),
        AppIcon::Clock => ("M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2z", "M12 6v6l4 2"),
        AppIcon::Calculator => (
            "M4 2h16a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z",
            "M8 6h8M16 14v4M12 14v4M8 14v4M16 10h.01M12 10h.01M8 10h.01",
        ),
        AppIcon::Microphone => (
            "M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z",
            "M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8",
        ),
        AppIcon::Phone => (
            "M17 2H7a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2z",
            "M12 18h.01",
        ),
        AppIcon::Checklist => (
            "M9 11l3 3L22 4",
            "M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11",
        ),
        AppIcon::Store => (
            "M6 2L3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4zM3 6h18",
            "M16 10a4 4 0 0 1-8 0",
        ),
        AppIcon::Paint => (
            "M18.37 2.63L14 7l3 3 4.37-4.37a2.12 2.12 0 1 0-3-3z",
            "M9 12l-7 7a1 1 0 0 0 0 1.41l1.59 1.59a1 1 0 0 0 1.41 0l7-7M12 15l3 3",
        ),
        AppIcon::Scissors => (
            "M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
            "M20 4L8.12 15.88M14.47 14.48L20 20M8.12 8.12L12 12",
        ),
    };

    rsx! {
        div {
            style: "width: 28px; height: 28px; border-radius: var(--radius-sm); background: color-mix(in srgb, var(--accent) 12%, var(--surface)); display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
            svg {
                style: "width: 16px; height: 16px; stroke: var(--accent); stroke-width: 2; fill: none; stroke-linecap: round; stroke-linejoin: round;",
                view_box: "0 0 24 24",
                if !p1.is_empty() {
                    path { d: "{p1}" }
                }
                if !p2.is_empty() {
                    path { d: "{p2}" }
                }
            }
        }
    }
}

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
            if en {
                "Trimming RAM working sets…".to_string()
            } else {
                "Optimizando memoria RAM…".to_string()
            },
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
                    tools_state.write().tool_log = Some((
                        if en {
                            format!(
                                "RAM trimmed: {} processes, {} skipped.",
                                report.processes_trimmed, report.processes_skipped
                            )
                        } else {
                            format!(
                                "RAM optimizada: {} procesos, {} omitidos.",
                                report.processes_trimmed, report.processes_skipped
                            )
                        },
                        Tone::Success,
                    ));
                    ram_report_modal.set(Some(report));
                }
                Err(e) => {
                    tools_state.write().tool_log = Some((format!("Error: {}", e), Tone::Danger));
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
                        if next {
                            if en {
                                "Context menu installed.".to_string()
                            } else {
                                "Menú contextual instalado.".to_string()
                            }
                        } else {
                            if en {
                                "Context menu removed.".to_string()
                            } else {
                                "Menú contextual desinstalado.".to_string()
                            }
                        },
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
                    tools_state.write().tool_log = Some((format!("Error: {}", e), Tone::Danger));
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
                    class: "card",
                    style: "border: 1px solid var(--warning); background: color-mix(in srgb, var(--warning) 8%, var(--surface));",
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
                    text: msg,
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
                    div { style: "display: flex; align-items: center; justify-content: space-between; gap: var(--space-3);",
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

                    if let Some(msg) = state.summary_msg.as_ref() {
                        span { class: "badge badge-info", "{msg}" }
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
                            let version_label = if pkg.is_winget {
                                "MS Store".to_string()
                            } else if pkg.version == "Evergreen" {
                                strings(en).latest_version.to_string()
                            } else {
                                format!("v{}", pkg.version)
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
                                            {render_generic_icon(pkg.icon)}
                                            div { class: "grow",
                                                div { class: "row wrap",
                                                    span { class: "text-sm fw-semibold", "{pkg.name}" }
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
                                        div { class: "text-xs text-danger ml-5", "{err_msg}" }
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
