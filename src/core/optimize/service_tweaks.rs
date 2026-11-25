//! Ajustes de servicios del perfil de optimización: capturar, aplicar y restaurar solo servicios de la tabla.

use std::time::Duration;
use windows::Win32::System::Services::{
    SERVICE_CHANGE_CONFIG, SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_QUERY_CONFIG,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START, SERVICE_START_PENDING, SERVICE_STOP,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::optimize::backup::ServiceSnapshot;
use crate::core::optimize::tweaks_catalog::{
    ServiceTargetAction, ServiceTweakSpec, is_allowed_service_name, is_valid_service_start_type,
};
use crate::core::platform::handles::ScHandle;

use crate::core::platform::services::{
    open_service, query_service_current_state, query_service_start_config, set_service_start_type,
    start_service_and_wait, stop_service_and_wait,
};

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

    open_service(service_name, access)
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

    if spec.action == ServiceTargetAction::DisableAndStop
        && let Err(e) = stop_service_and_wait(spec.service_name, &svc, Duration::from_secs(12))
    {
        return Ok(ServiceOpOutcome::AppliedPendingStop(e.to_string()));
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
