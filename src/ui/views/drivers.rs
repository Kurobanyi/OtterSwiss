use std::collections::HashSet;

use dioxus::prelude::*;

use crate::core::drivers::{
    CatalogDriverMatch, DriverManager, DriverUpdateCandidate, InstalledDriverDevice,
    InstalledDriverRecord, VendorDriverUpdate,
};
use crate::ui::components::{Card, ConfirmDialog, StatusBanner, Tone};
use crate::ui::i18n::{Lang, strings};

/// SEC-37: instalación que espera confirmación del usuario.
#[derive(Debug, Clone)]
enum PendingInstall {
    Wua(Vec<String>),
    Vendor(Box<VendorDriverUpdate>),
    Catalog { uid: String, title: String },
}

/// Resultado de una instalación ya confirmada: mensaje, si pide reinicio y si hubo fallos parciales.
struct InstallOutcome {
    message: String,
    reboot: bool,
    partial: bool,
}

/// Corre en un hilo bloqueante: respaldo opcional del Driver Store y luego la instalación. Si el respaldo pedido
/// falla, no se instala nada.
fn execute_install(
    pending: PendingInstall,
    backup: bool,
    en: bool,
) -> Result<InstallOutcome, String> {
    if backup {
        DriverManager::export_driver_store_backup()
            .map_err(|e| (strings(en).drivers.backup_failed)(&e.to_string()))?;
    }
    let report = match pending {
        PendingInstall::Wua(ids) => DriverManager::install_driver_updates(&ids),
        PendingInstall::Vendor(upd) => DriverManager::install_vendor_update(&upd),
        PendingInstall::Catalog { uid, title } => {
            DriverManager::install_catalog_driver(&uid, &title)
        }
    }
    .map_err(|e| format!("Error: {e}"))?;
    Ok(InstallOutcome {
        partial: report.failed_count > 0,
        reboot: report.reboot_required,
        message: report.message,
    })
}

/// SYS-26: relee el historial de drivers instalados desde OtterSwiss. Sin elevación o sin historial queda vacío.
fn refresh_driver_history(mut history: Signal<Vec<InstalledDriverRecord>>) {
    spawn(async move {
        if let Ok(Ok(list)) =
            tokio::task::spawn_blocking(DriverManager::driver_install_history).await
        {
            history.set(list);
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceFilter {
    All,
    ErrorsOnly,
    ThirdPartyOem,
}

fn trigger_device_scan(
    en: bool,
    mut is_working: Signal<bool>,
    mut status_msg: Signal<Option<(String, Tone)>>,
    mut devices: Signal<Vec<InstalledDriverDevice>>,
) {
    let t = strings(en).drivers;
    is_working.set(true);
    status_msg.set(Some((t.scanning_devices.to_string(), Tone::Info)));

    spawn(async move {
        let res = tokio::task::spawn_blocking(DriverManager::list_present_devices).await;
        match res {
            Ok(Ok(list)) => {
                let count = list.len();
                devices.set(list);
                status_msg.set(Some(((t.devices_detected)(count), Tone::Info)));
            }
            Ok(Err(e)) => {
                status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
            }
            Err(e) => {
                status_msg.set(Some((format!("{}: {e}", t.internal_error), Tone::Danger)));
            }
        }
        is_working.set(false);
    });
}

#[component]
pub fn DriversView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();
    let t = strings(en).drivers;

    let mut devices = use_signal(Vec::<InstalledDriverDevice>::new);
    let mut wua_updates = use_signal(Vec::<DriverUpdateCandidate>::new);
    let mut vendor_updates = use_signal(Vec::<VendorDriverUpdate>::new);
    let mut catalog_matches = use_signal(Vec::<CatalogDriverMatch>::new);
    let mut selected_wua = use_signal(HashSet::<String>::new);
    let mut has_checked_updates = use_signal(|| false);
    let mut wua_policy_warn = use_signal(|| false);

    let mut search_query = use_signal(String::new);
    let mut filter_mode = use_signal(|| DeviceFilter::All);
    let mut is_working = use_signal(|| false);
    let mut status_msg = use_signal(|| Option::<(String, Tone)>::None);
    let mut reboot_alert = use_signal(|| false);
    let mut pending_install = use_signal(|| Option::<PendingInstall>::None);
    let mut backup_first = use_signal(|| true);
    let install_history = use_signal(Vec::<InstalledDriverRecord>::new);
    let mut pending_revert = use_signal(|| Option::<InstalledDriverRecord>::None);

    use_hook(move || {
        trigger_device_scan(en, is_working, status_msg, devices);
        refresh_driver_history(install_history);
    });

    let on_confirm_revert = move |_| {
        let Some(record) = pending_revert.take() else {
            return;
        };
        is_working.set(true);
        spawn(async move {
            let name = record.published_name.clone();
            let res =
                tokio::task::spawn_blocking(move || DriverManager::revert_installed_driver(&name))
                    .await;
            match res {
                Ok(Ok(rep)) => {
                    if rep.reboot_required {
                        reboot_alert.set(true);
                    }
                    status_msg.set(Some((rep.message, Tone::Success)));
                    if let Ok(Ok(refreshed)) =
                        tokio::task::spawn_blocking(DriverManager::list_present_devices).await
                    {
                        devices.set(refreshed);
                    }
                }
                Ok(Err(e)) => status_msg.set(Some((format!("Error: {e}"), Tone::Danger))),
                Err(e) => {
                    status_msg.set(Some((format!("{}: {e}", t.internal_error), Tone::Danger)))
                }
            }
            refresh_driver_history(install_history);
            is_working.set(false);
        });
    };

    let on_refresh_devices = move |_| {
        trigger_device_scan(en, is_working, status_msg, devices);
    };

    let on_full_scan = move |_| {
        let current_devs = devices.read().clone();
        is_working.set(true);
        status_msg.set(Some((t.scanning_sources.to_string(), Tone::Info)));

        spawn(async move {
            let res =
                tokio::task::spawn_blocking(move || DriverManager::run_full_scan(&current_devs))
                    .await;

            match res {
                Ok(report) => {
                    let safe_wua_ids: HashSet<String> = report
                        .wua_updates
                        .iter()
                        .filter(|u| !u.is_firmware && !u.is_optional)
                        .map(|u| u.id.clone())
                        .collect();

                    let total_found = report.wua_updates.len()
                        + report.vendor_updates.len()
                        + report.catalog_matches.len();

                    wua_policy_warn.set(report.wua_policy_blocked);
                    selected_wua.set(safe_wua_ids);
                    wua_updates.set(report.wua_updates);
                    vendor_updates.set(report.vendor_updates);
                    catalog_matches.set(report.catalog_matches);
                    has_checked_updates.set(true);

                    // ERR-20: avisos de lo que no se pudo consultar (Windows Update, fabricantes, Catálogo).
                    let warnings: Vec<String> =
                        report.wua_note.into_iter().chain(report.notes).collect();
                    let summary = if total_found == 0 {
                        t.no_updates.to_string()
                    } else {
                        (t.updates_found)(total_found)
                    };
                    if warnings.is_empty() {
                        status_msg.set(Some((summary, Tone::Success)));
                    } else {
                        status_msg.set(Some((
                            format!(
                                "{summary}\n{}\n• {}",
                                t.sources_failed,
                                warnings.join("\n• ")
                            ),
                            Tone::Warning,
                        )));
                    }
                }
                Err(e) => {
                    status_msg.set(Some((format!("{}: {e}", t.internal_error), Tone::Danger)));
                }
            }
            is_working.set(false);
        });
    };

    let on_install_wua = move |_| {
        let ids: Vec<String> = selected_wua.read().iter().cloned().collect();
        if !ids.is_empty() {
            pending_install.set(Some(PendingInstall::Wua(ids)));
        }
    };

    let on_confirm_install = move |_| {
        let Some(pending) = pending_install.take() else {
            return;
        };
        let backup = *backup_first.read();
        let is_wua = matches!(pending, PendingInstall::Wua(_));
        is_working.set(true);
        status_msg.set(Some((t.installing.to_string(), Tone::Info)));
        spawn(async move {
            let res =
                tokio::task::spawn_blocking(move || execute_install(pending, backup, en)).await;
            match res {
                Ok(Ok(outcome)) => {
                    if outcome.reboot {
                        reboot_alert.set(true);
                    }
                    let tone = if outcome.partial {
                        Tone::Warning
                    } else {
                        Tone::Success
                    };
                    status_msg.set(Some((outcome.message, tone)));
                    // ERR-18: con fallos la lista queda para reintentar o ver cuál falló.
                    if is_wua && !outcome.partial {
                        wua_updates.write().clear();
                        selected_wua.write().clear();
                    }
                    if let Ok(Ok(refreshed)) =
                        tokio::task::spawn_blocking(DriverManager::list_present_devices).await
                    {
                        devices.set(refreshed);
                    }
                    refresh_driver_history(install_history);
                }
                Ok(Err(msg)) => status_msg.set(Some((msg, Tone::Danger))),
                Err(e) => {
                    status_msg.set(Some((format!("{}: {e}", t.internal_error), Tone::Danger)))
                }
            }
            is_working.set(false);
        });
    };

    let on_backup_oem = move |_| {
        is_working.set(true);
        status_msg.set(Some((t.exporting_backup.to_string(), Tone::Info)));

        spawn(async move {
            let res = tokio::task::spawn_blocking(DriverManager::export_driver_store_backup).await;
            match res {
                Ok(Ok(path)) => {
                    status_msg.set(Some((
                        (t.backup_saved)(&path.display().to_string()),
                        Tone::Success,
                    )));
                }
                Ok(Err(e)) => {
                    status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
                }
                Err(e) => {
                    status_msg.set(Some((format!("{}: {e}", t.internal_error), Tone::Danger)));
                }
            }
            is_working.set(false);
        });
    };

    let devs_guard = devices.read();
    let total_devs = devs_guard.len();
    let problem_count = devs_guard.iter().filter(|d| d.has_problem).count();
    let oem_count = devs_guard.iter().filter(|d| d.is_third_party_oem()).count();

    let q = search_query.read().trim().to_ascii_lowercase();
    let cur_filter = *filter_mode.read();
    let filtered_devs: Vec<InstalledDriverDevice> = devs_guard
        .iter()
        .filter(|d| match cur_filter {
            DeviceFilter::All => true,
            DeviceFilter::ErrorsOnly => d.has_problem,
            DeviceFilter::ThirdPartyOem => d.is_third_party_oem(),
        })
        .filter(|d| {
            if q.is_empty() {
                return true;
            }
            d.display_title().to_ascii_lowercase().contains(&q)
                || d.manufacturer
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(&q)
                || d.provider_name
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(&q)
                || d.inf_path
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(&q)
                || d.short_hw_id()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&q)
                || d.decoded_vendor_name()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(&q)
                || d.hardware_hint()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(&q)
        })
        .cloned()
        .collect();

    let wua_guard = wua_updates.read();
    let vendor_guard = vendor_updates.read();
    let catalog_guard = catalog_matches.read();
    let selected_wua_count = selected_wua.read().len();

    let problems_badge_txt = if problem_count > 0 {
        (t.device_errors)(problem_count)
    } else {
        t.all_ok.to_string()
    };

    rsx! {
        div { class: "content-body",
            Card {
                title: t.title.to_string(),
                subtitle: Some(t.subtitle.to_string()),
                div { class: "stack-3",
                    div { class: "row wrap",
                        button {
                            class: "btn btn-primary",
                            disabled: *is_working.read(),
                            onclick: on_full_scan,
                            "{t.full_scan}"
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_working.read(),
                            onclick: on_refresh_devices,
                            "{t.refresh_hardware}"
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_working.read(),
                            onclick: on_backup_oem,
                            "{t.backup_oem}"
                        }
                        span {
                            class: if problem_count > 0 { "badge badge-danger" } else { "badge badge-success" },
                            "{problems_badge_txt}"
                        }
                    }

                    if let Some(pending) = pending_install.read().clone() {
                        {
                            let items: Vec<(String, bool)> = match &pending {
                                PendingInstall::Wua(ids) => wua_updates
                                    .read()
                                    .iter()
                                    .filter(|u| ids.contains(&u.id))
                                    .map(|u| (u.title.clone(), u.is_firmware))
                                    .collect(),
                                PendingInstall::Vendor(upd) => vec![(
                                    format!("{} {} → {}", upd.vendor, upd.title, upd.latest_version),
                                    false,
                                )],
                                PendingInstall::Catalog { title, .. } => vec![(title.clone(), false)],
                            };
                            let has_firmware = items.iter().any(|(_, fw)| *fw);
                            rsx! {
                                ConfirmDialog {
                                    title: t.confirm_title.to_string(),
                                    subtitle: Some(t.confirm_subtitle.to_string()),
                                    confirm_label: t.install.to_string(),
                                    cancel_label: strings(en).cancel.to_string(),
                                    tone: if has_firmware { Tone::Danger } else { Tone::Warning },
                                    on_confirm: on_confirm_install,
                                    on_cancel: move |_| pending_install.set(None),
                                    ul { class: "stack-1",
                                        for (title, fw) in items.iter() {
                                            li { key: "{title}", class: "text-sm",
                                                "{title}"
                                                if *fw {
                                                    span { class: "badge badge-danger", " FIRMWARE / BIOS" }
                                                }
                                            }
                                        }
                                    }
                                    if has_firmware {
                                        StatusBanner { text: t.firmware_warning.to_string(), tone: Tone::Danger }
                                    }
                                    label { class: "row gap-2 clickable",
                                        input {
                                            r#type: "checkbox",
                                            checked: *backup_first.read(),
                                            onchange: move |evt| backup_first.set(evt.checked()),
                                        }
                                        span { class: "text-sm", "{t.backup_first}" }
                                    }
                                }
                            }
                        }
                    }

                    if *wua_policy_warn.read() {
                        StatusBanner { text: t.policy_managed.to_string(), tone: Tone::Warning }
                    }

                    if *reboot_alert.read() {
                        StatusBanner { text: t.reboot_needed.to_string(), tone: Tone::Warning }
                    }

                    if let Some((msg, tone)) = status_msg.read().clone() {
                        StatusBanner { text: msg, tone: tone }
                    }
                }
            }

            if !vendor_guard.is_empty() {
                Card {
                    title: t.vendor_title.to_string(),
                    subtitle: Some(t.vendor_subtitle.to_string()),
                    div { class: "stack-2",
                        for v_upd in vendor_guard.iter() {
                            {
                                let upd_clone = v_upd.clone();
                                rsx! {
                                    div {
                                        key: "{v_upd.id}",
                                        class: "item-row row-between gap-3 wrap",
                                        div { class: "grow-wide",
                                            div { class: "row wrap",
                                                span { class: "badge badge-info", "{v_upd.vendor}" }
                                                span { class: "text-sm fw-semibold", "{v_upd.title}" }
                                            }
                                            div { class: "text-xs text-muted mt-1",
                                                "{v_upd.current_version} → {v_upd.latest_version} • {v_upd.size_label}"
                                                if !v_upd.release_date.is_empty() {
                                                    " • {v_upd.release_date}"
                                                }
                                            }
                                        }
                                        button {
                                            class: "btn btn-primary",
                                            disabled: *is_working.read(),
                                            onclick: move |_| {
                                                pending_install.set(Some(PendingInstall::Vendor(Box::new(upd_clone.clone()))));
                                            },
                                            "{t.download_install}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !install_history.read().is_empty() {
                Card {
                    title: t.history_title.to_string(),
                    subtitle: Some(t.history_subtitle.to_string()),
                    div { class: "stack-2",
                        for rec in install_history.read().iter() {
                            {
                                let rec_c = rec.clone();
                                rsx! {
                                    div { key: "{rec.published_name}", class: "list-row row-between gap-3",
                                        div {
                                            div { class: "text-sm fw-semibold", "{rec.title}" }
                                            div { class: "text-xs text-muted", "{rec.published_name}" }
                                        }
                                        button {
                                            class: "btn btn-secondary btn-sm",
                                            disabled: *is_working.read(),
                                            onclick: move |_| pending_revert.set(Some(rec_c.clone())),
                                            "{t.remove}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(rec) = pending_revert.read().clone() {
                ConfirmDialog {
                    title: t.remove_title.to_string(),
                    subtitle: Some(format!("{} ({})", rec.title, rec.published_name)),
                    confirm_label: t.remove.to_string(),
                    cancel_label: strings(en).cancel.to_string(),
                    tone: Tone::Warning,
                    on_confirm: on_confirm_revert,
                    on_cancel: move |_| pending_revert.set(None),
                    p { class: "text-sm", "{t.remove_body}" }
                }
            }

            if !catalog_guard.is_empty() {
                Card {
                    title: t.catalog_title.to_string(),
                    subtitle: Some(t.catalog_subtitle.to_string()),
                    div { class: "stack-2",
                        for cat in catalog_guard.iter() {
                            {
                                let uid = cat.update_id.clone();
                                let title_c = cat.title.clone();
                                rsx! {
                                    div {
                                        key: "{uid}",
                                        class: "item-row row-between gap-3 wrap",
                                        div { class: "grow-wide",
                                            div { class: "text-sm fw-semibold", "{cat.title}" }
                                            div { class: "text-xs text-muted mt-1",
                                                "{cat.query_hw_id} • v{cat.version} • {cat.last_updated} • {cat.size_str}"
                                            }
                                        }
                                        button {
                                            class: "btn btn-primary",
                                            disabled: *is_working.read(),
                                            onclick: move |_| {
                                                pending_install.set(Some(PendingInstall::Catalog {
                                                    uid: uid.clone(),
                                                    title: title_c.clone(),
                                                }));
                                            },
                                            "{t.install_cab}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !wua_guard.is_empty() {
                Card {
                    title: t.wua_title.to_string(),
                    subtitle: Some(t.wua_subtitle.to_string()),
                    div { class: "stack-2",
                        for upd in wua_guard.iter() {
                            {
                                let uid = upd.id.clone();
                                let checked = selected_wua.read().contains(&uid);
                                let size_txt = if upd.size_mb > 0 {
                                    format!("{} MB", upd.size_mb)
                                } else {
                                    "—".to_string()
                                };

                                rsx! {
                                    label {
                                        key: "{uid}",
                                        class: "item-row row-between gap-3 clickable",
                                        div { class: "row grow",
                                            input {
                                                r#type: "checkbox",
                                                checked: checked,
                                                onchange: move |evt| {
                                                    let mut s = selected_wua.write();
                                                    if evt.checked() {
                                                        s.insert(uid.clone());
                                                    } else {
                                                        s.remove(&uid);
                                                    }
                                                },
                                            }
                                            div { class: "min-w-0",
                                                div { class: "row wrap",
                                                    span { class: "text-sm fw-semibold", "{upd.title}" }
                                                    if upd.is_firmware {
                                                        span { class: "badge badge-danger", "FIRMWARE / BIOS" }
                                                    }
                                                }
                                                div { class: "text-xs text-muted mt-1",
                                                    if !upd.driver_provider.is_empty() {
                                                        "{upd.driver_provider} • "
                                                    }
                                                    if !upd.driver_class.is_empty() {
                                                        "{upd.driver_class} • "
                                                    }
                                                    if !upd.hardware_id.is_empty() {
                                                        "{upd.hardware_id}"
                                                    }
                                                }
                                            }
                                        }
                                        span { class: "badge badge-info", "{size_txt}" }
                                    }
                                }
                            }
                        }

                        div { class: "row-end",
                            button {
                                class: "btn btn-primary",
                                disabled: *is_working.read() || selected_wua_count == 0,
                                onclick: on_install_wua,
                                "{(t.download_install_n)(selected_wua_count)}"
                            }
                        }
                    }
                }
            }

            Card {
                title: t.hardware_title.to_string(),
                subtitle: Some((t.showing_devices)(filtered_devs.len())),
                div { class: "stack-3",
                    div { class: "row wrap",
                        input {
                            r#type: "text",
                            class: "input grow-wide",
                            placeholder: "{t.search_placeholder}",
                            value: "{search_query.read()}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                        // TE-28: el filtro elegido se marca con aria-pressed, no alternando btn-primary.
                        button {
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if cur_filter == DeviceFilter::All { "true" } else { "false" },
                            onclick: move |_| filter_mode.set(DeviceFilter::All),
                            "{(t.filter_all)(total_devs)}"
                        }
                        button {
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if cur_filter == DeviceFilter::ErrorsOnly { "true" } else { "false" },
                            onclick: move |_| filter_mode.set(DeviceFilter::ErrorsOnly),
                            "{(t.filter_errors)(problem_count)}"
                        }
                        button {
                            class: "btn btn-secondary btn-sm",
                            "aria-pressed": if cur_filter == DeviceFilter::ThirdPartyOem { "true" } else { "false" },
                            onclick: move |_| filter_mode.set(DeviceFilter::ThirdPartyOem),
                            "{(t.filter_oem)(oem_count)}"
                        }
                    }

                    div { class: "modal-list tall",
                        for dev in filtered_devs.into_iter() {
                            {
                                let title_str = dev.display_title().to_string();
                                let vendor_str = dev
                                    .provider_name
                                    .as_deref()
                                    .or(dev.manufacturer.as_deref())
                                    .unwrap_or("")
                                    .to_string();
                                let decoded_ven = dev.decoded_vendor_name().unwrap_or("").to_string();
                                let hw_hint = dev.hardware_hint().unwrap_or("").to_string();
                                let short_hw = dev.short_hw_id().unwrap_or_default();
                                let ver_str = dev.driver_version.as_deref().unwrap_or("").to_string();
                                let date_str = dev.driver_date.as_deref().unwrap_or("").to_string();
                                let inf_str = dev.inf_path.as_deref().unwrap_or("").to_string();
                                let class_str = dev.class_name.as_deref().unwrap_or("").to_string();
                                let nv_ver = dev.friendly_gpu_version();

                                let status_badge_cls = if dev.has_problem {
                                    "badge badge-danger"
                                } else if dev.needs_reboot {
                                    "badge badge-warning"
                                } else {
                                    "badge badge-success"
                                };

                                let status_badge_txt = if dev.has_problem {
                                    (t.cm_problem)(dev.problem_code)
                                } else if dev.needs_reboot {
                                    t.pending_reboot
                                } else {
                                    "OK"
                                };

                                let key_str = format!("dev_{}_{}", dev.dev_inst, title_str);
                                let hw_for_btn = short_hw.clone();

                                rsx! {
                                    div {
                                        key: "{key_str}",
                                        class: "item-row row-between wrap",
                                        div { class: "grow-wide",
                                            div { class: "row wrap",
                                                span { class: "text-sm fw-semibold", "{title_str}" }
                                                if !hw_hint.is_empty() {
                                                    span { class: "badge badge-warning", "{hw_hint}" }
                                                } else if !decoded_ven.is_empty() {
                                                    span { class: "badge badge-info", "{decoded_ven}" }
                                                }
                                                if !class_str.is_empty() {
                                                    span { class: "badge badge-info", "{class_str}" }
                                                }
                                                if let Some(nv) = nv_ver {
                                                    span { class: "badge badge-success", "GeForce {nv}" }
                                                }
                                            }
                                            div { class: "text-xs text-muted mt-1",
                                                if !vendor_str.is_empty() {
                                                    "{vendor_str} • "
                                                }
                                                if !ver_str.is_empty() {
                                                    "v{ver_str} • "
                                                }
                                                if !date_str.is_empty() {
                                                    "{date_str} • "
                                                }
                                                if !inf_str.is_empty() {
                                                    "{inf_str} • "
                                                }
                                                if !short_hw.is_empty() {
                                                    "{short_hw}"
                                                }
                                            }
                                        }
                                        div { class: "row",
                                            if dev.has_problem && !hw_for_btn.is_empty() {
                                                button {
                                                    class: "btn btn-secondary",
                                                    disabled: *is_working.read(),
                                                    onclick: move |_| {
                                                        let q_id = hw_for_btn.clone();
                                                        is_working.set(true);
                                                        status_msg.set(Some(((t.searching_catalog)(&q_id), Tone::Info)));
                                                        spawn(async move {
                                                            let res = tokio::task::spawn_blocking(move || {
                                                                DriverManager::search_microsoft_catalog(&q_id)
                                                            })
                                                            .await;
                                                            match res {
                                                                Ok(Ok(list)) => {
                                                                    let n = list.len();
                                                                    catalog_matches.set(list);
                                                                    status_msg.set(Some(((t.catalog_found)(n), Tone::Info)));
                                                                }
                                                                Ok(Err(e)) => {
                                                                    status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
                                                                }
                                                                Err(e) => {
                                                                    status_msg.set(Some((format!("{}: {e}", t.internal_error), Tone::Danger)));
                                                                }
                                                            }
                                                            is_working.set(false);
                                                        });
                                                    },
                                                    "{t.search_catalog}"
                                                }
                                            }
                                            span { class: "{status_badge_cls}", "{status_badge_txt}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
