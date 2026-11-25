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

pub(super) fn read_wmi_i32(obj: &IWbemClassObject, prop: &str) -> Option<i32> {
    let w = to_wide_null(prop);
    let mut var = VARIANT::default();
    // SAFETY: `w` termina en NUL y vive hasta el final; `var` es un VARIANT inicializado que se libera abajo.
    unsafe {
        if obj
            .Get(PCWSTR(w.as_ptr()), 0, &mut var, None, None)
            .is_err()
        {
            return None;
        }
        let val = VariantToInt32(&var).ok();
        let _ = VariantClear(&mut var);
        val
    }
}

fn read_wmi_f32(obj: &IWbemClassObject, prop: &str) -> Option<f32> {
    let w = to_wide_null(prop);
    let mut var = VARIANT::default();
    // SAFETY: `w` termina en NUL y vive hasta el final; `var` es un VARIANT inicializado que se libera abajo.
    unsafe {
        if obj
            .Get(PCWSTR(w.as_ptr()), 0, &mut var, None, None)
            .is_err()
        {
            return None;
        }
        let val = VariantToDouble(&var).ok().map(|v| v as f32);
        let _ = VariantClear(&mut var);
        val
    }
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
            "SELECT Name, SensorType, Value, Max FROM Sensor WHERE SensorType = 'Temperature'",
            256,
        );
        let out: Vec<SensorReading> = sensors
            .iter()
            .filter_map(|obj| {
                let name = read_wmi_string(obj, "Name")?;
                if !is_cpu_sensor_name(&name) {
                    return None;
                }
                let current = read_wmi_f32(obj, "Value").filter(|v| *v > 1.0 && *v < 125.0)?;
                Some(SensorReading {
                    name,
                    sensor_type: label.into(),
                    current_celsius: current,
                    max_celsius: read_wmi_f32(obj, "Max").filter(|v| *v > 1.0 && *v < 150.0),
                })
            })
            .collect();
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

/// Nombres de sensor de LibreHardwareMonitor que corresponden a la CPU.
fn is_cpu_sensor_name(name: &str) -> bool {
    let low = name.to_ascii_lowercase();
    ["cpu", "package", "core", "tctl", "tdie", "ccd"]
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
            if c <= 1.0 || c >= 125.0 {
                continue;
            }
            let crit_c = read_wmi_i32(obj, "CriticalTripPoint")
                .map(decikelvin_to_c)
                .filter(|&v| v > 20.0 && v < 150.0);
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
                    .filter(|&c| c > 1.0 && c < 125.0);
                let std_k = read_wmi_i32(obj, "Temperature")
                    .map(|k| k as f32 - 273.15)
                    .filter(|&c| c > 1.0 && c < 125.0);
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
