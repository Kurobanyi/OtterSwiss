use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::core::archive::{
    self, ArchiveFormat, ArchiveMode, ArchiveProgress, DEFAULT_EXTRACT_QUOTA,
    resolve_extract_destination,
};
use crate::ui::components::{Card, Modal, ProgressBar, StatusBanner, Tone};
use crate::ui::format::{bytes_mb, eta, mb_1};
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::run_with_progress;

#[component]
pub fn ArchiverView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let txt = strings(en).archiver;

    let mut mode = use_signal(|| ArchiveMode::Extract);
    let mut src_path = use_signal(String::new);
    let mut dest_path = use_signal(String::new);
    let mut selected_format = use_signal(|| ArchiveFormat::Zip);
    let mut is_working = use_signal(|| false);
    let mut progress_state = use_signal(ArchiveProgress::default);
    let mut status_msg = use_signal(|| Option::<(Msg, Tone)>::None);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));

    let cancel_for_exec = Arc::clone(&cancel_flag);
    let on_execute = move |_| {
        let current_mode = *mode.read();
        let src = PathBuf::from(src_path.read().trim().trim_matches('"'));
        let typed_dest = PathBuf::from(dest_path.read().trim().trim_matches('"'));
        // Extraer nunca borra ni mezcla una carpeta existente: se usa la hermana libre «nombre (2)».
        let dest = if current_mode == ArchiveMode::Extract {
            resolve_extract_destination(&typed_dest)
        } else {
            typed_dest
        };
        let fmt = *selected_format.read();

        if !src.exists() {
            status_msg.set(Some((
                Msg::new(|en| strings(en).archiver.source_path_does_not_exist.to_string()),
                Tone::Danger,
            )));
            return;
        }

        cancel_for_exec.store(false, Ordering::Release);
        is_working.set(true);
        status_msg.set(None);
        progress_state.set(ArchiveProgress::default());

        let cancel_worker = Arc::clone(&cancel_for_exec);

        let dest_display = dest.display().to_string();
        spawn(async move {
            let res = run_with_progress(progress_state, move |prog| {
                if current_mode == ArchiveMode::Extract {
                    archive::extract_archive_with_progress(
                        &src,
                        &dest,
                        DEFAULT_EXTRACT_QUOTA,
                        &cancel_worker,
                        Some(prog),
                    )
                } else {
                    archive::compress_path_with_progress(
                        &src,
                        &dest,
                        fmt,
                        &cancel_worker,
                        Some(prog),
                    )
                }
                .map_err(Msg::from)
            })
            .await
            .and_then(|r| r);

            match res {
                Ok(bytes) => {
                    let text = Msg::new(move |en| {
                        (strings(en).archiver.done_processed)(&bytes_mb(bytes), &dest_display)
                    });
                    status_msg.set(Some((text, Tone::Success)));
                }
                Err(err) => {
                    status_msg.set(Some((err, Tone::Danger)));
                }
            }
            is_working.set(false);
        });
    };

    let cancel_for_btn = Arc::clone(&cancel_flag);
    let prog = progress_state.read();
    let mb_done = mb_1(prog.bytes_processed);
    let mb_total = mb_1(prog.total_bytes);
    let eta_txt = eta(prog.eta_seconds);

    let card_title = txt.secure_archiver_zip_7z_tar.to_string();
    let card_sub = Some(txt.n_100_open_source_rust_compression.to_string());

    rsx! {
        div { class: "content-body",
            // UI-09: diálogo de progreso; Escape no cancela por accidente (para eso está «Cancelar»).
            if *is_working.read() {
                Modal {
                    title: match (*mode.read() == ArchiveMode::Extract, en) {
                        (true, true) => "Extracting archive…".to_string(),
                        (true, false) => "Extrayendo archivo…".to_string(),
                        (false, true) => "Compressing…".to_string(),
                        (false, false) => "Comprimiendo…".to_string(),
                    },
                    subtitle: Some(if prog.current_file.is_empty() { "…".to_string() } else { prog.current_file.clone() }),
                    on_close: move |_| {},
                        ProgressBar { percent: prog.percent }
                        div { class: "row-between text-xs text-muted",
                            span { "{prog.percent}% • {prog.files_done}/{prog.total_files} • {mb_done}/{mb_total} MB" }
                            span { "{prog.speed_mbps:.1} MB/s • {strings(en).time_left} {eta_txt}" }
                        }
                        div { class: "modal-actions",
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| cancel_for_btn.store(true, Ordering::Release),
                                "{strings(en).cancel}"
                            }
                        }
                }
            }

            Card {
                title: card_title,
                subtitle: card_sub,
                div { class: "stack-3",
                    div { class: "flex gap-2",
                        button {
                            class: "btn btn-secondary",
                            "aria-pressed": if *mode.read() == ArchiveMode::Extract { "true" } else { "false" },
                            disabled: *is_working.read(),
                            onclick: move |_| mode.set(ArchiveMode::Extract),
                            "{txt.extract}"
                        }
                        button {
                            class: "btn btn-secondary",
                            "aria-pressed": if *mode.read() == ArchiveMode::Compress { "true" } else { "false" },
                            disabled: *is_working.read(),
                            onclick: move |_| mode.set(ArchiveMode::Compress),
                            "{txt.compress}"
                        }
                    }

                    div { class: "stack-2",
                        label { class: "text-xs text-muted",
                            if *mode.read() == ArchiveMode::Extract {
                                "{txt.archive_path_zip_7z_tar}"
                            } else { "{txt.source_file_or_folder}" }
                        }
                        input {
                            r#type: "text",
                            placeholder: "C:\\Path\\To\\Target",
                            value: "{src_path.read()}",
                            disabled: *is_working.read(),
                            class: "input",
                            oninput: move |evt| src_path.set(evt.value()),
                        }

                        label { class: "text-xs text-muted",
                            if *mode.read() == ArchiveMode::Extract {
                                "{txt.output_folder}"
                            } else { "{txt.output_archive_file}" }
                        }
                        input {
                            r#type: "text",
                            placeholder: if *mode.read() == ArchiveMode::Extract { "C:\\Extracted" } else { "C:\\Backup\\Archive.zip" },
                            value: "{dest_path.read()}",
                            disabled: *is_working.read(),
                            class: "input",
                            oninput: move |evt| dest_path.set(evt.value()),
                        }

                        if *mode.read() == ArchiveMode::Compress {
                            div { class: "row mt-1",
                                span { class: "text-xs text-muted", "{txt.format}" }
                                button {
                                    class: "btn btn-secondary",
                            "aria-pressed": if *selected_format.read() == ArchiveFormat::Zip { "true" } else { "false" },
                                    disabled: *is_working.read(),
                                    onclick: move |_| selected_format.set(ArchiveFormat::Zip),
                                    ".ZIP"
                                }
                                button {
                                    class: "btn btn-secondary",
                            "aria-pressed": if *selected_format.read() == ArchiveFormat::SevenZip { "true" } else { "false" },
                                    disabled: *is_working.read(),
                                    onclick: move |_| selected_format.set(ArchiveFormat::SevenZip),
                                    ".7Z"
                                }
                                button {
                                    class: "btn btn-secondary",
                            "aria-pressed": if *selected_format.read() == ArchiveFormat::TarGz { "true" } else { "false" },
                                    disabled: *is_working.read(),
                                    onclick: move |_| selected_format.set(ArchiveFormat::TarGz),
                                    ".TAR.GZ"
                                }
                            }
                        }

                        div { class: "flex gap-2 mt-2",
                            button {
                                class: "btn btn-primary",
                                disabled: *is_working.read() || src_path.read().trim().is_empty() || dest_path.read().trim().is_empty(),
                                onclick: on_execute,
                                if *mode.read() == ArchiveMode::Extract {
                                    "{txt.extract_now}"
                                } else { "{txt.compress_now}" }
                            }
                        }
                    }

                    if let Some((msg, tone)) = status_msg.read().clone() {
                        StatusBanner { text: msg.text(en), tone }
                    }
                }
            }
        }
    }
}
