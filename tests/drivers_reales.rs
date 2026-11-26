#![allow(
    clippy::print_stdout,
    reason = "la prueba informa cada paso por consola"
)]
//! Drivers contra el hardware real del equipo, **como administrador**. No corre con `cargo test`.
//!
//! 1. `buscar`: solo lee. Lista los dispositivos y lo que encuentran Windows Update, NVIDIA y el Catálogo.
//! 2. `instalar_y_quitar`: instala la actualización de Windows Update cuyo id está en `OTTER_WU_ID`, comprueba que quedó
//!    en el historial de OtterSwiss y la quita con «Quitar» (`revert_installed_driver`). Elegir a mano un driver que no
//!    sea de pantalla, almacenamiento ni chipset.
//!
//! ```text
//! cargo test --release --test drivers_reales buscar -- --ignored --nocapture
//! $env:OTTER_WU_ID = "<id>"; cargo test --release --test drivers_reales instalar_y_quitar -- --ignored --nocapture
//! ```

#![cfg(windows)]

use std::sync::atomic::AtomicBool;

use otter_swiss::core::drivers;

#[test]
#[ignore = "consulta Windows Update con el hardware real; como administrador"]
fn buscar() {
    let devices = drivers::list_present_devices().expect("listar dispositivos");
    println!("dispositivos: {}", devices.len());
    let report = drivers::run_full_scan(&devices, &AtomicBool::new(false));
    println!(
        "política bloquea Windows Update: {}",
        report.wua_policy_blocked
    );
    if let Some(note) = &report.wua_note {
        println!("nota de Windows Update: {note}");
    }
    for n in &report.notes {
        println!("nota: {n}");
    }
    println!("\n== Windows Update ({})", report.wua_updates.len());
    for u in &report.wua_updates {
        println!(
            "id={} | {} | clase={} | proveedor={} | hw={} | {} MB | firmware={} | opcional={}",
            u.id,
            u.title,
            u.driver_class,
            u.driver_provider,
            u.hardware_id,
            u.size_mb,
            u.is_firmware,
            u.is_optional
        );
    }
    println!("\n== Fabricante ({})", report.vendor_updates.len());
    for v in &report.vendor_updates {
        println!("{} | {} → {}", v.title, v.current_version, v.latest_version);
    }
    println!("\n== Catálogo ({})", report.catalog_matches.len());
    for c in &report.catalog_matches {
        println!("id={} | {} | {}", c.update_id, c.title, c.version);
    }
    let history = drivers::driver_install_history().expect("historial");
    println!("\nhistorial de OtterSwiss: {}", history.len());
}

#[test]
#[ignore = "instala y quita un driver real; como administrador y con OTTER_WU_ID"]
fn instalar_y_quitar() {
    let id = std::env::var("OTTER_WU_ID").expect("OTTER_WU_ID con el id de buscar");
    let before: Vec<String> = drivers::driver_install_history()
        .expect("historial")
        .into_iter()
        .map(|r| r.published_name)
        .collect();

    let report = drivers::install_driver_updates(std::slice::from_ref(&id)).expect("instalar");
    println!(
        "instalación: ok={} fallos={} reinicio={} aviso={:?}",
        report.succeeded_count, report.failed_count, report.reboot_required, report.history_warning
    );
    assert_eq!(report.failed_count, 0, "la instalación no debe fallar");

    let added: Vec<_> = drivers::driver_install_history()
        .expect("historial")
        .into_iter()
        .filter(|r| !before.contains(&r.published_name))
        .collect();
    println!("paquetes nuevos en el historial: {added:?}");

    for record in &added {
        let rb = drivers::revert_installed_driver(&record.published_name).expect("quitar");
        println!(
            "quitar {}: ok={} fallos={} reinicio={}",
            record.published_name, rb.succeeded_count, rb.failed_count, rb.reboot_required
        );
    }
    let still: Vec<_> = drivers::driver_install_history()
        .expect("historial")
        .into_iter()
        .filter(|r| added.iter().any(|a| a.published_name == r.published_name))
        .collect();
    assert!(still.is_empty(), "lo instalado ya no está: {still:?}");
    if added.is_empty() {
        println!(
            "Windows Update no agregó un paquete nuevo al Driver Store (ya estaba): no hay nada que quitar."
        );
    }
}
