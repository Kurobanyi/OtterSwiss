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
        div { class: "state-container",
            div { class: "state-spinner" }
            h4 { class: "state-title", "Cargando..." }
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