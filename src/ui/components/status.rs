//! Aviso de resultado de una acción.

use super::Tone;
use dioxus::prelude::*;

/// Aviso de resultado de una acción (UI-02: el tono sale del resultado real, no del `Ok`).
#[component]
pub fn StatusBanner(text: String, tone: Tone) -> Element {
    let class = format!("status-banner {}", tone.badge_class());
    let role = if tone == Tone::Danger {
        "alert"
    } else {
        "status"
    };
    rsx! {
        div { class: "{class}", role: "{role}", "{text}" }
    }
}
