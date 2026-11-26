//! IN-02: textos de la vista mini_archive. Un campo por texto; el compilador exige los dos idiomas.

pub struct MiniArchiveStrings {
    pub completed: &'static str,
    pub failed: &'static str,
    pub extracting: &'static str,
    pub compressing_zip: &'static str,
    pub destination: &'static str,
    pub preparing_files: &'static str,
}

pub static ES: MiniArchiveStrings = MiniArchiveStrings {
    completed: "Completado",
    failed: "No se pudo completar",
    extracting: "Descomprimiendo archivo…",
    compressing_zip: "Comprimiendo a ZIP…",
    destination: "Destino",
    preparing_files: "Preparando archivos…",
};

pub static EN: MiniArchiveStrings = MiniArchiveStrings {
    completed: "Completed",
    failed: "Could not complete",
    extracting: "Extracting archive…",
    compressing_zip: "Compressing to ZIP…",
    destination: "Destination",
    preparing_files: "Preparing files…",
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.completed,
                s.failed,
                s.extracting,
                s.compressing_zip,
                s.destination,
                s.preparing_files,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
