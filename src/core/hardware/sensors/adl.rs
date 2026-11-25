//! Temperaturas de GPU AMD con ADL2 (PMLog en RDNA, OverdriveN y Overdrive5).

use std::collections::HashSet;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::mem::{size_of, zeroed};
use std::ptr;

use super::{SensorReading, resolve_proc};
use crate::core::platform::handles::SafeLibrary;

pub(super) const ADL_OK: i32 = 0;
pub(super) const AMD_VENDOR_ID_HEX: i32 = 0x1002;
pub(super) const AMD_VENDOR_ID_DEC: i32 = 1002;
pub(super) const ADL_ODN_TEMP_EDGE: i32 = 1;
pub(super) const ADL_ODN_TEMP_HOTSPOT: i32 = 2;

pub(super) const ADL_PMLOG_MAX_SENSORS: usize = 256;
pub(super) const ADL_PMLOG_TEMPERATURE_EDGE: usize = 8;
pub(super) const ADL_PMLOG_TEMPERATURE_MEM: usize = 9;
pub(super) const ADL_PMLOG_TEMPERATURE_HOTSPOT: usize = 27;

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlAdapterInfo {
    i_size: c_int,
    i_adapter_index: c_int,
    str_udid: [c_char; 256],
    i_bus_number: c_int,
    i_device_number: c_int,
    i_function_number: c_int,
    i_vendor_id: c_int,
    str_adapter_name: [c_char; 256],
    str_display_name: [c_char; 256],
    i_present: c_int,
    i_exist: c_int,
    str_driver_path: [c_char; 256],
    str_driver_path_ext: [c_char; 256],
    str_pnp_string: [c_char; 256],
    i_os_display_index: c_int,
}

#[repr(C)]
#[derive(Default)]
struct AdlTemperature {
    i_size: c_int,
    i_temperature: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AdlSingleSensorData {
    supported: c_int,
    value: c_int,
}

#[repr(C)]
struct AdlPmLogDataOutput {
    size: c_int,
    sensors: [AdlSingleSensorData; ADL_PMLOG_MAX_SENSORS],
}

impl Default for AdlPmLogDataOutput {
    fn default() -> Self {
        Self {
            size: size_of::<Self>() as c_int,
            sensors: [AdlSingleSensorData::default(); ADL_PMLOG_MAX_SENSORS],
        }
    }
}

type AdlMallocFn = unsafe extern "system" fn(c_int) -> *mut c_void;
type Adl2CreateFn = unsafe extern "system" fn(AdlMallocFn, c_int, *mut *mut c_void) -> c_int;
type Adl2DestroyFn = unsafe extern "system" fn(*mut c_void) -> c_int;
type Adl2NumAdaptersFn = unsafe extern "system" fn(*mut c_void, *mut c_int) -> c_int;
type Adl2AdapterInfoFn =
    unsafe extern "system" fn(*mut c_void, *mut AdlAdapterInfo, c_int) -> c_int;
type Adl2PmLogGetFn =
    unsafe extern "system" fn(*mut c_void, c_int, *mut AdlPmLogDataOutput) -> c_int;
type Adl2OdnTempFn = unsafe extern "system" fn(*mut c_void, c_int, c_int, *mut c_int) -> c_int;
type Adl2Od5TempFn =
    unsafe extern "system" fn(*mut c_void, c_int, c_int, *mut AdlTemperature) -> c_int;

unsafe extern "system" fn adl_malloc(size: c_int) -> *mut c_void {
    if size <= 0 {
        return ptr::null_mut();
    }
    let layout = match std::alloc::Layout::from_size_align(size as usize, 8) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };
    // SAFETY: `layout` tiene tamaño > 0 (comprobado arriba) y alineación válida.
    unsafe { std::alloc::alloc_zeroed(layout) as *mut c_void }
}

pub(super) struct AdlSession {
    _lib: SafeLibrary,
    ctx: *mut c_void,
    destroy: Adl2DestroyFn,
    num_adapters: Adl2NumAdaptersFn,
    adapter_info: Adl2AdapterInfoFn,
    pmlog_get: Option<Adl2PmLogGetFn>,
    odn_temp: Option<Adl2OdnTempFn>,
    od5_temp: Option<Adl2Od5TempFn>,
}

#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "contexto y punteros de la DLL; ver SAFETY"
)]
// SAFETY: el contexto ADL2 se crea, se usa y se destruye en el hilo de sondeo que posee la sesión; ADL2 admite un
// contexto por hilo y la sesión no es `Sync`, así que nunca se usa desde dos hilos a la vez.
unsafe impl Send for AdlSession {}

impl AdlSession {
    pub(super) fn try_init() -> Option<Self> {
        let lib = SafeLibrary::load_system("atiadlxx.dll")
            .or_else(|_| SafeLibrary::load_system("atiadlxy.dll"))
            .ok()?;

        // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
        unsafe {
            let create: Adl2CreateFn = resolve_proc(&lib, b"ADL2_Main_Control_Create\0")?;
            let destroy: Adl2DestroyFn = resolve_proc(&lib, b"ADL2_Main_Control_Destroy\0")?;
            let num_adapters: Adl2NumAdaptersFn =
                resolve_proc(&lib, b"ADL2_Adapter_NumberOfAdapters_Get\0")?;
            let adapter_info: Adl2AdapterInfoFn =
                resolve_proc(&lib, b"ADL2_Adapter_AdapterInfo_Get\0")?;
            let pmlog_get: Option<Adl2PmLogGetFn> =
                resolve_proc(&lib, b"ADL2_New_QueryPMLogData_Get\0");
            let odn_temp: Option<Adl2OdnTempFn> =
                resolve_proc(&lib, b"ADL2_OverdriveN_Temperature_Get\0");
            let od5_temp: Option<Adl2Od5TempFn> =
                resolve_proc(&lib, b"ADL2_Overdrive5_Temperature_Get\0");

            let mut ctx: *mut c_void = ptr::null_mut();
            // 0 = enumerar todos los adaptadores (evita omitir GPUs si iEnumConnectedAdapters=1 falla)
            if create(adl_malloc, 0, &mut ctx) != ADL_OK || ctx.is_null() {
                return None;
            }

            Some(Self {
                _lib: lib,
                ctx,
                destroy,
                num_adapters,
                adapter_info,
                pmlog_get,
                odn_temp,
                od5_temp,
            })
        }
    }

    pub(super) fn poll(&self) -> (Vec<SensorReading>, Option<String>) {
        let mut out = Vec::new();
        let mut count: c_int = 0;
        // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
        let num_rc = unsafe { (self.num_adapters)(self.ctx, &mut count) };
        if num_rc != ADL_OK || count <= 0 {
            return (
                out,
                Some(format!(
                    "ADL2 sin adaptadores reportados (código {}, total {})",
                    num_rc, count
                )),
            );
        }

        let capped = (count as usize).min(64);
        let mut infos: Vec<AdlAdapterInfo> = (0..capped)
            // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
            .map(|_| unsafe {
                let mut info: AdlAdapterInfo = zeroed();
                info.i_size = size_of::<AdlAdapterInfo>() as c_int;
                info
            })
            .collect();

        let bytes = (capped * size_of::<AdlAdapterInfo>()) as c_int;
        // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
        let info_rc = unsafe { (self.adapter_info)(self.ctx, infos.as_mut_ptr(), bytes) };
        if info_rc != ADL_OK {
            return (
                out,
                Some(format!(
                    "ADL2_Adapter_AdapterInfo_Get falló (código {})",
                    info_rc
                )),
            );
        }

        let mut seen_pci = HashSet::new();
        let mut last_diag = String::new();

        for info in infos {
            if info.i_vendor_id != AMD_VENDOR_ID_HEX && info.i_vendor_id != AMD_VENDOR_ID_DEC {
                continue;
            }
            if info.i_exist == 0 {
                continue;
            }
            let pci_key = (
                info.i_bus_number,
                info.i_device_number,
                info.i_function_number,
            );
            if seen_pci.contains(&pci_key) {
                continue;
            }

            // SAFETY: la cadena viene de la estructura de la DLL y termina en NUL dentro de su tamaño fijo.
            let raw_name = unsafe { CStr::from_ptr(info.str_adapter_name.as_ptr()) }
                .to_string_lossy()
                .trim()
                .to_string();
            let base_name = if raw_name.is_empty() {
                format!("AMD Radeon GPU {}", info.i_adapter_index)
            } else {
                raw_name
            };

            let mut found_for_this_gpu = false;

            // 1) Ruta moderna RDNA (RX 5000 / 6000 / 7000): ADL2_New_QueryPMLogData_Get (valores en °C enteros)
            if let Some(pmlog) = self.pmlog_get {
                let mut pm = AdlPmLogDataOutput::default();
                // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
                let rc = unsafe { pmlog(self.ctx, info.i_adapter_index, &mut pm) };
                if rc == ADL_OK {
                    let edge = pm.sensors[ADL_PMLOG_TEMPERATURE_EDGE];
                    if edge.supported != 0 && edge.value > 0 && edge.value < 135 {
                        out.push(SensorReading {
                            name: format!("{} (GPU Edge)", base_name),
                            sensor_type: "AMD GPU (ADL2 PMLog)".into(),
                            current_celsius: edge.value as f32,
                            max_celsius: Some(110.0),
                        });
                        found_for_this_gpu = true;
                    }

                    let hotspot = pm.sensors[ADL_PMLOG_TEMPERATURE_HOTSPOT];
                    if hotspot.supported != 0 && hotspot.value > 0 && hotspot.value < 140 {
                        out.push(SensorReading {
                            name: format!("{} (Hotspot)", base_name),
                            sensor_type: "AMD GPU (ADL2 PMLog)".into(),
                            current_celsius: hotspot.value as f32,
                            max_celsius: Some(110.0),
                        });
                        found_for_this_gpu = true;
                    }

                    let mem = pm.sensors[ADL_PMLOG_TEMPERATURE_MEM];
                    if mem.supported != 0 && mem.value > 0 && mem.value < 130 {
                        out.push(SensorReading {
                            name: format!("{} (VRAM)", base_name),
                            sensor_type: "AMD GPU (ADL2 PMLog)".into(),
                            current_celsius: mem.value as f32,
                            max_celsius: None,
                        });
                        found_for_this_gpu = true;
                    }
                } else {
                    last_diag = format!("PMLog rc={} idx={}", rc, info.i_adapter_index);
                }
            }

            // 2) Fallback OverdriveN (milli-°C)
            if !found_for_this_gpu && let Some(odn) = self.odn_temp {
                let mut milli_c: c_int = 0;
                // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
                let rc_edge = unsafe {
                    odn(
                        self.ctx,
                        info.i_adapter_index,
                        ADL_ODN_TEMP_EDGE,
                        &mut milli_c,
                    )
                };
                if rc_edge == ADL_OK {
                    let c = milli_c as f32 / 1000.0;
                    if c > 0.0 && c < 135.0 {
                        out.push(SensorReading {
                            name: format!("{} (Core)", base_name),
                            sensor_type: "AMD GPU (ADL2 ODN)".into(),
                            current_celsius: c,
                            max_celsius: None,
                        });
                        found_for_this_gpu = true;
                    }
                } else if last_diag.is_empty() {
                    last_diag = format!("ODN rc={} idx={}", rc_edge, info.i_adapter_index);
                }

                let mut hotspot_mc: c_int = 0;
                // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
                if unsafe {
                    odn(
                        self.ctx,
                        info.i_adapter_index,
                        ADL_ODN_TEMP_HOTSPOT,
                        &mut hotspot_mc,
                    )
                } == ADL_OK
                {
                    let c = hotspot_mc as f32 / 1000.0;
                    if c > 0.0 && c < 140.0 {
                        out.push(SensorReading {
                            name: format!("{} (Hotspot)", base_name),
                            sensor_type: "AMD GPU (ADL2 ODN)".into(),
                            current_celsius: c,
                            max_celsius: None,
                        });
                        found_for_this_gpu = true;
                    }
                }
            }

            // 3) Fallback Overdrive5 (milli-°C)
            if !found_for_this_gpu && let Some(od5) = self.od5_temp {
                let mut adl_t = AdlTemperature {
                    i_size: size_of::<AdlTemperature>() as c_int,
                    i_temperature: 0,
                };
                // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
                let rc5 = unsafe { od5(self.ctx, info.i_adapter_index, 0, &mut adl_t) };
                if rc5 == ADL_OK {
                    let c = adl_t.i_temperature as f32 / 1000.0;
                    if c > 0.0 && c < 135.0 {
                        out.push(SensorReading {
                            name: base_name,
                            sensor_type: "AMD GPU (ADL2 OD5)".into(),
                            current_celsius: c,
                            max_celsius: None,
                        });
                        found_for_this_gpu = true;
                    }
                } else if last_diag.is_empty() {
                    last_diag = format!("OD5 rc={} idx={}", rc5, info.i_adapter_index);
                }
            }

            if found_for_this_gpu {
                seen_pci.insert(pci_key);
            }
        }

        let note = if out.is_empty() {
            Some(format!(
                "ADL2 inicializado ({} entradas), sin temperaturas reportadas ({})",
                count, last_diag
            ))
        } else {
            None
        };
        (out, note)
    }
}

impl Drop for AdlSession {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            // SAFETY: puntero de atiadlxx.dll con la firma de su encabezado; el contexto ADL2 y la DLL siguen vivos mientras viva la sesión.
            unsafe {
                let _ = (self.destroy)(self.ctx);
            }
            self.ctx = ptr::null_mut();
        }
    }
}
