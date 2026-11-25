//! Zonas térmicas ACPI por WMI (`MSAcpi_ThermalZoneTemperature` y `Win32_PerfFormattedData_Counters_ThermalZoneInformation`).

use windows::core::{BSTR, PCWSTR, VARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoSetProxyBlanket, CoTaskMemFree, CLSCTX_INPROC_SERVER, EOAC_NONE,
    RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::{VariantClear, VariantToInt32, VariantToStringAlloc};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY,
};

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

impl SensorSession {
    pub fn poll_wmi_acpi(&self) -> (Vec<SensorReading>, Option<String>) {
        let mut readings = Vec::new();
        let locator: IWbemLocator =
            match unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) } {
                Ok(l) => l,
                Err(e) => return (readings, Some(format!("WMI locator no disponible: {}", e))),
            };

        let ns = BSTR::from(r"root\WMI");
        let empty = BSTR::default();
        if let Ok(services) =
            unsafe { locator.ConnectServer(&ns, &empty, &empty, &empty, 0, &empty, None) }
        {
            let _ = unsafe {
                CoSetProxyBlanket(
                    &services,
                    RPC_C_AUTHN_WINNT,
                    RPC_C_AUTHZ_NONE,
                    None,
                    RPC_C_AUTHN_LEVEL_CALL,
                    RPC_C_IMP_LEVEL_IMPERSONATE,
                    None,
                    EOAC_NONE,
                )
            };

            let wql = BSTR::from("WQL");
            let query = BSTR::from(
                "SELECT InstanceName, CurrentTemperature, CriticalTripPoint FROM MSAcpi_ThermalZoneTemperature",
            );
            if let Ok(enumerator) = unsafe {
                services.ExecQuery(
                    &wql,
                    &query,
                    WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                    None,
                )
            } {
                for idx in 0..16 {
                    let mut objs: [Option<IWbemClassObject>; 1] = [None];
                    let mut returned = 0u32;
                    let hr = unsafe { enumerator.Next(1200, &mut objs, &mut returned) };
                    if hr.is_err() || returned == 0 {
                        break;
                    }
                    let Some(obj) = objs[0].take() else {
                        break;
                    };

                    let temp_c = read_wmi_i32(&obj, "CurrentTemperature")
                        .map(|tenths_k| (tenths_k as f32 / 10.0) - 273.15);

                    let Some(c) = temp_c else {
                        continue;
                    };
                    if c <= 1.0 || c >= 125.0 {
                        continue;
                    }

                    let crit_c = read_wmi_i32(&obj, "CriticalTripPoint")
                        .map(|tenths_k| (tenths_k as f32 / 10.0) - 273.15)
                        .filter(|&v| v > 20.0 && v < 150.0);

                    let instance_name = read_wmi_string(&obj, "InstanceName")
                        .unwrap_or_else(|| format!("Zona ACPI {}", idx));

                    readings.push(SensorReading {
                        name: format!("ACPI ({})", instance_name),
                        // UI-14: es una zona térmica ACPI; no tiene por qué ser la CPU.
                        sensor_type: "Zona térmica ACPI (WMI)".into(),
                        current_celsius: c,
                        max_celsius: crit_c,
                    });
                }
            }
        }

        // Fallback en root\CIMV2 -> Win32_PerfFormattedData_Counters_ThermalZoneInformation
        if readings.is_empty() {
            let cimv2 = BSTR::from(r"root\CIMV2");
            if let Ok(services) =
                unsafe { locator.ConnectServer(&cimv2, &empty, &empty, &empty, 0, &empty, None) }
            {
                let _ = unsafe {
                    CoSetProxyBlanket(
                        &services,
                        RPC_C_AUTHN_WINNT,
                        RPC_C_AUTHZ_NONE,
                        None,
                        RPC_C_AUTHN_LEVEL_CALL,
                        RPC_C_IMP_LEVEL_IMPERSONATE,
                        None,
                        EOAC_NONE,
                    )
                };
                let wql = BSTR::from("WQL");
                let query = BSTR::from(
                    "SELECT Name, Temperature, HighPrecisionTemperature FROM Win32_PerfFormattedData_Counters_ThermalZoneInformation",
                );
                if let Ok(enumerator) = unsafe {
                    services.ExecQuery(
                        &wql,
                        &query,
                        WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                        None,
                    )
                } {
                    for idx in 0..16 {
                        let mut objs: [Option<IWbemClassObject>; 1] = [None];
                        let mut returned = 0u32;
                        let hr = unsafe { enumerator.Next(1200, &mut objs, &mut returned) };
                        if hr.is_err() || returned == 0 {
                            break;
                        }
                        let Some(obj) = objs[0].take() else {
                            break;
                        };

                        let hp = read_wmi_i32(&obj, "HighPrecisionTemperature")
                            .map(|tk| (tk as f32 / 10.0) - 273.15)
                            .filter(|&c| c > 1.0 && c < 125.0);

                        let std_k = read_wmi_i32(&obj, "Temperature")
                            .map(|k| k as f32 - 273.15)
                            .filter(|&c| c > 1.0 && c < 125.0);

                        let Some(c) = hp.or(std_k) else {
                            continue;
                        };

                        let zone_name = read_wmi_string(&obj, "Name")
                            .unwrap_or_else(|| format!("ThermalZone {}", idx));

                        readings.push(SensorReading {
                            name: format!("ACPI ({})", zone_name),
                            sensor_type: "Zona térmica ACPI (PerfCounter)".into(),
                            current_celsius: c,
                            max_celsius: None,
                        });
                    }
                }
            }
        }

        let note = if readings.is_empty() {
            Some(
                "Esta placa base de escritorio no expone sensores de CPU en ACPI/WMI sin un driver kernel externo."
                    .into(),
            )
        } else {
            None
        };
        (readings, note)
    }
}
