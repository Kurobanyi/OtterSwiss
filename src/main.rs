// src/main.rs

#![windows_subsystem = "windows"]
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use dioxus::desktop::{icon_from_memory, use_window, Config, WindowBuilder};
use dioxus::prelude::*;
use otter_swiss::core::archiver::{
    resolve_extract_destination, ArchiveFormat, ArchiveManager, ArchiveProgress,
};
use otter_swiss::core::context_menu::ContextMenuManager;
use otter_swiss::ui::i18n::Lang;
use otter_swiss::ui::theme::Theme;
use otter_swiss::ui::App;
use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;

#[derive(Clone)]
struct MiniArchiveJob {
    is_extract: bool,
    src: PathBuf,
    dest: PathBuf,
}

static MINI_JOB: OnceLock<MiniArchiveJob> = OnceLock::new();

/// UI-09: con un `[role=dialog]` abierto, Tab y Mayús+Tab recorren solo sus controles.
const FOCUS_TRAP_SCRIPT: &str = r#"<script>
document.addEventListener('keydown', function (e) {
  if (e.key !== 'Tab') return;
  var dialogs = document.querySelectorAll('[role=dialog]');
  if (!dialogs.length) return;
  var dialog = dialogs[dialogs.length - 1];
  var items = Array.prototype.filter.call(
    dialog.querySelectorAll('button, input, select, textarea, a[href], [tabindex]:not([tabindex="-1"])'),
    function (el) { return !el.disabled && el.offsetParent !== null; });
  if (!items.length) { e.preventDefault(); dialog.focus(); return; }
  var first = items[0], last = items[items.length - 1];
  if (!dialog.contains(document.activeElement)) { e.preventDefault(); first.focus(); }
  else if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
});
</script>"#;

fn clean_archive_extract_dir(src: &Path) -> PathBuf {
    let parent = src.parent().unwrap_or(src);
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "extracted".into());
    let low = name.to_ascii_lowercase();
    let folder = if low.ends_with(".tar.gz") {
        &name[..name.len() - 7]
    } else {
        src.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted")
    };
    let clean = if folder.is_empty() {
        "extracted"
    } else {
        folder
    };
    parent.join(clean)
}

fn open_folder_in_explorer(target: &Path) {
    let folder = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    };
    let mut buf = [0u16; 260];
    let len = unsafe { GetWindowsDirectoryW(Some(&mut buf)) } as usize;
    let explorer = if len > 0 && len < buf.len() {
        PathBuf::from(String::from_utf16_lossy(&buf[..len])).join("explorer.exe")
    } else {
        PathBuf::from(r"C:\Windows\explorer.exe")
    };
    let _ = Command::new(explorer).arg(folder).spawn();
}

#[component]
fn MiniArchiveProgressApp() -> Element {
    let window = use_window();
    let theme = use_hook(Theme::load_persisted);
    let lang = use_hook(Lang::load_persisted);
    let en = lang.is_en();

    let mut progress = use_signal(ArchiveProgress::default);
    let mut is_running = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));
    let cancel_for_hook = Arc::clone(&cancel_flag);

    let job_opt = MINI_JOB.get().cloned();

    use_hook(move || {
        let Some(job) = job_opt else {
            is_running.set(false);
            error_msg.set(Some(
                if en {
                    "Invalid job"
                } else {
                    "Trabajo inválido"
                }
                .into(),
            ));
            return;
        };

        let prog_arc = Arc::new(Mutex::new(ArchiveProgress::default()));
        let prog_worker = Arc::clone(&prog_arc);
        let cancel_worker = Arc::clone(&cancel_for_hook);

        spawn(async move {
            let handle = tokio::task::spawn_blocking(move || {
                if job.is_extract {
                    ArchiveManager::extract_archive_with_progress(
                        &job.src,
                        &job.dest,
                        50 * 1024 * 1024 * 1024,
                        &cancel_worker,
                        Some(&prog_worker),
                    )
                } else {
                    ArchiveManager::compress_path_with_progress(
                        &job.src,
                        &job.dest,
                        ArchiveFormat::Zip,
                        &cancel_worker,
                        Some(&prog_worker),
                    )
                }
                .map_err(|e| e.to_string())
            });

            while !handle.is_finished() {
                if let Ok(snap) = prog_arc.lock() {
                    progress.set(snap.clone());
                }
                tokio::time::sleep(Duration::from_millis(120)).await;
            }

            let res = handle.await.unwrap_or_else(|e| Err(e.to_string()));

            if let Ok(snap) = prog_arc.lock() {
                progress.set(snap.clone());
            }
            if let Err(err) = res {
                error_msg.set(Some(err));
            }
            is_running.set(false);
        });
    });

    let prog = progress.read();
    let running = *is_running.read();
    let err_opt = error_msg.read().clone();
    let win_drag = window.clone();
    let win_close = window.clone();
    let win_done = window;
    let cancel_btn = Arc::clone(&cancel_flag);
    let cancel_close = Arc::clone(&cancel_flag);

    let dest_for_open = MINI_JOB.get().map(|j| j.dest.clone());
    let title_txt = match (MINI_JOB.get().map(|j| j.is_extract).unwrap_or(true), en) {
        (true, false) => "Extrayendo archivo...",
        (true, true) => "Extracting archive...",
        (false, false) => "Comprimiendo a ZIP...",
        (false, true) => "Compressing to ZIP...",
    };

    let top_status_txt = if running {
        title_txt
    } else if err_opt.is_some() {
        "Error"
    } else if en {
        "Completed"
    } else {
        "Completado"
    };

    let current_file_lbl = if prog.current_file.is_empty() {
        if en {
            "Preparing files...".to_string()
        } else {
            "Preparando archivos...".to_string()
        }
    } else {
        prog.current_file.clone()
    };

    let cancel_lbl = if en { "Cancel" } else { "Cancelar" };
    let open_folder_lbl = if en { "Open folder" } else { "Abrir carpeta" };
    let close_lbl = if en { "Close" } else { "Cerrar" };

    let mb_done = prog.bytes_processed as f64 / (1024.0 * 1024.0);
    let mb_total = prog.total_bytes as f64 / (1024.0 * 1024.0);
    let eta_txt = prog
        .eta_seconds
        .map(|s| format!("~{}s", s))
        .unwrap_or_else(|| "—".into());

    rsx! {
        style { {include_str!("../assets/main.css")} }

        div {
            class: "app-root-frameless",
            "data-theme": "{theme.as_str()}",
            style: "border: 1px solid var(--border);",

            header {
                class: "top-titlebar",
                style: "height: 40px;",
                div {
                    class: "top-brand",
                    onmousedown: move |_| { win_drag.drag(); },
                    span { class: "top-title", "OtterSwiss" }
                    span { class: "top-version", "{top_status_txt}" }
                }
                button {
                    class: "win-btn win-close-btn",
                    onclick: move |_| {
                        cancel_close.store(true, Ordering::Release);
                        win_close.close();
                    },
                    "✕"
                }
            }

            div {
                style: "flex: 1; padding: var(--space-4) var(--space-4); display: flex; flex-direction: column; justify-content: space-between; gap: var(--space-3);",
                if let Some(err) = err_opt {
                    div { class: "badge badge-danger", style: "padding: var(--space-2); white-space: normal;", "{err}" }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        div {
                            style: "font-size: var(--font-size-xs); font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                            "{current_file_lbl}"
                        }
                        div {
                            style: "width: 100%; height: var(--space-2); background: var(--surface); border-radius: 999px; overflow: hidden; border: 1px solid var(--border);",
                            div {
                                style: "height: 100%; width: {prog.percent}%; background: var(--accent); transition: width 0.15s ease;",
                            }
                        }
                        div { style: "display: flex; justify-content: space-between; font-size: var(--font-size-xs); color: var(--text-muted);",
                            span { "{prog.percent}% • {prog.files_done}/{prog.total_files} • {mb_done:.1}/{mb_total:.1} MB" }
                            span { "{prog.speed_mbps:.1} MB/s • ETA {eta_txt}" }
                        }
                    }
                }

                div { style: "display: flex; justify-content: flex-end; gap: var(--space-2);",
                    if running {
                        button {
                            class: "btn btn-danger",
                            style: "padding: var(--space-2) var(--space-4); font-size: var(--font-size-xs);",
                            onclick: move |_| cancel_btn.store(true, Ordering::Release),
                            "{cancel_lbl}"
                        }
                    } else {
                        if let Some(dest) = dest_for_open {
                            button {
                                class: "btn btn-secondary",
                                style: "padding: var(--space-2) var(--space-3); font-size: var(--font-size-xs);",
                                onclick: move |_| open_folder_in_explorer(&dest),
                                "{open_folder_lbl}"
                            }
                        }
                        button {
                            class: "btn btn-primary",
                            style: "padding: var(--space-2) var(--space-4); font-size: var(--font-size-xs);",
                            onclick: move |_| win_done.close(),
                            "{close_lbl}"
                        }
                    }
                }
            }
        }
    }
}

fn prepare_mini_archive_job(args: &[String]) -> Option<MiniArchiveJob> {
    if let Some(pos) = args.iter().position(|a| a == "--extract") {
        if let Some(target) = args.get(pos + 1) {
            let src = PathBuf::from(target.trim().trim_matches('"'));
            // Nunca se extrae sobre una carpeta existente: se usa la hermana libre «nombre (2)».
            let dest = resolve_extract_destination(&clean_archive_extract_dir(&src));
            return Some(MiniArchiveJob {
                is_extract: true,
                src,
                dest,
            });
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--compress-zip") {
        if let Some(target) = args.get(pos + 1) {
            let src = PathBuf::from(target.trim().trim_matches('"'));
            // UI-15: `mi.proyecto\` debe dar `mi.proyecto.zip`, no `mi.zip`.
            let name = src.file_name().unwrap_or_default().to_string_lossy();
            let dest = src.parent().unwrap_or(&src).join(format!("{}.zip", name));
            return Some(MiniArchiveJob {
                is_extract: false,
                src,
                dest,
            });
        }
    }
    None
}

/// Muestra un cuadro de mensaje nativo (no depende de WebView2). Devuelve `true` si se eligió «Sí».
fn native_message(title: &str, text: &str, ask_yes_no: bool, is_error: bool) -> bool {
    use otter_swiss::core::win_common::to_wide_null;
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_YESNO,
    };
    let title_w = to_wide_null(title);
    let text_w = to_wide_null(text);
    let mut style = if ask_yes_no { MB_YESNO } else { MB_OK };
    style |= if is_error {
        MB_ICONERROR
    } else {
        MB_ICONWARNING
    };
    let res = unsafe {
        MessageBoxW(
            None,
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            style,
        )
    };
    res == IDYES
}

/// ERR-13: la interfaz se dibuja con Microsoft Edge WebView2. Sin él, dioxus-desktop termina con un `panic`
/// antes de abrir la ventana, así que se comprueba aquí y se ofrece instalarlo con el instalador verificado.
/// Devuelve `false` si no se puede continuar.
fn ensure_webview2(en: bool) -> bool {
    use otter_swiss::core::dependencies::DependencyManager;

    if DependencyManager::is_webview2_installed() {
        return true;
    }
    let (title, ask) = if en {
        (
            "OtterSwiss — WebView2 required",
            "OtterSwiss needs the Microsoft Edge WebView2 Runtime to show its window, and it is not installed.\n\n\
             Download and install it now from Microsoft? (about 2 MB, verified before running)",
        )
    } else {
        (
            "OtterSwiss — Falta WebView2",
            "OtterSwiss necesita Microsoft Edge WebView2 Runtime para mostrar su ventana y no está instalado.\n\n\
             ¿Descargarlo e instalarlo ahora desde Microsoft? (unos 2 MB, verificado antes de ejecutarse)",
        )
    };
    if !native_message(title, ask, true, false) {
        return false;
    }
    match DependencyManager::install_runtime("webview2") {
        Ok(()) if DependencyManager::is_webview2_installed() => true,
        Ok(()) => {
            native_message(
                title,
                if en {
                    "WebView2 was installed but is not detected yet. Restart OtterSwiss."
                } else {
                    "WebView2 se instaló pero todavía no se detecta. Vuelve a abrir OtterSwiss."
                },
                false,
                false,
            );
            false
        }
        Err(e) => {
            native_message(
                title,
                &if en {
                    format!(
                        "WebView2 could not be installed: {e}\n\nInstall it from \
                         https://developer.microsoft.com/microsoft-edge/webview2/ and open OtterSwiss again."
                    )
                } else {
                    format!(
                        "No se pudo instalar WebView2: {e}\n\nInstálalo desde \
                         https://developer.microsoft.com/microsoft-edge/webview2/ y vuelve a abrir OtterSwiss."
                    )
                },
                false,
                true,
            );
            false
        }
    }
}

/// Un `panic` (p. ej. dentro de dioxus-desktop al crear la ventana) se muestra en un cuadro nativo en vez de
/// cerrar la app sin explicación: el binario es `windows_subsystem = "windows"` y no tiene consola.
fn install_panic_dialog() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "error desconocido".into());
        let place = info
            .location()
            .map(|l| format!(" ({}:{})", l.file(), l.line()))
            .unwrap_or_default();
        native_message(
            "OtterSwiss — Error inesperado",
            &format!("OtterSwiss se cerró por un error inesperado:\n\n{msg}{place}"),
            false,
            true,
        );
        default_hook(info);
    }));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--rollback") {
        let code = match otter_swiss::core::uninstaller::UninstallerManager::revert_all_changes() {
            Ok(report) if report.is_complete() => 0,
            Ok(_) => 2,
            Err(_) => 1,
        };
        std::process::exit(code);
    }

    if args.iter().any(|a| a == "--install-context-menu") {
        let _ = ContextMenuManager::install();
        return;
    }

    if args.iter().any(|a| a == "--uninstall-context-menu") {
        let _ = ContextMenuManager::uninstall();
        return;
    }

    install_panic_dialog();
    if !ensure_webview2(Lang::load_persisted().is_en()) {
        std::process::exit(1);
    }

    let window_icon = icon_from_memory(include_bytes!("../assets/logo.png")).ok();

    if let Some(job) = prepare_mini_archive_job(&args) {
        let _ = MINI_JOB.set(job);
        let mini_win = WindowBuilder::new()
            .with_title("OtterSwiss — Archivador")
            .with_inner_size(dioxus::desktop::LogicalSize::new(460.0, 235.0))
            .with_decorations(false)
            .with_resizable(false)
            .with_window_icon(window_icon);
        let cfg = Config::new()
            .with_menu(None)
            .with_disable_context_menu(true)
            .with_window(mini_win);
        dioxus::LaunchBuilder::desktop()
            .with_cfg(cfg)
            .launch(MiniArchiveProgressApp);
        return;
    }

    let window = WindowBuilder::new()
        .with_title("OtterSwiss — Suite Nativa Win32")
        .with_inner_size(dioxus::desktop::LogicalSize::new(1180.0, 760.0))
        .with_min_inner_size(dioxus::desktop::LogicalSize::new(960.0, 600.0))
        .with_decorations(false)
        .with_resizable(true)
        .with_window_icon(window_icon);

    let config = Config::new()
        .with_menu(None)
        .with_disable_context_menu(true)
        .with_background_color(Theme::load_persisted().background_rgba())
        .with_custom_head(FOCUS_TRAP_SCRIPT.to_string())
        .with_window(window);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}
