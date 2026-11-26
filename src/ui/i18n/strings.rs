//! IN-02: catálogo de textos de la interfaz. Un campo por texto y un `static` por idioma: si a un idioma le falta un
//! campo, no compila, así que se conserva la garantía de que no hay textos sin traducir.
//!
//! Regla para textos nuevos: van aquí, no como `if en { … } else { … }` en la vista. Cada vista tiene su módulo
//! (`archiver`, `cleaner`, `dashboard`…) y `strings(en).<vista>.<texto>`. Los que llevan datos son `fn(&dyn Display…)`: cada
//! idioma arma la frase con sus propios marcadores `{nombre}`, así el orden de los datos puede cambiar entre idiomas.

use super::archiver::{self, ArchiverStrings};
use super::cleaner::{self, CleanerStrings};
use super::dashboard::{self, DashboardStrings};
use super::debloater::{self, DebloaterStrings};
use super::dns::{self, DnsStrings};
use super::drivers::{self, DriversStrings};
use super::hardware::{self, HardwareStrings};
use super::mini_archive::{self, MiniArchiveStrings};
use super::sensors::{self, SensorsStrings};
use super::settings::{self, SettingsStrings};
use super::shell::{self, ShellStrings};
use super::tools::{self, ToolsStrings};
use super::uninstaller::{self, UninstallerStrings};
use crate::core::error::ErrorKind;

/// Textos fijos de la interfaz.
pub struct Strings {
    pub cancel: &'static str,
    pub close: &'static str,
    pub later: &'static str,
    pub minimize: &'static str,
    pub maximize: &'static str,
    pub theme: &'static str,
    pub switch_language: &'static str,
    pub uninstall: &'static str,
    pub clear: &'static str,
    pub starting: &'static str,
    pub scanning: &'static str,
    pub running: &'static str,
    pub installing: &'static str,
    pub installed_app: &'static str,
    pub installed_runtime: &'static str,
    pub reload_list: &'static str,
    pub open_folder: &'static str,
    pub completed: &'static str,
    pub time_left: &'static str,
    /// Plural: (singular, plural) para «adaptador» y «proceso» (IN-04).
    pub adapter: (&'static str, &'static str),
    pub process: (&'static str, &'static str),
    pub app: (&'static str, &'static str),
    pub leftover: (&'static str, &'static str),
    pub queued: &'static str,
    pub done: &'static str,
    pub store_title: &'static str,
    pub store_subtitle: &'static str,
    pub store_search: &'static str,
    pub store_empty: &'static str,
    pub store_empty_hint: &'static str,
    pub all_categories: &'static str,
    /// Versión de las apps que siempre se descargan en su última versión («Evergreen», UI-15).
    pub latest_version: &'static str,
    /// IN-08: pestaña Drivers.
    pub drivers: &'static DriversStrings,
    pub mini_archive: &'static MiniArchiveStrings,
    pub shell: &'static ShellStrings,
    pub archiver: &'static ArchiverStrings,
    pub cleaner: &'static CleanerStrings,
    pub dashboard: &'static DashboardStrings,
    pub debloater: &'static DebloaterStrings,
    pub dns: &'static DnsStrings,
    pub hardware: &'static HardwareStrings,
    pub sensors: &'static SensorsStrings,
    pub settings: &'static SettingsStrings,
    pub tools: &'static ToolsStrings,
    pub uninstaller: &'static UninstallerStrings,
    /// UI-11: título de un error de core según su tipo; el detalle técnico va detrás.
    pub error_title: fn(ErrorKind) -> &'static str,
}

pub static ES: Strings = Strings {
    cancel: "Cancelar",
    close: "Cerrar",
    later: "Más tarde",
    minimize: "Minimizar",
    maximize: "Maximizar",
    theme: "Tema",
    switch_language: "Cambiar idioma (ES/EN)",
    uninstall: "Desinstalar",
    clear: "Limpiar",
    starting: "Iniciando…",
    scanning: "Escaneando…",
    running: "Ejecutando…",
    installing: "Instalando…",
    installed_app: "Instalada",
    installed_runtime: "Instalado",
    reload_list: "Recargar lista",
    open_folder: "Abrir carpeta",
    completed: "Completado",
    time_left: "Restante",
    adapter: ("adaptador", "adaptadores"),
    process: ("proceso", "procesos"),
    app: ("app", "apps"),
    leftover: ("resto encontrado", "restos encontrados"),
    queued: "En cola",
    done: "Listo",
    store_title: "Tienda silenciosa y restaurador UWP",
    store_subtitle: "Descargas oficiales y apps de Microsoft Store con winget",
    store_search: "Buscar app, herramienta o desarrollador…",
    store_empty: "Ninguna app coincide",
    store_empty_hint: "Cambia la categoría o borra el texto de búsqueda.",
    all_categories: "Todas las categorías",
    latest_version: "última versión",
    drivers: &drivers::ES,
    mini_archive: &mini_archive::ES,
    shell: &shell::ES,
    archiver: &archiver::ES,
    cleaner: &cleaner::ES,
    dashboard: &dashboard::ES,
    debloater: &debloater::ES,
    dns: &dns::ES,
    hardware: &hardware::ES,
    sensors: &sensors::ES,
    settings: &settings::ES,
    tools: &tools::ES,
    uninstaller: &uninstaller::ES,
    error_title: |kind| match kind {
        ErrorKind::Io => "Error de lectura o escritura",
        ErrorKind::NotElevated => "Hace falta ejecutar como administrador",
        ErrorKind::Cancelled => "Operación cancelada",
        ErrorKind::AlreadyRunning => "Otra instancia de OtterSwiss está haciendo cambios",
        ErrorKind::Registry => "Error del registro",
        ErrorKind::Service => "Error de un servicio",
        ErrorKind::Power => "Error de energía",
        ErrorKind::Dns => "Error de DNS",
        ErrorKind::Trust => "Descarga o firma no confiable",
        ErrorKind::Backup => "Error del respaldo",
        ErrorKind::Archive => "Error del archivo comprimido",
        ErrorKind::Uninstaller => "Error del desinstalador",
        ErrorKind::SystemTool => "Error de una herramienta del sistema",
        ErrorKind::NoActiveAdapters => "No hay adaptadores de red físicos activos",
        ErrorKind::Validation => "Datos no válidos",
        ErrorKind::Panic => "Error interno inesperado",
    },
};

pub static EN: Strings = Strings {
    cancel: "Cancel",
    close: "Close",
    later: "Later",
    minimize: "Minimize",
    maximize: "Maximize",
    theme: "Theme",
    switch_language: "Switch language (ES/EN)",
    uninstall: "Uninstall",
    clear: "Clear",
    starting: "Starting…",
    scanning: "Scanning…",
    running: "Running…",
    installing: "Installing…",
    installed_app: "Installed",
    installed_runtime: "Installed",
    reload_list: "Reload list",
    open_folder: "Open folder",
    completed: "Completed",
    time_left: "ETA",
    adapter: ("adapter", "adapters"),
    process: ("process", "processes"),
    app: ("app", "apps"),
    leftover: ("leftover", "leftovers"),
    queued: "Queued",
    done: "Done",
    store_title: "Silent app store and UWP restore",
    store_subtitle: "Official vendor downloads and Microsoft Store apps through winget",
    store_search: "Search app, tool or publisher…",
    store_empty: "No app matches",
    store_empty_hint: "Change the category or clear the search text.",
    all_categories: "All categories",
    latest_version: "latest version",
    drivers: &drivers::EN,
    mini_archive: &mini_archive::EN,
    shell: &shell::EN,
    archiver: &archiver::EN,
    cleaner: &cleaner::EN,
    dashboard: &dashboard::EN,
    debloater: &debloater::EN,
    dns: &dns::EN,
    hardware: &hardware::EN,
    sensors: &sensors::EN,
    settings: &settings::EN,
    tools: &tools::EN,
    uninstaller: &uninstaller::EN,
    error_title: |kind| match kind {
        ErrorKind::Io => "Read or write error",
        ErrorKind::NotElevated => "Administrator rights required",
        ErrorKind::Cancelled => "Operation cancelled",
        ErrorKind::AlreadyRunning => "Another OtterSwiss instance is making changes",
        ErrorKind::Registry => "Registry error",
        ErrorKind::Service => "Service error",
        ErrorKind::Power => "Power management error",
        ErrorKind::Dns => "DNS error",
        ErrorKind::Trust => "Untrusted download or signature",
        ErrorKind::Backup => "Backup error",
        ErrorKind::Archive => "Archive error",
        ErrorKind::Uninstaller => "Uninstaller error",
        ErrorKind::SystemTool => "System tool error",
        ErrorKind::NoActiveAdapters => "No active physical network adapters",
        ErrorKind::Validation => "Invalid data",
        ErrorKind::Panic => "Unexpected internal error",
    },
};

/// IN-04: «1 adaptador», «3 adaptadores». ES y EN solo distinguen uno de varios.
pub fn plural(n: usize, forms: (&'static str, &'static str)) -> String {
    format!("{n} {}", if n == 1 { forms.0 } else { forms.1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IN-06: ningún texto vacío, y los que llevan puntos suspensivos usan «…».
    #[test]
    fn catalogos_completos_y_con_elipsis() {
        for s in [&ES, &EN] {
            let fields = [
                s.cancel,
                s.close,
                s.later,
                s.minimize,
                s.maximize,
                s.theme,
                s.switch_language,
                s.uninstall,
                s.clear,
                s.starting,
                s.scanning,
                s.running,
                s.installing,
                s.installed_app,
                s.installed_runtime,
                s.reload_list,
                s.open_folder,
                s.completed,
                s.time_left,
                s.queued,
                s.done,
                s.store_title,
                s.store_subtitle,
                s.store_search,
                s.store_empty,
                s.store_empty_hint,
                s.all_categories,
                s.latest_version,
            ];
            for f in fields {
                assert!(!f.trim().is_empty());
                assert!(!f.contains("..."), "usar «…»: {f}");
            }
        }
    }

    #[test]
    fn plurales() {
        assert_eq!(plural(1, ES.adapter), "1 adaptador");
        assert_eq!(plural(3, ES.adapter), "3 adaptadores");
        assert_eq!(plural(0, EN.process), "0 processes");
    }
}
