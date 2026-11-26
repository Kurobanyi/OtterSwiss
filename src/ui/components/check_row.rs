//! Fila con casilla: el `label` que envuelve el `input` y su contenido.

use dioxus::prelude::*;

/// Casilla con su contenido dentro de un `label`, para que todo el renglón sea clicable (CL-25). `class` agrega
/// utilidades al `row clickable` base (por ejemplo `item-row gap-3`); el contenido que deba empujarse a la derecha
/// usa `grow`.
#[component]
pub fn CheckRow(
    checked: bool,
    #[props(default = false)] disabled: bool,
    #[props(default)] class: String,
    on_change: EventHandler<bool>,
    children: Element,
) -> Element {
    rsx! {
        label { class: "row clickable {class}",
            input {
                r#type: "checkbox",
                checked,
                disabled,
                onchange: move |evt| on_change.call(evt.checked()),
            }
            {children}
        }
    }
}
