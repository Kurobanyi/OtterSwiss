//! Cabecera de una tarjeta o de un diálogo: título y subtítulo opcional.

use dioxus::prelude::*;

/// Cabecera común de `Card` y `Modal` (CL-11): el markup vivía copiado en los dos. `actions` son los botones a la derecha.
#[component]
pub fn CardHeader(title: String, subtitle: Option<String>, actions: Option<Element>) -> Element {
    rsx! {
        div { class: "card-header",
            div {
                h3 { class: "card-title", "{title}" }
                if let Some(sub) = subtitle {
                    p { class: "card-subtitle", "{sub}" }
                }
            }
            if let Some(actions) = actions {
                {actions}
            }
        }
    }
}
