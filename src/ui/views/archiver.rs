use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dioxus::prelude::*;

use crate::core::archive::{
    resolve_extract_destination, ArchiveFormat, ArchiveManager, ArchiveMode, ArchiveProgress,
    DEFAULT_EXTRACT_QUOTA,
};
use crate::ui::components::{Card, Modal, ProgressBar, StatusBanner, Tone};
use crate::ui::format::{bytes_mb, eta, mb_1};
use crate::ui::i18n::Lang;
use crate::ui::task::run_with_progress;

#[component]
pub fn ArchiverView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut mode = use_signal(|| ArchiveMode::Extract);
    let mut src_path = use_signal(String::new);
    let mut dest_path = use_signal(String::new);
    let mut selected_format = use_signal(|| ArchiveFormat::Zip);
    let mut is_working = use_signal(|| false);
    let mut progress_state = use_signal(ArchiveProgress::default);
    let mut status_msg = use_signal(|| Option::<(String, Tone)>::None);
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
                if en {
                    "Source path does not exist."
                } else {
                    "La ruta de origen no existe."
                }
                .to_string(),
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
                    ArchiveManager::extract_archive_with_progress(
                        &src,
                        &dest,
                        DEFAULT_EXTRACT_QUOTA,
                        &cancel_worker,
                        Some(prog),
                    )
                } else {
                    ArchiveManager::compress_path_with_progress(
                        &src,
                        &dest,
                        fmt,
                        &cancel_worker,
                        Some(prog),
                    )
                }
                .map_err(|e| e.to_string())
            })
            .await
            .and_then(|r| r);

            match res {
                Ok(bytes) => {
                    let text = if en {
                        format!("Done ({} processed) → {}", bytes_mb(bytes), dest_display)
                    } else {
                        format!(
                            "Completado ({} procesados) → {}",
                            bytes_mb(bytes),
                            dest_display
                        )
                    };
                    status_msg.set(Some((text, Tone::Success)));
                }
                Err(err) => {
                    status_msg.set(Some((format!("Error: {}", err), Tone::Danger)));
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

    let card_title = if en {
        "Secure Archiver (ZIP / 7Z / TAR.GZ)"
    } else {
        "Archivador Seguro (ZIP / 7Z / TAR.GZ)"
    }
    .to_string();
    let card_sub = Some(
        (if en {
            "100% open-source Rust compression & extraction"
        } else {
            "Compresión y extracción libre 100% Rust"
        })
        .to_string(),
    );

    rsx! {
        div { class: "content-body",
            // UI-09: diálogo de progreso; Escape no cancela por accidente (para eso está «Cancelar»).
            if *is_working.read() {
                Modal {
                    title: match (*mode.read() == ArchiveMode::Extract, en) {
                        (true, true) => "Extracting archive...".to_string(),
                        (true, false) => "Extrayendo archivo...".to_string(),
                        (false, true) => "Compressing...".to_string(),
                        (false, false) => "Comprimiendo...".to_string(),
                    },
                    subtitle: Some(if prog.current_file.is_empty() { "...".to_string() } else { prog.current_file.clone() }),
                    on_close: move |_| {},
                        ProgressBar { percent: prog.percent }
                        div { style: "display: flex; justify-content: space-between; font-size: var(--font-size-xs); color: var(--text-muted);",
                            span { "{prog.percent}% • {prog.files_done}/{prog.total_files} • {mb_done}/{mb_total} MB" }
                            span { "{prog.speed_mbps:.1} MB/s • ETA {eta_txt}" }
                        }
                        div { class: "modal-actions",
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| cancel_for_btn.store(true, Ordering::Release),
                                if en { "Cancel" } else { "Cancelar" }
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
                            if en { "Extract" } else { "Descomprimir" }
                        }
                        button {
                            class: "btn btn-secondary",
                            "aria-pressed": if *mode.read() == ArchiveMode::Compress { "true" } else { "false" },
                            disabled: *is_working.read(),
                            onclick: move |_| mode.set(ArchiveMode::Compress),
                            if en { "Compress" } else { "Comprimir" }
                        }
                    }

                    div { class: "stack-2",
                        label { class: "text-xs text-muted",
                            if *mode.read() == ArchiveMode::Extract {
                                if en { "Archive path (.zip, .7z, .tar, .tar.gz):" } else { "Archivo comprimido (.zip, .7z, .tar, .tar.gz):" }
                            } else if en {
                                "Source file or folder:"
                            } else {
                                "Archivo o carpeta origen:"
                            }
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
                                if en { "Output folder:" } else { "Carpeta destino:" }
                            } else if en {
                                "Output archive file:"
                            } else {
                                "Archivo destino:"
                            }
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
                            div { style: "display: flex; align-items: center; gap: var(--space-2); margin-top: var(--space-1);",
                                span { class: "text-xs text-muted", if en { "Format:" } else { "Formato:" } }
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

                        div { style: "display: flex; gap: var(--space-2); margin-top: var(--space-2);",
                            button {
                                class: "btn btn-primary",
                                disabled: *is_working.read() || src_path.read().trim().is_empty() || dest_path.read().trim().is_empty(),
                                onclick: on_execute,
                                if *mode.read() == ArchiveMode::Extract {
                                    if en { "Extract Now" } else { "Descomprimir" }
                                } else if en {
                                    "Compress Now"
                                } else {
                                    "Comprimir"
                                }
                            }
                        }
                    }

                    if let Some((msg, tone)) = status_msg.read().clone() {
                        StatusBanner { text: msg, tone }
                    }
                }
            }
        }
    }
}
