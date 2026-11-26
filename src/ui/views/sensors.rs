use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;

use crate::core::hardware::sensors::{SensorMonitor, SensorReading, SystemSensorsSnapshot};
use crate::ui::components::{EmptyState, InfoBar, Tone};
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

    let reset_peaks_lbl = txt.reset_peaks;
    let is_paused = *paused.read();
    let pause_lbl = if is_paused { txt.resume } else { txt.pause };
    let pause_glyph = if is_paused { "\u{E768}" } else { "\u{E769}" };

    let empty_title = txt.waiting_for_sensors.to_string();
    let empty_desc = txt.checking_nvml_adl2_wmi_nvme.to_string();

    // UI-14: son zonas térmicas ACPI; no siempre miden la CPU.
    let cpu_section_lbl = txt.acpi_thermal_zones;
    let storage_section_lbl = txt.storage_nvme_ssd;
    let diagnostics_title = txt.diagnostics.to_string();

    rsx! {
        div { class: "content-body",
            div { class: "row wrap",
                button {
                    class: "btn",
                    "aria-pressed": if is_paused { "true" } else { "false" },
                    onclick: move |_| {
                        let next = !*paused.read();
                        paused.set(next);
                    },
                    span { class: "fluent-icon", "aria-hidden": "true", "{pause_glyph}" }
                    "{pause_lbl}"
                }
                button {
                    class: "btn",
                    onclick: move |_| peaks.write().clear(),
                    span { class: "fluent-icon", "aria-hidden": "true", "\u{E72C}" }
                    "{reset_peaks_lbl}"
                }
                if is_paused {
                    span { class: "badge badge-info", "{txt.paused_note}" }
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
                InfoBar {
                    tone: Tone::Info,
                    title: diagnostics_title,
                    message: snap
                        .backend_notes
                        .iter()
                        .map(|n| tr(n, en).into_owned())
                        .collect::<Vec<_>>()
                        .join(" · "),
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
    let txt = strings(en).sensors;

    rsx! {
        div { class: "section-title", "{section_title}" }
        div { class: "snr-grid",
            for reading in items.iter() {
                {
                    // Umbrales de siempre; el estado va también en texto, no solo en color.
                    let (tone_class, badge_class, level) = if reading.current_celsius >= 80.0 {
                        ("text-danger", "badge badge-danger", txt.level_critical)
                    } else if reading.current_celsius >= 65.0 {
                        ("text-warning", "badge badge-warning", txt.level_high)
                    } else {
                        ("text-success", "badge badge-success", txt.level_normal)
                    };
                    rsx! {
                        div { key: "{reading.name}", class: "card snr-card",
                            div { class: "row-between",
                                div { class: "grow stack-1",
                                    span { class: "fw-semibold", "{reading.name}" }
                                    span { class: "text-xs text-muted", "{tr(&reading.sensor_type, en)}" }
                                }
                                span { class: "{badge_class}", "{level}" }
                            }
                            div { class: "snr-temp {tone_class}", "{reading.current_celsius:.1} °C" }
                            div { class: "snr-meta text-xs text-muted",
                                if let Some(pk) = peaks.get(&reading.name) {
                                    span { "{(txt.peak_c)(&pk)}" }
                                }
                                if let Some(max_t) = reading.max_celsius {
                                    span { "{(txt.max_c)(&max_t)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
