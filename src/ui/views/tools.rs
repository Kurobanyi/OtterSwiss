use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::context_menu::ContextMenuManager;
use crate::core::error::CoreError;
use crate::core::platform::elevation::ElevationManager;
use crate::core::store::app_store::{AppCategory, AppStoreManager};
use crate::core::store::dependencies::DependencyManager;
use crate::core::sys_tools::{SysTools, TrimWorkingSetsReport};
use crate::ui::components::{Card, Modal, ProgressBar, StatusBanner, Tone};
use crate::ui::i18n::{Lang, strings};
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

#[component]
pub fn BatchAppStoreCard() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut selected = use_signal(HashSet::<&'static str>::new);
    let tools_state = use_context::<Signal<ToolsSharedState>>();
    let dispatcher = use_context::<InstallerDispatcher>();

    let categories = [
        AppCategory::Gaming,
        AppCategory::Communication,
        AppCategory::Utilities,
        AppCategory::Browsers,
        AppCategory::Development,
    ];

    let on_install_selected = {
        let tx = dispatcher.tx.clone();
        move |_| {
            let ids: Vec<&'static str> = AppStoreManager::get_catalog()
                .iter()
                .filter(|p| selected.read().contains(p.id))
                .map(|p| p.id)
                .collect();

            if !ids.is_empty() {
                let _ = tx.send(InstallJobRequest::InstallAppBatch(ids));
            }
        }
    };

    let selected_count = selected.read().len();
    let state = tools_state.read();
    let is_running = state.is_working;

    let store_title = if en {
        "Silent App Store"
    } else {
        "Tienda silenciosa"
    }
    .to_string();
    let store_sub = Some(
        (if en {
            "Official vendor downloads • Batch unattended install"
        } else {
            "Descarga oficial • Instalación desatendida en lote"
        })
        .to_string(),
    );

    rsx! {
        Card {
            title: store_title,
            subtitle: store_sub,
            div { class: "stack-3",
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

                for cat in categories {
                    div { key: "{cat.label(en)}", class: "stack-2",
                        h4 { style: "font-size: var(--font-size-xs); color: var(--accent-text); border-bottom: 1px solid var(--border); padding-bottom: var(--space-1);",
                            "{cat.label(en)}"
                        }
                        for pkg in AppStoreManager::get_catalog().iter().filter(|p| p.category == cat) {
                            {
                                let pkg_id = pkg.id;
                                let is_checked = selected.read().contains(pkg_id);
                                let is_installed = state.installed_app_ids.contains(pkg_id);
                                let st = state.app_statuses.get(pkg_id).cloned();
                                let desc = if en { pkg.description_en } else { pkg.description_es };
                                // UI-15: «Evergreen» no es un número de versión.
                                let version_label = if pkg.version == "Evergreen" {
                                    if en { "latest version".to_string() } else { "última versión".to_string() }
                                } else {
                                    format!("v{}", pkg.version)
                                };

                                rsx! {
                                    div {
                                        key: "{pkg_id}",
                                        style: "display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-2); border: 1px solid var(--border); border-radius: var(--radius);",
                                        label {
                                            style: "display: flex; justify-content: space-between; align-items: center; cursor: pointer;",
                                            div { class: "row gap-3",
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
                                                div {
                                                    div { class: "text-sm fw-semibold",
                                                        "{pkg.name} "
                                                        span { style: "font-weight: 400; font-size: var(--font-size-xs); color: var(--text-muted);",
                                                            "{version_label} • © {pkg.copyright_holder} ({pkg.license})"
                                                        }
                                                    }
                                                    div { class: "text-xs text-muted",
                                                        "{desc}"
                                                    }
                                                }
                                            }

                                            div { class: "row",
                                                if is_installed {
                                                    span { class: "badge badge-success", "{strings(en).installed_app}" }
                                                }
                                                if let Some(ref status) = st {
                                                    match status {
                                                        InstallStatus::Queued => rsx! { span { class: "badge badge-info", if en { "Queued" } else { "En cola" } } },
                                                        InstallStatus::Installing => rsx! { span { class: "badge badge-warning", "{strings(en).installing}" } },
                                                        InstallStatus::Done => rsx! { span { class: "badge badge-success", if en { "Done" } else { "Listo" } } },
                                                        InstallStatus::Failed(_) => rsx! { span { class: "badge badge-danger", "Error" } },
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(InstallStatus::Failed(err_msg)) = st {
                                            div {
                                                style: "font-size: var(--font-size-xs); color: var(--danger-text); margin-left: var(--space-5);",
                                                "{err_msg}"
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
