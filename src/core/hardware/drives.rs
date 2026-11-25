//! AR-04: discos físicos (`\\.\PhysicalDriveN`). Una sola forma de abrirlos y de nombrarlos para especificaciones,
//! perfil de hardware y sensores.

use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::core::PCWSTR;

use crate::core::platform::handles::SafeHandle;
use crate::core::platform::wide::to_wide_null;

/// Cuántos discos físicos se consultan (`PhysicalDrive0` a `PhysicalDrive7`).
pub(crate) const MAX_PHYSICAL_DRIVES: u32 = 8;

/// Abre `\\.\PhysicalDrive{index}` con `access` (0 basta para consultar propiedades). `None` si no existe.
pub(crate) fn open_physical_drive(index: u32, access: u32) -> Option<SafeHandle> {
    let wide = to_wide_null(format!(r"\\.\PhysicalDrive{index}"));
    // SAFETY: `wide` termina en NUL; el handle devuelto pasa a ser de `SafeHandle`.
    let raw = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    };
    raw.ok().and_then(SafeHandle::new)
}

/// Nombre corto del bus (`STORAGE_BUS_TYPE`).
pub(crate) fn bus_label(bus: i32) -> &'static str {
    match bus {
        17 => "NVMe",
        11 => "SATA",
        7 => "USB",
        8 => "RAID",
        1 | 3 | 10 => "SCSI/ATA",
        _ => "Disco",
    }
}

/// Modelo a mostrar: «fabricante producto» salvo que el producto ya empiece por el fabricante. `None` si el disco
/// no informa ninguno de los dos.
pub(crate) fn drive_model(vendor: &str, product: &str) -> Option<String> {
    match (vendor.is_empty(), product.is_empty()) {
        (true, true) => None,
        (false, false)
            if !product
                .to_ascii_lowercase()
                .starts_with(&vendor.to_ascii_lowercase()) =>
        {
            Some(format!("{vendor} {product}"))
        }
        (_, false) => Some(product.to_string()),
        (false, true) => Some(vendor.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modelo_sin_repetir_fabricante() {
        assert_eq!(
            drive_model("Samsung", "SSD 980"),
            Some("Samsung SSD 980".into())
        );
        assert_eq!(
            drive_model("WDC", "wdc WD10EZEX"),
            Some("wdc WD10EZEX".into())
        );
        assert_eq!(drive_model("", "Patriot P300"), Some("Patriot P300".into()));
        assert_eq!(drive_model("", ""), None);
        assert_eq!(bus_label(17), "NVMe");
        assert_eq!(bus_label(99), "Disco");
    }
}
