//! Windows Update Agent por COM: búsqueda de drivers pendientes (con plazo, CON-03) e instalación de los elegidos.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::UpdateAgent::{
    IUpdate, IUpdate3, IUpdateCollection, IUpdateSession, IWindowsDriverUpdate, UpdateCollection,
    UpdateSession, orcFailed, orcSucceeded, orcSucceededWithErrors, ssDefault,
};
use windows::core::{BSTR, Interface};
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use super::{DriverInstallReport, DriverUpdateCandidate, ensure_driver_updates_allowed};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::ComGuard;

/// Plazo de una búsqueda de drivers en Windows Update (CON-03).
const WUA_SEARCH_TIMEOUT: Duration = Duration::from_secs(4 * 60);
/// Cada cuánto se mira si el usuario canceló la búsqueda.
const CANCEL_POLL: Duration = Duration::from_millis(250);

/// Drivers pendientes en Windows Update. `cancel` deja de esperar la búsqueda (`CoreError::Cancelled`).
pub(super) fn search_driver_updates(cancel: &AtomicBool) -> CoreResult<Vec<DriverUpdateCandidate>> {
    if is_windows_update_service_disabled() {
        return Err(CoreError::system_tool_msg(
            "El servicio Windows Update (wuauserv) está deshabilitado.",
        ));
    }

    if cancel.load(Ordering::Acquire) {
        return Err(CoreError::Cancelled);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(search_driver_updates_on_this_thread());
    });
    // CON-03: Windows Update puede quedarse colgado (servicio ocupado, red). Pasado el plazo, o si el usuario cancela,
    // se deja de esperar; la búsqueda solo lee, así que el hilo puede terminar solo después sin dejar nada a medias.
    let deadline = Instant::now() + WUA_SEARCH_TIMEOUT;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(CoreError::Cancelled);
        }
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
            Err(RecvTimeoutError::Timeout) => {
                return Err(CoreError::system_tool_msg(format!(
                    "Windows Update no respondió en {} minutos; prueba de nuevo más tarde.",
                    WUA_SEARCH_TIMEOUT.as_secs() / 60
                )));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(CoreError::system_tool_msg(
                    "Fallo inesperado en hilo COM de búsqueda de drivers",
                ));
            }
        }
    }
}

/// Búsqueda de drivers en WUA en el hilo actual (que no tenga COM iniciado en otro modo).
fn search_driver_updates_on_this_thread() -> CoreResult<Vec<DriverUpdateCandidate>> {
    let _com = ComGuard::init_sta()?;
    let (_session, updates) = wua_pending_drivers()?;
    // SAFETY: COM de WUA sobre una colección válida, en el hilo STA que la obtuvo.
    let count = unsafe { updates.Count() }.unwrap_or(0);
    Ok((0..count)
        .filter_map(|idx| {
            // SAFETY: igual que arriba; `idx` está dentro de `Count()`.
            let update = unsafe { updates.get_Item(idx) }.ok()?;
            Some(wua_candidate(&update, idx))
        })
        .collect())
}

pub(super) fn install_driver_updates_inner(
    selected_ids: &[String],
) -> CoreResult<DriverInstallReport> {
    ensure_driver_updates_allowed()?;
    if selected_ids.is_empty() {
        return Err(CoreError::Validation(
            "No hay actualizaciones seleccionadas para instalar".into(),
        ));
    }

    if is_windows_update_service_disabled() {
        return Err(CoreError::system_tool_msg(
            "El servicio Windows Update (wuauserv) está deshabilitado.",
        ));
    }

    let target_ids = selected_ids.to_vec();
    let handle = std::thread::spawn(move || wua_install_on_this_thread(&target_ids));
    handle.join().map_err(|_| {
        CoreError::system_tool_msg("Fallo inesperado en hilo COM de instalación de drivers")
    })?
}

fn is_windows_update_service_disabled() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) =
        hklm.open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\wuauserv", KEY_READ)
        && let Ok(start_type) = key.get_value::<u32, _>("Start")
    {
        return start_type == 4;
    }
    false
}

/// CL-20: el HRESULT original queda en `source`.
fn wua_error(what: &str, e: windows::core::Error) -> CoreError {
    CoreError::SystemTool {
        message: format!("{what}: {e}"),
        source: Some(Box::new(e)),
    }
}

/// Texto de una propiedad COM; vacío si la lectura falló (la vista muestra lo que haya).
fn bstr_text(r: windows::core::Result<BSTR>) -> String {
    r.map(|b| b.to_string()).unwrap_or_default()
}

/// Booleano de una propiedad COM; `default` si la lectura falló.
fn variant_flag(r: windows::core::Result<VARIANT_BOOL>, default: bool) -> bool {
    r.map(|b| b.0 != 0).unwrap_or(default)
}

/// Sesión de WUA y drivers pendientes, buscados en el servidor que configuró el administrador (SYS-25: WSUS incluido,
/// nunca Windows Update forzado). Debe llamarse en un hilo con `ComGuard::init_sta`.
fn wua_pending_drivers() -> CoreResult<(IUpdateSession, IUpdateCollection)> {
    // SAFETY: llamadas COM de WUA sobre interfaces que se acaban de crear, en el hilo STA del llamador; el criterio y
    // los VARIANT_BOOL son valores propios.
    unsafe {
        let session: IUpdateSession = CoCreateInstance(&UpdateSession, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| wua_error("No se pudo iniciar Windows Update Agent (WUA)", e))?;
        let searcher = session
            .CreateUpdateSearcher()
            .map_err(|e| wua_error("CreateUpdateSearcher falló", e))?;
        let _ = searcher.SetOnline(VARIANT_BOOL(-1));
        let _ = searcher.SetServerSelection(ssDefault);
        let criteria = BSTR::from("IsInstalled=0 and Type='Driver' and IsHidden=0");
        let result = searcher
            .Search(&criteria)
            .map_err(|e| wua_error("WUA Search falló", e))?;
        let updates = result
            .Updates()
            .map_err(|e| wua_error("Updates() falló", e))?;
        Ok((session, updates))
    }
}

/// `UpdateID` (GUID) de una actualización de WUA; es la clave con la que la vista elige qué instalar.
fn wua_update_id(update: &IUpdate) -> Option<String> {
    // SAFETY: lecturas COM sobre una actualización válida, en el hilo STA que la obtuvo.
    unsafe { update.Identity().and_then(|id| id.UpdateID()) }
        .ok()
        .map(|b| b.to_string())
}

/// Lo que la vista muestra de un driver pendiente de WUA.
fn wua_candidate(update: &IUpdate, idx: i32) -> DriverUpdateCandidate {
    // SAFETY: lecturas de propiedades COM sobre una actualización válida, en el hilo STA que la obtuvo.
    let (title, is_optional, size_mb, driver) = unsafe {
        (
            bstr_text(update.Title()),
            variant_flag(
                update.cast::<IUpdate3>().and_then(|u3| u3.BrowseOnly()),
                false,
            ),
            update.RecommendedHardDiskSpace().unwrap_or(0),
            update
                .cast::<IWindowsDriverUpdate>()
                .map(|drv| {
                    [
                        bstr_text(drv.DriverClass()),
                        bstr_text(drv.DriverProvider()),
                        bstr_text(drv.DriverHardwareID()),
                    ]
                })
                .unwrap_or_default(),
        )
    };
    let [driver_class, driver_provider, hardware_id] = driver;
    let low_title = title.to_ascii_lowercase();
    let is_firmware = driver_class.to_ascii_lowercase().contains("firmware")
        || low_title.contains("firmware")
        || low_title.contains("uefi")
        || low_title.contains("bios");
    DriverUpdateCandidate {
        id: wua_update_id(update).unwrap_or_else(|| format!("drv_{idx}")),
        title,
        driver_class,
        driver_provider,
        hardware_id,
        size_mb,
        is_firmware,
        is_optional,
    }
}

/// Colección vacía de WUA para armar la lista de descarga o de instalación. COM debe estar iniciado en este hilo.
fn wua_new_collection() -> CoreResult<IUpdateCollection> {
    // SAFETY: crea un objeto COM en proceso; COM ya está iniciado en este hilo.
    unsafe { CoCreateInstance(&UpdateCollection, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| wua_error("CoCreateInstance(UpdateCollection) falló", e))
}

/// Descarga e instala por WUA los drivers elegidos, en el hilo actual con COM en STA.
fn wua_install_on_this_thread(target_ids: &[String]) -> CoreResult<DriverInstallReport> {
    let _com = ComGuard::init_sta()?;
    let (session, found_updates) = wua_pending_drivers()?;
    let to_download = wua_new_collection()?;
    let to_install = wua_new_collection()?;

    // SAFETY: COM de WUA sobre colecciones válidas, en el hilo STA que las creó.
    let count = unsafe { found_updates.Count() }.unwrap_or(0);
    let mut matched = 0usize;
    for idx in 0..count {
        // SAFETY: igual que arriba; `idx` está dentro de `Count()`.
        let Ok(update) = (unsafe { found_updates.get_Item(idx) }) else {
            continue;
        };
        if !wua_update_id(&update).is_some_and(|uid| target_ids.contains(&uid)) {
            continue;
        }
        // SAFETY: métodos COM sobre una actualización y colecciones válidas, en el hilo STA que las obtuvo.
        let added = unsafe {
            if !variant_flag(update.EulaAccepted(), true) {
                let _ = update.AcceptEula();
            }
            if !variant_flag(update.IsDownloaded(), false) {
                let _ = to_download.Add(&update);
            }
            to_install.Add(&update).is_ok()
        };
        // ERR-18: solo cuenta lo que de verdad entró en la colección a instalar.
        if added {
            matched += 1;
        }
    }
    if matched == 0 {
        return Err(CoreError::system_tool_msg(
            "Ninguno de los drivers seleccionados sigue pendiente en Windows Update.",
        ));
    }

    // SAFETY: COM de WUA sobre la sesión y la colección válidas, en este hilo STA.
    unsafe {
        if to_download.Count().unwrap_or(0) > 0 {
            let downloader = session
                .CreateUpdateDownloader()
                .map_err(|e| wua_error("CreateUpdateDownloader falló", e))?;
            downloader
                .SetUpdates(&to_download)
                .map_err(|e| wua_error("Downloader.SetUpdates falló", e))?;
            let dl_res = downloader
                .Download()
                .map_err(|e| wua_error("Error descargando drivers desde Microsoft", e))?;
            let dl_code = dl_res.ResultCode().unwrap_or(orcFailed);
            if dl_code != orcSucceeded && dl_code != orcSucceededWithErrors {
                return Err(CoreError::system_tool_msg(format!(
                    "La descarga oficial falló con estado {:?}",
                    dl_code.0
                )));
            }
        }
    }

    // SAFETY: COM de WUA sobre la sesión y la colección válidas, en este hilo STA.
    let (reboot_required, results) = unsafe {
        let installer = session
            .CreateUpdateInstaller()
            .map_err(|e| wua_error("CreateUpdateInstaller falló", e))?;
        installer
            .SetUpdates(&to_install)
            .map_err(|e| wua_error("Installer.SetUpdates falló", e))?;
        let inst_res = installer
            .Install()
            .map_err(|e| wua_error("Error instalando drivers seleccionados", e))?;
        // ERR-18: el resultado de cada driver, no una estimación desde el código global. Si no se puede leer el
        // resultado de uno, cuenta como fallo.
        let results: Vec<bool> = (0..to_install.Count().unwrap_or(0))
            .map(|i| {
                inst_res
                    .GetUpdateResult(i)
                    .and_then(|r| r.ResultCode())
                    .is_ok_and(|code| code == orcSucceeded || code == orcSucceededWithErrors)
            })
            .collect();
        (variant_flag(inst_res.RebootRequired(), false), results)
    };

    let succeeded_count = results.iter().filter(|ok| **ok).count();
    let failed_count = results.len() - succeeded_count;
    Ok(DriverInstallReport {
        succeeded_count,
        failed_count,
        reboot_required,
        history_warning: None,
    })
}
