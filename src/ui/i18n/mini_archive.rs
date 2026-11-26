//! IN-02: textos de la vista mini_archive. Un campo por texto; el compilador exige los dos idiomas.

pub struct MiniArchiveStrings {
    pub completed: &'static str,
    pub preparing_files: &'static str,
}

pub static ES: MiniArchiveStrings = MiniArchiveStrings {
    completed: "Completado",
    preparing_files: "Preparando archivos…",
};

pub static EN: MiniArchiveStrings = MiniArchiveStrings {
    completed: "Completed",
    preparing_files: "Preparing files…",
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [s.completed, s.preparing_files] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
