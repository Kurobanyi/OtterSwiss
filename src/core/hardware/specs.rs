use std::arch::x86_64::{__cpuid, __cpuid_count};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_ID,
    STORAGE_PROPERTY_QUERY, STORAGE_QUERY_TYPE,
};
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GetSystemFirmwareTable, RelationCache, RelationProcessorCore,
    FIRMWARE_TABLE_PROVIDER, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows::Win32::System::IO::DeviceIoControl;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::core::error::CoreError;
use crate::core::platform::safe_fs;
use crate::core::platform::win32::AlignedBuf;

const RSMB_PROVIDER: FIRMWARE_TABLE_PROVIDER = FIRMWARE_TABLE_PROVIDER(0x5253_4D42);

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

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
pub fn collect_system_specs() -> Result<SystemSpecs, CoreError> {
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

pub fn export_specs_json(specs: &SystemSpecs, destination: &Path) -> Result<(), CoreError> {
    let json = serde_json::to_string_pretty(specs)
        .map_err(|e| CoreError::Validation(format!("Error serializando especificaciones: {e}")))?;
    safe_fs::atomic_write_new(destination, json.as_bytes())
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

    if let Ok(build_num) = current_build.parse::<u32>() {
        if build_num >= 22000 && edition.contains("Windows 10") {
            edition = edition.replace("Windows 10", "Windows 11");
        }
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

fn read_cpu_specs() -> Result<CpuSpecs, CoreError> {
    let leaf0 = __cpuid(0);
    let max_basic_leaf = leaf0.eax;

    let mut vendor_bytes = Vec::with_capacity(12);
    vendor_bytes.extend_from_slice(&leaf0.ebx.to_le_bytes());
    vendor_bytes.extend_from_slice(&leaf0.edx.to_le_bytes());
    vendor_bytes.extend_from_slice(&leaf0.ecx.to_le_bytes());
    let vendor = String::from_utf8_lossy(&vendor_bytes).trim().to_string();

    let (family, model, stepping, mut features) = if max_basic_leaf >= 1 {
        let leaf1 = __cpuid(1);
        let step = leaf1.eax & 0xF;
        let base_model = (leaf1.eax >> 4) & 0xF;
        let base_family = (leaf1.eax >> 8) & 0xF;
        let ext_model = (leaf1.eax >> 16) & 0xF;
        let ext_family = (leaf1.eax >> 20) & 0xFF;

        let eff_family = if base_family == 0xF {
            base_family + ext_family
        } else {
            base_family
        };
        let eff_model = if base_family == 0x6 || base_family == 0xF {
            (ext_model << 4) + base_model
        } else {
            base_model
        };

        let mut isa = Vec::new();
        if (leaf1.edx & (1 << 25)) != 0 {
            isa.push("SSE".into());
        }
        if (leaf1.edx & (1 << 26)) != 0 {
            isa.push("SSE2".into());
        }
        if (leaf1.ecx & (1 << 0)) != 0 {
            isa.push("SSE3".into());
        }
        if (leaf1.ecx & (1 << 9)) != 0 {
            isa.push("SSSE3".into());
        }
        if (leaf1.ecx & (1 << 19)) != 0 {
            isa.push("SSE4.1".into());
        }
        if (leaf1.ecx & (1 << 20)) != 0 {
            isa.push("SSE4.2".into());
        }
        if (leaf1.ecx & (1 << 25)) != 0 {
            isa.push("AES-NI".into());
        }
        if (leaf1.ecx & (1 << 28)) != 0 {
            isa.push("AVX".into());
        }

        if max_basic_leaf >= 7 {
            let leaf7 = __cpuid_count(7, 0);
            if (leaf7.ebx & (1 << 5)) != 0 {
                isa.push("AVX2".into());
            }
            if (leaf7.ebx & (1 << 16)) != 0 {
                isa.push("AVX-512F".into());
            }
            if (leaf7.ebx & (1 << 29)) != 0 {
                isa.push("SHA".into());
            }
        }

        (eff_family, eff_model, step, isa)
    } else {
        (0, 0, 0, Vec::new())
    };

    let max_ext = __cpuid(0x8000_0000).eax;
    let brand = if max_ext >= 0x8000_0004 {
        let mut bytes = Vec::with_capacity(48);
        for leaf in 0x8000_0002..=0x8000_0004 {
            let res = __cpuid(leaf);
            bytes.extend_from_slice(&res.eax.to_le_bytes());
            bytes.extend_from_slice(&res.ebx.to_le_bytes());
            bytes.extend_from_slice(&res.ecx.to_le_bytes());
            bytes.extend_from_slice(&res.edx.to_le_bytes());
        }
        String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    } else {
        "Procesador x86_64".to_string()
    };

    if max_ext >= 0x8000_0001 {
        let ext1 = __cpuid(0x8000_0001);
        if (ext1.edx & (1 << 29)) != 0 {
            features.push("x86-64".into());
        }
    }

    let (physical_cores, p_cores, e_cores, logical_threads) = read_core_topology()?;
    let (l1_bytes, l2_bytes, l3_bytes) = read_cache_sizes()?;

    Ok(CpuSpecs {
        brand,
        vendor,
        family,
        model,
        stepping,
        physical_cores,
        p_cores,
        e_cores,
        logical_threads,
        l1_cache_kb: l1_bytes / 1024,
        l2_cache_kb: l2_bytes / 1024,
        l3_cache_kb: l3_bytes / 1024,
        instruction_sets: features,
    })
}

fn read_core_topology() -> Result<(usize, Option<usize>, Option<usize>, usize), CoreError> {
    let mut len = 0u32;
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len) };
    if len == 0 {
        return Err(CoreError::SystemTool {
            message: "No se pudo determinar el tamaño de topología CPU".into(),
            source: None,
        });
    }

    let mut buf = AlignedBuf::zeroed(len as usize);
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(
                buf.as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
            ),
            &mut len,
        )
        .map_err(|e| CoreError::SystemTool {
            message: format!("Error leyendo núcleos físicos: {e}"),
            source: None,
        })?;
    }

    let mut cores_classes = Vec::new();
    let mut logical_threads = 0usize;
    let mut offset = 0usize;

    while offset + std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() <= len as usize {
        let info = unsafe {
            &*(buf
                .as_ptr()
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>())
        };
        if info.Size == 0 {
            break;
        }
        if info.Relationship == RelationProcessorCore {
            let proc_rel = unsafe { &info.Anonymous.Processor };
            cores_classes.push(proc_rel.EfficiencyClass);
            for idx in 0..(proc_rel.GroupCount as usize) {
                if let Some(group) = proc_rel.GroupMask.get(idx) {
                    logical_threads += group.Mask.count_ones() as usize;
                }
            }
        }
        offset += info.Size as usize;
    }

    let physical_cores = cores_classes.len();
    let max_class = cores_classes.iter().copied().max().unwrap_or(0);
    let (p_cores, e_cores) = if max_class > 0 {
        let p = cores_classes.iter().filter(|&&c| c > 0).count();
        let e = physical_cores.saturating_sub(p);
        (Some(p), Some(e))
    } else {
        (None, None)
    };

    Ok((physical_cores, p_cores, e_cores, logical_threads))
}

fn read_cache_sizes() -> Result<(u32, u32, u32), CoreError> {
    let mut len = 0u32;
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationCache, None, &mut len) };
    if len == 0 {
        return Ok((0, 0, 0));
    }

    let mut buf = AlignedBuf::zeroed(len as usize);
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationCache,
            Some(
                buf.as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
            ),
            &mut len,
        )
        .map_err(|e| CoreError::SystemTool {
            message: format!("Error leyendo cachés CPU: {e}"),
            source: None,
        })?;
    }

    let (mut l1, mut l2, mut l3) = (0u32, 0u32, 0u32);
    let mut offset = 0usize;

    while offset + std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() <= len as usize {
        let info = unsafe {
            &*(buf
                .as_ptr()
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>())
        };
        if info.Size == 0 {
            break;
        }
        if info.Relationship == RelationCache {
            let cache = unsafe { info.Anonymous.Cache };
            match cache.Level {
                1 => l1 = l1.saturating_add(cache.CacheSize),
                2 => l2 = l2.saturating_add(cache.CacheSize),
                3 => l3 = l3.saturating_add(cache.CacheSize),
                _ => {}
            }
        }
        offset += info.Size as usize;
    }

    Ok((l1, l2, l3))
}

fn read_smbios_board_and_memory() -> Result<(MotherboardSpecs, MemorySpecs), CoreError> {
    let required = unsafe { GetSystemFirmwareTable(RSMB_PROVIDER, 0, None) };
    if required <= 8 {
        return Err(CoreError::SystemTool {
            message: "Tabla SMBIOS no disponible en este firmware".into(),
            source: None,
        });
    }

    let mut raw = vec![0u8; required as usize];
    let written = unsafe { GetSystemFirmwareTable(RSMB_PROVIDER, 0, Some(&mut raw)) };
    if written <= 8 {
        return Err(CoreError::SystemTool {
            message: "No se pudo leer el contenido completo de SMBIOS".into(),
            source: None,
        });
    }

    // ERR-12: si el firmware informa más bytes de los pedidos, se acota al buffer en vez de entrar en pánico.
    let end = (written as usize).min(raw.len());
    let data = &raw[8..end];
    let mut board = MotherboardSpecs::default();
    let mut modules = Vec::new();
    let mut cursor = 0usize;

    while cursor + 4 <= data.len() {
        let struct_type = data[cursor];
        let formatted_len = data[cursor + 1] as usize;
        if formatted_len < 4 || cursor + formatted_len > data.len() {
            break;
        }

        let formatted = &data[cursor..cursor + formatted_len];
        let (strings, next_cursor) = parse_smbios_string_set(data, cursor + formatted_len);

        match struct_type {
            0 => {
                board.bios_vendor = get_smbios_str(&strings, formatted.get(0x04).copied());
                board.bios_version = get_smbios_str(&strings, formatted.get(0x05).copied());
                board.bios_date = get_smbios_str(&strings, formatted.get(0x08).copied());
            }
            2 => {
                board.manufacturer = get_smbios_str(&strings, formatted.get(0x04).copied());
                board.product = get_smbios_str(&strings, formatted.get(0x05).copied());
                board.version = get_smbios_str(&strings, formatted.get(0x06).copied());
            }
            17 => {
                if let Some(module) = parse_smbios_type17(formatted, &strings) {
                    modules.push(module);
                }
            }
            127 => break,
            _ => {}
        }

        cursor = next_cursor;
    }

    let total_slots = modules.len();
    let populated_slots = modules.iter().filter(|m| m.populated).count();
    let total_mb: u64 = modules
        .iter()
        .filter(|m| m.populated)
        .map(|m| u64::from(m.capacity_mb))
        .sum();

    let memory_type = modules
        .iter()
        .find(|m| m.populated && !m.memory_type.is_empty())
        .map(|m| m.memory_type.clone())
        .unwrap_or_else(|| "Desconocida".into());

    let effective_speed_mts = modules
        .iter()
        .filter(|m| m.populated && m.speed_mts > 0)
        .map(|m| m.speed_mts)
        .min()
        .unwrap_or(0);

    Ok((
        board,
        MemorySpecs {
            total_mb,
            populated_slots,
            total_slots,
            memory_type,
            effective_speed_mts,
            modules,
        },
    ))
}

fn parse_smbios_string_set(data: &[u8], start: usize) -> (Vec<String>, usize) {
    let mut strings = Vec::new();
    let mut pos = start;

    while pos < data.len() {
        if data[pos] == 0 {
            let next = if pos + 1 < data.len() && data[pos + 1] == 0 {
                pos + 2
            } else {
                pos + 1
            };
            return (strings, next);
        }

        let str_start = pos;
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        let text = String::from_utf8_lossy(&data[str_start..pos])
            .trim()
            .to_string();
        strings.push(text);
        pos += 1;
    }

    (strings, data.len())
}

fn get_smbios_str(strings: &[String], index: Option<u8>) -> String {
    match index {
        Some(idx) if idx > 0 => strings.get((idx - 1) as usize).cloned().unwrap_or_default(),
        _ => String::new(),
    }
}

fn read_u16_le(slice: &[u8], offset: usize) -> Option<u16> {
    let bytes = slice.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(slice: &[u8], offset: usize) -> Option<u32> {
    let bytes = slice.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn parse_smbios_type17(formatted: &[u8], strings: &[String]) -> Option<MemoryModuleSpec> {
    let raw_size = read_u16_le(formatted, 0x0C)?;
    let slot = get_smbios_str(strings, formatted.get(0x10).copied());

    if raw_size == 0 || raw_size == 0xFFFF {
        return Some(MemoryModuleSpec {
            slot: if slot.is_empty() {
                "Slot vacío".into()
            } else {
                slot
            },
            populated: false,
            ..Default::default()
        });
    }

    let capacity_mb = if raw_size == 0x7FFF {
        read_u32_le(formatted, 0x1C).unwrap_or(0) & 0x7FFF_FFFF
    } else if (raw_size & 0x8000) != 0 {
        u32::from(raw_size & 0x7FFF) / 1024
    } else {
        u32::from(raw_size)
    };

    let mem_type_code = formatted.get(0x12).copied().unwrap_or(0);
    let memory_type = match mem_type_code {
        0x18 => "DDR3",
        0x1A => "DDR4",
        0x1B => "LPDDR",
        0x1C => "LPDDR2",
        0x1D => "LPDDR3",
        0x1E => "LPDDR4",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        _ => "RAM",
    }
    .to_string();

    let nominal_speed = u32::from(read_u16_le(formatted, 0x15).unwrap_or(0));
    let configured_speed = u32::from(read_u16_le(formatted, 0x20).unwrap_or(0));
    let ext_configured_speed = read_u32_le(formatted, 0x54).unwrap_or(0);

    let speed_mts = if ext_configured_speed > 0 {
        ext_configured_speed
    } else if configured_speed > 0 {
        configured_speed
    } else {
        nominal_speed
    };

    let manufacturer = get_smbios_str(strings, formatted.get(0x17).copied());
    let part_number = get_smbios_str(strings, formatted.get(0x1A).copied());
    // ERR-12: el voltaje configurado está en 0x26 (0x28 es «Memory Technology»).
    let voltage_mv = read_u16_le(formatted, 0x26).filter(|&v| v > 0);

    Some(MemoryModuleSpec {
        slot,
        populated: true,
        capacity_mb,
        memory_type,
        speed_mts,
        manufacturer,
        part_number,
        voltage_mv,
    })
}

/// (proveedor, dispositivo, subsistema, revisión, VRAM dedicada) de un adaptador DXGI.
type GpuKey = (u32, u32, u32, u32, usize);

/// UI-16: descarta «Microsoft Basic Render Driver» (1414:008C) y la segunda vista del mismo hardware que
/// crea un controlador de pantalla virtual (mismo hardware, otro LUID). Dos GPU idénticas reales se ven como una.
fn keep_gpu_adapter(seen: &mut std::collections::HashSet<GpuKey>, key: GpuKey) -> bool {
    const MICROSOFT: u32 = 0x1414;
    const BASIC_RENDER: u32 = 0x8C;
    if key.0 == MICROSOFT && key.1 == BASIC_RENDER {
        return false;
    }
    seen.insert(key)
}

fn read_dxgi_gpus() -> Result<Vec<GpuSpecs>, CoreError> {
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|e| CoreError::SystemTool {
            message: format!("Error inicializando DXGI: {e}"),
            source: None,
        })?;

    let mut gpus = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut index = 0u32;

    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };

        if (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32)) != 0 {
            continue;
        }
        let key = (
            desc.VendorId,
            desc.DeviceId,
            desc.SubSysId,
            desc.Revision,
            desc.DedicatedVideoMemory,
        );
        if !keep_gpu_adapter(&mut seen, key) {
            continue;
        }

        let end = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..end])
            .trim()
            .to_string();

        let vendor_name = match desc.VendorId {
            0x1002 => "AMD",
            0x10DE => "NVIDIA",
            0x8086 => "Intel",
            _ => "Otro",
        }
        .to_string();

        gpus.push(GpuSpecs {
            name,
            vendor_name,
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            dedicated_vram_mb: (desc.DedicatedVideoMemory as u64) / (1024 * 1024),
            shared_system_mb: (desc.SharedSystemMemory as u64) / (1024 * 1024),
        });
    }

    Ok(gpus)
}

fn read_storage_devices() -> Vec<StorageSpecs> {
    let mut drives = Vec::new();

    for idx in 0..8u32 {
        let path_str = format!(r"\\.\PhysicalDrive{idx}");
        let wide: Vec<u16> = OsStr::new(&path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(wide.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE::default(),
            )
        };

        let Ok(raw_handle) = handle else {
            continue;
        };
        if raw_handle == INVALID_HANDLE_VALUE {
            continue;
        }
        let _guard = OwnedHandle(raw_handle);

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: STORAGE_PROPERTY_ID(0),
            QueryType: STORAGE_QUERY_TYPE(0),
            AdditionalParameters: [0],
        };

        let mut out_buf = AlignedBuf::zeroed(1024);
        let mut bytes_returned = 0u32;

        let ok = unsafe {
            DeviceIoControl(
                raw_handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some((&query as *const STORAGE_PROPERTY_QUERY).cast()),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(out_buf.as_mut_ptr().cast()),
                out_buf.len() as u32,
                Some(&mut bytes_returned),
                None,
            )
            .is_ok()
        };

        if !ok || (bytes_returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            continue;
        }

        let desc = unsafe { &*(out_buf.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>()) };
        let valid_slice = &out_buf[..bytes_returned as usize];

        let vendor = read_cstr_at(valid_slice, desc.VendorIdOffset as usize);
        let product = read_cstr_at(valid_slice, desc.ProductIdOffset as usize);
        let firmware = read_cstr_at(valid_slice, desc.ProductRevisionOffset as usize);
        let serial = read_cstr_at(valid_slice, desc.SerialNumberOffset as usize);

        let model = if !vendor.is_empty() && !product.starts_with(&vendor) {
            format!("{vendor} {product}")
        } else {
            product
        };

        let bus_type = match desc.BusType.0 {
            17 => "NVMe",
            11 => "SATA",
            7 => "USB",
            8 => "RAID",
            _ => "Almacenamiento",
        }
        .to_string();

        if !model.is_empty() {
            drives.push(StorageSpecs {
                device_index: idx,
                model,
                firmware,
                bus_type,
                serial,
            });
        }
    }

    drives
}

fn read_cstr_at(buf: &[u8], offset: usize) -> String {
    if offset == 0 || offset >= buf.len() {
        return String::new();
    }
    let slice = &buf[offset..];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
