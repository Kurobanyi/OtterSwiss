//! Aviso en línea al estilo InfoBar de Windows 11: icono de tono, título, mensaje y una acción opcional.

use super::Tone;
use dioxus::prelude::*;

/// Aviso persistente de una página (permisos, estado del sistema). Para el resultado de una acción, `StatusBanner`.
#[component]
pub fn InfoBar(tone: Tone, title: String, message: String, action: Option<Element>) -> Element {
    let (class, glyph) = match tone {
        Tone::Info => ("info-bar info-bar-info", "\u{E946}"),
        Tone::Success => ("info-bar info-bar-success", "\u{E73E}"),
        Tone::Warning => ("info-bar info-bar-warning", "\u{E7BA}"),
        Tone::Danger => ("info-bar info-bar-danger", "\u{EA39}"),
    };
    let role = if tone == Tone::Danger {
        "alert"
    } else {
        "status"
    };
    rsx! {
        div { class: "{class}", role: "{role}",
            span { class: "info-bar-icon fluent-icon", "aria-hidden": "true", "{glyph}" }
            div { class: "info-bar-text",
                strong { class: "info-bar-title", "{title}" }
                span { class: "info-bar-message", "{message}" }
            }
            if let Some(action) = action {
                div { class: "info-bar-action", {action} }
            }
        }
    }
}
