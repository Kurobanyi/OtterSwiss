//! TE-31: disposición de la ventana. Por defecto la navegación va arriba, junto a la nutria que arrastra la ventana (así
//! la quiere el dueño del proyecto); en Configuración se puede pasar a una barra lateral que se contrae a solo iconos.

use serde::{Deserialize, Serialize};

use crate::ui::prefs;

const PREF_FILE: &str = "layout.json";

/// Dónde va la navegación entre secciones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NavStyle {
    /// Pestañas en la barra superior (predeterminado).
    #[default]
    Top,
    /// Barra lateral a la izquierda.
    Side,
}

impl NavStyle {
    /// Valor de `data-nav` en la raíz de la app.
    pub fn css_name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Side => "side",
        }
    }
}

/// Preferencias de disposición, guardadas en `layout.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LayoutPrefs {
    pub nav: NavStyle,
    /// Barra lateral contraída a solo iconos (solo se usa con `NavStyle::Side`).
    pub sidebar_collapsed: bool,
}

impl LayoutPrefs {
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

    /// Sin archivo, o con uno viejo al que le falta un campo, la navegación va arriba.
    #[test]
    fn por_defecto_la_navegacion_va_arriba() {
        assert_eq!(LayoutPrefs::default().nav, NavStyle::Top);
        let parcial: LayoutPrefs =
            serde_json::from_str(r#"{"sidebar_collapsed":true}"#).expect("json");
        assert_eq!(parcial.nav, NavStyle::Top);
        assert!(parcial.sidebar_collapsed);
    }
}
