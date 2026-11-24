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
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Dark => Self::Cyberpunk,
            Self::Cyberpunk => Self::Nord,
            Self::Nord => Self::Emerald,
            Self::Emerald => Self::Crimson,
            Self::Crimson => Self::Light,
            Self::Light => Self::Dark,
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
        if let Some(path) = Self::config_file_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}