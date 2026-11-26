use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::core::cleaner::{CategoryCleanResult, CleanProgress, SystemCleaner};
use crate::ui::components::{Card, CheckRow, ConfirmDialog, ProgressBar, StatusBanner, Tone};
use crate::ui::format::bytes_mb;
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, strings};
use crate::ui::task::run_with_progress;
#[component]
pub fn CleanerView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut is_cleaning = use_signal(|| false);
    let mut results = use_signal(Vec::<CategoryCleanResult>::new);
    let mut clean_all_temps = use_signal(|| false);
    // ERR-07: vaciar la Papelera es irreversible; viene desmarcado.
    let mut clean_recycle = use_signal(|| false);
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
            // AR-16: el total lo pone el core en el primer paso; aquí no se inventa.
            total_steps: 0,
            current_category: strings(en).starting.to_string(),
            percent: 2,
            bytes_freed_so_far: 0,
            files_removed_so_far: 0,
        });

        let cancel_worker = Arc::clone(&cancel_for_start);

        spawn(async move {
            let res = run_with_progress(progress_state, move |prog| {
                SystemCleaner::run_full_cleanup_with_progress(
                    min_age_hours,
                    recycle,
                    &cancel_worker,
                    Some(prog),
                )
            })
            .await
            .unwrap_or_else(|e| {
                vec![CategoryCleanResult {
                    category: if en { "Cleanup" } else { "Limpieza" }.to_string(),
                    error: Some(e.text(en)),
                    ..Default::default()
                }]
            });
            results.set(res);
            is_cleaning.set(false);
        });
    };

    let total_freed: u64 = results.read().iter().map(|r| r.bytes_freed).sum();
    let total_files: usize = results.read().iter().map(|r| r.files_removed).sum();
    let error_count = results.read().iter().filter(|r| r.error.is_some()).count();
    let prog = progress_state.read();
    let cancel_for_btn = Arc::clone(&cancel_flag);

    let cleaner_title = if en {
        "System Cleaner"
    } else {
        "Limpiador de Sistema"
    }
    .to_string();
    let cleaner_sub = Some(
        (if en {
            "Safe temp & cache purge"
        } else {
            "Purga segura de temporales y caché"
        })
        .to_string(),
    );
    let prog_title = if en { "Cleanup Progress" } else { "Progreso" }.to_string();
    let freed_title = if en {
        "Space Freed"
    } else {
        "Espacio Liberado"
    }
    .to_string();
    let files_title = if en {
        "Files Removed"
    } else {
        "Archivos Borrados"
    }
    .to_string();
    let details_title = if en { "Details" } else { "Detalle" }.to_string();

    rsx! {
        div { class: "content-body",
            if *show_confirm_modal.read() {
                ConfirmDialog {
                    title: if en { "Before cleaning".to_string() } else { "Antes de limpiar".to_string() },
                    subtitle: Some(if en {
                        "Close open apps except OtterSwiss to avoid locked files.".to_string()
                    } else {
                        "Cierra las apps abiertas, menos OtterSwiss, para evitar archivos en uso.".to_string()
                    }),
                    confirm_label: if en { "Clean now".to_string() } else { "Limpiar ahora".to_string() },
                    cancel_label: strings(en).cancel.to_string(),
                    tone: if *clean_recycle.read() { Tone::Danger } else { Tone::Warning },
                    on_cancel: move |_| show_confirm_modal.set(false),
                    on_confirm: move |_| {
                        show_confirm_modal.set(false);
                        start_cleanup();
                    },
                    if *clean_recycle.read() {
                        StatusBanner {
                            text: if en {
                                "The Recycle Bin will be emptied: its files cannot be recovered afterwards.".to_string()
                            } else {
                                "Se vaciará la Papelera: sus archivos no se podrán recuperar después.".to_string()
                            },
                            tone: Tone::Warning,
                        }
                    }
                }
            }

            Card {
                title: cleaner_title,
                subtitle: cleaner_sub,
                div { class: "stack-3",
                    div { class: "row wrap gap-4",
                        CheckRow {
                            checked: *clean_recycle.read(),
                            disabled: *is_cleaning.read(),
                            on_change: move |on| clean_recycle.set(on),
                            span { if en { "Empty Recycle Bin (permanent)" } else { "Vaciar Papelera (permanente)" } }
                        }

                        CheckRow {
                            checked: *clean_all_temps.read(),
                            disabled: *is_cleaning.read(),
                            on_change: move |on| clean_all_temps.set(on),
                            span { if en { "All temp files (skip 24h wait)" } else { "Todo el directorio (sin espera de 24h)" } }
                        }
                    }

                    div { class: "row gap-3 wrap",
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
                                if en { "Cleaning…" } else { "Limpiando…" }
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
                                "{strings(en).cancel}"
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
                        tr(&prog.current_category, en),
                        prog.current_step,
                        prog.total_steps.max(1),
                        bytes_mb(prog.bytes_freed_so_far)
                    )),
                    div { class: "stack-2",
                        div { class: "text-xs text-muted",
                            "{prog.percent}% • {prog.files_removed_so_far} "
                            {if en { "files" } else { "archivos" }}
                        }
                        ProgressBar { percent: prog.percent }
                    }
                }
            }

            if !results.read().is_empty() && error_count > 0 {
                StatusBanner {
                    text: if en {
                        format!("{} categories could not be cleaned. See the details.", error_count)
                    } else {
                        format!("{} categorías no se pudieron limpiar. Mira el detalle.", error_count)
                    },
                    tone: Tone::Warning,
                }
            }

            if !results.read().is_empty() {
                div { class: "cards-grid",
                    Card {
                        title: freed_title,
                        subtitle: None,
                        h2 { class: "text-xl text-success", "{bytes_mb(total_freed)}" }
                    }
                    Card {
                        title: files_title,
                        subtitle: None,
                        h2 { class: "text-xl text-accent", "{total_files}" }
                    }
                }

                Card {
                    title: details_title,
                    subtitle: None,
                    div { class: "stack-2",
                        for item in results.read().iter() {
                            div {
                                key: "{item.category}",
                                class: "list-row",
                                div {
                                    div { class: "fw-semibold", "{tr(&item.category, en)}" }
                                    div { class: "text-xs text-muted",
                                        if en {
                                            "Removed: {item.files_removed} | Skipped: {item.skipped_recent_or_locked}"
                                        } else {
                                            "Borrados: {item.files_removed} | Omitidos: {item.skipped_recent_or_locked}"
                                        }
                                    }
                                    // ERR-07 / SYS-23: lo omitido o fallido se dice (p. ej. Windows Update activo).
                                    if let Some(err) = item.error.as_ref() {
                                        div { class: "text-xs text-warning", "{err}" }
                                    }
                                }
                                div { class: "fw-semibold text-success", "{bytes_mb(item.bytes_freed)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
