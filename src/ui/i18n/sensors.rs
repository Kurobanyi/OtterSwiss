//! IN-02: textos de la vista sensors. Un campo por texto; el compilador exige los dos idiomas.

pub struct SensorsStrings {
    pub acpi_thermal_zones: &'static str,
    pub checking_nvml_adl2_wmi_nvme: &'static str,
    pub diagnostics: &'static str,
    pub level_critical: &'static str,
    pub level_high: &'static str,
    pub level_normal: &'static str,
    pub paused_note: &'static str,
    pub pause: &'static str,
    pub reset_peaks: &'static str,
    pub resume: &'static str,
    pub storage_nvme_ssd: &'static str,
    pub waiting_for_sensors: &'static str,
    pub peak_c: fn(&dyn std::fmt::Display) -> String,
    pub max_c: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: SensorsStrings = SensorsStrings {
    acpi_thermal_zones: "Zonas térmicas ACPI",
    checking_nvml_adl2_wmi_nvme: "Comprobando NVML, ADL2, WMI y NVMe.",
    diagnostics: "Diagnóstico",
    level_critical: "Crítico",
    level_high: "Alto",
    level_normal: "Normal",
    paused_note: "En pausa",
    pause: "Pausar",
    reset_peaks: "Reiniciar picos",
    resume: "Reanudar",
    storage_nvme_ssd: "Almacenamiento (NVMe / SSD)",
    waiting_for_sensors: "Esperando sensores…",
    peak_c: |pk| format!("Pico: {pk:.1} °C"),
    max_c: |max_t| format!("Máx.: {max_t:.0} °C"),
};

pub static EN: SensorsStrings = SensorsStrings {
    acpi_thermal_zones: "ACPI thermal zones",
    checking_nvml_adl2_wmi_nvme: "Checking NVML, ADL2, WMI and NVMe.",
    diagnostics: "Diagnostics",
    level_critical: "Critical",
    level_high: "High",
    level_normal: "Normal",
    paused_note: "Paused",
    pause: "Pause",
    reset_peaks: "Reset peaks",
    resume: "Resume",
    storage_nvme_ssd: "Storage (NVMe / SSD)",
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
                s.level_critical,
                s.level_high,
                s.level_normal,
                s.paused_note,
                s.pause,
                s.reset_peaks,
                s.resume,
                s.storage_nvme_ssd,
                s.waiting_for_sensors,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
