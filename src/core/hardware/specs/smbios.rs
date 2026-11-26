//! Tabla SMBIOS del firmware: placa base (tipo 2) y módulos de memoria (tipo 17).

use windows::Win32::System::SystemInformation::{FIRMWARE_TABLE_PROVIDER, GetSystemFirmwareTable};

use super::{MemoryModuleSpec, MemorySpecs, MotherboardSpecs};
use crate::core::error::{CoreError, CoreResult};

const RSMB_PROVIDER: FIRMWARE_TABLE_PROVIDER = FIRMWARE_TABLE_PROVIDER(0x5253_4D42);

pub(super) fn read_smbios_board_and_memory() -> CoreResult<(MotherboardSpecs, MemorySpecs)> {
    // SAFETY: el búfer tiene la longitud que se pasa; con longitud 0 solo informa el tamaño.
    let required = unsafe { GetSystemFirmwareTable(RSMB_PROVIDER, 0, None) };
    if required <= 8 {
        return Err(CoreError::system_tool_msg(
            "Tabla SMBIOS no disponible en este firmware",
        ));
    }

    let mut raw = vec![0u8; required as usize];
    // SAFETY: el búfer tiene la longitud que se pasa; con longitud 0 solo informa el tamaño.
    let written = unsafe { GetSystemFirmwareTable(RSMB_PROVIDER, 0, Some(&mut raw)) };
    if written <= 8 {
        return Err(CoreError::system_tool_msg(
            "No se pudo leer el contenido completo de SMBIOS",
        ));
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

pub(super) fn get_smbios_str(strings: &[String], index: Option<u8>) -> String {
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

pub(super) fn parse_smbios_type17(
    formatted: &[u8],
    strings: &[String],
) -> Option<MemoryModuleSpec> {
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
