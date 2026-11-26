//! IN-02: textos de la ventana (título, aviso de actualización), de la cola de instalación y de los diálogos
//! nativos que salen antes de abrir Dioxus (WebView2 que falta, error inesperado).

pub struct ShellStrings {
    pub window_title: &'static str,
    pub window_title_archiver: &'static str,
    pub webview2_title: &'static str,
    pub webview2_ask: &'static str,
    pub webview2_not_detected: &'static str,
    pub webview2_failed: fn(&dyn std::fmt::Display) -> String,
    pub panic_title: &'static str,
    pub panic_intro: &'static str,
    pub panic_unknown: &'static str,
    pub download_and_install: &'static str,
    pub release_notes: &'static str,
    pub update_available_v: fn(&dyn std::fmt::Display) -> String,
    pub current_version_v_new_version: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub downloading: fn(&dyn std::fmt::Display) -> String,
    pub installing_dep: fn(&dyn std::fmt::Display) -> String,
    pub dep_installed: fn(&dyn std::fmt::Display) -> String,
    pub queue_apps: fn(&dyn std::fmt::Display) -> String,
    pub standard_mode: &'static str,
    pub standard_mode_hint: &'static str,
    pub relaunch_as_admin: &'static str,
}

pub static ES: ShellStrings = ShellStrings {
    window_title: "OtterSwiss — Suite Nativa Win32",
    window_title_archiver: "OtterSwiss — Archivador",
    webview2_title: "OtterSwiss — Falta WebView2",
    webview2_ask: "OtterSwiss necesita Microsoft Edge WebView2 Runtime para mostrar su ventana y no está instalado.\n\n\
        ¿Descargarlo e instalarlo ahora desde Microsoft? (unos 2 MB, verificado antes de ejecutarse)",
    webview2_not_detected: "WebView2 se instaló pero todavía no se detecta. Vuelve a abrir OtterSwiss.",
    webview2_failed: |e| {
        format!(
            "No se pudo instalar WebView2: {e}\n\nInstálalo desde \
             https://developer.microsoft.com/microsoft-edge/webview2/ y vuelve a abrir OtterSwiss."
        )
    },
    panic_title: "OtterSwiss — Error inesperado",
    panic_intro: "OtterSwiss se cerró por un error inesperado:",
    panic_unknown: "error desconocido",
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
    standard_mode: "Modo estándar",
    standard_mode_hint: "Lo que cambia el sistema (optimizar, Debloater, DNS, drivers, tienda) y parte de la limpieza piden administrador; el resto funciona así.",
    relaunch_as_admin: "Reabrir como administrador",
};

pub static EN: ShellStrings = ShellStrings {
    window_title: "OtterSwiss — Native Win32 Suite",
    window_title_archiver: "OtterSwiss — Archiver",
    webview2_title: "OtterSwiss — WebView2 required",
    webview2_ask: "OtterSwiss needs the Microsoft Edge WebView2 Runtime to show its window, and it is not installed.\n\n\
        Download and install it now from Microsoft? (about 2 MB, verified before running)",
    webview2_not_detected: "WebView2 was installed but is not detected yet. Restart OtterSwiss.",
    webview2_failed: |e| {
        format!(
            "WebView2 could not be installed: {e}\n\nInstall it from \
             https://developer.microsoft.com/microsoft-edge/webview2/ and open OtterSwiss again."
        )
    },
    panic_title: "OtterSwiss — Unexpected error",
    panic_intro: "OtterSwiss closed because of an unexpected error:",
    panic_unknown: "unknown error",
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
    standard_mode: "Standard mode",
    standard_mode_hint: "What changes the system (optimizer, Debloater, DNS, drivers, store) and part of the cleanup need an administrator; the rest works as is.",
    relaunch_as_admin: "Reopen as administrator",
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.download_and_install,
                s.release_notes,
                s.window_title,
                s.window_title_archiver,
                s.webview2_title,
                s.webview2_ask,
                s.webview2_not_detected,
                s.panic_title,
                s.panic_intro,
                s.panic_unknown,
                s.standard_mode,
                s.standard_mode_hint,
                s.relaunch_as_admin,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
