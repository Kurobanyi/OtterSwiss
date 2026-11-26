use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::core::cleaner::{self, CategoryCleanResult, CleanProgress};
use crate::ui::components::{Card, CheckRow, ConfirmDialog, ProgressBar, StatusBanner, Tone};
use crate::ui::format::bytes_mb;
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, strings};
use crate::ui::task::run_with_progress;
#[component]
pub fn CleanerView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let txt = strings(en).cleaner;

    let mut is_cleaning = use_signal(|| false);
    let mut results = use_signal(Vec::<CategoryCleanResult>::new);
    // El banner de éxito no debe salir si la limpieza se canceló a mitad.
    let mut was_cancelled = use_signal(|| false);
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
        let cancel_check = Arc::clone(&cancel_for_start);

        spawn(async move {
            let res = run_with_progress(progress_state, move |prog| {
                cleaner::run_full_cleanup_with_progress(
                    min_age_hours,
                    recycle,
                    &cancel_worker,
                    Some(prog),
                )
            })
            .await
            .unwrap_or_else(|e| {
                vec![CategoryCleanResult {
                    category: txt.cleanup.to_string(),
                    error: Some(e.text(en)),
                    ..Default::default()
                }]
            });
            was_cancelled.set(cancel_check.load(Ordering::Acquire));
            results.set(res);
            is_cleaning.set(false);
        });
    };

    let total_freed: u64 = results.read().iter().map(|r| r.bytes_freed).sum();
    let total_files: usize = results.read().iter().map(|r| r.files_removed).sum();
    let error_count = results.read().iter().filter(|r| r.error.is_some()).count();
    let prog = progress_state.read();
    let cancel_for_btn = Arc::clone(&cancel_flag);

    let prog_title = txt.cleanup_progress.to_string();

    rsx! {
        div { class: "content-body",
            if *show_confirm_modal.read() {
                ConfirmDialog {
                    title: txt.before_cleaning.to_string(),
                    subtitle: Some(txt.close_open_apps_except_otterswiss.to_string()),
                    confirm_label: txt.clean_now.to_string(),
                    cancel_label: strings(en).cancel.to_string(),
                    tone: if *clean_recycle.read() { Tone::Danger } else { Tone::Warning },
                    on_cancel: move |_| show_confirm_modal.set(false),
                    on_confirm: move |_| {
                        show_confirm_modal.set(false);
                        start_cleanup();
                    },
                    if *clean_recycle.read() {
                        StatusBanner {
                            text: txt.the_recycle_bin_will_be.to_string(),
                            tone: Tone::Warning,
                        }
                    }
                }
            }

            div { class: "card",
                div { class: "row-between gap-4 wrap",
                    div { class: "grow-wide stack-1",
                        div { class: "card-title", "{txt.system_cleaner}" }
                        div { class: "text-muted", "{txt.safe_temp_cache_purge}" }
                        div {
                            span { class: "badge badge-info",
                                if *clean_all_temps.read() {
                                    "{txt.mode_full_0h}"
                                } else { "{txt.mode_safe_24h}" }
                            }
                        }
                    }
                    div { class: "row",
                        button {
                            class: "btn btn-primary",
                            disabled: *is_cleaning.read(),
                            onclick: move |_| show_confirm_modal.set(true),
                            if *is_cleaning.read() {
                                "{txt.cleaning}"
                            } else { "{txt.start_cleanup}" }
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
                            {"{txt.files}"}
                        }
                        ProgressBar { percent: prog.percent }
                    }
                }
            }

            if !results.read().is_empty() {
                if *was_cancelled.read() {
                    StatusBanner { text: txt.cleanup_cancelled.to_string(), tone: Tone::Warning }
                } else if error_count > 0 {
                    StatusBanner {
                        text: (strings(en).cleaner.categories_could_not_be_cleaned)(&error_count),
                        tone: Tone::Warning,
                    }
                } else {
                    StatusBanner { text: txt.cleanup_done.to_string(), tone: Tone::Success }
                }
            }

            div { class: "section-title", "{txt.options}" }
            div { class: "stack-1",
                CheckRow {
                    class: "item-row gap-3",
                    checked: *clean_recycle.read(),
                    disabled: *is_cleaning.read(),
                    on_change: move |on| clean_recycle.set(on),
                    span { class: "fluent-icon cln-icon", "aria-hidden": "true", "\u{E74D}" }
                    div { class: "grow stack-1",
                        span { "{txt.empty_recycle_bin_permanent}" }
                        span { class: "text-xs text-muted", "{txt.empty_recycle_bin_desc}" }
                    }
                }
                CheckRow {
                    class: "item-row gap-3",
                    checked: *clean_all_temps.read(),
                    disabled: *is_cleaning.read(),
                    on_change: move |on| clean_all_temps.set(on),
                    span { class: "fluent-icon cln-icon", "aria-hidden": "true", "\u{E75C}" }
                    div { class: "grow stack-1",
                        span { "{txt.all_temp_files_skip_24h}" }
                        span { class: "text-xs text-muted", "{txt.all_temp_files_desc}" }
                    }
                }
            }

            if !results.read().is_empty() {
                div { class: "section-title", "{txt.results}" }
                div { class: "stack-1",
                    div { class: "item-row row-between gap-3",
                        span { "{txt.space_freed}" }
                        span { class: "text-xl fw-semibold text-success", "{bytes_mb(total_freed)}" }
                    }
                    div { class: "item-row row-between gap-3",
                        span { "{txt.files_removed}" }
                        span { class: "fw-semibold", "{total_files}" }
                    }
                }

                div { class: "section-title", "{txt.details}" }
                div { class: "stack-1",
                    for item in results.read().iter() {
                        div { key: "{item.category}", class: "item-row row-between gap-3",
                            div { class: "grow stack-1",
                                span { class: "fw-semibold", "{tr(&item.category, en)}" }
                                span { class: "text-xs text-muted",
                                    "{(strings(en).cleaner.removed_skipped)(&item.files_removed, &item.skipped_recent_or_locked)}"
                                }
                                // ERR-07 / SYS-23: lo omitido o fallido se dice (p. ej. Windows Update activo).
                                if let Some(err) = item.error.as_ref() {
                                    span { class: "text-xs",
                                        span { class: "badge badge-warning cln-badge", "{txt.skipped_reason}" }
                                        span { class: "text-warning", "{err}" }
                                    }
                                }
                            }
                            span { class: "fw-semibold text-success", "{bytes_mb(item.bytes_freed)}" }
                        }
                    }
                }
            }
        }
    }
}
