//! Borrado de los restos elegidos, con las claves de registro exportadas antes a `RegBackups`.

use std::path::{Path, PathBuf};

use winreg::enums::{KEY_ALL_ACCESS, KEY_READ, KEY_WRITE};

use windows::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_RoamingAppData};

use super::leftovers::is_protected_name;
use super::{LeftoverItem, LeftoverKind, PurgeReport, UninstallerManager};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::folders::known_folder;
use crate::core::platform::folders::{program_files_dir, program_files_x86_dir};
use crate::core::platform::process::run_captured;
use crate::core::platform::process::system32_dir;
use crate::core::platform::registry::RegLocation;
use crate::core::platform::safe_fs::{
    canonical_dos_path, delete_tree_verified, delete_verified_file_under, is_safe_non_root_dir,
    path_is_strictly_under,
};
use crate::core::security::data_dir::get_program_data_dir;
use crate::core::security::data_dir::reg_backups_dir;

/// Exporta `subkey` con `reg.exe export` a `ProgramData\OtterSwiss\RegBackups`, para poder restaurarla
/// con doble clic si se borró algo que no se debía.
pub(super) fn export_registry_key(loc: RegLocation, subkey: &str) -> CoreResult<PathBuf> {
    let dir = reg_backups_dir()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let leaf: String = subkey
        .rsplit('\\')
        .next()
        .unwrap_or("clave")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let file = dir.join(format!("{}_{}.reg", stamp, leaf));
    let full_key = format!("{}\\{}", loc.hive.as_str(), subkey);

    let mut args = vec![
        "export".to_string(),
        full_key.clone(),
        file.to_string_lossy().into_owned(),
        "/y".to_string(),
    ];
    if let Some(view) = loc.reg_exe_view_arg() {
        args.push(view.to_string());
    }
    let out = run_captured(
        &system32_dir()?.join("reg.exe"),
        &args,
        std::time::Duration::from_secs(60),
    )?;
    if !out.success || !file.is_file() {
        return Err(CoreError::Uninstaller {
            message: format!(
                "No se pudo respaldar '{}' antes de borrarla; no se borró: {}",
                full_key, out.stderr
            ),
            source: None,
        });
    }
    Ok(file)
}

pub(super) fn is_safe_registry_subkey_to_delete(subkey: &str) -> bool {
    let low = subkey.trim().to_ascii_lowercase();
    if !low.starts_with("software\\") {
        return false;
    }
    let rest = &low["software\\".len()..];
    !(rest.is_empty()
        || rest == "microsoft"
        || rest == "classes"
        || rest == "policies"
        || rest == "wow6432node"
        || rest == r"microsoft\windows\currentversion\uninstall"
        || (rest.starts_with("microsoft\\windows\\")
            && !rest.starts_with(r"microsoft\windows\currentversion\uninstall\")))
}

impl UninstallerManager {
    pub fn purge_leftover_items(items: &[LeftoverItem]) -> PurgeReport {
        let mut report = PurgeReport::default();

        for item in items {
            match item.kind {
                LeftoverKind::Directory => {
                    let Some(ref target) = item.path else {
                        continue;
                    };
                    if !target.exists() {
                        continue;
                    }
                    match Self::purge_leftovers_verified_dir(target) {
                        Ok(()) => {
                            report.removed_count += 1;
                            report.bytes_freed = report.bytes_freed.saturating_add(item.size_bytes);
                        }
                        Err(e) => {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", target.display(), e));
                        }
                    }
                }
                LeftoverKind::File => {
                    let Some(ref target) = item.path else {
                        continue;
                    };
                    if !target.exists() {
                        continue;
                    }
                    let res = target
                        .parent()
                        .ok_or_else(|| CoreError::Validation("Archivo sin padre".into()))
                        .and_then(canonical_dos_path)
                        .and_then(|canon| delete_verified_file_under(target, Some(&canon)));
                    match res {
                        Ok(()) => {
                            report.removed_count += 1;
                            report.bytes_freed = report.bytes_freed.saturating_add(item.size_bytes);
                        }
                        Err(e) => {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", target.display(), e));
                        }
                    }
                }
                LeftoverKind::RegistryKey => {
                    let Some(ref subkey) = item.reg_subkey else {
                        continue;
                    };
                    if !is_safe_registry_subkey_to_delete(subkey) {
                        report.failed_count += 1;
                        report
                            .errors
                            .push(format!("Clave protegida omitida: {}", subkey));
                        continue;
                    }
                    match Self::delete_registry_key_with_backup(item, subkey) {
                        Ok(true) => report.removed_count += 1,
                        Ok(false) => {}
                        Err(e) => {
                            report.failed_count += 1;
                            report
                                .errors
                                .push(format!("{}: {}", item.target_display, e));
                        }
                    }
                }
                LeftoverKind::RegistryValue => {
                    let (Some(subkey), Some(val_name)) = (&item.reg_subkey, &item.reg_value_name)
                    else {
                        continue;
                    };
                    match item
                        .reg_location
                        .root()
                        .open_subkey_with_flags(subkey, item.reg_location.flags(KEY_WRITE))
                        .and_then(|k| k.delete_value(val_name))
                    {
                        Ok(()) => report.removed_count += 1,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            report.failed_count += 1;
                            report
                                .errors
                                .push(format!("{}: {}", item.target_display, e));
                        }
                    }
                }
            }
        }

        report
    }

    /// Exporta la clave a un `.reg` en la carpeta de datos y después la borra. Si la exportación falla, no
    /// se borra nada. Devuelve `Ok(false)` si la clave ya no existía.
    pub(super) fn delete_registry_key_with_backup(
        item: &LeftoverItem,
        subkey: &str,
    ) -> CoreResult<bool> {
        let loc = item.reg_location;
        let root = loc.root();
        let (parent_path, leaf) = subkey
            .rsplit_once('\\')
            .ok_or_else(|| CoreError::Validation(format!("Clave sin padre: {}", subkey)))?;
        // RegDeleteTree necesita DELETE, KEY_ENUMERATE_SUB_KEYS y KEY_QUERY_VALUE sobre el padre (SYS-21).
        let parent = match root.open_subkey_with_flags(parent_path, loc.flags(KEY_ALL_ACCESS)) {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        if parent
            .open_subkey_with_flags(leaf, loc.flags(KEY_READ))
            .is_err()
        {
            return Ok(false);
        }

        export_registry_key(loc, subkey)?;
        parent.delete_subkey_all(leaf)?;
        Ok(true)
    }

    /// Borra un árbol de restos solo si su ruta **canónica** está dentro de una raíz conocida (obtenida con
    /// `SHGetKnownFolderPath`, nunca de variables de entorno) y ningún componente es una junction o symlink.
    pub(super) fn purge_leftovers_verified_dir(target: &Path) -> CoreResult<()> {
        let mut roots: Vec<PathBuf> = Vec::new();
        for r in [
            program_files_dir(),
            program_files_x86_dir(),
            get_program_data_dir().ok(),
            known_folder(&FOLDERID_RoamingAppData),
            known_folder(&FOLDERID_LocalAppData),
            known_folder(&FOLDERID_Desktop),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(c) = canonical_dos_path(&r) {
                roots.push(c);
            }
        }

        let target_canon = canonical_dos_path(target)?;
        // Si la ruta canónica difiere de la pedida, algún componente es una junction o symlink.
        let lexical = target
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        let canon = target_canon
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        if lexical != canon {
            return Err(CoreError::Validation(format!(
                "La ruta pasa por un enlace o junction ('{}' → '{}'); no se borra",
                target.display(),
                target_canon.display()
            )));
        }

        let Some(root) = roots
            .iter()
            .filter(|r| path_is_strictly_under(&target_canon, r))
            .max_by_key(|r| r.as_os_str().len())
        else {
            return Err(CoreError::Validation(format!(
                "Ruta fuera de raíces permitidas para purga: '{}'",
                target.display()
            )));
        };

        let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_protected_name(name) || !is_safe_non_root_dir(&target_canon, 2) {
            return Err(CoreError::Validation(format!(
                "Directorio protegido contra purga: '{}'",
                target.display()
            )));
        }

        delete_tree_verified(&target_canon, root, 12)?;
        Ok(())
    }

    pub fn purge_leftovers(target: &Path) -> CoreResult<()> {
        Self::purge_leftovers_verified_dir(target)
    }
}
