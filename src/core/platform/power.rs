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
    if unsafe { GetSystemPowerStatus(&mut status) }.is_ok() {
        status.BatteryFlag != 128 && status.BatteryFlag != 255
    } else {
        false
    }
}

/// Reinicia Windows de forma nativa habilitando `SE_SHUTDOWN_NAME` en el token actual.
pub fn reboot_system_now() -> CoreResult<()> {
    let mut raw_token = HANDLE::default();
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

    unsafe { AdjustTokenPrivileges(token.raw(), false, Some(&tp), 0, None, None) }.map_err(
        |e| CoreError::SystemTool {
            message: format!("AdjustTokenPrivileges(SE_SHUTDOWN_NAME) falló: {}", e),
            source: Some(Box::new(e)),
        },
    )?;

    unsafe {
        ExitWindowsEx(
            EWX_REBOOT | EWX_FORCEIFHUNG,
            SHTDN_REASON_MAJOR_APPLICATION | SHTDN_REASON_FLAG_PLANNED,
        )
    }
    .map_err(CoreError::sys("ExitWindowsEx(EWX_REBOOT) falló: "))?;

    Ok(())
}
