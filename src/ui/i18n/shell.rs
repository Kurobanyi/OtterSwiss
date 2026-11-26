//! IN-02: textos de la ventana (aviso de actualización) y de la cola de instalación.

pub struct ShellStrings {
    pub download_and_install: &'static str,
    pub release_notes: &'static str,
    pub update_available_v: fn(&dyn std::fmt::Display) -> String,
    pub current_version_v_new_version: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub downloading: fn(&dyn std::fmt::Display) -> String,
    pub installing_dep: fn(&dyn std::fmt::Display) -> String,
    pub dep_installed: fn(&dyn std::fmt::Display) -> String,
    pub queue_apps: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: ShellStrings = ShellStrings {
    download_and_install: "Descargar e instalar",
    release_notes: "Notas de la versión:",
    update_available_v: |a0| format!("Actualización disponible: v{a0}"),
    current_version_v_new_version: |pkg_ver, a1| {
        format!("Versión actual: v{pkg_ver} → Nueva versión: v{a1}")
    },
    downloading: |a0| format!("Descargando {a0}%…"),
    installing_dep: |dep_id| format!("Instalando '{dep_id}'…"),
    dep_installed: |dep_id| format!("'{dep_id}' instalado."),
    queue_apps: |apps| format!("Cola: {apps}…"),
};

pub static EN: ShellStrings = ShellStrings {
    download_and_install: "Download and Install",
    release_notes: "Release Notes:",
    update_available_v: |a0| format!("Update available: v{a0}"),
    current_version_v_new_version: |pkg_ver, a1| {
        format!("Current version: v{pkg_ver} → New version: v{a1}")
    },
    downloading: |a0| format!("Downloading {a0}%…"),
    installing_dep: |dep_id| format!("Installing '{dep_id}'…"),
    dep_installed: |dep_id| format!("'{dep_id}' installed."),
    queue_apps: |apps| format!("Queue: {apps}…"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [s.download_and_install, s.release_notes] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
