//! Historial de paquetes de drivers que instaló OtterSwiss, para poder quitarlos (SYS-26).

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{DriverInstallReport, DriverManager, PNPUTIL_TIMEOUT};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::folders::windows_dir;
use crate::core::platform::json_file::{read_backup, write_backup};
use crate::core::platform::process::{run_captured, system32_dir};
use crate::core::security::data_dir::ensure_admin_only_data_dir;

/// Paquete de driver (`oemNN.inf`) agregado al Driver Store por una instalación hecha desde OtterSwiss (SYS-26).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstalledDriverRecord {
    pub published_name: String,
    pub title: String,
    pub installed_at_unix: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DriverHistory {
    entries: Vec<InstalledDriverRecord>,
}

const DRIVER_HISTORY_FILE: &str = "driver_installs.json";
const MAX_DRIVER_HISTORY_BYTES: u64 = 1024 * 1024;

fn driver_history_path() -> CoreResult<PathBuf> {
    Ok(ensure_admin_only_data_dir()?.join(DRIVER_HISTORY_FILE))
}

fn read_driver_history() -> CoreResult<DriverHistory> {
    let doc: Option<DriverHistory> = read_backup(
        &driver_history_path()?,
        MAX_DRIVER_HISTORY_BYTES,
        "no se pueden revertir drivers instalados",
    )?;
    let mut doc = doc.unwrap_or_default();
    // Patrón 4: solo nombres con la forma de un paquete OEM, nunca rutas.
    doc.entries.retain(|e| is_oem_inf_name(&e.published_name));
    Ok(doc)
}

/// `oem<N>.inf`: nombre que Windows da a cada paquete de terceros en el Driver Store.
fn is_oem_inf_name(name: &str) -> bool {
    let low = name.to_ascii_lowercase();
    low.strip_prefix("oem")
        .and_then(|r| r.strip_suffix(".inf"))
        .is_some_and(|n| !n.is_empty() && n.len() <= 5 && n.chars().all(|c| c.is_ascii_digit()))
}

fn oem_inf_path(name: &str) -> Option<PathBuf> {
    is_oem_inf_name(name).then(|| windows_dir().map(|w| w.join("INF").join(name)))?
}

/// Paquetes OEM presentes ahora (`%WINDIR%\INF\oem*.inf`), en minúsculas.
fn oem_inf_names() -> HashSet<String> {
    let Some(dir) = windows_dir().map(|w| w.join("INF")) else {
        return HashSet::new();
    };
    std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
                .filter(|n| is_oem_inf_name(n))
                .collect()
        })
        .unwrap_or_default()
}

/// Corre una instalación y guarda en el historial los `oemNN.inf` que aparecieron. Si el historial no se puede
/// escribir, la instalación sigue siendo válida y el aviso lo dice.
pub(super) fn with_driver_history(
    title: &str,
    install: impl FnOnce() -> CoreResult<DriverInstallReport>,
) -> CoreResult<DriverInstallReport> {
    let before = oem_inf_names();
    let mut report = install()?;
    let added: Vec<String> = oem_inf_names().difference(&before).cloned().collect();
    if added.is_empty() {
        return Ok(report);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let saved = read_driver_history().and_then(|mut h| {
        let mut added = added.clone();
        added.sort();
        h.entries.extend(
            added
                .into_iter()
                .map(|published_name| InstalledDriverRecord {
                    published_name,
                    title: title.to_string(),
                    installed_at_unix: now,
                }),
        );
        write_backup(&driver_history_path()?, &h)
    });
    if let Err(e) = saved {
        report
            .message
            .push_str(&format!(" No se pudo registrar para revertir: {e}"));
    }
    Ok(report)
}

impl DriverManager {
    /// Paquetes de drivers que instaló OtterSwiss y todavía se pueden quitar, del más nuevo al más viejo.
    pub fn driver_install_history() -> CoreResult<Vec<InstalledDriverRecord>> {
        let mut entries = read_driver_history()?.entries;
        entries.retain(|e| oem_inf_path(&e.published_name).is_some_and(|p| p.exists()));
        entries.reverse();
        Ok(entries)
    }

    /// SYS-26: quita del Driver Store un paquete que instaló OtterSwiss (`pnputil /delete-driver … /uninstall`). Los
    /// dispositivos que lo usaban vuelven al mejor driver que quede. Solo acepta paquetes del historial propio.
    pub fn revert_installed_driver(published_name: &str) -> CoreResult<DriverInstallReport> {
        let mut history = read_driver_history()?;
        let Some(pos) = history
            .entries
            .iter()
            .position(|e| e.published_name.eq_ignore_ascii_case(published_name))
        else {
            return Err(CoreError::Validation(format!(
                "'{published_name}' no lo instaló OtterSwiss"
            )));
        };
        let record = history.entries[pos].clone();
        if !is_oem_inf_name(&record.published_name) {
            return Err(CoreError::Validation(
                "Nombre de paquete de driver inválido".into(),
            ));
        }
        let out = run_captured(
            &system32_dir()?.join("pnputil.exe"),
            &[
                "/delete-driver",
                record.published_name.as_str(),
                "/uninstall",
                "/force",
            ],
            PNPUTIL_TIMEOUT,
        )?;
        let code = out.code.unwrap_or(-1);
        if !(out.success || code == 3010) {
            return Err(CoreError::SystemTool {
                message: format!(
                    "pnputil no pudo quitar '{}' (código {code}): {}",
                    record.published_name,
                    out.stdout.trim()
                ),
                source: None,
            });
        }
        history.entries.remove(pos);
        write_backup(&driver_history_path()?, &history)?;
        Ok(DriverInstallReport::single(
            format!("Se quitó '{}' ({}).", record.title, record.published_name),
            code == 3010,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_paquetes_oem() {
        assert!(is_oem_inf_name("oem42.inf"));
        assert!(is_oem_inf_name("OEM7.INF"));
        assert!(!is_oem_inf_name("oem.inf"));
        assert!(!is_oem_inf_name("..\\oem1.inf"));
        assert!(!is_oem_inf_name("oem1.inf.bak"));
        assert!(!is_oem_inf_name("machine.inf"));
    }
}
