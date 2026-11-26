//! Registro de Windows: colmena, vista de 32 o 64 bits y tipos de valor. Sin reglas de la app.

use std::io;

use serde::{Deserialize, Serialize};
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, KEY_WRITE,
};
use winreg::{RegKey, RegValue};

use crate::core::error::{CoreError, CoreResult};

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

/// Mensaje de `RegLocation::ensure_writable`; `ui/i18n/core_text.rs` lo traduce.
pub const HKCU_OTHER_ACCOUNT: &str = "Ajuste de usuario omitido: OtterSwiss corre con otra cuenta que la del escritorio y lo aplicaría al perfil equivocado. Ábrelo con tu propia cuenta de administrador.";

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

    /// SYS-24: HKCU solo se escribe si el proceso corre con la cuenta del escritorio; elevado «sobre el hombro» con otra
    /// cuenta, el ajuste caería en el perfil del administrador.
    pub fn ensure_writable(&self) -> CoreResult<()> {
        if self.hive == RegHive::Hkcu && crate::core::platform::elevation::shell_user_differs() {
            return Err(CoreError::Validation(HKCU_OTHER_ACCOUNT.to_string()));
        }
        Ok(())
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

/// REG_SZ, REG_EXPAND_SZ, REG_BINARY, REG_DWORD y REG_QWORD: los únicos tipos que guardan los respaldos de registro
/// (el del perfil y el del Debloater). Otro tipo solo puede venir de un respaldo manipulado.
pub const ALLOWED_SNAPSHOT_TYPES: [u32; 5] = [1, 2, 3, 4, 11];

/// Tamaño máximo de un valor guardado en un respaldo: los ajustes son DWORD, QWORD o cadenas cortas.
pub const MAX_SNAPSHOT_VALUE_BYTES: usize = 1024;

/// ¿Un valor de un respaldo tiene un tipo y un tamaño que la app pudo haber guardado? (AR-02: una sola regla.)
pub fn is_storable_value(vtype: u32, len: usize) -> bool {
    ALLOWED_SNAPSHOT_TYPES.contains(&vtype) && len <= MAX_SNAPSHOT_VALUE_BYTES
}

fn registry_error(path: String, what: &str, e: io::Error) -> CoreError {
    CoreError::Registry {
        path,
        message: format!("{what}: {e}"),
        source: Some(Box::new(e)),
    }
}

/// Lee el valor tal como está (tipo y bytes). `None` si la clave o el valor no existen; cualquier otro error se
/// propaga, porque un error no puede guardarse como «no existía».
pub fn read_raw_value(
    location: RegLocation,
    subkey: &str,
    value_name: &str,
) -> CoreResult<Option<RegValue<'static>>> {
    let key = match location
        .root()
        .open_subkey_with_flags(subkey, location.flags(KEY_READ))
    {
        Ok(key) => key,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(registry_error(
                subkey.to_string(),
                "No se pudo abrir la clave original",
                e,
            ));
        }
    };
    match key.get_raw_value(value_name) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(registry_error(
            format!("{subkey}\\{value_name}"),
            "No se pudo leer el valor original",
            e,
        )),
    }
}

/// Escribe un valor con su tipo y bytes exactos, creando la clave si falta.
pub fn write_raw_value(
    location: RegLocation,
    subkey: &str,
    value_name: &str,
    vtype: u32,
    bytes: &[u8],
) -> CoreResult<()> {
    location.ensure_writable()?;
    let (key, _) = location
        .root()
        .create_subkey_with_flags(subkey, location.flags(KEY_READ | KEY_WRITE))
        .map_err(|e| registry_error(subkey.to_string(), "No se pudo abrir la clave", e))?;
    key.set_raw_value(
        value_name,
        &RegValue {
            bytes: bytes.into(),
            vtype: u32_to_reg_type(vtype),
        },
    )
    .map_err(|e| {
        registry_error(
            format!("{subkey}\\{value_name}"),
            "No se pudo restaurar el valor",
            e,
        )
    })
}

/// Borra un valor; que la clave o el valor ya no existan no es un error.
pub fn delete_value_if_present(
    location: RegLocation,
    subkey: &str,
    value_name: &str,
) -> CoreResult<()> {
    location.ensure_writable()?;
    let key = match location
        .root()
        .open_subkey_with_flags(subkey, location.flags(KEY_READ | KEY_WRITE))
    {
        Ok(key) => key,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(registry_error(
                subkey.to_string(),
                "No se pudo abrir la clave",
                e,
            ));
        }
    };
    match key.delete_value(value_name) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(registry_error(
            format!("{subkey}\\{value_name}"),
            "No se pudo borrar el valor creado por OtterSwiss",
            e,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winreg::enums::KEY_READ;

    /// SYS-24: con la cuenta del escritorio (el caso normal) HKCU y HKLM se pueden escribir; el bloqueo solo salta con otra
    /// cuenta, y si saltara aquí ningún ajuste de usuario se aplicaría.
    #[test]
    fn ambas_colmenas_son_escribibles_con_la_cuenta_del_escritorio() {
        assert!(RegLocation::native(RegHive::Hkcu).ensure_writable().is_ok());
        assert!(RegLocation::native(RegHive::Hklm).ensure_writable().is_ok());
    }

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

    /// AR-02: una sola regla para los dos respaldos de registro: tipos conocidos y valores cortos.
    #[test]
    fn solo_se_guardan_tipos_conocidos_y_valores_cortos() {
        assert!(is_storable_value(4, 4));
        assert!(is_storable_value(1, MAX_SNAPSHOT_VALUE_BYTES));
        assert!(!is_storable_value(1, MAX_SNAPSHOT_VALUE_BYTES + 1));
        assert!(!is_storable_value(7, 4)); // REG_MULTI_SZ
    }
}
