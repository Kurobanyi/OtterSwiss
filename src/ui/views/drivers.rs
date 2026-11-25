use std::collections::HashSet;

use dioxus::prelude::*;

use crate::core::drivers::{
    CatalogDriverMatch, DriverManager, DriverUpdateCandidate, InstalledDriverDevice,
    VendorDriverUpdate, describe_cm_problem,
};
use crate::ui::components::{Card, StatusBanner, Tone};
use crate::ui::i18n::Lang;

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
    is_working.set(true);
    status_msg.set(Some((
        if en {
            "Scanning PnP hardware & driver metadata...".to_string()
        } else {
            "Escaneando hardware PnP y metadatos de drivers...".to_string()
        },
        Tone::Info,
    )));

    spawn(async move {
        let res = tokio::task::spawn_blocking(DriverManager::list_present_devices).await;
        match res {
            Ok(Ok(list)) => {
                let count = list.len();
                devices.set(list);
                status_msg.set(Some((
                    if en {
                        format!("Detected {} active hardware devices.", count)
                    } else {
                        format!("{} dispositivos de hardware activos detectados.", count)
                    },
                    Tone::Info,
                )));
            }
            Ok(Err(e)) => {
                status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
            }
            Err(e) => {
                status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger)));
            }
        }
        is_working.set(false);
    });
}

#[component]
pub fn DriversView() -> Element {
    let lang = use_context::<Signal<Lang>>();
    let en = lang.read().is_en();

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

    use_hook(move || {
        trigger_device_scan(en, is_working, status_msg, devices);
    });

    let on_refresh_devices = move |_| {
        trigger_device_scan(en, is_working, status_msg, devices);
    };

    let on_full_scan = move |_| {
        let current_devs = devices.read().clone();
        is_working.set(true);
        status_msg.set(Some((
            if en {
                "Scanning Windows Update, NVIDIA/Intel Official Feeds & Microsoft Catalog..."
                    .to_string()
            } else {
                "Escaneando Windows Update, NVIDIA/Intel Oficial y Catálogo de Microsoft..."
                    .to_string()
            },
            Tone::Info,
        )));

        spawn(async move {
            let res =
                tokio::task::spawn_blocking(move || DriverManager::run_full_scan(&current_devs))
                    .await;

            match res {
                Ok(report) => {
                    let safe_wua_ids: HashSet<String> = report
                        .wua_updates
                        .iter()
                        .filter(|u| !u.is_firmware)
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

                    if total_found == 0 {
                        if let Some(note) = report.wua_note {
                            status_msg.set(Some((note, Tone::Warning)));
                        } else {
                            status_msg.set(Some((
                                if en {
                                    "No pending updates found across WUA, Vendors, or MS Catalog."
                                        .to_string()
                                } else {
                                    "No se encontraron actualizaciones pendientes en WUA, Fabricantes ni Catálogo MS."
                                        .to_string()
                                },
                                Tone::Success,
                            )));
                        }
                    } else {
                        status_msg.set(Some((
                            if en {
                                format!("Full scan complete: {} candidate(s) found.", total_found)
                            } else {
                                format!(
                                    "Escaneo completo: {} paquete(s) encontrado(s).",
                                    total_found
                                )
                            },
                            Tone::Success,
                        )));
                    }
                }
                Err(e) => {
                    status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger)));
                }
            }
            is_working.set(false);
        });
    };

    let on_install_wua = move |_| {
        let ids: Vec<String> = selected_wua.read().iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        is_working.set(true);
        status_msg.set(Some((
            if en {
                format!(
                    "Installing {} WHQL driver(s) via Windows Update...",
                    ids.len()
                )
            } else {
                format!(
                    "Instalando {} driver(s) WHQL vía Windows Update...",
                    ids.len()
                )
            },
            Tone::Info,
        )));

        spawn(async move {
            let res =
                tokio::task::spawn_blocking(move || DriverManager::install_driver_updates(&ids))
                    .await;
            match res {
                Ok(Ok(rep)) => {
                    if rep.reboot_required {
                        reboot_alert.set(true);
                    }
                    let tone = if rep.failed_count > 0 {
                        Tone::Warning
                    } else {
                        Tone::Success
                    };
                    status_msg.set(Some((rep.message, tone)));
                    if let Ok(Ok(refreshed)) =
                        tokio::task::spawn_blocking(DriverManager::list_present_devices).await
                    {
                        devices.set(refreshed);
                    }
                    wua_updates.write().clear();
                    selected_wua.write().clear();
                }
                Ok(Err(e)) => {
                    status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
                }
                Err(e) => {
                    status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger)));
                }
            }
            is_working.set(false);
        });
    };

    let on_backup_oem = move |_| {
        is_working.set(true);
        status_msg.set(Some((
            if en {
                "Exporting installed OEM drivers to ProgramData\\OtterSwiss\\DriverBackup..."
                    .to_string()
            } else {
                "Exportando drivers OEM instalados a ProgramData\\OtterSwiss\\DriverBackup..."
                    .to_string()
            },
            Tone::Info,
        )));

        spawn(async move {
            let res = tokio::task::spawn_blocking(DriverManager::export_driver_store_backup).await;
            match res {
                Ok(Ok(path)) => {
                    status_msg.set(Some((
                        if en {
                            format!("OEM Driver Store backup saved: {}", path.display())
                        } else {
                            format!("Respaldo de Driver Store guardado en: {}", path.display())
                        },
                        Tone::Success,
                    )));
                }
                Ok(Err(e)) => {
                    status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
                }
                Err(e) => {
                    status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger)));
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

    let main_title = if en {
        "Native Driver Manager & Updater"
    } else {
        "Gestor y Actualizador Nativo de Drivers"
    }
    .to_string();
    let main_sub = Some(
        if en {
            "Windows Update + Direct NVIDIA/Intel Feeds + Microsoft Update Catalog (.CAB)"
        } else {
            "Windows Update + Feeds Oficiales NVIDIA/Intel + Catálogo Oficial de Microsoft (.CAB)"
        }
        .to_string(),
    );
    let btn_full_scan = if en {
        "Full Driver Scan"
    } else {
        "Escaneo Completo de Drivers"
    };
    let btn_refresh_lbl = if en {
        "Refresh Hardware"
    } else {
        "Refrescar Hardware"
    };
    let btn_backup_lbl = if en {
        "Backup OEM Drivers"
    } else {
        "Respaldar Drivers OEM"
    };

    let problems_badge_txt = if problem_count > 0 {
        if en {
            format!("{} device error(s)", problem_count)
        } else {
            format!("{} dispositivo(s) con error", problem_count)
        }
    } else if en {
        "All hardware OK".to_string()
    } else {
        "Hardware sin errores".to_string()
    };

    let policy_warn_txt = if en {
        "Notice: Windows policy 'ExcludeWUDriversInQualityUpdate' is blocking driver updates in Windows Update.".to_string()
    } else {
        "Aviso: La política de Windows 'ExcludeWUDriversInQualityUpdate' está bloqueando drivers en Windows Update.".to_string()
    };

    let reboot_banner_txt = if en {
        "Windows requires a system restart to complete driver installation.".to_string()
    } else {
        "Windows requiere reiniciar el equipo para completar la instalación de los drivers."
            .to_string()
    };

    let vendor_card_title = if en {
        "Direct Vendor Updates (NVIDIA / Intel)"
    } else {
        "Actualizaciones Directas de Fabricantes (NVIDIA / Intel)"
    }
    .to_string();
    let vendor_card_sub = Some(
        if en {
            "Official signed installers matched by PCI/USB Hardware ID"
        } else {
            "Instaladores oficiales firmados emparejados por Hardware ID"
        }
        .to_string(),
    );
    let vendor_install_lbl = if en {
        "Download & Install"
    } else {
        "Descargar e Instalar"
    };

    let wua_card_title = if en {
        "Windows Update WHQL Drivers"
    } else {
        "Drivers WHQL de Windows Update"
    }
    .to_string();
    let wua_card_sub = Some(
        if en {
            "Verified packages from Windows Update Agent"
        } else {
            "Paquetes verificados mediante Windows Update Agent"
        }
        .to_string(),
    );
    let btn_install_wua = if en {
        format!("Download & Install ({})", selected_wua_count)
    } else {
        format!("Descargar e Instalar ({})", selected_wua_count)
    };
    let firmware_warn_lbl = "FIRMWARE / BIOS";

    let catalog_card_title = if en {
        "Microsoft Update Catalog (.CAB Solutions)"
    } else {
        "Catálogo Oficial de Microsoft (.CAB)"
    }
    .to_string();
    let catalog_card_sub = Some(
        if en {
            "Official Microsoft WHQL .cab packages for missing drivers"
        } else {
            "Paquetes .CAB firmados por Microsoft para dispositivos sin driver"
        }
        .to_string(),
    );
    let catalog_install_lbl = if en { "Install .CAB" } else { "Instalar .CAB" };

    let hw_card_title = if en {
        "Installed PnP Hardware & Drivers"
    } else {
        "Hardware PnP y Controladores Instalados"
    }
    .to_string();
    let hw_card_sub = Some(if en {
        format!("Showing {} device(s)", filtered_devs.len())
    } else {
        format!("Mostrando {} dispositivo(s)", filtered_devs.len())
    });
    let search_placeholder = if en {
        "Filter by device, vendor, Hardware ID or .inf..."
    } else {
        "Filtrar por dispositivo, fabricante, Hardware ID o .inf..."
    };

    let filter_all_lbl = if en {
        format!("All ({})", total_devs)
    } else {
        format!("Todos ({})", total_devs)
    };
    let filter_err_lbl = if en {
        format!("Errors ({})", problem_count)
    } else {
        format!("Con Errores ({})", problem_count)
    };
    let filter_oem_lbl = if en {
        format!("Third-Party OEM ({})", oem_count)
    } else {
        format!("Terceros OEM ({})", oem_count)
    };
    let find_catalog_btn = if en {
        "Search MS Catalog"
    } else {
        "Buscar en Catálogo"
    };

    let no_updates_lbl = if en {
        "No pending updates found across WUA, Vendors, or MS Catalog.".to_string()
    } else {
        "No se encontraron actualizaciones pendientes en WUA, Fabricantes ni Catálogo MS."
            .to_string()
    };

    rsx! {
        div { class: "content-body",
            Card {
                title: main_title,
                subtitle: main_sub,
                div { style: "display: flex; flex-direction: column; gap: var(--space-3);",
                    div { style: "display: flex; gap: var(--space-2); align-items: center; flex-wrap: wrap;",
                        button {
                            class: "btn btn-primary",
                            disabled: *is_working.read(),
                            onclick: on_full_scan,
                            "{btn_full_scan}"
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_working.read(),
                            onclick: on_refresh_devices,
                            "{btn_refresh_lbl}"
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: *is_working.read(),
                            onclick: on_backup_oem,
                            "{btn_backup_lbl}"
                        }
                        span {
                            class: if problem_count > 0 { "badge badge-danger" } else { "badge badge-success" },
                            "{problems_badge_txt}"
                        }
                    }

                    if *wua_policy_warn.read() {
                        StatusBanner { text: policy_warn_txt, tone: Tone::Warning }
                    }

                    if *reboot_alert.read() {
                        StatusBanner { text: reboot_banner_txt, tone: Tone::Warning }
                    }

                    if let Some((msg, tone)) = status_msg.read().clone() {
                        StatusBanner { text: msg, tone: tone }
                    }
                }
            }

            if !vendor_guard.is_empty() {
                Card {
                    title: vendor_card_title,
                    subtitle: vendor_card_sub,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        for v_upd in vendor_guard.iter() {
                            {
                                let upd_clone = v_upd.clone();
                                rsx! {
                                    div {
                                        key: "{v_upd.id}",
                                        style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg); flex-wrap: wrap;",
                                        div { style: "min-width: 240px; flex: 1;",
                                            div { style: "display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap;",
                                                span { class: "badge badge-info", "{v_upd.vendor}" }
                                                span { class: "text-sm", style: "font-weight: 600;", "{v_upd.title}" }
                                            }
                                            div { class: "text-xs text-muted", style: "margin-top: var(--space-1);",
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
                                                let item = upd_clone.clone();
                                                is_working.set(true);
                                                status_msg.set(Some((
                                                    if en {
                                                        format!("Downloading & verifying '{}'...", item.title)
                                                    } else {
                                                        format!("Descargando y verificando '{}'...", item.title)
                                                    },
                                                    Tone::Info,
                                                )));
                                                spawn(async move {
                                                    let res = tokio::task::spawn_blocking(move || {
                                                        DriverManager::install_vendor_update(&item)
                                                    })
                                                    .await;
                                                    match res {
                                                        Ok(Ok(m)) => status_msg.set(Some((m, Tone::Success))),
                                                        Ok(Err(e)) => {
                                                            status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
                                                        }
                                                        Err(e) => {
                                                            status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger)));
                                                        }
                                                    }
                                                    is_working.set(false);
                                                });
                                            },
                                            "{vendor_install_lbl}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !catalog_guard.is_empty() {
                Card {
                    title: catalog_card_title,
                    subtitle: catalog_card_sub,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                        for cat in catalog_guard.iter() {
                            {
                                let uid = cat.update_id.clone();
                                let title_c = cat.title.clone();
                                rsx! {
                                    div {
                                        key: "{uid}",
                                        style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg); flex-wrap: wrap;",
                                        div { style: "min-width: 240px; flex: 1;",
                                            div { class: "text-sm", style: "font-weight: 600;", "{cat.title}" }
                                            div { class: "text-xs text-muted", style: "margin-top: var(--space-1);",
                                                "{cat.query_hw_id} • v{cat.version} • {cat.last_updated} • {cat.size_str}"
                                            }
                                        }
                                        button {
                                            class: "btn btn-primary",
                                            disabled: *is_working.read(),
                                            onclick: move |_| {
                                                let target_uid = uid.clone();
                                                let t_name = title_c.clone();
                                                is_working.set(true);
                                                status_msg.set(Some((
                                                    if en {
                                                        format!("Downloading & installing .CAB for '{}'...", t_name)
                                                    } else {
                                                        format!("Descargando e instalando .CAB para '{}'...", t_name)
                                                    },
                                                    Tone::Info,
                                                )));
                                                spawn(async move {
                                                    let res = tokio::task::spawn_blocking(move || {
                                                        DriverManager::install_catalog_driver(&target_uid)
                                                    })
                                                    .await;
                                                    match res {
                                                        Ok(Ok(msg)) => {
                                                            if msg.contains("Reinicio") || msg.contains("Reboot") {
                                                                reboot_alert.set(true);
                                                            }
                                                            status_msg.set(Some((msg, Tone::Success)));
                                                            if let Ok(Ok(refreshed)) = tokio::task::spawn_blocking(
                                                                DriverManager::list_present_devices,
                                                            )
                                                            .await
                                                            {
                                                                devices.set(refreshed);
                                                            }
                                                        }
                                                        Ok(Err(e)) => {
                                                            status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
                                                        }
                                                        Err(e) => {
                                                            status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger)));
                                                        }
                                                    }
                                                    is_working.set(false);
                                                });
                                            },
                                            "{catalog_install_lbl}"
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
                    title: wua_card_title,
                    subtitle: wua_card_sub,
                    div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
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
                                        style: "display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg); cursor: pointer;",
                                        div { style: "display: flex; align-items: center; gap: var(--space-2); min-width: 0; flex: 1;",
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
                                            div { style: "min-width: 0;",
                                                div { style: "display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap;",
                                                    span { class: "text-sm", style: "font-weight: 600;", "{upd.title}" }
                                                    if upd.is_firmware {
                                                        span { class: "badge badge-danger", "{firmware_warn_lbl}" }
                                                    }
                                                }
                                                div { class: "text-xs text-muted", style: "margin-top: var(--space-1);",
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

                        div { style: "display: flex; justify-content: flex-end; margin-top: var(--space-2);",
                            button {
                                class: "btn btn-primary",
                                disabled: *is_working.read() || selected_wua_count == 0,
                                onclick: on_install_wua,
                                "{btn_install_wua}"
                            }
                        }
                    }
                }
            } else if *has_checked_updates.read() && vendor_guard.is_empty() && catalog_guard.is_empty() {
                StatusBanner { text: no_updates_lbl, tone: Tone::Success }
            }

            Card {
                title: hw_card_title,
                subtitle: hw_card_sub,
                div { style: "display: flex; flex-direction: column; gap: var(--space-3);",
                    div { style: "display: flex; gap: var(--space-2); align-items: center; flex-wrap: wrap;",
                        input {
                            r#type: "text",
                            placeholder: "{search_placeholder}",
                            value: "{search_query.read()}",
                            style: "flex: 1; min-width: 240px; padding: var(--space-2) var(--space-3); border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg); color: var(--text);",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                        button {
                            class: if cur_filter == DeviceFilter::All { "btn btn-primary" } else { "btn btn-secondary" },
                            onclick: move |_| filter_mode.set(DeviceFilter::All),
                            "{filter_all_lbl}"
                        }
                        button {
                            class: if cur_filter == DeviceFilter::ErrorsOnly { "btn btn-primary" } else { "btn btn-secondary" },
                            onclick: move |_| filter_mode.set(DeviceFilter::ErrorsOnly),
                            "{filter_err_lbl}"
                        }
                        button {
                            class: if cur_filter == DeviceFilter::ThirdPartyOem { "btn btn-primary" } else { "btn btn-secondary" },
                            onclick: move |_| filter_mode.set(DeviceFilter::ThirdPartyOem),
                            "{filter_oem_lbl}"
                        }
                    }

                    div { class: "modal-list", style: "max-height: 460px;",
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
                                    describe_cm_problem(dev.problem_code, en).to_string()
                                } else if dev.needs_reboot {
                                    if en { "Pending Reboot".to_string() } else { "Reinicio pendiente".to_string() }
                                } else {
                                    "OK".to_string()
                                };

                                let key_str = format!("dev_{}_{}", dev.dev_inst, title_str);
                                let hw_for_btn = short_hw.clone();

                                rsx! {
                                    div {
                                        key: "{key_str}",
                                        style: "padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg); display: flex; justify-content: space-between; align-items: center; gap: var(--space-2); flex-wrap: wrap;",
                                        div { style: "min-width: 240px; flex: 1;",
                                            div { style: "display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap;",
                                                span { class: "text-sm", style: "font-weight: 600;", "{title_str}" }
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
                                            div { class: "text-xs text-muted", style: "margin-top: var(--space-1);",
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
                                        div { style: "display: flex; gap: var(--space-2); align-items: center;",
                                            if dev.has_problem && !hw_for_btn.is_empty() {
                                                button {
                                                    class: "btn btn-secondary",
                                                    disabled: *is_working.read(),
                                                    onclick: move |_| {
                                                        let q_id = hw_for_btn.clone();
                                                        is_working.set(true);
                                                        status_msg.set(Some((
                                                            if en {
                                                                format!("Searching Microsoft Catalog for {}...", q_id)
                                                            } else {
                                                                format!("Buscando en Catálogo MS para {}...", q_id)
                                                            },
                                                            Tone::Info,
                                                        )));
                                                        spawn(async move {
                                                            let res = tokio::task::spawn_blocking(move || {
                                                                DriverManager::search_microsoft_catalog(&q_id)
                                                            })
                                                            .await;
                                                            match res {
                                                                Ok(Ok(list)) => {
                                                                    let n = list.len();
                                                                    catalog_matches.set(list);
                                                                    status_msg.set(Some((
                                                                        if en {
                                                                            format!("Found {} package(s) in MS Catalog.", n)
                                                                        } else {
                                                                            format!("{} paquete(s) encontrado(s) en Catálogo MS.", n)
                                                                        },
                                                                        Tone::Info,
                                                                    )));
                                                                }
                                                                Ok(Err(e)) => {
                                                                    status_msg.set(Some((format!("Error: {}", e), Tone::Danger)));
                                                                }
                                                                Err(e) => {
                                                                    status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger)));
                                                                }
                                                            }
                                                            is_working.set(false);
                                                        });
                                                    },
                                                    "{find_catalog_btn}"
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
