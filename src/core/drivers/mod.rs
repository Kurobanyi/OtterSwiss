//! Drivers: dispositivos presentes, búsqueda de actualizaciones (Windows Update, Catálogo de Microsoft, NVIDIA e
//! Intel), instalación verificada e historial para revertir. Este módulo tiene los tipos públicos, la fachada
//! [`DriverManager`] y lo que comparten los submódulos: la política del equipo (SYS-25) y las peticiones HTTPS.

mod catalog;
mod devices;
mod history;
mod hwid;
mod install;
mod vendor;
mod wua;

use std::collections::HashSet;

use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use crate::core::error::{CoreError, CoreResult};
use crate::core::security::download::validate_https_host;
use crate::core::security::http;

pub use devices::InstalledDriverDevice;
pub use history::InstalledDriverRecord;

use history::with_driver_history;

const DATA_TIMEOUTS: http::Timeouts = http::Timeouts {
    connect_ms: 15_000,
    io_ms: 30_000,
};
const PNPUTIL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct DriverUpdateCandidate {
    pub id: String,
    pub title: String,
    pub driver_class: String,
    pub driver_model: String,
    pub driver_manufacturer: String,
    pub driver_provider: String,
    pub hardware_id: String,
    pub size_mb: i32,
    pub is_downloaded: bool,
    pub is_firmware: bool,
    pub is_optional: bool,
}

#[derive(Debug, Clone)]
pub struct DriverInstallReport {
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub reboot_required: bool,
    pub message: String,
}

impl DriverInstallReport {
    /// ARQ-05: un paquete instalado; el reinicio viaja como dato (código 3010/1641), no dentro del texto.
    fn single(message: String, reboot_required: bool) -> Self {
        Self {
            succeeded_count: 1,
            failed_count: 0,
            reboot_required,
            message,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogDriverMatch {
    pub update_id: String,
    pub title: String,
    pub products: String,
    pub classification: String,
    pub last_updated: String,
    pub version: String,
    pub size_str: String,
    pub query_hw_id: String,
}

#[derive(Debug, Clone)]
pub struct VendorDriverUpdate {
    pub id: String,
    pub vendor: String,
    pub title: String,
    pub current_version: String,
    pub latest_version: String,
    pub release_date: String,
    pub download_url: String,
    pub size_label: String,
    pub expected_signer_cn: Option<String>,
    pub allowed_hosts: Vec<String>,
    pub is_zip_inf: bool,
    pub matched_hw_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct FullDriverScanResult {
    pub wua_updates: Vec<DriverUpdateCandidate>,
    pub wua_policy_blocked: bool,
    pub wua_note: Option<String>,
    pub vendor_updates: Vec<VendorDriverUpdate>,
    pub catalog_matches: Vec<CatalogDriverMatch>,
    pub notes: Vec<String>,
}

pub struct DriverManager;

impl DriverManager {
    /// Instala los drivers de Windows Update elegidos y registra los paquetes que agregó (SYS-26).
    pub fn install_driver_updates(selected_ids: &[String]) -> CoreResult<DriverInstallReport> {
        with_driver_history("Windows Update", || {
            Self::install_driver_updates_inner(selected_ids)
        })
    }

    /// Instala un driver del Catálogo de Microsoft y registra los paquetes que agregó (SYS-26).
    pub fn install_catalog_driver(update_id: &str, title: &str) -> CoreResult<DriverInstallReport> {
        with_driver_history(title, || Self::install_catalog_driver_inner(update_id))
    }

    /// Instala una actualización de fabricante y registra los paquetes que agregó (SYS-26).
    pub fn install_vendor_update(upd: &VendorDriverUpdate) -> CoreResult<DriverInstallReport> {
        with_driver_history(&upd.title, || Self::install_vendor_update_inner(upd))
    }

    pub fn run_full_scan(devices: &[InstalledDriverDevice]) -> FullDriverScanResult {
        let wua_policy_blocked = is_wu_driver_policy_blocked();
        if wua_policy_blocked {
            return FullDriverScanResult {
                wua_policy_blocked,
                ..Default::default()
            };
        }

        let (wua_updates, wua_note) = match Self::search_driver_updates() {
            Ok(wua) => (wua, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };

        // ERR-20: lo que no se pudo consultar se informa en `notes`, no se confunde con «sin actualizaciones».
        let mut notes = Vec::new();
        let mut vendor_updates = Vec::new();
        for (source, result) in [
            ("NVIDIA", Self::check_nvidia_updates(devices)),
            ("Intel", Self::check_intel_updates(devices)),
        ] {
            match result {
                Ok(found) => vendor_updates.extend(found),
                Err(e) => notes.push(format!("{source}: {e}")),
            }
        }

        let mut catalog_matches = Vec::new();
        let mut queried_hw_ids = HashSet::new();
        for dev in devices
            .iter()
            .filter(|d| d.has_problem && d.problem_code == 28)
        {
            let Some(short_id) = dev.short_hw_id() else {
                continue;
            };
            if !queried_hw_ids.insert(short_id.clone()) {
                continue;
            }
            match Self::search_microsoft_catalog(&short_id) {
                Ok(mut found) => {
                    if let Some(first) = found.drain(..).next() {
                        catalog_matches.push(first);
                    }
                }
                Err(e) => {
                    notes.push(format!("Catálogo de Microsoft: {e}"));
                    break;
                }
            }
            if catalog_matches.len() >= 10 {
                break;
            }
        }

        FullDriverScanResult {
            wua_updates,
            wua_policy_blocked,
            wua_note,
            vendor_updates,
            catalog_matches,
            notes,
        }
    }
}

/// SYS-25: `true` si el administrador del equipo decide los drivers: la política `ExcludeWUDriversInQualityUpdate` los
/// excluye de Windows Update o el equipo usa un servidor WSUS (`UseWUServer`). En ese caso OtterSwiss no busca ni instala
/// drivers por ningún camino (Windows Update, Catálogo ni fabricantes): saltarse esa decisión en un equipo gestionado
/// es justo lo que la política quiere impedir.
fn is_wu_driver_policy_blocked() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let read = |subkey: &str, value: &str| {
        hklm.open_subkey_with_flags(subkey, KEY_READ)
            .and_then(|k| k.get_value::<u32, _>(value))
            .ok()
    };
    read(
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate",
        "ExcludeWUDriversInQualityUpdate",
    ) == Some(1)
        || read(
            r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
            "UseWUServer",
        ) == Some(1)
}

/// Error común cuando la política del equipo bloquea los drivers (SYS-25).
fn ensure_driver_updates_allowed() -> CoreResult<()> {
    if is_wu_driver_policy_blocked() {
        return Err(CoreError::Validation(
            "La política del equipo (exclusión de drivers o WSUS) no permite instalar drivers desde OtterSwiss".into(),
        ));
    }
    Ok(())
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// AR-21 y ERR-19: GET de datos (JSON o HTML) con el cliente común: estado 200, host final en `hosts` y tope que falla
/// en vez de cortar en silencio. `agent` solo para servicios que exigen el de su cliente.
fn fetch_bytes(
    url: &str,
    hosts: &[&str],
    agent: Option<&str>,
    max_bytes: usize,
) -> CoreResult<Vec<u8>> {
    let resp = match agent {
        Some(a) => http::get_with_agent(url, a, DATA_TIMEOUTS)?,
        None => http::get(url, DATA_TIMEOUTS)?,
    };
    resp.require_ok()?;
    validate_https_host(&resp.final_url()?, hosts)?;
    resp.read_to_end_capped(max_bytes)
}
