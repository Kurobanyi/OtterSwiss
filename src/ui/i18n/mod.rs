pub mod drivers;
pub mod strings;

use serde::{Deserialize, Serialize};

pub use strings::{Strings, plural};

use crate::ui::prefs;

const PREF_FILE: &str = "lang.json";

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

    /// Etiqueta del botón de idioma.
    pub fn code(self) -> &'static str {
        match self {
            Self::Es => "ES",
            Self::En => "EN",
        }
    }

    /// Código BCP 47 para `<html lang>` (IN-07): Narrador y NVDA eligen la voz con él.
    pub fn html_code(self) -> &'static str {
        match self {
            Self::Es => "es",
            Self::En => "en",
        }
    }

    /// IN-01: sin preferencia guardada se usa el idioma de Windows (español si es español, si no inglés).
    pub fn from_system() -> Self {
        if crate::core::platform::user_settings::user_ui_language_is_spanish() {
            Self::Es
        } else {
            Self::En
        }
    }

    /// Catálogo de textos del idioma (IN-02).
    pub fn t(self) -> &'static Strings {
        match self {
            Self::Es => &strings::ES,
            Self::En => &strings::EN,
        }
    }

    pub fn load_persisted() -> Self {
        prefs::load(PREF_FILE).unwrap_or_else(Self::from_system)
    }

    pub fn save_persisted(self) {
        prefs::save(PREF_FILE, &self);
    }
}

/// Catálogo para las vistas que ya tienen `en: bool` a mano.
pub fn strings(en: bool) -> &'static Strings {
    if en { &strings::EN } else { &strings::ES }
}
