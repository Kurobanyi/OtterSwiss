use std::io;
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE};
use winreg::RegKey;

use crate::core::elevation::ElevationManager;
use crate::core::error::{CoreError, CoreResult};

pub struct ContextMenuManager;

struct MenuEntrySpec {
    subkey: &'static str,
    label: &'static str,
    cmd_template: &'static str,
}

const MENU_ENTRIES: &[MenuEntrySpec] = &[
    MenuEntrySpec {
        subkey: r"Software\Classes\Directory\Background\shell\OtterSwiss",
        label: "Abrir OtterSwiss",
        cmd_template: "\"{exe}\"",
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\Directory\shell\OtterSwissCompress",
        label: "Comprimir carpeta a .zip (OtterSwiss)",
        cmd_template: "\"{exe}\" --compress-zip \"%V\"",
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\*\shell\OtterSwissCompress",
        label: "Comprimir a .zip (OtterSwiss)",
        cmd_template: "\"{exe}\" --compress-zip \"%1\"",
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.zip\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.7z\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.tar\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.gz\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
    },
    MenuEntrySpec {
        subkey: r"Software\Classes\SystemFileAssociations\.tgz\shell\OtterSwissExtract",
        label: "Extraer aquí con OtterSwiss",
        cmd_template: "\"{exe}\" --extract \"%1\"",
    },
];

fn notify_shell_assoc_changed() {
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
                let current_exe = std::env::current_exe()?.to_string_lossy().to_string();
                let expected = format!("\"{}\"", current_exe);
                Ok(cmd.contains(&expected) || cmd.contains(&current_exe))
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
        let exe_str = current_exe.to_string_lossy().to_string();
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