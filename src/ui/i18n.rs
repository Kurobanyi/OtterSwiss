use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Es,
    En,
}

impl Lang {
    pub fn is_en(self) -> bool {
        matches!(self, Self::En)
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Es => Self::En,
            Self::En => Self::Es,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Es => "ES",
            Self::En => "EN",
        }
    }

    pub fn config_file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("OtterSwiss").join("lang.json"))
    }

    pub fn load_persisted() -> Self {
        if let Some(path) = Self::config_file_path() {
            if path.exists() {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(lang) = serde_json::from_slice::<Lang>(&bytes) {
                        return lang;
                    }
                }
            }
        }
        Self::Es
    }

    pub fn save_persisted(self) {
        // El proceso corre elevado: se escribe sin seguir junctions (SEC-19).
        if let (Some(root), Ok(json)) = (dirs::config_dir(), serde_json::to_vec_pretty(&self)) {
            let _ = crate::core::safe_fs::write_file_inside(&root, "OtterSwiss", "lang.json", &json);
        }
    }
}