//! Registro de Windows: colmena, vista de 32 o 64 bits y tipos de valor. Sin reglas de la app.

use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
use winreg::RegKey;

/// Colmena del registro que la app puede tocar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegHive {
    Hklm,
    Hkcu,
}

impl RegHive {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hklm => "HKLM",
            Self::Hkcu => "HKCU",
        }
    }

    pub fn from_str_strict(s: &str) -> Option<Self> {
        match s {
            "HKLM" => Some(Self::Hklm),
            "HKCU" => Some(Self::Hkcu),
            _ => None,
        }
    }
}

/// Vista del registro en Windows de 64 bits. HKCU no tiene vistas separadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegView {
    /// Vista de 64 bits (la nativa de la app).
    Native,
    /// `WOW6432Node`, donde escriben las apps de 32 bits.
    Wow64_32,
}

/// CL-04: dónde vive una clave: colmena y vista. Reemplaza los pares de booleanos `is_hkcu` y `wow64_32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegLocation {
    pub hive: RegHive,
    pub view: RegView,
}

impl RegLocation {
    pub fn native(hive: RegHive) -> Self {
        Self {
            hive,
            view: RegView::Native,
        }
    }

    pub fn root(&self) -> RegKey {
        RegKey::predef(match self.hive {
            RegHive::Hklm => HKEY_LOCAL_MACHINE,
            RegHive::Hkcu => HKEY_CURRENT_USER,
        })
    }

    /// `access` más el flag de vista que corresponde (en HKLM siempre explícito, para no depender de la
    /// arquitectura del proceso).
    pub fn flags(&self, access: u32) -> u32 {
        match (self.hive, self.view) {
            (RegHive::Hkcu, _) => access,
            (RegHive::Hklm, RegView::Native) => access | KEY_WOW64_64KEY,
            (RegHive::Hklm, RegView::Wow64_32) => access | KEY_WOW64_32KEY,
        }
    }

    /// Argumento `/reg:32` o `/reg:64` de `reg.exe` (solo HKLM).
    pub fn reg_exe_view_arg(&self) -> Option<&'static str> {
        match (self.hive, self.view) {
            (RegHive::Hkcu, _) => None,
            (RegHive::Hklm, RegView::Native) => Some("/reg:64"),
            (RegHive::Hklm, RegView::Wow64_32) => Some("/reg:32"),
        }
    }
}

pub(crate) fn reg_type_to_u32(t: &winreg::enums::RegType) -> u32 {
    use winreg::enums::RegType::*;
    match t {
        REG_NONE => 0,
        REG_SZ => 1,
        REG_EXPAND_SZ => 2,
        REG_BINARY => 3,
        REG_DWORD => 4,
        REG_DWORD_BIG_ENDIAN => 5,
        REG_LINK => 6,
        REG_MULTI_SZ => 7,
        REG_RESOURCE_LIST => 8,
        REG_FULL_RESOURCE_DESCRIPTOR => 9,
        REG_RESOURCE_REQUIREMENTS_LIST => 10,
        REG_QWORD => 11,
    }
}

pub(crate) fn u32_to_reg_type(v: u32) -> winreg::enums::RegType {
    use winreg::enums::RegType::*;
    match v {
        0 => REG_NONE,
        1 => REG_SZ,
        2 => REG_EXPAND_SZ,
        3 => REG_BINARY,
        4 => REG_DWORD,
        5 => REG_DWORD_BIG_ENDIAN,
        6 => REG_LINK,
        7 => REG_MULTI_SZ,
        8 => REG_RESOURCE_LIST,
        9 => REG_FULL_RESOURCE_DESCRIPTOR,
        10 => REG_RESOURCE_REQUIREMENTS_LIST,
        11 => REG_QWORD,
        _ => REG_BINARY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winreg::enums::KEY_READ;

    #[test]
    fn la_vista_solo_aplica_a_hklm() {
        let hkcu = RegLocation {
            hive: RegHive::Hkcu,
            view: RegView::Wow64_32,
        };
        assert_eq!(hkcu.flags(KEY_READ), KEY_READ);
        assert_eq!(hkcu.reg_exe_view_arg(), None);

        let hklm32 = RegLocation {
            hive: RegHive::Hklm,
            view: RegView::Wow64_32,
        };
        assert_eq!(hklm32.flags(KEY_READ), KEY_READ | KEY_WOW64_32KEY);
        assert_eq!(hklm32.reg_exe_view_arg(), Some("/reg:32"));

        let hklm = RegLocation::native(RegHive::Hklm);
        assert_eq!(hklm.flags(KEY_READ), KEY_READ | KEY_WOW64_64KEY);
        assert_eq!(hklm.reg_exe_view_arg(), Some("/reg:64"));
    }
}
