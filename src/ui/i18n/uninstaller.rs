//! IN-02: textos de la vista uninstaller. Un campo por texto; el compilador exige los dos idiomas.

pub struct UninstallerStrings {
    pub check_for_update: &'static str,
    pub deep_uninstaller: &'static str,
    pub files_and_folders_are_deleted: &'static str,
    pub installed_applications: &'static str,
    pub its_uninstaller_will_run_silently: &'static str,
    pub locked_items: &'static str,
    pub only_the_items_marked_as: &'static str,
    pub reading_installed_apps: &'static str,
    pub review: &'static str,
    pub kind_folder: &'static str,
    pub kind_file: &'static str,
    pub kind_registry: &'static str,
    pub kind_startup: &'static str,
    pub restart_to_finish: &'static str,
    pub scan_leftovers: &'static str,
    pub search_app_or_publisher: &'static str,
    pub size_largest: &'static str,
    pub size_smallest: &'static str,
    pub sort: &'static str,
    pub uninstall_apps_and_purge_leftover: &'static str,
    pub scanning_leftovers_for: fn(&dyn std::fmt::Display) -> String,
    pub could_not_scan: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub no_leftovers_found_for: fn(&dyn std::fmt::Display) -> String,
    pub found_for: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub uninstalling: fn(&dyn std::fmt::Display) -> String,
    pub could_not_uninstall: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub the_uninstaller_of_ended_with: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub is_still_installed_its_uninstaller: fn(&dyn std::fmt::Display) -> String,
    pub uninstalled_but_its_leftovers_could:
        fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub uninstalled_no_leftovers_found:
        fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub uninstalled_review_its_leftovers:
        fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub could_not_read_the_installed: fn(&dyn std::fmt::Display) -> String,
    pub cleaned_removed_failed_freed: fn(
        &dyn std::fmt::Display,
        &dyn std::fmt::Display,
        &dyn std::fmt::Display,
        &dyn std::fmt::Display,
    ) -> String,
    pub leftovers_for: fn(&dyn std::fmt::Display) -> String,
    pub delete_permanently: fn(&dyn std::fmt::Display) -> String,
    pub uninstall: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: UninstallerStrings = UninstallerStrings {
    check_for_update: "Buscar actualización",
    deep_uninstaller: "Desinstalador profundo",
    files_and_folders_are_deleted: "Los archivos y carpetas se borran de forma permanente (no van a la Papelera). Las claves de registro se exportan antes a ProgramData\\OtterSwiss\\RegBackups.",
    installed_applications: "Aplicaciones instaladas",
    its_uninstaller_will_run_silently: "Su desinstalador se ejecutará en modo silencioso, sin pedir confirmación. Después podrás revisar sus restos antes de borrar nada.",
    locked_items: "Elementos en uso",
    only_the_items_marked_as: "Solo vienen marcados los restos seguros. Revisa el resto antes de eliminar.",
    reading_installed_apps: "Leyendo las apps instaladas…",
    review: "Revisar",
    kind_folder: "Carpeta",
    kind_file: "Archivo",
    kind_registry: "Registro",
    kind_startup: "Inicio",
    restart_to_finish: " Reinicia Windows para terminar.",
    scan_leftovers: "Escanear restos",
    search_app_or_publisher: "Buscar aplicación o fabricante…",
    size_largest: "Tamaño (mayor)",
    size_smallest: "Tamaño (menor)",
    sort: "Orden:",
    uninstall_apps_and_purge_leftover: "Desinstala y purga archivos y registro residual",
    scanning_leftovers_for: |name| format!("Buscando restos de '{name}'…"),
    could_not_scan: |name, a1| format!("No se pudo escanear '{name}': {a1}"),
    no_leftovers_found_for: |name| format!("No quedaron restos de '{name}'."),
    found_for: |leftovers, name| format!("{leftovers} de '{name}'."),
    uninstalling: |name| format!("Desinstalando '{name}'…"),
    could_not_uninstall: |name, a1| format!("No se pudo desinstalar '{name}': {a1}"),
    the_uninstaller_of_ended_with: |name, code| {
        format!("El desinstalador de '{name}' terminó con el código {code}; no se buscaron restos.")
    },
    is_still_installed_its_uninstaller: |name| {
        format!(
            "'{name}' sigue instalada; su desinstalador puede necesitar un reinicio o tu intervención."
        )
    },
    uninstalled_but_its_leftovers_could: |name, a1| {
        format!("'{name}' desinstalada, pero no se pudieron buscar sus restos: {a1}")
    },
    uninstalled_no_leftovers_found: |name, a1| {
        format!("'{name}' desinstalada; no se encontraron restos.{a1}")
    },
    uninstalled_review_its_leftovers: |name, a1| {
        format!("'{name}' desinstalada; revisa sus restos.{a1}")
    },
    could_not_read_the_installed: |a0| format!("No se pudieron leer las apps instaladas: {a0}"),
    cleaned_removed_failed_freed: |a0, a1, a2, a3| {
        format!("Purga de '{a0}': {a1} eliminados, {a2} con error ({a3} liberados).")
    },
    leftovers_for: |a0| format!("Rastros de '{a0}'"),
    delete_permanently: |sel_count| format!("Borrar de forma permanente ({sel_count})"),
    uninstall: |a0| format!("¿Desinstalar '{a0}'?"),
};

pub static EN: UninstallerStrings = UninstallerStrings {
    check_for_update: "Check for update",
    deep_uninstaller: "Deep Uninstaller",
    files_and_folders_are_deleted: "Files and folders are deleted permanently (not to the Recycle Bin). Registry keys are exported first to ProgramData\\OtterSwiss\\RegBackups.",
    installed_applications: "Installed Applications",
    its_uninstaller_will_run_silently: "Its uninstaller will run silently, without asking for confirmation. Afterwards you can review its leftovers before deleting anything.",
    locked_items: "Locked items",
    only_the_items_marked_as: "Only the items marked as safe are preselected. Review the rest before purging.",
    reading_installed_apps: "Reading installed apps…",
    review: "Review",
    kind_folder: "Folder",
    kind_file: "File",
    kind_registry: "Registry",
    kind_startup: "Startup",
    restart_to_finish: " Restart Windows to finish.",
    scan_leftovers: "Scan Leftovers",
    search_app_or_publisher: "Search app or publisher…",
    size_largest: "Size (Largest)",
    size_smallest: "Size (Smallest)",
    sort: "Sort:",
    uninstall_apps_and_purge_leftover: "Uninstall apps and purge leftover files & registry",
    scanning_leftovers_for: |name| format!("Scanning leftovers for '{name}'…"),
    could_not_scan: |name, a1| format!("Could not scan '{name}': {a1}"),
    no_leftovers_found_for: |name| format!("No leftovers found for '{name}'."),
    found_for: |leftovers, name| format!("Found {leftovers} for '{name}'."),
    uninstalling: |name| format!("Uninstalling '{name}'…"),
    could_not_uninstall: |name, a1| format!("Could not uninstall '{name}': {a1}"),
    the_uninstaller_of_ended_with: |name, code| {
        format!("The uninstaller of '{name}' ended with code {code}; leftovers were not scanned.")
    },
    is_still_installed_its_uninstaller: |name| {
        format!("'{name}' is still installed; its uninstaller may need a restart or your input.")
    },
    uninstalled_but_its_leftovers_could: |name, a1| {
        format!("'{name}' uninstalled, but its leftovers could not be scanned: {a1}")
    },
    uninstalled_no_leftovers_found: |name, a1| {
        format!("'{name}' uninstalled; no leftovers found.{a1}")
    },
    uninstalled_review_its_leftovers: |name, a1| {
        format!("'{name}' uninstalled; review its leftovers.{a1}")
    },
    could_not_read_the_installed: |a0| format!("Could not read the installed apps: {a0}"),
    cleaned_removed_failed_freed: |a0, a1, a2, a3| {
        format!("Cleaned '{a0}': {a1} removed, {a2} failed ({a3} freed).")
    },
    leftovers_for: |a0| format!("Leftovers for '{a0}'"),
    delete_permanently: |sel_count| format!("Delete permanently ({sel_count})"),
    uninstall: |a0| format!("Uninstall '{a0}'?"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.check_for_update,
                s.deep_uninstaller,
                s.files_and_folders_are_deleted,
                s.installed_applications,
                s.its_uninstaller_will_run_silently,
                s.locked_items,
                s.only_the_items_marked_as,
                s.reading_installed_apps,
                s.review,
                s.kind_folder,
                s.kind_file,
                s.kind_registry,
                s.kind_startup,
                s.restart_to_finish,
                s.scan_leftovers,
                s.search_app_or_publisher,
                s.size_largest,
                s.size_smallest,
                s.sort,
                s.uninstall_apps_and_purge_leftover,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
