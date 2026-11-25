//! Cola de instalación de apps y runtimes de Herramientas, y el estado que comparte con su vista.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::ui::components::Tone;
use crate::ui::i18n::{plural, strings, Lang};
use dioxus::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::core::error::CoreError;
use crate::core::store::app_store::AppStoreManager;
use crate::core::store::dependencies::DependencyManager;

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

/// UI-05: arranca vacío; el inventario real lo carga `refresh_tools_inventory` fuera del hilo de UI.
#[derive(Debug, Clone, Default)]
pub struct ToolsSharedState {
    pub is_working: bool,
    pub progress_percent: u8,
    pub progress_label: Option<String>,
    /// Último aviso de Herramientas con su tono (CL-10: el tono ya no se deduce del texto).
    pub tool_log: Option<(String, Tone)>,
    pub summary_msg: Option<String>,
    pub dep_states: HashMap<&'static str, bool>,
    pub installed_app_ids: HashSet<&'static str>,
    pub app_statuses: HashMap<&'static str, InstallStatus>,
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

pub(super) fn run_installer_worker(
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
                        s.progress_label = Some(format!("{}…", dep_id));
                        s.tool_log = Some((
                            format!(
                                "{} '{}'…",
                                if lang_is_en() {
                                    "Installing"
                                } else {
                                    "Instalando"
                                },
                                dep_id
                            ),
                            Tone::Info,
                        ));
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
                            s.tool_log = Some((
                                if lang_is_en() {
                                    format!("'{}' installed.", dep_id)
                                } else {
                                    format!("'{}' instalado.", dep_id)
                                },
                                Tone::Success,
                            ))
                        }
                        Err(e) => {
                            s.tool_log = Some((format!("Error '{}': {}", dep_id, e), Tone::Danger))
                        }
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
                            format!("Queue: {}…", plural(total, strings(true).app))
                        } else {
                            format!("Cola: {}…", plural(total, strings(false).app))
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
                                        if lang_is_en() {
                                            "Cancelled"
                                        } else {
                                            "Cancelado"
                                        }
                                        .into(),
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
                        (true, _, true) => {
                            format!("Cancelled: {} installed, {} failed.", ok_count, err_count)
                        }
                        (true, _, false) => format!(
                            "Cancelado: {} instaladas, {} con error.",
                            ok_count, err_count
                        ),
                        (false, 0, true) => {
                            format!("{} installed.", plural(ok_count, strings(true).app))
                        }
                        (false, 0, false) => format!(
                            "{} {}.",
                            plural(ok_count, strings(false).app),
                            if ok_count == 1 {
                                "instalada"
                            } else {
                                "instaladas"
                            }
                        ),
                        (false, _, true) => format!(
                            "{} installed, {} failed: see each app's status.",
                            ok_count, err_count
                        ),
                        (false, _, false) => format!(
                            "{} instaladas, {} con error: mira el estado de cada app.",
                            ok_count, err_count
                        ),
                    });
                }
            }
        }
    });
}
