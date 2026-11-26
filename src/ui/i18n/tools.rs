//! IN-02: textos de la vista tools. Un campo por texto; el compilador exige los dos idiomas.

pub struct ToolsStrings {
    pub essential_official_windows_libraries: &'static str,
    pub explorer_context_menu: &'static str,
    pub gaming_runtimes: &'static str,
    pub install: &'static str,
    pub installing_please_keep_otterswiss_open: &'static str,
    pub missing: &'static str,
    pub failed: &'static str,
    pub categories_label: &'static str,
    pub not_installed: &'static str,
    pub right_click_compress_zip_extract: &'static str,
    pub install_selected: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: ToolsStrings = ToolsStrings {
    essential_official_windows_libraries: "Bibliotecas oficiales esenciales",
    explorer_context_menu: "Menú contextual del Explorador",
    gaming_runtimes: "Dependencias para juegos",
    install: "Instalar",
    installing_please_keep_otterswiss_open: "No cierres OtterSwiss hasta que termine.",
    missing: "Faltante",
    failed: "Error",
    categories_label: "Categorías",
    not_installed: "No instalado",
    right_click_compress_zip_extract: "Agrega Comprimir (.zip) y Extraer al clic derecho",
    install_selected: |selected_count| format!("Instalar ({selected_count})"),
};

pub static EN: ToolsStrings = ToolsStrings {
    essential_official_windows_libraries: "Essential official Windows libraries",
    explorer_context_menu: "Explorer context menu",
    gaming_runtimes: "Gaming runtimes",
    install: "Install",
    installing_please_keep_otterswiss_open: "Keep OtterSwiss open until it finishes.",
    missing: "Missing",
    failed: "Failed",
    categories_label: "Categories",
    not_installed: "Not installed",
    right_click_compress_zip_extract: "Adds Compress (.zip) and Extract to the right-click menu",
    install_selected: |selected_count| format!("Install ({selected_count})"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.essential_official_windows_libraries,
                s.explorer_context_menu,
                s.gaming_runtimes,
                s.install,
                s.installing_please_keep_otterswiss_open,
                s.missing,
                s.failed,
                s.categories_label,
                s.not_installed,
                s.right_click_compress_zip_extract,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
