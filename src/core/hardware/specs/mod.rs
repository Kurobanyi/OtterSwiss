//! Especificaciones del equipo: CPU, placa, memoria, GPU, discos y sistema operativo.

mod cpu;
mod gpu;
mod smbios;
pub(crate) mod storage;

use cpu::read_cpu_specs;
use gpu::read_dxgi_gpus;
use smbios::read_smbios_board_and_memory;
use storage::read_storage_devices;

use serde::{Deserialize, Serialize};
use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CpuSpecs {
    pub brand: String,
    pub vendor: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub physical_cores: usize,
    pub p_cores: Option<usize>,
    pub e_cores: Option<usize>,
    pub logical_threads: usize,
    pub l1_cache_kb: u32,
    pub l2_cache_kb: u32,
    pub l3_cache_kb: u32,
    pub instruction_sets: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MotherboardSpecs {
    pub manufacturer: String,
    pub product: String,
    pub version: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_date: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MemoryModuleSpec {
    pub slot: String,
    pub populated: bool,
    pub capacity_mb: u32,
    pub memory_type: String,
    pub speed_mts: u32,
    pub manufacturer: String,
    pub part_number: String,
    pub voltage_mv: Option<u16>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MemorySpecs {
    pub total_mb: u64,
    pub populated_slots: usize,
    pub total_slots: usize,
    pub memory_type: String,
    pub effective_speed_mts: u32,
    pub modules: Vec<MemoryModuleSpec>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GpuSpecs {
    pub name: String,
    pub vendor_name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub dedicated_vram_mb: u64,
    pub shared_system_mb: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct StorageSpecs {
    pub device_index: u32,
    pub model: String,
    pub firmware: String,
    pub bus_type: String,
    pub serial: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct OsSpecs {
    pub edition: String,
    pub display_version: String,
    pub build: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SystemSpecs {
    pub os: OsSpecs,
    pub cpu: CpuSpecs,
    pub motherboard: MotherboardSpecs,
    pub memory: MemorySpecs,
    pub gpus: Vec<GpuSpecs>,
    pub storage: Vec<StorageSpecs>,
    /// UI-06: lo que no se pudo leer; el resto de los datos sigue siendo válido.
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl SystemSpecs {
    pub fn to_compact_summary(&self) -> String {
        let gpu_line = self
            .gpus
            .first()
            .map(|g| format!("{} ({} MB VRAM)", g.name, g.dedicated_vram_mb))
            .unwrap_or_else(|| "No detectada".to_string());

        let ram_gb = self.memory.total_mb as f64 / 1024.0;
        format!(
            "OS: {} {} (Build {})\nCPU: {} ({}C/{}T)\nPlaca: {} {} (BIOS {})\nRAM: {:.0} GB {} @ {} MT/s ({}/{} slots)\nGPU: {}",
            self.os.edition,
            self.os.display_version,
            self.os.build,
            self.cpu.brand,
            self.cpu.physical_cores,
            self.cpu.logical_threads,
            self.motherboard.manufacturer,
            self.motherboard.product,
            self.motherboard.bios_version,
            ram_gb,
            self.memory.memory_type,
            self.memory.effective_speed_mts,
            self.memory.populated_slots,
            self.memory.total_slots,
            gpu_line,
        )
    }
}

/// Lee todo lo que se pueda: una fuente que falla deja su sección vacía y un aviso en `warnings` (UI-06).
pub fn collect_system_specs() -> CoreResult<SystemSpecs> {
    let mut warnings = Vec::new();
    let os = read_os_specs();
    let cpu = read_cpu_specs().unwrap_or_else(|e| {
        warnings.push(format!("CPU: {}", e));
        CpuSpecs::default()
    });
    let (motherboard, memory) = read_smbios_board_and_memory().unwrap_or_else(|e| {
        warnings.push(format!("SMBIOS: {}", e));
        (MotherboardSpecs::default(), MemorySpecs::default())
    });
    let gpus = read_dxgi_gpus().unwrap_or_else(|e| {
        warnings.push(format!("GPU (DXGI): {}", e));
        Vec::new()
    });
    let storage = read_storage_devices();

    Ok(SystemSpecs {
        os,
        cpu,
        motherboard,
        memory,
        gpus,
        storage,
        warnings,
    })
}

/// AR-15: guarda las especificaciones en el Escritorio como `OtterSwiss-Hardware-Specs.json`. Se escribe sin seguir
/// junctions (SEC-20: el Escritorio se puede redirigir). Devuelve la ruta escrita.
pub fn export_to_desktop(specs: &SystemSpecs) -> CoreResult<std::path::PathBuf> {
    let desktop = dirs::desktop_dir()
        .ok_or_else(|| CoreError::Validation("No se encontró la carpeta Escritorio".into()))?;
    let json = serde_json::to_vec_pretty(specs)
        .map_err(|e| CoreError::Validation(format!("Error serializando especificaciones: {e}")))?;
    safe_fs::write_file_inside(&desktop, "", "OtterSwiss-Hardware-Specs.json", &json)
}

fn read_os_specs() -> OsSpecs {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(key) = hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") else {
        return OsSpecs {
            edition: "Windows 10/11".into(),
            display_version: "Desconocida".into(),
            build: "N/D".into(),
        };
    };

    let mut edition: String = key
        .get_value("ProductName")
        .unwrap_or_else(|_| "Windows".into());
    let display_version: String = key.get_value("DisplayVersion").unwrap_or_default();
    let current_build: String = key.get_value("CurrentBuild").unwrap_or_default();
    let ubr: u32 = key.get_value("UBR").unwrap_or(0);

    if let Ok(build_num) = current_build.parse::<u32>()
        && build_num >= 22000
        && edition.contains("Windows 10")
    {
        edition = edition.replace("Windows 10", "Windows 11");
    }

    let build = if ubr > 0 && !current_build.is_empty() {
        format!("{current_build}.{ubr}")
    } else {
        current_build
    };

    OsSpecs {
        edition,
        display_version,
        build,
    }
}

#[cfg(test)]
mod tests {
    use super::gpu::keep_gpu_adapter;
    use super::smbios::{get_smbios_str, parse_smbios_type17};

    fn type17(size: u16, mem_type: u8, speed: u16) -> Vec<u8> {
        let mut f = vec![0u8; 0x5C];
        f[0x0C..0x0E].copy_from_slice(&size.to_le_bytes());
        f[0x10] = 1; // ranura = cadena 1
        f[0x12] = mem_type;
        f[0x20..0x22].copy_from_slice(&speed.to_le_bytes());
        f
    }

    #[test]
    fn modulo_ddr4_de_8_gb() {
        let m = parse_smbios_type17(&type17(8192, 0x1A, 3200), &["DIMM A1".to_string()]).unwrap();
        assert!(m.populated);
        assert_eq!(m.capacity_mb, 8192);
        assert_eq!(m.memory_type, "DDR4");
        assert_eq!(m.speed_mts, 3200);
        assert_eq!(m.slot, "DIMM A1");
    }

    #[test]
    fn ranura_vacia_y_tamano_extendido() {
        let vacia = parse_smbios_type17(&type17(0, 0, 0), &[]).unwrap();
        assert!(!vacia.populated);

        let mut ext = type17(0x7FFF, 0x22, 4800);
        ext[0x1C..0x20].copy_from_slice(&65536u32.to_le_bytes());
        assert_eq!(parse_smbios_type17(&ext, &[]).unwrap().capacity_mb, 65536);
    }

    #[test]
    fn voltaje_configurado_en_0x26() {
        let mut f = type17(8192, 0x1A, 3200);
        f[0x26..0x28].copy_from_slice(&1200u16.to_le_bytes());
        f[0x28] = 3; // Memory Technology: no es un voltaje
        assert_eq!(parse_smbios_type17(&f, &[]).unwrap().voltage_mv, Some(1200));
    }

    #[test]
    fn gpu_repetida_y_render_de_software_se_descartan() {
        let mut seen = std::collections::HashSet::new();
        let rx = (0x1002, 0x747E, 0x475D_1DA2, 0xC8, 16 << 30);
        assert!(keep_gpu_adapter(&mut seen, rx));
        assert!(!keep_gpu_adapter(&mut seen, rx));
        assert!(!keep_gpu_adapter(&mut seen, (0x1414, 0x8C, 0, 0, 0)));
        assert!(keep_gpu_adapter(
            &mut seen,
            (0x10DE, 0x2684, 1, 0, 24 << 30)
        ));
    }

    /// ERR-12: una estructura truncada no provoca pánico.
    #[test]
    fn estructura_truncada_no_entra_en_panico() {
        assert!(parse_smbios_type17(&[0u8; 4], &[]).is_none());
        assert_eq!(get_smbios_str(&[], Some(3)), "");
    }
}
