use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::archiver::{ArchiveFormat, ArchiveManager, ArchiveProgress};
use crate::ui::components::Card;
use crate::ui::i18n::Lang;

#[component]
pub fn ArchiverView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut mode = use_signal(|| "extract");
    let mut src_path = use_signal(String::new);
    let mut dest_path = use_signal(String::new);
    let mut selected_format = use_signal(|| ArchiveFormat::Zip);
    let mut is_working = use_signal(|| false);
    let mut progress_state = use_signal(ArchiveProgress::default);
    let mut status_msg = use_signal(|| Option::<String>::None);
    let mut is_error = use_signal(|| false);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));

    let cancel_for_exec = Arc::clone(&cancel_flag);
    let on_execute = move |_| {
        let current_mode = *mode.read();
        let src = PathBuf::from(src_path.read().trim().trim_matches('"'));
        let dest = PathBuf::from(dest_path.read().trim().trim_matches('"'));
        let fmt = *selected_format.read();

        if !src.exists() {
            status_msg.set(Some(if en {
                "Source path does not exist.".to_string()
            } else {
                "La ruta de origen no existe.".to_string()
            }));
            is_error.set(true);
            return;
        }

        cancel_for_exec.store(false, Ordering::Release);
        is_working.set(true);
        is_error.set(false);
        status_msg.set(None);
        progress_state.set(ArchiveProgress::default());

        let prog_arc = Arc::new(Mutex::new(ArchiveProgress::default()));
        let prog_worker = Arc::clone(&prog_arc);
        let cancel_worker = Arc::clone(&cancel_for_exec);

        spawn(async move {
            let handle = tokio::task::spawn_blocking(move || {
                if current_mode == "extract" {
                    ArchiveManager::extract_archive_with_progress(
                        &src,
                        &dest,
                        50 * 1024 * 1024 * 1024,
                        &cancel_worker,
                        Some(&prog_worker),
                    )
                } else {
                    ArchiveManager::compress_path_with_progress(
                        &src,
                        &dest,
                        fmt,
                        &cancel_worker,
                        Some(&prog_worker),
                    )
                }
                .map_err(|e| e.to_string())
            });

            while !handle.is_finished() {
                if let Ok(snap) = prog_arc.lock() {
                    progress_state.set(snap.clone());
                }
                tokio::time::sleep(Duration::from_millis(120)).await;
            }

            let res = handle
                .await
                .unwrap_or_else(|e| Err(format!("Thread error: {}", e)));

            match res {
                Ok(bytes) => {
                    let mb = bytes as f64 / (1024.0 * 1024.0);
                    status_msg.set(Some(if en {
                        format!("Done ({:.2} MB processed).", mb)
                    } else {
                        format!("Completado ({:.2} MB procesados).", mb)
                    }));
                    is_error.set(false);
                }
                Err(err) => {
                    status_msg.set(Some(format!("Error: {}", err)));
                    is_error.set(true);
                }
            }
            is_working.set(false);
        });
    };

    let cancel_for_btn = Arc::clone(&cancel_flag);
    let prog = progress_state.read();
    let mb_done = prog.bytes_processed as f64 / (1024.0 * 1024.0);
    let mb_total = prog.total_bytes as f64 / (1024.0 * 1024.0);
    let eta_txt = prog
        .eta_seconds
        .map(|s| format!("~{}s", s))
        .unwrap_or_else(|| "—".to_string());

    let card_title = if en { "Secure Archiver (ZIP / 7Z / TAR.GZ)" } else { "Archivador Seguro (ZIP / 7Z / TAR.GZ)" }.to_string();
    let card_sub = Some((if en { "100% open-source Rust compression & extraction" } else { "Compresión y extracción libre 100% Rust" }).to_string());

    rsx! {
        div { class: "content-body",
            if *is_working.read() {
                div {
                    style: "position: fixed; inset: 0; background: rgba(0,0,0,0.72); display: flex; align-items: center; justify-content: center; z-index: 9999; padding: 20px;",
                    div {
                        class: "card",
                        style: "max-width: 460px; width: 100%; border: 1px solid var(--accent);",
                        div { class: "card-header",
                            div {
                                h3 { class: "card-title",
                                    if *mode.read() == "extract" {
                                        if en { "Extracting archive..." } else { "Extrayendo archivo..." }
                                    } else if en {
                                        "Compressing..."
                                    } else {
                                        "Comprimiendo..."
                                    }
                                }
                                p {
                                    class: "card-subtitle",
                                    style: "max-width: 400px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    if prog.current_file.is_empty() { "..." } else { "{prog.current_file}" }
                                }
                            }
                        }
                        div {
                            style: "width: 100%; height: 10px; background: var(--bg); border-radius: 999px; overflow: hidden; border: 1px solid var(--border);",
                            div {
                                style: "height: 100%; width: {prog.percent}%; background: var(--accent); transition: width 0.15s ease;",
                            }
                        }
                        div { style: "display: flex; justify-content: space-between; font-size: 11px; color: var(--text-muted);",
                            span { "{prog.percent}% • {prog.files_done}/{prog.total_files} • {mb_done:.1}/{mb_total:.1} MB" }
                            span { "{prog.speed_mbps:.1} MB/s • ETA {eta_txt}" }
                        }
                        div { style: "display: flex; justify-content: flex-end;",
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| cancel_for_btn.store(true, Ordering::Release),
                                if en { "Cancel" } else { "Cancelar" }
                            }
                        }
                    }
                }
            }

            Card {
                title: card_title,
                subtitle: card_sub,
                div { style: "display: flex; flex-direction: column; gap: var(--space-3);",
                    div { style: "display: flex; gap: var(--space-2);",
                        button {
                            class: if *mode.read() == "extract" { "btn btn-primary" } else { "btn btn-secondary" },
                            disabled: *is_working.read(),
                            onclick: move |_| mode.set("extract"),
                            if en { "Extract" } else { "Descomprimir" }
                        }
                        button {
                            class: if *mode.read() == "compress" { "btn btn-primary" } else { "btn btn-secondary" },
                            disabled: *is_working.read(),
                            onclick: move |_| mode.set("compress"),
                            if en { "Compress" } else { "Comprimir" }
                        }
                    }

                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        label { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                            if *mode.read() == "extract" {
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
                            style: "padding: 8px 12px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg); color: var(--text);",
                            oninput: move |evt| src_path.set(evt.value()),
                        }

                        label { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                            if *mode.read() == "extract" {
                                if en { "Output folder:" } else { "Carpeta destino:" }
                            } else if en {
                                "Output archive file:"
                            } else {
                                "Archivo destino:"
                            }
                        }
                        input {
                            r#type: "text",
                            placeholder: if *mode.read() == "extract" { "C:\\Extracted" } else { "C:\\Backup\\Archive.zip" },
                            value: "{dest_path.read()}",
                            disabled: *is_working.read(),
                            style: "padding: 8px 12px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg); color: var(--text);",
                            oninput: move |evt| dest_path.set(evt.value()),
                        }

                        if *mode.read() == "compress" {
                            div { style: "display: flex; align-items: center; gap: var(--space-2); margin-top: 4px;",
                                span { style: "font-size: var(--font-size-xs); color: var(--text-muted);", if en { "Format:" } else { "Formato:" } }
                                button {
                                    class: if *selected_format.read() == ArchiveFormat::Zip { "btn btn-primary" } else { "btn btn-secondary" },
                                    disabled: *is_working.read(),
                                    onclick: move |_| selected_format.set(ArchiveFormat::Zip),
                                    ".ZIP"
                                }
                                button {
                                    class: if *selected_format.read() == ArchiveFormat::SevenZip { "btn btn-primary" } else { "btn btn-secondary" },
                                    disabled: *is_working.read(),
                                    onclick: move |_| selected_format.set(ArchiveFormat::SevenZip),
                                    ".7Z"
                                }
                                button {
                                    class: if *selected_format.read() == ArchiveFormat::TarGz { "btn btn-primary" } else { "btn btn-secondary" },
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
                                if *mode.read() == "extract" {
                                    if en { "Extract Now" } else { "Descomprimir" }
                                } else if en {
                                    "Compress Now"
                                } else {
                                    "Comprimir"
                                }
                            }
                        }
                    }

                    if let Some(msg) = status_msg.read().as_ref() {
                        div {
                            class: if *is_error.read() { "badge badge-danger" } else { "badge badge-success" },
                            style: "padding: var(--space-2) var(--space-3);",
                            "{msg}"
                        }
                    }
                }
            }
        }
    }
}