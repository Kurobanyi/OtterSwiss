//! TE-31: textos de Configuración (apariencia de la ventana).

pub struct SettingsStrings {
    pub title: &'static str,
    pub description: &'static str,
    pub open_settings: &'static str,
    pub navigation: &'static str,
    pub navigation_hint: &'static str,
    pub nav_top: &'static str,
    pub nav_top_hint: &'static str,
    pub nav_side: &'static str,
    pub nav_side_hint: &'static str,
    pub default_tag: &'static str,
    pub collapse_sidebar: &'static str,
    pub collapse_sidebar_hint: &'static str,
    pub expand_sidebar: &'static str,
    pub theme: &'static str,
    pub theme_hint: &'static str,
    pub language: &'static str,
    pub language_hint: &'static str,
}

pub static ES: SettingsStrings = SettingsStrings {
    title: "Configuración",
    description: "Elige cómo se ve OtterSwiss: navegación, tema e idioma.",
    open_settings: "Configuración",
    navigation: "Navegación",
    navigation_hint: "Dónde van las secciones de la app.",
    nav_top: "Arriba",
    nav_top_hint: "Las secciones en la barra superior, junto a la nutria.",
    nav_side: "Barra lateral",
    nav_side_hint: "Las secciones a la izquierda; se puede contraer a solo iconos.",
    default_tag: "Predeterminado",
    collapse_sidebar: "Contraer la barra lateral",
    collapse_sidebar_hint: "Muestra solo los iconos para dejar más espacio al contenido.",
    expand_sidebar: "Expandir la barra lateral",
    theme: "Tema",
    theme_hint: "Colores de toda la ventana. «Según Windows» sigue el modo claro u oscuro del sistema.",
    language: "Idioma",
    language_hint: "Idioma de la interfaz.",
};

pub static EN: SettingsStrings = SettingsStrings {
    title: "Settings",
    description: "Choose how OtterSwiss looks: navigation, theme and language.",
    open_settings: "Settings",
    navigation: "Navigation",
    navigation_hint: "Where the app sections go.",
    nav_top: "Top",
    nav_top_hint: "Sections in the top bar, next to the otter.",
    nav_side: "Sidebar",
    nav_side_hint: "Sections on the left; it can collapse to icons only.",
    default_tag: "Default",
    collapse_sidebar: "Collapse the sidebar",
    collapse_sidebar_hint: "Shows only the icons to leave more room for the content.",
    expand_sidebar: "Expand the sidebar",
    theme: "Theme",
    theme_hint: "Colors for the whole window. “Match Windows” follows the system light or dark mode.",
    language: "Language",
    language_hint: "Interface language.",
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.title,
                s.description,
                s.open_settings,
                s.navigation,
                s.navigation_hint,
                s.nav_top,
                s.nav_top_hint,
                s.nav_side,
                s.nav_side_hint,
                s.default_tag,
                s.collapse_sidebar,
                s.collapse_sidebar_hint,
                s.expand_sidebar,
                s.theme,
                s.theme_hint,
                s.language,
                s.language_hint,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
