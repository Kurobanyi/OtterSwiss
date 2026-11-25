use crate::core::hardware::specs::SystemSpecs;
use crate::ui::components::{ErrorState, LoadingState, StatusBanner, Tone};
use crate::ui::i18n::Lang;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct HardwareViewProps {
    pub specs: Option<SystemSpecs>,
    #[props(default = None)]
    pub error: Option<String>,
    #[props(default = None)]
    pub cpu_temp_c: Option<f32>,
    #[props(default = None)]
    pub gpu_edge_temp_c: Option<f32>,
    #[props(default = None)]
    pub gpu_hotspot_temp_c: Option<f32>,
}

#[component]
pub fn HardwareView(props: HardwareViewProps) -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let mut status_msg = use_signal(|| None::<String>);

    let Some(specs) = props.specs.clone() else {
        if let Some(err) = props.error {
            return rsx! {
                ErrorState {
                    title: if en { "Could not read the hardware".to_string() } else { "No se pudo leer el hardware".to_string() },
                    error_msg: err,
                }
            };
        }
        return rsx! {
            LoadingState {
                message: if en { "Scanning CPU, SMBIOS & DXGI...".to_string() } else { "Analizando CPU, SMBIOS y DXGI...".to_string() },
            }
        };
    };

    let specs_for_copy = specs.clone();
    let specs_for_export = specs.clone();

    let cores_label = match (specs.cpu.p_cores, specs.cpu.e_cores) {
        (Some(p), Some(e)) => format!(
            "{}C ({}P+{}E) / {}T",
            specs.cpu.physical_cores, p, e, specs.cpu.logical_threads
        ),
        _ => format!(
            "{}C / {}T",
            specs.cpu.physical_cores, specs.cpu.logical_threads
        ),
    };

    let ram_gb = specs.memory.total_mb as f64 / 1024.0;

    rsx! {
        div { class: "content-body",
            div {
                class: "card",
                style: "display: flex; justify-content: space-between; align-items: center; padding: var(--space-3) var(--space-4); flex-wrap: wrap; gap: var(--space-3);",
                div {
                    h3 { style: "margin: 0; font-size: var(--font-size-md); color: var(--text);",
                        if en { "Hardware Specifications" } else { "Especificaciones del Sistema" }
                    }
                    span { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                        "{specs.os.edition} {specs.os.display_version} • Build {specs.os.build}"
                    }
                }
                div { style: "display: flex; align-items: center; gap: var(--space-2);",
                    if let Some(msg) = status_msg() {
                        span { style: "font-size: var(--font-size-xs); color: var(--accent-text); margin-right: var(--space-2);", "{msg}" }
                    }
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| {
                            let summary = specs_for_copy.to_compact_summary();
                            let escaped = serde_json::to_string(&summary).unwrap_or_default();
                            let script = format!("navigator.clipboard.writeText({escaped});");
                            document::eval(&script);
                            status_msg.set(Some(if en { "Copied!".to_string() } else { "¡Copiado!".to_string() }));
                        },
                        if en { "Copy Summary" } else { "Copiar Resumen" }
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let res = crate::core::hardware::specs::export_to_desktop(&specs_for_export)
                                .map_err(|e| e.to_string());
                            match res {
                                Ok(path) => status_msg.set(Some(if en {
                                    format!("Saved: {}", path.display())
                                } else {
                                    format!("Guardado: {}", path.display())
                                })),
                                Err(e) => status_msg.set(Some(format!("Error: {e}"))),
                            }
                        },
                        if en { "Export JSON" } else { "Exportar JSON" }
                    }
                }
            }

            if !specs.warnings.is_empty() {
                StatusBanner {
                    text: format!(
                        "{}\n{}",
                        if en { "Some data could not be read:" } else { "Algunos datos no se pudieron leer:" },
                        specs.warnings.join("\n")
                    ),
                    tone: Tone::Warning,
                }
            }

            div { class: "cards-grid",
                div { class: "card", style: "padding: var(--space-4);",
                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-2);",
                        strong { style: "color: var(--accent-text);", "CPU" }
                        if let Some(temp) = props.cpu_temp_c {
                            span { style: "font-size: var(--font-size-xs); font-weight: 600;", "{temp:.0} °C" }
                        }
                    }
                    div { style: "font-size: var(--font-size-md); font-weight: 600; margin-bottom: var(--space-2);", "{specs.cpu.brand}" }
                    SpecRow {
                        label: if en { "Arch".to_string() } else { "Arq.".to_string() },
                        value: format!("{} • F:{:X}h M:{:X}h S:{}", specs.cpu.vendor, specs.cpu.family, specs.cpu.model, specs.cpu.stepping),
                    }
                    SpecRow {
                        label: if en { "Cores".to_string() } else { "Núcleos".to_string() },
                        value: cores_label,
                    }
                    SpecRow {
                        label: "L1 / L2 / L3".to_string(),
                        value: format!("{} KB / {} KB / {:.1} MB", specs.cpu.l1_cache_kb, specs.cpu.l2_cache_kb, specs.cpu.l3_cache_kb as f64 / 1024.0),
                    }
                    SpecRow {
                        label: "ISA".to_string(),
                        value: specs.cpu.instruction_sets.join(", "),
                    }
                }

                div { class: "card", style: "padding: var(--space-4);",
                    div { style: "margin-bottom: var(--space-2);",
                        strong { style: "color: var(--accent-text);", if en { "MOTHERBOARD & BIOS" } else { "PLACA BASE Y BIOS" } }
                    }
                    div { style: "font-size: var(--font-size-md); font-weight: 600; margin-bottom: var(--space-2);", "{specs.motherboard.manufacturer} {specs.motherboard.product}" }
                    SpecRow {
                        label: "BIOS".to_string(),
                        value: format!("{} ({})", specs.motherboard.bios_version, specs.motherboard.bios_date),
                    }
                    SpecRow {
                        label: if en { "Vendor".to_string() } else { "Fabricante".to_string() },
                        value: specs.motherboard.bios_vendor.clone(),
                    }
                    SpecRow {
                        label: "Rev.".to_string(),
                        value: if specs.motherboard.version.is_empty() { "N/A".to_string() } else { specs.motherboard.version.clone() },
                    }
                }

                div { class: "card", style: "padding: var(--space-4);",
                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-2);",
                        strong { style: "color: var(--accent-text);", "GPU" }
                        if let Some(edge) = props.gpu_edge_temp_c {
                            span { style: "font-size: var(--font-size-xs); font-weight: 600;", "{edge:.0} °C" }
                        }
                    }
                    // UI-14: dos GPU iguales tienen el mismo nombre; la clave es el índice.
                    for (gpu_idx, gpu) in specs.gpus.iter().enumerate() {
                        div { key: "{gpu_idx}", style: "margin-bottom: var(--space-2);",
                            div { style: "font-size: var(--font-size-md); font-weight: 600; margin-bottom: var(--space-1);", "{gpu.name}" }
                            SpecRow {
                                label: "ID".to_string(),
                                value: format!("{} ({:04X}:{:04X})", gpu.vendor_name, gpu.vendor_id, gpu.device_id),
                            }
                            SpecRow {
                                label: "VRAM".to_string(),
                                value: format!("{} MB ({:.1} GB)", gpu.dedicated_vram_mb, gpu.dedicated_vram_mb as f64 / 1024.0),
                            }
                        }
                    }
                }

                div { class: "card", style: "padding: var(--space-4);",
                    div { style: "margin-bottom: var(--space-2);",
                        strong { style: "color: var(--accent-text);", if en { "STORAGE" } else { "ALMACENAMIENTO" } }
                    }
                    for drive in &specs.storage {
                        div { key: "{drive.device_index}", style: "margin-bottom: var(--space-2);",
                            div { style: "font-size: var(--font-size-sm); font-weight: 600;", "[Disk {drive.device_index}] {drive.model}" }
                            SpecRow {
                                label: "Bus / FW".to_string(),
                                value: format!("{} • {}", drive.bus_type, drive.firmware),
                            }
                        }
                    }
                }
            }

            div { class: "card", style: "padding: var(--space-4);",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-2);",
                    strong { style: "color: var(--accent-text);", "RAM (SMBIOS)" }
                    span { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                        "{ram_gb:.1} GB {specs.memory.memory_type} @ {specs.memory.effective_speed_mts} MT/s ({specs.memory.populated_slots}/{specs.memory.total_slots})"
                    }
                }
                div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: var(--space-3);",
                    for module in &specs.memory.modules {
                        div {
                            key: "{module.slot}",
                            style: "border: 1px solid var(--border); border-radius: var(--radius-sm); padding: var(--space-3);",
                            div { style: "font-size: var(--font-size-xs); font-weight: 700; margin-bottom: var(--space-1);", "{module.slot}" }
                            if module.populated {
                                div { style: "font-size: var(--font-size-sm);", "{format_capacity(module.capacity_mb)} {module.memory_type} • {module.speed_mts} MT/s" }
                                div { style: "font-size: var(--font-size-xs); color: var(--text-muted);", "{module.manufacturer} ({module.part_number})" }
                            } else {
                                div { style: "font-size: var(--font-size-xs); color: var(--text-muted);", if en { "Empty slot" } else { "Ranura libre" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SpecRow(label: String, value: String) -> Element {
    rsx! {
        div { style: "display: flex; justify-content: space-between; gap: var(--space-3); font-size: var(--font-size-xs); padding: var(--space-1) 0; border-bottom: 1px solid var(--border);",
            span { style: "color: var(--text-muted);", "{label}" }
            span { style: "color: var(--text); text-align: right; font-weight: 500;", "{value}" }
        }
    }
}

/// ERR-12: un módulo de 512 MB no debe verse como «0 GB».
fn format_capacity(mb: u32) -> String {
    if mb >= 1024 && mb.is_multiple_of(1024) {
        format!("{} GB", mb / 1024)
    } else if mb >= 1024 {
        format!("{:.1} GB", mb as f64 / 1024.0)
    } else {
        format!("{} MB", mb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacidad_de_modulos_pequenos() {
        assert_eq!(format_capacity(512), "512 MB");
        assert_eq!(format_capacity(8192), "8 GB");
        assert_eq!(format_capacity(1536), "1.5 GB");
    }
}
