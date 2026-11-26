//! IN-02: textos de la vista cleaner. Un campo por texto; el compilador exige los dos idiomas.

pub struct CleanerStrings {
    pub all_temp_files_skip_24h: &'static str,
    pub before_cleaning: &'static str,
    pub clean_now: &'static str,
    pub cleaning: &'static str,
    pub cleanup: &'static str,
    pub cleanup_progress: &'static str,
    pub close_open_apps_except_otterswiss: &'static str,
    pub details: &'static str,
    pub empty_recycle_bin_permanent: &'static str,
    pub files: &'static str,
    pub files_removed: &'static str,
    pub mode_full_0h: &'static str,
    pub mode_safe_24h: &'static str,
    pub safe_temp_cache_purge: &'static str,
    pub space_freed: &'static str,
    pub start_cleanup: &'static str,
    pub system_cleaner: &'static str,
    pub the_recycle_bin_will_be: &'static str,
    pub categories_could_not_be_cleaned: fn(&dyn std::fmt::Display) -> String,
    pub removed_skipped: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
}

pub static ES: CleanerStrings = CleanerStrings {
    all_temp_files_skip_24h: "Todo el directorio (sin espera de 24h)",
    before_cleaning: "Antes de limpiar",
    clean_now: "Limpiar ahora",
    cleaning: "Limpiando…",
    cleanup: "Limpieza",
    cleanup_progress: "Progreso",
    close_open_apps_except_otterswiss: "Cierra las apps abiertas, menos OtterSwiss, para evitar archivos en uso.",
    details: "Detalle",
    empty_recycle_bin_permanent: "Vaciar Papelera (permanente)",
    files: "archivos",
    files_removed: "Archivos Borrados",
    mode_full_0h: "Modo: Total (0h)",
    mode_safe_24h: "Modo: Seguro (> 24h)",
    safe_temp_cache_purge: "Purga segura de temporales y caché",
    space_freed: "Espacio Liberado",
    start_cleanup: "Iniciar Limpieza",
    system_cleaner: "Limpiador de Sistema",
    the_recycle_bin_will_be: "Se vaciará la Papelera: sus archivos no se podrán recuperar después.",
    categories_could_not_be_cleaned: |error_count| {
        format!("{error_count} categorías no se pudieron limpiar. Mira el detalle.")
    },
    removed_skipped: |a0, a1| format!("Borrados: {a0} | Omitidos: {a1}"),
};

pub static EN: CleanerStrings = CleanerStrings {
    all_temp_files_skip_24h: "All temp files (skip 24h wait)",
    before_cleaning: "Before cleaning",
    clean_now: "Clean now",
    cleaning: "Cleaning…",
    cleanup: "Cleanup",
    cleanup_progress: "Cleanup Progress",
    close_open_apps_except_otterswiss: "Close open apps except OtterSwiss to avoid locked files.",
    details: "Details",
    empty_recycle_bin_permanent: "Empty Recycle Bin (permanent)",
    files: "files",
    files_removed: "Files Removed",
    mode_full_0h: "Mode: Full (0h)",
    mode_safe_24h: "Mode: Safe (> 24h)",
    safe_temp_cache_purge: "Safe temp & cache purge",
    space_freed: "Space Freed",
    start_cleanup: "Start Cleanup",
    system_cleaner: "System Cleaner",
    the_recycle_bin_will_be: "The Recycle Bin will be emptied: its files cannot be recovered afterwards.",
    categories_could_not_be_cleaned: |error_count| {
        format!("{error_count} categories could not be cleaned. See the details.")
    },
    removed_skipped: |a0, a1| format!("Removed: {a0} | Skipped: {a1}"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.all_temp_files_skip_24h,
                s.before_cleaning,
                s.clean_now,
                s.cleaning,
                s.cleanup,
                s.cleanup_progress,
                s.close_open_apps_except_otterswiss,
                s.details,
                s.empty_recycle_bin_permanent,
                s.files,
                s.files_removed,
                s.mode_full_0h,
                s.mode_safe_24h,
                s.safe_temp_cache_purge,
                s.space_freed,
                s.start_cleanup,
                s.system_cleaner,
                s.the_recycle_bin_will_be,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
