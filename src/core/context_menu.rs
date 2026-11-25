use std::io;
use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::folders::{program_files_dir, program_files_x86_dir};
use crate::core::platform::safe_fs::{canonical_dos_path, path_is_strictly_under};

pub struct ContextMenuManager;

struct MenuEntrySpec {
    subkey: &'static str,
    label: &'static str,
    cmd_template: &'static str,
    /// Filtro AQS del verbo (`AppliesTo`); `None` = todos los archivos de la clase.
    applies_to: Option<&'static str>,
}

const MENU_ENTRIES: &[MenuEntrySpec] = &[
    MenuEntrySpec {
        subkey: r"Software\Classes\Directory\Background\shell\OtterSwiss",
        label: "Abrir OtterSwiss",
        cmd_template: "\"{exe}\"",
        applies_to: None,
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\Directory\shell\OtterSwissCompress",
        label: "Comprimir carpeta a .zip (OtterSwiss)",
        cmd_template: "\"{exe}\" --compress-zip \"%V\"",
        applies_to: None,
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\*\shell\OtterSwissCompress",
        label: "Comprimir a .zip (OtterSwiss)",
        cmd_template: "\"{exe}\" --compress-zip \"%1\"",
        applies_to: None,
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.zip\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
        applies_to: None,
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.7z\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
        applies_to: None,
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.tar\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
        applies_to: None,
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.gz\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
        // UI-15: solo se extraen .tar.gz; un .gz suelto no debe ofrecer el verbo.
        applies_to: Some("System.FileName:\"*.tar.gz\""),
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.tgz\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
        applies_to: None,
    },
];

fn notify_shell_assoc_changed() {
    // SAFETY: notificación sin datos (SHCNE_ASSOCCHANGED con punteros nulos).
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

impl ContextMenuManager {
    pub fn is_installed() -> CoreResult<bool> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let primary = MENU_ENTRIES[0].subkey;
        let cmd_key_path = format!(r"{}\command", primary);

        match hklm.open_subkey_with_flags(&cmd_key_path, KEY_READ | KEY_WOW64_64KEY) {
            Ok(key) => {
                let cmd: String = key.get_value("").unwrap_or_default();
                // CL-16: `"exe"` contiene a `exe`, así que basta con buscar la ruta.
                let current_exe = std::env::current_exe()?.to_string_lossy().to_string();
                Ok(cmd.contains(&current_exe))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(CoreError::Registry {
                path: cmd_key_path,
                message: format!("Error verificando menú contextual: {}", e),
                source: Some(e),
            }),
        }
    }

    pub fn install() -> CoreResult<()> {
        ElevationManager::require_elevated("Registrar menú contextual en HKLM")?;

        let current_exe = std::env::current_exe()?;
        // El menú vive en HKLM (lo ven todos los usuarios) y cada clic eleva este ejecutable: solo se registra
        // si está instalado en Program Files, donde un usuario sin privilegios no puede reemplazarlo ni dejar
        // DLLs a su lado (SEC-13). Una copia portable en Descargas no lo registra.
        let exe_canon = canonical_dos_path(&current_exe)?;
        let in_program_files = [program_files_dir(), program_files_x86_dir()]
            .into_iter()
            .flatten()
            .filter_map(|pf| canonical_dos_path(&pf).ok())
            .any(|pf| path_is_strictly_under(&exe_canon, &pf));
        if !in_program_files {
            return Err(CoreError::Validation(format!(
                "El menú contextual solo se registra con OtterSwiss instalado en Program Files (ejecutable actual: '{}')",
                exe_canon.display()
            )));
        }
        // CL-15: una ruta que no es Unicode válido no se escribe con pérdida en el registro: se rechaza.
        let exe_str = exe_canon
            .to_str()
            .ok_or_else(|| {
                CoreError::Validation(format!(
                    "La ruta del ejecutable no es Unicode válido: '{}'",
                    exe_canon.display()
                ))
            })?
            .to_string();
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        for entry in MENU_ENTRIES {
            let _ = hklm.delete_subkey_all(entry.subkey);

            let (menu_key, _) = hklm
                .create_subkey_with_flags(entry.subkey, KEY_WRITE | KEY_WOW64_64KEY)
                .map_err(|e| CoreError::Registry {
                    path: entry.subkey.to_string(),
                    message: format!("Error creando clave de menú contextual: {}", e),
                    source: Some(e),
                })?;

            menu_key.set_value("", &entry.label)?;
            menu_key.set_value("Icon", &format!("{},0", exe_str))?;
            if let Some(filter) = entry.applies_to {
                menu_key.set_value("AppliesTo", &filter)?;
            }

            let (cmd_key, _) = menu_key
                .create_subkey_with_flags("command", KEY_WRITE | KEY_WOW64_64KEY)
                .map_err(|e| CoreError::Registry {
                    path: format!(r"{}\command", entry.subkey),
                    message: format!("Error creando comando de menú contextual: {}", e),
                    source: Some(e),
                })?;

            let final_cmd = entry.cmd_template.replace("{exe}", &exe_str);
            cmd_key.set_value("", &final_cmd)?;
        }

        notify_shell_assoc_changed();
        Ok(())
    }

    pub fn uninstall() -> CoreResult<()> {
        ElevationManager::require_elevated("Eliminar menú contextual de HKLM")?;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for entry in MENU_ENTRIES {
            match hklm.delete_subkey_all(entry.subkey) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(CoreError::Registry {
                        path: entry.subkey.to_string(),
                        message: format!("Error eliminando clave de menú contextual: {}", e),
                        source: Some(e),
                    });
                }
            }
        }

        notify_shell_assoc_changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AR-12: las órdenes del menú contextual solo usan flags que `CliCommand::parse` entiende.
    #[test]
    fn el_menu_solo_usa_flags_de_la_cli() {
        for entry in MENU_ENTRIES {
            for word in entry.cmd_template.split_whitespace() {
                if word.starts_with("--") {
                    assert!(
                        crate::core::cli::RELAUNCH_FLAGS.contains(&word),
                        "{word} no está en core::cli"
                    );
                }
            }
        }
    }
}
