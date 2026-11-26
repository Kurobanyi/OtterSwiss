//! Hardware IDs y versiones de drivers: funciones puras, sin Windows ni red.

use crate::core::platform::version::NumericVersion;

/// ERR-23: valor de `key` en un Hardware ID en mayúsculas (`PCI\VEN_1002&DEV_744C&SUBSYS_…`): lo que sigue a la clave
/// hasta `&` o `\`. Así `ACPI\VEN_INT&DEV_33A0` da `INT` y no `INT&`.
fn hwid_field<'a>(up: &'a str, key: &str) -> Option<&'a str> {
    let start = up.find(key)? + key.len();
    let rest = &up[start..];
    let end = rest.find(['&', '\\']).unwrap_or(rest.len());
    let value = &rest[..end];
    (!value.is_empty()).then_some(value)
}

/// ID corto para buscar en el Catálogo de Microsoft: `PCI\VEN_x&DEV_y` o `USB\VID_x&PID_y` (el prefijo del bus solo
/// se conserva para PCI y USB) y, si no hay ninguno de esos pares, los dos primeros campos. `None` si `raw` está vacío.
pub(super) fn short_hw_id_of(raw: &str) -> Option<String> {
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

pub(super) fn ven_dev_of(raw: &str) -> Option<(String, String, Option<String>)> {
    let up = raw.to_ascii_uppercase();
    let ven = hwid_field(&up, "VEN_").or_else(|| hwid_field(&up, "VID_"))?;
    let dev = hwid_field(&up, "DEV_").or_else(|| hwid_field(&up, "PID_"))?;
    let subsys = hwid_field(&up, "SUBSYS_").map(str::to_string);
    Some((ven.to_string(), dev.to_string(), subsys))
}

/// ERR-23: si alguna de las dos no es numérica no se sabe cuál es más nueva; no se ofrece la actualización.
pub(super) fn is_version_newer(candidate: &str, current: &str) -> bool {
    NumericVersion::is_newer(candidate, current).unwrap_or(false)
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
}
