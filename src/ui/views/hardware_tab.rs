use std::path::PathBuf;
use dioxus::prelude::*;
use crate::core::specs::{export_specs_json, SystemSpecs};
use crate::ui::i18n::Lang;

#[derive(Props, Clone, PartialEq)]
pub struct HardwareTabProps {
    pub specs: Option<SystemSpecs>,
    #[props(default = None)]
    pub cpu_temp_c: Option<f32>,
    #[props(default = None)]
    pub gpu_edge_temp_c: Option<f32>,
    #[props(default = None)]
    pub gpu_hotspot_temp_c: Option<f32>,
}

#[component]
pub fn HardwareTab(props: HardwareTabProps) -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let mut status_msg = use_signal(|| None::<String>);

    let Some(specs) = props.specs.clone() else {
        return rsx! {
            div { class: "card", style: "padding: 24px; text-align: center;",
                p { style: "color: var(--text-muted);",
                    if en { "Scanning CPU, SMBIOS & DXGI..." } else { "Analizando CPU, SMBIOS y DXGI..." }
                }
            }
        };
    };

    let specs_for_copy = specs.clone();
    let specs_for_export = specs.clone();

    let cores_label = match (specs.cpu.p_cores, specs.cpu.e_cores) {
        (Some(p), Some(e)) => format!("{}C ({}P+{}E) / {}T", specs.cpu.physical_cores, p, e, specs.cpu.logical_threads),
        _ => format!("{}C / {}T", specs.cpu.physical_cores, specs.cpu.logical_threads),
    };

    let ram_gb = specs.memory.total_mb as f64 / 1024.0;

    rsx! {
        div { class: "content-body",
            div {
                class: "card",
                style: "display: flex; justify-content: space-between; align-items: center; padding: 12px 16px; flex-wrap: wrap; gap: 10px;",
                div {
                    h3 { style: "margin: 0; font-size: 15px; color: var(--text);",
                        if en { "Hardware Specifications" } else { "Especificaciones del Sistema" }
                    }
                    span { style: "font-size: 12px; color: var(--text-muted);",
                        "{specs.os.edition} {specs.os.display_version} • Build {specs.os.build}"
                    }
                }
                div { style: "display: flex; align-items: center; gap: 8px;",
                    if let Some(msg) = status_msg() {
                        span { style: "font-size: 12px; color: var(--accent); margin-right: 6px;", "{msg}" }
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
                            let base_dir = dirs::desktop_dir().unwrap_or_else(|| PathBuf::from("."));
                            let out_path = base_dir.join("OtterSwiss-Hardware-Specs.json");
                            match export_specs_json(&specs_for_export, &out_path) {
                                Ok(()) => status_msg.set(Some(format!("OK: {}", out_path.display()))),
                                Err(e) => status_msg.set(Some(format!("Error: {e}"))),
                            }
                        },
                        if en { "Export JSON" } else { "Exportar JSON" }
                    }
                }
            }

            div { class: "cards-grid",
                div { class: "card", style: "padding: 16px;",
                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;",
                        strong { style: "color: var(--accent);", "CPU" }
                        if let Some(temp) = props.cpu_temp_c {
                            span { style: "font-size: 12px; font-weight: 600;", "{temp:.0} °C" }
                        }
                    }
                    div { style: "font-size: 14px; font-weight: 600; margin-bottom: 8px;", "{specs.cpu.brand}" }
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

                div { class: "card", style: "padding: 16px;",
                    div { style: "margin-bottom: 8px;",
                        strong { style: "color: var(--accent);", if en { "MOTHERBOARD & BIOS" } else { "PLACA BASE Y BIOS" } }
                    }
                    div { style: "font-size: 14px; font-weight: 600; margin-bottom: 8px;", "{specs.motherboard.manufacturer} {specs.motherboard.product}" }
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

                div { class: "card", style: "padding: 16px;",
                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;",
                        strong { style: "color: var(--accent);", "GPU" }
                        if let Some(edge) = props.gpu_edge_temp_c {
                            span { style: "font-size: 12px; font-weight: 600;", "{edge:.0} °C" }
                        }
                    }
                    for gpu in &specs.gpus {
                        div { key: "{gpu.name}", style: "margin-bottom: 8px;",
                            div { style: "font-size: 14px; font-weight: 600; margin-bottom: 4px;", "{gpu.name}" }
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

                div { class: "card", style: "padding: 16px;",
                    div { style: "margin-bottom: 8px;",
                        strong { style: "color: var(--accent);", if en { "STORAGE" } else { "ALMACENAMIENTO" } }
                    }
                    for drive in &specs.storage {
                        div { key: "{drive.device_index}", style: "margin-bottom: 8px;",
                            div { style: "font-size: 13px; font-weight: 600;", "[Disk {drive.device_index}] {drive.model}" }
                            SpecRow {
                                label: "Bus / FW".to_string(),
                                value: format!("{} • {}", drive.bus_type, drive.firmware),
                            }
                        }
                    }
                }
            }

            div { class: "card", style: "padding: 16px;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;",
                    strong { style: "color: var(--accent);", "RAM (SMBIOS)" }
                    span { style: "font-size: 12px; color: var(--text-muted);",
                        "{ram_gb:.1} GB {specs.memory.memory_type} @ {specs.memory.effective_speed_mts} MT/s ({specs.memory.populated_slots}/{specs.memory.total_slots})"
                    }
                }
                div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 10px;",
                    for module in &specs.memory.modules {
                        div {
                            key: "{module.slot}",
                            style: "border: 1px solid var(--border); border-radius: 6px; padding: 10px;",
                            div { style: "font-size: 12px; font-weight: 700; margin-bottom: 4px;", "{module.slot}" }
                            if module.populated {
                                div { style: "font-size: 13px;", "{module.capacity_mb / 1024} GB {module.memory_type} • {module.speed_mts} MT/s" }
                                div { style: "font-size: 11px; color: var(--text-muted);", "{module.manufacturer} ({module.part_number})" }
                            } else {
                                div { style: "font-size: 12px; color: var(--text-muted);", if en { "Empty slot" } else { "Ranura libre" } }
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
        div { style: "display: flex; justify-content: space-between; gap: 12px; font-size: 12px; padding: 3px 0; border-bottom: 1px solid rgba(255,255,255,0.04);",
            span { style: "color: var(--text-muted);", "{label}" }
            span { style: "color: var(--text); text-align: right; font-weight: 500;", "{value}" }
        }
    }
}