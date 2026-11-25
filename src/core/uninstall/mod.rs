//! Desinstalador: lista de apps, ejecución de su desinstalador, búsqueda de restos y purga.

mod inventory;
mod leftovers;
mod purge;
mod runner;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::core::error::CoreResult;
use crate::core::platform::process::INSTALLER_TIMEOUT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub display_name: String,
    pub display_version: String,
    pub publisher: String,
    pub uninstall_string: String,
    pub quiet_uninstall_string: Option<String>,
    pub install_location: Option<PathBuf>,
    pub display_icon: Option<PathBuf>,
    pub estimated_size_kb: Option<u64>,
    pub is_per_user: bool,
    pub is_wow64: bool,
    pub registry_subkey: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftoverKind {
    Directory,
    File,
    RegistryKey,
    RegistryValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftoverItem {
    pub id: String,
    pub kind: LeftoverKind,
    pub category: String,
    pub target_display: String,
    pub path: Option<PathBuf>,
    pub reg_is_hkcu: bool,
    pub reg_wow64_32: bool,
    pub reg_subkey: Option<String>,
    pub reg_value_name: Option<String>,
    pub size_bytes: u64,
    /// Si la UI lo marca por defecto. Solo lo es lo que con seguridad pertenece a la app: datos de usuario
    /// (AppData, HKCU) y carpetas de todo un fabricante se muestran pero quedan sin marcar.
    pub recommended: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PurgeReport {
    pub removed_count: usize,
    pub failed_count: usize,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
}

pub struct UninstallerManager;

/// AR-01: resultado completo de «desinstalar y buscar restos», para que la vista solo lo traduzca a texto.
#[derive(Debug, Clone)]
pub enum UninstallOutcome {
    /// SYS-02: el desinstalador devolvió un código de fallo; no se buscan restos.
    Failed { code: String },
    /// Terminó bien pero la app sigue registrada (puede pedir reinicio o intervención del usuario).
    StillRegistered,
    /// Desinstalada. `leftovers` es lo que encontró el escaneo, o su error.
    Removed {
        reboot_required: bool,
        leftovers: Result<Vec<LeftoverItem>, String>,
    },
}

impl UninstallerManager {
    /// Ejecuta el desinstalador de `app` (con `INSTALLER_TIMEOUT`), comprueba que se haya ido y busca restos.
    pub fn uninstall_and_scan(app: &InstalledApp) -> CoreResult<UninstallOutcome> {
        let exit = Self::run_uninstaller(app, INSTALLER_TIMEOUT)?;
        if !exit.is_success() {
            return Ok(UninstallOutcome::Failed {
                code: exit.code_display(),
            });
        }
        if Self::is_still_registered(app) {
            return Ok(UninstallOutcome::StillRegistered);
        }
        Ok(UninstallOutcome::Removed {
            reboot_required: exit.exit_code != 0,
            leftovers: Self::scan_leftovers(app).map_err(|e| e.to_string()),
        })
    }

    /// Ids de los restos que vienen marcados por defecto (SYS-03: solo los seguros).
    pub fn recommended_ids(items: &[LeftoverItem]) -> HashSet<String> {
        items
            .iter()
            .filter(|i| i.recommended)
            .map(|i| i.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::leftovers::{build_app_tokens, is_recommended_leftover, matches_any_token};
    use super::purge::is_safe_registry_subkey_to_delete;
    use super::runner::parse_cmd_exe_and_args;
    use super::*;

    fn app(name: &str, publisher: &str, location: Option<&str>) -> InstalledApp {
        InstalledApp {
            id: name.into(),
            display_name: name.into(),
            display_version: String::new(),
            publisher: publisher.into(),
            uninstall_string: String::new(),
            quiet_uninstall_string: None,
            install_location: location.map(PathBuf::from),
            display_icon: None,
            estimated_size_kb: None,
            is_per_user: false,
            is_wow64: false,
            registry_subkey: String::new(),
        }
    }

    /// SYS-03: la coincidencia es exacta; una subcadena marcaba datos de otras apps.
    #[test]
    fn tokens_coinciden_solo_exactos() {
        let tokens = vec!["Python".to_string(), "Office".to_string()];
        assert!(matches_any_token("python", &tokens));
        assert!(!matches_any_token("Python312", &tokens));
        assert!(!matches_any_token("LibreOffice", &tokens));
    }

    #[test]
    fn tokens_quitan_version_y_carpetas_genericas() {
        let (tokens, publisher) = build_app_tokens(&app(
            "Foo Editor 2.1 (x64)",
            "Foo Inc.",
            Some(r"C:\Program Files\Games"),
        ));
        assert!(tokens.iter().any(|t| t == "Foo Editor"));
        assert!(!tokens.iter().any(|t| t.eq_ignore_ascii_case("games")));
        assert_eq!(publisher.as_deref(), Some("Foo Inc."));
    }

    #[test]
    fn nombres_protegidos_no_son_tokens() {
        let (tokens, _) = build_app_tokens(&app("Microsoft", "Microsoft", Some(r"C:\Windows")));
        assert!(tokens.is_empty());
    }

    #[test]
    fn separa_ejecutable_y_argumentos() {
        assert_eq!(
            parse_cmd_exe_and_args(r#""C:\Program Files\App\unins000.exe" /SILENT"#),
            (r"C:\Program Files\App\unins000.exe", "/SILENT")
        );
        // SEC-22: sin comillas, el corte va tras «.exe», no en el primer espacio.
        assert_eq!(
            parse_cmd_exe_and_args(r"C:\Program Files\App\uninst.exe /S"),
            (r"C:\Program Files\App\uninst.exe", "/S")
        );
        assert_eq!(
            parse_cmd_exe_and_args("MsiExec.exe /X{ABC}"),
            ("MsiExec.exe", "/X{ABC}")
        );
    }

    #[test]
    fn solo_se_borran_subclaves_de_software_concretas() {
        assert!(!is_safe_registry_subkey_to_delete("Software"));
        assert!(!is_safe_registry_subkey_to_delete(r"Software\Microsoft"));
        assert!(!is_safe_registry_subkey_to_delete(r"Software\Classes"));
        assert!(!is_safe_registry_subkey_to_delete(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall"
        ));
        assert!(!is_safe_registry_subkey_to_delete(
            r"Software\Microsoft\Windows\Shell"
        ));
        assert!(!is_safe_registry_subkey_to_delete(
            r"System\CurrentControlSet\Services\x"
        ));
        assert!(is_safe_registry_subkey_to_delete(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\FooApp"
        ));
        assert!(is_safe_registry_subkey_to_delete(
            r"Software\Foo Inc\Foo Editor"
        ));
    }

    fn leftover(category: &str, path: &str) -> LeftoverItem {
        LeftoverItem {
            id: path.into(),
            kind: LeftoverKind::Directory,
            category: category.into(),
            target_display: path.into(),
            path: Some(PathBuf::from(path)),
            reg_is_hkcu: false,
            reg_wow64_32: false,
            reg_subkey: None,
            reg_value_name: None,
            size_bytes: 0,
            recommended: false,
        }
    }

    /// SYS-03: AppData y la carpeta de todo el fabricante no vienen marcadas.
    #[test]
    fn recomendados_excluyen_datos_de_usuario_y_fabricante() {
        // El llamador pasa tokens y fabricante en minúsculas.
        let tokens = vec!["foo editor".to_string()];
        let publisher = Some("foo inc");
        assert!(!is_recommended_leftover(
            &leftover("AppData Roaming", r"C:\Users\u\AppData\Roaming\Foo Editor"),
            &tokens,
            publisher
        ));
        assert!(!is_recommended_leftover(
            &leftover("Program Files", r"C:\Program Files\Foo Inc"),
            &tokens,
            publisher
        ));
        assert!(is_recommended_leftover(
            &leftover("Program Files", r"C:\Program Files\Foo Editor"),
            &tokens,
            publisher
        ));
    }
}
