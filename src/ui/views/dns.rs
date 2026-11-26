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
use crate::ui::i18n::{Lang, Msg, pick, plural, strings};

/// Estado de la lista de adaptadores: `None` mientras se lee (UI-07).
type AdaptersState = Option<Result<Vec<NetworkAdapterInfo>, Msg>>;

/// Las señales de la vista, para pasarlas juntas a los subcomponentes y a las acciones.
#[derive(Clone, Copy, PartialEq)]
struct DnsSignals {
    adapters: Signal<AdaptersState>,
    has_backup: Signal<bool>,
    selected_preset: Signal<&'static str>,
    status_msg: Signal<Option<(Msg, Tone)>>,
    is_working: Signal<bool>,
    is_benchmarking: Signal<bool>,
    confirm_apply: Signal<bool>,
    benchmarks: Signal<HashMap<&'static str, DnsBenchmarkResult>>,
    best_preset_id: Signal<Option<&'static str>>,
}

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

/// Mide el ping a cada servidor y deja elegido el más rápido.
fn start_benchmark(mut sig: DnsSignals) {
    sig.is_benchmarking.set(true);
    sig.status_msg.set(Some((
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

        sig.benchmarks.set(map);
        if let Some((best_id, best_ms)) = best {
            sig.best_preset_id.set(Some(best_id));
            sig.selected_preset.set(best_id);
            if let Some(p) = DNS_PRESETS.iter().find(|x| x.id == best_id) {
                // UI-15: es ping ICMP al servidor, no tiempo de resolución DNS.
                sig.status_msg.set(Some((
                    Msg::new(move |en| (strings(en).dns.lowest_ping_ms)(&p.name, &best_ms)),
                    Tone::Success,
                )));
            }
        } else {
            sig.best_preset_id.set(None);
            sig.status_msg.set(Some((
                Msg::new(move |en| strings(en).dns.no_server_answered_the_ping.to_string()),
                Tone::Danger,
            )));
        }
        sig.is_benchmarking.set(false);
    });
}

/// Aplica el servidor elegido a los adaptadores físicos.
fn start_apply(mut sig: DnsSignals) {
    let preset_id = *sig.selected_preset.read();
    let Some(p) = DNS_PRESETS.iter().find(|p| p.id == preset_id) else {
        return;
    };

    let preset = *p;
    let p_name = p.name;

    sig.is_working.set(true);
    spawn(async move {
        let res = tokio::task::spawn_blocking(move || {
            DnsManager::open_default()?.apply_preset_to_physical(&preset)
        })
        .await
        .unwrap_or_else(|e| Err(CoreError::Panic(e.to_string())));

        sig.status_msg.set(Some(match res {
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
        spawn_refresh(sig.adapters, sig.has_backup);
        sig.is_working.set(false);
    });
}

/// Devuelve los adaptadores al DNS que tenían antes de aplicar uno.
fn start_restore(mut sig: DnsSignals) {
    sig.is_working.set(true);
    spawn(async move {
        let res = tokio::task::spawn_blocking(move || {
            DnsManager::open_default().and_then(|mgr| mgr.restore_dns())
        })
        .await
        .unwrap_or_else(|e| Err(CoreError::Panic(e.to_string())));

        // El core conserva en el respaldo los adaptadores pendientes y devuelve Err (SYS-08).
        sig.status_msg.set(Some(match res {
            Ok(count) => (
                Msg::new(move |en| {
                    let adapters = plural(count, strings(en).adapter);
                    (strings(en).dns.previous_dns_restored)(&adapters)
                }),
                Tone::Success,
            ),
            Err(e) => (Msg::from(e), Tone::Danger),
        }));
        spawn_refresh(sig.adapters, sig.has_backup);
        sig.is_working.set(false);
    });
}

/// Nombres de los adaptadores físicos (sin virtuales ni VPN), que son los que cambia «Aplicar».
fn physical_names(adapters: &AdaptersState) -> Vec<String> {
    match adapters {
        Some(Ok(list)) => list
            .iter()
            .filter(|a| !a.is_virtual_or_vpn)
            .map(|a| a.friendly_name.clone())
            .collect(),
        _ => Vec::new(),
    }
}

#[component]
pub fn DnsView() -> Element {
    let adapters = use_signal(|| AdaptersState::None);
    let has_backup = use_signal(|| false);
    let sig = DnsSignals {
        adapters,
        has_backup,
        selected_preset: use_signal(|| "cloudflare"),
        status_msg: use_signal(|| None),
        is_working: use_signal(|| false),
        is_benchmarking: use_signal(|| false),
        confirm_apply: use_signal(|| false),
        benchmarks: use_signal(HashMap::new),
        best_preset_id: use_signal(|| None),
    };

    use_hook(move || spawn_refresh(adapters, has_backup));

    let physical = physical_names(&adapters.read());

    rsx! {
        div { class: "content-body",
            if *sig.confirm_apply.read() {
                ApplyConfirm { sig, physical: physical.clone() }
            }
            OptimizerCard { sig, can_apply: !physical.is_empty() }
            AdaptersCard { adapters }
        }
    }
}

/// UX-04: confirmar con los adaptadores afectados y el aviso de privacidad.
#[component]
fn ApplyConfirm(sig: DnsSignals, physical: Vec<String>) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dns;
    let mut confirm_apply = sig.confirm_apply;
    let Some(p) = DNS_PRESETS
        .iter()
        .find(|p| p.id == *sig.selected_preset.read())
    else {
        return rsx! {};
    };

    rsx! {
        ConfirmDialog {
            title: (txt.apply)(&p.name),
            subtitle: Some(pick(en, p.privacy_notice, p.privacy_notice_en).to_string()),
            confirm_label: txt.apply_dns.to_string(),
            cancel_label: strings(en).cancel.to_string(),
            tone: Tone::Warning,
            on_cancel: move |_| confirm_apply.set(false),
            on_confirm: move |_| {
                confirm_apply.set(false);
                start_apply(sig);
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

/// Servidores públicos con su ping, aplicar y restaurar.
#[component]
fn OptimizerCard(sig: DnsSignals, can_apply: bool) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dns;
    let mut selected_preset = sig.selected_preset;
    let mut confirm_apply = sig.confirm_apply;
    let bench_map = sig.benchmarks.read();
    let best_id = *sig.best_preset_id.read();
    let active_preset = DNS_PRESETS.iter().find(|p| p.id == *selected_preset.read());

    let ping_btn_text = if *sig.is_benchmarking.read() {
        txt.pinging
    } else {
        txt.test_ping
    };

    rsx! {
        Card {
            title: txt.dns_optimizer.to_string(),
            subtitle: Some(txt.public_dns_servers_and_ping.to_string()),
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
                        disabled: *sig.is_benchmarking.read() || *sig.is_working.read(),
                        onclick: move |_| start_benchmark(sig),
                        "{ping_btn_text}"
                    }
                }

                if let Some(active_p) = active_preset {
                    div { class: "info-box text-xs",
                        p { strong { "IPv4: " } "{active_p.ipv4_primary}, {active_p.ipv4_secondary}   |   " strong { "IPv6: " } "{active_p.ipv6_primary}, {active_p.ipv6_secondary}" }
                        p { class: "text-muted", "{pick(en, active_p.privacy_notice, active_p.privacy_notice_en)}" }
                    }
                }

                div { class: "flex gap-3",
                    button {
                        class: "btn btn-primary",
                        disabled: *sig.is_working.read() || !can_apply,
                        onclick: move |_| confirm_apply.set(true),
                        "{txt.apply_dns}"
                    }
                    // UI-07: «Restaurar» solo existe si hay un respaldo que restaurar.
                    if *sig.has_backup.read() {
                        button {
                            class: "btn btn-secondary",
                            disabled: *sig.is_working.read(),
                            onclick: move |_| start_restore(sig),
                            "{txt.restore_previous}"
                        }
                    }
                }

                if let Some((msg, tone)) = sig.status_msg.read().clone().map(|(m, t)| (m.text(en), t)) {
                    StatusBanner { text: msg, tone }
                }
            }
        }
    }
}

/// Adaptadores de red activos con el DNS que usan.
#[component]
fn AdaptersCard(adapters: Signal<AdaptersState>) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let txt = strings(en).dns;
    let adapters_state = adapters.read();

    rsx! {
        Card {
            title: txt.network_adapters.to_string(),
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
                                    span { class: "badge badge-success", "{txt.physical}" }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
