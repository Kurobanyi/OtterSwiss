//! IN-08: textos de la pestaña Drivers. Los que llevan datos son `fn` para que ES y EN armen la frase a su modo; el
//! compilador sigue exigiendo los dos idiomas.

use super::strings::plural;

pub struct DriversStrings {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub full_scan: &'static str,
    pub refresh_hardware: &'static str,
    pub backup_oem: &'static str,
    pub scanning_devices: &'static str,
    pub devices_detected: fn(usize) -> String,
    pub scanning_sources: &'static str,
    pub no_updates: &'static str,
    pub updates_found: fn(usize) -> String,
    pub sources_failed: &'static str,
    pub installing: &'static str,
    pub exporting_backup: &'static str,
    pub backup_saved: fn(&str) -> String,
    pub backup_failed: fn(&str) -> String,
    pub internal_error: &'static str,
    pub device_errors: fn(usize) -> String,
    pub all_ok: &'static str,
    pub policy_managed: &'static str,
    pub reboot_needed: &'static str,
    pub confirm_title: &'static str,
    pub confirm_subtitle: &'static str,
    pub install: &'static str,
    pub firmware_warning: &'static str,
    pub backup_first: &'static str,
    pub vendor_title: &'static str,
    pub vendor_subtitle: &'static str,
    pub download_install: &'static str,
    pub history_title: &'static str,
    pub history_subtitle: &'static str,
    pub remove: &'static str,
    pub remove_title: &'static str,
    pub remove_body: &'static str,
    pub catalog_title: &'static str,
    pub catalog_subtitle: &'static str,
    pub install_cab: &'static str,
    pub wua_title: &'static str,
    pub wua_subtitle: &'static str,
    pub download_install_n: fn(usize) -> String,
    pub hardware_title: &'static str,
    pub showing_devices: fn(usize) -> String,
    pub search_placeholder: &'static str,
    pub filter_all: fn(usize) -> String,
    pub filter_errors: fn(usize) -> String,
    pub filter_oem: fn(usize) -> String,
    pub search_catalog: &'static str,
    pub searching_catalog: fn(&str) -> String,
    pub catalog_found: fn(usize) -> String,
    pub pending_reboot: &'static str,
    /// Texto del código de problema del Administrador de dispositivos (`CM_PROB_*`).
    pub cm_problem: fn(u32) -> &'static str,
}

pub static ES: DriversStrings = DriversStrings {
    title: "Gestor y actualizador de drivers",
    subtitle: "Windows Update, NVIDIA e Intel oficiales y el Catálogo de Microsoft (.cab)",
    full_scan: "Buscar actualizaciones",
    refresh_hardware: "Releer hardware",
    backup_oem: "Respaldar drivers OEM",
    scanning_devices: "Leyendo el hardware y sus drivers…",
    devices_detected: |n| {
        format!(
            "{}.",
            plural(n, ("dispositivo detectado", "dispositivos detectados"))
        )
    },
    scanning_sources: "Buscando en Windows Update, NVIDIA e Intel y el Catálogo de Microsoft…",
    no_updates: "No hay actualizaciones de drivers pendientes.",
    updates_found: |n| {
        format!(
            "Búsqueda completa: {}.",
            plural(
                n,
                ("actualización encontrada", "actualizaciones encontradas")
            )
        )
    },
    sources_failed: "No se pudieron consultar algunas fuentes:",
    installing: "Instalando drivers…",
    exporting_backup: "Exportando los drivers OEM a ProgramData\\OtterSwiss\\DriverBackup…",
    backup_saved: |path| format!("Respaldo del Driver Store guardado en {path}"),
    backup_failed: |e| format!("Falló el respaldo, no se instaló nada: {e}"),
    internal_error: "Error interno",
    device_errors: |n| format!("{} con error", plural(n, ("dispositivo", "dispositivos"))),
    all_ok: "Hardware sin errores",
    policy_managed: "Este equipo está gestionado: una política (exclusión de drivers o WSUS) decide las actualizaciones de drivers, así que OtterSwiss no las busca ni las instala.",
    reboot_needed: "Windows requiere reiniciar el equipo para terminar de instalar los drivers.",
    confirm_title: "¿Instalar drivers?",
    confirm_subtitle: "Un driver equivocado puede dejar un dispositivo sin funcionar. Solo se deshace desde el Administrador de dispositivos o un punto de restauración.",
    install: "Instalar",
    firmware_warning: "Incluye firmware o BIOS: si la actualización se interrumpe, el equipo puede no arrancar. Mantenlo enchufado.",
    backup_first: "Respaldar antes los drivers instalados (ProgramData\\OtterSwiss\\DriverBackup)",
    vendor_title: "Actualizaciones de fabricantes (NVIDIA e Intel)",
    vendor_subtitle: "Instaladores oficiales firmados, elegidos por Hardware ID",
    download_install: "Descargar e instalar",
    history_title: "Drivers instalados desde OtterSwiss",
    history_subtitle: "Quitar uno devuelve sus dispositivos al mejor driver que quede en el equipo.",
    remove: "Quitar",
    remove_title: "¿Quitar este driver?",
    remove_body: "Windows quita el paquete del Driver Store y los dispositivos que lo usaban vuelven al driver anterior. Puede pedir reiniciar.",
    catalog_title: "Catálogo de Microsoft (.cab)",
    catalog_subtitle: "Paquetes .cab firmados por Microsoft para dispositivos sin driver",
    install_cab: "Instalar .cab",
    wua_title: "Drivers WHQL de Windows Update",
    wua_subtitle: "Paquetes verificados por Windows Update Agent",
    download_install_n: |n| format!("Descargar e instalar ({n})"),
    hardware_title: "Hardware y drivers instalados",
    showing_devices: |n| format!("Se muestran {}", plural(n, ("dispositivo", "dispositivos"))),
    search_placeholder: "Filtrar por dispositivo, fabricante, Hardware ID o .inf…",
    filter_all: |n| format!("Todos ({n})"),
    filter_errors: |n| format!("Con errores ({n})"),
    filter_oem: |n| format!("Terceros OEM ({n})"),
    search_catalog: "Buscar en el Catálogo",
    searching_catalog: |id| format!("Buscando {id} en el Catálogo de Microsoft…"),
    catalog_found: |n| {
        format!(
            "{} en el Catálogo de Microsoft.",
            plural(n, ("paquete encontrado", "paquetes encontrados"))
        )
    },
    pending_reboot: "Reinicio pendiente",
    cm_problem: |code| match code {
        1 => "Dispositivo mal configurado (código 1)",
        10 => "El dispositivo no puede iniciarse (código 10)",
        12 => "Recursos libres insuficientes (código 12)",
        18 => "Reinstalar los drivers (código 18)",
        22 => "Dispositivo deshabilitado (código 22)",
        28 => "Faltan los drivers (código 28)",
        31 => "El dispositivo no funciona correctamente (código 31)",
        43 => "Windows detuvo el dispositivo por errores (código 43)",
        45 => "Dispositivo no conectado (código 45)",
        52 => "Firma digital del driver sin verificar (código 52)",
        _ => "Problema del dispositivo o su driver",
    },
};

pub static EN: DriversStrings = DriversStrings {
    title: "Driver manager and updater",
    subtitle: "Windows Update, official NVIDIA and Intel feeds and the Microsoft Update Catalog (.cab)",
    full_scan: "Check for updates",
    refresh_hardware: "Reload hardware",
    backup_oem: "Back up OEM drivers",
    scanning_devices: "Reading hardware and drivers…",
    devices_detected: |n| format!("{} detected.", plural(n, ("device", "devices"))),
    scanning_sources: "Checking Windows Update, NVIDIA and Intel, and the Microsoft Update Catalog…",
    no_updates: "No pending driver updates.",
    updates_found: |n| format!("Scan complete: {} found.", plural(n, ("update", "updates"))),
    sources_failed: "Some sources could not be checked:",
    installing: "Installing drivers…",
    exporting_backup: "Exporting OEM drivers to ProgramData\\OtterSwiss\\DriverBackup…",
    backup_saved: |path| format!("Driver Store backup saved to {path}"),
    backup_failed: |e| format!("Backup failed, nothing was installed: {e}"),
    internal_error: "Internal error",
    device_errors: |n| format!("{} with errors", plural(n, ("device", "devices"))),
    all_ok: "All hardware OK",
    policy_managed: "This PC is managed: a policy (driver exclusion or WSUS) decides driver updates, so OtterSwiss does not search for or install drivers.",
    reboot_needed: "Windows needs a restart to finish installing the drivers.",
    confirm_title: "Install drivers?",
    confirm_subtitle: "A bad driver can leave a device without working. Windows can only undo it from Device Manager or a restore point.",
    install: "Install",
    firmware_warning: "Includes firmware or BIOS: an interrupted update can leave the PC unable to boot. Keep it plugged in.",
    backup_first: "Back up installed drivers first (ProgramData\\OtterSwiss\\DriverBackup)",
    vendor_title: "Vendor updates (NVIDIA and Intel)",
    vendor_subtitle: "Official signed installers matched by Hardware ID",
    download_install: "Download and install",
    history_title: "Drivers installed from OtterSwiss",
    history_subtitle: "Removing one sends its devices back to the best driver left on the PC.",
    remove: "Remove",
    remove_title: "Remove this driver?",
    remove_body: "Windows removes the package from the Driver Store and the devices that used it go back to the previous driver. It may ask for a restart.",
    catalog_title: "Microsoft Update Catalog (.cab)",
    catalog_subtitle: "Microsoft-signed .cab packages for devices without a driver",
    install_cab: "Install .cab",
    wua_title: "Windows Update WHQL drivers",
    wua_subtitle: "Packages verified by Windows Update Agent",
    download_install_n: |n| format!("Download and install ({n})"),
    hardware_title: "Installed hardware and drivers",
    showing_devices: |n| format!("Showing {}", plural(n, ("device", "devices"))),
    search_placeholder: "Filter by device, vendor, Hardware ID or .inf…",
    filter_all: |n| format!("All ({n})"),
    filter_errors: |n| format!("With errors ({n})"),
    filter_oem: |n| format!("Third-party OEM ({n})"),
    search_catalog: "Search the Catalog",
    searching_catalog: |id| format!("Searching the Microsoft Update Catalog for {id}…"),
    catalog_found: |n| {
        format!(
            "{} in the Microsoft Update Catalog.",
            plural(n, ("package found", "packages found"))
        )
    },
    pending_reboot: "Restart pending",
    cm_problem: |code| match code {
        1 => "Device not configured correctly (code 1)",
        10 => "Device cannot start (code 10)",
        12 => "Not enough free resources (code 12)",
        18 => "Reinstall the drivers (code 18)",
        22 => "Device is disabled (code 22)",
        28 => "Drivers are not installed (code 28)",
        31 => "Device is not working properly (code 31)",
        43 => "Windows stopped this device due to errors (code 43)",
        45 => "Device not connected (code 45)",
        52 => "Driver digital signature not verified (code 52)",
        _ => "Device or driver problem",
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    /// IN-06/IN-10: sin textos vacíos ni «...»; las frases con cantidad usan el singular solo con 1.
    #[test]
    fn textos_de_drivers_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.title,
                s.subtitle,
                s.scanning_devices,
                s.scanning_sources,
                s.search_placeholder,
                s.installing,
                s.exporting_backup,
                (s.cm_problem)(43),
                (s.cm_problem)(9999),
            ] {
                assert!(!f.trim().is_empty());
                assert!(!f.contains("..."), "usar «…»: {f}");
            }
        }
        assert_eq!((ES.devices_detected)(1), "1 dispositivo detectado.");
        assert_eq!((ES.devices_detected)(3), "3 dispositivos detectados.");
        assert_eq!((ES.device_errors)(1), "1 dispositivo con error");
        assert_eq!((ES.device_errors)(2), "2 dispositivos con error");
        assert_eq!(
            (EN.catalog_found)(1),
            "1 package found in the Microsoft Update Catalog."
        );
    }
}
