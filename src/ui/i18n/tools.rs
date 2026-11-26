//! IN-02: textos de la vista tools. Un campo por texto; el compilador exige los dos idiomas.

pub struct ToolsStrings {
    pub essential_official_windows_libraries: &'static str,
    pub explorer_context_menu: &'static str,
    pub gaming_runtimes: &'static str,
    pub install: &'static str,
    pub installing_please_keep_otterswiss_open: &'static str,
    pub missing: &'static str,
    pub not_installed: &'static str,
    pub right_click_compress_zip_extract: &'static str,
    pub install_selected: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: ToolsStrings = ToolsStrings {
    essential_official_windows_libraries: "Bibliotecas oficiales esenciales",
    explorer_context_menu: "Menú contextual",
    gaming_runtimes: "Dependencias para juegos",
    install: "Instalar",
    installing_please_keep_otterswiss_open: "Instalando — No cierres OtterSwiss hasta que finalice.",
    missing: "Faltante",
    not_installed: "No Instalado",
    right_click_compress_zip_extract: "Comprimir (.zip) y extraer en clic derecho",
    install_selected: |selected_count| format!("Instalar ({selected_count})"),
};

pub static EN: ToolsStrings = ToolsStrings {
    essential_official_windows_libraries: "Essential official Windows libraries",
    explorer_context_menu: "Explorer Context Menu",
    gaming_runtimes: "Gaming Runtimes",
    install: "Install",
    installing_please_keep_otterswiss_open: "Installing — Please keep OtterSwiss open until finished.",
    missing: "Missing",
    not_installed: "Not Installed",
    right_click_compress_zip_extract: "Right-click Compress (.zip) & Extract",
    install_selected: |selected_count| format!("Install Selected ({selected_count})"),
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
                s.not_installed,
                s.right_click_compress_zip_extract,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
