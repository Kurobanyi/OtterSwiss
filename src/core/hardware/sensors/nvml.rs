//! Temperaturas de GPU NVIDIA con NVML (`nvml.dll`, firmada por NVIDIA).

use std::ffi::{c_char, c_uint, c_void, CStr};
use std::ptr;

use super::{resolve_proc, SensorReading};
use crate::core::platform::handles::SafeLibrary;
use crate::core::platform::safe_fs::open_verified_file_read;
use crate::core::security::authenticode::verify_authenticode_on_handle;

pub(super) const NVML_SUCCESS: i32 = 0;
pub(super) const NVML_TEMPERATURE_GPU: u32 = 0;
pub(super) const NVML_TEMPERATURE_THRESHOLD_SLOWDOWN: u32 = 1;

type NvmlInitFn = unsafe extern "C" fn() -> i32;
type NvmlShutdownFn = unsafe extern "C" fn() -> i32;
type NvmlDeviceGetCountFn = unsafe extern "C" fn(*mut c_uint) -> i32;
type NvmlDeviceGetHandleByIndexFn = unsafe extern "C" fn(c_uint, *mut *mut c_void) -> i32;
type NvmlDeviceGetNameFn = unsafe extern "C" fn(*mut c_void, *mut c_char, c_uint) -> i32;
type NvmlDeviceGetTemperatureFn = unsafe extern "C" fn(*mut c_void, c_uint, *mut c_uint) -> i32;
type NvmlDeviceGetTempThresholdFn = unsafe extern "C" fn(*mut c_void, c_uint, *mut c_uint) -> i32;

pub(super) struct NvmlSession {
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
    pub(super) fn try_init() -> Option<Self> {
        let lib = match SafeLibrary::load_system("nvml.dll") {
            Ok(l) => l,
            Err(_) => {
                // Carpeta real de Program Files (no `%ProgramFiles%`, redefinible por el usuario) y DLL
                // firmada por NVIDIA: el proceso corre elevado y cargar una DLL ajena sería ejecutar código.
                let pf = crate::core::platform::folders::program_files_dir()?;
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

    pub(super) fn poll(&self) -> Vec<SensorReading> {
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
