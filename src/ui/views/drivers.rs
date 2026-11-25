use std::collections::HashSet;

use dioxus::prelude::*;

use crate::core::drivers::{
    CatalogDriverMatch, DriverManager, DriverUpdateCandidate, InstalledDriverDevice,
    InstalledDriverRecord, VendorDriverUpdate, describe_cm_problem,
};
use crate::ui::components::{Card, ConfirmDialog, StatusBanner, Tone};
use crate::ui::i18n::Lang;

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
        DriverManager::export_driver_store_backup().map_err(|e| {
            if en {
                format!("Backup failed, nothing was installed: {e}")
            } else {
                format!("Falló el respaldo, no se instaló nada: {e}")
            }
        })?;
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
                Err(e) => status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger))),
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
                        if en {
                            "No pending driver updates found.".to_string()
                        } else {
                            "No se encontraron actualizaciones de drivers pendientes.".to_string()
                        }
                    } else if en {
                        format!("Scan complete: {} update(s) found.", total_found)
                    } else {
                        format!(
                            "Escaneo completo: {} actualización(es) encontrada(s).",
                            total_found
                        )
                    };
                    if warnings.is_empty() {
                        status_msg.set(Some((summary, Tone::Success)));
                    } else {
                        let lead = if en {
                            "Some sources could not be checked:"
                        } else {
                            "No se pudieron consultar algunas fuentes:"
                        };
                        status_msg.set(Some((
                            format!("{summary}\n{lead}\n• {}", warnings.join("\n• ")),
                            Tone::Warning,
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
        status_msg.set(Some((
            if en {
                "Installing drivers…".to_string()
            } else {
                "Instalando drivers…".to_string()
            },
            Tone::Info,
        )));
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
                Err(e) => status_msg.set(Some((format!("Error de hilo: {}", e), Tone::Danger))),
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
        "This PC is managed: a policy (driver exclusion or WSUS) decides driver updates, so OtterSwiss does not search for or install drivers.".to_string()
    } else {
        "Este equipo está gestionado: una política (exclusión de drivers o WSUS) decide las actualizaciones de drivers, así que OtterSwiss no las busca ni las instala.".to_string()
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
                                    title: if en { "Install drivers?".to_string() } else { "¿Instalar drivers?".to_string() },
                                    subtitle: Some(if en {
                                        "A bad driver can leave a device without working. Windows can only undo it from Device Manager or a restore point.".to_string()
                                    } else {
                                        "Un driver equivocado puede dejar un dispositivo sin funcionar. Solo se deshace desde el Administrador de dispositivos o un punto de restauración.".to_string()
                                    }),
                                    confirm_label: if en { "Install".to_string() } else { "Instalar".to_string() },
                                    cancel_label: if en { "Cancel".to_string() } else { "Cancelar".to_string() },
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
                                        StatusBanner {
                                            text: if en {
                                                "Includes firmware or BIOS: an interrupted update can leave the PC unable to boot. Keep it plugged in.".to_string()
                                            } else {
                                                "Incluye firmware o BIOS: si la actualización se interrumpe, el equipo puede no arrancar. Mantenlo enchufado.".to_string()
                                            },
                                            tone: Tone::Danger,
                                        }
                                    }
                                    label { class: "row gap-2 clickable",
                                        input {
                                            r#type: "checkbox",
                                            checked: *backup_first.read(),
                                            onchange: move |evt| backup_first.set(evt.checked()),
                                        }
                                        span { class: "text-sm",
                                            if en { "Back up installed drivers first (ProgramData\\OtterSwiss\\DriverBackup)" } else { "Respaldar antes los drivers instalados (ProgramData\\OtterSwiss\\DriverBackup)" }
                                        }
                                    }
                                }
                            }
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
                                                pending_install.set(Some(PendingInstall::Vendor(Box::new(upd_clone.clone()))));
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

            if !install_history.read().is_empty() {
                Card {
                    title: if en { "Drivers installed from OtterSwiss".to_string() } else { "Drivers instalados desde OtterSwiss".to_string() },
                    subtitle: Some(if en {
                        "Removing one sends its devices back to the best driver left on the PC.".to_string()
                    } else {
                        "Quitar uno devuelve sus dispositivos al mejor driver que quede en el equipo.".to_string()
                    }),
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
                                            if en { "Remove" } else { "Quitar" }
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
                    title: if en { "Remove this driver?".to_string() } else { "¿Quitar este driver?".to_string() },
                    subtitle: Some(format!("{} ({})", rec.title, rec.published_name)),
                    confirm_label: if en { "Remove".to_string() } else { "Quitar".to_string() },
                    cancel_label: if en { "Cancel".to_string() } else { "Cancelar".to_string() },
                    tone: Tone::Warning,
                    on_confirm: on_confirm_revert,
                    on_cancel: move |_| pending_revert.set(None),
                    p { class: "text-sm",
                        if en {
                            "Windows removes the package from the Driver Store and the devices that used it go back to the previous driver. It may ask for a restart."
                        } else {
                            "Windows quita el paquete del Driver Store y los dispositivos que lo usaban vuelven al driver anterior. Puede pedir reiniciar."
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
                                                pending_install.set(Some(PendingInstall::Catalog {
                                                    uid: uid.clone(),
                                                    title: title_c.clone(),
                                                }));
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
