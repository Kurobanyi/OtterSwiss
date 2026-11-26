#![allow(
    clippy::print_stdout,
    reason = "el recorrido informa cada paso por consola"
)]
//! Recorrido de las acciones que cambian el sistema, contra Windows real y **como administrador**.
//!
//! Solo para una VM desechable: aplica ajustes, cambia el DNS, elimina una app Appx, borra temporales y
//! desinstala una app falsa. Cada paso comprueba el efecto y que la reversión deja el sistema como estaba.
//! No corre con `cargo test`; se lanza a propósito:
//!
//! ```text
//! cargo test --release --test vm_sistema -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Un área sola: `cargo test --release --test vm_sistema dns -- --ignored --nocapture`.
//!
//! Escribe cada comprobación como `OK`/`FALLO` y termina en `panic` si alguna falló.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use otter_swiss::core::archive::{self, ArchiveFormat, resolve_extract_destination};
use otter_swiss::core::cleaner;
use otter_swiss::core::debloat::{self, DEBLOAT_REG_TWEAKS};
use otter_swiss::core::dns::{DnsManager, list_active_adapters};
use otter_swiss::core::optimize::engine::{OptimizationEngine, OptimizationProfile};
use otter_swiss::core::optimize::tweaks_catalog::{REGISTRY_TWEAKS, RegHive, SERVICE_TWEAKS};
use otter_swiss::core::security::data_dir::ensure_admin_only_data_dir;
use otter_swiss::core::store::app_store;
use otter_swiss::core::uninstall::{self, LeftoverKind};
use winreg::RegKey;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, KEY_READ, KEY_WOW64_64KEY,
};

struct Checks {
    failed: Vec<String>,
}

impl Checks {
    fn new() -> Self {
        Self { failed: Vec::new() }
    }
    fn check(&mut self, ok: bool, what: &str) {
        if ok {
            println!("OK     {what}");
        } else {
            println!("FALLO  {what}");
            self.failed.push(what.to_string());
        }
    }
    fn note(&self, msg: &str) {
        println!("       {msg}");
    }
}

/// `(tipo, bytes)` del valor, o `None` si no existe.
fn raw_value(hklm: bool, subkey: &str, name: &str) -> Option<(u32, Vec<u8>)> {
    let (root, flags) = if hklm {
        (
            RegKey::predef(HKEY_LOCAL_MACHINE),
            KEY_READ | KEY_WOW64_64KEY,
        )
    } else {
        (RegKey::predef(HKEY_CURRENT_USER), KEY_READ)
    };
    let key = root.open_subkey_with_flags(subkey, flags).ok()?;
    let v = key.get_raw_value(name).ok()?;
    Some((v.vtype as u32, v.bytes.into_owned()))
}

fn service_start(name: &str) -> Option<u32> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(format!(r"SYSTEM\CurrentControlSet\Services\{name}"))
        .ok()?
        .get_value::<u32, _>("Start")
        .ok()
}

fn active_power_scheme() -> String {
    let out = Command::new("powercfg").arg("/getactivescheme").output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .unwrap_or_default()
        .split_whitespace()
        .find(|w| w.len() == 36 && w.matches('-').count() == 4)
        .unwrap_or_default()
        .to_string()
}

type Snapshot = (Vec<Option<(u32, Vec<u8>)>>, Vec<Option<u32>>, String);

fn engine_snapshot() -> Snapshot {
    let regs = REGISTRY_TWEAKS
        .iter()
        .map(|t| raw_value(t.hive == RegHive::Hklm, t.subkey, t.value_name))
        .collect();
    let svcs = SERVICE_TWEAKS
        .iter()
        .map(|s| service_start(s.service_name))
        .collect();
    (regs, svcs, active_power_scheme())
}

fn debloat_snapshot() -> Vec<Option<(u32, Vec<u8>)>> {
    DEBLOAT_REG_TWEAKS
        .iter()
        .flat_map(|t| t.entries.iter())
        .map(|e| raw_value(e.hive == RegHive::Hklm, e.subkey, e.value_name))
        .collect()
}

fn dns_snapshot(guid: &str) -> (Option<String>, Option<String>) {
    let read = |stack: &str| {
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(format!(
                r"SYSTEM\CurrentControlSet\Services\{stack}\Parameters\Interfaces\{{{guid}}}"
            ))
            .ok()
            .and_then(|k| k.get_value::<String, _>("NameServer").ok())
            .map(|s| s.replace(' ', ","))
    };
    (read("Tcpip"), read("Tcpip6"))
}

fn optimizar_y_revertir(c: &mut Checks, data: &Path) {
    println!("\n== Optimizar y revertir");
    let before = engine_snapshot();
    let engine = OptimizationEngine::new(data);
    match engine.apply_profile(OptimizationProfile::HighEnd) {
        Ok(r) => {
            c.check(r.failed_items.is_empty(), "aplicar perfil sin fallos");
            for f in &r.failed_items {
                c.note(f);
            }
            c.note(&format!(
                "aplicados {}, omitidos {}",
                r.applied_count, r.skipped_count
            ));
        }
        Err(e) => c.check(false, &format!("aplicar perfil: {e}")),
    }
    c.check(engine.has_backup(), "queda respaldo tras optimizar");
    c.check(engine_snapshot() != before, "el sistema cambió");
    match engine.rollback(false) {
        Ok(r) => {
            c.check(r.failed_items.is_empty(), "revertir sin fallos");
            c.check(r.backup_deleted, "el respaldo se borra tras revertir");
            for f in &r.failed_items {
                c.note(f);
            }
        }
        Err(e) => c.check(false, &format!("revertir: {e}")),
    }
    let after = engine_snapshot();
    c.check(after.0 == before.0, "registro igual que antes");
    c.check(after.1 == before.1, "servicios igual que antes");
    c.check(after.2 == before.2, "plan de energía igual que antes");
}

fn debloater(c: &mut Checks) {
    println!("\n== Debloater (registro y una app)");
    let before = debloat_snapshot();
    let ids: Vec<&'static str> = DEBLOAT_REG_TWEAKS.iter().map(|t| t.id).collect();
    let cancel = AtomicBool::new(false);
    match debloat::execute_debloat(&[], &ids, &cancel, None) {
        Ok(r) => c.check(
            r.failed_count == 0,
            "aplicar ajustes del Debloater sin fallos",
        ),
        Err(e) => c.check(false, &format!("aplicar Debloater: {e}")),
    }
    let applied = DEBLOAT_REG_TWEAKS
        .iter()
        .flat_map(|t| t.entries.iter())
        .all(|e| {
            raw_value(e.hive == RegHive::Hklm, e.subkey, e.value_name)
                .is_some_and(|(_, b)| b == e.target_dword.to_le_bytes())
        });
    c.check(applied, "todos los valores quedaron en su objetivo");
    match debloat::revert_registry_debloat() {
        Ok(n) => c.note(&format!("revertidos {n}")),
        Err(e) => c.check(false, &format!("revertir Debloater: {e}")),
    }
    c.check(
        debloat_snapshot() == before,
        "registro del Debloater igual que antes",
    );
    c.check(
        !debloat::has_registry_backup(),
        "el respaldo del Debloater se borró",
    );

    // Una app Appx del catálogo (la VM es desechable): debe desaparecer del escaneo.
    let Ok(scan) = debloat::scan_system() else {
        c.check(false, "escanear apps Appx");
        return;
    };
    let target = scan.apps.iter().find(|a| {
        a.installed
            && matches!(
                a.package_name,
                "Microsoft.BingWeather" | "Microsoft.BingNews" | "Microsoft.GetHelp"
            )
    });
    let Some(app) = target else {
        c.note("ninguna app de prueba instalada; se omite la eliminación");
        return;
    };
    c.note(&format!("eliminando {}", app.package_name));
    match debloat::execute_debloat(std::slice::from_ref(app), &[], &cancel, None) {
        Ok(r) => {
            c.check(r.removed_apps == 1, "app Appx eliminada");
            for o in &r.outcomes {
                c.note(&format!("{}: {}", o.label, o.message));
            }
        }
        Err(e) => c.check(false, &format!("eliminar app: {e}")),
    }
    let still = debloat::scan_system()
        .map(|s| s.apps.iter().any(|a| a.id == app.id && a.installed))
        .unwrap_or(true);
    c.check(!still, "la app ya no aparece instalada");
}

fn dns(c: &mut Checks, data: &Path) {
    println!("\n== DNS");
    let adapters = list_active_adapters().unwrap_or_default();
    // Preferir uno físico; en Windows Sandbox solo hay un adaptador virtual de Hyper-V.
    let Some(ad) = adapters
        .iter()
        .find(|a| !a.is_virtual_or_vpn)
        .or_else(|| adapters.first())
    else {
        c.check(false, "hay un adaptador de red activo");
        return;
    };
    c.note(&format!("adaptador: {}", ad.friendly_name));
    let before = dns_snapshot(&ad.interface_guid);
    let mgr = DnsManager::new(data);
    let res = mgr.apply_dns_to_selected(
        std::slice::from_ref(&ad.interface_guid),
        "1.1.1.1,1.0.0.1",
        "2606:4700:4700::1111",
    );
    c.check(res.is_ok(), &format!("aplicar DNS {:?}", res.err()));
    let applied = dns_snapshot(&ad.interface_guid);
    c.check(
        applied.0.as_deref().is_some_and(|v| v.contains("1.1.1.1")),
        "IPv4 aplicado",
    );
    let res = mgr.restore_dns();
    c.check(res.is_ok(), &format!("restaurar DNS {:?}", res.err()));
    let after = dns_snapshot(&ad.interface_guid);
    let same = |a: &Option<String>, b: &Option<String>| {
        a.clone().unwrap_or_default() == b.clone().unwrap_or_default()
    };
    c.check(same(&after.0, &before.0), "IPv4 igual que antes");
    c.check(same(&after.1, &before.1), "IPv6 igual que antes (SYS-07)");
    c.check(!mgr.has_backup(), "el respaldo de DNS se borró");
}

fn limpiador(c: &mut Checks) {
    println!("\n== Limpiador (con junction trampa en C:\\Windows\\Temp)");
    let canary_dir = PathBuf::from(r"C:\OtterCanary");
    let _ = std::fs::create_dir_all(&canary_dir);
    let canary = canary_dir.join("no-borrar.txt");
    std::fs::write(&canary, "canario").ok();
    let junction = PathBuf::from(r"C:\Windows\Temp\otter-trampa");
    let _ = std::fs::remove_dir(&junction);
    let mk = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&canary_dir)
        .output();
    c.check(
        mk.is_ok_and(|o| o.status.success()),
        "crear junction trampa",
    );
    let tmp_file = std::env::temp_dir().join("otter-prueba-limpiador.tmp");
    std::fs::write(&tmp_file, "temporal").ok();

    let cancel = AtomicBool::new(false);
    let results = cleaner::run_full_cleanup(0, false, &cancel);
    for r in &results {
        c.note(&format!(
            "{}: {} archivos, {} bytes{}",
            r.category,
            r.files_removed,
            r.bytes_freed,
            r.error
                .as_ref()
                .map(|e| format!(" — {e}"))
                .unwrap_or_default()
        ));
    }
    c.check(!tmp_file.exists(), "el temporal del usuario se borró");
    c.check(
        canary.exists(),
        "el archivo detrás de la junction sigue (SEC-01)",
    );
    let _ = std::fs::remove_dir(&junction);
    let _ = std::fs::remove_dir_all(&canary_dir);
}

fn archivador(c: &mut Checks) {
    println!("\n== Archivador");
    let base = std::env::temp_dir().join(format!("otter-arch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let src = base.join("proyecto");
    std::fs::create_dir_all(src.join("sub")).ok();
    std::fs::write(src.join("a.txt"), "uno").ok();
    std::fs::write(src.join("sub").join("b.txt"), "dos").ok();
    let zip = base.join("proyecto.zip");
    let cancel = AtomicBool::new(false);
    let res = archive::compress_path(&src, &zip, ArchiveFormat::Zip, &cancel);
    c.check(res.is_ok() && zip.is_file(), "comprimir a .zip");
    // Marca del archivo descargado, para comprobar que se propaga (SEC-15).
    let mut ads = zip.as_os_str().to_os_string();
    ads.push(":Zone.Identifier");
    std::fs::write(PathBuf::from(ads), "[ZoneTransfer]\r\nZoneId=3\r\n").ok();

    // `proyecto` ya existe: extraer no debe tocarla (SYS-01).
    std::fs::write(src.join("centinela.txt"), "no tocar").ok();
    let dest = resolve_extract_destination(&src);
    c.check(dest != src, &format!("destino libre: {}", dest.display()));
    let res = archive::extract_archive(&zip, &dest, 1 << 30, &cancel);
    c.check(res.is_ok(), &format!("extraer {:?}", res.err()));
    c.check(
        src.join("centinela.txt").exists(),
        "la carpeta existente quedó intacta",
    );
    let extracted = dest.join("proyecto").join("a.txt");
    let extracted = if extracted.exists() {
        extracted
    } else {
        dest.join("a.txt")
    };
    c.check(extracted.exists(), "archivo extraído");
    let mut zone = extracted.as_os_str().to_os_string();
    zone.push(":Zone.Identifier");
    c.check(
        PathBuf::from(zone).exists(),
        "el extraído conserva Mark-of-the-Web",
    );
    let _ = std::fs::remove_dir_all(&base);
}

fn desinstalador(c: &mut Checks) {
    println!("\n== Desinstalar y purgar (app falsa)");
    let pf = PathBuf::from(r"C:\Program Files\OtterFakeApp");
    std::fs::create_dir_all(&pf).ok();
    std::fs::write(pf.join("app.txt"), "app").ok();
    let other = PathBuf::from(r"C:\Program Files\OtterFakeAppTools");
    std::fs::create_dir_all(&other).ok();
    std::fs::write(other.join("otra.txt"), "otra app").ok();

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let uninst_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\OtterFakeApp";
    let (k, _) = hklm
        .create_subkey_with_flags(uninst_path, KEY_ALL_ACCESS | KEY_WOW64_64KEY)
        .expect("crear entrada Uninstall");
    k.set_value("DisplayName", &"OtterFakeApp").ok();
    k.set_value("Publisher", &"Otter Test").ok();
    k.set_value("InstallLocation", &pf.to_string_lossy().to_string())
        .ok();
    k.set_value(
        "UninstallString",
        &format!(r#""C:\Windows\System32\reg.exe" delete "HKLM\{uninst_path}" /f /reg:64"#),
    )
    .ok();
    drop(k);
    let (sw, _) = hklm
        .create_subkey_with_flags(r"SOFTWARE\OtterFakeApp", KEY_ALL_ACCESS | KEY_WOW64_64KEY)
        .expect("crear clave de ajustes");
    sw.set_value("Ajuste", &1u32).ok();
    drop(sw);

    let app = uninstall::list_installed_apps()
        .unwrap_or_default()
        .into_iter()
        .find(|a| a.display_name == "OtterFakeApp");
    let Some(app) = app else {
        c.check(false, "la app falsa aparece en la lista");
        return;
    };
    // Mientras sigue registrada, su carpeta no es un rastro (SYS-04).
    let early = uninstall::scan_leftovers(&app).unwrap_or_default();
    c.check(
        !early
            .iter()
            .any(|i| i.path.as_deref() == Some(pf.as_path())),
        "instalada: no se ofrece su carpeta",
    );

    let code = uninstall::run_uninstaller(&app, std::time::Duration::from_secs(60));
    c.check(
        code.as_ref().is_ok_and(|e| e.exit_code == 0),
        &format!("desinstalador: {code:?}"),
    );
    c.check(
        !uninstall::is_still_registered(&app),
        "la entrada Uninstall desapareció",
    );

    let items = uninstall::scan_leftovers(&app).unwrap_or_default();
    for i in &items {
        c.note(&format!(
            "rastro [{}] {} ({})",
            if i.recommended { "x" } else { " " },
            i.target_display,
            i.category
        ));
    }
    c.check(
        items.iter().any(|i| {
            i.kind == LeftoverKind::Directory
                && i.path.as_deref() == Some(pf.as_path())
                && i.recommended
        }),
        "su carpeta aparece y viene marcada",
    );
    c.check(
        !items
            .iter()
            .any(|i| i.path.as_deref() == Some(other.as_path())),
        "otra app con nombre parecido no aparece (SYS-03)",
    );
    let chosen: Vec<_> = items.iter().filter(|i| i.recommended).cloned().collect();
    let report = uninstall::purge_leftover_items(&chosen);
    c.check(
        report.failed_count == 0,
        &format!("purga sin fallos {:?}", report.errors),
    );
    c.check(!pf.exists(), "carpeta de la app borrada");
    c.check(other.exists(), "la otra app sigue intacta");
    let backups = ensure_admin_only_data_dir()
        .map(|d| d.join("RegBackups"))
        .ok()
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|it| {
            it.flatten()
                .any(|e| e.file_name().to_string_lossy().contains("OtterFakeApp"))
        })
        .unwrap_or(false);
    c.check(backups, "la clave borrada quedó exportada en RegBackups");
    let _ = std::fs::remove_dir_all(&other);
    let _ = hklm.delete_subkey_all(r"SOFTWARE\OtterFakeApp");
}

fn revertir_todo(c: &mut Checks, data: &Path) {
    println!("\n== Revertir todo (lo que hace --rollback)");
    let before_engine = engine_snapshot();
    let before_debloat = debloat_snapshot();
    let engine = OptimizationEngine::new(data);
    let _ = engine.apply_profile(OptimizationProfile::HighEnd);
    let ids: Vec<&'static str> = DEBLOAT_REG_TWEAKS.iter().map(|t| t.id).collect();
    let _ = debloat::execute_debloat(&[], &ids, &AtomicBool::new(false), None);
    match otter_swiss::core::rollback::revert_all_changes() {
        Ok(r) => {
            c.check(r.is_complete(), "reversión completa");
            for f in &r.failures {
                c.note(f);
            }
            c.check(r.data_dir_deleted, "la carpeta de datos se borró");
        }
        Err(e) => c.check(false, &format!("revert_all_changes: {e}")),
    }
    c.check(engine_snapshot() == before_engine, "optimización revertida");
    c.check(debloat_snapshot() == before_debloat, "Debloater revertido");
}

/// ERR-24: instala de verdad una app de la tienda para el equipo y otra por usuario, cancela una a mitad y quita la primera.
/// Necesita red y winget; es lenta, por eso va en su propio paso.
fn tienda(c: &mut Checks, _data: &Path) {
    println!(
        "
== Tienda (winget real)"
    );
    let cancel = AtomicBool::new(false);
    for id in ["7zip", "playnite"] {
        let before = app_store::detect_installed_ids().contains(id);
        c.check(!before, &format!("{id}: no estaba instalada"));
        let res = app_store::install_package_cancellable(id, &cancel);
        c.check(res.is_ok(), &format!("{id}: instalar {:?}", res.err()));
        c.check(
            app_store::detect_installed_ids().contains(id),
            &format!("{id}: aparece instalada"),
        );
    }

    // Cancelar a mitad: VLC pesa lo bastante como para que 4 s caigan dentro de la descarga.
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let flag = std::sync::Arc::clone(&cancel);
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        flag.store(true, std::sync::atomic::Ordering::Release);
    });
    let res = app_store::install_package_cancellable("vlc", &cancel);
    let _ = stopper.join();
    c.check(
        matches!(res, Err(otter_swiss::core::error::CoreError::Cancelled)),
        &format!("cancelar a mitad devuelve Cancelled ({res:?})"),
    );
    c.check(
        !app_store::detect_installed_ids().contains("vlc"),
        "la instalación cancelada no deja la app registrada",
    );

    // Quitar 7-Zip con el desinstalador de la app.
    if let Some(app) = uninstall::list_installed_apps()
        .unwrap_or_default()
        .into_iter()
        .find(|a| a.display_name.starts_with("7-Zip"))
    {
        let code = uninstall::run_uninstaller(&app, std::time::Duration::from_secs(120));
        c.note(&format!("desinstalador de 7-Zip: {code:?}"));
        c.check(
            !app_store::detect_installed_ids().contains("7zip"),
            "7-Zip ya no aparece instalada",
        );
    } else {
        c.check(false, "7-Zip aparece en la lista de apps instaladas");
    }
}

/// Corre un área con su propia lista de comprobaciones y falla si alguna falló.
fn run_area(name: &str, area: impl FnOnce(&mut Checks, &Path)) {
    let mut c = Checks::new();
    let data = ensure_admin_only_data_dir().expect("carpeta de datos (¿corre como admin?)");
    println!("\n##### {name}");
    area(&mut c, &data);
    println!("== {}: {} fallos", name, c.failed.len());
    assert!(c.failed.is_empty(), "{name}: fallaron {:#?}", c.failed);
}

// CL-18: un test por área para poder correr una sola (`cargo test --test vm_sistema dns -- --ignored`). Con
// `--test-threads=1` corren en el orden de su nombre, así que «revertir todo» va al final.
#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador"]
fn paso_1_optimizar_y_revertir() {
    run_area("optimizar", optimizar_y_revertir);
}

#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador"]
fn paso_2_debloater() {
    run_area("debloater", |c, _| debloater(c));
}

#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador"]
fn paso_3_dns() {
    run_area("dns", dns);
}

#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador"]
fn paso_4_limpiador() {
    run_area("limpiador", |c, _| limpiador(c));
}

#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador"]
fn paso_5_archivador() {
    run_area("archivador", |c, _| archivador(c));
}

#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador"]
fn paso_6_desinstalador() {
    run_area("desinstalador", |c, _| desinstalador(c));
}

#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador"]
fn paso_7_revertir_todo() {
    run_area("revertir todo", revertir_todo);
}

#[test]
#[ignore = "cambia el sistema: solo en una VM desechable, como administrador y con red"]
fn paso_8_tienda() {
    run_area("tienda", tienda);
}
