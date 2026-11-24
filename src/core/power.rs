use std::ptr;
use windows::core::GUID;
use windows::Win32::Foundation::{LocalFree, HLOCAL, WIN32_ERROR};
use windows::Win32::System::Power::{
    PowerDeleteScheme, PowerDuplicateScheme, PowerGetActiveScheme, PowerReadFriendlyName,
    PowerSetActiveScheme,
};

use crate::core::backup::PowerSnapshot;
use crate::core::error::{CoreError, CoreResult};

/// GUIDs integrados estándar de Windows.
pub const GUID_BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const GUID_HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const GUID_ULTIMATE_PERFORMANCE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

const BUILTIN_SCHEMES: &[&str] = &[
    GUID_BALANCED,
    GUID_HIGH_PERFORMANCE,
    GUID_ULTIMATE_PERFORMANCE,
    "a1841308-3541-4fab-bc81-f71556f20b4a", // Ahorro de energía
];

const BALANCED_GUID_STRUCT: GUID = GUID {
    data1: 0x381b4222,
    data2: 0xf694,
    data3: 0x41f0,
    data4: [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e],
};

#[derive(Debug, Clone)]
pub struct PowerApplyReport {
    pub snapshot: PowerSnapshot,
    pub used_fallback_scheme: bool,
    pub active_scheme_guid: String,
}

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

pub fn get_active_scheme_guid() -> CoreResult<GUID> {
    let mut guid_ptr: *mut GUID = ptr::null_mut();
    let err: WIN32_ERROR = unsafe { PowerGetActiveScheme(None, &mut guid_ptr) };
    if err.0 != 0 || guid_ptr.is_null() {
        return Err(CoreError::Power {
            message: format!("PowerGetActiveScheme falló (código {})", err.0),
            source: None,
        });
    }
    let guid = unsafe { *guid_ptr };
    unsafe {
        let _ = LocalFree(HLOCAL(guid_ptr as _));
    }
    Ok(guid)
}

fn scheme_exists(guid: &GUID) -> bool {
    let mut buf_size = 0u32;
    let err = unsafe { PowerReadFriendlyName(None, Some(guid), None, None, None, &mut buf_size) };
    err.0 == 0 && buf_size > 0
}

/// Prepara el snapshot de energía antes de activar el esquema destino.
/// Si el esquema activo ya fue creado por OtterSwiss (`known_created_guids`), evita duplicar de nuevo.
pub fn capture_and_prepare_power_scheme(
    target_template_guid: &str,
    fallback_template_guid: Option<&str>,
    known_created_guids: &[String],
) -> CoreResult<PowerApplyReport> {
    let current = get_active_scheme_guid()?;
    let current_str = format_guid(&current);

    let (base_guid, used_fallback) = {
        let primary = parse_guid_strict(target_template_guid)?;
        if scheme_exists(&primary) {
            (primary, false)
        } else if let Some(fb) = fallback_template_guid {
            let fb_guid = parse_guid_strict(fb)?;
            if scheme_exists(&fb_guid) {
                (fb_guid, true)
            } else {
                return Err(CoreError::Power {
                    message: "Ni el plan de energía objetivo ni el alternativo están disponibles"
                        .into(),
                    source: None,
                });
            }
        } else {
            return Err(CoreError::Power {
                message: format!("Plan de energía '{}' no disponible", target_template_guid),
                source: None,
            });
        }
    };

    if known_created_guids
        .iter()
        .any(|g| g.eq_ignore_ascii_case(&current_str))
    {
        return Ok(PowerApplyReport {
            snapshot: PowerSnapshot {
                previous_scheme_guid: current_str.clone(),
                created_scheme_guid: None,
                active_after_apply_guid: current_str.clone(),
            },
            used_fallback_scheme: used_fallback,
            active_scheme_guid: current_str,
        });
    }

    let mut dup_ptr: *mut GUID = ptr::null_mut();
    let dup_err = unsafe { PowerDuplicateScheme(None, &base_guid, &mut dup_ptr) };
    if dup_err.0 != 0 || dup_ptr.is_null() {
        return Err(CoreError::Power {
            message: format!("PowerDuplicateScheme falló (código {})", dup_err.0),
            source: None,
        });
    }
    let new_guid = unsafe { *dup_ptr };
    unsafe {
        let _ = LocalFree(HLOCAL(dup_ptr as _));
    }
    let new_guid_str = format_guid(&new_guid);

    Ok(PowerApplyReport {
        snapshot: PowerSnapshot {
            previous_scheme_guid: current_str,
            created_scheme_guid: Some(new_guid_str.clone()),
            active_after_apply_guid: new_guid_str.clone(),
        },
        used_fallback_scheme: used_fallback,
        active_scheme_guid: new_guid_str,
    })
}

/// Activa el esquema preparado tras persistir el snapshot en `backup.json`.
/// Si `PowerSetActiveScheme` falla, limpia inmediatamente el esquema duplicado para no dejar huérfanos.
pub fn activate_prepared_power_scheme(snap: &PowerSnapshot) -> CoreResult<String> {
    let target = parse_guid_strict(&snap.active_after_apply_guid)?;
    let set_err = unsafe { PowerSetActiveScheme(None, Some(&target)) };
    if set_err.0 != 0 {
        if let Some(ref created) = snap.created_scheme_guid {
            if let Ok(cg) = parse_guid_strict(created) {
                unsafe {
                    let _ = PowerDeleteScheme(None, &cg);
                }
            }
        }
        return Err(CoreError::Power {
            message: format!("PowerSetActiveScheme falló (código {})", set_err.0),
            source: None,
        });
    }

    let verified = get_active_scheme_guid()?;
    let verified_str = format_guid(&verified);
    if !verified_str.eq_ignore_ascii_case(&snap.active_after_apply_guid) {
        return Err(CoreError::Power {
            message: format!(
                "El esquema activo tras aplicar ({}) no coincide con el esperado ({})",
                verified_str, snap.active_after_apply_guid
            ),
            source: None,
        });
    }

    Ok(verified_str)
}

/// Revierte el plan de energía. Si el esquema anterior ya no existe, cae a Equilibrado
/// y procede igual con la eliminación del esquema creado por OtterSwiss.
pub fn restore_power_snapshot(snap: &PowerSnapshot) -> CoreResult<()> {
    let prev_guid = parse_guid_strict(&snap.previous_scheme_guid).unwrap_or(BALANCED_GUID_STRUCT);

    let restore_target = if scheme_exists(&prev_guid) {
        prev_guid
    } else {
        BALANCED_GUID_STRUCT
    };

    let set_err = unsafe { PowerSetActiveScheme(None, Some(&restore_target)) };
    if set_err.0 != 0 {
        return Err(CoreError::Power {
            message: format!(
                "No se pudo restaurar el plan de energía previo/Equilibrado (código {})",
                set_err.0
            ),
            source: None,
        });
    }

    if let Some(ref created_str) = snap.created_scheme_guid {
        if BUILTIN_SCHEMES
            .iter()
            .any(|b| b.eq_ignore_ascii_case(created_str))
        {
            return Err(CoreError::Validation(format!(
                "Intento bloqueado de borrar esquema integrado de Windows: '{}'",
                created_str
            )));
        }

        let created_guid = parse_guid_strict(created_str)?;
        if scheme_exists(&created_guid) {
            let del_err = unsafe { PowerDeleteScheme(None, &created_guid) };
            if del_err.0 != 0 {
                return Err(CoreError::Power {
                    message: format!(
                        "No se pudo eliminar el esquema temporal '{}' (código {})",
                        created_str, del_err.0
                    ),
                    source: None,
                });
            }
        }
    }

    Ok(())
}
