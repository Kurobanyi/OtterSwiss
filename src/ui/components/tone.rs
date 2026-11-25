//! Tono de un aviso o diálogo (resultado, advertencia, peligro).

/// Tono visual de un aviso o de un diálogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Info,
    Success,
    Warning,
    Danger,
}

impl Tone {
    pub(super) fn badge_class(self) -> &'static str {
        match self {
            Tone::Info => "badge-info",
            Tone::Success => "badge-success",
            Tone::Warning => "badge-warning",
            Tone::Danger => "badge-danger",
        }
    }

    pub(super) fn modal_class(self) -> &'static str {
        match self {
            Tone::Danger => "modal-danger",
            Tone::Warning => "modal-warning",
            _ => "",
        }
    }
}
