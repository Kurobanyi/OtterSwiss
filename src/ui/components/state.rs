//! Estados de una sección: cargando, vacía y con error.

use dioxus::prelude::*;

#[component]
pub fn LoadingState(message: String) -> Element {
    rsx! {
        // UI-11: sin el «Cargando...» fijo en español; el mensaje ya viene traducido.
        div { class: "state-container", role: "status",
            div { class: "state-spinner", "aria-hidden": "true" }
            p { class: "state-desc", "{message}" }
        }
    }
}

#[component]
pub fn EmptyState(title: String, description: String) -> Element {
    rsx! {
        div { class: "state-container",
            h4 { class: "state-title", "{title}" }
            p { class: "state-desc", "{description}" }
        }
    }
}

#[component]
pub fn ErrorState(title: String, error_msg: String) -> Element {
    rsx! {
        div { class: "state-container state-error",
            h4 { class: "state-title", "{title}" }
            p { class: "state-desc", "{error_msg}" }
        }
    }
}
