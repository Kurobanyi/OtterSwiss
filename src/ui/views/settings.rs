//! TE-31: Configuración de la apariencia: navegación (arriba o lateral), tema e idioma. Cada cambio se aplica al
//! momento y queda guardado.

use dioxus::prelude::*;

use crate::ui::i18n::{Lang, strings};
use crate::ui::layout::{LayoutPrefs, NavStyle};
use crate::ui::theme::Theme;

#[component]
pub fn SettingsView() -> Element {
    let mut layout = use_context::<Signal<LayoutPrefs>>();
    let mut theme = use_context::<Signal<Theme>>();
    let mut lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let txt = strings(en).settings;
    let current = *layout.read();
    let current_theme = *theme.read();

    let mut set_layout = move |next: LayoutPrefs| {
        layout.set(next);
        next.save_persisted();
    };

    rsx! {
        div { class: "content-body",
            section { class: "set-section",
                div { class: "set-head",
                    h2 { class: "section-title", "{txt.navigation}" }
                    p { class: "text-muted", "{txt.navigation_hint}" }
                }
                div { class: "set-layouts", role: "group", "aria-label": "{txt.navigation}",
                    for (style, name, hint) in [
                        (NavStyle::Top, txt.nav_top, txt.nav_top_hint),
                        (NavStyle::Side, txt.nav_side, txt.nav_side_hint),
                    ] {
                        button {
                            key: "{style.css_name()}",
                            class: "set-layout-card",
                            "aria-pressed": if current.nav == style { "true" } else { "false" },
                            onclick: move |_| set_layout(LayoutPrefs { nav: style, ..current }),
                            // Miniatura dibujada con CSS: barra superior o lateral y bloques de contenido.
                            div { class: "set-layout-preview", "data-nav": "{style.css_name()}", "aria-hidden": "true",
                                div { class: "slp-bar" }
                                div { class: "slp-side" }
                                div { class: "slp-body",
                                    div { class: "slp-line slp-line-strong" }
                                    div { class: "slp-line" }
                                    div { class: "slp-block" }
                                }
                            }
                            div { class: "set-layout-text",
                                span { class: "fw-semibold", "{name}" }
                                if style == NavStyle::Top {
                                    span { class: "badge badge-info", "{txt.default_tag}" }
                                }
                            }
                            span { class: "text-xs text-muted", "{hint}" }
                        }
                    }
                }
                if current.nav == NavStyle::Side {
                    label { class: "set-toggle-row",
                        div { class: "stack-1 grow",
                            span { class: "fw-semibold", "{txt.collapse_sidebar}" }
                            span { class: "text-xs text-muted", "{txt.collapse_sidebar_hint}" }
                        }
                        input {
                            r#type: "checkbox",
                            role: "switch",
                            class: "toggle-switch",
                            checked: current.sidebar_collapsed,
                            onchange: move |e| set_layout(LayoutPrefs { sidebar_collapsed: e.checked(), ..current }),
                        }
                    }
                }
            }

            section { class: "set-section",
                div { class: "set-head",
                    h2 { class: "section-title", "{txt.theme}" }
                    p { class: "text-muted", "{txt.theme_hint}" }
                }
                div { class: "set-themes", role: "group", "aria-label": "{txt.theme}",
                    for t in Theme::ALL {
                        button {
                            key: "{t.key()}",
                            class: "set-theme-card",
                            "aria-pressed": if current_theme == t { "true" } else { "false" },
                            onclick: move |_| {
                                theme.set(t);
                                t.save_persisted();
                            },
                            // La muestra lleva el data-theme de ese tema: se pinta con sus propios colores.
                            div { class: "set-theme-swatch", "data-theme": "{t.css_name()}", "aria-hidden": "true",
                                div { class: "sts-surface",
                                    div { class: "sts-line" }
                                    div { class: "sts-line sts-line-short" }
                                    div { class: "sts-accent" }
                                }
                            }
                            span { class: "set-theme-name", "{t.display_name(en)}" }
                        }
                    }
                }
            }

            section { class: "set-section",
                div { class: "set-head",
                    h2 { class: "section-title", "{txt.language}" }
                    p { class: "text-muted", "{txt.language_hint}" }
                }
                div { class: "arc-segmented", role: "group", "aria-label": "{txt.language}",
                    for (l, name) in [(Lang::Es, "Español"), (Lang::En, "English")] {
                        button {
                            key: "{l.code()}",
                            class: "btn btn-sm",
                            "aria-pressed": if *lang.read() == l { "true" } else { "false" },
                            onclick: move |_| {
                                lang.set(l);
                                l.save_persisted();
                            },
                            "{name}"
                        }
                    }
                }
            }
        }
    }
}
