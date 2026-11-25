//! Sensores de temperatura: GPU (NVML, ADL2), discos y zonas ACPI, sondeados en un hilo propio.

mod adl;
mod nvml;
mod storage;
mod wmi;

use adl::AdlSession;
use nvml::NvmlSession;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::core::platform::handles::{ComGuard, SafeLibrary};

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

/// Busca `symbol` en la DLL y lo convierte al tipo de puntero a función `T`.
///
/// # Safety
/// `T` tiene que ser un `unsafe extern` fn con la firma real que exporta la DLL; `symbol` termina en NUL.
unsafe fn resolve_proc<T>(lib: &SafeLibrary, symbol: &[u8]) -> Option<T> {
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<usize>());
    // SAFETY: la DLL sigue cargada y el puntero se resolvió con la firma de su encabezado.
    let f = unsafe { lib.get_proc(symbol)? };
    // SAFETY: contrato de resolve_proc: `T` es un puntero a función del mismo tamaño que el devuelto.
    Some(unsafe { std::mem::transmute_copy(&f) })
}

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

    pub fn poll_all(&self, started: Instant) -> SystemSensorsSnapshot {
        let (mut cpu_temps, wmi_note) = self.poll_wmi_acpi();
        disambiguate_sensor_names(&mut cpu_temps);
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
    /// Intervalo de lectura de los sensores en el hilo de fondo.
    pub const DEFAULT_POLL_MS: u64 = 1000;

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
