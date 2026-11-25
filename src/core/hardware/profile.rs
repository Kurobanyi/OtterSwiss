//! Perfil de hardware para decidir el nivel del perfil de optimización (`HardwareTier`).

use std::collections::HashSet;
use std::ptr;

use serde::{Deserialize, Serialize};
use sysinfo::System;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY,
    STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
};
use windows::Win32::System::IO::DeviceIoControl;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
use winreg::RegKey;

use crate::core::platform::handles::SafeHandle;
use crate::core::platform::memory::AlignedBuf;
use crate::core::platform::power::detect_battery_presence;
use crate::core::platform::wide::{read_cstr_at, to_wide_null};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareTier {
    LowEnd,
    MidRange,
    HighEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub logical_cores: usize,
    pub total_ram_mb: u64,
    pub is_laptop_or_battery: bool,
    pub tier: HardwareTier,
    #[serde(default)]
    pub cpu_name: String,
    #[serde(default)]
    pub gpu_names: Vec<String>,
    #[serde(default)]
    pub motherboard: String,
    #[serde(default)]
    pub storage_drives: Vec<String>,
}

impl HardwareProfile {
    pub fn detect() -> Self {
        let mut sys = System::new();
        sys.refresh_memory();
        sys.refresh_cpu_all();

        let logical_cores = sys.cpus().len().max(1);
        let total_ram_mb = sys.total_memory() / (1024 * 1024);
        let is_laptop_or_battery = detect_battery_presence();

        let tier = if logical_cores >= 8 && total_ram_mb >= 14_336 {
            HardwareTier::HighEnd
        } else if logical_cores >= 4 && total_ram_mb >= 7_168 {
            HardwareTier::MidRange
        } else {
            HardwareTier::LowEnd
        };

        let cpu_name = detect_cpu_name(&sys);
        let gpu_names = detect_gpu_names();
        let motherboard = detect_motherboard_name();
        let storage_drives = detect_storage_drives();

        Self {
            logical_cores,
            total_ram_mb,
            is_laptop_or_battery,
            tier,
            cpu_name,
            gpu_names,
            motherboard,
            storage_drives,
        }
    }
}

fn detect_cpu_name(sys: &System) -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(k) = hklm.open_subkey_with_flags(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        KEY_READ | KEY_WOW64_64KEY,
    ) {
        if let Ok(name) = k.get_value::<String, _>("ProcessorNameString") {
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    sys.cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Procesador x86_64".into())
}

fn detect_motherboard_name() -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(k) = hklm.open_subkey_with_flags(
        r"HARDWARE\DESCRIPTION\System\BIOS",
        KEY_READ | KEY_WOW64_64KEY,
    ) {
        let mfg = k
            .get_value::<String, _>("BaseBoardManufacturer")
            .unwrap_or_default()
            .trim()
            .to_string();
        let prod = k
            .get_value::<String, _>("BaseBoardProduct")
            .unwrap_or_default()
            .trim()
            .to_string();
        let sys_prod = k
            .get_value::<String, _>("SystemProductName")
            .unwrap_or_default()
            .trim()
            .to_string();

        let board = match (mfg.is_empty(), prod.is_empty()) {
            (false, false) => format!("{} {}", mfg, prod),
            (false, true) => mfg,
            (true, false) => prod,
            (true, true) => sys_prod,
        };
        if !board.is_empty() {
            return board;
        }
    }
    "Placa Base Estándar".into()
}

fn detect_gpu_names() -> Vec<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let mut gpus = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(class_key) = hklm.open_subkey_with_flags(base, KEY_READ | KEY_WOW64_64KEY) {
        for idx in 0..16 {
            let sub = format!("{:04}", idx);
            if let Ok(dev_key) = class_key.open_subkey_with_flags(&sub, KEY_READ | KEY_WOW64_64KEY)
            {
                if let Ok(desc) = dev_key.get_value::<String, _>("DriverDesc") {
                    let clean = desc.trim().to_string();
                    if !clean.is_empty() && seen.insert(clean.clone()) {
                        gpus.push(clean);
                    }
                }
            }
        }
    }

    if gpus.len() > 1 {
        gpus.retain(|g| !g.eq_ignore_ascii_case("Microsoft Basic Display Adapter"));
    }
    if gpus.is_empty() {
        gpus.push("Adaptador de pantalla estándar".into());
    }
    gpus
}

fn detect_storage_drives() -> Vec<String> {
    let mut drives = Vec::new();
    for drive_num in 0..8 {
        let path_str = format!(r"\\.\PhysicalDrive{}", drive_num);
        let wide = to_wide_null(&path_str);
        let raw = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            )
        };
        let Some(handle) = SafeHandle::new(raw.unwrap_or(HANDLE(ptr::null_mut()))) else {
            continue;
        };

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0; 1],
        };

        let mut buf = AlignedBuf::zeroed(1024);
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle.raw(),
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(&query as *const _ as *const _),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(buf.as_mut_ptr() as *mut _),
                buf.len() as u32,
                Some(&mut returned),
                None,
            )
        };

        if ok.is_err() || (returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            continue;
        }

        let valid = &buf[..(returned as usize).min(buf.len())];
        let desc = unsafe { &*(valid.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
        let vendor = read_cstr_at(valid, desc.VendorIdOffset);
        let product = read_cstr_at(valid, desc.ProductIdOffset);

        let bus_label = match desc.BusType.0 {
            17 => "NVMe",
            11 => "SATA",
            7 => "USB",
            8 => "RAID",
            1 | 3 | 10 => "SCSI/ATA",
            _ => "Disco",
        };

        let model = match (vendor.is_empty(), product.is_empty()) {
            (false, false)
                if !product
                    .to_ascii_lowercase()
                    .starts_with(&vendor.to_ascii_lowercase()) =>
            {
                format!("{} {}", vendor, product)
            }
            (_, false) => product,
            (false, true) => vendor,
            (true, true) => format!("Unidad Física {}", drive_num),
        };

        drives.push(format!("[{}] {}", bus_label, model));
    }
    drives
}
