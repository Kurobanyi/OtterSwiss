//! AR-24: tabla única de fabricantes por ID de PCI/USB, para el resumen de GPU y la lista de dispositivos.

/// Nombre del fabricante de un ID de vendor PCI (o USB: Logitech y Razer), si es uno conocido.
pub fn vendor_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x8086 => "Intel",
        0x8087 => "Intel Wireless / BT",
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x10EC => "Realtek",
        0x14C3 => "MediaTek",
        0x17CB => "Qualcomm",
        0x1B21 => "ASMedia",
        0x144D => "Samsung",
        0x1987 => "Phison",
        0x15B7 => "WD / SanDisk",
        0xC0A9 => "Micron / Crucial",
        0x2646 => "Kingston",
        0x1912 => "Renesas",
        0x1106 => "VIA",
        0x046D => "Logitech",
        0x1532 => "Razer",
        0x1B4B => "Marvell",
        _ => return None,
    })
}

/// [`vendor_name`] desde el texto hexadecimal de un Hardware ID (`"10DE"`).
pub fn vendor_name_hex(hex: &str) -> Option<&'static str> {
    u16::from_str_radix(hex, 16).ok().and_then(vendor_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabricantes_conocidos_y_desconocidos() {
        assert_eq!(vendor_name(0x10DE), Some("NVIDIA"));
        assert_eq!(vendor_name_hex("1002"), Some("AMD"));
        assert_eq!(vendor_name_hex("10de"), Some("NVIDIA"));
        assert_eq!(vendor_name_hex("FFFF"), None);
        assert_eq!(vendor_name_hex("INT"), None);
    }
}
