//! `C:\ProgramData\OtterSwiss`: la carpeta de datos de la app, solo para Administradores y SYSTEM.

use std::path::PathBuf;

use windows::Win32::UI::Shell::FOLDERID_ProgramData;

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::folders::known_folder;
use crate::core::security::acl::{
    apply_admin_only_acl, create_admin_only_dir, verify_tree_owned_by_admins,
};

/// `C:\ProgramData` por `SHGetKnownFolderPath` (no por `%ProgramData%`, que un proceso sin elevar puede cambiar).
pub fn get_program_data_dir() -> CoreResult<PathBuf> {
    known_folder(&FOLDERID_ProgramData).ok_or_else(|| {
        CoreError::trust_msg("No se pudo resolver la carpeta ProgramData (FOLDERID_ProgramData)")
    })
}

/// Único punto de acceso a `C:\ProgramData\OtterSwiss`.
///
/// `C:\ProgramData` deja a cualquier usuario crear carpetas y archivos (de los que queda como dueño), así que
/// la carpeta se crea ya con un descriptor solo-admin, se reinician dueño y DACL si existía, y se rechaza
/// cualquier reparse point o hijo cuyo dueño no sea Administradores o SYSTEM: esos archivos pudo plantarlos
/// un usuario sin privilegios para que OtterSwiss los lea o ejecute como administrador.
pub fn ensure_admin_only_data_dir() -> CoreResult<PathBuf> {
    let base = get_program_data_dir()?;
    let target = base.join("OtterSwiss");

    match std::fs::symlink_metadata(&target) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(CoreError::trust_msg(format!(
                    "Ruta crítica '{}' es un symlink o archivo no válido (posible squatting)",
                    target.display()
                )));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_admin_only_dir(&target)?,
        Err(e) => return Err(CoreError::Io(e)),
    }

    apply_admin_only_acl(&target)?;
    verify_tree_owned_by_admins(&target, 8)?;
    Ok(target)
}

/// AR-07: carpeta donde se descargan los instaladores antes de ejecutarlos (dentro de la carpeta solo-admin).
pub fn staging_dir() -> CoreResult<PathBuf> {
    let dir = ensure_admin_only_data_dir()?.join("staging");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// AR-07: carpeta donde se exportan las claves de registro antes de borrarlas.
pub fn reg_backups_dir() -> CoreResult<PathBuf> {
    let dir = ensure_admin_only_data_dir()?.join("RegBackups");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
