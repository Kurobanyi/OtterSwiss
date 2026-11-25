//! IN-02: catálogo de textos de la interfaz. Un campo por texto y un `static` por idioma: si a un idioma le falta un
//! campo, no compila, así que se conserva la garantía de que no hay textos sin traducir.
//!
//! Regla para textos nuevos: van aquí, no como `if en { … } else { … }` en la vista. Los que tienen parámetros son
//! métodos de `Strings` con argumentos con nombre (el orden de los argumentos no puede desordenarse en un solo idioma).

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
