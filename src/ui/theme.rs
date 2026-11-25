use serde::{Deserialize, Serialize};

use crate::ui::prefs;

const PREF_FILE: &str = "theme.json";

/// Tema visual. `System` sigue el modo claro u oscuro de Windows (TE-03); el resto es fijo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
    Cyberpunk,
    Nord,
    Emerald,
    Crimson,
    Dracula,
    TokyoNight,
    Solarized,
    Sakura,
}

impl Theme {
    /// Orden en el selector de tema.
    pub const ALL: [Theme; 11] = [
        Self::System,
        Self::Dark,
        Self::Light,
        Self::Cyberpunk,
        Self::Nord,
        Self::Emerald,
        Self::Crimson,
        Self::Dracula,
        Self::TokyoNight,
        Self::Solarized,
        Self::Sakura,
    ];

    /// Identificador estable: se guarda en `theme.json` y es el `value` del selector.
    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Cyberpunk => "cyberpunk",
            Self::Nord => "nord",
            Self::Emerald => "emerald",
            Self::Crimson => "crimson",
            Self::Dracula => "dracula",
            Self::TokyoNight => "tokyonight",
            Self::Solarized => "solarized",
            Self::Sakura => "sakura",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// El tema concreto que se pinta: `System` se resuelve con la preferencia de Windows.
    pub fn resolved(self) -> Self {
        match self {
            Self::System if crate::core::platform::user_settings::apps_use_light_theme() => {
                Self::Light
            }
            Self::System => Self::Dark,
            other => other,
        }
    }

    /// Valor de `data-theme` en el CSS (siempre un tema concreto).
    pub fn css_name(self) -> &'static str {
        self.resolved().key()
    }

    /// Color de fondo de la ventana antes del primer render. Tiene que ser `--bg` del tema en `main.css`
    /// (lo comprueba el test `fondo_inicial_igual_al_css`).
    pub fn background_rgba(self) -> (u8, u8, u8, u8) {
        match self.resolved() {
            Self::System | Self::Dark => (0x0f, 0x11, 0x1a, 0xff),
            Self::Cyberpunk => (0x0d, 0x09, 0x1a, 0xff),
            Self::Light => (0xf1, 0xf5, 0xf9, 0xff),
            Self::Nord => (0x2e, 0x34, 0x40, 0xff),
            Self::Emerald => (0x07, 0x13, 0x0e, 0xff),
            Self::Crimson => (0x12, 0x09, 0x0c, 0xff),
            Self::Dracula => (0x28, 0x2a, 0x36, 0xff),
            Self::TokyoNight => (0x1a, 0x1b, 0x26, 0xff),
            Self::Solarized => (0x00, 0x2b, 0x36, 0xff),
            Self::Sakura => (0x19, 0x17, 0x24, 0xff),
        }
    }

    pub fn display_name(self, en: bool) -> &'static str {
        match (self, en) {
            (Self::System, false) => "Según Windows",
            (Self::System, true) => "Match Windows",
            (Self::Dark, false) => "Oscuro",
            (Self::Dark, true) => "Dark",
            (Self::Light, false) => "Claro",
            (Self::Light, true) => "Light",
            (Self::Cyberpunk, _) => "Cyberpunk",
            (Self::Nord, _) => "Nord",
            (Self::Emerald, false) => "Esmeralda",
            (Self::Emerald, true) => "Emerald",
            (Self::Crimson, false) => "Carmesí",
            (Self::Crimson, true) => "Crimson",
            (Self::Dracula, _) => "Dracula",
            (Self::TokyoNight, _) => "Tokyo Night",
            (Self::Solarized, false) => "Solarizado",
            (Self::Solarized, true) => "Solarized",
            (Self::Sakura, _) => "Sakura",
        }
    }

    pub fn load_persisted() -> Self {
        prefs::load(PREF_FILE).unwrap_or_default()
    }

    pub fn save_persisted(self) {
        prefs::save(PREF_FILE, &self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--bg` del bloque `[data-theme="<key>"]` de `main.css`.
    fn css_bg(css: &str, key: &str) -> (u8, u8, u8, u8) {
        let marker = format!("[data-theme=\"{key}\"]");
        let block = &css[css.find(&marker).expect("tema en main.css")..];
        let block = &block[..block.find('}').expect("fin del bloque")];
        let hex = block
            .split("--bg:")
            .nth(1)
            .expect("--bg en el tema")
            .trim_start()
            .trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex");
        (byte(0), byte(2), byte(4), 0xff)
    }

    #[test]
    fn fondo_inicial_igual_al_css() {
        let css = include_str!("../../assets/main.css");
        for theme in Theme::ALL.into_iter().filter(|t| *t != Theme::System) {
            assert_eq!(
                theme.background_rgba(),
                css_bg(css, theme.key()),
                "{}",
                theme.key()
            );
        }
    }

    #[test]
    fn claves_de_tema_ida_y_vuelta() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_key(theme.key()), Some(theme));
        }
        assert_eq!(Theme::from_key("inexistente"), None);
    }
}
