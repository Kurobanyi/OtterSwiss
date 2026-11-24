// src/ui/mod.rs

pub mod components;
pub mod i18n;
pub mod theme;
pub mod views;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use dioxus::desktop::use_window;
use dioxus::prelude::*;
use i18n::Lang;
use theme::Theme;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use views::archiver::ArchiverView;
use views::cleaner::CleanerView;
use views::dashboard::DashboardView;
use views::debloater::DebloaterView;
use views::dns::DnsView;
use views::hardware_tab::HardwareTab;
use views::sensors::SensorsView;
use views::tools::ToolsView;
use views::uninstaller::UninstallerView;

use crate::core::app_store::AppStoreManager;
use crate::ui::components::{Modal, StatusBanner, Tone};
use crate::core::dependencies::DependencyManager;
use crate::core::error::CoreError;
use crate::core::updater::{
    check_for_updates, download_and_install_update, launch_installer_and_exit, UpdateReleaseInfo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActiveTab {
    Dashboard,
    Cleaner,
    Debloater,
    Dns,
    Archiver,
    Tools,
    Uninstaller,
    Hardware,
    Sensors,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStatus {
    Queued,
    Installing,
    Done,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum InstallJobRequest {
    InstallRuntime(&'static str),
    InstallAppBatch(Vec<&'static str>),
}

#[derive(Debug, Clone)]
pub struct ToolsSharedState {
    pub is_working: bool,
    pub progress_percent: u8,
    pub progress_label: Option<String>,
    pub tool_log: Option<String>,
    pub summary_msg: Option<String>,
    pub dep_states: HashMap<&'static str, bool>,
    pub installed_app_ids: HashSet<&'static str>,
    pub app_statuses: HashMap<&'static str, InstallStatus>,
}

impl Default for ToolsSharedState {
    fn default() -> Self {
        let dep_states = DependencyManager::get_catalog()
            .iter()
            .map(|d| (d.id, DependencyManager::is_installed(d.id)))
            .collect();
        let installed_app_ids = AppStoreManager::detect_installed_ids();
        Self {
            is_working: false,
            progress_percent: 0,
            progress_label: None,
            tool_log: None,
            summary_msg: None,
            dep_states,
            installed_app_ids,
            app_statuses: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct InstallerDispatcher {
    pub tx: UnboundedSender<InstallJobRequest>,
    pub cancel_flag: Arc<AtomicBool>,
}

pub fn refresh_tools_inventory(mut tools_state: Signal<ToolsSharedState>) {
    spawn(async move {
        let (deps, apps) = tokio::task::spawn_blocking(|| {
            let d = DependencyManager::get_catalog()
                .iter()
                .map(|dep| (dep.id, DependencyManager::is_installed(dep.id)))
                .collect::<HashMap<_, _>>();
            let a = AppStoreManager::detect_installed_ids();
            (d, a)
        })
        .await
        .unwrap_or_default();

        let mut s = tools_state.write();
        s.dep_states = deps;
        s.installed_app_ids = apps;
    });
}

fn run_installer_worker(
    mut rx: UnboundedReceiver<InstallJobRequest>,
    cancel_worker: Arc<AtomicBool>,
    mut state_sig: Signal<ToolsSharedState>,
    lang: Signal<Lang>,
) {
    let lang_is_en = move || lang.peek().is_en();
    spawn(async move {
        while let Some(req) = rx.recv().await {
            cancel_worker.store(false, Ordering::Release);
            match req {
                InstallJobRequest::InstallRuntime(dep_id) => {
                    {
                        let mut s = state_sig.write();
                        s.is_working = true;
                        s.progress_percent = 30;
                        s.progress_label = Some(format!("{}...", dep_id));
                        s.tool_log = Some(format!("{} '{}'...", if lang_is_en() { "Installing" } else { "Instalando" }, dep_id));
                    }

                    let res = tokio::task::spawn_blocking(move || {
                        DependencyManager::install_runtime(dep_id)
                    })
                    .await
                    .unwrap_or_else(|e| {
                        Err(CoreError::SystemTool {
                            message: e.to_string(),
                            source: None,
                        })
                    });

                    let updated_deps = tokio::task::spawn_blocking(|| {
                        DependencyManager::get_catalog()
                            .iter()
                            .map(|d| (d.id, DependencyManager::is_installed(d.id)))
                            .collect::<HashMap<_, _>>()
                    })
                    .await
                    .unwrap_or_default();

                    let mut s = state_sig.write();
                    s.dep_states = updated_deps;
                    s.progress_percent = 100;
                    s.progress_label = None;
                    s.is_working = false;
                    match res {
                        Ok(()) => {
                            s.tool_log = Some(if lang_is_en() {
                                format!("'{}' installed.", dep_id)
                            } else {
                                format!("'{}' instalado.", dep_id)
                            })
                        }
                        Err(e) => s.tool_log = Some(format!("Error '{}': {}", dep_id, e)),
                    }
                }
                InstallJobRequest::InstallAppBatch(ids) => {
                    let total = ids.len().max(1);
                    {
                        let mut s = state_sig.write();
                        s.is_working = true;
                        s.progress_percent = 5;
                        s.progress_label = Some(format!("0/{}", total));
                        s.summary_msg = Some(if lang_is_en() {
                            format!("Queue: {} app(s)...", total)
                        } else {
                            format!("Cola: {} app(s)...", total)
                        });
                        s.app_statuses.clear();
                        for &id in &ids {
                            s.app_statuses.insert(id, InstallStatus::Queued);
                        }
                    }

                    let mut ok_count = 0usize;
                    let mut err_count = 0usize;
                    let mut cancelled = false;

                    for (idx, &pkg_id) in ids.iter().enumerate() {
                        if cancel_worker.load(Ordering::Acquire) {
                            break;
                        }

                        let pct = ((idx * 100) / total).max(5) as u8;
                        {
                            let mut s = state_sig.write();
                            s.progress_percent = pct;
                            s.progress_label = Some(format!("{}/{} • {}", idx + 1, total, pkg_id));
                            s.app_statuses.insert(pkg_id, InstallStatus::Installing);
                        }

                        let flag_clone = Arc::clone(&cancel_worker);
                        let res = tokio::task::spawn_blocking(move || {
                            AppStoreManager::install_package_cancellable(pkg_id, &flag_clone)
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(CoreError::SystemTool {
                                message: e.to_string(),
                                source: None,
                            })
                        });

                        let mut s = state_sig.write();
                        match res {
                            Ok(()) => {
                                ok_count += 1;
                                s.app_statuses.insert(pkg_id, InstallStatus::Done);
                            }
                            Err(CoreError::Cancelled) => {
                                cancelled = true;
                                s.app_statuses.insert(
                                    pkg_id,
                                    InstallStatus::Failed(
                                        if lang_is_en() { "Cancelled" } else { "Cancelado" }.into(),
                                    ),
                                );
                                break;
                            }
                            Err(e) => {
                                err_count += 1;
                                s.app_statuses
                                    .insert(pkg_id, InstallStatus::Failed(e.to_string()));
                            }
                        }
                    }

                    let refreshed_apps =
                        tokio::task::spawn_blocking(AppStoreManager::detect_installed_ids)
                            .await
                            .unwrap_or_default();

                    let mut s = state_sig.write();
                    s.installed_app_ids = refreshed_apps;
                    s.progress_percent = 100;
                    s.progress_label = None;
                    s.is_working = false;
                    // UI-02: el resumen dice qué falló o si se canceló, no un «OK» genérico.
                    let en = lang_is_en();
                    s.summary_msg = Some(match (cancelled, err_count, en) {
                        (true, _, true) => format!("Cancelled: {} installed, {} failed.", ok_count, err_count),
                        (true, _, false) => format!("Cancelado: {} instaladas, {} con error.", ok_count, err_count),
                        (false, 0, true) => format!("{} app(s) installed.", ok_count),
                        (false, 0, false) => format!("{} app(s) instalada(s).", ok_count),
                        (false, _, true) => format!("{} installed, {} failed: see each app's status.", ok_count, err_count),
                        (false, _, false) => format!("{} instaladas, {} con error: mira el estado de cada app.", ok_count, err_count),
                    });
                }
            }
        }
    });
}

#[component]
pub fn App() -> Element {
    let mut current_theme = use_signal(Theme::load_persisted);
    let mut current_lang = use_signal(Lang::load_persisted);
    let mut active_tab = use_signal(|| ActiveTab::Dashboard);
    let tools_state = use_signal(ToolsSharedState::default);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));
    let window = use_window();

    let mut system_specs = use_signal(|| None::<crate::core::specs::SystemSpecs>);
    use_future(move || async move {
        if let Ok(Ok(data)) =
            tokio::task::spawn_blocking(crate::core::specs::collect_system_specs).await
        {
            system_specs.set(Some(data));
        }
    });

    // --- Auto-Actualizador Nativo GitHub ---
    let mut update_info = use_signal(|| None::<UpdateReleaseInfo>);
    let mut show_update_modal = use_signal(|| false);
    let mut is_downloading_update = use_signal(|| false);
    let mut update_progress = use_signal(|| 0u8);
    let mut update_error = use_signal(|| None::<String>);

    use_future(move || async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        if let Ok(Ok(Some(info))) = tokio::task::spawn_blocking(check_for_updates).await {
            update_info.set(Some(info));
        }
    });

    let logo_data_uri = use_hook(|| {
        format!(
            "data:image/gif;base64,{}",
            STANDARD.encode(include_bytes!("../../assets/logo.gif"))
        )
    });

    let dispatcher = use_hook(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<InstallJobRequest>();
        run_installer_worker(rx, Arc::clone(&cancel_flag), tools_state, current_lang);
        InstallerDispatcher {
            tx,
            cancel_flag: Arc::clone(&cancel_flag),
        }
    });

    provide_context(current_theme);
    provide_context(current_lang);
    provide_context(tools_state);
    provide_context(dispatcher);

    let theme_val = *current_theme.read();
    let en = current_lang.read().is_en();
    let win_min = window.clone();
    let win_max = window.clone();
    let win_close = window.clone();
    let win_drag = window.clone();

    let pkg_ver = env!("CARGO_PKG_VERSION");
    let version_badge = if let Some(maj_min) = pkg_ver.strip_suffix(".0") {
        format!("v{}", maj_min)
    } else {
        format!("v{}", pkg_ver)
    };

    let on_start_update = move |_| {
        let Some(info) = update_info.read().clone() else {
            return;
        };
        is_downloading_update.set(true);
        update_error.set(None);
        update_progress.set(0);

        let cancel_arc = Arc::new(AtomicBool::new(false));
        let prog_arc = Arc::new(AtomicU8::new(0));

        let prog_poll = Arc::clone(&prog_arc);
        spawn(async move {
            while *is_downloading_update.read() {
                update_progress.set(prog_poll.load(Ordering::Acquire));
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
        });

        spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                let installer_path = download_and_install_update(&info, &cancel_arc, &prog_arc)?;
                // Solo vuelve si no pudo lanzar el instalador; si lo lanza, la app se cierra.
                launch_installer_and_exit(&installer_path, &info)
            })
            .await
            .unwrap_or_else(|e| Err(CoreError::Trust { message: e.to_string(), source: None }));

            if let Err(e) = res {
                update_error.set(Some(e.to_string()));
                is_downloading_update.set(false);
            }
        });
    };

    rsx! {
        style { {include_str!("../../assets/main.css")} }

        div {
            class: "app-root-frameless",
            "data-theme": "{theme_val.as_str()}",

            header {
                class: "top-titlebar",

                div {
                    class: "top-brand",
                    onmousedown: move |_| { win_drag.drag(); },
                    img {
                        class: "brand-icon",
                        src: "{logo_data_uri}",
                        alt: "Logo",
                    }
                    span { class: "top-title", "OtterSwiss" }
                    span { class: "top-version", "{version_badge}" }
                    if let Some(upd) = update_info.read().as_ref() {
                        button {
                            class: "badge badge-warning",
                            style: "cursor: pointer; margin-left: var(--space-2); padding: var(--space-1) var(--space-2); border: 1px solid var(--accent); font-size: var(--font-size-xs); font-weight: bold;",
                            onclick: move |_| show_update_modal.set(true),
                            "↑ v{upd.version}"
                        }
                    }
                }

                nav { class: "top-nav",
                    button {
                        class: if *active_tab.read() == ActiveTab::Dashboard { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Dashboard),
                        if en { "Home" } else { "Inicio" }
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Cleaner { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Cleaner),
                        if en { "Cleaner" } else { "Limpiador" }
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Debloater { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Debloater),
                        "Debloater"
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Dns { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Dns),
                        "DNS"
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Archiver { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Archiver),
                        if en { "Archiver" } else { "Archivos" }
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Tools { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| {
                            active_tab.set(ActiveTab::Tools);
                            refresh_tools_inventory(tools_state);
                        },
                        if en { "Tools & Store" } else { "Herramientas" }
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Uninstaller { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Uninstaller),
                        if en { "Uninstaller" } else { "Desinstalador" }
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Hardware { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Hardware),
                        "Hardware"
                    }
                    button {
                        class: if *active_tab.read() == ActiveTab::Sensors { "top-nav-item active" } else { "top-nav-item" },
                        onclick: move |_| active_tab.set(ActiveTab::Sensors),
                        if en { "Sensors" } else { "Sensores" }
                    }
                }

                div { class: "top-controls",
                    button {
                        class: "theme-toggle-btn",
                        title: if en { "Switch language (ES/EN)" } else { "Cambiar idioma (ES/EN)" },
                        onclick: move |_| {
                            let next = current_lang.read().toggle();
                            current_lang.set(next);
                            next.save_persisted();
                        },
                        "{current_lang.read().code()}"
                    }
                    button {
                        class: "theme-toggle-btn",
                        title: if en { "Change theme" } else { "Cambiar tema" },
                        onclick: move |_| {
                            let next = current_theme.read().next();
                            current_theme.set(next);
                            next.save_persisted();
                        },
                        "{theme_val.display_name(en)}"
                    }
                    button {
                        class: "win-btn",
                        title: if en { "Minimize" } else { "Minimizar" },
                        onclick: move |_| { win_min.set_minimized(true); },
                        "─"
                    }
                    button {
                        class: "win-btn",
                        title: if en { "Maximize" } else { "Maximizar" },
                        onclick: move |_| { win_max.toggle_maximized(); },
                        "□"
                    }
                    button {
                        class: "win-btn win-close-btn",
                        title: if en { "Close" } else { "Cerrar" },
                        onclick: move |_| { win_close.close(); },
                        "✕"
                    }
                }
            }

            // --- Modal de Actualización Disponible (UI-09) ---
            if *show_update_modal.read() {
                if let Some(upd) = update_info.read().clone() {
                    {
                        let modal_title = if en { format!("Update available: v{}", upd.version) } else { format!("Actualización disponible: v{}", upd.version) };
                        let modal_sub = if en { format!("Current version: v{} → New version: v{}", pkg_ver, upd.version) } else { format!("Versión actual: v{} → Nueva versión: v{}", pkg_ver, upd.version) };
                        let update_btn_lbl = if *is_downloading_update.read() {
                            if en { format!("Downloading {}%...", *update_progress.read()) } else { format!("Descargando {}%...", *update_progress.read()) }
                        } else if en {
                            "Download and Install".to_string()
                        } else {
                            "Descargar e Instalar".to_string()
                        };
                        let close_modal_lbl = if en { "Later" } else { "Más tarde" };
                        let notes_header = if en { "Release Notes:" } else { "Notas de la versión:" };

                        rsx! {
                            Modal {
                                title: modal_title,
                                subtitle: Some(modal_sub),
                                wide: true,
                                on_close: move |_| {
                                    if !*is_downloading_update.peek() {
                                        show_update_modal.set(false);
                                    }
                                },
                                if !upd.notes.is_empty() {
                                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                                        span { class: "text-xs text-muted", style: "font-weight: 600;", "{notes_header}" }
                                        div {
                                            class: "modal-list",
                                            style: "white-space: pre-wrap; font-family: monospace;",
                                            "{upd.notes}"
                                        }
                                    }
                                }

                                if *is_downloading_update.read() {
                                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                                        div { style: "width: 100%; height: var(--space-2); background: var(--surface); border-radius: 999px; overflow: hidden; border: 1px solid var(--border);",
                                            div { style: "height: 100%; width: {update_progress.read()}%; background: var(--accent); transition: width 0.15s ease;" }
                                        }
                                        div { class: "text-xs text-muted", style: "display: flex; justify-content: space-between;",
                                            span { "{update_progress.read()}%" }
                                            span { "{upd.asset_name}" }
                                        }
                                    }
                                }

                                if let Some(err) = update_error.read().clone() {
                                    StatusBanner { text: err, tone: Tone::Danger }
                                }

                                div { class: "modal-actions",
                                    button {
                                        class: "btn btn-secondary",
                                        disabled: *is_downloading_update.read(),
                                        onclick: move |_| show_update_modal.set(false),
                                        "{close_modal_lbl}"
                                    }
                                    button {
                                        class: "btn btn-primary",
                                        disabled: *is_downloading_update.read(),
                                        onclick: on_start_update,
                                        "{update_btn_lbl}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            main { class: "main-content-full",
                {
                    match *active_tab.read() {
                        ActiveTab::Dashboard => rsx! { DashboardView {} },
                        ActiveTab::Cleaner => rsx! { CleanerView {} },
                        ActiveTab::Debloater => rsx! { DebloaterView {} },
                        ActiveTab::Dns => rsx! { DnsView {} },
                        ActiveTab::Archiver => rsx! { ArchiverView {} },
                        ActiveTab::Tools => rsx! { ToolsView {} },
                        ActiveTab::Uninstaller => rsx! { UninstallerView {} },
                        ActiveTab::Hardware => rsx! {
                            HardwareTab {
                                specs: system_specs(),
                            }
                        },
                        ActiveTab::Sensors => rsx! { SensorsView {} },
                    }
                }
            }
        }
    }
}