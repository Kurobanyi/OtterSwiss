use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileDispositionInfo, GetFileInformationByHandle, GetFinalPathNameByHandleW,
    SetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_NONE, FILE_SHARE_READ,
    OPEN_EXISTING, VOLUME_NAME_DOS,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::SafeHandle;
use crate::core::platform::wide::to_wide_null;

#[derive(Debug, Clone)]
pub struct VerifiedHandleInfo {
    pub is_directory: bool,
    pub is_reparse_point: bool,
    pub number_of_links: u32,
    pub file_size: u64,
    pub final_path: PathBuf,
}

pub fn inspect_handle(handle: HANDLE) -> CoreResult<VerifiedHandleInfo> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(handle, &mut info) }.map_err(|e| {
        CoreError::Io(io::Error::other(format!(
            "GetFileInformationByHandle falló: {}",
            e
        )))
    })?;

    let needed = unsafe { GetFinalPathNameByHandleW(handle, &mut [], VOLUME_NAME_DOS) };
    if needed == 0 {
        return Err(CoreError::Io(io::Error::last_os_error()));
    }
    let mut buf = vec![0u16; needed as usize];
    let written = unsafe { GetFinalPathNameByHandleW(handle, &mut buf, VOLUME_NAME_DOS) };
    if written == 0 || written as usize >= buf.len() {
        return Err(CoreError::Io(io::Error::last_os_error()));
    }
    let raw_final = String::from_utf16_lossy(&buf[..written as usize]);
    let normalized = raw_final.strip_prefix(r"\\?\").unwrap_or(&raw_final);

    let file_size = ((info.nFileSizeHigh as u64) << 32) | (info.nFileSizeLow as u64);

    Ok(VerifiedHandleInfo {
        is_directory: (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0) != 0,
        is_reparse_point: (info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0,
        number_of_links: info.nNumberOfLinks,
        file_size,
        final_path: PathBuf::from(normalized),
    })
}

/// Abre un archivo existente sin seguir symlinks/junctions y verifica sus atributos sobre el handle.
pub fn open_verified_file_read(
    path: &Path,
    reject_hardlinks: bool,
    max_size: Option<u64>,
) -> CoreResult<(File, VerifiedHandleInfo)> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;

    let handle = HANDLE(file.as_raw_handle() as _);
    let info = inspect_handle(handle)?;

    if info.is_reparse_point {
        return Err(CoreError::Validation(format!(
            "Reparse point/symlink rechazado en '{}'",
            path.display()
        )));
    }
    if info.is_directory {
        return Err(CoreError::Validation(format!(
            "Se esperaba archivo y se encontró directorio: '{}'",
            path.display()
        )));
    }
    if reject_hardlinks && info.number_of_links > 1 {
        return Err(CoreError::Validation(format!(
            "Archivo con múltiples enlaces duros ({}) rechazado: '{}'",
            info.number_of_links,
            path.display()
        )));
    }
    if let Some(limit) = max_size {
        if info.file_size > limit {
            return Err(CoreError::Validation(format!(
                "Archivo '{}' excede el límite de {} bytes ({} bytes)",
                path.display(),
                limit,
                info.file_size
            )));
        }
    }

    Ok((file, info))
}

pub fn read_bounded_bytes(path: &Path, max_size: u64) -> CoreResult<Vec<u8>> {
    let (mut file, info) = open_verified_file_read(path, false, Some(max_size))?;
    let mut buf = Vec::with_capacity(info.file_size as usize);
    (&mut file).take(max_size + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > max_size {
        return Err(CoreError::Validation(format!(
            "Lectura excedió límite máximo de {} bytes",
            max_size
        )));
    }
    Ok(buf)
}

/// Crea un archivo nuevo exclusivamente (`create_new(true)`), sin seguir reparse points,
/// verifica el handle y devuelve el `File` abierto.
pub fn create_verified_new_file(
    path: &Path,
    exclusive_share: bool,
) -> CoreResult<(File, VerifiedHandleInfo)> {
    let share = if exclusive_share {
        FILE_SHARE_NONE.0
    } else {
        FILE_SHARE_READ.0
    };
    let file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .share_mode(share)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;

    let handle = HANDLE(file.as_raw_handle() as _);
    let info = inspect_handle(handle)?;
    if info.is_reparse_point || info.is_directory || info.number_of_links > 1 {
        drop(file);
        let _ = delete_verified_file(path);
        return Err(CoreError::Validation(format!(
            "El archivo recién creado no superó la verificación de integridad: '{}'",
            path.display()
        )));
    }

    Ok((file, info))
}

/// Escribe atómicamente creando un temporal único con `create_new(true)`, haciendo `sync_all()`
/// y reemplazando `target` solo si no es un reparse point.
pub fn atomic_write_new(target: &Path, data: &[u8]) -> CoreResult<()> {
    let parent = target.parent().ok_or_else(|| {
        CoreError::Validation(format!(
            "Ruta destino sin directorio padre: '{}'",
            target.display()
        ))
    })?;

    if target.exists() {
        let meta = std::fs::symlink_metadata(target)?;
        if meta.file_type().is_symlink() || meta.is_dir() {
            return Err(CoreError::Validation(format!(
                "Destino inválido o symlink rechazado: '{}'",
                target.display()
            )));
        }
    }

    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let tmp_path = parent.join(format!(".{}.{}.{}.tmp", file_name, pid, nonce));

    let write_res = (|| -> CoreResult<()> {
        let (mut tmp_file, _) = create_verified_new_file(&tmp_path, true)?;
        tmp_file.write_all(data)?;
        tmp_file.sync_all()?;
        drop(tmp_file);
        std::fs::rename(&tmp_path, target)?;
        Ok(())
    })();

    if write_res.is_err() {
        let _ = delete_verified_file(&tmp_path);
    }
    write_res
}

/// Abre `path` sin seguir reparse points, con acceso `DELETE` pero **sin** `FILE_FLAG_DELETE_ON_CLOSE`:
/// el borrado solo se marca después de validar (ver `mark_handle_for_delete`).
fn open_for_delete(path: &Path, directory: bool) -> CoreResult<SafeHandle> {
    let wide = to_wide_null(path);
    let mut flags = FILE_FLAG_OPEN_REPARSE_POINT;
    if directory {
        flags |= FILE_FLAG_BACKUP_SEMANTICS;
    }
    let raw = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            (DELETE | FILE_READ_ATTRIBUTES).0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .map_err(|e| CoreError::Io(io::Error::other(e.to_string())))?;
    SafeHandle::new(raw).ok_or_else(|| CoreError::Io(io::Error::last_os_error()))
}

/// Marca para borrado el objeto abierto en `handle`. Se borra al cerrarse el handle.
fn mark_handle_for_delete(handle: &SafeHandle, path: &Path) -> CoreResult<()> {
    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: true.into(),
    };
    unsafe {
        SetFileInformationByHandle(
            handle.raw(),
            FileDispositionInfo,
            &disposition as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(|e| {
        CoreError::Io(io::Error::other(format!(
            "No se pudo marcar '{}' para borrado: {}",
            path.display(),
            e
        )))
    })
}

/// Comprueba que la ruta final del handle quede estrictamente dentro de `allowed_root`.
fn ensure_under_root(final_path: &Path, allowed_root: Option<&Path>) -> CoreResult<()> {
    if let Some(root) = allowed_root {
        let root_canon = canonical_dos_path(root)?;
        if !path_is_strictly_under(final_path, &root_canon) {
            return Err(CoreError::Validation(format!(
                "Ruta resuelta '{}' escapa de raíz permitida '{}'",
                final_path.display(),
                root_canon.display()
            )));
        }
    }
    Ok(())
}

/// `true` si `child` está dentro de `root` (por componentes, sin distinguir mayúsculas) y no es la propia raíz.
pub fn path_is_strictly_under(child: &Path, root: &Path) -> bool {
    let mut c = child.components();
    for r in root.components() {
        match c.next() {
            Some(cc) if cc.as_os_str().eq_ignore_ascii_case(r.as_os_str()) => {}
            _ => return false,
        }
    }
    c.next().is_some()
}

/// Elimina un archivo (o symlink sin seguirlo). Valida tipo y raíz sobre el handle y solo
/// después marca el borrado: si la validación falla, el archivo queda intacto.
pub fn delete_verified_file_under(path: &Path, allowed_root: Option<&Path>) -> CoreResult<()> {
    let handle = open_for_delete(path, false)?;
    let info = inspect_handle(handle.raw())?;

    if info.is_directory && !info.is_reparse_point {
        return Err(CoreError::Validation(format!(
            "Se intentó borrar un directorio como archivo: '{}'",
            path.display()
        )));
    }
    ensure_under_root(&info.final_path, allowed_root)?;

    mark_handle_for_delete(&handle, path)?;
    drop(handle);
    Ok(())
}

/// Elimina un archivo propio cuya raíz es su carpeta padre (archivos temporales o de datos de la app).
pub fn delete_verified_file(path: &Path) -> CoreResult<()> {
    let parent = path.parent().ok_or_else(|| {
        CoreError::Validation(format!("Ruta sin directorio padre: '{}'", path.display()))
    })?;
    delete_verified_file_under(path, Some(parent))
}

/// Elimina un directorio vacío (o junction sin entrar) verificando que esté bajo `allowed_root`.
pub fn delete_verified_empty_dir_under(path: &Path, allowed_root: Option<&Path>) -> CoreResult<()> {
    let handle = open_for_delete(path, true)?;
    let info = inspect_handle(handle.raw())?;

    if !info.is_directory {
        return Err(CoreError::Validation(format!(
            "No es un directorio: '{}'",
            path.display()
        )));
    }
    ensure_under_root(&info.final_path, allowed_root)?;

    mark_handle_for_delete(&handle, path)?;
    drop(handle);
    Ok(())
}

/// Recorrido iterativo (sin recursión ilimitada) para eliminar un subárbol sin seguir junctions/symlinks.
pub fn delete_tree_verified(
    target: &Path,
    allowed_root: &Path,
    max_depth: usize,
) -> CoreResult<()> {
    let root_canon = canonical_dos_path(allowed_root)?;
    let target_canon = canonical_dos_path(target)?;

    if !path_is_strictly_under(&target_canon, &root_canon) {
        return Err(CoreError::Validation(format!(
            "Objetivo '{}' no es subdirectorio válido de '{}'",
            target_canon.display(),
            root_canon.display()
        )));
    }

    let meta = std::fs::symlink_metadata(target)?;
    if meta.file_type().is_symlink() {
        return if meta.is_dir() {
            delete_verified_empty_dir_under(target, Some(&root_canon))
        } else {
            delete_verified_file_under(target, Some(&root_canon))
        };
    }

    let mut stack: Vec<(PathBuf, usize, bool)> = vec![(target.to_path_buf(), 0, false)];

    while let Some((current, depth, visited)) = stack.pop() {
        if depth > max_depth {
            return Err(CoreError::Validation(format!(
                "Profundidad máxima ({}) excedida en '{}'",
                max_depth,
                current.display()
            )));
        }

        if visited {
            delete_verified_empty_dir_under(&current, Some(&root_canon))?;
            continue;
        }

        stack.push((current.clone(), depth, true));

        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let child = entry.path();
            let ft = entry.file_type()?;
            if ft.is_symlink() {
                if ft.is_dir() {
                    delete_verified_empty_dir_under(&child, Some(&root_canon))?;
                } else {
                    delete_verified_file_under(&child, Some(&root_canon))?;
                }
            } else if ft.is_dir() {
                stack.push((child, depth + 1, false));
            } else {
                delete_verified_file_under(&child, Some(&root_canon))?;
            }
        }
    }

    Ok(())
}

pub fn canonical_dos_path(path: &Path) -> CoreResult<PathBuf> {
    let wide = to_wide_null(path);
    let raw = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(|e| CoreError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?;

    let handle = SafeHandle::new(raw).ok_or_else(|| CoreError::Io(io::Error::last_os_error()))?;
    let info = inspect_handle(handle.raw())?;
    Ok(info.final_path)
}

/// Comprueba que una ruta tenga suficientes componentes (rechaza raíces de unidad como `C:\` o `D:\`).
pub fn is_safe_non_root_dir(path: &Path, min_normal_components: usize) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let normal_count = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    normal_count >= min_normal_components
}

/// Escribe el flujo alternativo NTFS `:Zone.Identifier` (Mark-of-the-Web, ZoneId=3).
pub fn write_zone_identifier(target_file: &Path, source_url: &str) -> CoreResult<()> {
    let mut ads_os = target_file.as_os_str().to_os_string();
    ads_os.push(":Zone.Identifier");
    let ads_path = PathBuf::from(ads_os);

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&ads_path)?;

    let content = format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={}\r\n", source_url);
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

/// Escribe `file_name` dentro de `root/relative_dir` sin seguir junctions ni symlinks: la carpeta final debe
/// resolverse a sí misma (ningún componente redirigido) y quedar dentro de `root`. Para archivos que el
/// proceso elevado escribe en carpetas del usuario (preferencias, exportaciones), donde un proceso sin elevar
/// podría poner una junction hacia una carpeta del sistema (SEC-19, SEC-20).
pub fn write_file_inside(
    root: &Path,
    relative_dir: &str,
    file_name: &str,
    data: &[u8],
) -> CoreResult<PathBuf> {
    let dir = if relative_dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative_dir)
    };
    std::fs::create_dir_all(&dir)?;
    let root_canon = canonical_dos_path(root)?;
    let dir_canon = canonical_dos_path(&dir)?;
    let lexical = dir
        .to_string_lossy()
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    let canon = dir_canon
        .to_string_lossy()
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    let inside = relative_dir.is_empty() || path_is_strictly_under(&dir_canon, &root_canon);
    if lexical != canon || !inside {
        return Err(CoreError::Validation(format!(
            "La carpeta '{}' está redirigida por un enlace o junction; no se escribe",
            dir.display()
        )));
    }
    let target = dir.join(file_name);
    atomic_write_new(&target, data)?;
    Ok(target)
}

/// Escribe en `:Zone.Identifier` un contenido ya armado (el del archivo comprimido de origen).
pub fn write_zone_identifier_raw(target_file: &Path, content: &[u8]) -> CoreResult<()> {
    let mut ads_os = target_file.as_os_str().to_os_string();
    ads_os.push(":Zone.Identifier");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(PathBuf::from(ads_os))?;
    file.write_all(content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dentro_de_raiz_por_componentes() {
        assert!(path_is_strictly_under(
            Path::new(r"C:\Datos\App\x.txt"),
            Path::new(r"c:\datos")
        ));
        assert!(!path_is_strictly_under(
            Path::new(r"C:\DatosMal\x.txt"),
            Path::new(r"C:\Datos")
        ));
        assert!(!path_is_strictly_under(
            Path::new(r"C:\Datos"),
            Path::new(r"C:\Datos")
        ));
        assert!(!path_is_strictly_under(
            Path::new(r"D:\Datos\x"),
            Path::new(r"C:\Datos")
        ));
    }

    /// SEC-01: si la validación de raíz falla, el archivo queda intacto.
    #[test]
    fn borrado_fuera_de_raiz_no_borra_nada() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base =
            std::env::temp_dir().join(format!("otter_sec01_{}_{}", std::process::id(), nonce));
        let root = base.join("raiz");
        std::fs::create_dir_all(&root).unwrap();
        let inside = root.join("dentro.txt");
        let outside = base.join("fuera.txt");
        std::fs::write(&inside, b"x").unwrap();
        std::fs::write(&outside, b"x").unwrap();

        assert!(delete_verified_file_under(&outside, Some(&root)).is_err());
        assert!(
            outside.is_file(),
            "SEC-01: borró un archivo fuera de la raíz"
        );

        delete_verified_file_under(&inside, Some(&root)).unwrap();
        assert!(!inside.exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
