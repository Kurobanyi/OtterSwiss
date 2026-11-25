//! Energía del equipo: batería y reinicio.

use windows::Win32::Foundation::{HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Shutdown::{
    EWX_FORCEIFHUNG, EWX_REBOOT, ExitWindowsEx, SHTDN_REASON_FLAG_PLANNED,
    SHTDN_REASON_MAJOR_APPLICATION,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::PCWSTR;

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::SafeHandle;

pub fn detect_battery_presence() -> bool {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: la estructura de salida es local.
    if unsafe { GetSystemPowerStatus(&mut status) }.is_ok() {
        // 255 = estado desconocido; el bit 128 = sin batería (puede venir junto a otros bits).
        status.BatteryFlag != 255 && (status.BatteryFlag & 128) == 0
    } else {
        false
    }
}

/// Reinicia Windows de forma nativa habilitando `SE_SHUTDOWN_NAME` en el token actual.
pub fn reboot_system_now() -> CoreResult<()> {
    let mut raw_token = HANDLE::default();
    // SAFETY: el handle de proceso es válido y el token devuelto pasa a SafeHandle.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut raw_token,
        )
    }
    .map_err(CoreError::sys("OpenProcessToken falló para reiniciar: "))?;

    let token = SafeHandle::new(raw_token)
        .ok_or_else(|| CoreError::invalid_handle("Token de proceso inválido"))?;

    let mut luid = LUID::default();
    // SAFETY: nombre de privilegio constante; el LUID de salida es local.
    unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid) }.map_err(
        CoreError::sys("LookupPrivilegeValueW(SE_SHUTDOWN_NAME) falló: "),
    )?;

    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };

    // SAFETY: el token es propio y la estructura de privilegios vive durante la llamada.
    unsafe { AdjustTokenPrivileges(token.raw(), false, Some(&tp), 0, None, None) }.map_err(
        |e| CoreError::SystemTool {
            message: format!("AdjustTokenPrivileges(SE_SHUTDOWN_NAME) falló: {}", e),
            source: Some(Box::new(e)),
        },
    )?;

    // SAFETY: reinicio pedido por el usuario; el privilegio de apagado ya se habilitó.
    unsafe {
        ExitWindowsEx(
            EWX_REBOOT | EWX_FORCEIFHUNG,
            SHTDN_REASON_MAJOR_APPLICATION | SHTDN_REASON_FLAG_PLANNED,
        )
    }
    .map_err(CoreError::sys("ExitWindowsEx(EWX_REBOOT) falló: "))?;

    Ok(())
}
