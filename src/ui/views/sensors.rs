use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::hardware::sensors::{SensorMonitor, SensorReading, SystemSensorsSnapshot};
use crate::ui::components::{Card, EmptyState};
use crate::ui::i18n::Lang;

#[component]
pub fn SensorsView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let monitor = use_hook(|| Arc::new(SensorMonitor::new(SensorMonitor::DEFAULT_POLL_MS)));
    let mut snapshot = use_signal(SystemSensorsSnapshot::default);
    let mut peaks = use_signal(HashMap::<String, f32>::new);
    let mut paused = use_signal(|| false);

    use_effect(move || {
        let mon = Arc::clone(&monitor);
        spawn(async move {
            loop {
                if !*paused.read() {
                    let snap = mon.get_snapshot();
                    {
                        let mut p = peaks.write();
                        for s in snap
                            .cpu_temperatures
                            .iter()
                            .chain(snap.gpu_temperatures.iter())
                            .chain(snap.storage_temperatures.iter())
                        {
                            let entry = p.entry(s.name.clone()).or_insert(s.current_celsius);
                            if s.current_celsius > *entry {
                                *entry = s.current_celsius;
                            }
                        }
                    }
                    snapshot.set(snap);
                }
                // La vista refresca al doble de frecuencia que el hilo lee, para no mostrar datos viejos.
                tokio::time::sleep(Duration::from_millis(SensorMonitor::DEFAULT_POLL_MS / 2)).await;
            }
        });
    });

    let snap = snapshot.read();
    let peak_map = peaks.read();
    let has_any = !snap.cpu_temperatures.is_empty()
        || !snap.gpu_temperatures.is_empty()
        || !snap.storage_temperatures.is_empty();

    let thermal_title = if en {
        "Thermal Monitor"
    } else {
        "Monitor Térmico"
    }
    .to_string();
    let thermal_sub = Some(
        if en {
            "Live GPU, NVMe & ACPI readings"
        } else {
            "Lecturas en vivo de GPU, NVMe y ACPI"
        }
        .to_string(),
    );
    let reset_peaks_lbl = if en { "Reset Peaks" } else { "Reiniciar Picos" };
    let pause_lbl = if *paused.read() {
        if en {
            "Resume"
        } else {
            "Reanudar"
        }
    } else if en {
        "Pause"
    } else {
        "Pausar"
    };

    let empty_title = if en {
        "Waiting for sensors..."
    } else {
        "Esperando sensores..."
    }
    .to_string();
    let empty_desc = if en {
        "Checking NVML, ADL2, WMI & NVMe."
    } else {
        "Comprobando NVML, ADL2, WMI y NVMe."
    }
    .to_string();

    // UI-14: son zonas térmicas ACPI; no siempre miden la CPU.
    let cpu_section_lbl = if en {
        "ACPI thermal zones"
    } else {
        "Zonas térmicas ACPI"
    };
    let storage_section_lbl = if en {
        "Storage (NVMe / SSD)"
    } else {
        "Almacenamiento (NVMe / SSD)"
    };
    let diagnostics_title = if en { "Diagnostics" } else { "Diagnóstico" }.to_string();

    rsx! {
        div { class: "content-body",
            Card {
                title: thermal_title,
                subtitle: thermal_sub,
                div { style: "display: flex; justify-content: flex-end; gap: var(--space-2);",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| peaks.write().clear(),
                        "{reset_peaks_lbl}"
                    }
                    button {
                        class: if *paused.read() { "btn btn-primary" } else { "btn btn-secondary" },
                        onclick: move |_| {
                            let next = !*paused.read();
                            paused.set(next);
                        },
                        "{pause_lbl}"
                    }
                }
            }

            if !has_any {
                EmptyState {
                    title: empty_title,
                    description: empty_desc,
                }
            } else {
                {render_sensor_section(cpu_section_lbl, &snap.cpu_temperatures, &peak_map, en)}
                {render_sensor_section("GPU", &snap.gpu_temperatures, &peak_map, en)}
                {render_sensor_section(storage_section_lbl, &snap.storage_temperatures, &peak_map, en)}
            }

            if !snap.backend_notes.is_empty() {
                Card {
                    title: diagnostics_title,
                    subtitle: None,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--font-size-xs); color: var(--text-muted);",
                        for note in snap.backend_notes.iter() {
                            p { key: "{note}", "• {note}" }
                        }
                    }
                }
            }
        }
    }
}

fn render_sensor_section(
    section_title: &str,
    items: &[SensorReading],
    peaks: &HashMap<String, f32>,
    en: bool,
) -> Element {
    if items.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "stack-2",
            h3 { class: "text-md", "{section_title}" }
            div { class: "cards-grid",
                for reading in items.iter() {
                    Card {
                        key: "{reading.name}",
                        title: reading.name.clone(),
                        subtitle: Some(reading.sensor_type.clone()),
                        div { class: "stack-2",
                            h2 {
                                style: if reading.current_celsius >= 80.0 {
                                    "color: var(--danger-text); font-size: var(--font-size-xl);"
                                } else if reading.current_celsius >= 65.0 {
                                    "color: var(--warning-text); font-size: var(--font-size-xl);"
                                } else {
                                    "color: var(--success-text); font-size: var(--font-size-xl);"
                                },
                                "{reading.current_celsius:.1} °C"
                            }
                            div { style: "display: flex; gap: var(--space-3); font-size: var(--font-size-xs); color: var(--text-muted);",
                                if let Some(pk) = peaks.get(&reading.name) {
                                    span { if en { "Peak: {pk:.1} °C" } else { "Pico: {pk:.1} °C" } }
                                }
                                if let Some(max_t) = reading.max_celsius {
                                    span { if en { "Max: {max_t:.0} °C" } else { "Máx: {max_t:.0} °C" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
