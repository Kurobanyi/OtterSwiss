//! Zonas térmicas ACPI por WMI (`MSAcpi_ThermalZoneTemperature` y `Win32_PerfFormattedData_Counters_ThermalZoneInformation`).

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, CoSetProxyBlanket, CoTaskMemFree, EOAC_NONE,
    RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::System::Variant::{
    VariantClear, VariantToDouble, VariantToInt32, VariantToStringAlloc,
};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
    WbemLocator,
};
use windows::core::{BSTR, PCWSTR};

use super::{SensorReading, SensorSession};
use crate::core::platform::wide::to_wide_null;

pub(super) const RPC_C_AUTHN_WINNT: u32 = 10;
pub(super) const RPC_C_AUTHZ_NONE: u32 = 0;

/// CL-24: temperatura actual creíble en °C; fuera de este rango la lectura es basura del sensor.
const CURRENT_TEMP_C: std::ops::RangeInclusive<f32> = 1.0..=125.0;
/// Máximo o punto crítico creíble en °C.
const LIMIT_TEMP_C: std::ops::RangeInclusive<f32> = 1.0..=150.0;
/// Punto crítico ACPI creíble: por debajo de esto el firmware no lo informó de verdad.
const MIN_CRITICAL_C: f32 = 20.0;
/// Filas que se piden a la consulta de sensores de LibreHardwareMonitor.
const MAX_LHM_SENSORS: usize = 256;

/// CL-24: una propiedad de un objeto WMI convertida con `conv` (`VariantToInt32`, `VariantToDouble`…).
fn read_wmi<T>(
    obj: &IWbemClassObject,
    prop: &str,
    conv: unsafe fn(*const VARIANT) -> windows::core::Result<T>,
) -> Option<T> {
    let w = to_wide_null(prop);
    let mut var = VARIANT::default();
    // SAFETY: `w` termina en NUL y vive hasta el final; `var` es un VARIANT inicializado que `conv` solo lee y que
    // se libera abajo.
    unsafe {
        if obj
            .Get(PCWSTR(w.as_ptr()), 0, &mut var, None, None)
            .is_err()
        {
            return None;
        }
        let val = conv(&var).ok();
        let _ = VariantClear(&mut var);
        val
    }
}

pub(super) fn read_wmi_i32(obj: &IWbemClassObject, prop: &str) -> Option<i32> {
    read_wmi(obj, prop, VariantToInt32)
}

fn read_wmi_f32(obj: &IWbemClassObject, prop: &str) -> Option<f32> {
    read_wmi(obj, prop, VariantToDouble).map(|v| v as f32)
}

pub(super) fn read_wmi_string(obj: &IWbemClassObject, prop: &str) -> Option<String> {
    let w = to_wide_null(prop);
    let mut var = VARIANT::default();
    // SAFETY: `w` termina en NUL y vive hasta el final; `var` se libera abajo y la cadena que devuelve
    // `VariantToStringAlloc` se libera con `CoTaskMemFree` después de copiarla.
    unsafe {
        if obj
            .Get(PCWSTR(w.as_ptr()), 0, &mut var, None, None)
            .is_err()
        {
            return None;
        }
        let res = VariantToStringAlloc(&var).ok().map(|pw| {
            let s = pw.to_string().unwrap_or_default();
            CoTaskMemFree(Some(pw.0 as *const _));
            s
        });
        let _ = VariantClear(&mut var);
        res.filter(|s| !s.trim().is_empty())
    }
}

/// CL-09: décimas de kelvin (como las da WMI) a grados Celsius.
fn decikelvin_to_c(tenths_k: i32) -> f32 {
    (tenths_k as f32 / 10.0) - 273.15
}

/// Ejecuta `query` en el espacio de nombres `namespace` y devuelve hasta `max` objetos. Vacío si WMI falla.
fn wmi_query(
    locator: &IWbemLocator,
    namespace: &str,
    query: &str,
    max: usize,
) -> Vec<IWbemClassObject> {
    let mut out = Vec::new();
    let ns = BSTR::from(namespace);
    let empty = BSTR::default();
    // SAFETY: llamadas COM sobre interfaces válidas; las BSTR viven hasta el final de la función.
    unsafe {
        let Ok(services) = locator.ConnectServer(&ns, &empty, &empty, &empty, 0, &empty, None)
        else {
            return out;
        };
        let _ = CoSetProxyBlanket(
            &services,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        );
        let Ok(enumerator) = services.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from(query),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        ) else {
            return out;
        };
        for _ in 0..max {
            let mut objs: [Option<IWbemClassObject>; 1] = [None];
            let mut returned = 0u32;
            if enumerator.Next(1200, &mut objs, &mut returned).is_err() || returned == 0 {
                break;
            }
            match objs[0].take() {
                Some(obj) => out.push(obj),
                None => break,
            }
        }
    }
    out
}

/// Temperaturas de CPU que publica LibreHardwareMonitor u OpenHardwareMonitor en WMI, si el usuario tiene uno
/// abierto. Así hay lecturas reales de CPU sin que OtterSwiss cargue un driver de kernel propio.
fn poll_external_cpu_wmi(locator: &IWbemLocator) -> Vec<SensorReading> {
    for (namespace, label) in [
        (r"root\LibreHardwareMonitor", "CPU (LibreHardwareMonitor)"),
        (r"root\OpenHardwareMonitor", "CPU (OpenHardwareMonitor)"),
    ] {
        let sensors = wmi_query(
            locator,
            namespace,
            "SELECT Name, Identifier, Value, Max FROM Sensor WHERE SensorType = 'Temperature'",
            MAX_LHM_SENSORS,
        );
        let out: Vec<SensorReading> = sensors
            .iter()
            .filter_map(|obj| {
                let name = read_wmi_string(obj, "Name")?;
                let identifier = read_wmi_string(obj, "Identifier").unwrap_or_default();
                if !is_cpu_temperature(&identifier, &name) {
                    return None;
                }
                let current = read_wmi_f32(obj, "Value").filter(|v| CURRENT_TEMP_C.contains(v))?;
                Some(SensorReading {
                    name,
                    sensor_type: label.into(),
                    current_celsius: current,
                    max_celsius: read_wmi_f32(obj, "Max").filter(|v| LIMIT_TEMP_C.contains(v)),
                })
            })
            .collect();
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

/// UI-22: `true` si el sensor de LibreHardwareMonitor es una temperatura de CPU. Se decide por `Identifier`
/// (`/amdcpu/0/temperature/2`, `/intelcpu/0/temperature/0`), no por el nombre: «GPU Core» también contiene «core».
/// Las «distancias a TjMax» son de tipo Temperature pero no son temperaturas.
fn is_cpu_temperature(identifier: &str, name: &str) -> bool {
    let id = identifier.to_ascii_lowercase();
    let low = name.to_ascii_lowercase();
    if low.contains("distance") {
        return false;
    }
    if !id.is_empty() {
        return id.starts_with("/amdcpu/")
            || id.starts_with("/intelcpu/")
            || id.starts_with("/cpu/");
    }
    // Sin `Identifier` (versiones viejas): por nombre, descartando lo que es de la GPU.
    !low.contains("gpu")
        && ["cpu", "package", "core", "tctl", "tdie", "ccd"]
            .iter()
            .any(|k| low.contains(k))
}

impl SensorSession {
    pub fn poll_wmi_acpi(&self) -> (Vec<SensorReading>, Option<String>) {
        let mut readings = Vec::new();
        // SAFETY: crea el localizador WMI; COM ya está inicializado en el hilo de sondeo (ComGuard).
        let locator: IWbemLocator =
            match unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) } {
                Ok(l) => l,
                Err(e) => return (readings, Some(format!("WMI locator no disponible: {}", e))),
            };
        readings.extend(poll_external_cpu_wmi(&locator));

        let zones = wmi_query(
            &locator,
            r"root\WMI",
            "SELECT InstanceName, CurrentTemperature, CriticalTripPoint FROM MSAcpi_ThermalZoneTemperature",
            16,
        );
        for (idx, obj) in zones.iter().enumerate() {
            let Some(c) = read_wmi_i32(obj, "CurrentTemperature").map(decikelvin_to_c) else {
                continue;
            };
            if !CURRENT_TEMP_C.contains(&c) {
                continue;
            }
            let crit_c = read_wmi_i32(obj, "CriticalTripPoint")
                .map(decikelvin_to_c)
                .filter(|v| *v >= MIN_CRITICAL_C && LIMIT_TEMP_C.contains(v));
            let instance_name = read_wmi_string(obj, "InstanceName")
                .unwrap_or_else(|| format!("Zona ACPI {}", idx));
            readings.push(SensorReading {
                name: format!("ACPI ({})", instance_name),
                // UI-14: es una zona térmica ACPI; no tiene por qué ser la CPU.
                sensor_type: "Zona térmica ACPI (WMI)".into(),
                current_celsius: c,
                max_celsius: crit_c,
            });
        }

        // Respaldo en root\CIMV2 -> Win32_PerfFormattedData_Counters_ThermalZoneInformation
        if readings.is_empty() {
            let zones = wmi_query(
                &locator,
                r"root\CIMV2",
                "SELECT Name, Temperature, HighPrecisionTemperature FROM Win32_PerfFormattedData_Counters_ThermalZoneInformation",
                16,
            );
            for (idx, obj) in zones.iter().enumerate() {
                let hp = read_wmi_i32(obj, "HighPrecisionTemperature")
                    .map(decikelvin_to_c)
                    .filter(|c| CURRENT_TEMP_C.contains(c));
                let std_k = read_wmi_i32(obj, "Temperature")
                    .map(|k| k as f32 - 273.15)
                    .filter(|c| CURRENT_TEMP_C.contains(c));
                let Some(c) = hp.or(std_k) else {
                    continue;
                };
                let zone_name =
                    read_wmi_string(obj, "Name").unwrap_or_else(|| format!("ThermalZone {}", idx));
                readings.push(SensorReading {
                    name: format!("ACPI ({})", zone_name),
                    sensor_type: "Zona térmica ACPI (PerfCounter)".into(),
                    current_celsius: c,
                    max_celsius: None,
                });
            }
        }

        let note = if readings.is_empty() {
            Some(
                "Esta placa base de escritorio no expone sensores de CPU en ACPI/WMI sin un driver kernel externo (opcionalmente puedes tener abierto LibreHardwareMonitor en segundo plano)."
                    .into(),
            )
        } else {
            None
        };
        (readings, note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_gpu_no_cuenta_como_cpu() {
        assert!(is_cpu_temperature(
            "/amdcpu/0/temperature/2",
            "Core (Tctl/Tdie)"
        ));
        assert!(is_cpu_temperature(
            "/intelcpu/0/temperature/0",
            "CPU Package"
        ));
        assert!(!is_cpu_temperature(
            "/gpu-nvidia/0/temperature/0",
            "GPU Core"
        ));
        assert!(!is_cpu_temperature(
            "/intelcpu/0/temperature/5",
            "CPU Core #1 Distance to TjMax"
        ));
        assert!(!is_cpu_temperature("", "GPU Core"));
        assert!(is_cpu_temperature("", "CPU Package"));
    }
}
