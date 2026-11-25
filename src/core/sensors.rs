use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::mem::{size_of, zeroed};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::core::{BSTR, PCWSTR, VARIANT};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoSetProxyBlanket, CoTaskMemFree, CLSCTX_INPROC_SERVER, EOAC_NONE,
    RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceTemperatureProperty, IOCTL_STORAGE_QUERY_PROPERTY,
    STORAGE_PROPERTY_QUERY, STORAGE_TEMPERATURE_DATA_DESCRIPTOR, STORAGE_TEMPERATURE_INFO,
};
use windows::Win32::System::Variant::{VariantClear, VariantToInt32, VariantToStringAlloc};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY,
};
use windows::Win32::System::IO::DeviceIoControl;

use crate::core::safe_fs::open_verified_file_read;
use crate::core::trust::verify_authenticode_on_handle;
use crate::core::win_common::{to_wide_null, AlignedBuf, ComGuard, SafeHandle, SafeLibrary};

const RPC_C_AUTHN_WINNT: u32 = 10;
const RPC_C_AUTHZ_NONE: u32 = 0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SensorReading {
    pub name: String,
    pub sensor_type: String,
    pub current_celsius: f32,
    pub max_celsius: Option<f32>,
}

/// UI-14: dos GPU del mismo modelo dan el mismo nombre; la UI usa el nombre como clave y para picos.
/// Desde la segunda aparición se agrega « #n».
pub fn disambiguate_sensor_names(readings: &mut [SensorReading]) {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in readings.iter_mut() {
        let count = seen.entry(r.name.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            r.name = format!("{} #{}", r.name, count);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemSensorsSnapshot {
    pub cpu_temperatures: Vec<SensorReading>,
    pub gpu_temperatures: Vec<SensorReading>,
    pub storage_temperatures: Vec<SensorReading>,
    pub backend_notes: Vec<String>,
    pub updated_at_ms: u64,
}

unsafe fn resolve_proc<T>(lib: &SafeLibrary, symbol: &[u8]) -> Option<T> {
    let f = unsafe { lib.get_proc(symbol)? };
    Some(unsafe { std::mem::transmute_copy(&f) })
}

// ============================================================================
// NVIDIA NVML
// ============================================================================
const NVML_SUCCESS: i32 = 0;
const NVML_TEMPERATURE_GPU: u32 = 0;
const NVML_TEMPERATURE_THRESHOLD_SLOWDOWN: u32 = 1;

type NvmlInitFn = unsafe extern "C" fn() -> i32;
type NvmlShutdownFn = unsafe extern "C" fn() -> i32;
type NvmlDeviceGetCountFn = unsafe extern "C" fn(*mut c_uint) -> i32;
type NvmlDeviceGetHandleByIndexFn = unsafe extern "C" fn(c_uint, *mut *mut c_void) -> i32;
type NvmlDeviceGetNameFn = unsafe extern "C" fn(*mut c_void, *mut c_char, c_uint) -> i32;
type NvmlDeviceGetTemperatureFn = unsafe extern "C" fn(*mut c_void, c_uint, *mut c_uint) -> i32;
type NvmlDeviceGetTempThresholdFn = unsafe extern "C" fn(*mut c_void, c_uint, *mut c_uint) -> i32;

struct NvmlSession {
    _lib: SafeLibrary,
    initialized: bool,
    shutdown: NvmlShutdownFn,
    get_count: NvmlDeviceGetCountFn,
    get_handle: NvmlDeviceGetHandleByIndexFn,
    get_name: Option<NvmlDeviceGetNameFn>,
    get_temp: NvmlDeviceGetTemperatureFn,
    get_threshold: Option<NvmlDeviceGetTempThresholdFn>,
}

// SAFETY: la sesión se crea y se usa solo desde el hilo de sondeo que la posee; NVML admite llamadas desde cualquier hilo
// y los punteros a funciones no se comparten. Ningún `&NvmlSession` cruza hilos (no es `Sync`).
#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "punteros de la DLL; ver SAFETY"
)]
unsafe impl Send for NvmlSession {}

impl NvmlSession {
    fn try_init() -> Option<Self> {
        let lib = match SafeLibrary::load_system("nvml.dll") {
            Ok(l) => l,
            Err(_) => {
                // Carpeta real de Program Files (no `%ProgramFiles%`, redefinible por el usuario) y DLL
                // firmada por NVIDIA: el proceso corre elevado y cargar una DLL ajena sería ejecutar código.
                let pf = crate::core::win_common::program_files_dir()?;
                let fallback = pf.join(r"NVIDIA Corporation\NVSMI\nvml.dll");
                let (file, _) = open_verified_file_read(&fallback, true, None).ok()?;
                let handle = windows::Win32::Foundation::HANDLE(
                    std::os::windows::io::AsRawHandle::as_raw_handle(&file) as _,
                );
                verify_authenticode_on_handle(&fallback, handle, "NVIDIA Corporation", None)
                    .ok()?;
                // `file` sigue abierto (solo lectura compartida) hasta después de cargar la DLL.
                let lib = SafeLibrary::load_from_verified_path(&fallback).ok()?;
                drop(file);
                lib
            }
        };

        unsafe {
            let init: NvmlInitFn = resolve_proc(&lib, b"nvmlInit_v2\0")
                .or_else(|| resolve_proc(&lib, b"nvmlInit\0"))?;
            let shutdown: NvmlShutdownFn = resolve_proc(&lib, b"nvmlShutdown\0")?;
            let get_count: NvmlDeviceGetCountFn = resolve_proc(&lib, b"nvmlDeviceGetCount_v2\0")
                .or_else(|| resolve_proc(&lib, b"nvmlDeviceGetCount\0"))?;
            let get_handle: NvmlDeviceGetHandleByIndexFn =
                resolve_proc(&lib, b"nvmlDeviceGetHandleByIndex_v2\0")
                    .or_else(|| resolve_proc(&lib, b"nvmlDeviceGetHandleByIndex\0"))?;
            let get_name: Option<NvmlDeviceGetNameFn> = resolve_proc(&lib, b"nvmlDeviceGetName\0");
            let get_temp: NvmlDeviceGetTemperatureFn =
                resolve_proc(&lib, b"nvmlDeviceGetTemperature\0")?;
            let get_threshold: Option<NvmlDeviceGetTempThresholdFn> =
                resolve_proc(&lib, b"nvmlDeviceGetTemperatureThreshold\0");

            if init() != NVML_SUCCESS {
                return None;
            }

            Some(Self {
                _lib: lib,
                initialized: true,
                shutdown,
                get_count,
                get_handle,
                get_name,
                get_temp,
                get_threshold,
            })
        }
    }

    fn poll(&self) -> Vec<SensorReading> {
        let mut out = Vec::new();
        let mut count: c_uint = 0;
        if unsafe { (self.get_count)(&mut count) } != NVML_SUCCESS {
            return out;
        }

        for idx in 0..count.min(8) {
            let mut dev: *mut c_void = ptr::null_mut();
            if unsafe { (self.get_handle)(idx, &mut dev) } != NVML_SUCCESS || dev.is_null() {
                continue;
            }

            let mut temp: c_uint = 0;
            if unsafe { (self.get_temp)(dev, NVML_TEMPERATURE_GPU, &mut temp) } != NVML_SUCCESS {
                continue;
            }
            if temp == 0 || temp > 130 {
                continue;
            }

            let name = self
                .get_name
                .and_then(|f| {
                    let mut buf = [0 as c_char; 96];
                    if unsafe { f(dev, buf.as_mut_ptr(), buf.len() as c_uint) } == NVML_SUCCESS {
                        let s = unsafe { CStr::from_ptr(buf.as_ptr()) }
                            .to_string_lossy()
                            .trim()
                            .to_string();
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                    None
                })
                .unwrap_or_else(|| format!("NVIDIA GPU {}", idx));

            let max_temp = self.get_threshold.and_then(|f| {
                let mut th: c_uint = 0;
                if unsafe { f(dev, NVML_TEMPERATURE_THRESHOLD_SLOWDOWN, &mut th) } == NVML_SUCCESS
                    && th > 0
                    && th < 150
                {
                    Some(th as f32)
                } else {
                    None
                }
            });

            out.push(SensorReading {
                name,
                sensor_type: "NVIDIA GPU (NVML)".into(),
                current_celsius: temp as f32,
                max_celsius: max_temp,
            });
        }

        out
    }
}

impl Drop for NvmlSession {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                let _ = (self.shutdown)();
            }
            self.initialized = false;
        }
    }
}

// ============================================================================
// AMD ADL2 (PMLog RDNA + OverdriveN + Overdrive5)
// ============================================================================
const ADL_OK: i32 = 0;
const AMD_VENDOR_ID_HEX: i32 = 0x1002;
const AMD_VENDOR_ID_DEC: i32 = 1002;
const ADL_ODN_TEMP_EDGE: i32 = 1;
const ADL_ODN_TEMP_HOTSPOT: i32 = 2;

const ADL_PMLOG_MAX_SENSORS: usize = 256;
const ADL_PMLOG_TEMPERATURE_EDGE: usize = 8;
const ADL_PMLOG_TEMPERATURE_MEM: usize = 9;
const ADL_PMLOG_TEMPERATURE_HOTSPOT: usize = 27;

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

struct AdlSession {
    _lib: SafeLibrary,
    ctx: *mut c_void,
    destroy: Adl2DestroyFn,
    num_adapters: Adl2NumAdaptersFn,
    adapter_info: Adl2AdapterInfoFn,
    pmlog_get: Option<Adl2PmLogGetFn>,
    odn_temp: Option<Adl2OdnTempFn>,
    od5_temp: Option<Adl2Od5TempFn>,
}

// SAFETY: el contexto ADL2 se crea, se usa y se destruye en el hilo de sondeo que posee la sesión; ADL2 admite un
// contexto por hilo y la sesión no es `Sync`, así que nunca se usa desde dos hilos a la vez.
#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "contexto y punteros de la DLL; ver SAFETY"
)]
unsafe impl Send for AdlSession {}

impl AdlSession {
    fn try_init() -> Option<Self> {
        let lib = SafeLibrary::load_system("atiadlxx.dll")
            .or_else(|_| SafeLibrary::load_system("atiadlxy.dll"))
            .ok()?;

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

    fn poll(&self) -> (Vec<SensorReading>, Option<String>) {
        let mut out = Vec::new();
        let mut count: c_int = 0;
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
            .map(|_| unsafe {
                let mut info: AdlAdapterInfo = zeroed();
                info.i_size = size_of::<AdlAdapterInfo>() as c_int;
                info
            })
            .collect();

        let bytes = (capped * size_of::<AdlAdapterInfo>()) as c_int;
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
            if !found_for_this_gpu {
                if let Some(odn) = self.odn_temp {
                    let mut milli_c: c_int = 0;
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
            }

            // 3) Fallback Overdrive5 (milli-°C)
            if !found_for_this_gpu {
                if let Some(od5) = self.od5_temp {
                    let mut adl_t = AdlTemperature {
                        i_size: size_of::<AdlTemperature>() as c_int,
                        i_temperature: 0,
                    };
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
            unsafe {
                let _ = (self.destroy)(self.ctx);
            }
            self.ctx = ptr::null_mut();
        }
    }
}

// ============================================================================
// Sesión unificada de sensores
// ============================================================================
pub struct SensorSession {
    nvml: Option<NvmlSession>,
    adl: Option<AdlSession>,
}

impl SensorSession {
    pub fn new() -> Self {
        Self {
            nvml: NvmlSession::try_init(),
            adl: AdlSession::try_init(),
        }
    }

    pub fn poll_storage(&self) -> Vec<SensorReading> {
        let mut readings = Vec::new();

        for drive_num in 0..8 {
            let path_str = format!(r"\\.\PhysicalDrive{}", drive_num);
            let wide = to_wide_null(&path_str);

            let raw = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    FILE_GENERIC_READ.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    Default::default(),
                    None,
                )
            };

            let handle = match SafeHandle::new(raw.unwrap_or(HANDLE(ptr::null_mut()))) {
                Some(h) => h,
                None => continue,
            };

            let query = STORAGE_PROPERTY_QUERY {
                PropertyId: StorageDeviceTemperatureProperty,
                QueryType: PropertyStandardQuery,
                AdditionalParameters: [0; 1],
            };

            let mut header = STORAGE_TEMPERATURE_DATA_DESCRIPTOR::default();
            let mut returned = 0u32;

            let ok = unsafe {
                DeviceIoControl(
                    handle.raw(),
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    Some(&query as *const _ as *const _),
                    size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                    Some(&mut header as *mut _ as *mut _),
                    size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>() as u32,
                    Some(&mut returned),
                    None,
                )
            };

            if ok.is_err()
                || (header.Size as usize) < size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>()
            {
                continue;
            }

            let buf_size = (header.Size as usize).min(4096);
            let mut full_buf = AlignedBuf::zeroed(buf_size);
            let full_ok = unsafe {
                DeviceIoControl(
                    handle.raw(),
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    Some(&query as *const _ as *const _),
                    size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                    Some(full_buf.as_mut_ptr() as *mut _),
                    buf_size as u32,
                    Some(&mut returned),
                    None,
                )
            };

            if full_ok.is_err()
                || (returned as usize) < size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>()
            {
                continue;
            }

            let desc =
                unsafe { &*(full_buf.as_ptr() as *const STORAGE_TEMPERATURE_DATA_DESCRIPTOR) };

            let info_offset = size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>()
                - size_of::<STORAGE_TEMPERATURE_INFO>();
            let max_possible = (returned as usize).saturating_sub(info_offset)
                / size_of::<STORAGE_TEMPERATURE_INFO>();
            let count = (desc.InfoCount as usize).min(max_possible);

            if count == 0 {
                continue;
            }

            let infos = unsafe { std::slice::from_raw_parts(desc.TemperatureInfo.as_ptr(), count) };

            let crit = if desc.CriticalTemperature > 0 && desc.CriticalTemperature < 150 {
                Some(desc.CriticalTemperature as f32)
            } else {
                None
            };

            for info in infos {
                if info.Temperature > 0 && info.Temperature < 125 {
                    let label = if count > 1 {
                        format!("Disco {} (Sensor {})", drive_num, info.Index)
                    } else {
                        format!("Disco {}", drive_num)
                    };
                    let over = (info.OverThreshold > 0 && info.OverThreshold < 150)
                        .then_some(info.OverThreshold as f32);
                    let max_c = crit.or(over);

                    readings.push(SensorReading {
                        name: label,
                        sensor_type: "NVMe / SSD Storage".into(),
                        current_celsius: info.Temperature as f32,
                        max_celsius: max_c,
                    });
                }
            }
        }

        readings
    }

    pub fn poll_gpu(&self) -> (Vec<SensorReading>, Option<String>) {
        let mut gpus = Vec::new();
        let mut note = None;
        if let Some(nv) = &self.nvml {
            gpus.extend(nv.poll());
        }
        if let Some(amd) = &self.adl {
            let (amd_readings, amd_note) = amd.poll();
            gpus.extend(amd_readings);
            note = amd_note;
        }
        (gpus, note)
    }

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

    pub fn poll_all(&self, started: Instant) -> SystemSensorsSnapshot {
        let (cpu_temps, wmi_note) = self.poll_wmi_acpi();
        let (mut gpu_temps, gpu_note) = self.poll_gpu();
        disambiguate_sensor_names(&mut gpu_temps);
        let storage_temps = self.poll_storage();

        let mut notes = Vec::new();
        if self.nvml.is_none() && self.adl.is_none() {
            notes.push("NVML y ADL2 no detectados (sin driver NVIDIA/AMD compatible)".into());
        }
        if let Some(gn) = gpu_note {
            notes.push(gn);
        }
        if let Some(n) = wmi_note {
            notes.push(n);
        }

        SystemSensorsSnapshot {
            cpu_temperatures: cpu_temps,
            gpu_temperatures: gpu_temps,
            storage_temperatures: storage_temps,
            backend_notes: notes,
            updated_at_ms: started.elapsed().as_millis() as u64,
        }
    }
}

fn read_wmi_i32(obj: &IWbemClassObject, prop: &str) -> Option<i32> {
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

fn read_wmi_string(obj: &IWbemClassObject, prop: &str) -> Option<String> {
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

impl Default for SensorSession {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SensorMonitor {
    cached_snapshot: Arc<Mutex<SystemSensorsSnapshot>>,
    stop_flag: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Default for SensorMonitor {
    fn default() -> Self {
        Self::new(1500)
    }
}

impl SensorMonitor {
    pub fn new(poll_interval_ms: u64) -> Self {
        let cached = Arc::new(Mutex::new(SystemSensorsSnapshot::default()));
        let cached_clone = Arc::clone(&cached);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);

        let worker = thread::Builder::new()
            .name("OtterSwiss_SensorsWorker".into())
            .spawn(move || {
                let _com = ComGuard::init_mt();
                let session = SensorSession::new();
                let interval = Duration::from_millis(poll_interval_ms.max(250));
                let started = Instant::now();

                while !stop_clone.load(Ordering::Acquire) {
                    let snap = session.poll_all(started);
                    if let Ok(mut lock) = cached_clone.lock() {
                        *lock = snap;
                    }

                    let mut slept = Duration::ZERO;
                    while slept < interval && !stop_clone.load(Ordering::Acquire) {
                        let step = Duration::from_millis(100).min(interval - slept);
                        thread::sleep(step);
                        slept += step;
                    }
                }
            })
            .ok();

        Self {
            cached_snapshot: cached,
            stop_flag,
            worker,
        }
    }

    pub fn get_snapshot(&self) -> SystemSensorsSnapshot {
        self.cached_snapshot
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Release);
    }
}

impl Drop for SensorMonitor {
    /// UI-05: no espera al hilo (un `poll_all` WMI puede tardar decenas de segundos y bloquearía la UI
    /// al salir de Sensores). El hilo ve la bandera y termina solo; su sesión se libera dentro de él.
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        drop(self.worker.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(name: &str) -> SensorReading {
        SensorReading {
            name: name.into(),
            sensor_type: "GPU".into(),
            current_celsius: 50.0,
            max_celsius: None,
        }
    }

    /// UI-14: dos GPU del mismo modelo no comparten nombre (clave de la UI y de los picos).
    #[test]
    fn nombres_de_sensores_repetidos_se_distinguen() {
        let mut list = vec![reading("RTX 3080"), reading("RTX 3080"), reading("RX 7900")];
        disambiguate_sensor_names(&mut list);
        let names: Vec<&str> = list.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["RTX 3080", "RTX 3080 #2", "RX 7900"]);
    }
}
