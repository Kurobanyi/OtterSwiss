use crate::core::hardware::specs::SystemSpecs;
use crate::ui::components::{ErrorState, InfoBar, LoadingState, StatusBanner, Tone};
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
    // Tono del aviso: verde si copió o guardó, rojo si la exportación falló.
    let mut status_ok = use_signal(|| true);

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
    let status_tone = if status_ok() {
        Tone::Success
    } else {
        Tone::Danger
    };

    rsx! {
        div { class: "content-body",
            div { class: "row wrap",
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        match crate::core::hardware::specs::export_to_desktop(&specs_for_export) {
                            Ok(path) => {
                                let path = path.display().to_string();
                                status_ok.set(true);
                                status_msg.set(Some(Msg::new(move |en| (strings(en).hardware.saved)(&path))));
                            }
                            Err(e) => {
                                status_ok.set(false);
                                status_msg.set(Some(Msg::from(e)));
                            }
                        }
                    },
                    span { class: "fluent-icon", "aria-hidden": "true", "\u{E896}" }
                    "{txt.export_json}"
                }
                button {
                    class: "btn",
                    onclick: move |_| {
                        let summary = compact_summary(&specs_for_copy, en);
                        let escaped = serde_json::to_string(&summary).unwrap_or_default();
                        let script = format!("navigator.clipboard.writeText({escaped});");
                        document::eval(&script);
                        status_ok.set(true);
                        status_msg.set(Some(Msg::new(|en| strings(en).hardware.copied.to_string())));
                    },
                    span { class: "fluent-icon", "aria-hidden": "true", "\u{E8C8}" }
                    "{txt.copy_summary}"
                }
            }

            if let Some(msg) = status_msg().map(|m| m.text(en)) {
                StatusBanner { text: msg, tone: status_tone }
            }

            if !specs.warnings.is_empty() {
                InfoBar {
                    tone: Tone::Warning,
                    title: txt.some_data_could_not_be.to_string(),
                    message: specs.warnings.join("; "),
                }
            }

            div { class: "section-title", "{txt.system}" }
            div { class: "stack-1",
                SpecRow {
                    label: txt.windows.to_string(),
                    value: format!("{} {}", specs.os.edition, tr(&specs.os.display_version, en)),
                    strong: true,
                }
                SpecRow {
                    label: txt.build.to_string(),
                    value: tr(&specs.os.build, en).to_string(),
                    mono: true,
                }
            }

            div { class: "section-title", "{txt.processor}" }
            div { class: "stack-1",
                SpecRow {
                    label: txt.model.to_string(),
                    value: specs.cpu.brand.clone(),
                    strong: true,
                }
                if let Some(temp) = props.cpu_temp_c {
                    SpecRow {
                        label: txt.temperature.to_string(),
                        value: format!("{temp:.0} °C"),
                    }
                }
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
                    label: txt.instruction_sets.to_string(),
                    value: specs.cpu.instruction_sets.join(", "),
                    long: true,
                }
            }

            div { class: "section-title", "{txt.motherboard_bios}" }
            div { class: "stack-1",
                SpecRow {
                    label: txt.board.to_string(),
                    value: format!("{} {}", specs.motherboard.manufacturer, specs.motherboard.product),
                    strong: true,
                }
                SpecRow {
                    label: "BIOS".to_string(),
                    value: format!("{} ({})", specs.motherboard.bios_version, specs.motherboard.bios_date),
                }
                SpecRow {
                    label: txt.vendor.to_string(),
                    value: specs.motherboard.bios_vendor.clone(),
                }
                SpecRow {
                    label: txt.revision.to_string(),
                    value: if specs.motherboard.version.is_empty() { "N/A".to_string() } else { specs.motherboard.version.clone() },
                }
            }

            div { class: "section-title", "{txt.graphics}" }
            div { class: "stack-1",
                // UI-14: dos GPU iguales tienen el mismo nombre; la clave es el índice.
                for (gpu_idx, gpu) in specs.gpus.iter().enumerate() {
                    Fragment { key: "{gpu_idx}",
                    SpecRow {
                        label: "GPU".to_string(),
                        value: gpu.name.clone(),
                        detail: format!("{} MB VRAM ({:.1} GB)", gpu.dedicated_vram_mb, gpu.dedicated_vram_mb as f64 / 1024.0),
                        strong: true,
                    }
                    SpecRow {
                        label: "ID".to_string(),
                        value: format!("{} ({:04X}:{:04X})", gpu.vendor_name, gpu.vendor_id, gpu.device_id),
                        mono: true,
                    }
                    }
                }
                if let Some(edge) = props.gpu_edge_temp_c {
                    SpecRow {
                        label: txt.temperature.to_string(),
                        value: format!("{edge:.0} °C"),
                    }
                }
            }

            div { class: "section-title", "{txt.storage}" }
            div { class: "stack-1",
                for drive in &specs.storage {
                    SpecRow {
                        key: "{drive.device_index}",
                        label: (txt.disk)(&drive.device_index),
                        value: drive.model.clone(),
                        detail: format!("{} • {}", drive.bus_type, drive.firmware),
                        strong: true,
                    }
                }
            }

            div { class: "section-title", "RAM (SMBIOS)" }
            div { class: "stack-1",
                SpecRow {
                    label: txt.total.to_string(),
                    value: format!("{ram_gb:.1} GB {} @ {} MT/s", specs.memory.memory_type, specs.memory.effective_speed_mts),
                    strong: true,
                }
                SpecRow {
                    label: txt.slots_in_use.to_string(),
                    value: format!("{}/{}", specs.memory.populated_slots, specs.memory.total_slots),
                }
                for module in &specs.memory.modules {
                    if module.populated {
                        SpecRow {
                            key: "{module.slot}",
                            label: module.slot.clone(),
                            value: format!("{} {} • {} MT/s", capacity_mb(module.capacity_mb), module.memory_type, module.speed_mts),
                            detail: format!("{} ({})", module.manufacturer, module.part_number),
                        }
                    } else {
                        SpecRow {
                            key: "{module.slot}",
                            label: module.slot.clone(),
                            value: txt.empty_slot.to_string(),
                        }
                    }
                }
            }
        }
    }
}

/// Fila etiqueta/valor al estilo de Inicio. `strong` destaca el dato principal del grupo, `mono` los identificadores,
/// `long` deja que una lista larga se lea alineada a la izquierda y `detail` añade una línea secundaria.
#[component]
fn SpecRow(
    label: String,
    value: String,
    #[props(default = String::new())] detail: String,
    #[props(default = false)] strong: bool,
    #[props(default = false)] mono: bool,
    #[props(default = false)] long: bool,
) -> Element {
    let value_class = format!(
        "dash-value hw-value{}{}{}",
        if strong { " hw-strong" } else { "" },
        if mono { " mono" } else { "" },
        if long { " hw-long" } else { "" },
    );
    rsx! {
        div { class: if long { "item-row dash-row hw-row-top" } else { "item-row dash-row" },
            span { class: "dash-label", "{label}" }
            div { class: "{value_class}",
                span { "{value}" }
                if !detail.is_empty() {
                    span { class: "text-xs text-muted hw-detail", "{detail}" }
                }
            }
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
