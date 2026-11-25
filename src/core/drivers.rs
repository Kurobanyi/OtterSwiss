use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::Value;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVNODE_STATUS_FLAGS, CM_Get_DevNode_Status, CM_PROB, CR_SUCCESS, DIGCF_ALLCLASSES,
    DIGCF_PRESENT, DN_HAS_PROBLEM, DN_NEED_RESTART, HDEVINFO, SETUP_DI_REGISTRY_PROPERTY,
    SP_DEVINFO_DATA, SPDRP_CLASS, SPDRP_DEVICEDESC, SPDRP_DRIVER, SPDRP_FRIENDLYNAME,
    SPDRP_HARDWAREID, SPDRP_MFG, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
};
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, VARIANT_BOOL};
use windows::Win32::Networking::WinInet::{
    HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE, HttpOpenRequestW, HttpQueryInfoW,
    HttpSendRequestW, INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD, INTERNET_FLAG_SECURE,
    INTERNET_OPEN_TYPE_PRECONFIG, INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT,
    INTERNET_OPTION_SEND_TIMEOUT, INTERNET_SERVICE_HTTP, InternetConnectW, InternetOpenUrlW,
    InternetOpenW, InternetReadFile, InternetSetOptionW,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::UpdateAgent::{
    IUpdate3, IUpdateCollection, IUpdateSession, IWindowsDriverUpdate, UpdateCollection,
    UpdateSession, orcSucceeded, orcSucceededWithErrors, ssDefault, ssWindowsUpdate,
};
use windows::core::{BSTR, Interface, PCWSTR};
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use crate::core::archive::ArchiveManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::WinInetHandle;
use crate::core::platform::memory::AlignedBuf;
use crate::core::platform::process::{Launch, run_captured, run_in_job, system32_dir};
use crate::core::platform::safe_fs::{create_verified_new_file, delete_tree_verified};
use crate::core::platform::wide::to_wide_null;
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

    pub fn short_hw_id(&self) -> Option<String> {
        let raw = self.hardware_id.as_deref()?.trim();
        if raw.is_empty() {
            return None;
        }
        let up = raw.to_ascii_uppercase();
        if let Some(idx) = up.find("VEN_") {
            let prefix = if up.starts_with("PCI\\") { "PCI\\" } else { "" };
            let rest = &up[idx..];
            let ven = rest.get(4..8)?;
            if let Some(d_idx) = rest.find("DEV_") {
                let dev = rest.get(d_idx + 4..d_idx + 8)?;
                return Some(format!("{}VEN_{}&DEV_{}", prefix, ven, dev));
            }
        }
        if let Some(idx) = up.find("VID_") {
            let prefix = if up.starts_with("USB\\") { "USB\\" } else { "" };
            let rest = &up[idx..];
            let vid = rest.get(4..8)?;
            if let Some(p_idx) = rest.find("PID_") {
                let pid = rest.get(p_idx + 4..p_idx + 8)?;
                return Some(format!("{}VID_{}&PID_{}", prefix, vid, pid));
            }
        }
        Some(raw.split('&').take(2).collect::<Vec<_>>().join("&"))
    }

    pub fn extract_ven_dev(&self) -> Option<(String, String, Option<String>)> {
        let raw = self.hardware_id.as_deref()?.to_ascii_uppercase();
        let ven = if let Some(i) = raw.find("VEN_") {
            raw.get(i + 4..i + 8)?.to_string()
        } else {
            let i = raw.find("VID_")?;
            raw.get(i + 4..i + 8)?.to_string()
        };

        let dev = if let Some(i) = raw.find("DEV_") {
            raw.get(i + 4..i + 8)?.to_string()
        } else {
            let i = raw.find("PID_")?;
            raw.get(i + 4..i + 8)?.to_string()
        };

        let subsys = if let Some(i) = raw.find("SUBSYS_") {
            raw.get(i + 7..i + 15).map(|s| s.to_string())
        } else {
            None
        };

        Some((ven, dev, subsys))
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

fn is_wu_driver_policy_blocked() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate",
        KEY_READ,
    ) && let Ok(val) = key.get_value::<u32, _>("ExcludeWUDriversInQualityUpdate")
    {
        return val == 1;
    }
    false
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

fn wininet_get_bytes(url: &str, user_agent: &str, max_bytes: usize) -> CoreResult<Vec<u8>> {
    let agent_w = to_wide_null(user_agent);
    let h_internet = WinInetHandle::new(unsafe {
        InternetOpenW(
            PCWSTR(agent_w.as_ptr()),
            INTERNET_OPEN_TYPE_PRECONFIG.0,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: "InternetOpenW falló".into(),
        source: None,
    })?;

    let timeout_ms: u32 = 20_000;
    unsafe {
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_CONNECT_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_SEND_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_RECEIVE_TIMEOUT,
            Some(&timeout_ms as *const _ as *const _),
            4,
        );
    }

    let url_w = to_wide_null(url);
    let h_url = WinInetHandle::new(unsafe {
        InternetOpenUrlW(
            h_internet.raw(),
            PCWSTR(url_w.as_ptr()),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            None,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: format!("No se pudo abrir URL '{}'", url),
        source: None,
    })?;

    let mut status_code: u32 = 0;
    let mut status_len: u32 = std::mem::size_of::<u32>() as u32;
    unsafe {
        HttpQueryInfoW(
            h_url.raw(),
            HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
            Some(&mut status_code as *mut _ as *mut _),
            &mut status_len,
            None,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("HttpQueryInfoW falló: {}", e),
        source: None,
    })?;

    if status_code != 200 {
        return Err(CoreError::Trust {
            message: format!("HTTP {} desde '{}'", status_code, url),
            source: None,
        });
    }

    let mut data = Vec::new();
    let mut chunk = [0u8; 32 * 1024];
    loop {
        let mut read_bytes = 0u32;
        let ok = unsafe {
            InternetReadFile(
                h_url.raw(),
                chunk.as_mut_ptr() as *mut _,
                chunk.len() as u32,
                &mut read_bytes,
            )
        };
        if ok.is_err() || read_bytes == 0 {
            break;
        }
        if data.len() + (read_bytes as usize) > max_bytes {
            return Err(CoreError::Trust {
                message: "La respuesta excede el límite permitido".into(),
                source: None,
            });
        }
        data.extend_from_slice(&chunk[..read_bytes as usize]);
    }

    Ok(data)
}

fn wininet_post_form(host: &str, path: &str, form_body: &str) -> CoreResult<String> {
    let agent_w = to_wide_null("OtterSwiss-Catalog/4.0");
    let h_internet = WinInetHandle::new(unsafe {
        InternetOpenW(
            PCWSTR(agent_w.as_ptr()),
            INTERNET_OPEN_TYPE_PRECONFIG.0,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: "InternetOpenW falló".into(),
        source: None,
    })?;

    let host_w = to_wide_null(host);
    let h_conn = WinInetHandle::new(unsafe {
        InternetConnectW(
            h_internet.raw(),
            PCWSTR(host_w.as_ptr()),
            443,
            PCWSTR::null(),
            PCWSTR::null(),
            INTERNET_SERVICE_HTTP,
            0,
            None,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: format!("InternetConnectW falló para {}", host),
        source: None,
    })?;

    let verb_w = to_wide_null("POST");
    let path_w = to_wide_null(path);
    let h_req = WinInetHandle::new(unsafe {
        HttpOpenRequestW(
            h_conn.raw(),
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            None,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: format!("HttpOpenRequestW falló para {}", path),
        source: None,
    })?;

    let hdrs_w = to_wide_null("Content-Type: application/x-www-form-urlencoded\r\n");
    let body_bytes = form_body.as_bytes();
    unsafe {
        HttpSendRequestW(
            h_req.raw(),
            Some(&hdrs_w[..hdrs_w.len() - 1]),
            Some(body_bytes.as_ptr() as *const _),
            body_bytes.len() as u32,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("HttpSendRequestW falló: {}", e),
        source: None,
    })?;

    let mut data = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let mut read_bytes = 0u32;
        let ok = unsafe {
            InternetReadFile(
                h_req.raw(),
                chunk.as_mut_ptr() as *mut _,
                chunk.len() as u32,
                &mut read_bytes,
            )
        };
        if ok.is_err() || read_bytes == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..read_bytes as usize]);
        if data.len() > 4 * 1024 * 1024 {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&data).into_owned())
}

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

fn parse_version_tuple(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

fn is_version_newer(candidate: &str, current: &str) -> bool {
    let a = parse_version_tuple(candidate);
    let b = parse_version_tuple(current);
    if a.is_empty() {
        return false;
    }
    if b.is_empty() {
        return true;
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

fn read_windows_build() -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) =
        hklm.open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ)
        && let Ok(b) = key.get_value::<String, _>("CurrentBuildNumber")
    {
        return b.trim().to_string();
    }
    "22631".to_string()
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

        let handle = std::thread::spawn(move || -> CoreResult<Vec<DriverUpdateCandidate>> {
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
                if searcher.SetServerSelection(ssWindowsUpdate).is_err() {
                    let _ = searcher.SetServerSelection(ssDefault);
                }
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
        });

        handle.join().map_err(|_| CoreError::SystemTool {
            message: "Fallo inesperado en hilo COM de búsqueda de drivers".into(),
            source: None,
        })?
    }

    pub fn install_driver_updates(selected_ids: &[String]) -> CoreResult<DriverInstallReport> {
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
                if searcher.SetServerSelection(ssWindowsUpdate).is_err() {
                    let _ = searcher.SetServerSelection(ssDefault);
                }
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
                let _ = unsafe { to_install.Add(&update) };
                matched += 1;
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
                let dl_code = unsafe { dl_res.ResultCode() }.unwrap_or(orcSucceededWithErrors);
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

            let res_code = unsafe { inst_res.ResultCode() }.unwrap_or(orcSucceededWithErrors);
            let reboot_required = unsafe { inst_res.RebootRequired() }
                .map(|b| b.0 != 0)
                .unwrap_or(false);

            let (succeeded_count, failed_count) = if res_code == orcSucceeded {
                (matched, 0)
            } else if res_code == orcSucceededWithErrors {
                (matched.saturating_sub(1), 1)
            } else {
                (0, matched)
            };

            Ok(DriverInstallReport {
                succeeded_count,
                failed_count,
                reboot_required,
                message: format!(
                    "Resultado WUA: {:?} (OK: {}, Fallos: {})",
                    res_code.0, succeeded_count, failed_count
                ),
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
        let bytes = wininet_get_bytes(&url, "OtterSwiss-Catalog/4.0", 4 * 1024 * 1024)?;
        let html = String::from_utf8_lossy(&bytes);

        let mut matches = Vec::new();
        let mut cursor = 0usize;

        while let Some(pos) = html[cursor..].find("_link\"") {
            let abs_pos = cursor + pos;
            cursor = abs_pos + 6;

            let search_back = &html[abs_pos.saturating_sub(64)..abs_pos];
            let Some(id_start) = search_back.rfind("id=\"") else {
                continue;
            };
            let guid = search_back[id_start + 4..].trim();
            if guid.len() < 32 || guid.len() > 40 || !guid.contains('-') {
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

    pub fn install_catalog_driver(update_id: &str) -> CoreResult<String> {
        if update_id.contains(['\\', '/', '.', ' ']) || update_id.is_empty() {
            return Err(CoreError::Validation(
                "UpdateID de catálogo inválido".into(),
            ));
        }

        let json_payload = format!(
            r#"[{{"size":0,"languages":"","uidInfo":"{}","updateID":"{}"}}]"#,
            update_id, update_id
        );
        let form_body = format!("updateIDs={}", url_encode(&json_payload));
        let dialog_html = wininet_post_form(
            "www.catalog.update.microsoft.com",
            "/DownloadDialog.aspx",
            &form_body,
        )?;

        let mut cab_url: Option<String> = None;
        for marker in ["https://", "http://"] {
            let mut cur = 0usize;
            while let Some(p) = dialog_html[cur..].find(marker) {
                let abs = cur + p;
                let tail = &dialog_html[abs..];
                let end = tail
                    .find(['\'', '"', '<', ' '])
                    .unwrap_or(tail.len().min(512));
                let candidate = &tail[..end];
                if candidate.contains("download.windowsupdate.com")
                    && (candidate.to_ascii_lowercase().ends_with(".cab")
                        || candidate.to_ascii_lowercase().ends_with(".msu")
                        || candidate.to_ascii_lowercase().ends_with(".exe"))
                {
                    let secure = if let Some(rest) = candidate.strip_prefix("http://") {
                        format!("https://{}", rest)
                    } else {
                        candidate.to_string()
                    };
                    cab_url = Some(secure);
                    break;
                }
                cur = abs + marker.len();
            }
            if cab_url.is_some() {
                break;
            }
        }

        let download_url = cab_url.ok_or_else(|| CoreError::SystemTool {
            message: "No se encontró enlace de descarga .CAB válido en Microsoft Catalog.".into(),
            source: None,
        })?;

        let base_dir = ensure_admin_only_data_dir()?;
        let cache_root = base_dir.join("DriverCache");
        create_admin_only_dir(&cache_root)?;
        let pkg_dir = cache_root.join(update_id);
        if pkg_dir.exists() {
            let _ = delete_tree_verified(&pkg_dir, &cache_root, 8);
        }
        create_admin_only_dir(&pkg_dir)?;

        let filename = download_url
            .rsplit('/')
            .next()
            .unwrap_or("driver.cab")
            .split('?')
            .next()
            .unwrap_or("driver.cab");
        let pkg_path = pkg_dir.join(filename);

        let raw_bytes =
            wininet_get_bytes(&download_url, "OtterSwiss-Catalog/4.0", 350 * 1024 * 1024)?;
        {
            let (mut f, _) = create_verified_new_file(&pkg_path, true)?;
            f.write_all(&raw_bytes)?;
            f.flush()?;
            f.sync_all()?;
        }

        if filename.to_ascii_lowercase().ends_with(".cab") {
            let extract_dir = pkg_dir.join("extracted");
            create_admin_only_dir(&extract_dir)?;

            let expand_exe = get_system32_tool("expand.exe");
            let exp_out = std::process::Command::new(&expand_exe)
                .args([
                    pkg_path.to_string_lossy().as_ref(),
                    "-F:*",
                    extract_dir.to_string_lossy().as_ref(),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| CoreError::SystemTool {
                    message: format!("No se pudo ejecutar expand.exe: {}", e),
                    source: None,
                })?;

            if !exp_out.status.success() {
                return Err(CoreError::SystemTool {
                    message: format!(
                        "Error extrayendo .CAB: {}",
                        String::from_utf8_lossy(&exp_out.stderr)
                    ),
                    source: None,
                });
            }

            let inf_pattern = extract_dir.join("*.inf");
            let pnputil = get_system32_tool("pnputil.exe");
            let pnp_out = std::process::Command::new(&pnputil)
                .args([
                    "/add-driver",
                    inf_pattern.to_string_lossy().as_ref(),
                    "/install",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| CoreError::SystemTool {
                    message: format!("No se pudo ejecutar pnputil.exe: {}", e),
                    source: None,
                })?;

            let code = pnp_out.status.code().unwrap_or(-1);
            if pnp_out.status.success() || code == 3010 {
                let reboot_msg = if code == 3010 {
                    " (Reinicio requerido)"
                } else {
                    ""
                };
                return Ok(format!(
                    "Driver instalado nativamente desde Microsoft Catalog{}.",
                    reboot_msg
                ));
            }

            return Err(CoreError::SystemTool {
                message: format!(
                    "pnputil devolvió código {}: {}",
                    code,
                    String::from_utf8_lossy(&pnp_out.stdout)
                ),
                source: None,
            });
        }

        Err(CoreError::SystemTool {
            message: format!(
                "Paquete descargado ({}) no es un .CAB estándar para instalación directa.",
                filename
            ),
            source: None,
        })
    }

    pub fn check_nvidia_updates(devices: &[InstalledDriverDevice]) -> Vec<VendorDriverUpdate> {
        let mut results = Vec::new();
        let is_laptop = is_system_laptop();
        let os_build = read_windows_build();

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
            return results;
        };

        let Some((_, dev, subsys)) = nv_gpu.extract_ven_dev() else {
            return results;
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

        let Ok(bytes) = wininet_get_bytes(&url, "NvBackend/36.0.0.0", 2 * 1024 * 1024) else {
            return results;
        };
        let Ok(parsed) = serde_json::from_slice::<NvidiaLookupResp>(&bytes) else {
            return results;
        };
        let Some(attrs) = parsed.driver_attributes else {
            return results;
        };

        let Some(latest_ver) = attrs.version else {
            return results;
        };
        let Some(dl_url) = attrs.download_url_admin.or(attrs.download_url) else {
            return results;
        };
        if !dl_url.starts_with("https://") {
            return results;
        }

        let current_ver = nv_gpu
            .friendly_gpu_version()
            .or_else(|| nv_gpu.driver_version.clone())
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

        results
    }

    pub fn check_intel_updates(devices: &[InstalledDriverDevice]) -> Vec<VendorDriverUpdate> {
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
            return results;
        }

        let Ok(bytes) = wininet_get_bytes(
            "https://dsadata.intel.com/data/en",
            "OtterSwiss-IntelDSA/4.0",
            32 * 1024 * 1024,
        ) else {
            return results;
        };

        let Ok(root) = serde_json::from_slice::<Value>(&bytes) else {
            return results;
        };

        let mut seen_urls = HashSet::new();
        collect_intel_matches(&root, &local_intel, &mut seen_urls, &mut results);
        results
    }

    /// SEC-30: descarga e instala una actualización de fabricante. Hosts y firmante salen de [`vendor_policy`] (fijos en el
    /// binario), nunca de `upd`, que viene del escaneo (de la red) y la vista devuelve tal cual. Todo ejecutable pasa por
    /// Authenticode con el firmante del fabricante y corre dentro de un Job; los temporales se borran al terminar.
    pub fn install_vendor_update(upd: &VendorDriverUpdate) -> CoreResult<String> {
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

        let (wua_updates, wua_note) = match Self::search_driver_updates() {
            Ok(wua) => (wua, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };

        let mut vendor_updates = Self::check_nvidia_updates(devices);
        vendor_updates.extend(Self::check_intel_updates(devices));

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
            if let Ok(mut found) = Self::search_microsoft_catalog(&short_id)
                && let Some(first) = found.drain(..).next()
            {
                catalog_matches.push(first);
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
            notes: Vec::new(),
        }
    }
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
) -> CoreResult<String> {
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
        Ok(format!(
            "Instalador oficial '{title}' verificado y terminado."
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

/// Carpeta temporal de un paquete de fabricante: se borra al soltarse (SEC-34).
struct VendorScratch {
    root: PathBuf,
    dir: PathBuf,
}

impl Drop for VendorScratch {
    fn drop(&mut self) {
        let _ = delete_tree_verified(&self.dir, &self.root, 64);
    }
}

/// SEC-30: paquete ZIP de Intel. Se baja con el cliente HTTPS común validando el host final, se extrae con el
/// archivador endurecido (rutas, symlinks, cuota) y solo se ejecuta un instalador firmado por Intel; si no hay
/// instalador, `pnputil` agrega los INF del paquete.
fn install_vendor_zip(
    upd: &VendorDriverUpdate,
    safe_id: &str,
    hosts: &'static [&'static str],
    signer: &'static str,
) -> CoreResult<String> {
    use std::sync::atomic::AtomicBool;

    let vendor_dir = ensure_admin_only_data_dir()?
        .join("DriverCache")
        .join("Vendor");
    std::fs::create_dir_all(&vendor_dir)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let scratch = VendorScratch {
        root: vendor_dir.clone(),
        dir: vendor_dir.join(format!("{safe_id}_{nonce}")),
    };
    create_admin_only_dir(&scratch.dir)?;

    let zip_path = scratch.dir.join("package.zip");
    {
        let resp = http::get(
            &upd.download_url,
            http::Timeouts {
                connect_ms: 30_000,
                io_ms: 120_000,
            },
        )?;
        resp.require_ok()?;
        validate_https_host(&resp.final_url()?, hosts)?;
        let (mut f, _) = create_verified_new_file(&zip_path, true)?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = resp.read_chunk(&mut buf)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > MAX_VENDOR_ZIP_BYTES {
                return Err(CoreError::Trust {
                    message: format!("El paquete de '{}' excede el tamaño máximo", upd.title),
                    source: None,
                });
            }
            f.write_all(&buf[..n])?;
        }
        f.sync_all()?;
    }

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
        Ok(format!(
            "Driver '{}' instalado vía Driver Store.",
            upd.title
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
