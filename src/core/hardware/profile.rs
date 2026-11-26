//! Perfil de hardware para decidir el nivel del perfil de optimización (`HardwareTier`).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use windows::Win32::System::SystemInformation::{
    GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

use crate::core::platform::power::detect_battery_presence;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareTier {
    LowEnd,
    MidRange,
    HighEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl HardwareTier {
    /// AR-04: el nivel solo depende de núcleos lógicos y RAM; no hace falta leer GPU ni discos para decidirlo.
    pub fn from_cores_and_ram(logical_cores: usize, total_ram_mb: u64) -> Self {
        if logical_cores >= 8 && total_ram_mb >= 14_336 {
            Self::HighEnd
        } else if logical_cores >= 4 && total_ram_mb >= 7_168 {
            Self::MidRange
        } else {
            Self::LowEnd
        }
    }
}

impl HardwareProfile {
    pub fn detect() -> Self {
        let logical_cores = logical_processor_count();
        let total_ram_mb = total_physical_ram_mb();
        Self {
            logical_cores,
            total_ram_mb,
            is_laptop_or_battery: detect_battery_presence(),
            tier: HardwareTier::from_cores_and_ram(logical_cores, total_ram_mb),
            cpu_name: detect_cpu_name(),
            gpu_names: detect_gpu_names(),
            motherboard: detect_motherboard_name(),
            storage_drives: detect_storage_drives(),
        }
    }
}

/// Procesadores lógicos del sistema (`GetSystemInfo`). AR-18: antes salía de `sysinfo`, que traía otra versión del
/// crate `windows`.
fn logical_processor_count() -> usize {
    let mut info = SYSTEM_INFO::default();
    // SAFETY: `info` es un SYSTEM_INFO válido que la función rellena.
    unsafe { GetSystemInfo(&mut info) };
    (info.dwNumberOfProcessors as usize).max(1)
}

/// RAM física total en MB (`GlobalMemoryStatusEx`). 0 si la consulta falla.
fn total_physical_ram_mb() -> u64 {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `status` tiene `dwLength` puesto, como exige la API.
    match unsafe { GlobalMemoryStatusEx(&mut status) } {
        Ok(()) => status.ullTotalPhys / (1024 * 1024),
        Err(_) => 0,
    }
}

fn detect_cpu_name() -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(k) = hklm.open_subkey_with_flags(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        KEY_READ | KEY_WOW64_64KEY,
    ) && let Ok(name) = k.get_value::<String, _>("ProcessorNameString")
    {
        let trimmed = name.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    "Procesador x86_64".into()
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
                && let Ok(desc) = dev_key.get_value::<String, _>("DriverDesc")
            {
                let clean = desc.trim().to_string();
                if !clean.is_empty() && seen.insert(clean.clone()) {
                    gpus.push(clean);
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

/// AR-04: los mismos discos que muestra la pestaña Hardware, con «[bus] modelo».
fn detect_storage_drives() -> Vec<String> {
    crate::core::hardware::specs::storage::read_storage_devices()
        .into_iter()
        .map(|d| format!("[{}] {}", d.bus_type, d.model))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nivel_por_nucleos_y_ram() {
        assert_eq!(
            HardwareTier::from_cores_and_ram(16, 32_768),
            HardwareTier::HighEnd
        );
        assert_eq!(
            HardwareTier::from_cores_and_ram(16, 8_192),
            HardwareTier::MidRange
        );
        assert_eq!(
            HardwareTier::from_cores_and_ram(4, 7_168),
            HardwareTier::MidRange
        );
        assert_eq!(
            HardwareTier::from_cores_and_ram(2, 32_768),
            HardwareTier::LowEnd
        );
    }
}
