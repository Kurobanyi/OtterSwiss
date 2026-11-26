//! OR-07: icono de una app de la tienda (dos trazados SVG) sobre un fondo del color de acento. Estilo en `.app-icon`.

use dioxus::prelude::*;

use crate::core::store::app_store::AppIcon;

#[component]
pub fn AppIconBadge(icon: AppIcon) -> Element {
    let (p1, p2) = icon_paths(icon);
    rsx! {
        div { class: "app-icon", "aria-hidden": "true",
            svg { view_box: "0 0 24 24",
                if !p1.is_empty() {
                    path { d: "{p1}" }
                }
                if !p2.is_empty() {
                    path { d: "{p2}" }
                }
            }
        }
    }
}

/// Trazados del icono (el segundo puede ir vacío).
fn icon_paths(icon: AppIcon) -> (&'static str, &'static str) {
    match icon {
        AppIcon::Gamepad => (
            "M6 12h4m-2-2v4",
            "M15 11h.01M18 13h.01M17.5 7A5.5 5.5 0 0 0 12 12.5a5.5 5.5 0 0 0-5.5-5.5A5.5 5.5 0 0 0 2 12.5C2 16.5 6 20 12 20s10-3.5 10-7.5A5.5 5.5 0 0 0 17.5 7z",
        ),
        AppIcon::Browser => (
            "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2z",
            "M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
        ),
        AppIcon::Chat => (
            "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
            "",
        ),
        AppIcon::Video => (
            "M23 7l-7 5 7 5V7z",
            "M1 5a2 2 0 0 1 2-2h11a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V5z",
        ),
        AppIcon::Code => ("M16 18l6-6-6-6", "M8 6l-6 6 6 6"),
        AppIcon::Terminal => ("M4 17l6-6-6-6", "M12 19h8"),
        AppIcon::Archive => ("M21 8v13H3V8", "M1 3h22v5H1zM10 12h4"),
        AppIcon::Download => (
            "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4",
            "M7 10l5 5 5-5M12 15V3",
        ),
        AppIcon::Search => ("M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16z", "M21 21l-4.35-4.35"),
        AppIcon::Wrench => (
            "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z",
            "",
        ),
        AppIcon::Disk => (
            "M22 12H2",
            "M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11zM6 16h.01M10 16h.01",
        ),
        AppIcon::Camera => (
            "M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z",
            "M12 17a4 4 0 1 0 0-8 4 4 0 0 0 0 8z",
        ),
        AppIcon::Clock => ("M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2z", "M12 6v6l4 2"),
        AppIcon::Calculator => (
            "M4 2h16a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z",
            "M8 6h8M16 14v4M12 14v4M8 14v4M16 10h.01M12 10h.01M8 10h.01",
        ),
        AppIcon::Microphone => (
            "M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z",
            "M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8",
        ),
        AppIcon::Phone => (
            "M17 2H7a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2z",
            "M12 18h.01",
        ),
        AppIcon::Checklist => (
            "M9 11l3 3L22 4",
            "M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11",
        ),
        AppIcon::Store => (
            "M6 2L3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4zM3 6h18",
            "M16 10a4 4 0 0 1-8 0",
        ),
        AppIcon::Paint => (
            "M18.37 2.63L14 7l3 3 4.37-4.37a2.12 2.12 0 1 0-3-3z",
            "M9 12l-7 7a1 1 0 0 0 0 1.41l1.59 1.59a1 1 0 0 0 1.41 0l7-7M12 15l3 3",
        ),
        AppIcon::Scissors => (
            "M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
            "M20 4L8.12 15.88M14.47 14.48L20 20M8.12 8.12L12 12",
        ),
    }
}
