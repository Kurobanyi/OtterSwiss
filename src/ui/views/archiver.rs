use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::core::archive::{
    self, ArchiveFormat, ArchiveMode, ArchiveProgress, DEFAULT_EXTRACT_QUOTA,
    resolve_extract_destination,
};
use crate::core::sys_tools::open_folder_in_explorer;
use crate::ui::components::{InfoBar, Modal, ProgressBar, StatusBanner, Tone};
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
    // UI-25: destino del último trabajo terminado, para mostrarlo y abrir su carpeta.
    let mut last_dest = use_signal(|| Option::<PathBuf>::None);
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
        last_dest.set(None);
        progress_state.set(ArchiveProgress::default());

        let cancel_worker = Arc::clone(&cancel_for_exec);

        let dest_display = dest.display().to_string();
        let dest_done = dest.clone();
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
                    last_dest.set(Some(dest_done));
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

    let extracting = *mode.read() == ArchiveMode::Extract;
    let busy = *is_working.read();

    rsx! {
        div { class: "content-body",
            // UI-09: diálogo de progreso; Escape no cancela por accidente (para eso está «Cancelar»).
            if busy {
                Modal {
                    title: if extracting { txt.extracting.to_string() } else { txt.compressing.to_string() },
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

            div { class: "arc-segmented", role: "group", "aria-label": "{txt.operation}",
                button {
                    class: "btn",
                    "aria-pressed": if extracting { "true" } else { "false" },
                    disabled: busy,
                    onclick: move |_| mode.set(ArchiveMode::Extract),
                    span { class: "fluent-icon", "aria-hidden": "true", "\u{E896}" }
                    "{txt.extract}"
                }
                button {
                    class: "btn",
                    "aria-pressed": if extracting { "false" } else { "true" },
                    disabled: busy,
                    onclick: move |_| mode.set(ArchiveMode::Compress),
                    span { class: "fluent-icon", "aria-hidden": "true", "\u{E7B8}" }
                    "{txt.compress}"
                }
            }

            div { class: "card",
                div { class: "arc-field",
                    label { class: "fw-semibold", r#for: "arc-src",
                        if extracting { "{txt.archive_path}" } else { "{txt.source_path}" }
                    }
                    input {
                        id: "arc-src",
                        r#type: "text",
                        class: "input",
                        placeholder: if extracting { "D:\\Backups\\files.zip" } else { "D:\\Projects\\Photos" },
                        "aria-describedby": "arc-src-hint",
                        value: "{src_path.read()}",
                        disabled: busy,
                        oninput: move |evt| src_path.set(evt.value()),
                    }
                    span { id: "arc-src-hint", class: "text-xs text-muted",
                        if extracting { "{txt.archive_path_hint}" } else { "{txt.source_path_hint}" }
                    }
                }

                div { class: "arc-field",
                    label { class: "fw-semibold", r#for: "arc-dest",
                        if extracting { "{txt.extract_destination}" } else { "{txt.output_archive}" }
                    }
                    input {
                        id: "arc-dest",
                        r#type: "text",
                        class: "input",
                        placeholder: if extracting { "D:\\Backups\\files" } else { "D:\\Backups\\Photos.zip" },
                        "aria-describedby": "arc-dest-hint",
                        value: "{dest_path.read()}",
                        disabled: busy,
                        oninput: move |evt| dest_path.set(evt.value()),
                    }
                    span { id: "arc-dest-hint", class: "text-xs text-muted",
                        if extracting { "{txt.extract_destination_hint}" } else { "{txt.output_archive_hint}" }
                    }
                }

                if !extracting {
                    div { class: "arc-field",
                        span { class: "fw-semibold", id: "arc-format-label", "{txt.format}" }
                        div { class: "arc-segmented", role: "group", "aria-labelledby": "arc-format-label",
                            button {
                                class: "btn",
                                "aria-pressed": if *selected_format.read() == ArchiveFormat::Zip { "true" } else { "false" },
                                disabled: busy,
                                onclick: move |_| selected_format.set(ArchiveFormat::Zip),
                                "ZIP"
                            }
                            button {
                                class: "btn",
                                "aria-pressed": if *selected_format.read() == ArchiveFormat::SevenZip { "true" } else { "false" },
                                disabled: busy,
                                onclick: move |_| selected_format.set(ArchiveFormat::SevenZip),
                                "7Z"
                            }
                            button {
                                class: "btn",
                                "aria-pressed": if *selected_format.read() == ArchiveFormat::TarGz { "true" } else { "false" },
                                disabled: busy,
                                onclick: move |_| selected_format.set(ArchiveFormat::TarGz),
                                "TAR.GZ"
                            }
                        }
                    }
                }

                div { class: "row-end",
                    button {
                        class: "btn btn-primary",
                        disabled: busy || src_path.read().trim().is_empty() || dest_path.read().trim().is_empty(),
                        onclick: on_execute,
                        if extracting { "{txt.extract_now}" } else { "{txt.compress_now}" }
                    }
                }
            }

            if let Some((msg, tone)) = status_msg.read().clone() {
                if let Some(done) = last_dest.read().clone() {
                    InfoBar {
                        tone: Tone::Success,
                        title: strings(en).completed.to_string(),
                        message: msg.text(en),
                        action: rsx! {
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| open_folder_in_explorer(&done),
                                span { class: "fluent-icon", "aria-hidden": "true", "\u{E8B7}" }
                                "{strings(en).open_folder}"
                            }
                        },
                    }
                } else {
                    StatusBanner { text: msg.text(en), tone }
                }
            }

            p { class: "text-xs text-muted", "{txt.supported_formats}" }
        }
    }
}
