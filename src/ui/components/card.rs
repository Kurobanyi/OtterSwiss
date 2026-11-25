//! Tarjeta con título, subtítulo opcional y contenido.

use dioxus::prelude::*;

#[component]
pub fn Card(title: String, subtitle: Option<String>, children: Element) -> Element {
    rsx! {
        div { class: "card",
            div { class: "card-header",
                div {
                    h3 { class: "card-title", "{title}" }
                    if let Some(sub) = subtitle {
                        p { class: "card-subtitle", "{sub}" }
                    }
                }
            }
            {children}
        }
    }
}
