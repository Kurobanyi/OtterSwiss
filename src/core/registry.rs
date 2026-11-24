use std::io;
use winreg::enums::{KEY_READ, KEY_WRITE};
use winreg::{RegKey, RegValue};

use crate::core::backup::RegistrySnapshot;
use crate::core::error::{CoreError, CoreResult};
use crate::core::tweaks_catalog::{is_allowed_registry_entry, RegHive, RegistryTweakSpec};

fn root_key(hive: RegHive) -> RegKey {
    match hive {
        RegHive::Hklm => RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE),
        RegHive::Hkcu => RegKey::predef(winreg::enums::HKEY_CURRENT_USER),
    }
}

/// Captura el estado previo exacto `(vtype, raw_bytes)` distinguiendo `NotFound`
/// de errores reales de permisos o corrupción.
pub fn capture_registry_tweak(spec: &RegistryTweakSpec) -> CoreResult<RegistrySnapshot> {
    if !is_allowed_registry_entry(spec.hive, spec.subkey, spec.value_name) {
        return Err(CoreError::Validation(format!(
            "Clave de registro fuera de la tabla única: {:?}\\{}\\{}",
            spec.hive, spec.subkey, spec.value_name
        )));
    }

    let root = root_key(spec.hive);
    let applied_bytes = spec.value.to_bytes();

    match root.open_subkey_with_flags(spec.subkey, KEY_READ) {
        Ok(key) => match key.get_raw_value(spec.value_name) {
            Ok(raw_val) => Ok(RegistrySnapshot {
                hive: spec.hive,
                subkey: spec.subkey.to_string(),
                value_name: spec.value_name.to_string(),
                existed: true,
                key_created: false,
                value_type: Some(reg_type_to_u32(&raw_val.vtype)),
                raw_bytes: Some(raw_val.bytes),
                applied_bytes,
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(RegistrySnapshot {
                hive: spec.hive,
                subkey: spec.subkey.to_string(),
                value_name: spec.value_name.to_string(),
                existed: false,
                key_created: false,
                value_type: None,
                raw_bytes: None,
                applied_bytes,
            }),
            Err(e) => Err(CoreError::Registry {
                path: format!(
                    "{}\\{}\\{}",
                    spec.hive.as_str(),
                    spec.subkey,
                    spec.value_name
                ),
                message: format!("Error leyendo valor previo: {}", e),
                source: Some(e),
            }),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(RegistrySnapshot {
            hive: spec.hive,
            subkey: spec.subkey.to_string(),
            value_name: spec.value_name.to_string(),
            existed: false,
            key_created: true,
            value_type: None,
            raw_bytes: None,
            applied_bytes,
        }),
        Err(e) => Err(CoreError::Registry {
            path: format!("{}\\{}", spec.hive.as_str(), spec.subkey),
            message: format!("Error abriendo subclave: {}", e),
            source: Some(e),
        }),
    }
}

/// Aplica un ajuste validado tras haber persistido su snapshot en `backup.json`.
pub fn apply_registry_tweak(spec: &RegistryTweakSpec) -> CoreResult<()> {
    if !is_allowed_registry_entry(spec.hive, spec.subkey, spec.value_name) {
        return Err(CoreError::Validation(format!(
            "Clave de registro no permitida: {:?}\\{}",
            spec.hive, spec.subkey
        )));
    }

    let root = root_key(spec.hive);
    let (key, _) = root
        .create_subkey_with_flags(spec.subkey, KEY_READ | KEY_WRITE)
        .map_err(|e| CoreError::Registry {
            path: format!("{}\\{}", spec.hive.as_str(), spec.subkey),
            message: format!("No se pudo abrir/crear subclave con KEY_WRITE: {}", e),
            source: Some(e),
        })?;

    let raw_val = RegValue {
        bytes: spec.value.to_bytes(),
        vtype: u32_to_reg_type(spec.value.kind().win32_type()),
    };

    key.set_raw_value(spec.value_name, &raw_val)
        .map_err(|e| CoreError::Registry {
            path: format!(
                "{}\\{}\\{}",
                spec.hive.as_str(),
                spec.subkey,
                spec.value_name
            ),
            message: format!("Error escribiendo valor: {}", e),
            source: Some(e),
        })
}

/// Revierte un snapshot validado. Comprueba si el usuario modificó el valor posteriormente
/// antes de pisarlo y propaga cualquier error real de escritura/borrado.
pub fn restore_registry_snapshot(snap: &RegistrySnapshot, force: bool) -> CoreResult<bool> {
    if !snap.is_valid() {
        return Err(CoreError::Validation(format!(
            "Snapshot de registro inválido rechazado: {:?}\\{}\\{}",
            snap.hive, snap.subkey, snap.value_name
        )));
    }

    let root = root_key(snap.hive);
    let key = match root.open_subkey_with_flags(&snap.subkey, KEY_READ | KEY_WRITE) {
        Ok(k) => k,
        Err(e) if e.kind() == io::ErrorKind::NotFound && !snap.existed => return Ok(true),
        Err(e) => {
            return Err(CoreError::Registry {
                path: format!("{}\\{}", snap.hive.as_str(), snap.subkey),
                message: format!("Error abriendo subclave para revertir: {}", e),
                source: Some(e),
            });
        }
    };

    if !force {
        if let Ok(current) = key.get_raw_value(&snap.value_name) {
            if current.bytes != snap.applied_bytes {
                return Ok(false);
            }
        }
    }

    if snap.existed {
        let vtype = snap.value_type.ok_or_else(|| {
            CoreError::Validation("Snapshot inconsistente: existed=true sin value_type".into())
        })?;
        let bytes = snap.raw_bytes.clone().ok_or_else(|| {
            CoreError::Validation("Snapshot inconsistente: existed=true sin raw_bytes".into())
        })?;
        let raw_val = RegValue {
            bytes,
            vtype: u32_to_reg_type(vtype),
        };
        key.set_raw_value(&snap.value_name, &raw_val)
            .map_err(|e| CoreError::Registry {
                path: format!(
                    "{}\\{}\\{}",
                    snap.hive.as_str(),
                    snap.subkey,
                    snap.value_name
                ),
                message: format!("Error restaurando valor previo: {}", e),
                source: Some(e),
            })?;
    } else {
        match key.delete_value(&snap.value_name) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(CoreError::Registry {
                    path: format!(
                        "{}\\{}\\{}",
                        snap.hive.as_str(),
                        snap.subkey,
                        snap.value_name
                    ),
                    message: format!("Error eliminando valor creado por OtterSwiss: {}", e),
                    source: Some(e),
                });
            }
        }
        // Solo se borra la clave creada por OtterSwiss si quedó vacía: otra política o programa puede
        // haber escrito valores en ella después.
        if snap.key_created {
            let is_empty = key
                .query_info()
                .map(|i| i.values == 0 && i.sub_keys == 0)
                .unwrap_or(false);
            drop(key);
            if is_empty {
                let _ = root.delete_subkey(&snap.subkey);
            }
        }
    }

    Ok(true)
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
