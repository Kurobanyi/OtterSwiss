use std::collections::HashMap;

use dioxus::prelude::*;

use crate::core::dns::{
    DNS_PRESETS, DnsBenchmarkResult, DnsManager, NetworkAdapterInfo, benchmark_dns_presets,
    list_active_adapters,
};
use crate::core::error::CoreError;
use crate::ui::components::{
    Card, ConfirmDialog, EmptyState, ErrorState, LoadingState, StatusBanner, Tone,
};
use crate::ui::i18n::{Lang, Msg, plural, strings};

/// Estado de la lista de adaptadores: `None` mientras se lee (UI-07).
type AdaptersState = Option<Result<Vec<NetworkAdapterInfo>, Msg>>;

/// Relee adaptadores y si hay respaldo de DNS, fuera del hilo de UI (UI-05).
fn spawn_refresh(mut adapters: Signal<AdaptersState>, mut has_backup: Signal<bool>) {
    spawn(async move {
        let res = tokio::task::spawn_blocking(|| {
            let list = list_active_adapters().map_err(Msg::from);
            let backup = DnsManager::open_default()
                .map(|mgr| mgr.has_backup())
                .unwrap_or(false);
            (list, backup)
        })
        .await;
        match res {
            Ok((list, backup)) => {
                adapters.set(Some(list));
                has_backup.set(backup);
            }
            Err(e) => adapters.set(Some(Err(Msg::plain(e.to_string())))),
        }
    });
}

#[component]
pub fn DnsView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let txt = strings(en).dns;

    let adapters = use_signal(|| AdaptersState::None);
    let has_backup = use_signal(|| false);
    let mut selected_preset = use_signal(|| "cloudflare");
    let mut status_msg = use_signal(|| Option::<(Msg, Tone)>::None);
    let mut is_working = use_signal(|| false);
    let mut is_benchmarking = use_signal(|| false);
    let mut confirm_apply = use_signal(|| false);
    let mut benchmarks = use_signal(HashMap::<&'static str, DnsBenchmarkResult>::new);
    let mut best_preset_id = use_signal(|| Option::<&'static str>::None);

    use_hook(move || spawn_refresh(adapters, has_backup));

    let on_benchmark = move |_| {
        is_benchmarking.set(true);
        status_msg.set(Some((
            Msg::new(move |en| strings(en).dns.measuring_ping.to_string()),
            Tone::Info,
        )));

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
                    // UI-15: es ping ICMP al servidor, no tiempo de resolución DNS.
                    status_msg.set(Some((
                        Msg::new(move |en| (strings(en).dns.lowest_ping_ms)(&p.name, &best_ms)),
                        Tone::Success,
                    )));
                }
            } else {
                best_preset_id.set(None);
                status_msg.set(Some((
                    Msg::new(move |en| strings(en).dns.no_server_answered_the_ping.to_string()),
                    Tone::Danger,
                )));
            }
            is_benchmarking.set(false);
        });
    };

    let mut start_apply = move || {
        let preset_id = *selected_preset.read();
        let Some(p) = DNS_PRESETS.iter().find(|p| p.id == preset_id) else {
            return;
        };

        let preset = *p;
        let p_name = p.name;

        is_working.set(true);
        spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                DnsManager::open_default()?.apply_preset_to_physical(&preset)
            })
            .await
            .unwrap_or_else(|e| Err(CoreError::Panic(e.to_string())));

            status_msg.set(Some(match res {
                Ok(count) => (
                    Msg::new(move |en| {
                        let adapters = plural(count, strings(en).adapter);
                        (strings(en).dns.preset_applied)(&p_name, &adapters)
                    }),
                    Tone::Success,
                ),
                Err(CoreError::NoActiveAdapters) => (
                    Msg::new(move |en| strings(en).dns.there_are_no_active_physical.to_string()),
                    Tone::Danger,
                ),
                Err(e) => (Msg::from(e), Tone::Danger),
            }));
            spawn_refresh(adapters, has_backup);
            is_working.set(false);
        });
    };

    let on_restore_dns = move |_| {
        is_working.set(true);
        spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                DnsManager::open_default().and_then(|mgr| mgr.restore_dns())
            })
            .await
            .unwrap_or_else(|e| Err(CoreError::Panic(e.to_string())));

            // El core conserva en el respaldo los adaptadores pendientes y devuelve Err (SYS-08).
            status_msg.set(Some(match res {
                Ok(count) => (
                    Msg::new(move |en| {
                        let adapters = plural(count, strings(en).adapter);
                        (strings(en).dns.previous_dns_restored)(&adapters)
                    }),
                    Tone::Success,
                ),
                Err(e) => (Msg::from(e), Tone::Danger),
            }));
            spawn_refresh(adapters, has_backup);
            is_working.set(false);
        });
    };

    let bench_map = benchmarks.read();
    let best_id = *best_preset_id.read();
    let active_preset = DNS_PRESETS.iter().find(|p| p.id == *selected_preset.read());
    let adapters_state = adapters.read();
    let physical: Vec<String> = match adapters_state.as_ref() {
        Some(Ok(list)) => list
            .iter()
            .filter(|a| !a.is_virtual_or_vpn)
            .map(|a| a.friendly_name.clone())
            .collect(),
        _ => Vec::new(),
    };

    let optimizer_card_title = txt.dns_optimizer.to_string();
    let optimizer_card_sub = Some(txt.public_dns_servers_and_ping.to_string());
    let adapters_card_title = txt.network_adapters.to_string();

    let ping_btn_text = if *is_benchmarking.read() {
        txt.pinging
    } else {
        txt.test_ping
    };

    let apply_dns_lbl = txt.apply_dns;
    let restore_dns_lbl = txt.restore_previous;
    let physical_lbl = txt.physical;

    rsx! {
        div { class: "content-body",
            // UX-04: confirmar con los adaptadores afectados y el aviso de privacidad.
            if *confirm_apply.read() {
                if let Some(p) = active_preset {
                    ConfirmDialog {
                        title: (strings(en).dns.apply)(&p.name),
                        subtitle: Some(if en { p.privacy_notice_en.to_string() } else { p.privacy_notice.to_string() }),
                        confirm_label: apply_dns_lbl.to_string(),
                        cancel_label: strings(en).cancel.to_string(),
                        tone: Tone::Warning,
                        on_cancel: move |_| confirm_apply.set(false),
                        on_confirm: move |_| {
                            confirm_apply.set(false);
                            start_apply();
                        },
                        p { class: "text-xs",
                            "{txt.it_replaces_the_dns_of}"
                        }
                        div { class: "modal-list",
                            for name in physical.iter() {
                                div { key: "{name}", "• {name}" }
                            }
                        }
                    }
                }
            }

            Card {
                title: optimizer_card_title,
                subtitle: optimizer_card_sub,
                div { class: "stack-3",
                    div { class: "row wrap",
                        for p in DNS_PRESETS {
                            {
                                let b = bench_map.get(p.id);
                                let is_best = best_id == Some(p.id);
                                rsx! {
                                    button {
                                        key: "{p.id}",
                                        class: "btn btn-secondary",
                            "aria-pressed": if *selected_preset.read() == p.id { "true" } else { "false" },
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

                    if let Some(active_p) = active_preset {
                        div { class: "info-box text-xs",
                            p { strong { "IPv4: " } "{active_p.ipv4_primary}, {active_p.ipv4_secondary}   |   " strong { "IPv6: " } "{active_p.ipv6_primary}, {active_p.ipv6_secondary}" }
                            p { class: "text-muted", if en { "{active_p.privacy_notice_en}" } else { "{active_p.privacy_notice}" } }
                        }
                    }

                    div { class: "flex gap-3",
                        button {
                            class: "btn btn-primary",
                            disabled: *is_working.read() || physical.is_empty(),
                            onclick: move |_| confirm_apply.set(true),
                            "{apply_dns_lbl}"
                        }
                        // UI-07: «Restaurar» solo existe si hay un respaldo que restaurar.
                        if *has_backup.read() {
                            button {
                                class: "btn btn-secondary",
                                disabled: *is_working.read(),
                                onclick: on_restore_dns,
                                "{restore_dns_lbl}"
                            }
                        }
                    }

                    if let Some((msg, tone)) = status_msg.read().clone().map(|(m, t)| (m.text(en), t)) {
                        StatusBanner { text: msg, tone }
                    }
                }
            }

            Card {
                title: adapters_card_title,
                subtitle: None,
                div { class: "stack-2",
                    match adapters_state.as_ref() {
                        None => rsx! {
                            LoadingState { message: txt.reading_network_adapters.to_string() }
                        },
                        Some(Err(err)) => rsx! {
                            ErrorState {
                                title: txt.could_not_read_the_adapters.to_string(),
                                error_msg: err.text(en),
                            }
                        },
                        Some(Ok(list)) if list.is_empty() => rsx! {
                            EmptyState {
                                title: txt.no_active_adapters.to_string(),
                                description: txt.connect_to_a_network_and.to_string(),
                            }
                        },
                        Some(Ok(list)) => rsx! {
                            for adapter in list.iter() {
                                div {
                                    key: "{adapter.interface_guid}",
                                    class: "item-row row-between",
                                    div {
                                        div { class: "text-sm fw-semibold", "{adapter.friendly_name}" }
                                        div { class: "text-xs text-muted",
                                            "IPv4: "
                                            {if adapter.current_ipv4_dns.is_empty() { "{txt.automatic_dhcp}" } else { adapter.current_ipv4_dns.as_str() }}
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
                        },
                    }
                }
            }
        }
    }
}
