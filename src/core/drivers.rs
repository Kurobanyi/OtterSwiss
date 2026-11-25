use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVNODE_STATUS_FLAGS, CM_Get_DevNode_Status, CM_PROB, CR_SUCCESS, DIGCF_ALLCLASSES,
    DIGCF_PRESENT, DN_HAS_PROBLEM, DN_NEED_RESTART, HDEVINFO, SETUP_DI_REGISTRY_PROPERTY,
    SP_DEVINFO_DATA, SPDRP_CLASS, SPDRP_DEVICEDESC, SPDRP_DRIVER, SPDRP_FRIENDLYNAME,
    SPDRP_HARDWAREID, SPDRP_MFG, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
};
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, VARIANT_BOOL};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::UpdateAgent::{
    IUpdate3, IUpdateCollection, IUpdateSession, IWindowsDriverUpdate, UpdateCollection,
    UpdateSession, orcFailed, orcSucceeded, orcSucceededWithErrors, ssDefault,
};
use windows::core::{BSTR, Interface, PCWSTR};
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use crate::core::archive::ArchiveManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::folders::windows_dir;
use crate::core::platform::json_file::{read_backup, write_backup};
use crate::core::platform::memory::AlignedBuf;
use crate::core::platform::process::{Launch, run_captured, run_in_job, system32_dir};
use crate::core::platform::safe_fs::{create_verified_new_file, delete_tree_verified};
use crate::core::security::acl::create_admin_only_dir;
use crate::core::security::data_dir::ensure_admin_only_data_dir;
use crate::core::security::download::{
    DownloadPolicy, StagedDownload, reopen_verified_for_exec, validate_https_host,
};
use crate::core::security::http;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct InstalledDriverDevice {
    pub device_desc: Option<String>,
    pub friendly_name: Option<String>,
    pub manufacturer: Option<String>,
    pub provider_name: Option<String>,
    pub class_name: Option<String>,
    pub hardware_id: Option<String>,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
    pub inf_path: Option<String>,
    pub dev_inst: u32,
    pub has_problem: bool,
    pub problem_code: u32,
    pub needs_reboot: bool,
}

impl InstalledDriverDevice {
    pub fn display_title(&self) -> &str {
        self.friendly_name
            .as_deref()
            .or(self.device_desc.as_deref())
            .unwrap_or("Dispositivo desconocido")
    }

    pub fn is_third_party_oem(&self) -> bool {
        self.inf_path
            .as_deref()
            .map(|p| p.to_ascii_lowercase().starts_with("oem"))
            .unwrap_or(false)
    }

    /// ID corto para buscar en el Catálogo: `PCI\VEN_x&DEV_y`, `USB\VID_x&PID_y` o los dos primeros campos.
    pub fn short_hw_id(&self) -> Option<String> {
        short_hw_id_of(self.hardware_id.as_deref()?)
    }

    /// `(fabricante, dispositivo, subsistema)` del Hardware ID (VEN/DEV o VID/PID).
    pub fn extract_ven_dev(&self) -> Option<(String, String, Option<String>)> {
        ven_dev_of(self.hardware_id.as_deref()?)
    }

    pub fn decoded_vendor_name(&self) -> Option<&'static str> {
        let (ven, _, _) = self.extract_ven_dev()?;
        match ven.as_str() {
            "8086" => Some("Intel"),
            "8087" => Some("Intel Wireless / BT"),
            "10DE" => Some("NVIDIA"),
            "1002" | "1022" => Some("AMD"),
            "10EC" => Some("Realtek"),
            "14C3" => Some("MediaTek"),
            "17CB" => Some("Qualcomm"),
            "1B21" => Some("ASMedia"),
            "144D" => Some("Samsung"),
            "1987" => Some("Phison"),
            "15B7" => Some("WD / SanDisk"),
            "C0A9" => Some("Micron / Crucial"),
            "2646" => Some("Kingston"),
            "1912" => Some("Renesas"),
            "1106" => Some("VIA"),
            "046D" => Some("Logitech"),
            "1532" => Some("Razer"),
            "1B4B" => Some("Marvell"),
            _ => None,
        }
    }

    pub fn hardware_hint(&self) -> Option<&'static str> {
        let (ven, dev, _) = self.extract_ven_dev()?;
        if ven == "8086" {
            match dev.as_str() {
                "7D1D" | "AD1D" | "643E" | "B03E" => return Some("Intel® NPU (AI Boost)"),
                "7E4C" | "A74F" | "464F" | "9A11" | "7D4C" => {
                    return Some("Intel® GNA Scoring Accelerator");
                }
                "A77D" | "7D60" | "4660" | "51EE" | "7A68" | "7E70" | "5468" => {
                    return Some("Intel® Management Engine / Serial IO");
                }
                "461D" | "A71D" | "7D03" | "9A03" | "641D" => {
                    return Some("Intel® Dynamic Tuning (DTT)");
                }
                "09AB" | "467F" | "7D0B" | "9A0B" => return Some("Intel® VMD / RST Storage"),
                _ => {}
            }
        }
        None
    }

    pub fn friendly_gpu_version(&self) -> Option<String> {
        let ver = self.driver_version.as_deref()?;
        let is_nvidia = self
            .provider_name
            .as_deref()
            .or(self.manufacturer.as_deref())
            .map(|s| s.to_ascii_lowercase().contains("nvidia"))
            .unwrap_or(false);

        if !is_nvidia {
            return None;
        }

        let digits: String = ver.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 5 {
            let tail = &digits[digits.len() - 5..];
            return Some(format!("{}.{}", &tail[..3], &tail[3..]));
        }
        None
    }
}

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

struct DevInfoGuard(HDEVINFO);
impl Drop for DevInfoGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = SetupDiDestroyDeviceInfoList(self.0);
            }
        }
    }
}

struct ComApartmentGuard {
    should_uninit: bool,
}
impl ComApartmentGuard {
    fn new_sta() -> CoreResult<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_ok() {
            Ok(Self {
                should_uninit: true,
            })
        } else if hr == RPC_E_CHANGED_MODE {
            Ok(Self {
                should_uninit: false,
            })
        } else {
            Err(CoreError::SystemTool {
                message: format!("CoInitializeEx(STA) falló: {:?}", hr),
                source: None,
            })
        }
    }
}
impl Drop for ComApartmentGuard {
    fn drop(&mut self) {
        if self.should_uninit {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

pub fn describe_cm_problem(code: u32, en: bool) -> &'static str {
    if en {
        match code {
            1 => "Device not configured correctly (Code 1)",
            10 => "Device cannot start (Code 10)",
            12 => "Not enough free resources (Code 12)",
            18 => "Reinstall drivers for this device (Code 18)",
            22 => "Device is disabled (Code 22)",
            28 => "Drivers are not installed (Code 28)",
            31 => "Device is not working properly (Code 31)",
            43 => "Windows stopped this device due to errors (Code 43)",
            45 => "Hardware device not connected (Code 45)",
            52 => "Cannot verify digital signature of drivers (Code 52)",
            _ => "Hardware / Driver issue",
        }
    } else {
        match code {
            1 => "Dispositivo mal configurado (Código 1)",
            10 => "El dispositivo no puede iniciarse (Código 10)",
            12 => "Recursos libres insuficientes (Código 12)",
            18 => "Reinstalar controladores (Código 18)",
            22 => "Dispositivo deshabilitado (Código 22)",
            28 => "Faltan los controladores (Código 28)",
            31 => "El dispositivo no funciona correctamente (Código 31)",
            43 => "Windows detuvo el dispositivo por errores (Código 43)",
            45 => "Dispositivo no conectado actualmente (Código 45)",
            52 => "Firma digital de controlador no verificada (Código 52)",
            _ => "Problema de dispositivo o controlador",
        }
    }
}

fn is_windows_update_service_disabled() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) =
        hklm.open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\wuauserv", KEY_READ)
        && let Ok(start_type) = key.get_value::<u32, _>("Start")
    {
        return start_type == 4;
    }
    false
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

fn get_system32_tool(name: &str) -> PathBuf {
    let mut buf = [0u16; 260];
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len > 0 && len < buf.len() {
        PathBuf::from(String::from_utf16_lossy(&buf[..len])).join(name)
    } else {
        PathBuf::from(r"C:\Windows\System32").join(name)
    }
}

fn get_device_property(
    h_dev_info: HDEVINFO,
    dev_data: &mut SP_DEVINFO_DATA,
    property: SETUP_DI_REGISTRY_PROPERTY,
) -> Option<String> {
    let mut required_size = 0u32;
    let _ = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            h_dev_info,
            dev_data,
            property,
            None,
            None,
            Some(&mut required_size),
        )
    };

    if required_size < 2 {
        return None;
    }

    let mut buf = AlignedBuf::zeroed(required_size as usize);
    let ok = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            h_dev_info,
            dev_data,
            property,
            None,
            Some(&mut buf[..]),
            Some(&mut required_size),
        )
    };

    if ok.is_ok() && required_size > 1 {
        let len_u16 = (required_size as usize).min(buf.len()) / 2;
        let u16_slice = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, len_u16) };
        let end = u16_slice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(u16_slice.len());
        let val = String::from_utf16_lossy(&u16_slice[..end])
            .trim()
            .to_string();
        if !val.is_empty() {
            return Some(val);
        }
    }

    None
}

fn read_driver_class_metadata(
    driver_key: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let path = format!(r"SYSTEM\CurrentControlSet\Control\Class\{}", driver_key);
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(subkey) = hklm.open_subkey_with_flags(&path, KEY_READ) else {
        return (None, None, None, None);
    };

    let read_opt = |name: &str| -> Option<String> {
        subkey
            .get_value::<String, _>(name)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    (
        read_opt("DriverVersion"),
        read_opt("DriverDate"),
        read_opt("ProviderName"),
        read_opt("InfPath"),
    )
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

const DATA_TIMEOUTS: http::Timeouts = http::Timeouts {
    connect_ms: 15_000,
    io_ms: 30_000,
};
const CATALOG_HOSTS: &[&str] = &["www.catalog.update.microsoft.com"];
const NVIDIA_API_HOSTS: &[&str] = &["gfwsl.geforce.com"];
const INTEL_DSA_HOSTS: &[&str] = &["dsadata.intel.com"];

fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// ERR-23: valor de `key` en un Hardware ID en mayúsculas (`PCI\VEN_1002&DEV_744C&SUBSYS_…`): lo que sigue a la clave
/// hasta `&` o `\`. Así `ACPI\VEN_INT&DEV_33A0` da `INT` y no `INT&`.
fn hwid_field<'a>(up: &'a str, key: &str) -> Option<&'a str> {
    let start = up.find(key)? + key.len();
    let rest = &up[start..];
    let end = rest.find(['&', '\\']).unwrap_or(rest.len());
    let value = &rest[..end];
    (!value.is_empty()).then_some(value)
}

fn short_hw_id_of(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let up = raw.to_ascii_uppercase();
    let bus = up
        .split('\\')
        .next()
        .filter(|_| up.contains('\\'))
        .map(|b| format!("{b}\\"));
    if let (Some(ven), Some(dev)) = (hwid_field(&up, "VEN_"), hwid_field(&up, "DEV_")) {
        let prefix = bus.as_deref().filter(|b| *b == "PCI\\").unwrap_or("");
        return Some(format!("{prefix}VEN_{ven}&DEV_{dev}"));
    }
    if let (Some(vid), Some(pid)) = (hwid_field(&up, "VID_"), hwid_field(&up, "PID_")) {
        let prefix = bus.as_deref().filter(|b| *b == "USB\\").unwrap_or("");
        return Some(format!("{prefix}VID_{vid}&PID_{pid}"));
    }
    Some(raw.split('&').take(2).collect::<Vec<_>>().join("&"))
}

fn ven_dev_of(raw: &str) -> Option<(String, String, Option<String>)> {
    let up = raw.to_ascii_uppercase();
    let ven = hwid_field(&up, "VEN_").or_else(|| hwid_field(&up, "VID_"))?;
    let dev = hwid_field(&up, "DEV_").or_else(|| hwid_field(&up, "PID_"))?;
    let subsys = hwid_field(&up, "SUBSYS_").map(str::to_string);
    Some((ven.to_string(), dev.to_string(), subsys))
}

fn parse_version_tuple(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

fn is_version_newer(candidate: &str, current: &str) -> bool {
    let a = parse_version_tuple(candidate);
    let b = parse_version_tuple(current);
    // ERR-23: si alguna de las dos no es numérica no se sabe cuál es más nueva; no se ofrece la actualización.
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let len = a.len().max(b.len());
    for i in 0..len {
        let va = *a.get(i).unwrap_or(&0);
        let vb = *b.get(i).unwrap_or(&0);
        if va > vb {
            return true;
        }
        if va < vb {
            return false;
        }
    }
    false
}

fn is_system_laptop() -> bool {
    let mut pwr = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut pwr).is_ok() } {
        return pwr.BatteryFlag != 255 && (pwr.BatteryFlag & 128) == 0;
    }
    false
}

/// Número de build de Windows, o `None` si no se puede leer (antes se inventaba `22631`, ERR-23).
fn read_windows_build() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ)
        .ok()?;
    let build: String = key.get_value("CurrentBuildNumber").ok()?;
    let build = build.trim().to_string();
    build.chars().all(|c| c.is_ascii_digit()).then_some(build)
}

#[derive(Debug, Deserialize)]
struct NvidiaLookupResp {
    #[serde(rename = "DriverAttributes")]
    driver_attributes: Option<NvidiaDriverAttrs>,
}

#[derive(Debug, Deserialize)]
struct NvidiaDriverAttrs {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
    #[serde(rename = "ReleaseDateTime")]
    release_date: Option<String>,
    #[serde(rename = "DownloadURLAdmin")]
    download_url_admin: Option<String>,
    #[serde(rename = "DownloadURL")]
    download_url: Option<String>,
    #[serde(rename = "DownloadURLFileSize")]
    file_size: Option<String>,
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

    pub fn list_present_devices() -> CoreResult<Vec<InstalledDriverDevice>> {
        let h_dev_info: HDEVINFO = unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES | DIGCF_PRESENT)
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("SetupDiGetClassDevsW falló: {}", e),
            source: None,
        })?;

        if h_dev_info.is_invalid() {
            return Err(CoreError::SystemTool {
                message: "Handle de dispositivo SetupAPI inválido".into(),
                source: None,
            });
        }

        let _guard = DevInfoGuard(h_dev_info);
        let mut devices = Vec::new();
        let mut dev_data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };

        let mut member_idx = 0u32;
        loop {
            let ok = unsafe { SetupDiEnumDeviceInfo(h_dev_info, member_idx, &mut dev_data) };
            if ok.is_err() {
                break;
            }
            member_idx += 1;

            let desc = get_device_property(h_dev_info, &mut dev_data, SPDRP_DEVICEDESC);
            let friendly = get_device_property(h_dev_info, &mut dev_data, SPDRP_FRIENDLYNAME);
            let mfg = get_device_property(h_dev_info, &mut dev_data, SPDRP_MFG);
            let hw_id = get_device_property(h_dev_info, &mut dev_data, SPDRP_HARDWAREID);
            let class_name = get_device_property(h_dev_info, &mut dev_data, SPDRP_CLASS);
            let driver_key = get_device_property(h_dev_info, &mut dev_data, SPDRP_DRIVER);

            if desc.is_none() && friendly.is_none() && hw_id.is_none() {
                continue;
            }

            let (driver_version, driver_date, provider_name, inf_path) = match driver_key {
                Some(ref key) => read_driver_class_metadata(key),
                None => (None, None, None, None),
            };

            let mut status = CM_DEVNODE_STATUS_FLAGS::default();
            let mut problem = CM_PROB::default();
            let cr =
                unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, dev_data.DevInst, 0) };

            let (has_problem, problem_code, needs_reboot) = if cr == CR_SUCCESS {
                let prob_flag = (status.0 & DN_HAS_PROBLEM.0) != 0;
                let reboot_flag = (status.0 & DN_NEED_RESTART.0) != 0;
                (
                    prob_flag,
                    if prob_flag { problem.0 } else { 0 },
                    reboot_flag,
                )
            } else {
                (false, 0, false)
            };

            devices.push(InstalledDriverDevice {
                device_desc: desc,
                friendly_name: friendly,
                manufacturer: mfg,
                provider_name,
                class_name,
                hardware_id: hw_id,
                driver_version,
                driver_date,
                inf_path,
                dev_inst: dev_data.DevInst,
                has_problem,
                problem_code,
                needs_reboot,
            });
        }

        devices.sort_by(|a, b| {
            b.has_problem
                .cmp(&a.has_problem)
                .then_with(|| b.is_third_party_oem().cmp(&a.is_third_party_oem()))
                .then_with(|| {
                    a.display_title()
                        .to_ascii_lowercase()
                        .cmp(&b.display_title().to_ascii_lowercase())
                })
        });

        Ok(devices)
    }

    pub fn export_driver_store_backup() -> CoreResult<PathBuf> {
        let base_dir = ensure_admin_only_data_dir()?;
        let backup_root = base_dir.join("DriverBackup");
        create_admin_only_dir(&backup_root)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup_dir = backup_root.join(format!("backup_{}", stamp));
        create_admin_only_dir(&backup_dir)?;

        let pnputil = get_system32_tool("pnputil.exe");

        let output = std::process::Command::new(&pnputil)
            .args(["/export-driver", "*", backup_dir.to_string_lossy().as_ref()])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| CoreError::SystemTool {
                message: format!("No se pudo ejecutar pnputil.exe: {}", e),
                source: None,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(CoreError::SystemTool {
                message: format!("pnputil falló al exportar drivers: {} {}", stderr, stdout),
                source: None,
            });
        }

        Ok(backup_dir)
    }

    pub fn search_driver_updates() -> CoreResult<Vec<DriverUpdateCandidate>> {
        if is_windows_update_service_disabled() {
            return Err(CoreError::SystemTool {
                message: "El servicio Windows Update (wuauserv) está deshabilitado.".into(),
                source: None,
            });
        }

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Self::search_driver_updates_on_this_thread());
        });
        // CON-03: Windows Update puede quedarse colgado (servicio ocupado, red). Pasado el plazo se deja de esperar;
        // la búsqueda solo lee, así que el hilo puede terminar solo después sin dejar nada a medias.
        match rx.recv_timeout(WUA_SEARCH_TIMEOUT) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(CoreError::SystemTool {
                message: format!(
                    "Windows Update no respondió en {} minutos; prueba de nuevo más tarde.",
                    WUA_SEARCH_TIMEOUT.as_secs() / 60
                ),
                source: None,
            }),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(CoreError::SystemTool {
                message: "Fallo inesperado en hilo COM de búsqueda de drivers".into(),
                source: None,
            }),
        }
    }

    /// Búsqueda de drivers en WUA en el hilo actual (que no tenga COM iniciado en otro modo).
    fn search_driver_updates_on_this_thread() -> CoreResult<Vec<DriverUpdateCandidate>> {
        {
            let _com = ComApartmentGuard::new_sta()?;

            let session: IUpdateSession =
                unsafe { CoCreateInstance(&UpdateSession, None, CLSCTX_INPROC_SERVER) }.map_err(
                    |e| CoreError::SystemTool {
                        message: format!("No se pudo iniciar Windows Update Agent (WUA): {}", e),
                        source: None,
                    },
                )?;

            let searcher =
                unsafe { session.CreateUpdateSearcher() }.map_err(|e| CoreError::SystemTool {
                    message: format!("CreateUpdateSearcher falló: {}", e),
                    source: None,
                })?;

            unsafe {
                let _ = searcher.SetOnline(VARIANT_BOOL(-1));
                // SYS-25: el servidor que configuró el administrador (WSUS incluido), nunca Windows Update forzado.
                let _ = searcher.SetServerSelection(ssDefault);
            }

            let criteria = BSTR::from("IsInstalled=0 and Type='Driver' and IsHidden=0");
            let result =
                unsafe { searcher.Search(&criteria) }.map_err(|e| CoreError::SystemTool {
                    message: format!("WUA Search falló: {}", e),
                    source: None,
                })?;

            let updates = unsafe { result.Updates() }.map_err(|e| CoreError::SystemTool {
                message: format!("Updates() falló: {}", e),
                source: None,
            })?;

            let count = unsafe { updates.Count() }.unwrap_or(0);
            let mut candidates = Vec::new();

            for idx in 0..count {
                let Ok(update) = (unsafe { updates.get_Item(idx) }) else {
                    continue;
                };

                let title = unsafe { update.Title() }
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                let is_downloaded = unsafe { update.IsDownloaded() }
                    .map(|b| b.0 != 0)
                    .unwrap_or(false);
                let is_optional = update
                    .cast::<IUpdate3>()
                    .and_then(|u3| unsafe { u3.BrowseOnly() })
                    .map(|b| b.0 != 0)
                    .unwrap_or(false);
                let size_mb = unsafe { update.RecommendedHardDiskSpace() }.unwrap_or(0);

                let update_id = unsafe { update.Identity() }
                    .and_then(|id| unsafe { id.UpdateID() })
                    .map(|b| b.to_string())
                    .unwrap_or_else(|_| format!("drv_{}", idx));

                let (driver_class, driver_model, driver_manufacturer, driver_provider, hardware_id) =
                    if let Ok(drv) = update.cast::<IWindowsDriverUpdate>() {
                        (
                            unsafe { drv.DriverClass() }
                                .map(|b| b.to_string())
                                .unwrap_or_default(),
                            unsafe { drv.DriverModel() }
                                .map(|b| b.to_string())
                                .unwrap_or_default(),
                            unsafe { drv.DriverManufacturer() }
                                .map(|b| b.to_string())
                                .unwrap_or_default(),
                            unsafe { drv.DriverProvider() }
                                .map(|b| b.to_string())
                                .unwrap_or_default(),
                            unsafe { drv.DriverHardwareID() }
                                .map(|b| b.to_string())
                                .unwrap_or_default(),
                        )
                    } else {
                        (
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                        )
                    };

                let low_title = title.to_ascii_lowercase();
                let low_class = driver_class.to_ascii_lowercase();
                let is_firmware = low_class.contains("firmware")
                    || low_title.contains("firmware")
                    || low_title.contains("uefi")
                    || low_title.contains("bios");

                candidates.push(DriverUpdateCandidate {
                    id: update_id,
                    title,
                    driver_class,
                    driver_model,
                    driver_manufacturer,
                    driver_provider,
                    hardware_id,
                    size_mb,
                    is_downloaded,
                    is_firmware,
                    is_optional,
                });
            }

            Ok(candidates)
        }
    }

    fn install_driver_updates_inner(selected_ids: &[String]) -> CoreResult<DriverInstallReport> {
        ensure_driver_updates_allowed()?;
        if selected_ids.is_empty() {
            return Err(CoreError::Validation(
                "No hay actualizaciones seleccionadas para instalar".into(),
            ));
        }

        if is_windows_update_service_disabled() {
            return Err(CoreError::SystemTool {
                message: "El servicio Windows Update (wuauserv) está deshabilitado.".into(),
                source: None,
            });
        }

        let target_ids = selected_ids.to_vec();
        let handle = std::thread::spawn(move || -> CoreResult<DriverInstallReport> {
            let _com = ComApartmentGuard::new_sta()?;

            let session: IUpdateSession =
                unsafe { CoCreateInstance(&UpdateSession, None, CLSCTX_INPROC_SERVER) }.map_err(
                    |e| CoreError::SystemTool {
                        message: format!("No se pudo abrir sesión WUA: {}", e),
                        source: None,
                    },
                )?;

            let searcher =
                unsafe { session.CreateUpdateSearcher() }.map_err(|e| CoreError::SystemTool {
                    message: format!("CreateUpdateSearcher falló: {}", e),
                    source: None,
                })?;

            unsafe {
                let _ = searcher.SetOnline(VARIANT_BOOL(-1));
                // SYS-25: el servidor que configuró el administrador (WSUS incluido), nunca Windows Update forzado.
                let _ = searcher.SetServerSelection(ssDefault);
            }

            let criteria = BSTR::from("IsInstalled=0 and Type='Driver' and IsHidden=0");
            let search_res =
                unsafe { searcher.Search(&criteria) }.map_err(|e| CoreError::SystemTool {
                    message: format!("Error al localizar paquetes seleccionados: {}", e),
                    source: None,
                })?;

            let found_updates =
                unsafe { search_res.Updates() }.map_err(|e| CoreError::SystemTool {
                    message: format!("Updates() falló: {}", e),
                    source: None,
                })?;

            let to_download: IUpdateCollection =
                unsafe { CoCreateInstance(&UpdateCollection, None, CLSCTX_INPROC_SERVER) }
                    .map_err(|e| CoreError::SystemTool {
                        message: format!("CoCreateInstance(UpdateCollection) falló: {}", e),
                        source: None,
                    })?;

            let to_install: IUpdateCollection =
                unsafe { CoCreateInstance(&UpdateCollection, None, CLSCTX_INPROC_SERVER) }
                    .map_err(|e| CoreError::SystemTool {
                        message: format!("CoCreateInstance(UpdateCollection) falló: {}", e),
                        source: None,
                    })?;

            let count = unsafe { found_updates.Count() }.unwrap_or(0);
            let mut matched = 0usize;

            for idx in 0..count {
                let Ok(update) = (unsafe { found_updates.get_Item(idx) }) else {
                    continue;
                };
                let uid = unsafe { update.Identity() }
                    .and_then(|id| unsafe { id.UpdateID() })
                    .map(|b| b.to_string())
                    .unwrap_or_default();

                if !target_ids.contains(&uid) {
                    continue;
                }

                let eula_ok = unsafe { update.EulaAccepted() }
                    .map(|b| b.0 != 0)
                    .unwrap_or(true);
                if !eula_ok {
                    let _ = unsafe { update.AcceptEula() };
                }

                let is_dl = unsafe { update.IsDownloaded() }
                    .map(|b| b.0 != 0)
                    .unwrap_or(false);
                if !is_dl {
                    let _ = unsafe { to_download.Add(&update) };
                }
                // ERR-18: solo cuenta lo que de verdad entró en la colección a instalar.
                if unsafe { to_install.Add(&update) }.is_ok() {
                    matched += 1;
                }
            }

            if matched == 0 {
                return Err(CoreError::SystemTool {
                    message:
                        "Ninguno de los drivers seleccionados sigue pendiente en Windows Update."
                            .into(),
                    source: None,
                });
            }

            if unsafe { to_download.Count() }.unwrap_or(0) > 0 {
                let downloader = unsafe { session.CreateUpdateDownloader() }.map_err(|e| {
                    CoreError::SystemTool {
                        message: format!("CreateUpdateDownloader falló: {}", e),
                        source: None,
                    }
                })?;
                unsafe { downloader.SetUpdates(&to_download) }.map_err(|e| {
                    CoreError::SystemTool {
                        message: format!("Downloader.SetUpdates falló: {}", e),
                        source: None,
                    }
                })?;
                let dl_res =
                    unsafe { downloader.Download() }.map_err(|e| CoreError::SystemTool {
                        message: format!("Error descargando drivers desde Microsoft: {}", e),
                        source: None,
                    })?;
                let dl_code = unsafe { dl_res.ResultCode() }.unwrap_or(orcFailed);
                if dl_code != orcSucceeded && dl_code != orcSucceededWithErrors {
                    return Err(CoreError::SystemTool {
                        message: format!("La descarga oficial falló con estado {:?}", dl_code.0),
                        source: None,
                    });
                }
            }

            let installer =
                unsafe { session.CreateUpdateInstaller() }.map_err(|e| CoreError::SystemTool {
                    message: format!("CreateUpdateInstaller falló: {}", e),
                    source: None,
                })?;
            unsafe { installer.SetUpdates(&to_install) }.map_err(|e| CoreError::SystemTool {
                message: format!("Installer.SetUpdates falló: {}", e),
                source: None,
            })?;

            let inst_res = unsafe { installer.Install() }.map_err(|e| CoreError::SystemTool {
                message: format!("Error instalando drivers seleccionados: {}", e),
                source: None,
            })?;

            let reboot_required = unsafe { inst_res.RebootRequired() }
                .map(|b| b.0 != 0)
                .unwrap_or(false);

            // ERR-18: el resultado de cada driver, no una estimación desde el código global. Si no se puede leer el
            // resultado de uno, cuenta como fallo.
            let total = unsafe { to_install.Count() }.unwrap_or(0);
            let mut succeeded_count = 0usize;
            let mut failed_count = 0usize;
            for i in 0..total {
                let ok = unsafe { inst_res.GetUpdateResult(i) }
                    .and_then(|r| unsafe { r.ResultCode() })
                    .is_ok_and(|code| code == orcSucceeded || code == orcSucceededWithErrors);
                if ok {
                    succeeded_count += 1;
                } else {
                    failed_count += 1;
                }
            }

            Ok(DriverInstallReport {
                succeeded_count,
                failed_count,
                reboot_required,
                message: if failed_count == 0 {
                    format!("Windows Update instaló {succeeded_count} driver(s).")
                } else {
                    format!(
                        "Windows Update instaló {succeeded_count} driver(s); {failed_count} fallaron."
                    )
                },
            })
        });

        handle.join().map_err(|_| CoreError::SystemTool {
            message: "Fallo inesperado en hilo COM de instalación de drivers".into(),
            source: None,
        })?
    }

    pub fn search_microsoft_catalog(hw_id_query: &str) -> CoreResult<Vec<CatalogDriverMatch>> {
        let url = format!(
            "https://www.catalog.update.microsoft.com/Search.aspx?q={}",
            url_encode(hw_id_query)
        );
        let bytes = fetch_bytes(&url, CATALOG_HOSTS, None, 4 * 1024 * 1024)?;
        let html = String::from_utf8_lossy(&bytes);

        let mut matches = Vec::new();
        let mut cursor = 0usize;

        // El Catálogo marca cada resultado con `<a id='<guid>_link' …>` (comillas simples o dobles).
        while let Some(pos) = html[cursor..].find("_link") {
            let abs_pos = cursor + pos;
            cursor = abs_pos + 5;
            if !matches!(html.as_bytes().get(abs_pos + 5), Some(b'\'' | b'"')) {
                continue;
            }

            let mut back = abs_pos.saturating_sub(64);
            while !html.is_char_boundary(back) {
                back += 1;
            }
            let search_back = &html[back..abs_pos];
            let Some(id_start) = search_back.rfind("id=") else {
                continue;
            };
            let guid = search_back[id_start + 3..]
                .trim_start_matches(['\'', '"'])
                .trim();
            if !is_guid(guid) {
                continue;
            }

            let row_start = html[..abs_pos].rfind("<tr").unwrap_or(abs_pos);
            let Some(row_end_rel) = html[abs_pos..].find("</tr>") else {
                continue;
            };
            let row_html = &html[row_start..abs_pos + row_end_rel];

            let mut cells = Vec::new();
            let mut td_cur = 0usize;
            while let Some(td_open) = row_html[td_cur..].find("<td") {
                let td_abs = td_cur + td_open;
                let Some(td_close) = row_html[td_abs..].find("</td>") else {
                    break;
                };
                let cell_text = strip_html_tags(&row_html[td_abs..td_abs + td_close]);
                cells.push(cell_text);
                td_cur = td_abs + td_close + 5;
            }

            if cells.len() >= 7 {
                let title = cells[1].clone();
                if title.is_empty() {
                    continue;
                }
                let classification = cells[3].clone();
                let products = cells[2].clone();
                let last_updated = cells[4].clone();
                let version = cells[5].clone();
                let size_str = cells[6].clone();

                matches.push(CatalogDriverMatch {
                    update_id: guid.to_string(),
                    title,
                    products,
                    classification,
                    last_updated,
                    version,
                    size_str,
                    query_hw_id: hw_id_query.to_string(),
                });
            }

            if matches.len() >= 8 {
                break;
            }
        }

        Ok(matches)
    }

    /// SEC-33: instala un driver del Microsoft Update Catalog. El `update_id` tiene que ser un GUID; el enlace del CAB
    /// se acepta solo si su host es de Windows Update y termina en `.cab`; el CAB se baja con el cliente HTTPS común
    /// (host final validado), tiene que estar firmado por Microsoft antes de expandirse y todo vive en una carpeta
    /// temporal que se borra al terminar.
    fn install_catalog_driver_inner(update_id: &str) -> CoreResult<DriverInstallReport> {
        ensure_driver_updates_allowed()?;
        if !is_guid(update_id) {
            return Err(CoreError::Validation(
                "UpdateID de catálogo inválido".into(),
            ));
        }

        let download_url = catalog_cab_url(update_id)?;

        let scratch = ScratchDir::new("Catalog", update_id)?;
        let cab_path = scratch.dir.join("driver.cab");
        download_to_file(
            &download_url,
            CATALOG_CAB_HOSTS,
            &cab_path,
            MAX_CATALOG_CAB_BYTES,
        )?;

        let policy = DownloadPolicy {
            url: &download_url,
            allowed_final_hosts: CATALOG_CAB_HOSTS,
            expected_sha256_hex: None,
            expected_cert_thumbprint_sha1: None,
            expected_signer_cn: Some(MICROSOFT_SIGNER),
            max_bytes: MAX_CATALOG_CAB_BYTES,
        };
        // El handle de solo lectura impide cambiar el CAB entre la verificación y `expand`.
        let _cab_lock = reopen_verified_for_exec(&cab_path, &policy)?;

        let extract_dir = scratch.dir.join("extracted");
        create_admin_only_dir(&extract_dir)?;
        let system32 = system32_dir()?;
        let exp_out = run_captured(
            &system32.join("expand.exe"),
            &[
                cab_path.as_os_str(),
                std::ffi::OsStr::new("-F:*"),
                extract_dir.as_os_str(),
            ],
            PNPUTIL_TIMEOUT,
        )?;
        if !exp_out.success {
            return Err(CoreError::SystemTool {
                message: format!("Error extrayendo .CAB: {}", exp_out.stderr.trim()),
                source: None,
            });
        }

        let inf_pattern = extract_dir.join("*.inf");
        let pnp_out = run_captured(
            &system32.join("pnputil.exe"),
            &[
                std::ffi::OsStr::new("/add-driver"),
                inf_pattern.as_os_str(),
                std::ffi::OsStr::new("/install"),
            ],
            PNPUTIL_TIMEOUT,
        )?;
        let code = pnp_out.code.unwrap_or(-1);
        if pnp_out.success || code == 3010 {
            return Ok(DriverInstallReport::single(
                "Driver instalado desde el Catálogo de Microsoft.".into(),
                code == 3010,
            ));
        }
        Err(CoreError::SystemTool {
            message: format!(
                "pnputil devolvió código {}: {}",
                code,
                pnp_out.stdout.trim()
            ),
            source: None,
        })
    }

    pub fn check_nvidia_updates(
        devices: &[InstalledDriverDevice],
    ) -> CoreResult<Vec<VendorDriverUpdate>> {
        let mut results = Vec::new();
        let is_laptop = is_system_laptop();
        let Some(os_build) = read_windows_build() else {
            return Ok(results);
        };

        let Some(nv_gpu) = devices.iter().find(|d| {
            let is_display = d
                .class_name
                .as_deref()
                .map(|c| c.eq_ignore_ascii_case("Display"))
                .unwrap_or(false);
            let is_nv = d
                .extract_ven_dev()
                .map(|(v, _, _)| v == "10DE")
                .unwrap_or(false);
            is_display && is_nv
        }) else {
            return Ok(results);
        };

        let Some((_, dev, subsys)) = nv_gpu.extract_ven_dev() else {
            return Ok(results);
        };

        let did_entry = match (is_laptop, subsys) {
            (false, Some(sub)) if sub.len() == 8 => {
                format!("{}_10DE_{}_{}", dev, &sub[..4], &sub[4..8])
            }
            _ => format!("{}_10DE", dev),
        };

        let json_req = format!(
            r#"{{"dIDa":["{}"],"osC":"10.0","osB":"{}","is6":"1","lg":"1033","iLp":"{}","prvMd":"0","upCRD":"0"}}"#,
            did_entry,
            os_build,
            if is_laptop { "1" } else { "0" }
        );
        let url = format!(
            "https://gfwsl.geforce.com/nvidia_web_services/controller.gfeclientcontent.NG.php/com.nvidia.services.GFEClientContent_NG.getDispDrvrByDevid/{}",
            url_encode(&json_req)
        );

        // El servicio de NVIDIA responde a su cliente (NvBackend), no a un agente genérico.
        let bytes = fetch_bytes(
            &url,
            NVIDIA_API_HOSTS,
            Some("NvBackend/36.0.0.0"),
            2 * 1024 * 1024,
        )?;
        let parsed = serde_json::from_slice::<NvidiaLookupResp>(&bytes).map_err(|e| {
            CoreError::SystemTool {
                message: "Respuesta inesperada del servicio de drivers".into(),
                source: Some(Box::new(e)),
            }
        })?;
        let Some(attrs) = parsed.driver_attributes else {
            return Ok(results);
        };

        let Some(latest_ver) = attrs.version else {
            return Ok(results);
        };
        let Some(dl_url) = attrs.download_url_admin.or(attrs.download_url) else {
            return Ok(results);
        };
        if !dl_url.starts_with("https://") {
            return Ok(results);
        }

        // ERR-23: la versión interna (`32.0.15.x`) no se compara con la pública (`566.xx`).
        let current_ver = nv_gpu
            .friendly_gpu_version()
            .unwrap_or_else(|| "Desconocida".into());

        if nv_gpu.has_problem || is_version_newer(&latest_ver, &current_ver) {
            results.push(VendorDriverUpdate {
                id: format!("nvidia_{}", latest_ver),
                vendor: "NVIDIA".into(),
                title: attrs
                    .name
                    .unwrap_or_else(|| "NVIDIA GeForce Game Ready Driver".into())
                    .replace("%20", " "),
                current_version: current_ver,
                latest_version: latest_ver,
                release_date: attrs.release_date.unwrap_or_default(),
                download_url: dl_url,
                size_label: attrs.file_size.unwrap_or_else(|| "—".into()),
                expected_signer_cn: Some(NVIDIA_SIGNER.into()),
                allowed_hosts: NVIDIA_DOWNLOAD_HOSTS.iter().map(|h| (*h).into()).collect(),
                is_zip_inf: false,
                matched_hw_id: nv_gpu.short_hw_id().unwrap_or_default(),
            });
        }

        Ok(results)
    }

    pub fn check_intel_updates(
        devices: &[InstalledDriverDevice],
    ) -> CoreResult<Vec<VendorDriverUpdate>> {
        let mut results = Vec::new();

        let mut local_intel: HashMap<String, (Option<String>, bool)> = HashMap::new();
        for d in devices {
            if let Some((ven, dev, _)) = d.extract_ven_dev()
                && (ven == "8086" || ven == "8087")
            {
                let key = format!("{}_{}", ven, dev);
                let entry = local_intel
                    .entry(key)
                    .or_insert_with(|| (d.driver_version.clone(), d.has_problem));
                if d.has_problem {
                    entry.1 = true;
                }
                if entry.0.is_none() && d.driver_version.is_some() {
                    entry.0 = d.driver_version.clone();
                }
            }
        }

        if local_intel.is_empty() {
            return Ok(results);
        }

        let bytes = fetch_bytes(
            "https://dsadata.intel.com/data/en",
            INTEL_DSA_HOSTS,
            None,
            32 * 1024 * 1024,
        )?;

        let root = serde_json::from_slice::<Value>(&bytes).map_err(|e| CoreError::SystemTool {
            message: "Respuesta inesperada del servicio de drivers".into(),
            source: Some(Box::new(e)),
        })?;

        let mut seen_urls = HashSet::new();
        collect_intel_matches(&root, &local_intel, &mut seen_urls, &mut results);
        Ok(results)
    }

    /// SEC-30: descarga e instala una actualización de fabricante. Hosts y firmante salen de [`vendor_policy`] (fijos en el
    /// binario), nunca de `upd`, que viene del escaneo (de la red) y la vista devuelve tal cual. Todo ejecutable pasa por
    /// Authenticode con el firmante del fabricante y corre dentro de un Job; los temporales se borran al terminar.
    fn install_vendor_update_inner(upd: &VendorDriverUpdate) -> CoreResult<DriverInstallReport> {
        ensure_driver_updates_allowed()?;
        let (hosts, signer) = vendor_policy(&upd.vendor)?;
        validate_https_host(&upd.download_url, hosts)?;
        let safe_id: String = upd
            .id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        if upd.is_zip_inf {
            return install_vendor_zip(upd, &safe_id, hosts, signer);
        }

        let policy = DownloadPolicy {
            url: &upd.download_url,
            allowed_final_hosts: hosts,
            expected_sha256_hex: None,
            expected_cert_thumbprint_sha1: None,
            expected_signer_cn: Some(signer),
            max_bytes: 1600 * 1024 * 1024,
        };
        let installer = StagedDownload::download(&safe_id, "exe", &policy)?;
        run_verified_vendor_installer(installer.path(), &policy, &upd.title)
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

/// Enlace del CAB de una actualización del Catálogo, pedido a `DownloadDialog.aspx` (`update_id` ya validado como GUID).
fn catalog_cab_url(update_id: &str) -> CoreResult<String> {
    let json_payload = format!(
        r#"[{{"size":0,"languages":"","uidInfo":"{}","updateID":"{}"}}]"#,
        update_id, update_id
    );
    let form_body = format!("updateIDs={}", url_encode(&json_payload));
    let resp = http::post_form(
        CATALOG_HOSTS[0],
        "/DownloadDialog.aspx",
        &form_body,
        DATA_TIMEOUTS,
    )?;
    resp.require_ok()?;
    validate_https_host(&resp.final_url()?, CATALOG_HOSTS)?;
    let dialog_html =
        String::from_utf8_lossy(&resp.read_to_end_capped(4 * 1024 * 1024)?).into_owned();
    find_catalog_cab_url(&dialog_html).ok_or_else(|| CoreError::SystemTool {
        message: "No se encontró enlace de descarga .CAB válido en Microsoft Catalog.".into(),
        source: None,
    })
}

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
fn with_driver_history(
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

/// Plazo de una búsqueda de drivers en Windows Update (CON-03).
const WUA_SEARCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4 * 60);

/// Hosts de los que se aceptan CAB del Microsoft Update Catalog (SEC-33).
const CATALOG_CAB_HOSTS: &[&str] = &["download.windowsupdate.com", "*.download.windowsupdate.com"];
const MICROSOFT_SIGNER: &str =
    "Microsoft Corporation|Microsoft Windows|Microsoft Windows Hardware Compatibility Publisher";
const MAX_CATALOG_CAB_BYTES: u64 = 350 * 1024 * 1024;

/// `true` si `s` es un GUID `8-4-4-4-12` en hexadecimal (forma en que el Catálogo nombra sus actualizaciones).
fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, len)| p.len() == len && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Primer enlace a un `.cab` de Windows Update en el HTML de `DownloadDialog.aspx`, pasado a HTTPS. Se descarta lo
/// que no sea `.cab` (antes se bajaban `.msu` y `.exe` para rechazarlos después, ERR-22) y lo que no tenga un host de
/// [`CATALOG_CAB_HOSTS`] exacto.
fn find_catalog_cab_url(html: &str) -> Option<String> {
    for marker in ["https://", "http://"] {
        let mut cur = 0usize;
        while let Some(p) = html[cur..].find(marker) {
            let abs = cur + p;
            let tail = &html[abs..];
            let end = tail
                .find(['\'', '"', '<', ' '])
                .unwrap_or(tail.len().min(512));
            let candidate = &tail[..end];
            let secure = match candidate.strip_prefix("http://") {
                Some(rest) => format!("https://{rest}"),
                None => candidate.to_string(),
            };
            let path = secure.split(['?', '#']).next().unwrap_or_default();
            if path.to_ascii_lowercase().ends_with(".cab")
                && validate_https_host(&secure, CATALOG_CAB_HOSTS).is_ok()
            {
                return Some(secure);
            }
            cur = abs + marker.len();
        }
    }
    None
}

/// Hosts de descarga de NVIDIA (el comodín cubre los espejos regionales).
const NVIDIA_DOWNLOAD_HOSTS: &[&str] = &[
    "*.download.nvidia.com",
    "us.download.nvidia.com",
    "uk.download.nvidia.com",
    "international.download.nvidia.com",
];
const NVIDIA_SIGNER: &str = "NVIDIA Corporation";
const INTEL_DOWNLOAD_HOSTS: &[&str] = &["downloadmirror.intel.com", "download.intel.com"];
const INTEL_SIGNER: &str = "Intel Corporation|Intel(R) Corporation|Intel(R) Software";
/// Un instalador de drivers puede pedir interacción (NVIDIA no tiene modo silencioso aquí): margen amplio antes de
/// cortarlo, porque matar un instalador de drivers a mitad es peor que esperar.
const VENDOR_INSTALLER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60 * 60);
const PNPUTIL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);
const MAX_VENDOR_ZIP_BYTES: u64 = 900 * 1024 * 1024;

/// SEC-30: hosts permitidos y firmante Authenticode de cada fabricante.
fn vendor_policy(vendor: &str) -> CoreResult<(&'static [&'static str], &'static str)> {
    match vendor {
        "NVIDIA" => Ok((NVIDIA_DOWNLOAD_HOSTS, NVIDIA_SIGNER)),
        "Intel" => Ok((INTEL_DOWNLOAD_HOSTS, INTEL_SIGNER)),
        other => Err(CoreError::Validation(format!(
            "Fabricante de drivers no soportado: '{other}'"
        ))),
    }
}

/// Reabre `path` comprobando de nuevo la firma sobre el handle que se ejecuta y lo corre en un Job hasta que termina.
fn run_verified_vendor_installer(
    path: &Path,
    policy: &DownloadPolicy<'_>,
    title: &str,
) -> CoreResult<DriverInstallReport> {
    let _exec_lock = reopen_verified_for_exec(path, policy)?;
    let working_dir = path.parent().unwrap_or(path);
    let exit = run_in_job(
        &Launch {
            application: path,
            command_line: format!("\"{}\"", path.display()),
            working_dir,
            user_token: None,
        },
        VENDOR_INSTALLER_TIMEOUT,
        None,
    )?;
    if exit.is_success() {
        Ok(DriverInstallReport::single(
            format!("Instalador oficial '{title}' verificado y terminado."),
            exit.exit_code == 3010 || exit.exit_code == 1641,
        ))
    } else {
        Err(CoreError::SystemTool {
            message: format!(
                "El instalador de '{title}' terminó con código {}",
                exit.code_display()
            ),
            source: None,
        })
    }
}

/// Carpeta temporal solo-admin de un paquete de drivers (`DriverCache\<kind>\<stem>_<nonce>`): se borra al soltarse
/// (SEC-34).
struct ScratchDir {
    root: PathBuf,
    dir: PathBuf,
}

impl ScratchDir {
    fn new(kind: &str, stem: &str) -> CoreResult<Self> {
        let root = ensure_admin_only_data_dir()?.join("DriverCache").join(kind);
        std::fs::create_dir_all(&root)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = root.join(format!("{stem}_{nonce}"));
        create_admin_only_dir(&dir)?;
        Ok(Self { root, dir })
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = delete_tree_verified(&self.dir, &self.root, 64);
    }
}

/// Baja `url` a `dest` (archivo nuevo) por el cliente HTTPS común, exigiendo estado 200, que el host final esté en
/// `hosts` y que no pase de `max_bytes`. Una lectura cortada falla en vez de dejar un archivo truncado (ERR-19).
fn download_to_file(url: &str, hosts: &[&str], dest: &Path, max_bytes: u64) -> CoreResult<()> {
    let resp = http::get(
        url,
        http::Timeouts {
            connect_ms: 30_000,
            io_ms: 120_000,
        },
    )?;
    resp.require_ok()?;
    validate_https_host(&resp.final_url()?, hosts)?;
    let (mut f, _) = create_verified_new_file(dest, true)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = resp.read_chunk(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max_bytes {
            return Err(CoreError::Trust {
                message: format!("La descarga de '{url}' excede el tamaño máximo"),
                source: None,
            });
        }
        f.write_all(&buf[..n])?;
    }
    if resp.content_length().is_some_and(|len| len != total) {
        return Err(CoreError::Trust {
            message: format!("Descarga incompleta de '{url}'"),
            source: None,
        });
    }
    f.sync_all()?;
    Ok(())
}

/// SEC-30: paquete ZIP de Intel. Se baja con el cliente HTTPS común validando el host final, se extrae con el
/// archivador endurecido (rutas, symlinks, cuota) y solo se ejecuta un instalador firmado por Intel; si no hay
/// instalador, `pnputil` agrega los INF del paquete.
fn install_vendor_zip(
    upd: &VendorDriverUpdate,
    safe_id: &str,
    hosts: &'static [&'static str],
    signer: &'static str,
) -> CoreResult<DriverInstallReport> {
    use std::sync::atomic::AtomicBool;

    let scratch = ScratchDir::new("Vendor", safe_id)?;
    let zip_path = scratch.dir.join("package.zip");
    download_to_file(&upd.download_url, hosts, &zip_path, MAX_VENDOR_ZIP_BYTES)?;

    let extract_dir = scratch.dir.join("extracted");
    let cancel = AtomicBool::new(false);
    ArchiveManager::extract_archive(&zip_path, &extract_dir, 4 * MAX_VENDOR_ZIP_BYTES, &cancel)?;

    if let Some(setup_exe) = find_installer_in_dir(&extract_dir) {
        let policy = DownloadPolicy {
            url: &upd.download_url,
            allowed_final_hosts: hosts,
            expected_sha256_hex: None,
            expected_cert_thumbprint_sha1: None,
            expected_signer_cn: Some(signer),
            max_bytes: MAX_VENDOR_ZIP_BYTES,
        };
        return run_verified_vendor_installer(&setup_exe, &policy, &upd.title);
    }

    // ERR-21: pnputil no admite `**`; `/subdirs` ya recorre las subcarpetas.
    let inf_pattern = extract_dir.join("*.inf");
    let out = run_captured(
        &system32_dir()?.join("pnputil.exe"),
        &[
            std::ffi::OsStr::new("/add-driver"),
            inf_pattern.as_os_str(),
            std::ffi::OsStr::new("/subdirs"),
            std::ffi::OsStr::new("/install"),
        ],
        PNPUTIL_TIMEOUT,
    )?;
    if out.success || out.code == Some(3010) {
        Ok(DriverInstallReport::single(
            format!("Driver '{}' instalado vía Driver Store.", upd.title),
            out.code == Some(3010),
        ))
    } else {
        Err(CoreError::SystemTool {
            message: format!("pnputil devolvió error: {}", out.stdout.trim()),
            source: None,
        })
    }
}

fn find_installer_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if name == "setup.exe" || name == "installer.exe" || name == "igxpin.exe" {
                return Some(path);
            }
        }
    }
    None
}

fn collect_intel_matches(
    node: &Value,
    local_intel: &HashMap<String, (Option<String>, bool)>,
    seen_urls: &mut HashSet<String>,
    out: &mut Vec<VendorDriverUpdate>,
) {
    match node {
        Value::Array(arr) => {
            for item in arr {
                collect_intel_matches(item, local_intel, seen_urls, out);
            }
        }
        Value::Object(map) => {
            let blob = node.to_string().to_ascii_uppercase();
            let mut matched_local: Option<(String, Option<String>, bool)> = None;

            for (key, (inst_ver, has_prob)) in local_intel {
                let parts: Vec<&str> = key.split('_').collect();
                if parts.len() == 2 {
                    let pci_pat = format!("VEN_{}&DEV_{}", parts[0], parts[1]);
                    let usb_pat = format!("VID_{}&PID_{}", parts[0], parts[1]);
                    if blob.contains(&pci_pat) || blob.contains(&usb_pat) {
                        matched_local = Some((
                            format!("{}:{}", parts[0], parts[1]),
                            inst_ver.clone(),
                            *has_prob,
                        ));
                        break;
                    }
                }
            }

            if let Some((hw_tag, inst_ver, has_prob)) = matched_local
                && let Some(dl_url) = find_intel_download_url(node)
                && seen_urls.insert(dl_url.clone())
            {
                let title = find_json_str(map, &["Title", "Name", "PackageName"])
                    .unwrap_or_else(|| "Intel® Driver Package".into());
                let version = find_json_str(map, &["Version", "DriverVersion"])
                    .unwrap_or_else(|| "Latest".into());
                let rel_date = find_json_str(map, &["ReleaseDate", "Date", "PublishedDate"])
                    .unwrap_or_default();

                let cur_ver_str = inst_ver.clone().unwrap_or_else(|| "Sin driver".into());
                let should_offer =
                    has_prob || inst_ver.is_none() || is_version_newer(&version, &cur_ver_str);

                if should_offer {
                    let is_zip = dl_url.to_ascii_lowercase().ends_with(".zip");
                    out.push(VendorDriverUpdate {
                        id: format!("intel_{}_{}", hw_tag, version),
                        vendor: "Intel".into(),
                        title,
                        current_version: cur_ver_str,
                        latest_version: version,
                        release_date: rel_date,
                        download_url: dl_url,
                        size_label: "Oficial Intel".into(),
                        expected_signer_cn: Some(INTEL_SIGNER.into()),
                        allowed_hosts: INTEL_DOWNLOAD_HOSTS.iter().map(|h| (*h).into()).collect(),
                        is_zip_inf: is_zip,
                        matched_hw_id: hw_tag,
                    });
                    return;
                }
            }

            for val in map.values() {
                if matches!(val, Value::Array(_) | Value::Object(_)) {
                    collect_intel_matches(val, local_intel, seen_urls, out);
                }
            }
        }
        _ => {}
    }
}

fn find_json_str(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for target in keys {
        for (k, v) in map {
            if k.eq_ignore_ascii_case(target)
                && let Some(s) = v.as_str()
            {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

fn find_intel_download_url(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => {
            let low = s.to_ascii_lowercase();
            if s.starts_with("https://")
                && validate_https_host(&low, INTEL_DOWNLOAD_HOSTS).is_ok()
                && (low.ends_with(".exe") || low.ends_with(".zip"))
            {
                return Some(s.clone());
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_intel_download_url),
        Value::Object(map) => map.values().find_map(find_intel_download_url),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_pci_se_acorta_a_ven_dev() {
        let raw = r"PCI\VEN_1002&DEV_744C&SUBSYS_0E3C1002&REV_C8";
        assert_eq!(
            short_hw_id_of(raw).as_deref(),
            Some(r"PCI\VEN_1002&DEV_744C")
        );
        assert_eq!(
            ven_dev_of(raw),
            Some(("1002".into(), "744C".into(), Some("0E3C1002".into())))
        );
    }

    #[test]
    fn id_acpi_y_usb() {
        assert_eq!(
            short_hw_id_of(r"ACPI\VEN_INT&DEV_33A0").as_deref(),
            Some("VEN_INT&DEV_33A0")
        );
        assert_eq!(
            short_hw_id_of(r"USB\VID_046D&PID_C52B&REV_1200").as_deref(),
            Some(r"USB\VID_046D&PID_C52B")
        );
        assert_eq!(
            short_hw_id_of(r"ROOT\AMDXE").as_deref(),
            Some(r"ROOT\AMDXE")
        );
        assert_eq!(ven_dev_of(r"ROOT\AMDXE"), None);
    }

    #[test]
    fn version_desconocida_no_ofrece_actualizar() {
        assert!(is_version_newer("566.36", "565.90"));
        assert!(!is_version_newer("566.36", "566.36"));
        assert!(!is_version_newer("566.36", "Desconocida"));
        assert!(!is_version_newer("Latest", "31.0.101.5590"));
    }

    #[test]
    fn version_amigable_de_nvidia() {
        let dev = InstalledDriverDevice {
            device_desc: None,
            friendly_name: None,
            manufacturer: Some("NVIDIA".into()),
            provider_name: Some("NVIDIA".into()),
            class_name: Some("Display".into()),
            hardware_id: None,
            driver_version: Some("32.0.15.6636".into()),
            driver_date: None,
            inf_path: None,
            dev_inst: 0,
            has_problem: false,
            problem_code: 0,
            needs_reboot: false,
        };
        assert_eq!(dev.friendly_gpu_version().as_deref(), Some("566.36"));
    }

    #[test]
    fn update_id_solo_guid() {
        assert!(is_guid("0a1b2c3d-4e5f-6789-abcd-ef0123456789"));
        assert!(!is_guid("C:xxx-0000-0000-0000-000000000000"));
        assert!(!is_guid("0a1b2c3d-4e5f-6789-abcd-ef012345678\""));
        assert!(!is_guid(""));
    }

    #[test]
    fn enlace_cab_del_catalogo() {
        let ok = r#"x 'http://catalog.s.download.windowsupdate.com/d/msdownload/abc_123.cab' y"#;
        assert_eq!(
            find_catalog_cab_url(ok).as_deref(),
            Some("https://catalog.s.download.windowsupdate.com/d/msdownload/abc_123.cab")
        );
        // Subcadena del host en otro dominio, traversal en el nombre y paquetes que no son CAB.
        assert_eq!(
            find_catalog_cab_url("'https://evil.example/download.windowsupdate.com/a.cab'"),
            None
        );
        assert_eq!(
            find_catalog_cab_url("'https://download.windowsupdate.com.evil.example/a.cab'"),
            None
        );
        assert_eq!(
            find_catalog_cab_url("'https://download.windowsupdate.com/a.msu'"),
            None
        );
    }

    /// Red real: busca en el Catálogo y resuelve el enlace del CAB del primer resultado (no lo descarga).
    #[test]
    #[ignore = "usa la red"]
    fn catalogo_en_vivo() {
        let found =
            DriverManager::search_microsoft_catalog(r"PCI\VEN_8086&DEV_A0F0").expect("búsqueda");
        let first = found.first().expect("algún resultado");
        let url = catalog_cab_url(&first.update_id).expect("enlace del CAB");
        assert!(url.to_ascii_lowercase().ends_with(".cab"));
    }

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
