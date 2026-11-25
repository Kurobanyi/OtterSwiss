#![windows_subsystem = "windows"]
#![cfg(windows)]

mod startup;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder, icon_from_memory};
use otter_swiss::core::cli::CliCommand;
use otter_swiss::core::context_menu::ContextMenuManager;
use otter_swiss::core::rollback::revert_all_changes;
use otter_swiss::ui::App;
use otter_swiss::ui::document::document_head;
use otter_swiss::ui::i18n::Lang;
use otter_swiss::ui::theme::Theme;
use otter_swiss::ui::views::mini_archive::{MiniArchiveJob, MiniArchiveView};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let job = match CliCommand::parse(&args) {
        CliCommand::Rollback => {
            let code = match revert_all_changes() {
                Ok(report) if report.is_complete() => 0,
                Ok(_) => 2,
                Err(_) => 1,
            };
            std::process::exit(code);
        }
        CliCommand::InstallContextMenu => {
            let _ = ContextMenuManager::install();
            return;
        }
        CliCommand::UninstallContextMenu => {
            let _ = ContextMenuManager::uninstall();
            return;
        }
        CliCommand::Extract(src) => Some(MiniArchiveJob::extract(src)),
        CliCommand::CompressZip(src) => Some(MiniArchiveJob::compress_zip(src)),
        CliCommand::Gui => None,
    };

    let lang = Lang::load_persisted();
    let theme = Theme::load_persisted();
    let en = lang.is_en();
    startup::install_panic_dialog(en);
    if !startup::ensure_webview2(en) {
        std::process::exit(1);
    }

    let window_icon = icon_from_memory(include_bytes!("../assets/brand/logo.png")).ok();
    let config = |window: WindowBuilder| {
        Config::new()
            .with_menu(None)
            .with_disable_context_menu(true)
            .with_background_color(theme.background_rgba())
            .with_custom_head(document_head())
            .with_window(
                window
                    .with_decorations(false)
                    .with_window_icon(window_icon.clone()),
            )
    };

    if let Some(job) = job {
        let window = WindowBuilder::new()
            .with_title(if en {
                "OtterSwiss — Archiver"
            } else {
                "OtterSwiss — Archivador"
            })
            .with_inner_size(LogicalSize::new(460.0, 235.0))
            .with_resizable(false);
        dioxus::LaunchBuilder::desktop()
            .with_cfg(config(window))
            .with_context(job)
            .launch(MiniArchiveView);
        return;
    }

    let window = WindowBuilder::new()
        .with_title(if en {
            "OtterSwiss — Native Win32 Suite"
        } else {
            "OtterSwiss — Suite Nativa Win32"
        })
        .with_inner_size(LogicalSize::new(1180.0, 760.0))
        .with_min_inner_size(LogicalSize::new(960.0, 600.0))
        .with_resizable(true);
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config(window))
        .launch(App);
}
