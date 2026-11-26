//! IN-02: textos de la vista hardware. Un campo por texto; el compilador exige los dos idiomas.

pub struct HardwareStrings {
    pub arch: &'static str,
    pub board: &'static str,
    pub build: &'static str,
    pub copied: &'static str,
    pub copy_summary: &'static str,
    pub cores: &'static str,
    pub could_not_read_the_hardware: &'static str,
    pub empty_slot: &'static str,
    pub export_json: &'static str,
    pub graphics: &'static str,
    pub instruction_sets: &'static str,
    pub model: &'static str,
    pub motherboard_bios: &'static str,
    pub processor: &'static str,
    pub revision: &'static str,
    pub scanning_cpu_smbios_dxgi: &'static str,
    pub slots_in_use: &'static str,
    pub some_data_could_not_be: &'static str,
    pub storage: &'static str,
    pub system: &'static str,
    pub temperature: &'static str,
    pub total: &'static str,
    pub vendor: &'static str,
    pub windows: &'static str,
    pub summary_board: &'static str,
    pub summary_gpu_none: &'static str,
    pub disk: fn(&dyn std::fmt::Display) -> String,
    pub saved: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: HardwareStrings = HardwareStrings {
    arch: "Arquitectura",
    board: "Placa base",
    build: "Compilación",
    copied: "Resumen copiado al portapapeles.",
    copy_summary: "Copiar resumen",
    summary_board: "Placa",
    summary_gpu_none: "No detectada",
    cores: "Núcleos",
    could_not_read_the_hardware: "No se pudo leer el hardware",
    empty_slot: "Ranura libre",
    export_json: "Exportar JSON",
    graphics: "Gráficos",
    instruction_sets: "Instrucciones",
    model: "Modelo",
    motherboard_bios: "Placa base y BIOS",
    processor: "Procesador",
    revision: "Revisión",
    scanning_cpu_smbios_dxgi: "Analizando CPU, SMBIOS y DXGI…",
    slots_in_use: "Ranuras ocupadas",
    some_data_could_not_be: "Algunos datos no se pudieron leer",
    storage: "Almacenamiento",
    system: "Sistema",
    temperature: "Temperatura",
    total: "Total",
    vendor: "Fabricante",
    windows: "Windows",
    disk: |n| format!("Disco {n}"),
    saved: |path| format!("Guardado en {path}"),
};

pub static EN: HardwareStrings = HardwareStrings {
    arch: "Architecture",
    board: "Motherboard",
    build: "Build",
    copied: "Summary copied to the clipboard.",
    copy_summary: "Copy summary",
    summary_board: "Board",
    summary_gpu_none: "Not detected",
    cores: "Cores",
    could_not_read_the_hardware: "Could not read the hardware",
    empty_slot: "Empty slot",
    export_json: "Export JSON",
    graphics: "Graphics",
    instruction_sets: "Instruction sets",
    model: "Model",
    motherboard_bios: "Motherboard and BIOS",
    processor: "Processor",
    revision: "Revision",
    scanning_cpu_smbios_dxgi: "Scanning CPU, SMBIOS and DXGI…",
    slots_in_use: "Slots in use",
    some_data_could_not_be: "Some data could not be read",
    storage: "Storage",
    system: "System",
    temperature: "Temperature",
    total: "Total",
    vendor: "Vendor",
    windows: "Windows",
    disk: |n| format!("Disk {n}"),
    saved: |path| format!("Saved to {path}"),
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
                s.board,
                s.build,
                s.copied,
                s.copy_summary,
                s.cores,
                s.could_not_read_the_hardware,
                s.empty_slot,
                s.export_json,
                s.graphics,
                s.instruction_sets,
                s.model,
                s.motherboard_bios,
                s.processor,
                s.revision,
                s.scanning_cpu_smbios_dxgi,
                s.slots_in_use,
                s.some_data_could_not_be,
                s.storage,
                s.system,
                s.temperature,
                s.total,
                s.vendor,
                s.windows,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
