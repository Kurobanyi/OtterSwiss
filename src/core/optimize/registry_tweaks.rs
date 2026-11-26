//! Ajustes de registro del perfil de optimización: capturar, aplicar y restaurar solo entradas de la tabla.

use std::io;
use winreg::RegValue;
use winreg::enums::{KEY_READ, KEY_WRITE};

use crate::core::error::{CoreError, CoreResult};
use crate::core::optimize::backup::RegistrySnapshot;
use crate::core::optimize::tweaks_catalog::{RegistryTweakSpec, is_allowed_registry_entry};
use crate::core::platform::registry::{RegLocation, reg_type_to_u32, u32_to_reg_type};

pub fn capture_registry_tweak(spec: &RegistryTweakSpec) -> CoreResult<RegistrySnapshot> {
    if !is_allowed_registry_entry(spec.hive, spec.subkey, spec.value_name) {
        return Err(CoreError::Validation(format!(
            "Clave de registro fuera de la tabla única: {:?}\\{}\\{}",
            spec.hive, spec.subkey, spec.value_name
        )));
    }

    let root = RegLocation::native(spec.hive).root();
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
                raw_bytes: Some(raw_val.bytes.into_owned()),
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
                source: Some(Box::new(e)),
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
            source: Some(Box::new(e)),
        }),
    }
}

pub fn apply_registry_tweak(spec: &RegistryTweakSpec) -> CoreResult<()> {
    if !is_allowed_registry_entry(spec.hive, spec.subkey, spec.value_name) {
        return Err(CoreError::Validation(format!(
            "Clave de registro no permitida: {:?}\\{}",
            spec.hive, spec.subkey
        )));
    }

    RegLocation::native(spec.hive).ensure_writable()?;
    let root = RegLocation::native(spec.hive).root();
    let (key, _) = root
        .create_subkey_with_flags(spec.subkey, KEY_READ | KEY_WRITE)
        .map_err(|e| CoreError::Registry {
            path: format!("{}\\{}", spec.hive.as_str(), spec.subkey),
            message: format!("No se pudo abrir/crear subclave con KEY_WRITE: {}", e),
            source: Some(Box::new(e)),
        })?;

    let raw_val = RegValue {
        bytes: spec.value.to_bytes().into(),
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
            source: Some(Box::new(e)),
        })
}

pub fn restore_registry_snapshot(snap: &RegistrySnapshot, force: bool) -> CoreResult<bool> {
    if !snap.is_valid() {
        return Err(CoreError::Validation(format!(
            "Snapshot de registro inválido rechazado: {:?}\\{}\\{}",
            snap.hive, snap.subkey, snap.value_name
        )));
    }

    RegLocation::native(snap.hive).ensure_writable()?;
    let root = RegLocation::native(snap.hive).root();
    let key = match root.open_subkey_with_flags(&snap.subkey, KEY_READ | KEY_WRITE) {
        Ok(k) => k,
        Err(e) if e.kind() == io::ErrorKind::NotFound && !snap.existed => return Ok(true),
        Err(e) => {
            return Err(CoreError::Registry {
                path: format!("{}\\{}", snap.hive.as_str(), snap.subkey),
                message: format!("Error abriendo subclave para revertir: {}", e),
                source: Some(Box::new(e)),
            });
        }
    };

    if !force
        && let Ok(current) = key.get_raw_value(&snap.value_name)
        && current.bytes != snap.applied_bytes
    {
        return Ok(false);
    }

    if snap.existed {
        let vtype = snap.value_type.ok_or_else(|| {
            CoreError::Validation("Snapshot inconsistente: existed=true sin value_type".into())
        })?;
        let bytes = snap.raw_bytes.clone().ok_or_else(|| {
            CoreError::Validation("Snapshot inconsistente: existed=true sin raw_bytes".into())
        })?;
        let raw_val = RegValue {
            bytes: bytes.into(),
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
                source: Some(Box::new(e)),
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
                    source: Some(Box::new(e)),
                });
            }
        }
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
