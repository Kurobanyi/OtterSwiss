// src/ui/views/dns.rs

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::core::dns::{
    benchmark_dns_presets, list_active_adapters, DnsBenchmarkResult, DnsManager, DNS_PRESETS,
};
use crate::core::trust::get_program_data_dir;
use crate::ui::components::Card;
use crate::ui::i18n::Lang;

#[component]
pub fn DnsView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

    let mut adapters = use_signal(|| list_active_adapters().unwrap_or_default());
    let mut selected_preset = use_signal(|| "cloudflare");
    let mut status_msg = use_signal(|| Option::<String>::None);
    let mut is_error = use_signal(|| false);
    let mut is_working = use_signal(|| false);
    let mut is_benchmarking = use_signal(|| false);
    let mut benchmarks = use_signal(HashMap::<&'static str, DnsBenchmarkResult>::new);
    let mut best_preset_id = use_signal(|| Option::<&'static str>::None);

    let on_benchmark = move |_| {
        is_benchmarking.set(true);
        status_msg.set(Some(if en { "Testing DNS latency..." } else { "Midiendo latencia..." }.to_string()));
        is_error.set(false);

        spawn(async move {
            let results = tokio::task::spawn_blocking(benchmark_dns_presets)
                .await
                .unwrap_or_default();

            let mut best: Option<(&'static str, u32)> = None;
            let mut map = HashMap::new();

            for r in results {
                if let Some(ms) = r.avg_latency_ms {
                    match best {
                        Some((_, cur_ms)) if ms >= cur_ms => {}
                        _ => best = Some((r.preset_id, ms)),
                    }
                }
                map.insert(r.preset_id, r);
            }

            benchmarks.set(map);
            if let Some((best_id, best_ms)) = best {
                best_preset_id.set(Some(best_id));
                selected_preset.set(best_id);
                if let Some(p) = DNS_PRESETS.iter().find(|x| x.id == best_id) {
                    status_msg.set(Some(if en {
                        format!("Fastest: {} ({} ms)", p.name, best_ms)
                    } else {
                        format!("Mejor: {} ({} ms)", p.name, best_ms)
                    }));
                }
            } else {
                best_preset_id.set(None);
                status_msg.set(Some(if en { "No ping reply." } else { "Sin respuesta." }.to_string()));
                is_error.set(true);
            }
            is_benchmarking.set(false);
        });
    };

    let on_apply_dns = move |_| {
        let preset_id = *selected_preset.read();
        let Some(p) = DNS_PRESETS.iter().find(|p| p.id == preset_id) else {
            return;
        };

        let v4_csv = format!("{},{}", p.ipv4_primary, p.ipv4_secondary);
        let v6_csv = format!("{},{}", p.ipv6_primary, p.ipv6_secondary);
        let p_name = p.name;

        is_working.set(true);
        spawn(async move {
            let res = tokio::task::spawn_blocking(move || -> Result<String, String> {
                let data_dir = get_program_data_dir()
                    .map_err(|e| e.to_string())?
                    .join("OtterSwiss");
                std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

                let dns_mgr = DnsManager::new(&data_dir);
                let active = list_active_adapters().map_err(|e| e.to_string())?;

                let guids: Vec<String> = active
                    .iter()
                    .filter(|a| !a.is_virtual_or_vpn)
                    .map(|a| a.interface_guid.clone())
                    .collect();

                if guids.is_empty() {
                    return Err("No active physical adapters".to_string());
                }

                let count = dns_mgr
                    .apply_dns_to_selected(&guids, &v4_csv, &v6_csv)
                    .map_err(|e| e.to_string())?;

                Ok(format!("{} OK ({} NICs)", p_name, count))
            })
            .await
            .unwrap_or_else(|e| Err(format!("Thread error: {}", e)));

            match res {
                Ok(msg) => {
                    adapters.set(list_active_adapters().unwrap_or_default());
                    status_msg.set(Some(msg));
                    is_error.set(false);
                }
                Err(e) => {
                    status_msg.set(Some(format!("Error: {}", e)));
                    is_error.set(true);
                }
            }
            is_working.set(false);
        });
    };

    let on_restore_dns = move |_| {
        is_working.set(true);
        spawn(async move {
            let res = tokio::task::spawn_blocking(move || -> Result<String, String> {
                let data_dir = get_program_data_dir()
                    .map_err(|e| e.to_string())?
                    .join("OtterSwiss");

                let dns_mgr = DnsManager::new(&data_dir);
                let count = dns_mgr.restore_dns().map_err(|e| e.to_string())?;
                Ok(format!("Restored {} NIC(s)", count))
            })
            .await
            .unwrap_or_else(|e| Err(format!("Thread error: {}", e)));

            match res {
                Ok(msg) => {
                    adapters.set(list_active_adapters().unwrap_or_default());
                    status_msg.set(Some(msg));
                    is_error.set(false);
                }
                Err(e) => {
                    status_msg.set(Some(format!("Error: {}", e)));
                    is_error.set(true);
                }
            }
            is_working.set(false);
        });
    };

    let bench_map = benchmarks.read();
    let best_id = *best_preset_id.read();

    let optimizer_card_title = if en { "DNS Optimizer" } else { "Optimizador DNS" }.to_string();
    let optimizer_card_sub = Some(if en { "Fast resolution & latency benchmark" } else { "Resolución rápida y test de latencia" }.to_string());
    let adapters_card_title = if en { "Network Adapters" } else { "Adaptadores de Red" }.to_string();

    let ping_btn_text = if *is_benchmarking.read() {
        if en { "Pinging..." } else { "Midiendo..." }
    } else if en {
        "Test Ping"
    } else {
        "Probar Ping"
    };

    let apply_dns_lbl = if en { "Apply DNS" } else { "Aplicar DNS" };
    let restore_dns_lbl = if en { "Restore Previous" } else { "Restaurar DNS" };
    let physical_lbl = if en { "Physical" } else { "Físico" };

    rsx! {
        div { class: "content-body",
            Card {
                title: optimizer_card_title,
                subtitle: optimizer_card_sub,
                div { style: "display: flex; flex-direction: column; gap: var(--space-3);",
                    div { style: "display: flex; gap: var(--space-2); align-items: center; flex-wrap: wrap;",
                        for p in DNS_PRESETS {
                            {
                                let b = bench_map.get(p.id);
                                let is_best = best_id == Some(p.id);
                                rsx! {
                                    button {
                                        key: "{p.id}",
                                        class: if *selected_preset.read() == p.id { "btn btn-primary" } else { "btn btn-secondary" },
                                        onclick: move |_| selected_preset.set(p.id),
                                        "{p.name}"
                                        if let Some(res) = b {
                                            if let Some(ms) = res.avg_latency_ms {
                                                " • {ms} ms"
                                            } else {
                                                " • N/A"
                                            }
                                        }
                                        if is_best { " ★" }
                                    }
                                }
                            }
                        }

                        button {
                            class: "btn btn-secondary",
                            disabled: *is_benchmarking.read() || *is_working.read(),
                            onclick: on_benchmark,
                            "{ping_btn_text}"
                        }
                    }

                    if let Some(active_p) = DNS_PRESETS.iter().find(|p| p.id == *selected_preset.read()) {
                        div { style: "background: var(--bg); padding: var(--space-3); border-radius: var(--radius); border: 1px solid var(--border); font-size: var(--font-size-xs);",
                            p { strong { "IPv4: " } "{active_p.ipv4_primary}, {active_p.ipv4_secondary}   |   " strong { "IPv6: " } "{active_p.ipv6_primary}, {active_p.ipv6_secondary}" }
                        }
                    }

                    div { style: "display: flex; gap: var(--space-3);",
                        button {
                            class: "btn btn-primary",
                            disabled: *is_working.read(),
                            onclick: on_apply_dns,
                            "{apply_dns_lbl}"
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_working.read(),
                            onclick: on_restore_dns,
                            "{restore_dns_lbl}"
                        }
                    }

                    if let Some(msg) = status_msg.read().as_ref() {
                        div {
                            class: if *is_error.read() { "badge badge-danger" } else { "badge badge-success" },
                            style: "padding: var(--space-2) var(--space-3);",
                            "{msg}"
                        }
                    }
                }
            }

            Card {
                title: adapters_card_title,
                subtitle: None,
                div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                    for adapter in adapters.read().iter() {
                        div {
                            key: "{adapter.interface_guid}",
                            style: "padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg); display: flex; justify-content: space-between; align-items: center;",
                            div {
                                div { style: "font-weight: 600; font-size: var(--font-size-sm);", "{adapter.friendly_name}" }
                                div { style: "font-size: var(--font-size-xs); color: var(--text-muted);",
                                    "IPv4: "
                                    {if adapter.current_ipv4_dns.is_empty() { "DHCP" } else { adapter.current_ipv4_dns.as_str() }}
                                    if !adapter.current_ipv6_dns.is_empty() {
                                        " | IPv6: {adapter.current_ipv6_dns}"
                                    }
                                }
                            }
                            if adapter.is_virtual_or_vpn {
                                span { class: "badge badge-warning", "Virtual / VPN" }
                            } else {
                                span { class: "badge badge-success", "{physical_lbl}" }
                            }
                        }
                    }
                }
            }
        }
    }
}