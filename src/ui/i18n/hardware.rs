//! IN-02: textos de la vista hardware. Un campo por texto; el compilador exige los dos idiomas.

pub struct HardwareStrings {
    pub arch: &'static str,
    pub copied: &'static str,
    pub copy_summary: &'static str,
    pub cores: &'static str,
    pub could_not_read_the_hardware: &'static str,
    pub empty_slot: &'static str,
    pub export_json: &'static str,
    pub hardware_specifications: &'static str,
    pub motherboard_bios: &'static str,
    pub scanning_cpu_smbios_dxgi: &'static str,
    pub some_data_could_not_be: &'static str,
    pub storage: &'static str,
    pub vendor: &'static str,
    pub saved: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: HardwareStrings = HardwareStrings {
    arch: "Arq.",
    copied: "¡Copiado!",
    copy_summary: "Copiar resumen",
    cores: "Núcleos",
    could_not_read_the_hardware: "No se pudo leer el hardware",
    empty_slot: "Ranura libre",
    export_json: "Exportar JSON",
    hardware_specifications: "Especificaciones del sistema",
    motherboard_bios: "PLACA BASE Y BIOS",
    scanning_cpu_smbios_dxgi: "Analizando CPU, SMBIOS y DXGI…",
    some_data_could_not_be: "Algunos datos no se pudieron leer:",
    storage: "ALMACENAMIENTO",
    vendor: "Fabricante",
    saved: |path| format!("Guardado: {path}"),
};

pub static EN: HardwareStrings = HardwareStrings {
    arch: "Arch",
    copied: "Copied!",
    copy_summary: "Copy Summary",
    cores: "Cores",
    could_not_read_the_hardware: "Could not read the hardware",
    empty_slot: "Empty slot",
    export_json: "Export JSON",
    hardware_specifications: "Hardware Specifications",
    motherboard_bios: "MOTHERBOARD & BIOS",
    scanning_cpu_smbios_dxgi: "Scanning CPU, SMBIOS & DXGI…",
    some_data_could_not_be: "Some data could not be read:",
    storage: "STORAGE",
    vendor: "Vendor",
    saved: |path| format!("Saved: {path}"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.arch,
                s.copied,
                s.copy_summary,
                s.cores,
                s.could_not_read_the_hardware,
                s.empty_slot,
                s.export_json,
                s.hardware_specifications,
                s.motherboard_bios,
                s.scanning_cpu_smbios_dxgi,
                s.some_data_could_not_be,
                s.storage,
                s.vendor,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
