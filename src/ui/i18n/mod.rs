pub mod archiver;
pub mod cleaner;
pub mod core_text;
pub mod dashboard;
pub mod debloater;
pub mod dns;
pub mod drivers;
pub mod hardware;
pub mod mini_archive;
pub mod msg;
pub mod sensors;
pub mod shell;
pub mod strings;
pub mod tools;
pub mod uninstaller;

use serde::{Deserialize, Serialize};

pub use msg::Msg;
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

/// Elige entre los dos textos de un dato bilingüe de core (la política de privacidad de un proveedor de DNS, la
/// descripción de una app de la tienda): core guarda el par y la interfaz escoge según el idioma.
pub fn pick<'a>(en: bool, es_text: &'a str, en_text: &'a str) -> &'a str {
    if en { en_text } else { es_text }
}
