use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::hardware::sensors::{SensorMonitor, SensorReading, SystemSensorsSnapshot};
use crate::ui::components::{Card, EmptyState};
use crate::ui::i18n::core_text::tr;
use crate::ui::i18n::{Lang, strings};

#[component]
pub fn SensorsView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let txt = strings(en).sensors;

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

    let thermal_title = txt.thermal_monitor.to_string();
    let thermal_sub = Some(txt.live_gpu_nvme_acpi_readings.to_string());
    let reset_peaks_lbl = txt.reset_peaks;
    let pause_lbl = if *paused.read() {
        txt.resume
    } else {
        txt.pause
    };

    let empty_title = txt.waiting_for_sensors.to_string();
    let empty_desc = txt.checking_nvml_adl2_wmi_nvme.to_string();

    // UI-14: son zonas térmicas ACPI; no siempre miden la CPU.
    let cpu_section_lbl = txt.acpi_thermal_zones;
    let storage_section_lbl = txt.storage_nvme_ssd;
    let diagnostics_title = txt.diagnostics.to_string();

    rsx! {
        div { class: "content-body",
            Card {
                title: thermal_title,
                subtitle: thermal_sub,
                div { class: "row-end",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| peaks.write().clear(),
                        "{reset_peaks_lbl}"
                    }
                    button {
                        class: "btn btn-secondary",
                            "aria-pressed": if *paused.read() { "true" } else { "false" },
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
                    div { class: "stack-1 text-xs text-muted",
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
                        subtitle: Some(tr(&reading.sensor_type, en).into_owned()),
                        div { class: "stack-2",
                            h2 {
                                class: if reading.current_celsius >= 80.0 {
                                    "text-xl text-danger"
                                } else if reading.current_celsius >= 65.0 {
                                    "text-xl text-warning"
                                } else {
                                    "text-xl text-success"
                                },
                                "{reading.current_celsius:.1} °C"
                            }
                            div { class: "flex gap-3 text-xs text-muted",
                                if let Some(pk) = peaks.get(&reading.name) {
                                    span { "{(strings(en).sensors.peak_c)(&pk)}" }
                                }
                                if let Some(max_t) = reading.max_celsius {
                                    span { "{(strings(en).sensors.max_c)(&max_t)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
