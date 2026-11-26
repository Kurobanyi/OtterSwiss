//! IN-03 y UI-11: texto que se arma al mostrarse, no al crearse. Un aviso guardado como `Msg` cambia de idioma junto
//! con el resto de la ventana; un error de core muestra su título en el idioma del usuario y el detalle técnico.

use std::fmt;
use std::sync::Arc;

use super::strings;
use crate::core::error::{CoreError, ErrorKind};

/// Texto de un aviso, en el idioma que tenga la ventana cuando se dibuja.
#[derive(Clone)]
pub struct Msg(Arc<dyn Fn(bool) -> String + Send + Sync>);

impl Msg {
    /// Arma el texto con el idioma (`en`) del momento de mostrarlo.
    pub fn new(build: impl Fn(bool) -> String + Send + Sync + 'static) -> Self {
        Self(Arc::new(build))
    }

    /// Texto que no depende del idioma (una ruta, un nombre, un detalle técnico).
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        Self::new(move |_| text.clone())
    }

    /// Error de core: título traducido según su tipo y, si lo hay, el detalle técnico.
    pub fn error(e: &CoreError) -> Self {
        let kind = e.kind();
        let detail = e.detail();
        Self::new(move |en| error_text(kind, &detail, en))
    }

    /// Igual que [`Msg::error`] con un texto antes («Falló el respaldo: …»).
    pub fn error_with(e: &CoreError, lead: fn(bool) -> &'static str) -> Self {
        let kind = e.kind();
        let detail = e.detail();
        Self::new(move |en| format!("{} {}", lead(en), error_text(kind, &detail, en)))
    }

    pub fn text(&self, en: bool) -> String {
        (self.0)(en)
    }
}

impl From<CoreError> for Msg {
    fn from(e: CoreError) -> Self {
        Self::error(&e)
    }
}

impl From<&CoreError> for Msg {
    fn from(e: &CoreError) -> Self {
        Self::error(e)
    }
}

impl PartialEq for Msg {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Msg").field(&self.text(false)).finish()
    }
}

fn error_text(kind: ErrorKind, detail: &str, en: bool) -> String {
    let title = (strings(en).error_title)(kind);
    if detail.is_empty() {
        title.to_string()
    } else {
        // El detalle que core escribe en español (la operación que pidió administrador, por ejemplo) se traduce si
        // está en el catálogo; si no, se muestra tal cual, como cualquier detalle técnico.
        format!("{title}: {}", super::core_text::tr(detail, en))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IN-03: el mismo aviso se dibuja en el idioma del momento.
    #[test]
    fn el_aviso_cambia_de_idioma() {
        let msg = Msg::error(&CoreError::Cancelled);
        assert_eq!(msg.text(false), "Operación cancelada");
        assert_eq!(msg.text(true), "Operation cancelled");

        let msg = Msg::from(CoreError::Validation("ruta fuera de la carpeta".into()));
        assert_eq!(msg.text(true), "Invalid data: ruta fuera de la carpeta");
    }
}
