//! IN-02: textos de la vista sensors. Un campo por texto; el compilador exige los dos idiomas.

pub struct SensorsStrings {
    pub acpi_thermal_zones: &'static str,
    pub checking_nvml_adl2_wmi_nvme: &'static str,
    pub diagnostics: &'static str,
    pub live_gpu_nvme_acpi_readings: &'static str,
    pub pause: &'static str,
    pub reset_peaks: &'static str,
    pub resume: &'static str,
    pub storage_nvme_ssd: &'static str,
    pub thermal_monitor: &'static str,
    pub waiting_for_sensors: &'static str,
    pub peak_c: fn(&dyn std::fmt::Display) -> String,
    pub max_c: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: SensorsStrings = SensorsStrings {
    acpi_thermal_zones: "Zonas térmicas ACPI",
    checking_nvml_adl2_wmi_nvme: "Comprobando NVML, ADL2, WMI y NVMe.",
    diagnostics: "Diagnóstico",
    live_gpu_nvme_acpi_readings: "Lecturas en vivo de GPU, NVMe y ACPI",
    pause: "Pausar",
    reset_peaks: "Reiniciar picos",
    resume: "Reanudar",
    storage_nvme_ssd: "Almacenamiento (NVMe / SSD)",
    thermal_monitor: "Monitor Térmico",
    waiting_for_sensors: "Esperando sensores…",
    peak_c: |pk| format!("Pico: {pk:.1} °C"),
    max_c: |max_t| format!("Máx: {max_t:.0} °C"),
};

pub static EN: SensorsStrings = SensorsStrings {
    acpi_thermal_zones: "ACPI thermal zones",
    checking_nvml_adl2_wmi_nvme: "Checking NVML, ADL2, WMI & NVMe.",
    diagnostics: "Diagnostics",
    live_gpu_nvme_acpi_readings: "Live GPU, NVMe & ACPI readings",
    pause: "Pause",
    reset_peaks: "Reset Peaks",
    resume: "Resume",
    storage_nvme_ssd: "Storage (NVMe / SSD)",
    thermal_monitor: "Thermal Monitor",
    waiting_for_sensors: "Waiting for sensors…",
    peak_c: |pk| format!("Peak: {pk:.1} °C"),
    max_c: |max_t| format!("Max: {max_t:.0} °C"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.acpi_thermal_zones,
                s.checking_nvml_adl2_wmi_nvme,
                s.diagnostics,
                s.live_gpu_nvme_acpi_readings,
                s.pause,
                s.reset_peaks,
                s.resume,
                s.storage_nvme_ssd,
                s.thermal_monitor,
                s.waiting_for_sensors,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
