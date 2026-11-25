use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    ERROR_DEPENDENT_SERVICES_RUNNING, ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_DOES_NOT_EXIST,
    ERROR_SERVICE_NOT_ACTIVE,
};
use windows::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, ControlService, OpenSCManagerW, OpenServiceW,
    QueryServiceConfig2W, QueryServiceConfigW, QueryServiceStatusEx, StartServiceW,
    ENUM_SERVICE_TYPE, QUERY_SERVICE_CONFIGW, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO,
    SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
    SERVICE_CONTROL_STOP, SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DEMAND_START, SERVICE_DISABLED,
    SERVICE_ERROR, SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
    SERVICE_START, SERVICE_START_PENDING, SERVICE_START_TYPE, SERVICE_STATUS,
    SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOPPED, SERVICE_STOP_PENDING,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::optimize::backup::ServiceSnapshot;
use crate::core::optimize::tweaks_catalog::{
    is_allowed_service_name, is_valid_service_start_type, ServiceTargetAction, ServiceTweakSpec,
};
use crate::core::platform::handles::ScHandle;
use crate::core::platform::memory::AlignedBuf;
use crate::core::platform::wide::to_wide_null;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceOpOutcome {
    Applied,
    /// El tipo de inicio se cambió, pero el servicio no se detuvo a tiempo: quedará detenido al reiniciar.
    AppliedPendingStop(String),
    SkippedNotInstalled,
    SkippedUserModified,
}

fn open_allowed_service(service_name: &str, access: u32) -> CoreResult<Option<ScHandle>> {
    if !is_allowed_service_name(service_name) {
        return Err(CoreError::Validation(format!(
            "Servicio fuera de la tabla única rechazado: '{}'",
            service_name
        )));
    }

    let scm_raw = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map_err(|e| CoreError::Service {
            service: service_name.to_string(),
            message: format!("OpenSCManagerW falló: {}", e),
            source: None,
        })?;
    let scm = ScHandle::new(scm_raw).ok_or_else(|| CoreError::Service {
        service: service_name.to_string(),
        message: "Handle SCM nulo".into(),
        source: None,
    })?;

    let svc_w = to_wide_null(service_name);
    match unsafe { OpenServiceW(scm.raw(), PCWSTR(svc_w.as_ptr()), access) } {
        Ok(raw) => Ok(ScHandle::new(raw)),
        Err(e) if e.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() => Ok(None),
        Err(e) => Err(CoreError::Service {
            service: service_name.to_string(),
            message: format!("OpenServiceW falló: {}", e),
            source: None,
        }),
    }
}

/// Captura la configuración completa del servicio (StartType, DelayedAutoStart y estado real)
/// usando un único handle antes de realizar cualquier modificación.
pub fn capture_service_tweak(spec: &ServiceTweakSpec) -> CoreResult<Option<ServiceSnapshot>> {
    let access = SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS;
    let Some(svc) = open_allowed_service(spec.service_name, access)? else {
        return Ok(None);
    };

    let (start_type, delayed_auto) = query_service_start_config(spec.service_name, &svc)?;
    if !is_valid_service_start_type(start_type) {
        return Err(CoreError::Validation(format!(
            "Tipo de inicio de servicio no soportado ({}) en '{}'",
            start_type, spec.service_name
        )));
    }

    let state = query_service_current_state(spec.service_name, &svc)?;
    let was_running = state == SERVICE_RUNNING.0 || state == SERVICE_START_PENDING.0;
    let applied_start_type = match spec.action {
        ServiceTargetAction::DisableAndStop => SERVICE_DISABLED.0,
        ServiceTargetAction::ManualStart => SERVICE_DEMAND_START.0,
    };

    Ok(Some(ServiceSnapshot {
        service_name: spec.service_name.to_string(),
        previous_start_type: start_type,
        previous_delayed_auto: delayed_auto,
        was_running,
        applied_start_type,
    }))
}

pub fn apply_service_tweak(spec: &ServiceTweakSpec) -> CoreResult<ServiceOpOutcome> {
    let access = SERVICE_QUERY_CONFIG | SERVICE_CHANGE_CONFIG | SERVICE_QUERY_STATUS | SERVICE_STOP;
    let Some(svc) = open_allowed_service(spec.service_name, access)? else {
        return Ok(ServiceOpOutcome::SkippedNotInstalled);
    };

    let target_start = match spec.action {
        ServiceTargetAction::DisableAndStop => SERVICE_DISABLED.0,
        ServiceTargetAction::ManualStart => SERVICE_DEMAND_START.0,
    };

    set_service_start_type(spec.service_name, &svc, target_start, false)?;

    if spec.action == ServiceTargetAction::DisableAndStop {
        if let Err(e) = stop_service_and_wait(spec.service_name, &svc, Duration::from_secs(12)) {
            return Ok(ServiceOpOutcome::AppliedPendingStop(e.to_string()));
        }
    }

    Ok(ServiceOpOutcome::Applied)
}

pub fn restore_service_snapshot(
    snap: &ServiceSnapshot,
    force: bool,
) -> CoreResult<ServiceOpOutcome> {
    if !snap.is_valid() {
        return Err(CoreError::Validation(format!(
            "Snapshot de servicio inválido rechazado: '{}'",
            snap.service_name
        )));
    }

    let access = SERVICE_QUERY_CONFIG
        | SERVICE_CHANGE_CONFIG
        | SERVICE_QUERY_STATUS
        | SERVICE_START
        | SERVICE_STOP;
    let Some(svc) = open_allowed_service(&snap.service_name, access)? else {
        return Ok(ServiceOpOutcome::SkippedNotInstalled);
    };

    if !force {
        let (current_start, _) = query_service_start_config(&snap.service_name, &svc)?;
        if current_start != snap.applied_start_type {
            return Ok(ServiceOpOutcome::SkippedUserModified);
        }
    }

    set_service_start_type(
        &snap.service_name,
        &svc,
        snap.previous_start_type,
        snap.previous_delayed_auto,
    )?;

    if snap.was_running && snap.previous_start_type != SERVICE_DISABLED.0 {
        start_service_and_wait(&snap.service_name, &svc, Duration::from_secs(12))?;
    }

    Ok(ServiceOpOutcome::Applied)
}

fn query_service_start_config(service_name: &str, svc: &ScHandle) -> CoreResult<(u32, bool)> {
    let mut needed = 0u32;
    let _ = unsafe { QueryServiceConfigW(svc.raw(), None, 0, &mut needed) };
    if needed == 0 {
        return Err(CoreError::Service {
            service: service_name.to_string(),
            message: "QueryServiceConfigW devolvió tamaño 0".into(),
            source: None,
        });
    }

    let mut buf = AlignedBuf::zeroed(needed as usize);
    unsafe {
        QueryServiceConfigW(
            svc.raw(),
            Some(buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW),
            needed,
            &mut needed,
        )
    }
    .map_err(|e| CoreError::Service {
        service: service_name.to_string(),
        message: format!("QueryServiceConfigW falló: {}", e),
        source: None,
    })?;

    let cfg = unsafe { &*(buf.as_ptr() as *const QUERY_SERVICE_CONFIGW) };
    let start_type = cfg.dwStartType.0;

    let mut delayed = false;
    if start_type == SERVICE_AUTO_START.0 {
        let mut delayed_info = SERVICE_DELAYED_AUTO_START_INFO::default();
        let mut bytes_out = 0u32;
        let ok = unsafe {
            QueryServiceConfig2W(
                svc.raw(),
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                Some(std::slice::from_raw_parts_mut(
                    &mut delayed_info as *mut _ as *mut u8,
                    std::mem::size_of::<SERVICE_DELAYED_AUTO_START_INFO>(),
                )),
                &mut bytes_out,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!(
                "No se pudo leer el inicio retrasado de '{}': {}",
                service_name, e
            ),
            source: None,
        });
        // Sin este dato, restaurar dejaría «Automático» donde había «Automático (inicio retrasado)».
        ok?;
        delayed = delayed_info.fDelayedAutostart.as_bool();
    }

    Ok((start_type, delayed))
}

/// Consulta de solo lectura: `true` si el servicio existe y está detenido (o no existe). No modifica nada,
/// así que no exige estar en la tabla de servicios que OtterSwiss cambia.
pub fn is_service_stopped(service_name: &str) -> CoreResult<bool> {
    let scm_raw = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map_err(|e| CoreError::Service {
            service: service_name.to_string(),
            message: format!("OpenSCManagerW falló: {}", e),
            source: None,
        })?;
    let scm = ScHandle::new(scm_raw).ok_or_else(|| CoreError::Service {
        service: service_name.to_string(),
        message: "Handle SCM nulo".into(),
        source: None,
    })?;
    let svc_w = to_wide_null(service_name);
    let svc = match unsafe { OpenServiceW(scm.raw(), PCWSTR(svc_w.as_ptr()), SERVICE_QUERY_STATUS) }
    {
        Ok(raw) => ScHandle::new(raw),
        Err(e) if e.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() => return Ok(true),
        Err(e) => {
            return Err(CoreError::Service {
                service: service_name.to_string(),
                message: format!("OpenServiceW falló: {}", e),
                source: None,
            })
        }
    };
    let Some(svc) = svc else {
        return Ok(true);
    };
    Ok(query_service_current_state(service_name, &svc)? == SERVICE_STOPPED.0)
}

fn query_service_current_state(service_name: &str, svc: &ScHandle) -> CoreResult<u32> {
    let mut ssp = SERVICE_STATUS_PROCESS::default();
    let mut bytes_needed = 0u32;
    unsafe {
        QueryServiceStatusEx(
            svc.raw(),
            SC_STATUS_PROCESS_INFO,
            Some(std::slice::from_raw_parts_mut(
                &mut ssp as *mut _ as *mut u8,
                std::mem::size_of::<SERVICE_STATUS_PROCESS>(),
            )),
            &mut bytes_needed,
        )
    }
    .map_err(|e| CoreError::Service {
        service: service_name.to_string(),
        message: format!("QueryServiceStatusEx falló: {}", e),
        source: None,
    })?;

    Ok(ssp.dwCurrentState.0)
}

fn set_service_start_type(
    service_name: &str,
    svc: &ScHandle,
    start_type: u32,
    delayed_auto: bool,
) -> CoreResult<()> {
    if !is_valid_service_start_type(start_type) {
        return Err(CoreError::Validation(format!(
            "StartType inválido ({}) para servicio '{}'",
            start_type, service_name
        )));
    }

    unsafe {
        ChangeServiceConfigW(
            svc.raw(),
            ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
            SERVICE_START_TYPE(start_type),
            SERVICE_ERROR(SERVICE_NO_CHANGE),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
    }
    .map_err(|e| CoreError::Service {
        service: service_name.to_string(),
        message: format!("ChangeServiceConfigW({}) falló: {}", start_type, e),
        source: None,
    })?;

    if start_type == SERVICE_AUTO_START.0 {
        let delayed_info = SERVICE_DELAYED_AUTO_START_INFO {
            fDelayedAutostart: delayed_auto.into(),
        };
        unsafe {
            ChangeServiceConfig2W(
                svc.raw(),
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                Some(&delayed_info as *const _ as *const _),
            )
        }
        .map_err(|e| CoreError::Service {
            service: service_name.to_string(),
            message: format!("ChangeServiceConfig2W(DELAYED_AUTO_START) falló: {}", e),
            source: None,
        })?;
    }

    Ok(())
}

fn stop_service_and_wait(service_name: &str, svc: &ScHandle, timeout: Duration) -> CoreResult<()> {
    let current = query_service_current_state(service_name, svc)?;
    if current == SERVICE_STOPPED.0 {
        return Ok(());
    }

    if current != SERVICE_STOP_PENDING.0 {
        let mut status = SERVICE_STATUS::default();
        if let Err(e) = unsafe { ControlService(svc.raw(), SERVICE_CONTROL_STOP, &mut status) } {
            if e.code() == ERROR_SERVICE_NOT_ACTIVE.to_hresult() {
                return Ok(());
            }
            if e.code() == ERROR_DEPENDENT_SERVICES_RUNNING.to_hresult() {
                return Err(CoreError::Service {
                    service: service_name.to_string(),
                    message: "No se pudo detener porque tiene servicios dependientes activos"
                        .into(),
                    source: None,
                });
            }
            return Err(CoreError::Service {
                service: service_name.to_string(),
                message: format!("ControlService(STOP) falló: {}", e),
                source: None,
            });
        }
    }

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if query_service_current_state(service_name, svc)? == SERVICE_STOPPED.0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    Err(CoreError::Service {
        service: service_name.to_string(),
        message: "Tiempo de espera agotado deteniendo el servicio".into(),
        source: None,
    })
}

fn start_service_and_wait(service_name: &str, svc: &ScHandle, timeout: Duration) -> CoreResult<()> {
    let current = query_service_current_state(service_name, svc)?;
    if current == SERVICE_RUNNING.0 {
        return Ok(());
    }

    if current != SERVICE_START_PENDING.0 {
        if let Err(e) = unsafe { StartServiceW(svc.raw(), None) } {
            if e.code() == ERROR_SERVICE_ALREADY_RUNNING.to_hresult() {
                return Ok(());
            }
            return Err(CoreError::Service {
                service: service_name.to_string(),
                message: format!("StartServiceW falló: {}", e),
                source: None,
            });
        }
    }

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if query_service_current_state(service_name, svc)? == SERVICE_RUNNING.0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    Err(CoreError::Service {
        service: service_name.to_string(),
        message: "Tiempo de espera agotado iniciando el servicio".into(),
        source: None,
    })
}
