// src/ui/mod.rs

pub mod components;
pub mod i18n;
pub mod theme;
pub mod views;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::core::dependencies::DependencyManager;
use crate::core::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
) {
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
                        s.tool_log = Some(format!("Instalando '{}'...", dep_id));
                    }

                    let res = tokio::task::spawn_blocking(move || {
                        DependencyManager::install_runtime(dep_id)
                    })
                    .await
                    .unwrap_or_else(|e| {
                        Err(CoreError::SystemTool {
                            message: format!("Fallo interno de hilo: {}", e),
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
                        Ok(()) => s.tool_log = Some(format!("'{}' OK.", dep_id)),
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
                        s.summary_msg = Some(format!("Cola: {} app(s)...", total));
                        s.app_statuses.clear();
                        for &id in &ids {
                            s.app_statuses.insert(id, InstallStatus::Queued);
                        }
                    }

                    let mut ok_count = 0usize;
                    let mut err_count = 0usize;

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
                                message: format!("Fallo interno de hilo: {}", e),
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
                                s.app_statuses
                                    .insert(pkg_id, InstallStatus::Failed("Cancelado".into()));
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
                    s.summary_msg = Some(format!("OK: {} | Error: {}", ok_count, err_count));
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

    let logo_data_uri = use_hook(|| {
        format!(
            "data:image/gif;base64,{}",
            STANDARD.encode(include_bytes!("../../assets/logo.gif"))
        )
    });

    let dispatcher = use_hook(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<InstallJobRequest>();
        run_installer_worker(rx, Arc::clone(&cancel_flag), tools_state);
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