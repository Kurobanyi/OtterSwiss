use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::core::drivers::{
    CatalogDriverMatch, DriverManager, DriverUpdateCandidate, InstalledDriverDevice,
    InstalledDriverRecord, VendorDriverUpdate,
};
use crate::ui::components::{
    Card, CheckRow, ConfirmDialog, EmptyState, LoadingState, StatusBanner, Tone,
};
use crate::ui::i18n::drivers::DriversStrings;
use crate::ui::i18n::{Lang, Msg, strings};
use crate::ui::task::run_blocking;

/// SEC-37: instalación que espera confirmación del usuario.
#[derive(Debug, Clone)]
enum PendingInstall {
    Wua(Vec<String>),
    Vendor(Box<VendorDriverUpdate>),
    Catalog { uid: String, title: String },
}

/// Resultado de una instalación ya confirmada: aviso, si pide reinicio y si hubo fallos parciales.
struct InstallOutcome {
    message: Msg,
    reboot: bool,
    partial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceFilter {
    All,
    ErrorsOnly,
    ThirdPartyOem,
}

/// Texto fijo del catálogo de Drivers como aviso que se traduce al mostrarse (IN-03).
fn drv(text: fn(&'static DriversStrings) -> &'static str) -> Msg {
    Msg::new(move |en| text(strings(en).drivers).to_string())
}

/// Aviso de un hilo que entró en pánico o se canceló.
fn thread_error(e: tokio::task::JoinError) -> Msg {
    let detail = e.to_string();
    Msg::new(move |en| format!("{}: {detail}", strings(en).drivers.internal_error))
}

/// AR-26: resultado de una instalación como frase, con el aviso del historial si no se pudo anotar (SYS-26).
fn install_message(ok: usize, failed: usize, history_warning: Option<String>) -> Msg {
    Msg::new(move |en| {
        let t = strings(en).drivers;
        let mut text = (t.installed)(ok, failed);
        if let Some(w) = &history_warning {
            text.push_str(&format!(" {} {w}", t.history_not_saved));
        }
        text
    })
}

/// Corre en un hilo bloqueante: respaldo opcional del Driver Store y luego la instalación. Si el respaldo pedido
/// falla, no se instala nada.
fn execute_install(pending: PendingInstall, backup: bool) -> Result<InstallOutcome, Msg> {
    if backup {
        DriverManager::export_driver_store_backup().map_err(|e| {
            let err = Msg::from(e);
            Msg::new(move |en| (strings(en).drivers.backup_failed)(&err.text(en)))
        })?;
    }
    let report = match pending {
        PendingInstall::Wua(ids) => DriverManager::install_driver_updates(&ids),
        PendingInstall::Vendor(upd) => DriverManager::install_vendor_update(&upd),
        PendingInstall::Catalog { uid, title } => {
            DriverManager::install_catalog_driver(&uid, &title)
        }
    }
    .map_err(Msg::from)?;
    Ok(InstallOutcome {
        partial: report.failed_count > 0,
        reboot: report.reboot_required,
        message: install_message(
            report.succeeded_count,
            report.failed_count,
            report.history_warning,
        ),
    })
}

/// SYS-26: relee el historial de drivers instalados desde OtterSwiss. Sin elevación o sin historial queda vacío.
fn refresh_driver_history(mut history: Signal<Vec<InstalledDriverRecord>>) {
    spawn(async move {
        if let Ok(list) = run_blocking(DriverManager::driver_install_history).await {
            history.set(list);
        }
    });
}

fn trigger_device_scan(
    mut is_working: Signal<bool>,
    mut status_msg: Signal<Option<(Msg, Tone)>>,
    mut devices: Signal<Vec<InstalledDriverDevice>>,
) {
    is_working.set(true);
    status_msg.set(Some((drv(|t| t.scanning_devices), Tone::Info)));

    spawn(async move {
        let res = run_blocking(DriverManager::list_present_devices).await;
        match res {
            Ok(list) => {
                let count = list.len();
                devices.set(list);
                status_msg.set(Some((
                    Msg::new(move |en| (strings(en).drivers.devices_detected)(count)),
                    Tone::Info,
                )));
            }
            Err(e) => status_msg.set(Some((e, Tone::Danger))),
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

    let mut is_working = use_signal(|| false);
    let mut status_msg = use_signal(|| Option::<(Msg, Tone)>::None);
    let mut reboot_alert = use_signal(|| false);
    let mut pending_install = use_signal(|| Option::<PendingInstall>::None);
    let backup_first = use_signal(|| true);
    let install_history = use_signal(Vec::<InstalledDriverRecord>::new);
    let mut pending_revert = use_signal(|| Option::<InstalledDriverRecord>::None);
    // CON-03: bandera de la búsqueda en curso; cada búsqueda tiene la suya, así una cancelada no pisa a la siguiente.
    let mut scan_cancel = use_signal(|| Option::<Arc<AtomicBool>>::None);

    use_hook(move || {
        trigger_device_scan(is_working, status_msg, devices);
        refresh_driver_history(install_history);
    });

    let on_confirm_revert = move |_| {
        let Some(record) = pending_revert.take() else {
            return;
        };
        is_working.set(true);
        spawn(async move {
            let name = record.published_name.clone();
            let res = run_blocking(move || DriverManager::revert_installed_driver(&name)).await;
            match res {
                Ok(rep) => {
                    if rep.reboot_required {
                        reboot_alert.set(true);
                    }
                    let (title, file) = (record.title.clone(), record.published_name.clone());
                    status_msg.set(Some((
                        Msg::new(move |en| (strings(en).drivers.removed)(&title, &file)),
                        Tone::Success,
                    )));
                    if let Ok(refreshed) = run_blocking(DriverManager::list_present_devices).await {
                        devices.set(refreshed);
                    }
                }
                Err(e) => status_msg.set(Some((e, Tone::Danger))),
            }
            refresh_driver_history(install_history);
            is_working.set(false);
        });
    };

    let on_refresh_devices = move |_| {
        trigger_device_scan(is_working, status_msg, devices);
    };

    let on_full_scan = move |_| {
        let current_devs = devices.read().clone();
        let cancel = Arc::new(AtomicBool::new(false));
        scan_cancel.set(Some(Arc::clone(&cancel)));
        is_working.set(true);
        status_msg.set(Some((drv(|t| t.scanning_sources), Tone::Info)));

        spawn(async move {
            let worker_cancel = Arc::clone(&cancel);
            let res = tokio::task::spawn_blocking(move || {
                DriverManager::run_full_scan(&current_devs, &worker_cancel)
            })
            .await;
            // Cancelada: la vista ya quedó libre y quizá haya otra tarea en curso; no se toca nada.
            if cancel.load(Ordering::Acquire) {
                return;
            }
            scan_cancel.set(None);

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
                    let tone = if warnings.is_empty() {
                        Tone::Success
                    } else {
                        Tone::Warning
                    };
                    let summary = Msg::new(move |en| {
                        let t = strings(en).drivers;
                        let mut text = if total_found == 0 {
                            t.no_updates.to_string()
                        } else {
                            (t.updates_found)(total_found)
                        };
                        if !warnings.is_empty() {
                            text.push_str(&format!(
                                "\n{}\n• {}",
                                t.sources_failed,
                                warnings.join("\n• ")
                            ));
                        }
                        text
                    });
                    status_msg.set(Some((summary, tone)));
                }
                Err(e) => {
                    status_msg.set(Some((thread_error(e), Tone::Danger)));
                }
            }
            is_working.set(false);
        });
    };

    let on_cancel_scan = move |_| {
        if let Some(flag) = scan_cancel.take() {
            flag.store(true, Ordering::Release);
            is_working.set(false);
            status_msg.set(Some((drv(|t| t.scan_cancelled), Tone::Info)));
        }
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
        status_msg.set(Some((drv(|t| t.installing), Tone::Info)));
        spawn(async move {
            let res = run_blocking(move || execute_install(pending, backup)).await;
            match res {
                Ok(outcome) => {
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
                    if let Ok(refreshed) = run_blocking(DriverManager::list_present_devices).await {
                        devices.set(refreshed);
                    }
                    refresh_driver_history(install_history);
                }
                Err(e) => status_msg.set(Some((e, Tone::Danger))),
            }
            is_working.set(false);
        });
    };

    let on_backup_oem = move |_| {
        is_working.set(true);
        status_msg.set(Some((drv(|t| t.exporting_backup), Tone::Info)));

        spawn(async move {
            let res = run_blocking(DriverManager::export_driver_store_backup).await;
            match res {
                Ok(path) => {
                    let path = path.display().to_string();
                    status_msg.set(Some((
                        Msg::new(move |en| (strings(en).drivers.backup_saved)(&path)),
                        Tone::Success,
                    )));
                }
                Err(e) => status_msg.set(Some((e, Tone::Danger))),
            }
            is_working.set(false);
        });
    };

    let problem_count = devices.read().iter().filter(|d| d.has_problem()).count();
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
                        if scan_cancel.read().is_some() {
                            button {
                                class: "btn btn-danger",
                                onclick: on_cancel_scan,
                                "{strings(en).cancel}"
                            }
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

                    InstallConfirm {
                        pending_install,
                        wua_updates,
                        backup_first,
                        on_confirm: on_confirm_install,
                    }

                    if *wua_policy_warn.read() {
                        StatusBanner { text: t.policy_managed.to_string(), tone: Tone::Warning }
                    }

                    if *reboot_alert.read() {
                        StatusBanner { text: t.reboot_needed.to_string(), tone: Tone::Warning }
                    }

                    if let Some((msg, tone)) = status_msg.read().clone() {
                        StatusBanner { text: msg.text(en), tone: tone }
                    }
                }
            }

            VendorUpdatesCard { vendor_updates, is_working, pending_install }

            HistoryCard { install_history, is_working, pending_revert }

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

            CatalogCard { catalog_matches, is_working, pending_install }

            WuaCard {
                wua_updates,
                selected_wua,
                is_working,
                on_install: on_install_wua,
            }

            DeviceListCard { devices, is_working, status_msg, catalog_matches }
        }
    }
}

/// SEC-37: diálogo de confirmación de la instalación pendiente, con la casilla del respaldo del Driver Store.
#[component]
fn InstallConfirm(
    mut pending_install: Signal<Option<PendingInstall>>,
    wua_updates: Signal<Vec<DriverUpdateCandidate>>,
    mut backup_first: Signal<bool>,
    on_confirm: EventHandler<()>,
) -> Element {
    let en = use_context::<Signal<Lang>>().read().is_en();
    let t = strings(en).drivers;
    let Some(pending) = pending_install.read().clone() else {
        return rsx! {};
    };
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
            on_confirm,
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
            CheckRow {
                checked: *backup_first.read(),
                on_change: move |on| backup_first.set(on),
                span { class: "text-sm", "{t.backup_first}" }
            }
        }
    }
}

/// Actualizaciones de fabricante (NVIDIA): una fila por paquete con su botón de instalar.
#[component]
fn VendorUpdatesCard(
    vendor_updates: Signal<Vec<VendorDriverUpdate>>,
    is_working: Signal<bool>,
    mut pending_install: Signal<Option<PendingInstall>>,
) -> Element {
    let t = strings(use_context::<Signal<Lang>>().read().is_en()).drivers;
    let list = vendor_updates.read();
    if list.is_empty() {
        return rsx! {};
    }
    rsx! {
        Card {
            title: t.vendor_title.to_string(),
            subtitle: Some(t.vendor_subtitle.to_string()),
            div { class: "stack-2",
                for v_upd in list.iter() {
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
}

/// SYS-26: drivers instalados desde OtterSwiss, cada uno con «Quitar».
#[component]
fn HistoryCard(
    install_history: Signal<Vec<InstalledDriverRecord>>,
    is_working: Signal<bool>,
    mut pending_revert: Signal<Option<InstalledDriverRecord>>,
) -> Element {
    let t = strings(use_context::<Signal<Lang>>().read().is_en()).drivers;
    let list = install_history.read();
    if list.is_empty() {
        return rsx! {};
    }
    rsx! {
        Card {
            title: t.history_title.to_string(),
            subtitle: Some(t.history_subtitle.to_string()),
            div { class: "stack-2",
                for rec in list.iter() {
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
}

/// Coincidencias del Catálogo de Microsoft para un dispositivo con problema.
#[component]
fn CatalogCard(
    catalog_matches: Signal<Vec<CatalogDriverMatch>>,
    is_working: Signal<bool>,
    mut pending_install: Signal<Option<PendingInstall>>,
) -> Element {
    let t = strings(use_context::<Signal<Lang>>().read().is_en()).drivers;
    let list = catalog_matches.read();
    if list.is_empty() {
        return rsx! {};
    }
    rsx! {
        Card {
            title: t.catalog_title.to_string(),
            subtitle: Some(t.catalog_subtitle.to_string()),
            div { class: "stack-2",
                for cat in list.iter() {
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
}

/// Actualizaciones de Windows Update con casilla; las seguras vienen marcadas.
#[component]
fn WuaCard(
    wua_updates: Signal<Vec<DriverUpdateCandidate>>,
    mut selected_wua: Signal<HashSet<String>>,
    is_working: Signal<bool>,
    on_install: EventHandler<()>,
) -> Element {
    let t = strings(use_context::<Signal<Lang>>().read().is_en()).drivers;
    let list = wua_updates.read();
    if list.is_empty() {
        return rsx! {};
    }
    let selected_count = selected_wua.read().len();
    rsx! {
        Card {
            title: t.wua_title.to_string(),
            subtitle: Some(t.wua_subtitle.to_string()),
            div { class: "stack-2",
                for upd in list.iter() {
                    {
                        let uid = upd.id.clone();
                        let checked = selected_wua.read().contains(&uid);
                        let size_txt = if upd.size_mb > 0 {
                            format!("{} MB", upd.size_mb)
                        } else {
                            "—".to_string()
                        };
                        rsx! {
                            CheckRow {
                                key: "{uid}",
                                class: "item-row gap-3",
                                checked,
                                on_change: move |on| {
                                    let mut s = selected_wua.write();
                                    if on {
                                        s.insert(uid.clone());
                                    } else {
                                        s.remove(&uid);
                                    }
                                },
                                div { class: "grow",
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
                                span { class: "badge badge-info", "{size_txt}" }
                            }
                        }
                    }
                }

                div { class: "row-end",
                    button {
                        class: "btn btn-primary",
                        disabled: *is_working.read() || selected_count == 0,
                        onclick: move |_| on_install.call(()),
                        "{(t.download_install_n)(selected_count)}"
                    }
                }
            }
        }
    }
}

/// ¿El dispositivo coincide con el texto buscado? Compara en minúsculas contra todos los campos visibles.
fn device_matches(d: &InstalledDriverDevice, q: &str) -> bool {
    let fields = [
        d.display_title().to_string(),
        d.manufacturer.clone().unwrap_or_default(),
        d.provider_name.clone().unwrap_or_default(),
        d.inf_path.clone().unwrap_or_default(),
        d.short_hw_id().unwrap_or_default(),
        d.decoded_vendor_name().unwrap_or("").to_string(),
        d.hardware_hint().unwrap_or("").to_string(),
    ];
    fields.iter().any(|f| f.to_ascii_lowercase().contains(q))
}

/// Busca en el Catálogo de Microsoft los drivers de un dispositivo con problema.
fn search_catalog(
    hw_id: String,
    mut is_working: Signal<bool>,
    mut status_msg: Signal<Option<(Msg, Tone)>>,
    mut catalog_matches: Signal<Vec<CatalogDriverMatch>>,
) {
    is_working.set(true);
    let shown = hw_id.clone();
    status_msg.set(Some((
        Msg::new(move |en| (strings(en).drivers.searching_catalog)(&shown)),
        Tone::Info,
    )));
    spawn(async move {
        let res = run_blocking(move || DriverManager::search_microsoft_catalog(&hw_id)).await;
        match res {
            Ok(list) => {
                let n = list.len();
                catalog_matches.set(list);
                status_msg.set(Some((
                    Msg::new(move |en| (strings(en).drivers.catalog_found)(n)),
                    Tone::Info,
                )));
            }
            Err(e) => status_msg.set(Some((e, Tone::Danger))),
        }
        is_working.set(false);
    });
}

/// Dispositivos presentes con búsqueda y filtros (todos, con error, de terceros).
#[component]
fn DeviceListCard(
    devices: Signal<Vec<InstalledDriverDevice>>,
    is_working: Signal<bool>,
    status_msg: Signal<Option<(Msg, Tone)>>,
    catalog_matches: Signal<Vec<CatalogDriverMatch>>,
) -> Element {
    let t = strings(use_context::<Signal<Lang>>().read().is_en()).drivers;

    let mut search_query = use_signal(String::new);
    let mut filter_mode = use_signal(|| DeviceFilter::All);

    let all = devices.read();
    let total_devs = all.len();
    let problem_count = all.iter().filter(|d| d.has_problem()).count();
    let oem_count = all.iter().filter(|d| d.is_third_party_oem()).count();

    let q = search_query.read().trim().to_ascii_lowercase();
    let cur_filter = *filter_mode.read();
    let filtered: Vec<InstalledDriverDevice> = all
        .iter()
        .filter(|d| match cur_filter {
            DeviceFilter::All => true,
            DeviceFilter::ErrorsOnly => d.has_problem(),
            DeviceFilter::ThirdPartyOem => d.is_third_party_oem(),
        })
        .filter(|d| q.is_empty() || device_matches(d, &q))
        .cloned()
        .collect();

    rsx! {
        Card {
            title: t.hardware_title.to_string(),
            subtitle: Some((t.showing_devices)(filtered.len())),
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

                if filtered.is_empty() {
                    if total_devs == 0 && *is_working.read() {
                        LoadingState { message: t.scanning_devices.to_string() }
                    } else if total_devs > 0 {
                        EmptyState {
                            title: t.no_devices.to_string(),
                            description: t.no_devices_hint.to_string(),
                        }
                    }
                }

                div { class: "modal-list tall",
                    for dev in filtered.into_iter() {
                        DeviceRow { key: "dev_{dev.dev_inst}_{dev.display_title()}", dev, is_working, status_msg, catalog_matches }
                    }
                }
            }
        }
    }
}

/// Una fila de dispositivo: nombre, insignias, detalle del driver y, si falla, la búsqueda en el Catálogo.
#[component]
fn DeviceRow(
    dev: InstalledDriverDevice,
    is_working: Signal<bool>,
    status_msg: Signal<Option<(Msg, Tone)>>,
    catalog_matches: Signal<Vec<CatalogDriverMatch>>,
) -> Element {
    let t = strings(use_context::<Signal<Lang>>().read().is_en()).drivers;
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

    let (status_badge_cls, status_badge_txt) = if let Some(code) = dev.problem {
        ("badge badge-danger", (t.cm_problem)(code))
    } else if dev.needs_reboot {
        ("badge badge-warning", t.pending_reboot)
    } else {
        ("badge badge-success", "OK")
    };
    let hw_for_btn = short_hw.clone();

    rsx! {
        div { class: "item-row row-between wrap",
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
                if dev.has_problem() && !hw_for_btn.is_empty() {
                    button {
                        class: "btn btn-secondary",
                        disabled: *is_working.read(),
                        onclick: move |_| search_catalog(hw_for_btn.clone(), is_working, status_msg, catalog_matches),
                        "{t.search_catalog}"
                    }
                }
                span { class: "{status_badge_cls}", "{status_badge_txt}" }
            }
        }
    }
}
