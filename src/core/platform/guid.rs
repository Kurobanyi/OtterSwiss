//! GUID en texto (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`), como los usan los planes de energía y las interfaces de red.

use windows::core::GUID;

use crate::core::error::{CoreError, CoreResult};

/// Parser estricto de GUID (exactamente 36 caracteres: 8-4-4-4-12 hexadecimales).
pub fn parse_guid_strict(s: &str) -> CoreResult<GUID> {
    let trimmed = s.trim();
    if trimmed.len() != 36 {
        return Err(CoreError::Validation(format!(
            "GUID inválido (longitud != 36): '{}'",
            trimmed
        )));
    }
    let bytes = trimmed.as_bytes();
    for &idx in &[8usize, 13, 18, 23] {
        if bytes[idx] != b'-' {
            return Err(CoreError::Validation(format!(
                "GUID inválido (guiones incorrectos): '{}'",
                trimmed
            )));
        }
    }

    let d1 = u32::from_str_radix(&trimmed[0..8], 16)
        .map_err(|_| CoreError::Validation(format!("GUID hex inválido: '{}'", trimmed)))?;
    let d2 = u16::from_str_radix(&trimmed[9..13], 16)
        .map_err(|_| CoreError::Validation(format!("GUID hex inválido: '{}'", trimmed)))?;
    let d3 = u16::from_str_radix(&trimmed[14..18], 16)
        .map_err(|_| CoreError::Validation(format!("GUID hex inválido: '{}'", trimmed)))?;

    let mut d4 = [0u8; 8];
    d4[0] = u8::from_str_radix(&trimmed[19..21], 16)
        .map_err(|_| CoreError::Validation(format!("GUID hex inválido: '{}'", trimmed)))?;
    d4[1] = u8::from_str_radix(&trimmed[21..23], 16)
        .map_err(|_| CoreError::Validation(format!("GUID hex inválido: '{}'", trimmed)))?;
    for i in 0..6 {
        let start = 24 + i * 2;
        d4[2 + i] = u8::from_str_radix(&trimmed[start..start + 2], 16)
            .map_err(|_| CoreError::Validation(format!("GUID hex inválido: '{}'", trimmed)))?;
    }

    Ok(GUID {
        data1: d1,
        data2: d2,
        data3: d3,
        data4: d4,
    })
}

pub fn format_guid(g: &GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7],
    )
}
