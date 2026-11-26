//! Rutas seguras: nombres reservados, componentes peligrosos, rutas dentro de un archivo y destino libre.

use std::path::{Component, Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{canonical_dos_path, is_safe_non_root_dir};

pub(super) const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5",
    "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9",
];

/// Profundidad máxima de rutas dentro de un archivo comprimido. La limpieza usa el mismo límite, así que
/// nunca queda un staging a medio borrar (ERR-08).
pub(super) const MAX_ARCHIVE_DEPTH: usize = 48;

pub fn is_reserved_windows_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim();
    RESERVED_NAMES.iter().any(|&r| stem.eq_ignore_ascii_case(r))
}

pub fn validate_path_components(p: &Path) -> CoreResult<()> {
    for comp in p.components() {
        match comp {
            Component::Normal(os_name) => {
                let s = os_name.to_string_lossy();
                if s.contains('\0') || s.ends_with(' ') || s.ends_with('.') {
                    return Err(CoreError::Validation(format!(
                        "Nombre de archivo inválido para Windows: '{}'",
                        s
                    )));
                }
                if is_reserved_windows_name(&s) {
                    return Err(CoreError::Validation(format!(
                        "Nombre reservado de Windows detectado: '{}'",
                        s
                    )));
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::Validation(format!(
                    "Ruta insegura o fuera de raíz detectada: '{}'",
                    p.display()
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn sanitize_archive_rel_path(raw: &str) -> CoreResult<PathBuf> {
    if raw.contains('\0') {
        return Err(CoreError::Validation("Ruta con byte NUL en archivo".into()));
    }
    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/') || normalized.starts_with("//") {
        return Err(CoreError::Validation(format!(
            "Ruta absoluta rechazada en archivo: '{}'",
            raw
        )));
    }

    let mut clean = PathBuf::new();
    for seg in normalized.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." || seg.contains(':') {
            return Err(CoreError::Validation(format!(
                "Traversal o prefijo de unidad rechazado: '{}'",
                raw
            )));
        }
        if is_reserved_windows_name(seg) || seg.ends_with(' ') || seg.ends_with('.') {
            return Err(CoreError::Validation(format!(
                "Componente inválido o reservado en archivo: '{}'",
                seg
            )));
        }
        clean.push(seg);
    }

    if clean.as_os_str().is_empty() {
        return Err(CoreError::Validation(
            "Entrada de archivo con ruta vacía".into(),
        ));
    }
    if clean.components().count() > MAX_ARCHIVE_DEPTH {
        return Err(CoreError::Validation(format!(
            "Ruta demasiado profunda en el archivo (más de {} niveles): '{}'",
            MAX_ARCHIVE_DEPTH, raw
        )));
    }
    validate_path_components(&clean)?;
    Ok(clean)
}

pub(super) fn validate_cli_paths(src: &Path, dst: &Path) -> CoreResult<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        return Err(CoreError::Validation("Origen symlink rechazado".into()));
    }
    let _ = canonical_dos_path(src)?;
    if dst.is_dir() && !is_safe_non_root_dir(dst, 1) {
        return Err(CoreError::Validation(
            "No se permite escribir directamente en raíz de unidad".into(),
        ));
    }
    Ok(())
}

/// Destino real de una extracción: si `dest` ya existe, la carpeta hermana libre `dest (2)`, `dest (3)`…
/// Extraer nunca borra ni mezcla una carpeta existente (SYS-01).
pub fn resolve_extract_destination(dest: &Path) -> PathBuf {
    if !dest.exists() {
        return dest.to_path_buf();
    }
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "extracted".into());
    let parent = dest.parent().unwrap_or(dest);
    for i in 2..1000 {
        let candidate = parent.join(format!("{} ({})", name, i));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(
        "{} ({})",
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::test_util::TempDir;

    #[test]
    fn saneo_rechaza_traversal_unidades_y_ads() {
        for bad in [
            "../x",
            "a/../../x",
            "C:/Windows/x",
            "/abs",
            "\\\\srv\\share\\x",
            "a/b:stream",
            "c:x",
        ] {
            assert!(sanitize_archive_rel_path(bad).is_err(), "aceptó '{}'", bad);
        }
    }

    #[test]
    fn saneo_rechaza_nombres_reservados_y_finales_invalidos() {
        for bad in ["CON", "dir/NUL.txt", "dir/file.", "dir/file ", "", "./"] {
            assert!(sanitize_archive_rel_path(bad).is_err(), "aceptó '{}'", bad);
        }
    }

    #[test]
    fn saneo_normaliza_rutas_validas() {
        let p = sanitize_archive_rel_path("dir\\sub/./file.txt").unwrap();
        assert_eq!(p, PathBuf::from("dir").join("sub").join("file.txt"));
    }

    #[test]
    fn saneo_limita_la_profundidad() {
        let ok = vec!["d"; MAX_ARCHIVE_DEPTH].join("/");
        let deep = vec!["d"; MAX_ARCHIVE_DEPTH + 1].join("/");
        assert!(sanitize_archive_rel_path(&ok).is_ok());
        assert!(sanitize_archive_rel_path(&deep).is_err());
    }

    #[test]
    fn componentes_de_ruta_rechazan_raiz_y_padre() {
        assert!(validate_path_components(Path::new("a/b.txt")).is_ok());
        assert!(validate_path_components(Path::new("../b.txt")).is_err());
        assert!(validate_path_components(Path::new("C:\\b.txt")).is_err());
    }

    /// SYS-01: extraer junto a una carpeta existente nunca la reutiliza ni la borra.
    #[test]
    fn destino_existente_usa_carpeta_hermana_y_conserva_el_centinela() {
        let root = TempDir::new("dest");
        let dest = root.join("proyecto");
        assert_eq!(resolve_extract_destination(&dest), dest);

        std::fs::create_dir_all(&dest).unwrap();
        let sentinel = dest.join("centinela.txt");
        std::fs::write(&sentinel, b"no borrar").unwrap();

        let resolved = resolve_extract_destination(&dest);
        assert_eq!(resolved, root.join("proyecto (2)"));
        assert!(sentinel.is_file());

        std::fs::create_dir_all(&resolved).unwrap();
        assert_eq!(
            resolve_extract_destination(&dest),
            root.join("proyecto (3)")
        );
    }
}
