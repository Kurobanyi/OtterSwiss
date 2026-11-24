use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::cleaner::{CategoryCleanResult, CleanProgress, SystemCleaner};
use crate::ui::components::Card;
use crate::ui::i18n::Lang;

fn format_mb(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 100.0 {
        format!("{:.0} MB", mb)
    } else {
        format!("{:.2} MB", mb)
    }
}

#[component]
pub fn CleanerView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut is_cleaning = use_signal(|| false);
    let mut results = use_signal(Vec::<CategoryCleanResult>::new);
    let mut clean_all_temps = use_signal(|| false);
    let mut clean_recycle = use_signal(|| true);
    let mut show_confirm_modal = use_signal(|| false);
    let mut progress_state = use_signal(CleanProgress::default);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));

    let cancel_for_start = Arc::clone(&cancel_flag);
    let mut start_cleanup = move || {
        let min_age_hours = if *clean_all_temps.read() { 0 } else { 24 };
        let recycle = *clean_recycle.read();

        cancel_for_start.store(false, Ordering::Release);
        is_cleaning.set(true);
        progress_state.set(CleanProgress {
            current_step: 0,
            total_steps: 6,
            current_category: if en { "Starting..." } else { "Iniciando..." }.to_string(),
            percent: 2,
            bytes_freed_so_far: 0,
            files_removed_so_far: 0,
        });

        let prog_arc = Arc::new(Mutex::new(CleanProgress::default()));
        let prog_worker = Arc::clone(&prog_arc);
        let cancel_worker = Arc::clone(&cancel_for_start);

        spawn(async move {
            let handle = tokio::task::spawn_blocking(move || {
                SystemCleaner::run_full_cleanup_with_progress(
                    min_age_hours,
                    recycle,
                    &cancel_worker,
                    Some(&prog_worker),
                )
            });

            while !handle.is_finished() {
                if let Ok(snap) = prog_arc.lock() {
                    progress_state.set(snap.clone());
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }

            let res = handle.await.unwrap_or_default();
            if let Ok(snap) = prog_arc.lock() {
                progress_state.set(snap.clone());
            }
            results.set(res);
            is_cleaning.set(false);
        });
    };

    let total_freed: u64 = results.read().iter().map(|r| r.bytes_freed).sum();
    let total_files: usize = results.read().iter().map(|r| r.files_removed).sum();
    let prog = progress_state.read();
    let cancel_for_btn = Arc::clone(&cancel_flag);

    let cleaner_title = if en { "System Cleaner" } else { "Limpiador de Sistema" }.to_string();
    let cleaner_sub = Some((if en { "Safe temp & cache purge" } else { "Purga segura de temporales y caché" }).to_string());
    let prog_title = if en { "Cleanup Progress" } else { "Progreso" }.to_string();
    let freed_title = if en { "Space Freed" } else { "Espacio Liberado" }.to_string();
    let files_title = if en { "Files Removed" } else { "Archivos Borrados" }.to_string();
    let details_title = if en { "Details" } else { "Detalle" }.to_string();

    rsx! {
        div { class: "content-body",
            if *show_confirm_modal.read() {
                div {
                    style: "position: fixed; inset: 0; background: rgba(0, 0, 0, 0.72); display: flex; align-items: center; justify-content: center; z-index: 9999; padding: 24px;",
                    div {
                        class: "card",
                        style: "max-width: 460px; width: 100%; border: 1px solid var(--warning);",
                        div { class: "card-header",
                            div {
                                h3 { class: "card-title", if en { "Before Cleaning" } else { "Antes de Limpiar" } }
                                p { class: "card-subtitle", if en { "Close open apps except OtterSwiss to avoid locked files." } else { "Cierra las apps en uso excepto OtterSwiss." } }
                            }
                        }
                        div { style: "display: flex; justify-content: flex-end; gap: var(--space-3);",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| show_confirm_modal.set(false),
                                if en { "Cancel" } else { "Cancelar" }
                            }
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    show_confirm_modal.set(false);
                                    start_cleanup();
                                },
                                if en { "Clean Now" } else { "Limpiar Ahora" }
                            }
                        }
                    }
                }
            }

            Card {
                title: cleaner_title,
                subtitle: cleaner_sub,
                div { style: "display: flex; flex-direction: column; gap: var(--space-3);",
                    div { style: "display: flex; gap: var(--space-4); align-items: center; flex-wrap: wrap;",
                        label { style: "display: flex; align-items: center; gap: var(--space-2); cursor: pointer;",
                            input {
                                r#type: "checkbox",
                                checked: *clean_recycle.read(),
                                disabled: *is_cleaning.read(),
                                onchange: move |evt| clean_recycle.set(evt.checked()),
                            }
                            span { if en { "Empty Recycle Bin" } else { "Vaciar Papelera" } }
                        }

                        label { style: "display: flex; align-items: center; gap: var(--space-2); cursor: pointer;",
                            input {
                                r#type: "checkbox",
                                checked: *clean_all_temps.read(),
                                disabled: *is_cleaning.read(),
                                onchange: move |evt| clean_all_temps.set(evt.checked()),
                            }
                            span { if en { "All temp files (skip 24h wait)" } else { "Todo el directorio (sin espera de 24h)" } }
                        }
                    }

                    div { style: "display: flex; gap: var(--space-3); align-items: center; flex-wrap: wrap;",
                        span { class: "badge badge-info",
                            if *clean_all_temps.read() {
                                if en { "Mode: Full (0h)" } else { "Modo: Total (0h)" }
                            } else if en {
                                "Mode: Safe (> 24h)"
                            } else {
                                "Modo: Seguro (> 24h)"
                            }
                        }

                        button {
                            class: "btn btn-primary",
                            disabled: *is_cleaning.read(),
                            onclick: move |_| show_confirm_modal.set(true),
                            if *is_cleaning.read() {
                                if en { "Cleaning..." } else { "Limpiando..." }
                            } else if en {
                                "Start Cleanup"
                            } else {
                                "Iniciar Limpieza"
                            }
                        }

                        if *is_cleaning.read() {
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| cancel_for_btn.store(true, Ordering::Release),
                                if en { "Cancel" } else { "Cancelar" }
                            }
                        }
                    }
                }
            }

            if *is_cleaning.read() {
                Card {
                    title: prog_title,
                    subtitle: Some(format!(
                        "{} ({}/{}) • {}",
                        prog.current_category,
                        prog.current_step,
                        prog.total_steps.max(1),
                        format_mb(prog.bytes_freed_so_far)
                    )),
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        div { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                            "{prog.percent}% • {prog.files_removed_so_far} "
                            {if en { "files" } else { "archivos" }}
                        }
                        div {
                            style: "width: 100%; height: 10px; background: var(--bg); border-radius: 999px; overflow: hidden; border: 1px solid var(--border);",
                            div { style: "height: 100%; width: {prog.percent}%; background: var(--accent); transition: width 0.2s ease;" }
                        }
                    }
                }
            }

            if !results.read().is_empty() {
                div { class: "cards-grid",
                    Card {
                        title: freed_title,
                        subtitle: None,
                        h2 { style: "color: var(--success); font-size: var(--font-size-xl);", "{format_mb(total_freed)}" }
                    }
                    Card {
                        title: files_title,
                        subtitle: None,
                        h2 { style: "color: var(--accent); font-size: var(--font-size-xl);", "{total_files}" }
                    }
                }

                Card {
                    title: details_title,
                    subtitle: None,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        for item in results.read().iter() {
                            div {
                                key: "{item.category}",
                                style: "display: flex; justify-content: space-between; align-items: center; padding: 6px 0; border-bottom: 1px solid var(--border);",
                                div {
                                    div { style: "font-weight: 600;", "{item.category}" }
                                    div { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                                        if en {
                                            "Removed: {item.files_removed} | Skipped: {item.skipped_recent_or_locked}"
                                        } else {
                                            "Borrados: {item.files_removed} | Omitidos: {item.skipped_recent_or_locked}"
                                        }
                                    }
                                }
                                div { style: "font-weight: 600; color: var(--success);", "{format_mb(item.bytes_freed)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}