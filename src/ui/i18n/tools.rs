//! IN-02: textos de la vista tools. Un campo por texto; el compilador exige los dos idiomas.

pub struct ToolsStrings {
    pub essential_official_windows_libraries: &'static str,
    pub explorer_context_menu: &'static str,
    pub gaming_runtimes: &'static str,
    pub install: &'static str,
    pub installing_please_keep_otterswiss_open: &'static str,
    pub missing: &'static str,
    pub move_idle_pages_to_windows: &'static str,
    pub not_installed: &'static str,
    pub ram_optimizer: &'static str,
    pub ram_working_sets_trimmed: &'static str,
    pub right_click_compress_zip_extract: &'static str,
    pub trim_ram_now: &'static str,
    pub trimming: &'static str,
    pub trimming_ram_working_sets: &'static str,
    pub ram_trimmed_processes_skipped: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub trimmed_skipped: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub install_selected: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: ToolsStrings = ToolsStrings {
    essential_official_windows_libraries: "Bibliotecas oficiales esenciales",
    explorer_context_menu: "Menú contextual",
    gaming_runtimes: "Dependencias para juegos",
    install: "Instalar",
    installing_please_keep_otterswiss_open: "Instalando — No cierres OtterSwiss hasta que finalice.",
    missing: "Faltante",
    move_idle_pages_to_windows: "Mueve páginas inactivas a Standby",
    not_installed: "No Instalado",
    ram_optimizer: "Optimización de RAM",
    ram_working_sets_trimmed: "Optimización de RAM Completada",
    right_click_compress_zip_extract: "Comprimir (.zip) y extraer en clic derecho",
    trim_ram_now: "Optimizar RAM",
    trimming: "Procesando…",
    trimming_ram_working_sets: "Optimizando memoria RAM…",
    ram_trimmed_processes_skipped: |trimmed, skipped| {
        format!("RAM optimizada: {trimmed} procesos, {skipped} omitidos.")
    },
    trimmed_skipped: |a0, a1| format!("Procesados: {a0} | Omitidos: {a1}"),
    install_selected: |selected_count| format!("Instalar ({selected_count})"),
};

pub static EN: ToolsStrings = ToolsStrings {
    essential_official_windows_libraries: "Essential official Windows libraries",
    explorer_context_menu: "Explorer Context Menu",
    gaming_runtimes: "Gaming Runtimes",
    install: "Install",
    installing_please_keep_otterswiss_open: "Installing — Please keep OtterSwiss open until finished.",
    missing: "Missing",
    move_idle_pages_to_windows: "Move idle pages to Windows Standby list",
    not_installed: "Not Installed",
    ram_optimizer: "RAM Optimizer",
    ram_working_sets_trimmed: "RAM Working Sets Trimmed",
    right_click_compress_zip_extract: "Right-click Compress (.zip) & Extract",
    trim_ram_now: "Trim RAM Now",
    trimming: "Trimming…",
    trimming_ram_working_sets: "Trimming RAM working sets…",
    ram_trimmed_processes_skipped: |trimmed, skipped| {
        format!("RAM trimmed: {trimmed} processes, {skipped} skipped.")
    },
    trimmed_skipped: |a0, a1| format!("Trimmed: {a0} | Skipped: {a1}"),
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
                s.move_idle_pages_to_windows,
                s.not_installed,
                s.ram_optimizer,
                s.ram_working_sets_trimmed,
                s.right_click_compress_zip_extract,
                s.trim_ram_now,
                s.trimming,
                s.trimming_ram_working_sets,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
