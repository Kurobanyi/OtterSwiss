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
/// Tono visual de un aviso o de un diálogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Info,
    Success,
    Warning,
    Danger,
}

impl Tone {
    fn badge_class(self) -> &'static str {
        match self {
            Tone::Info => "badge-info",
            Tone::Success => "badge-success",
            Tone::Warning => "badge-warning",
            Tone::Danger => "badge-danger",
        }
    }

    fn modal_class(self) -> &'static str {
        match self {
            Tone::Danger => "modal-danger",
            Tone::Warning => "modal-warning",
            _ => "",
        }
    }
}

/// Aviso de resultado de una acción (UI-02: el tono sale del resultado real, no del `Ok`).
#[component]
pub fn StatusBanner(text: String, tone: Tone) -> Element {
    let class = format!("status-banner {}", tone.badge_class());
    let role = if tone == Tone::Danger { "alert" } else { "status" };
    rsx! {
        div { class: "{class}", role: "{role}", "{text}" }
    }
}

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
    let confirm_class = if tone == Tone::Danger { "btn btn-danger" } else { "btn btn-primary" };
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
