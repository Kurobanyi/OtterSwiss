//! Ventana principal: barra de título (marca, idioma, tema, botones de ventana), panel de navegación lateral al estilo
//! de Windows 11, encabezado de cada página, pestañas y actualizador.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::theme::Theme;
use crate::ui::views::archiver::ArchiverView;
use crate::ui::views::cleaner::CleanerView;
use crate::ui::views::dashboard::DashboardView;
use crate::ui::views::debloater::DebloaterView;
use crate::ui::views::dns::DnsView;
use crate::ui::views::drivers::DriversView;
use crate::ui::views::hardware::HardwareView;
use crate::ui::views::sensors::SensorsView;
use crate::ui::views::tools::ToolsView;
use crate::ui::views::uninstaller::UninstallerView;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use dioxus::desktop::use_window;
use dioxus::prelude::*;

use crate::core::cli;
use crate::core::error::CoreError;
use crate::core::hardware::specs::SystemSpecs;
use crate::core::platform::elevation;
use crate::core::updater::{
    UpdateReleaseInfo, check_for_updates, download_and_install_update, launch_installer_and_exit,
};
use crate::ui::components::{InfoBar, Modal, ProgressBar, StatusBanner, Tone};
use crate::ui::document::sync_document_attrs;
use crate::ui::installer::{
    InstallJobRequest, InstallerDispatcher, ToolsSharedState, refresh_tools_inventory,
    run_installer_worker,
};

const KEEP_ALIVE_TABS: [ActiveTab; 8] = [
    ActiveTab::Dashboard,
    ActiveTab::Cleaner,
    ActiveTab::Debloater,
    ActiveTab::Dns,
    ActiveTab::Drivers,
    ActiveTab::Archiver,
    ActiveTab::Tools,
    ActiveTab::Uninstaller,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActiveTab {
    Dashboard,
    Cleaner,
    Debloater,
    Dns,
    Drivers,
    Archiver,
    Tools,
    Uninstaller,
    Hardware,
    Sensors,
}

impl ActiveTab {
    /// Orden de la barra superior.
    const ALL: [ActiveTab; 10] = [
        Self::Dashboard,
        Self::Cleaner,
        Self::Debloater,
        Self::Dns,
        Self::Drivers,
        Self::Archiver,
        Self::Tools,
        Self::Uninstaller,
        Self::Hardware,
        Self::Sensors,
    ];

    /// Glifo de Segoe Fluent Icons (fuente del sistema en Windows 11; en Windows 10, Segoe MDL2 Assets).
    fn icon(self) -> &'static str {
        match self {
            Self::Dashboard => "\u{E80F}",
            Self::Cleaner => "\u{E75C}",
            Self::Debloater => "\u{E71C}",
            Self::Dns => "\u{E774}",
            Self::Drivers => "\u{E772}",
            Self::Archiver => "\u{E7B8}",
            Self::Tools => "\u{E90F}",
            Self::Uninstaller => "\u{E74D}",
            Self::Hardware => "\u{E7F8}",
            Self::Sensors => "\u{E9CA}",
        }
    }

    fn description(self, en: bool) -> &'static str {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        strings(en).shell.page_descriptions[idx]
    }

    fn label(self, en: bool) -> &'static str {
        match (self, en) {
            (Self::Dashboard, true) => "Home",
            (Self::Dashboard, false) => "Inicio",
            (Self::Cleaner, true) => "Cleaner",
            (Self::Cleaner, false) => "Limpiador",
            (Self::Debloater, _) => "Debloater",
            (Self::Dns, _) => "DNS",
            (Self::Drivers, _) => "Drivers",
            (Self::Archiver, true) => "Archiver",
            (Self::Archiver, false) => "Archivos",
            (Self::Tools, true) => "Tools & Store",
            (Self::Tools, false) => "Herramientas",
            (Self::Uninstaller, true) => "Uninstaller",
            (Self::Uninstaller, false) => "Desinstalador",
            (Self::Hardware, _) => "Hardware",
            (Self::Sensors, true) => "Sensors",
            (Self::Sensors, false) => "Sensores",
        }
    }
}

/// Versión corta para la insignia: `4.2.0` se muestra como `v4.2`.
fn version_badge(pkg_ver: &str) -> String {
    format!("v{}", pkg_ver.strip_suffix(".0").unwrap_or(pkg_ver))
}

/// Barra de título: marca y aviso de actualización, idioma, tema y botones de ventana. La franja libre arrastra la
/// ventana, como en una app nativa.
#[component]
fn TitleBar(
    update_info: Signal<Option<UpdateReleaseInfo>>,
    show_update_modal: Signal<bool>,
) -> Element {
    let mut current_theme = use_context::<Signal<Theme>>();
    let mut current_lang = use_context::<Signal<Lang>>();
    let window = use_window();
    let (win_min, win_max, win_close, win_drag, win_drag_strip) = (
        window.clone(),
        window.clone(),
        window.clone(),
        window.clone(),
        window,
    );
    let en = current_lang.read().is_en();
    let theme_val = *current_theme.read();

    // TE-10: icono de 64 px (unos 5 KB); el GIF animado de 1,7 MB queda solo para el README.
    let logo_data_uri = use_hook(|| {
        format!(
            "data:image/png;base64,{}",
            STANDARD.encode(include_bytes!("../../assets/brand/icon-64.png"))
        )
    });

    rsx! {
        header {
            class: "top-titlebar",

            div {
                class: "top-brand",
                onmousedown: move |_| { win_drag.drag(); },
                img {
                    class: "brand-icon",
                    src: "{logo_data_uri}",
                    alt: "Logo",
                }
                span { class: "top-title", "OtterSwiss" }
                span { class: "top-version", "{version_badge(env!(\"CARGO_PKG_VERSION\"))}" }
            }
            if let Some(upd) = update_info.read().as_ref() {
                button {
                    class: "update-chip",
                    onclick: move |_| show_update_modal.set(true),
                    span { class: "fluent-icon", "aria-hidden": "true", "\u{E896}" }
                    "v{upd.version}"
                }
            }
            div {
                class: "titlebar-drag",
                onmousedown: move |_| { win_drag_strip.drag(); },
            }

            div { class: "top-controls",
                button {
                    class: "theme-toggle-btn",
                    title: strings(en).switch_language,
                    onclick: move |_| {
                        let next = current_lang.read().toggle();
                        current_lang.set(next);
                        next.save_persisted();
                    },
                    "{current_lang.read().code()}"
                }
                // TE-03: selector en vez de recorrer los temas uno por uno.
                select {
                    class: "theme-select",
                    title: strings(en).theme,
                    "aria-label": strings(en).theme,
                    onchange: move |evt| {
                        if let Some(next) = Theme::from_key(&evt.value()) {
                            current_theme.set(next);
                            next.save_persisted();
                        }
                    },
                    for t in Theme::ALL {
                        option { key: "{t.key()}", value: "{t.key()}", selected: t == theme_val, "{t.display_name(en)}" }
                    }
                }
                button {
                    class: "win-btn",
                    title: strings(en).minimize,
                    "aria-label": strings(en).minimize,
                    onclick: move |_| { win_min.set_minimized(true); },
                    "\u{E921}"
                }
                button {
                    class: "win-btn",
                    title: strings(en).maximize,
                    "aria-label": strings(en).maximize,
                    onclick: move |_| { win_max.toggle_maximized(); },
                    "\u{E922}"
                }
                button {
                    class: "win-btn win-close-btn",
                    title: strings(en).close,
                    "aria-label": strings(en).close,
                    onclick: move |_| { win_close.close(); },
                    "\u{E8BB}"
                }
            }
        }
    }
}

/// Panel de navegación lateral (NavigationView de Windows 11). En ventanas estrechas queda solo con iconos y el
/// nombre de cada sección pasa al `title`.
#[component]
fn NavPane(active_tab: Signal<ActiveTab>, tools_state: Signal<ToolsSharedState>) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    rsx! {
        nav { class: "nav-pane", "aria-label": strings(en).shell.navigation,
            for tab in ActiveTab::ALL {
                button {
                    key: "{tab.label(false)}",
                    class: "nav-item",
                    title: tab.label(en),
                    "aria-current": if *active_tab.read() == tab { "page" } else { "false" },
                    onclick: move |_| {
                        active_tab.set(tab);
                        if tab == ActiveTab::Tools {
                            refresh_tools_inventory(tools_state);
                        }
                    },
                    span { class: "nav-icon fluent-icon", "aria-hidden": "true", "{tab.icon()}" }
                    span { class: "nav-label", "{tab.label(en)}" }
                }
            }
        }
    }
}

/// UI-09: aviso de actualización disponible, con notas de la versión y descarga con progreso.
#[component]
fn UpdateModal(
    update_info: Signal<Option<UpdateReleaseInfo>>,
    show_update_modal: Signal<bool>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).shell;
    let mut is_downloading_update = use_signal(|| false);
    let mut update_progress = use_signal(|| 0u8);
    let mut update_error = use_signal(|| None::<Msg>);

    let Some(upd) = update_info.read().clone() else {
        return rsx! {};
    };

    let info = upd.clone();
    let on_start_update = move |_| {
        let info = info.clone();
        is_downloading_update.set(true);
        update_error.set(None);
        update_progress.set(0);

        let cancel_arc = Arc::new(AtomicBool::new(false));
        let prog_arc = Arc::new(AtomicU8::new(0));

        let prog_poll = Arc::clone(&prog_arc);
        spawn(async move {
            while *is_downloading_update.read() {
                update_progress.set(prog_poll.load(Ordering::Acquire));
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
        });

        spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                let installer_path = download_and_install_update(&info, &cancel_arc, &prog_arc)?;
                // Solo vuelve si no pudo lanzar el instalador; si lo lanza, la app se cierra.
                launch_installer_and_exit(&installer_path, &info)
            })
            .await
            .unwrap_or_else(|e| {
                Err(CoreError::Trust {
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })
            });

            if let Err(e) = res {
                update_error.set(Some(Msg::from(e)));
                is_downloading_update.set(false);
            }
        });
    };

    let update_btn_lbl = if *is_downloading_update.read() {
        (txt.downloading)(&*update_progress.read())
    } else {
        txt.download_and_install.to_string()
    };

    rsx! {
        Modal {
            title: (txt.update_available_v)(&upd.version),
            subtitle: Some((txt.current_version_v_new_version)(&env!("CARGO_PKG_VERSION"), &upd.version)),
            wide: true,
            on_close: move |_| {
                if !*is_downloading_update.peek() {
                    show_update_modal.set(false);
                }
            },
            if !upd.notes.is_empty() {
                div { class: "stack-2",
                    span { class: "text-xs text-muted fw-semibold", "{txt.release_notes}" }
                    div {
                        class: "modal-list mono pre-wrap",
                        "{upd.notes}"
                    }
                }
            }

            if *is_downloading_update.read() {
                div { class: "stack-2",
                    ProgressBar { percent: *update_progress.read() }
                    div { class: "row-between text-xs text-muted",
                        span { "{update_progress.read()}%" }
                        span { "{upd.asset_name}" }
                    }
                }
            }

            if let Some(err) = update_error.read().clone() {
                StatusBanner { text: err.text(en), tone: Tone::Danger }
            }

            div { class: "modal-actions",
                button {
                    class: "btn btn-secondary",
                    disabled: *is_downloading_update.read(),
                    onclick: move |_| show_update_modal.set(false),
                    "{strings(en).later}"
                }
                button {
                    class: "btn btn-primary",
                    disabled: *is_downloading_update.read(),
                    onclick: on_start_update,
                    "{update_btn_lbl}"
                }
            }
        }
    }
}

/// SEC-17: la app arranca con los permisos del usuario. Mientras no sea administrador avisa qué necesita elevación y
/// ofrece reabrirla elevada (UAC); al aceptar, esta ventana se cierra para no dejar dos.
#[component]
fn StandardModeBanner() -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).shell;
    let window = use_window();
    rsx! {
        InfoBar {
            tone: Tone::Info,
            title: txt.standard_mode.to_string(),
            message: txt.standard_mode_hint.to_string(),
            action: rsx! {
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        if elevation::relaunch_as_admin(&[cli::ELEVATED]).is_ok() {
                            window.close();
                        }
                    },
                    "{txt.relaunch_as_admin}"
                }
            },
        }
    }
}

/// CON-02: las pestañas con tareas largas se montan al visitarlas y quedan vivas, ocultas. Sensores y Hardware no
/// tienen tareas: Sensores se desmonta para dejar de sondear.
#[component]
fn TabViews(
    active_tab: Signal<ActiveTab>,
    visited_tabs: Signal<HashSet<ActiveTab>>,
    system_specs: Signal<Option<SystemSpecs>>,
    specs_error: Signal<Option<Msg>>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let is_elevated = use_hook(|| elevation::is_elevated().unwrap_or(false));
    rsx! {
        main { class: "main-content-full",
            header { class: "page-header",
                h1 { class: "page-title", "{active_tab.read().label(en)}" }
                p { class: "page-desc", "{active_tab.read().description(en)}" }
            }
            if !is_elevated {
                StandardModeBanner {}
            }
            for tab in KEEP_ALIVE_TABS {
                if visited_tabs.read().contains(&tab) {
                    div {
                        key: "{tab:?}",
                        hidden: *active_tab.read() != tab,
                        match tab {
                            ActiveTab::Dashboard => rsx! { DashboardView {} },
                            ActiveTab::Cleaner => rsx! { CleanerView {} },
                            ActiveTab::Debloater => rsx! { DebloaterView {} },
                            ActiveTab::Dns => rsx! { DnsView {} },
                            ActiveTab::Drivers => rsx! { DriversView {} },
                            ActiveTab::Archiver => rsx! { ArchiverView {} },
                            ActiveTab::Tools => rsx! { ToolsView {} },
                            ActiveTab::Uninstaller => rsx! { UninstallerView {} },
                            ActiveTab::Hardware | ActiveTab::Sensors => rsx! {},
                        }
                    }
                }
            }
            if *active_tab.read() == ActiveTab::Hardware {
                HardwareView { specs: system_specs(), error: specs_error().map(|m| m.text(en)) }
            }
            if *active_tab.read() == ActiveTab::Sensors {
                SensorsView {}
            }
        }
    }
}

#[component]
pub fn App() -> Element {
    let current_theme = use_signal(Theme::load_persisted);
    let current_lang = use_signal(Lang::load_persisted);
    let active_tab = use_signal(|| ActiveTab::Dashboard);
    // CON-02: una pestaña visitada sigue montada (oculta) para no perder una tarea en curso.
    let mut visited_tabs = use_signal(|| HashSet::from([ActiveTab::Dashboard]));
    use_effect(move || {
        let tab = *active_tab.read();
        if !visited_tabs.peek().contains(&tab) {
            visited_tabs.write().insert(tab);
        }
    });
    let tools_state = use_signal(ToolsSharedState::default);
    use_hook(move || refresh_tools_inventory(tools_state));
    let cancel_flag = use_hook(|| Arc::new(AtomicBool::new(false)));

    let mut system_specs = use_signal(|| None::<SystemSpecs>);
    let mut specs_error = use_signal(|| None::<Msg>);
    use_future(move || async move {
        // UI-06: un fallo se muestra en la pestaña en vez de dejarla «Analizando...» para siempre.
        match tokio::task::spawn_blocking(crate::core::hardware::specs::collect_system_specs).await
        {
            Ok(Ok(data)) => system_specs.set(Some(data)),
            Ok(Err(e)) => specs_error.set(Some(Msg::from(e))),
            Err(e) => specs_error.set(Some(Msg::plain(e.to_string()))),
        }
    });

    // --- Auto-Actualizador Nativo GitHub ---
    let mut update_info = use_signal(|| None::<UpdateReleaseInfo>);
    let show_update_modal = use_signal(|| false);
    use_future(move || async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        if let Ok(Ok(Some(info))) = tokio::task::spawn_blocking(check_for_updates).await {
            update_info.set(Some(info));
        }
    });

    let dispatcher = use_hook(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<InstallJobRequest>();
        run_installer_worker(rx, Arc::clone(&cancel_flag), tools_state);
        InstallerDispatcher {
            tx,
            cancel_flag: Arc::clone(&cancel_flag),
        }
    });

    // TE-04 e IN-07: <html> lleva el tema (color-scheme, scrollbars) y el idioma (lectores de pantalla).
    use_effect(move || {
        sync_document_attrs(
            current_theme.read().css_name(),
            current_lang.read().html_code(),
        );
    });

    provide_context(current_theme);
    provide_context(current_lang);
    provide_context(tools_state);
    provide_context(dispatcher);

    rsx! {
        div {
            class: "app-root-frameless",
            "data-theme": "{current_theme.read().css_name()}",
            TitleBar { update_info, show_update_modal }
            if *show_update_modal.read() {
                UpdateModal { update_info, show_update_modal }
            }
            div { class: "app-body",
                NavPane { active_tab, tools_state }
                TabViews { active_tab, visited_tabs, system_specs, specs_error }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La insignia muestra `v4.2` para `4.2.0` y deja intacta una versión con parche.
    #[test]
    fn insignia_de_version() {
        assert_eq!(version_badge("4.2.0"), "v4.2");
        assert_eq!(version_badge("4.2.1"), "v4.2.1");
    }
}
