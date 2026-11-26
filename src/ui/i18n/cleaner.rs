//! IN-02: textos de la vista cleaner. Un campo por texto; el compilador exige los dos idiomas.

pub struct CleanerStrings {
    pub all_temp_files_skip_24h: &'static str,
    pub all_temp_files_desc: &'static str,
    pub empty_recycle_bin_desc: &'static str,
    pub options: &'static str,
    pub results: &'static str,
    pub skipped_reason: &'static str,
    pub cleanup_done: &'static str,
    pub cleanup_cancelled: &'static str,
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
    all_temp_files_skip_24h: "Borrar todos los temporales",
    all_temp_files_desc: "Incluye los archivos de menos de 24 horas. Los que estén en uso se omiten.",
    empty_recycle_bin_desc: "Permanente: los archivos no se podrán recuperar después.",
    options: "Qué limpiar",
    results: "Resultado",
    skipped_reason: "Omitida",
    cleanup_done: "Limpieza terminada.",
    cleanup_cancelled: "Limpieza cancelada: abajo está lo que alcanzó a borrar.",
    before_cleaning: "Antes de limpiar",
    clean_now: "Limpiar ahora",
    cleaning: "Limpiando…",
    cleanup: "Limpieza",
    cleanup_progress: "Progreso",
    close_open_apps_except_otterswiss: "Cierra las apps abiertas, menos OtterSwiss, para evitar archivos en uso.",
    details: "Detalle",
    empty_recycle_bin_permanent: "Vaciar la Papelera",
    files: "archivos",
    files_removed: "Archivos borrados",
    mode_full_0h: "Modo total: sin espera",
    mode_safe_24h: "Modo seguro: más de 24 h",
    safe_temp_cache_purge: "Elimina los temporales y la caché del sistema. Por defecto solo borra lo que lleva más de 24 horas sin usarse.",
    space_freed: "Espacio liberado",
    start_cleanup: "Iniciar limpieza",
    system_cleaner: "Liberar espacio",
    the_recycle_bin_will_be: "Se vaciará la Papelera: sus archivos no se podrán recuperar después.",
    categories_could_not_be_cleaned: |error_count| {
        format!("{error_count} categorías no se pudieron limpiar. Mira el detalle.")
    },
    removed_skipped: |a0, a1| format!("Borrados: {a0} · Omitidos: {a1}"),
};

pub static EN: CleanerStrings = CleanerStrings {
    all_temp_files_skip_24h: "Delete all temp files",
    all_temp_files_desc: "Includes files newer than 24 hours. Files in use are skipped.",
    empty_recycle_bin_desc: "Permanent: the files cannot be recovered afterwards.",
    options: "What to clean",
    results: "Result",
    skipped_reason: "Skipped",
    cleanup_done: "Cleanup finished.",
    cleanup_cancelled: "Cleanup cancelled: below is what it deleted before stopping.",
    before_cleaning: "Before cleaning",
    clean_now: "Clean now",
    cleaning: "Cleaning…",
    cleanup: "Cleanup",
    cleanup_progress: "Cleanup progress",
    close_open_apps_except_otterswiss: "Close open apps except OtterSwiss to avoid locked files.",
    details: "Details",
    empty_recycle_bin_permanent: "Empty the Recycle Bin",
    files: "files",
    files_removed: "Files removed",
    mode_full_0h: "Full mode: no wait",
    mode_safe_24h: "Safe mode: older than 24 h",
    safe_temp_cache_purge: "Removes temp files and system cache. By default it only deletes what has been unused for over 24 hours.",
    space_freed: "Space freed",
    start_cleanup: "Start cleanup",
    system_cleaner: "Free up space",
    the_recycle_bin_will_be: "The Recycle Bin will be emptied: its files cannot be recovered afterwards.",
    categories_could_not_be_cleaned: |error_count| {
        format!("{error_count} categories could not be cleaned. See the details.")
    },
    removed_skipped: |a0, a1| format!("Removed: {a0} · Skipped: {a1}"),
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
                s.all_temp_files_desc,
                s.empty_recycle_bin_desc,
                s.options,
                s.results,
                s.skipped_reason,
                s.cleanup_done,
                s.cleanup_cancelled,
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
