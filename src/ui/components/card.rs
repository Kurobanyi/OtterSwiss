//! Tarjeta con título, subtítulo opcional y contenido.

use dioxus::prelude::*;

use super::CardHeader;

#[component]
pub fn Card(
    title: String,
    subtitle: Option<String>,
    actions: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "card",
            CardHeader { title, subtitle, actions }
            {children}
        }
    }
}
