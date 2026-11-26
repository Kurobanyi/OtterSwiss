//! Diálogos modales accesibles: `Modal` genérico y `ConfirmDialog` para confirmar una acción.

use super::{CardHeader, Tone};
use dioxus::prelude::*;

/// Diálogo modal accesible (UI-09): `role="dialog"`, `aria-modal`, Escape cierra y el foco entra al abrir.
/// El foco queda atrapado dentro con el script global de `main.rs` (`FOCUS_TRAP_SCRIPT`).
#[component]
pub fn Modal(
    title: String,
    subtitle: Option<String>,
    #[props(default)] tone: Tone,
    #[props(default = false)] wide: bool,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    let class = format!(
        "card modal-card {} {}",
        tone.modal_class(),
        if wide { "modal-wide" } else { "" }
    );
    rsx! {
        div {
            class: "modal-overlay",
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Escape {
                    evt.stop_propagation();
                    on_close.call(());
                }
            },
            div {
                class: "{class}",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "{title}",
                tabindex: "-1",
                onmounted: move |evt: MountedEvent| async move {
                    let _ = evt.set_focus(true).await;
                },
                CardHeader { title: title.clone(), subtitle, actions: None }
                {children}
            }
        }
    }
}

/// Confirmación de una acción con efecto en el sistema (patrón 8 de `docs/BUGS.md`).
#[component]
pub fn ConfirmDialog(
    title: String,
    subtitle: Option<String>,
    confirm_label: String,
    cancel_label: String,
    #[props(default)] tone: Tone,
    #[props(default = false)] wide: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
    children: Element,
) -> Element {
    let confirm_class = if tone == Tone::Danger {
        "btn btn-danger"
    } else {
        "btn btn-primary"
    };
    rsx! {
        Modal {
            title,
            subtitle,
            tone,
            wide,
            on_close: move |_| on_cancel.call(()),
            {children}
            div { class: "modal-actions",
                button { class: "btn btn-secondary", onclick: move |_| on_cancel.call(()), "{cancel_label}" }
                button { class: "{confirm_class}", onclick: move |_| on_confirm.call(()), "{confirm_label}" }
            }
        }
    }
}
