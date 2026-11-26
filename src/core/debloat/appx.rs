//! Apps Appx instaladas y aprovisionadas, consultadas con PowerShell.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use super::registry_backup::debloat_backup_path;
use super::{DEBLOAT_APP_CATALOG, DebloatScanResult, DebloaterManager, DetectedBloatApp};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::process::run_captured;

/// Tope de cada consulta de PowerShell del escaneo (antes no tenía, AR-10).
const SCAN_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Deserialize)]
struct PsAppxEntry {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "PackageFullName")]
    package_full_name: Option<String>,
    #[serde(rename = "DisplayName")]
    display_name: Option<String>,
    #[serde(rename = "PackageName")]
    package_name: Option<String>,
}

impl DebloaterManager {
    pub fn scan_system() -> CoreResult<DebloatScanResult> {
        // ERR-05: sin elevación `Get-AppxPackage -AllUsers` falla con acceso denegado y, con
        // `-ErrorAction SilentlyContinue`, el escaneo diría «nada instalado».
        ElevationManager::require_elevated("Escanear las apps instaladas para todos los usuarios")?;
        let ps_path = get_powershell_path()?;

        let ps_installed = run_ps(
            &ps_path,
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $p = @(Get-AppxPackage -AllUsers -ErrorAction SilentlyContinue | Select-Object Name, PackageFullName); if ($p.Count -eq 0) { '[]' } else { ConvertTo-Json -InputObject $p -Compress }",
            SCAN_TIMEOUT_MS,
        )?;

        let mut full_map: HashMap<String, Vec<String>> = HashMap::new();
        {
            let entries = parse_ps_json(&ps_installed, "Get-AppxPackage")?;
            {
                for e in entries {
                    if let (Some(n), Some(full)) = (e.name, e.package_full_name)
                        && is_valid_package_ident(&full)
                    {
                        let vec = full_map.entry(n.to_ascii_lowercase()).or_default();
                        if !vec.contains(&full) {
                            vec.push(full);
                        }
                    }
                }
            }
        }

        let ps_prov = run_ps(
            &ps_path,
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $p = @(Get-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue | Select-Object DisplayName, PackageName); if ($p.Count -eq 0) { '[]' } else { ConvertTo-Json -InputObject $p -Compress }",
            SCAN_TIMEOUT_MS,
        )?;

        let mut prov_map: HashMap<String, Vec<String>> = HashMap::new();
        {
            let entries = parse_ps_json(&ps_prov, "Get-AppxProvisionedPackage")?;
            {
                for e in entries {
                    if let (Some(dn), Some(pn)) = (e.display_name, e.package_name)
                        && is_valid_package_ident(&pn)
                    {
                        let vec = prov_map.entry(dn.to_ascii_lowercase()).or_default();
                        if !vec.contains(&pn) {
                            vec.push(pn);
                        }
                    }
                }
            }
        }

        let apps = DEBLOAT_APP_CATALOG
            .iter()
            .map(|spec| {
                let key = spec.package_name.to_ascii_lowercase();
                let full_names = full_map.get(&key).cloned().unwrap_or_default();
                let provisioned_names = prov_map.get(&key).cloned().unwrap_or_default();
                let installed = !full_names.is_empty() || !provisioned_names.is_empty();
                DetectedBloatApp {
                    id: spec.id,
                    package_name: spec.package_name,
                    display_name: spec.display_name,
                    description: spec.description,
                    category: spec.category,
                    default_selected: spec.default_selected,
                    installed,
                    full_names,
                    provisioned_names,
                }
            })
            .collect();

        let has_reg_backup = debloat_backup_path().map(|p| p.exists()).unwrap_or(false);

        Ok(DebloatScanResult {
            apps,
            has_reg_backup,
        })
    }
}

pub(super) fn get_powershell_path() -> CoreResult<PathBuf> {
    let mut buf = [0u16; 260];
    let len =
        // SAFETY: el búfer es nuestro y la API escribe como mucho su longitud.
        unsafe { windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut buf)) }
            as usize;
    if len == 0 || len >= buf.len() {
        return Err(CoreError::SystemTool {
            message: "No se pudo localizar System32 para PowerShell".into(),
            source: None,
        });
    }
    let sys_dir = String::from_utf16_lossy(&buf[..len]);
    let ps = Path::new(&sys_dir).join(r"WindowsPowerShell\v1.0\powershell.exe");
    if !ps.is_file() {
        return Err(CoreError::SystemTool {
            message: format!("No se encontró powershell.exe en '{}'", ps.display()),
            source: None,
        });
    }
    Ok(ps)
}

pub(super) fn is_valid_package_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 220
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~'))
}

/// Ejecuta un script de PowerShell y devuelve su salida estándar. Las salidas se leen en hilos mientras el
/// proceso corre: si solo se leyeran al final, un error largo llenaría el pipe y PowerShell se quedaría
/// bloqueado hasta el timeout (ERR-01).
pub(super) fn run_ps(ps_path: &Path, script: &str, timeout_ms: u64) -> CoreResult<String> {
    let out = run_captured(
        ps_path,
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
        Duration::from_millis(timeout_ms),
    )?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(CoreError::SystemTool {
            message: if out.stderr.is_empty() {
                format!("PowerShell finalizó con código {:?}", out.code)
            } else {
                out.stderr
            },
            source: None,
        })
    }
}

/// `ConvertTo-Json` devuelve un objeto suelto (no una lista) cuando hay un solo elemento.
fn parse_ps_json(txt: &str, what: &str) -> CoreResult<Vec<PsAppxEntry>> {
    let t = txt.trim();
    let parsed = if t.starts_with('{') {
        serde_json::from_str::<PsAppxEntry>(t).map(|e| vec![e])
    } else {
        serde_json::from_str::<Vec<PsAppxEntry>>(t)
    };
    parsed.map_err(|e| CoreError::SystemTool {
        message: format!("No se pudo interpretar la salida de {}: {}", what, e),
        source: Some(Box::new(e)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identificadores_de_paquete_sin_inyeccion() {
        assert!(is_valid_package_ident(
            "Microsoft.BingNews_4.55.62231.0_x64__8wekyb3d8bbwe"
        ));
        assert!(!is_valid_package_ident("x'; Remove-Item C:\\ -Recurse; '"));
        assert!(!is_valid_package_ident(""));
        assert!(!is_valid_package_ident(&"a".repeat(300)));
    }
}
