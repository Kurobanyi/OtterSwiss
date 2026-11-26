use crate::core::hardware::specs::SystemSpecs;
use crate::ui::components::{ErrorState, LoadingState, StatusBanner, Tone};
use crate::ui::format::capacity_mb;
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, Msg, strings};
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
    let txt = strings(en).hardware;
    let mut status_msg = use_signal(|| None::<Msg>);

    let Some(specs) = props.specs.clone() else {
        if let Some(err) = props.error {
            return rsx! {
                ErrorState {
                    title: txt.could_not_read_the_hardware.to_string(),
                    error_msg: err,
                }
            };
        }
        return rsx! {
            LoadingState {
                message: txt.scanning_cpu_smbios_dxgi.to_string(),
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
                class: "card card-bar",
                div {
                    h3 { class: "heading-flat",
                        "{txt.hardware_specifications}"
                    }
                    span { class: "text-xs text-muted",
                        "{specs.os.edition} {tr(&specs.os.display_version, en)} • Build {tr(&specs.os.build, en)}"
                    }
                }
                div { class: "row",
                    if let Some(msg) = status_msg().map(|m| m.text(en)) {
                        span { class: "text-xs text-accent mr-2", "{msg}" }
                    }
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| {
                            let summary = compact_summary(&specs_for_copy, en);
                            let escaped = serde_json::to_string(&summary).unwrap_or_default();
                            let script = format!("navigator.clipboard.writeText({escaped});");
                            document::eval(&script);
                            status_msg.set(Some(Msg::new(|en| strings(en).hardware.copied.to_string())));
                        },
                        "{txt.copy_summary}"
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            match crate::core::hardware::specs::export_to_desktop(&specs_for_export) {
                                Ok(path) => {
                                    let path = path.display().to_string();
                                    status_msg.set(Some(Msg::new(move |en| (strings(en).hardware.saved)(&path))));
                                }
                                Err(e) => status_msg.set(Some(Msg::from(e))),
                            }
                        },
                        "{txt.export_json}"
                    }
                }
            }

            if !specs.warnings.is_empty() {
                StatusBanner {
                    text: format!(
                        "{}\n{}",
                        txt.some_data_could_not_be,
                        specs.warnings.join("\n")
                    ),
                    tone: Tone::Warning,
                }
            }

            div { class: "cards-grid",
                div { class: "card",
                    div { class: "row-between mb-2",
                        strong { class: "text-accent", "CPU" }
                        if let Some(temp) = props.cpu_temp_c {
                            span { class: "text-xs fw-semibold", "{temp:.0} °C" }
                        }
                    }
                    div { class: "text-md fw-semibold mb-2", "{specs.cpu.brand}" }
                    SpecRow {
                        label: txt.arch.to_string(),
                        value: format!("{} • F:{:X}h M:{:X}h S:{}", specs.cpu.vendor, specs.cpu.family, specs.cpu.model, specs.cpu.stepping),
                    }
                    SpecRow {
                        label: txt.cores.to_string(),
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

                div { class: "card",
                    div { class: "mb-2",
                        strong { class: "text-accent", "{txt.motherboard_bios}" }
                    }
                    div { class: "text-md fw-semibold mb-2", "{specs.motherboard.manufacturer} {specs.motherboard.product}" }
                    SpecRow {
                        label: "BIOS".to_string(),
                        value: format!("{} ({})", specs.motherboard.bios_version, specs.motherboard.bios_date),
                    }
                    SpecRow {
                        label: txt.vendor.to_string(),
                        value: specs.motherboard.bios_vendor.clone(),
                    }
                    SpecRow {
                        label: "Rev.".to_string(),
                        value: if specs.motherboard.version.is_empty() { "N/A".to_string() } else { specs.motherboard.version.clone() },
                    }
                }

                div { class: "card",
                    div { class: "row-between mb-2",
                        strong { class: "text-accent", "GPU" }
                        if let Some(edge) = props.gpu_edge_temp_c {
                            span { class: "text-xs fw-semibold", "{edge:.0} °C" }
                        }
                    }
                    // UI-14: dos GPU iguales tienen el mismo nombre; la clave es el índice.
                    for (gpu_idx, gpu) in specs.gpus.iter().enumerate() {
                        div { key: "{gpu_idx}", class: "mb-2",
                            div { class: "text-md fw-semibold mb-1", "{gpu.name}" }
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

                div { class: "card",
                    div { class: "mb-2",
                        strong { class: "text-accent", "{txt.storage}" }
                    }
                    for drive in &specs.storage {
                        div { key: "{drive.device_index}", class: "mb-2",
                            div { class: "text-sm fw-semibold", "[Disk {drive.device_index}] {drive.model}" }
                            SpecRow {
                                label: "Bus / FW".to_string(),
                                value: format!("{} • {}", drive.bus_type, drive.firmware),
                            }
                        }
                    }
                }
            }

            div { class: "card",
                div { class: "row-between mb-2",
                    strong { class: "text-accent", "RAM (SMBIOS)" }
                    span { class: "text-xs text-muted",
                        "{ram_gb:.1} GB {specs.memory.memory_type} @ {specs.memory.effective_speed_mts} MT/s ({specs.memory.populated_slots}/{specs.memory.total_slots})"
                    }
                }
                div { class: "card-grid",
                    for module in &specs.memory.modules {
                        div {
                            key: "{module.slot}",
                            class: "tile",
                            div { class: "text-xs fw-bold mb-1", "{module.slot}" }
                            if module.populated {
                                div { class: "text-sm", "{capacity_mb(module.capacity_mb)} {module.memory_type} • {module.speed_mts} MT/s" }
                                div { class: "text-xs text-muted", "{module.manufacturer} ({module.part_number})" }
                            } else {
                                div { class: "text-xs text-muted", "{txt.empty_slot}" }
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
        div { class: "kv-row",
            span { class: "text-muted", "{label}" }
            span { class: "kv-value", "{value}" }
        }
    }
}

/// Resumen en texto plano para copiar al portapapeles, con los rótulos en el idioma de la interfaz.
fn compact_summary(specs: &SystemSpecs, en: bool) -> String {
    let txt = strings(en).hardware;
    let gpu_line = specs
        .gpus
        .first()
        .map(|g| format!("{} ({} MB VRAM)", g.name, g.dedicated_vram_mb))
        .unwrap_or_else(|| txt.summary_gpu_none.to_string());
    let ram_gb = specs.memory.total_mb as f64 / 1024.0;
    format!(
        "OS: {} {} (Build {})\nCPU: {} ({}C/{}T)\n{}: {} {} (BIOS {})\nRAM: {:.0} GB {} @ {} MT/s ({}/{} slots)\nGPU: {}",
        specs.os.edition,
        tr(&specs.os.display_version, en),
        specs.os.build,
        specs.cpu.brand,
        specs.cpu.physical_cores,
        specs.cpu.logical_threads,
        txt.summary_board,
        specs.motherboard.manufacturer,
        specs.motherboard.product,
        specs.motherboard.bios_version,
        ram_gb,
        tr(&specs.memory.memory_type, en),
        specs.memory.effective_speed_mts,
        specs.memory.populated_slots,
        specs.memory.total_slots,
        gpu_line,
    )
}
