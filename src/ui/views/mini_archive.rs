//! Mini ventana del archivador: extrae o comprime lo que se eligió desde el menú contextual de Windows.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::desktop::use_window;
use dioxus::prelude::*;

use crate::core::archive::{
    self, ArchiveFormat, ArchiveMode, ArchiveProgress, DEFAULT_EXTRACT_QUOTA, default_extract_dir,
    default_zip_path, resolve_extract_destination,
};
use crate::core::sys_tools::open_folder_in_explorer;
use crate::ui::components::{StatusBanner, Tone};
use crate::ui::document::sync_document_attrs;
use crate::ui::format::{eta, mb_1};
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::run_with_progress;
use crate::ui::theme::Theme;

/// Trabajo que llega por la línea de comandos (`--extract` o `--compress-zip`).
#[derive(Debug, Clone)]
pub struct MiniArchiveJob {
    pub mode: ArchiveMode,
    pub src: PathBuf,
    pub dest: PathBuf,
}

impl MiniArchiveJob {
    /// Extrae junto al archivo, nunca sobre una carpeta existente (usa la hermana libre «nombre (2)»).
    pub fn extract(src: PathBuf) -> Self {
        let dest = resolve_extract_destination(&default_extract_dir(&src));
        Self {
            mode: ArchiveMode::Extract,
            src,
            dest,
        }
    }

    pub fn compress_zip(src: PathBuf) -> Self {
        let dest = default_zip_path(&src);
        Self {
            mode: ArchiveMode::Compress,
            src,
            dest,
        }
    }
}

/// El trabajo se entrega con `LaunchBuilder::with_context`.
#[component]
pub fn MiniArchiveView() -> Element {
    let job = use_context::<MiniArchiveJob>();
    let window = use_window();
    let theme = use_hook(Theme::load_persisted);
    let lang = use_hook(Lang::load_persisted);
    let en = lang.is_en();
    let txt = strings(en).mini_archive;

    let progress = use_signal(ArchiveProgress::default);
    let mut is_running = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<Msg>::None);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));

    let worker_job = job.clone();
    let cancel_for_hook = Arc::clone(&cancel_flag);
    use_hook(move || {
        let cancel_worker = Arc::clone(&cancel_for_hook);

        spawn(async move {
            let res = run_with_progress(progress, move |prog| {
                match worker_job.mode {
                    ArchiveMode::Extract => archive::extract_archive_with_progress(
                        &worker_job.src,
                        &worker_job.dest,
                        DEFAULT_EXTRACT_QUOTA,
                        &cancel_worker,
                        Some(prog),
                    ),
                    ArchiveMode::Compress => archive::compress_path_with_progress(
                        &worker_job.src,
                        &worker_job.dest,
                        ArchiveFormat::Zip,
                        &cancel_worker,
                        Some(prog),
                    ),
                }
                .map_err(Msg::from)
            })
            .await
            .and_then(|r| r);
            if let Err(err) = res {
                error_msg.set(Some(err));
            }
            is_running.set(false);
        });
    });

    use_effect(move || sync_document_attrs(theme.css_name(), lang.html_code()));

    let prog = progress.read();
    let running = *is_running.read();
    let err_opt = error_msg.read().as_ref().map(|m| m.text(en));
    let win_drag = window.clone();
    let win_drag_fill = window.clone();
    let win_close = window.clone();
    let win_done = window;
    let cancel_btn = Arc::clone(&cancel_flag);
    let cancel_close = Arc::clone(&cancel_flag);
    let dest_for_open = job.dest.clone();

    let title_txt = match job.mode {
        ArchiveMode::Extract => txt.extracting,
        ArchiveMode::Compress => txt.compressing_zip,
    };
    let failed = err_opt.is_some();
    let (state_title, state_icon, state_class) = if running {
        (
            title_txt,
            match job.mode {
                ArchiveMode::Extract => "\u{E896}",
                ArchiveMode::Compress => "\u{E7B8}",
            },
            "text-accent",
        )
    } else if failed {
        (txt.failed, "\u{EA39}", "text-danger")
    } else {
        (txt.completed, "\u{E73E}", "text-success")
    };
    let dest_text = dest_for_open.display().to_string();
    let current_file_lbl = if prog.current_file.is_empty() {
        txt.preparing_files.to_string()
    } else {
        prog.current_file.clone()
    };
    let cancel_lbl = strings(en).cancel;
    let open_folder_lbl = strings(en).open_folder;
    let close_lbl = strings(en).close;

    let mb_done = mb_1(prog.bytes_processed);
    let mb_total = mb_1(prog.total_bytes);
    let eta_txt = eta(prog.eta_seconds);

    rsx! {
        div {
            class: "app-root-frameless mini-window",
            "data-theme": "{theme.css_name()}",

            header {
                class: "top-titlebar",
                div {
                    class: "top-brand",
                    onmousedown: move |_| { win_drag.drag(); },
                    span { class: "top-title", "OtterSwiss" }
                }
                div {
                    class: "titlebar-drag",
                    onmousedown: move |_| { win_drag_fill.drag(); },
                }
                button {
                    class: "win-btn win-close-btn",
                    title: "{close_lbl}",
                    "aria-label": "{close_lbl}",
                    onclick: move |_| {
                        cancel_close.store(true, Ordering::Release);
                        win_close.close();
                    },
                    "\u{E8BB}"
                }
            }

            div { class: "mini-window-body",
                div { class: "stack-2",
                    div { class: "row",
                        span { class: "fluent-icon {state_class}", "aria-hidden": "true", "{state_icon}" }
                        h1 { class: "mini-title", role: "status", "{state_title}" }
                    }
                    if let Some(err) = err_opt {
                        StatusBanner { text: err, tone: Tone::Danger }
                    } else if running {
                        div { class: "stack-2",
                            div { class: "text-xs text-muted truncate", title: "{current_file_lbl}", "{current_file_lbl}" }
                            div {
                                class: "progress",
                                role: "progressbar",
                                "aria-label": "{state_title}",
                                "aria-valuemin": "0",
                                "aria-valuemax": "100",
                                "aria-valuenow": "{prog.percent}",
                                div { class: "progress-fill", style: "width: {prog.percent}%;" }
                            }
                            div { class: "row-between text-xs text-muted",
                                span { "{prog.percent}% • {prog.files_done}/{prog.total_files} • {mb_done}/{mb_total} MB" }
                                span { "{prog.speed_mbps:.1} MB/s • {strings(en).time_left} {eta_txt}" }
                            }
                        }
                    } else {
                        div { class: "stack-1",
                            span { class: "text-xs text-muted", "{txt.destination}" }
                            span { class: "mono text-xs mini-path", title: "{dest_text}", "{dest_text}" }
                        }
                    }
                }

                div { class: "row-end",
                    if running {
                        button {
                            class: "btn btn-danger btn-sm",
                            onclick: move |_| cancel_btn.store(true, Ordering::Release),
                            "{cancel_lbl}"
                        }
                    } else {
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| open_folder_in_explorer(&dest_for_open),
                            "{open_folder_lbl}"
                        }
                        button {
                            class: "btn btn-primary btn-sm",
                            onclick: move |_| win_done.close(),
                            "{close_lbl}"
                        }
                    }
                }
            }
        }
    }
}
