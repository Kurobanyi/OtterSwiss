//! HE-14: freno a las regresiones de una fusión. Cuenta en `src/` los indicadores que la fusión de v4.2.0 hizo subir
//! (`style:` 50 → 95, `if en` 206 → 249, `source: None` 96 → 142) y falla si alguno pasa de su techo. Cuando un cambio
//! baje uno, se baja aquí su techo en el mismo commit, para que no vuelva a subir.

use std::fs;
use std::path::{Path, PathBuf};

/// Techos medidos el 25/11/2026 en la rama `auditoria/buenas-practicas`.
const INDICADORES: &[(&str, usize, &str)] = &[
    (
        "style:",
        47,
        "estilos inline: usar clases de assets/main.css (TE-02, TE-25)",
    ),
    (
        "if en",
        217,
        "textos sueltos: usar el catálogo de ui/i18n (IN-02, IN-08)",
    ),
    (
        "source: None",
        118,
        "errores sin causa: conservar el error original (AR-06, CL-20)",
    ),
];

/// Ningún módulo se exime de documentar sus `unsafe` (CL-19): el `#[allow]` de drivers tapó 65 sin que nada avisara.
const EXENCION_PROHIBIDA: &str = "undocumented_unsafe_blocks";

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files
        .into_iter()
        .map(|p| {
            let text = fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect()
}

#[test]
fn indicadores_no_suben() {
    let files = sources();
    let mut excedidos = Vec::new();
    for (pattern, techo, regla) in INDICADORES {
        let total: usize = files.iter().map(|(_, t)| t.matches(pattern).count()).sum();
        if total > *techo {
            excedidos.push(format!("`{pattern}`: {total} (techo {techo}) — {regla}"));
        }
    }
    assert!(
        excedidos.is_empty(),
        "Subieron indicadores de calidad:\n{}",
        excedidos.join("\n")
    );
}

#[test]
fn ningun_modulo_se_exime_de_safety() {
    let exentos: Vec<String> = sources()
        .into_iter()
        .filter(|(_, t)| t.contains(EXENCION_PROHIBIDA))
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(exentos.is_empty(), "Exención de SAFETY en: {exentos:?}");
}
