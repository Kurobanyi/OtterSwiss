//! Carpeta temporal solo para administradores donde se extrae antes de mover al destino, y Mark-of-the-Web.

use std::path::{Path, PathBuf};

use super::paths::MAX_ARCHIVE_DEPTH;
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{delete_tree_verified, write_zone_identifier_raw};
use crate::core::security::acl::{create_admin_only_dir, reset_to_inherited_acl};

/// Crea el staging **solo para administradores**: durante la extracción (que corre elevada) ningún proceso
/// del usuario puede cambiar una subcarpeta por una junction para redirigir escrituras (SEC-08).
pub(super) fn create_staging_dir(dest_dir: &Path) -> CoreResult<(&Path, PathBuf)> {
    let parent = dest_dir
        .parent()
        .ok_or_else(|| CoreError::Validation("Directorio destino sin padre".into()))?;
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_dir = parent.join(format!(".extract.{}.{}.tmp", pid, nonce));
    create_admin_only_dir(&tmp_dir)?;
    Ok((parent, tmp_dir))
}

/// Borra el staging tras un error o una cancelación, con el mismo límite de profundidad que la extracción.
pub(super) fn discard_staging(tmp_dir: &Path, parent: &Path) {
    let _ = delete_tree_verified(tmp_dir, parent, MAX_ARCHIVE_DEPTH + 2);
}

/// Mueve el staging a `dest_dir` (que no debe existir), le devuelve los permisos heredados de la carpeta
/// destino y propaga el Mark-of-the-Web del archivo original a cada archivo extraído (SEC-15).
pub(super) fn finalize_staging_dir(
    tmp_dir: &Path,
    dest_dir: &Path,
    parent: &Path,
    zone_identifier: Option<&[u8]>,
) -> CoreResult<()> {
    let res = (|| -> CoreResult<()> {
        if let Some(zone) = zone_identifier {
            mark_tree_with_zone(tmp_dir, zone)?;
        }
        if dest_dir.exists() {
            return Err(CoreError::Validation(format!(
                "El destino '{}' ya existe; no se sobrescribe",
                dest_dir.display()
            )));
        }
        std::fs::rename(tmp_dir, dest_dir)?;
        Ok(())
    })();
    if let Err(e) = res {
        discard_staging(tmp_dir, parent);
        return Err(e);
    }
    reset_to_inherited_acl(dest_dir)?;
    Ok(())
}

/// `Zone.Identifier` del archivo comprimido, si lo tiene (descargado de internet).
pub(super) fn read_zone_identifier(archive_path: &Path) -> Option<Vec<u8>> {
    let mut ads = archive_path.as_os_str().to_os_string();
    ads.push(":Zone.Identifier");
    let data = std::fs::read(PathBuf::from(ads)).ok()?;
    if data.is_empty() || data.len() > 8192 {
        None
    } else {
        Some(data)
    }
}

pub(super) fn mark_tree_with_zone(root: &Path, zone: &[u8]) -> CoreResult<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                write_zone_identifier_raw(&entry.path(), zone)?;
            }
        }
    }
    Ok(())
}
