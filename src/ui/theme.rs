// src/ui/theme.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Cyberpunk,
    Light,
    Nord,
    Emerald,
    Crimson,
    Dracula,
    TokyoNight,
    Solarized,
    Sakura,
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Cyberpunk => "cyberpunk",
            Self::Light => "light",
            Self::Nord => "nord",
            Self::Emerald => "emerald",
            Self::Crimson => "crimson",
            Self::Dracula => "dracula",
            Self::TokyoNight => "tokyonight",
            Self::Solarized => "solarized",
            Self::Sakura => "sakura",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Dark => Self::Cyberpunk,
            Self::Cyberpunk => Self::Nord,
            Self::Nord => Self::Emerald,
            Self::Emerald => Self::Crimson,
            Self::Crimson => Self::Dracula,
            Self::Dracula => Self::TokyoNight,
            Self::TokyoNight => Self::Solarized,
            Self::Solarized => Self::Sakura,
            Self::Sakura => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    /// Color de fondo de la ventana antes del primer render (igual a `--bg` del tema en `main.css`).
    pub fn background_rgba(&self) -> (u8, u8, u8, u8) {
        match self {
            Self::Dark => (0x0f, 0x11, 0x1a, 0xff),
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

    pub fn display_name(&self, en: bool) -> &'static str {
        match (self, en) {
            (Self::Dark, false) => "Oscuro",
            (Self::Dark, true) => "Dark",
            (Self::Cyberpunk, _) => "Cyberpunk",
            (Self::Light, false) => "Claro",
            (Self::Light, true) => "Light",
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

    pub fn config_file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("OtterSwiss").join("theme.json"))
    }

    pub fn load_persisted() -> Self {
        if let Some(path) = Self::config_file_path() {
            if path.exists() {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(theme) = serde_json::from_slice::<Theme>(&bytes) {
                        return theme;
                    }
                }
            }
        }
        Self::Dark
    }

    pub fn save_persisted(&self) {
        if let (Some(root), Ok(json)) = (dirs::config_dir(), serde_json::to_vec_pretty(self)) {
            let _ =
                crate::core::safe_fs::write_file_inside(&root, "OtterSwiss", "theme.json", &json);
        }
    }
}