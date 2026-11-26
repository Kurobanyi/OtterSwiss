# Expedientes de buenas prácticas

Evidencia y propuesta por ID. El índice, las prioridades y el orden de ataque viven en `docs/PRACTICAS.md`. Cada ID tiene una sola
sección `###` y un ancla `#id` en minúscula.

## Origen

Auditoría de buenas prácticas del 24/11/2026 sobre `main` (`adf794d`, v3.5.1 con los arreglos de `fix/auditoria`). Cuatro pasadas
en paralelo, solo de lectura: arquitectura y orden; código limpio y reutilizable; temas, tokens y CSS; i18n, linters y herramientas.
La de herramientas compiló y midió (`clippy` con lints extra, `-W rust-2024-compatibility`, `cargo tree -d`, `git ls-files --eol`);
los contrastes se calcularon con la fórmula WCAG sobre los hex de `assets/main.css`. Los hallazgos repetidos entre pasadas se
fusionaron en un ID. El agente principal volvió a comprobar contra el código una muestra de cada pasada (marcados **confirmado**) y
corrigió las cifras que no coincidían. Los defectos (no mejoras) que salieron de aquí están en `docs/BUGS.md`: `SEC-29`, `ERR-14`,
`UI-17` y `UI-18`.

Leyenda:

- **Prioridad**: P1 antes de que crezca el código o de la próxima release · P2 siguiente versión · P3 pulido.
- **Esfuerzo**: S (horas) · M (un día o dos) · L (varios días, conviene por etapas).
- **Evidencia**: *observado* (lectura o medición citada) · *inferido* (no comprobado en la app) · *confirmado* (re-leído por el
  agente principal).
- **Estado**: Pendiente · En curso · Hecho · Descartado.

# Orden de archivos y carpetas (`OR-`)

<a id="or-01"></a>
### OR-01 · `src/core` es una carpeta plana de 26 archivos que mezcla plataforma, seguridad, funciones y catálogos
- **Prioridad**: P1 · **Esfuerzo**: L · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: primitivas Win32 (`win_common`, `safe_fs`, `process`, `registry`, `services`, `power`), seguridad (`trust`), hardware
  (`specs`, `sensors`), funciones (`archiver`, `cleaner`, `debloater`, `uninstaller`…) y datos (`tweaks_catalog`) están al mismo
  nivel, sin que la carpeta diga qué capa depende de cuál.
- **Propuesta** (aprobada por el usuario el 24/11): `core/{platform, security, hardware, optimize, debloat, uninstall, archive,
  store}/` más `error.rs`, `cli.rs`, `rollback.rs` y las funciones chicas sueltas (`cleaner`, `dns`, `updater`, `context_menu`,
  `sys_tools`). `drivers.rs` queda donde está (`ARQ-03`, diferido).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): core en platform/security/hardware/optimize/debloat/uninstall/archive/store; platform ya no importa de capas superiores (registry y services partidos). Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="or-02"></a>
### OR-02 · Todos los componentes de UI en un solo `components/mod.rs`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Propuesta**: un archivo por componente (`modal.rs`, `card.rs`, `status.rs`, `state.rs`, `progress.rs`, `check_row.rs`) y
  `mod.rs` solo con los `pub use`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): components/{card, state, tone, status, modal, progress}.rs; mod.rs solo reexporta. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="or-03"></a>
### OR-03 · Nombres y lugares que no siguen la convención
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `views/hardware_tab.rs` es la única vista con sufijo; la mini ventana del archivador (un componente) vive en
  `main.rs`; `ui/mod.rs` es a la vez módulo y shell de la app; `otterswiss.iss` está suelto en la raíz; los logos están mezclados
  con `main.css` en `assets/`.
- **Propuesta**: `views/hardware.rs`, `views/mini_archive.rs`, `ui/app.rs` y `ui/nav.rs`, `installer/otterswiss.iss`,
  `assets/brand/`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): views/hardware.rs, views/mini_archive.rs, ui/app.rs, installer/otterswiss.iss, assets/brand/. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="or-04"></a>
### OR-04 · El orden interno de los archivos no sigue una regla
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: constantes entre funciones (`GENERIC_FOLDER_TOKENS` en `uninstaller.rs:234`, entre helpers), helpers antes y después
  del `impl`, tipos públicos al final (`FullRevertReport` en la línea 1349), cabeceras `// src/…` que repiten la ruta.
- **Propuesta**: regla en `AGENTS.md`: `use` → constantes → tipos públicos → `impl` → funciones privadas → `#[cfg(test)]`; sin
  cabeceras de ruta.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Regla escrita en AGENTS.md y fuera las cabeceras `// src/…`; los archivos partidos ya la siguen, el resto se ordena cuando se toque. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): los 29 archivos de `src/` que rompían la regla (constantes entre funciones, funciones antes del `impl`, tipos públicos al final) se reordenaron con un script que parte cada archivo en elementos de nivel superior y los ordena por `use` → constantes → tipos e `impl` → funciones → `#[cfg(test)]`, sin tocar su texto (inserciones = borrados en el diff). Excepciones escritas en `AGENTS.md`: los `static ES/EN` de `ui/i18n/` van tras su `struct` y los dos archivos FFI de sensores (`adl.rs`, `nvml.rs`). Verificado: `cargo test` (89 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="or-05"></a>
### OR-05 · Dos carpetas de configuración de agentes sin explicar
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `.agents/` (reglas, skills, scripts, hook, relevo) y `.claude/agents/` (auditores, commit-writer); `AGENTS.md` no
  menciona ninguna.
- **Propuesta**: documentar en `AGENTS.md` qué va en cada una.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): AGENTS.md explica `.agents/` y `.claude/agents/`. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

# Arquitectura y orden (`AR-`)

<a id="ar-01"></a>
### AR-01 · La vista del desinstalador decide el éxito y orquesta el flujo
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: confirmado · **Estado**: Hecho
- **Qué pasa**: `uninstall_exit_is_success` (`matches!(code, 0 | 3010 | 1641)`) vive en la vista y repite `ProcessExit::is_success`.
  `run_uninstaller` devuelve solo `exit_code` y la secuencia «ejecutar → `is_still_registered` → `scan_leftovers`», con su timeout de
  300 s, está en la UI. Dos fuentes de verdad, y el flujo central no se puede probar ni usar desde la CLI.
- **Dónde**: `src/ui/views/uninstaller.rs:30-32,142-277`; `src/core/process.rs:166-168`; `src/core/uninstaller.rs:603,678`.
- **Propuesta**: `run_uninstaller` devuelve `ProcessExit`; `UninstallerManager::uninstall_and_scan(app) -> UninstallOutcome`
  (`Failed(code)`, `StillRegistered`, `Removed { reboot_required, leftovers }`); la vista solo traduce el enum.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): UninstallerManager::uninstall_and_scan devuelve UninstallOutcome; run_uninstaller devuelve ProcessExit; la vista solo traduce. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-02"></a>
### AR-02 · Dos pilas de respaldo de registro y tres almacenes JSON casi iguales
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Parcial
- **Qué pasa**: el Debloater tiene su propio `DebloatRegHive`, `DebloatRegSnapshot`, captura, aplicación y restauración, que duplican
  `RegHive`, `RegistrySnapshot` y `registry::capture/apply/restore`. La validación está copiada con números mágicos
  (`[1, 2, 3, 4, 11]` y `1024` en los dos lados) y ya divergen: el snapshot del Debloater no tiene `key_created` ni `applied_bytes`.
  Además, `backup.rs`, `debloater.rs` y `dns.rs` repiten «leer acotado → `from_slice` → validar → `to_vec_pretty` →
  `atomic_write_new`». El patrón 4 y el 6 de `docs/BUGS.md` se arreglaron tres veces por separado.
- **Dónde**: `src/core/debloater.rs:290,459-503,911-1120`; `src/core/backup.rs:15-16,119-277`; `src/core/registry.rs:9,153-336`;
  `src/core/dns.rs:554-585`; `src/core/tweaks_catalog.rs:5`.
- **Propuesta**: `JsonStore<T>` con `load_or(default, max, validate)` y `save`; el Debloater expresa sus entradas como
  `RegistryTweakSpec` y usa `registry` y un `BackupManager` parametrizado; un lector del formato viejo solo para migrar.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): RegHive/RegLocation comunes y platform/json_file.rs (read_backup, write_backup) para los tres respaldos. El snapshot del Debloater conserva su formato propio por compatibilidad con respaldos existentes. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-03"></a>
### AR-03 · Todo es `pub`: hay atajos que saltan las validaciones y el compilador no ve el código muerto
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `lib.rs` exporta `core` y `ui` enteros y cada módulo es `pub mod`. `ArchiveManager::compress_to_zip`, `extract_zip`,
  `extract_7z`, `extract_tar*` y compañía son `pub` sin usos externos, y llamarlas directamente se salta `validate_cli_paths`, el «no
  sobrescribir» y el `.partial` con renombre (SYS-22). Como todo es alcanzable, `rustc` no avisa `dead_code` (ver `AR-05`).
- **Dónde**: `src/lib.rs`; `src/core/mod.rs`; `src/core/archiver.rs:421-921,713-762`.
- **Propuesta**: `pub(crate)` o privado por defecto; `pub` solo la fachada que usan `main.rs` y `tests/vm_sistema.rs`. Módulo por
  módulo.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Las funciones por formato del archivador son pub(super): ya no se puede saltar validate_cli_paths ni el .partial. El resto de la API sigue `pub`. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): 24 módulos de ui y core pasan a `pub(crate)`; quedan `pub` solo `cli`, `progress`, `rollback` y lo que usan los tests de integración. Clippy ya marca lo muerto. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-04"></a>
### AR-04 · La detección de hardware está escrita tres veces y el motor paga la completa para leer el tier
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `HardwareProfile::detect` (CPU, placa, GPU, discos por `\\.\PhysicalDrive{n}`), `specs.rs` (lo mismo con más
  detalle) y `sensors.rs` (discos otra vez) se solapan, con dos tablas de `BusType` de números mágicos y dos lectores de C-string. El
  motor llama a `detect()` completo solo para `tier` (núcleos y RAM). `sysinfo` entra solo para esto (ver `AR-18`).
- **Dónde**: `src/core/win_common.rs:422-630,543,604`; `src/core/specs.rs:233,467,685,746-848`; `src/core/sensors.rs:593-710`;
  `src/core/engine.rs:89-120,199`.
- **Propuesta**: `HardwareTier::detect()` aparte; la descripción en `specs` (y el Dashboard muestra `SystemSpecs`);
  `core/hw/storage.rs` con la enumeración de discos que comparten `specs` y `sensors`, y `BusType` con las constantes del crate.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): HardwareTier::from_cores_and_ram aparte (con test) y fuera sysinfo. La enumeración de discos sigue en profile, specs y sensors. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): `hardware/drives.rs` abre y nombra los discos una sola vez; specs, perfil y sensores lo usan. Arregla además que el perfil sin elevación no listaba discos (abría con FILE_GENERIC_READ). Salida de specs comparada antes y después: idéntica. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-05"></a>
### AR-05 · Código muerto que el compilador no puede señalar
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (`config.rs`) · **Estado**: Hecho
- **Qué pasa**: sin usos fuera de su archivo: `core/config.rs` entero (`AppConfig`, `DefaultArchiveFormat`, que además duplica
  `ArchiveFormat`), `specs::export_specs_json` (mientras `hardware_tab.rs:89-104` reimplementa la exportación),
  `safe_fs::duplicate_file_handle` (ver `CL-01`), `TrimWorkingSetsReport::explanatory_note`, `CoreError::Sensor`,
  `AppStoreManager::install_package` y `find_package`. `drivers.rs` queda fuera: es `ARQ-03`, diferido por decisión.
- **Dónde**: `src/core/config.rs`; `src/core/specs.rs:191`; `src/core/safe_fs.rs:485-488`; `src/core/sys_tools.rs:28,80`;
  `src/core/app_store.rs:662`.
- **Propuesta**: borrarlo, o conectar `AppConfig` si se va a usar (ver `AR-20`). Tras `AR-03`, clippy marcará lo que quede.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Fuera config.rs, install_package, duplicate_file_handle, CoreError::Sensor, explanatory_note y export_specs_json (reemplazado por export_to_desktop); find_package privado. drivers.rs queda por ARQ-03. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-06"></a>
### AR-06 · `CoreError` clasifica por módulo y no por causa, y casi nunca guarda el error de origen
- **Prioridad**: P2 · **Esfuerzo**: L · **Evidencia**: confirmado (161 `source: None`) · **Estado**: Hecha
- **Qué pasa**: variantes `Trust`, `Backup`, `Archive`, `SystemTool` (52 usos, cajón de sastre) con `message: String`; 161
  `source: None`, así que se pierde el `HRESULT` o el `io::ErrorKind`. `Registry` y `Service` usan `Option<io::Error>` y el resto
  `Box<dyn Error>`. Tras un `SafeHandle::new(..)` que da `None` se llama a `CoreError::from_win32`, que lee un `last_os_error` ajeno.
  La UI solo distingue `Cancelled` y `NotElevated`; el resto llega como `e.to_string()`, y `dns.rs` usa `Err(String::new())` como
  centinela de «no hay adaptadores». Es la raíz técnica de que `UI-11` no se pueda cerrar: el texto en español se arma en core.
- **Dónde**: `src/core/error.rs`; `src/core/process.rs:77-107`; `src/core/win_common.rs:236-270,649-689`; `src/ui/views/dns.rs:141,159`;
  `src/core/specs.rs` (7 firmas con `Result<T, CoreError>` en vez de `CoreResult`).
- **Propuesta**: variantes por causa (`NotFound`, `AccessDenied`, `Timeout`, `ExitCode(u32)`, `Win32 { api, code }`,
  `NoActiveAdapters`…) con `#[source]` siempre que lo haya; helpers `CoreError::sys(ctx)` y `invalid_handle(ctx)`; un `source` de un
  solo tipo. Gradual, módulo por módulo.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): CoreError::sys/trust guardan el source (30 sitios), invalid_handle no lee GetLastError ajeno, NoActiveAdapters tipado. Quedan unos 125 `source: None` para migrar de a poco. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre final** (2026-11-25): se revisaron los 95 source: None que quedaban. Ninguno perdía un error de origen: eran fallos de una comprobación propia (política de descarga, firma que no coincide, puntero nulo, tiempo agotado), que no tienen causa que guardar. En vez de variantes `NotFound`/`AccessDenied`/`ExitCode` (que la UI no distingue y repartirían `message` entre más enums), esos 87 sitios usan constructores `CoreError::*_msg` (`trust_msg`, `service_msg`, `power_msg`, `dns_msg`…); así un `source: None` suelto vuelve a significar «causa perdida» y el techo de `tests/indicadores.rs` baja de 95 a 10 (solo los constructores). Prueba: `source_solo_cuando_hay_error_de_origen`. Verificado: `cargo test`, `clippy --all-targets -- -D warnings` y `fmt --check` en verde. Descartado: variantes por causa, por falta de un consumidor que las use.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): Más sitios con `source`; quedan 96 `source: None`, casi todos errores propios sin causa externa. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre parcial** (2026-11-25, rama `auditoria/buenas-practicas`): `Registry` y `Service` guardan ahora su origen como `BoxedSource` (antes `io::Error`, que no admitía un HRESULT de `windows`), y 20 sitios con el error en la mano (servicios, SetupAPI, ACL, DNS, elevación, hilos de la UI) pasan de `source: None` a `Some(Box::new(e))`. `source: None` baja de 115 a 95 (techo de `tests/indicadores.rs`); los que quedan son códigos de salida, estados de API y validaciones sin error de origen. Siguen sin hacerse las variantes por causa (`NotFound`, `AccessDenied`, `ExitCode`…). Verificado: `cargo test` (89 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="ar-07"></a>
### AR-07 · La carpeta de datos la resuelve la UI y sus nombres están repartidos
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: las vistas llaman a `ensure_admin_only_data_dir()` y construyen los managers; `OptimizationEngine` y `DnsManager`
  reciben la ruta, el Debloater la fija por dentro. `"staging"`, `"RegBackups"`, `"backup.json"`, `"dns_backup.json"` y
  `"debloat_backup.json"` están en cinco archivos, y la raíz vive en `trust.rs` mezclada con las descargas.
- **Dónde**: `src/ui/views/dashboard.rs:74,143`; `src/ui/views/dns.rs:25,132,179`; `src/core/debloater.rs:654-655`;
  `src/core/app_store.rs:681`; `src/core/dependencies.rs:229`; `src/core/uninstaller.rs:287`; `src/core/trust.rs:79-127`.
- **Propuesta**: `core/data_dir.rs` con `DataDir::open()` y métodos con nombre (`staging()`, `reg_backups()`,
  `optimization_backup()`…); `open_default()` en los managers; el Debloater recibe el `DataDir` como los demás.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): data_dir::staging_dir y reg_backups_dir; DnsManager/OptimizationEngine::open_default; las vistas ya no resuelven la carpeta de datos. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-08"></a>
### AR-08 · `revert_all_changes` vive en el desinstalador
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: orquesta motor, DNS y Debloater, y es la única razón de que `uninstaller` dependa de esos tres módulos. Esconde el
  `--rollback` del producto en un archivo de 1509 líneas.
- **Dónde**: `src/core/uninstaller.rs:18-21,1296-1365`.
- **Propuesta**: `core/rollback.rs` con `revert_all_changes` y `FullRevertReport`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): core/rollback.rs con revert_all_changes y FullRevertReport. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-09"></a>
### AR-09 · «Descargar, verificar y ejecutar un instalador» está escrito dos veces
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: la tienda y las dependencias repiten staging con nonce de `SystemTime` (el nonce aparece 8 veces en total),
  `DownloadPolicy`, `secure_download_and_verify` y borrado si falla, `reopen_verified_for_exec`, `run_in_job(…, 300 s)`,
  `exit_code != 1638` y borrado final. Es el camino del patrón 2: una corrección de seguridad tendría que hacerse dos veces.
  `updater.rs:361-380` repite la parte de descarga.
- **Dónde**: `src/core/app_store.rs:681-780`; `src/core/dependencies.rs:229-290`; `src/core/updater.rs:361-380`.
- **Propuesta**: `trust::download_to_staging(…) -> StagedFile` (su `Drop` borra) y `download_and_run(policy, args, opts) ->
  ProcessExit`; `const ERROR_PRODUCT_VERSION: u32 = 1638`; `fn unique_suffix()`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): StagedDownload (descarga, verifica y borra al soltarse) en tienda y runtimes; INSTALLER_TIMEOUT y ERROR_PRODUCT_VERSION. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-10"></a>
### AR-10 · Hay cinco formas de lanzar procesos
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `process::run_in_job` (Job y timeout); `debloater::run_ps` (lectores propios, mata solo al padre); `Command::output()`
  sin timeout en el escaneo del Debloater, con dos bloques casi iguales; `Command` con `reg.exe` en el desinstalador; `Command` en el
  updater. Las garantías del patrón 3 dependen de qué camino tome cada llamador. `get_powershell_path` duplica `system32_dir` y
  `CREATE_NO_WINDOW` se define dos veces.
- **Dónde**: `src/core/debloater.rs:21,522-624,667-722`; `src/core/uninstaller.rs:285,310`; `src/core/updater.rs:397`.
- **Propuesta**: `process::run_in_job_capture(launch, timeout, cancel) -> (ProcessExit, stdout, stderr)` y migrar PowerShell y
  `reg.exe`; `ps_json_query(script)` sobre ella; una sola constante.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): process::run_captured (pipes leídos, timeout) para PowerShell, escaneo de Appx y reg.exe; tests con cmd.exe. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-11"></a>
### AR-11 · Módulos de más de 900 líneas con responsabilidades mezcladas
- **Prioridad**: P2 · **Esfuerzo**: L · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa / propuesta** (particiones concretas, sin cambiar comportamiento, un módulo por vez):
  - `uninstaller.rs` (1509): `inventory.rs` (lista, tamaños), `runner.rs` (`parse_cmd_exe_and_args`, `run_uninstaller`),
    `leftovers.rs` (tokens, `scan_leftovers`, `is_still_registered`), `purge.rs` (purga y respaldo de claves); el rollback sale a
    `AR-08`.
  - `debloater.rs` (1188): `catalog.rs` (unas 390 líneas de datos), `appx.rs`; `run_ps` a `process` (`AR-10`); respaldo vía
    `AR-02`.
  - `archiver.rs` (1056): `paths.rs`, `staging.rs`, `progress.rs` y un archivo por formato con funciones privadas.
  - `sensors.rs` (1037): `nvml.rs`, `adl.rs`, `storage.rs`, `wmi.rs` y `mod.rs` con `SensorMonitor`.
  - `trust.rs` (913): `data_dir.rs`, `acl.rs`, `download.rs`, `authenticode.rs`.
  - `specs.rs` (909): `cpu.rs`, `smbios.rs`, `gpu.rs`, `storage.rs`.
  - `win_common.rs` (692): solo primitivas Win32; `HardwareProfile` a `specs`, batería y reinicio a `power`, y
    `parse_guid_strict`/`format_guid` de `power` a `win_common` (hoy `dns` importa `power` solo por eso).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Partidos win32, trust, uninstaller, debloater, archiver, sensors y specs; ningún archivo de core pasa de 830 líneas. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-12"></a>
### AR-12 · `main.rs` hace demasiado y los flags de la CLI son literales sueltos
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: confirmado · **Estado**: Hecho
- **Qué pasa**: `main.rs` contiene un componente Dioxus completo (`MiniArchiveProgressApp`, 200 líneas) que duplica worker, sondeo,
  cálculo de MB y ETA y textos del archivador; lanza el Explorador con Win32 directo y un `C:\Windows` fijo de respaldo aunque existe
  `win_common::windows_dir()`; y arma WebView2, el hook de panic y sus textos. Los flags se comparan como literales en `main.rs`, se
  repiten en `elevation.rs`, `tools.rs`, `dashboard.rs` y en las plantillas de `context_menu.rs`: renombrar uno rompe el menú
  contextual o el relanzamiento sin error de compilación (así nació `SEC-29`). `clean_archive_extract_dir` conoce `.tar.gz` pero no
  `.tgz`, que el core sí reconoce.
- **Dónde**: `src/main.rs:51,72-86,88-288,291-449`; `src/core/elevation.rs:13-21`; `src/ui/views/tools.rs:110-112`;
  `src/ui/views/dashboard.rs:183`; `src/core/context_menu.rs:31-68`; `src/core/archiver.rs:409`.
- **Propuesta**: `core/cli.rs` con `enum CliCommand` y `parse`/`as_flag`; la mini ventana a `ui/views/mini_archive.rs` compartiendo
  `start_archive_job` y un panel de progreso; `open_folder_in_explorer` a `sys_tools`; `main.rs` queda en unas 80 líneas.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): core/cli.rs con CliCommand y los flags (test de que cada flag tiene rama); main.rs de 528 a 90 líneas; mini ventana en views; startup.rs. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-13"></a>
### AR-13 · El ciclo «tarea bloqueante con progreso» está copiado seis veces y el `JoinError` se maneja de cinco maneras
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `Arc<Mutex<P>>` → `spawn_blocking` → `while !handle.is_finished() { lock; set; sleep }` aparece en archiver,
  cleaner, dashboard (dos), debloater y `main.rs`, con 100, 120 o 150 ms. El `JoinError` va por `unwrap_or_else`, por
  `.map_err(to_string)`, por `.ok()` y por `unwrap_or_default()` (un panic del worker se ve como «vacío»); `CoreError::Panic` casi
  no se usa. Las cargas iniciales usan `use_hook(spawn)`, `use_future` o `use_effect` sin criterio, ninguna `use_resource`. Un arreglo
  de concurrencia se aplicaría en una sola copia.
- **Dónde**: `src/main.rs:117-158`; `src/ui/views/archiver.rs:58-97`; `cleaner.rs:50-75`; `dashboard.rs:69-92,138-158`;
  `debloater.rs:146-170`; `src/ui/mod.rs:116,157,172,222,269,389-398`; `uninstaller.rs:96,171,223,287,321`; `dns.rs:73`.
- **Propuesta**: `ui/task.rs` con `run_blocking<T>(f) -> CoreResult<T>` (JoinError → `CoreError::Panic`) y
  `spawn_with_progress(signal, interval, f)`; `use_resource` para las cargas.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): ui/task.rs con run_with_progress y run_blocking en las seis vistas. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-14"></a>
### AR-14 · `ui/mod.rs` mezcla el shell, el estado global y el worker de instalación
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `ToolsSharedState` junta el progreso de instalación con el `tool_log` de RAM y menú contextual;
  `run_installer_worker` (175 líneas) vive en el módulo raíz; los 9 botones de navegación están copiados a mano; `dep_states` se
  calcula dos veces; el `impl Default` manual equivale a `#[derive(Default)]`.
- **Dónde**: `src/ui/mod.rs:73-99,109-114,126-301,165-170,452-499`.
- **Propuesta**: `ui/installer.rs` para worker y tipos; `tool_log` aparte; navegación desde `const TABS`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): ui/app.rs, ui/installer.rs, navegación desde ActiveTab::ALL, derive(Default). Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-15"></a>
### AR-15 · Reglas de negocio en las vistas
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: DNS arma el CSV y filtra adaptadores virtuales en la vista; el Debloater decide la preselección recomendada en la
  vista; Hardware decide ruta y nombre de la exportación.
- **Dónde**: `src/ui/views/dns.rs:125-141`; `src/ui/views/debloater.rs:15-33`; `src/ui/views/hardware_tab.rs:89-104`.
- **Propuesta**: `DnsManager::apply_preset_to_physical(&preset)` con error tipado; `DebloatScanResult::recommended_selection()`;
  usar `specs::export_specs_json`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): apply_preset_to_physical, recommended_selection, recommended_ids y export_to_desktop en core. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-16"></a>
### AR-16 · Números mágicos repetidos entre la UI y el core
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: cuota de extracción `50 GiB` en la vista y en `main.rs`; `total_steps: 6` en la vista del limpiador cuando el core
  cuenta como máximo 5; `total_steps: 13` en el Dashboard; `SensorMonitor::new(1000)` en la vista frente a 1500 en `AppConfig`;
  timeout de 300 s en tres sitios; hosts de GitHub repetidos en el catálogo y el updater.
- **Dónde**: `src/ui/views/archiver.rs:69`; `src/main.rs:127`; `src/ui/views/cleaner.rs:43`; `src/core/cleaner.rs:288`;
  `src/ui/views/dashboard.rs:58,127`; `src/ui/views/sensors.rs:18,45`; `src/core/app_store.rs:209-345,766`; `src/core/updater.rs:38`.
- **Propuesta**: constantes en core (`DEFAULT_EXTRACT_QUOTA`, `INSTALLER_TIMEOUT`, `DEFAULT_POLL_MS`, `GITHUB_HOSTS`); el progreso
  inicial con `total_steps: 0` y que el core lo rellene.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): DEFAULT_EXTRACT_QUOTA, INSTALLER_TIMEOUT, DEFAULT_POLL_MS; las vistas no inventan totales de pasos. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-17"></a>
### AR-17 · Textos de presentación generados en core
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Parcial
- **Qué pasa**: `specs::to_compact_summary` tiene etiquetas en español; `win_common` usa textos de relleno como valores («Placa Base
  Estándar», «Unidad Física {}»); `AppCategory::label(en)` es bilingüe y `DebloatCategory::label()` solo español: dos estrategias de
  idioma en core. Es el diseño que hay detrás de `UI-11` (diferido); este ID solo propone la regla para el código nuevo.
- **Dónde**: `src/core/specs.rs:133-160`; `src/core/win_common.rs:510,537,621,624`; `src/core/app_store.rs:55`;
  `src/core/debloater.rs:33`.
- **Propuesta**: core devuelve datos (`Option<String>`, enums) y la UI pone el texto (`IN-02`).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Va con UI-11 (diferido por el autor).
- **Cierre parcial** (2026-11-25): `CoreError::kind()` y `detail()`: la UI pone el título del error en el idioma del usuario y deja el detalle técnico (decisión del autor: traducir cada detalle va con `AR-06`). Faltan los catálogos con texto en core (ajustes del motor y sus categorías, Debloater, limpiador, restos, menú contextual, rellenos de `specs`): sus informes y su progreso llevan la etiqueta en español y no el ID, así que traducirlos pide cambiar esos informes; queda para un bloque propio.
- **Avance** (2026-11-25): `ui/i18n/core_text.rs` traduce en la UI los textos de los catálogos de core (ajustes del motor y sus categorías, apps, categorías y ajustes del Debloater, categorías del limpiador y de restos, pasos fijos del motor) más plantillas con un dato («Servicio '{}' detenido y deshabilitado»). El español de core es la clave, como en gettext: core no cambia y el test `catalogos_de_core_traducidos` recorre sus catálogos y falla si falta una traducción (comprobado quitando una). Inicio, Debloater, Limpiador y Desinstalador la usan. Siguen en español los detalles con datos que no cubren las plantillas y el detalle de cada error (`AR-06`). Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde; recorrido en la app (2026-11-25): Inicio, Debloater, Limpiador y Desinstalador en inglés se ven traducidos.

<a id="ar-18"></a>
### AR-18 · Tres versiones del crate `windows` y dependencias duplicadas
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: confirmado (`Cargo.lock`) · **Estado**: Parcial
- **Qué pasa**: `windows` 0.57 (por `sysinfo` 0.31), 0.58 (propia) y 0.61 (Dioxus y `webview2-com`); `windows-sys` en cuatro
  versiones; `thiserror` 1 y 2; `dirs` 5 y 6. `sysinfo` solo cuenta núcleos, RAM y marca del CPU, que `specs` ya lee. Más tiempo de
  compilación, binario más grande y tres juegos de bindings.
- **Dónde**: `Cargo.toml`; `Cargo.lock`; `src/core/win_common.rs:424-471`.
- **Propuesta**: quitar `sysinfo` (con `AR-04`); subir `windows` a 0.61, `thiserror` a 2 y `dirs` a 6, revisando los cambios de API.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Fuera sysinfo (y windows 0.57); thiserror 2 y dirs 6. Queda alinear windows 0.58 con 0.61, que cambia la API. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): windows 0.58 → 0.61 hecho en la rama `tmp/windows-061` (commit `db8c18c`): un solo crate `windows` en el árbol, clippy, tests y build release en verde. Falta unirla a esta rama. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="ar-19"></a>
### AR-19 · Tres estilos de API en core
- **Prioridad**: P3 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: structs unitarios con solo funciones asociadas (`SysTools`, `ArchiveManager`, `UninstallerManager`…), managers con
  estado (`DnsManager::new(dir)`) y funciones libres (`registry`, `services`, `power`). Cuesta descubrir la API y causa la diferencia
  de `AR-07`.
- **Propuesta**: funciones libres para lo que no tiene estado; struct solo cuando hay estado (`DataDir`, mutex).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Decisión de diseño; no se tocó.

<a id="ar-20"></a>
### AR-20 · La persistencia de tema e idioma está duplicada
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `i18n.rs:31-54` y `theme.rs:66-89` son el mismo código con otro nombre de archivo, con un `path.exists()` redundante
  y lectura sin límite, mientras `AppConfig` (sin uso) sí lee acotado.
- **Propuesta**: `UiPrefs { theme, lang }` en un solo `prefs.json`, con `read_bounded_bytes`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): ui/prefs.rs con lectura acotada para tema e idioma. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

# Código limpio y reutilizable (`CL-`)

<a id="cl-01"></a>
### CL-01 · 213 bloques `unsafe` sin ningún comentario `SAFETY`
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: confirmado · **Estado**: Hecha
- **Qué pasa**: 213 `unsafe {` (40 en `sensors.rs`, 31 en `trust.rs`, 29 en `win_common.rs`) y cero `SAFETY`; un solo `# Safety`.
  `resolve_proc<T>` hace `transmute_copy` a cualquier `T` sin comprobar tamaño ni documentar el contrato; dos `unsafe impl Send` sin
  justificar; `read_wmi_*` son `unsafe fn` que no exigen nada al llamador. `safe_fs::duplicate_file_handle` es una `fn` segura que
  hace `File::from_raw_handle` (posible doble cierre) y su nombre y comentario no corresponden a lo que hace. Con este volumen de FFI,
  el comentario es lo único que permite revisar un cambio sin releer la API Win32. La UI tiene 0 `unsafe`.
- **Dónde**: `src/core/sensors.rs:67,98,324,907,921`; `src/core/safe_fs.rs:485-488`; `src/core/win_common.rs:187`.
- **Propuesta**: `#![forbid(unsafe_code)]` en `src/ui/mod.rs` (gratis hoy); `clippy::undocumented_unsafe_blocks` y
  `missing_safety_doc` módulo por módulo, empezando por `trust`, `safe_fs`, `process` y `elevation`; `debug_assert` de tamaño en
  `resolve_proc`; `read_wmi_*` seguras; justificar los `Send`; borrar `duplicate_file_handle`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): forbid(unsafe_code) en la UI; SAFETY en los unsafe fn, resolve_proc con # Safety y debug_assert, Send justificados, fuera duplicate_file_handle; el código nuevo (http, wmi, profile) lleva SAFETY. Quedan bloques antiguos sin comentario. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): `undocumented_unsafe_blocks = "warn"` en todo el crate y SAFETY en todos los bloques. Solo `drivers.rs` queda con `allow` y motivo, porque ARQ-03 está diferido. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-02"></a>
### CL-02 · Wrappers RAII duplicados y liberaciones a mano
- **Prioridad**: P1 · **Esfuerzo**: S/M · **Evidencia**: confirmado (`WinInetHandle`) · **Estado**: Hecha
- **Qué pasa**: `WinInetHandle` definido igual en `trust.rs` y `updater.rs`; `OwnedHandle` en `specs.rs` duplica `SafeHandle`;
  `LocalFree` manual en seis sitios (un `?` insertado en medio fuga); `CoTaskMemFree` manual en `sensors.rs` aunque `dependencies.rs`
  ya usa `CoTaskMemPWSTR`; el descriptor desde SDDL está escrito dos veces.
- **Dónde**: `src/core/trust.rs:53-76,131-153,239,363`; `src/core/updater.rs:51-74`; `src/core/specs.rs:31-41`;
  `src/core/power.rs:113,183`; `src/core/win_common.rs:298-321`; `src/core/sensors.rs:932`.
- **Propuesta**: `WinInetHandle` en `win_common`; borrar `OwnedHandle`; `LocalMem<T>` con `Drop`;
  `SecurityDescriptor::from_sddl(&str)`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Un WinInetHandle, fuera OwnedHandle. Quedan los LocalFree manuales y el SDDL duplicado. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): `LocalMem` libera con LocalFree en Drop y `SecurityDescriptor::from_sddl` reemplaza el SDDL duplicado (con tests). Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-03"></a>
### CL-03 · Cliente HTTP de WinInet duplicado y user-agent desactualizado
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `updater.rs` repite casi línea por línea a `trust.rs`: `InternetOpenW`, los tres timeouts, `InternetOpenUrlW` con
  los mismos flags, estado 200 y bucle de lectura con tope. `trust.rs` manda `"OtterSwiss/3.0"` fijo; `updater.rs` usa
  `CARGO_PKG_VERSION`.
- **Dónde**: `src/core/updater.rs:184-280`; `src/core/trust.rs:438-600`.
- **Propuesta**: `core/http.rs` con `Session::open`, `open_secure(url) -> Response` (`status`, `final_url`, `content_length`) y
  `read_chunks(max, cancel, f)`; `trust` y `updater` quedan como consumidores.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): security/http.rs; user-agent desde Cargo.toml; el actualizador comprueba el host final. Consulta real a GitHub en verde. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-04"></a>
### CL-04 · La ubicación de registro de un resto se modela con dos booleanos y campos opcionales sueltos
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `LeftoverItem` lleva `path`, `reg_is_hkcu`, `reg_wow64_32`, `reg_subkey` y `reg_value_name`, válidos según `kind`
  (25 usos de los booleanos). La conversión a colmena y flags se reescribe cuatro veces y el `id` se arma con
  `format!("reg:{}:{}:{}", …)`. El `match` de colmena del Debloater se repite tres veces más.
- **Dónde**: `src/core/uninstaller.rs:60-74,888,1005,1155-1169,1195-1206`; `src/core/debloater.rs:959,1011,1080`.
- **Propuesta**: `RegLocation { hive: RegHive, view: RegView }` con `root()` y `flags(access)`;
  `enum LeftoverTarget { Path, RegKey, RegValue }` del que se deriva el `id`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): RegLocation reemplaza reg_is_hkcu/reg_wow64_32; test de vistas. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-05"></a>
### CL-05 · Funciones de unas 400 líneas
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Parcial
- **Qué pasa**: `apply_profile_inner` (387 líneas; el bloque «resultado → push → progreso» 15 veces y el booleano `backup_failed`
  consultado en tres sitios), `scan_leftovers` (389, nueve niveles de anidamiento, el literal `LeftoverItem` tres veces),
  `AdlSession::poll` (199), `execute_debloat`, `poll_wmi_acpi`, `secure_download_and_verify_with_progress` (261). Las vistas
  `UninstallerView` (524), `DashboardView` (515), `DebloaterView` (438), `DnsView` (364), `App` (344) y `ToolsView` (314). No se
  pueden probar por partes y cuesta revisar en ellas los patrones 1 y 6.
- **Dónde**: `src/core/engine.rs:194-583`; `src/core/uninstaller.rs:680-1071`; `src/core/sensors.rs:365-566,725-880`;
  `src/core/debloater.rs:756-916`; `src/core/trust.rs`; `src/ui/views/*.rs`.
- **Propuesta**: `StepRecorder` (`skip`, `fail`, `applied`) y una función por tipo de tweak que devuelva `ControlFlow`; un
  `scan_*` por fuente de restos; las vistas en subcomponentes (resultados, modal de restos, lista de apps).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): apply_profile_inner (StepRecorder) y scan_leftovers (LeftoverScan, salida idéntica en todas las apps de esta PC) partidos. Las vistas largas siguen igual. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre parcial** (2026-11-25, rama `auditoria/buenas-practicas`): la mayor de las vistas, `DriversView`, pasó de 825 a 303 líneas (`CL-21`); quedan `UninstallerView`, `DashboardView`, `DebloaterView`, `DnsView`, `App` y `ToolsView`, que no se tocaron en esta sesión.

<a id="cl-06"></a>
### CL-06 · Helpers Win32 escritos varias veces
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (`known_folder`) · **Estado**: Hecho
- **Qué pasa**: `known_folder` cuatro veces (una idéntica byte a byte en el desinstalador); System32 o Windows tres veces;
  `read_cstr_at_offset` y `read_cstr_at`; `to_wide_null` reescrito a mano en `specs.rs`. Son rutinas del patrón 5: cada copia es otro
  sitio donde un arreglo de seguridad puede no llegar.
- **Dónde**: `src/core/win_common.rs:73,543`; `src/core/uninstaller.rs:130`; `src/core/cleaner.rs:43`; `src/core/trust.rs:78`;
  `src/core/process.rs:40`; `src/core/debloater.rs:523-533`; `src/main.rs:77-83`; `src/core/specs.rs:751-754,839`.
- **Propuesta**: un solo `known_folder(id) -> CoreResult<PathBuf>`, `system32_dir()` y `read_cstr_at` en `win_common`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Un known_folder, un read_cstr_at, get_program_data_dir sobre known_folder. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-07"></a>
### CL-07 · El reporte de progreso por pasos está copiado en core
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: el mismo bloque `lock → current_step, total_steps, label, percent` cuatro veces, con tope `min(100)` o `min(99)`;
  `CleanProgress`, `DebloatProgress` y `EngineProgress` repiten los cuatro campos.
- **Dónde**: `src/core/engine.rs:221,618`; `src/core/debloater.rs:770`; `src/core/cleaner.rs:290`.
- **Propuesta**: `struct StepProgress` embebido y `fn report_step(progress, idx, total, label)`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): core::progress::step_percent común; los tres structs de progreso siguen separados. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): Trait `StepProgress` con `report_step`, implementado por los tres progresos de limpieza, debloat y motor. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-08"></a>
### CL-08 · Apertura del SCM y del servicio duplicada
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Dónde**: `src/core/services.rs:42-63,223-245`.
- **Propuesta**: `open_service(name, access) -> CoreResult<Option<ScHandle>>` sin lista blanca, y que `open_allowed_service` valide
  y la llame.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): open_service común. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-09"></a>
### CL-09 · Consulta WMI duplicada
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `ConnectServer`, `CoSetProxyBlanket`, `ExecQuery` y el bucle `Next` dos veces; décimas de kelvin a grados tres veces.
- **Dónde**: `src/core/sensors.rs:735-800,803-870,774,784,832`.
- **Propuesta**: `wmi_query(locator, ns, wql) -> Vec<IWbemClassObject>` y `decikelvin_to_c`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): wmi_query y decikelvin_to_c. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-10"></a>
### CL-10 · Cuatro modelos distintos de «mensaje de estado» en la UI
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `Option<(String, Tone)>` (desinstalador, DNS); `Option<String>` más `is_error: bool` (archivador, con badge propio
  sin `role`); `error_banner` y `status_banner` a la vez (Debloater); `tool_log` con el tono deducido por
  `msg.starts_with("Error")`; Hardware muestra errores en color de acento.
- **Dónde**: `src/ui/views/uninstaller.rs:27`; `dns.rs:49`; `archiver.rs:25-26,272-276`; `debloater.rs:86`; `tools.rs:229`;
  `hardware_tab.rs:23,72`.
- **Propuesta**: `struct Notice { kind, tone }` (con `IN-03`, el `kind` es un evento y no texto) y siempre `StatusBanner`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Archivador con StatusBanner; tool_log guarda (texto, Tone). Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-11"></a>
### CL-11 · Faltan componentes y el mismo `rsx!` se repite
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: barra de progreso siete veces (con pistas `--surface` o `--bg` y transiciones de 0,15, 0,2 o 0,25 s, sin
  `role="progressbar"`); fila con checkbox seis veces; cabecera de tarjeta dos veces en `components`; Hardware arma `div.card` a mano;
  el Dashboard copia el markup de `Card` porque no admite acciones.
- **Dónde**: `src/ui/mod.rs:583`; `src/main.rs:248`; `archiver.rs:157`; `cleaner.rs:157,167,229`; `dashboard.rs:313-320,405`;
  `debloater.rs:378,428,473`; `tools.rs:218,446`; `uninstaller.rs:567`; `components/mod.rs:7-13,127-133`;
  `hardware_tab.rs:129-213`.
- **Propuesta**: `ProgressBar { percent }` accesible, `CheckRow`, `CardHeader`, y `Card` con `actions` y `tone` (ver `TE-12`).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): ProgressBar con role=progressbar en las seis barras. CheckRow y CardHeader no: las filas difieren demasiado; se resolvió con clases (select-row, list-row). Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `CardHeader` (`ui/components/card_header.rs`) es la cabecera de `Card` y `Modal`, que la tenían copiada, y `Card` admite `actions` (el aviso «Se necesitan derechos de administrador» del Dashboard dejó de copiar el markup). `CheckRow` llegó con `CL-25` y la barra con `role=progressbar` ya estaba. Las tarjetas de especificaciones de Hardware no son `Card` (sin título de sección) y usan `.card-bar`/`.tile` (`TE-02`). Verificado: `cargo test` (92 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; recorrido en la app sin elevar (2026-11-25): Dashboard con el aviso de administrador y sus acciones, Limpiador y Debloater con `CheckRow`, en oscuro y claro, español e inglés, sin diferencias.

<a id="cl-12"></a>
### CL-12 · Formato de tamaños y duraciones en tres versiones
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `format_mb`, `format_size_mb` y `format_capacity`, más divisiones por `1024.0 * 1024.0` sueltas; los formatos
  divergen («< 0.01 MB» y «0.00 MB»). El ETA `"~{}s"` está dos veces y da «~3600s».
- **Dónde**: `src/ui/views/cleaner.rs:11`; `uninstaller.rs:43`; `hardware_tab.rs:251`; `archiver.rs:102,121-126`;
  `src/main.rs:203-208`.
- **Propuesta**: `ui/format.rs` con `bytes(n)` y `duration(secs)` (con sus tests; ya hay en `hardware_tab.rs:267`).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): ui/format.rs con tests. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-13"></a>
### CL-13 · Strings mágicos donde va un enum
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: el modo del archivador es `use_signal(|| "extract")` comparado en siete sitios, y `MiniArchiveJob` usa
  `is_extract: bool`; los ids de runtime (`"webview2"`, `"vcredist_x64"`) son strings.
- **Dónde**: `src/ui/views/archiver.rs:19-215`; `src/main.rs:24,370`; `src/core/dependencies.rs:206-211,291`.
- **Propuesta**: `enum ArchiveMode` y `enum RuntimeId` en core. Los flags de CLI van en `AR-12`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): ArchiveMode en core y en las dos vistas. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-14"></a>
### CL-14 · Dos `too_many_arguments` que tapan un diseño mejorable
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Dónde**: `src/ui/views/uninstaller.rs:141` (10 parámetros, 8 `Signal`); `src/core/archiver.rs:332` (8).
- **Propuesta**: `#[derive(Clone, Copy)] struct UninstallerSignals`; `struct ExtractCtx`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Los dos allow llevan motivo; agrupar en structs queda pendiente.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): `UninstallerSignals` agrupa las señales; fuera los dos `too_many_arguments`. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-15"></a>
### CL-15 · Rutas tratadas como `String` con pérdida
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `wide_ptr_to_string_and_free` devuelve `String` lossy también para rutas; `to_string_lossy().to_ascii_lowercase()` se
  usa como clave de identidad de ruta (no pliega mayúsculas fuera de ASCII); el menú contextual escribe la ruta lossy en el registro.
- **Dónde**: `src/core/win_common.rs:53`; `src/core/uninstaller.rs:358,397,809,1014`; `src/core/archiver.rs:402`;
  `src/core/context_menu.rs:87,117`.
- **Propuesta**: `OsString::from_wide` para rutas, un `path_key(&Path)` único y `OsStr` hasta el borde.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): `wide_ptr_to_path_and_free` devuelve `OsString` y `path_key` normaliza las comparaciones; el menú contextual rechaza rutas no UTF-8 en vez de perderlas. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-16"></a>
### CL-16 · Detalles sueltos
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- `context_menu.rs:88-89`: `cmd.contains(&expected) || cmd.contains(&current_exe)`; la primera condición sobra.
- `main.rs:320`: `native_message(…, ask_yes_no: bool, is_error: bool)`; mejor enums o dos funciones.
- `format!("{}", e)` en vez de `e.to_string()` (4 casos); 7 `redundant_clone`.
- Clones completos en cada render: `uninstaller.rs:~410` (`filtered`) y `debloater.rs:115`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): native_message con ask_yes_no/show_warning/show_error; clones redundantes fuera. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): Fuera el `get_known_folder` propio del limpiador (usa `platform::folders`), `ProcessIdToSessionId` del crate en vez de un extern a mano y una rama imposible del desinstalador. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-17"></a>
### CL-17 · Comentarios
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: los 92 comentarios con ID de `BUGS.md` dan trazabilidad, pero algunos cuentan la historia en vez de la regla
  (`components/mod.rs:23`); «Transversal 6» no se define en ningún sitio (`backup.rs:119`, `tweaks_catalog.rs:107`); ocho archivos
  abren con `// src/…`, que repite la ruta.
- **Propuesta**: la regla primero y el ID entre paréntesis; definir o quitar «Transversal»; borrar las cabeceras de ruta.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): «Transversal 6» corregido y cabeceras de ruta quitadas. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="cl-18"></a>
### CL-18 · Tests: carpetas temporales sin limpieza y un test de sistema monolítico
- **Prioridad**: P3 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: dos helpers de carpeta temporal duplicados y sin guarda RAII (si una aserción falla, queda en `%TEMP%`);
  `vm_sistema.rs` usa nombres fijos (`otter-arch`) y es un solo `#[test]` de 480 líneas, así que no se puede correr un área sola.
  Funciones puras que viven en vistas (`uninstall_exit_is_success`, `recommended_ids`, `format_*`) no se pueden probar sin Dioxus.
- **Dónde**: `src/core/archiver.rs:976`; `src/core/safe_fs.rs:518`; `tests/vm_sistema.rs:275,303,468`.
- **Propuesta**: `test_util::TempDir` con `Drop` (o `tempfile` como dev-dependency); un `#[test] #[ignore]` por área con helper
  común; mover esas funciones a core (`AR-01`, `AR-15`).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): TempDir con Drop; vm_sistema en siete pasos con nombre único. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

# Temas, tokens y CSS (`TE-`)

Contrastes calculados con la fórmula WCAG sobre los hex de `assets/main.css` y recalculados por el agente principal. Los defectos
de contraste de texto están en `UI-18` (`docs/BUGS.md`).

<a id="te-01"></a>
### TE-01 · Sin `color-scheme` ni `accent-color`: barras de desplazamiento y checkboxes claros en los temas oscuros
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: confirmado (0 resultados de `color-scheme`) · **Estado**: Hecho
- **Qué pasa**: WebView2 dibuja scrollbars, checkboxes e inputs en esquema claro si no se declara otro: gris claro y azul de
  Windows sobre fondos casi negros, y el checkbox no sigue el acento del tema.
- **Dónde**: `assets/main.css` (bloques `[data-theme]`); listas con scroll en `:295,441,449`; checkboxes en `cleaner.rs`,
  `debloater.rs`, `uninstaller.rs`.
- **Propuesta**: `color-scheme: dark` en los cinco temas oscuros y `light` en Claro; `accent-color: var(--accent)`;
  `scrollbar-color: var(--border) transparent`. Depende de `TE-04` para que llegue a `html`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): color-scheme por tema, accent-color y scrollbar-color. Revisado en la app (scroll oscuro y claro). Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-02"></a>
### TE-02 · 185 estilos inline: el CSS no tiene clases de layout ni de componente
- **Prioridad**: P1 · **Esfuerzo**: M/L · **Evidencia**: confirmado (185 `style:`) · **Estado**: Hecha
- **Qué pasa**: `flex column gap-2` 19 veces, `xs muted` 19, fila con `border-bottom` 9, botón chico 9, inputs con estilo completo
  3. UI-10 pasó los valores a tokens pero los dejó inline: un cambio de diseño toca de 7 a 19 sitios, las copias ya divergen y el
  inline no admite `:hover`, `:disabled`, `:focus-visible` ni media queries. `AGENTS.md:10` dice que `main.css` concentra los
  estilos.
- **Propuesta**: capa de utilidades (`.stack-2`, `.stack-3`, `.row`, `.row-between`, `.wrap`, `.text-sm`, `.text-muted`) y de
  componentes (`.progress`, `.list-row`, `.input`, `.btn-sm`, `.card-kicker`, `.select-row`); migrar vista por vista junto con
  `CL-11`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): 110 de 161 estilos inline pasaron a clases; quedan 51, sobre todo valores calculados o únicos. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `style:` en `src/` baja de 45 a 2: los dos anchos calculados de las barras de progreso (`ProgressBar` y el archivador), que son valores en tiempo de ejecución. Clases nuevas en `assets/main.css`: `.list-row.dense|.padded`, `.card-bar`, `.card-warning`, `.heading-flat`, `.card-grid`, `.tile`, `.kv-row`, `.kv-value`, `.info-box`, `.code-box`, `.mt-2`, `.mr-2`, `.gap-4`, `.break-all`; el resto usa las utilidades existentes (`stack-*`, `row-between`, `text-*`, `item-row`, `btn-sm`). El techo de `style:` en `tests/indicadores.rs` baja de 45 a 2. Verificado: `cargo test` (78 + 4, con `tests/temas.rs` sobre clases usadas y tokens), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="te-03"></a>
### TE-03 · No hay opción «según el sistema» y el selector de tema es un ciclo de seis
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: sin `theme.json` la app abre en Oscuro; se ignoran `prefers-color-scheme`, `AppsUseLightTheme` y el contraste alto.
  Para ir de Oscuro a Claro hay que pulsar cinco veces el mismo botón.
- **Dónde**: `src/ui/theme.rs:7,28-37,80`; `src/ui/mod.rs:508-515`.
- **Propuesta**: `Theme::System` por defecto; un menú de temas en vez del ciclo; `@media (forced-colors: active)`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Theme::System por defecto y selector; probado en la app. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-04"></a>
### TE-04 · Los tokens viven en un `div` y `:root` no tiene colores
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `data-theme` está en `div.app-root-frameless`, no en `<html>`; por eso `html, body { color: var(--text) }` no hace
  nada y `color-scheme` no puede leer el tema. `rgba(var(--accent-rgb, 59, 130, 246), …)` lleva un fallback fijo que solo existe
  porque `:root` no define colores; un `data-theme` desconocido deja la app sin colores.
- **Dónde**: `assets/main.css:18,157,241,328`; `src/ui/mod.rs:426`; `src/main.rs:215`.
- **Propuesta**: poner el atributo en `document.documentElement`, o `:root, [data-theme="dark"] { … }` como respaldo; quitar los
  fallbacks literales.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): data-theme en <html> y en la raíz; :root con el tema oscuro de respaldo. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-05"></a>
### TE-05 · Un solo nivel de tokens y `--bg` copiado a mano en Rust
- **Prioridad**: P2 · **Esfuerzo**: S (test) / M (capas) · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: cada tema define 18 colores semánticos con hex directo, sin paleta primitiva ni tokens de componente.
  `background_rgba` repite los seis `--bg` y nada comprueba que coincidan: si se desincronizan vuelve el parpadeo de UI-10.
  `--accent-rgb` duplica `--accent` cuando ya se usa `color-mix`.
- **Dónde**: `assets/main.css:19-148,404-407`; `src/ui/theme.rs:40-48`.
- **Propuesta**: test que lea `include_str!("../../assets/main.css")` y compare `--bg` con `background_rgba()`; quitar
  `--accent-rgb`; a mediano plazo `--control-bg`, `--control-border`, `--track-bg`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Test fondo_inicial_igual_al_css; fuera --accent-rgb; tokens de control. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-06"></a>
### TE-06 · Estados de interacción casi invisibles
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: derivado · **Estado**: Hecho
- **Qué pasa**: `--surface-hover` contra `--surface` da 1,05 en Claro y 1,08–1,18 en el resto: el hover del menú y de los botones
  de ventana casi no se ve. `.btn-danger:hover` usa `opacity: 0.9` en vez de un token. Ningún botón tiene `:active`. (El contraste
  del texto en el hover primario está en `UI-18`.)
- **Dónde**: `assets/main.css:234,263,283,366-368,386`.
- **Propuesta**: `--surface-active` y `--danger-hover` con diferencia perceptible; `:active` en botones.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): --surface-hover/-active recalculados, --danger-hover, :active. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-07"></a>
### TE-07 · Fondos blancos translúcidos fijos que desaparecen en Claro
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Dónde**: `assets/main.css:210` (`.top-version`), `:251` (`.theme-toggle-btn`).
- **Propuesta**: `color-mix(in srgb, var(--text) 6%, transparent)` o un `--surface-subtle` por tema.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): --surface-subtle. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-08"></a>
### TE-08 · Bordes de controles por debajo de 3:1 (WCAG 1.4.11)
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: derivado · **Estado**: Hecho
- **Qué pasa**: `--border` sobre `--surface` va de 1,21 (Oscuro) a 1,52 (Esmeralda) y es el único borde de inputs,
  `.btn-secondary` y filas seleccionables: los campos de texto apenas se distinguen.
- **Propuesta**: `--border` decorativo como está y un `--control-border` ≥ 3:1 para controles.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): --control-border ≥ 3:1 en los seis temas. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-09"></a>
### TE-09 · Sin `prefers-reduced-motion` y logo animado en bucle
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (0 `@media`) · **Estado**: Hecho
- **Dónde**: transiciones en `assets/main.css:230,259,280,323,358` y en las barras inline; `logo.gif` (30 fotogramas) en
  `src/ui/mod.rs:346-350`.
- **Propuesta**: `@media (prefers-reduced-motion: reduce)` que anule transiciones y animaciones; marca estática bajo `reduce`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): prefers-reduced-motion. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-10"></a>
### TE-10 · Logo de 1,7 MB en base64 para mostrarse a 30×30
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (1 716 706 bytes) · **Estado**: Hecho
- **Qué pasa**: `logo.gif` (640×360) va codificado en base64 en el binario y el DOM (unos 2,3 MB de texto) para un icono de 30px
  recortado de 16:9 a cuadrado. El README lo pinta a 128×128 y lo deforma.
- **Dónde**: `src/ui/mod.rs:349`; `assets/main.css:193-198`; `README.md:2`.
- **Propuesta**: `brand-32.png`/`brand-64.png` o un SVG cuadrado de pocos KB con `asset!()`; el GIF solo para el README.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): icon-64.png de 4,7 KB; el GIF solo en el README. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-11"></a>
### TE-11 · Sin reset base: márgenes del navegador y sin `box-sizing`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (la mini ventana: inferido) · **Estado**: Hecho
- **Qué pasa**: el `<h3>` de `Card` conserva `margin-block: 1em` y `hardware_tab.rs:63` lo compensa a mano: ritmo vertical distinto
  entre tarjetas. La mini ventana suma `border: 1px` a `100vw × 100vh` con `content-box`, así que probablemente recorta los bordes
  derecho e inferior.
- **Dónde**: `assets/main.css:166-167,337`; `src/main.rs:216`.
- **Propuesta**: `*, *::before, *::after { box-sizing: border-box }`, `h1–h4, p { margin: 0 }`, espaciado con `gap`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): box-sizing y márgenes a cero. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-12"></a>
### TE-12 · Componentes que se esquivan: `Card`, `StatusBanner` y `badge`
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: Hardware arma `div.card` con un padding redundante; Archivos y la mini ventana usan `badge` (pastilla de 999px) como
  mensaje de varias líneas sin `role`; el botón de actualización mezcla `badge-warning` con borde de acento; Herramientas pinta una
  tarjeta de aviso inline.
- **Dónde**: `src/ui/views/hardware_tab.rs:129,155,174,197,213`; `archiver.rs:273-276`; `src/main.rs:240`; `src/ui/mod.rs:443-444`;
  `tools.rs:208-211`.
- **Propuesta**: `Card` con `actions` y `tone`, `StatusBanner` en Archivos y la mini ventana, `.btn-sm`/`.chip`, `.card-warning`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Sin padding redundante en .card; StatusBanner en archivador y mini ventana; botón de actualización como badge con borde propio. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-13"></a>
### TE-13 · Controles segmentados y pestañas sin estado accesible
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: la opción elegida se marca alternando `btn-primary`/`btn-secondary` y la pestaña activa solo con la clase `active`; 0
  `aria-pressed` y 0 `aria-current`. El estado solo se comunica con color, y un primario de llamada a la acción se confunde con una
  selección.
- **Dónde**: `src/ui/views/archiver.rs:182,188,236-248`; `uninstaller.rs:706-724`; `dns.rs:302`; `sensors.rs:121`;
  `src/ui/mod.rs:453-496`.
- **Propuesta**: `.segmented > button[aria-pressed="true"]` y `aria-current="page"`; estilar por atributo.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): aria-pressed en los grupos de opciones; aria-current en la pestaña activa. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-14"></a>
### TE-14 · Objetivos táctiles chicos y tipografía fuera de la escala
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `.theme-toggle-btn` mide unos 21px de alto y los botones chicos inline unos 22px (WCAG 2.5.8 pide 24px); 11px fijos
  en `.top-version`, `.top-nav-item` y `.badge` (menos que `--font-size-xs`); 14px y 12px fijos en vez de tokens; `--font-size-lg`
  no se usa.
- **Dónde**: `assets/main.css:201,207,227,253-255,277,400`; `tools.rs:312`; `uninstaller.rs:707-725`.
- **Propuesta**: `min-height: 28px` en controles; `--font-size-2xs` o subir a 12px; usar los tokens y borrar los sin uso.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): --control-min-h 28px, --font-size-2xs y tokens. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-15"></a>
### TE-15 · La barra superior probablemente no entra a 960px
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: inferido (estimado unos 1150px con los textos en español) · **Estado**: Hecho
- **Qué pasa**: nueve botones con `white-space: nowrap`, sin `overflow`, `flex-shrink` ni media queries, y ventana mínima de 960px.
  Se cruza con la prueba pendiente de la ventana en tamaño chico.
- **Dónde**: `src/main.rs:482`; `assets/main.css:215-232`.
- **Propuesta**: verificarlo en la app; `@media (max-width: 1150px)` que oculte la versión y reduzca el padding, o `overflow-x: auto`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): overflow-x en la barra y media query a 1150px. Sin verificar a 960px exactos. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-16"></a>
### TE-16 · `!important` y `pointer-events` sin motivo
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: 10 `!important`; tres `pointer-events: auto !important` sin ningún `none` que anular; cinco en `html, body`; dos en
  el hover del botón de cerrar, que se resuelve con especificidad.
- **Dónde**: `assets/main.css:151-158,171,289-290,298,324`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-17"></a>
### TE-17 · Valores sueltos sin token
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: z-index 100 y 9999; `0.15s ease` siete veces más 0,2 y 0,25 inline; pesos 400–700 y `bold`; radios de 999px, 8px y
  4px fuera de `--radius-*`; `#e81123` del botón de cerrar; overlay `rgb(0 0 0 / 0.72)`; medidas de layout; `monospace` y
  `min-width: 220px` inline.
- **Propuesta**: `--z-titlebar`, `--z-modal`, `--duration-fast`, `--ease`, `--fw-*`, `--radius-pill`, `--overlay`, `--font-mono`,
  `--win-close`, `--titlebar-h`, `--content-max`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-18"></a>
### TE-18 · `transition: all`
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Dónde**: `assets/main.css:230,358`.
- **Propuesta**: listar `background-color`, `border-color`, `color`, `opacity`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-19"></a>
### TE-19 · Pila de fuentes pensada para la web y glifos de ventana con caracteres de texto
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Dónde**: `assets/main.css:2` (`-apple-system, BlinkMacSystemFont, Roboto…`); `src/ui/mod.rs:519-530`; `src/main.rs:233`
  («─», «□», «✕»).
- **Propuesta**: `"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`; iconos con `Segoe Fluent Icons`/`Segoe MDL2 Assets`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-20"></a>
### TE-20 · `user-select: none` global impide copiar rutas y errores
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: no se pueden copiar rutas de restos, mensajes de error, notas de versión ni el comando del Dashboard.
- **Dónde**: `assets/main.css:159`; `uninstaller.rs:589`; `src/ui/mod.rs:573-577`; `dashboard.rs:359`.
- **Propuesta**: `none` solo en la barra de título y botones; `text` en `.card`, `code`, `.modal-list`, `.status-banner`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-21"></a>
### TE-21 · Nord: `--text-muted` casi igual a `--text`
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: derivado (1,17:1 entre ambos; 2–3,75 en los demás temas) · **Estado**: Hecho
- **Dónde**: `assets/main.css:89-90`.
- **Propuesta**: un `--text-muted` más apagado que siga ≥ 4,5:1 sobre `--surface`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-22"></a>
### TE-22 · CSS inyectado como `<style>` en dos sitios y la mini ventana sin color de fondo inicial
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado (el destello: inferido) · **Estado**: Hecho
- **Dónde**: `src/ui/mod.rs:422`; `src/main.rs:211,465-469,490`.
- **Propuesta**: cargar el CSS una vez (`with_custom_head` o `document::Stylesheet` con `asset!()`);
  `.with_background_color(Theme::load_persisted().background_rgba())` en la mini ventana.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): CSS una vez en el head; color de fondo en la mini ventana. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-23"></a>
### TE-23 · `main.css` sin secciones
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Propuesta**: cabeceras para tokens, temas, reset y base, layout, componentes, utilidades, accesibilidad y medios; la regla de
  foco (`:420`) sale de entre los componentes.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="te-24"></a>
### TE-24 · Deshabilitado solo con opacidad y cursor de mano en labels deshabilitados
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Dónde**: `assets/main.css:390`; `cleaner.rs:157`; `debloater.rs:428`.
- **Propuesta**: `.btn:disabled` con fondo y texto propios; `label:has(input:disabled) { cursor: not-allowed; opacity: .6 }`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Hecho como propone. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

# Idiomas (`IN-`)

<a id="in-01"></a>
### IN-01 · El idioma por defecto es español fijo y hay dos fuentes de idioma
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: confirmado · **Estado**: Hecho
- **Qué pasa**: sin `lang.json`, `load_persisted` devuelve `Es`. El instalador detecta el idioma del sistema pero no se lo pasa a
  la app, y el core usa `GetUserDefaultUILanguage` para la tienda. Un usuario con Windows en inglés instala en inglés y la app abre
  en español; con la app en inglés y Windows en español, la tienda baja Firefox en español.
- **Dónde**: `src/ui/i18n.rs:35-46`; `otterswiss.iss:24-26`; `src/core/app_store.rs:39-43`.
- **Propuesta**: `load_persisted() -> Option<Lang>` y, sin archivo, `Lang::from_system()` (la función de `app_store`); decidir y
  documentar si la tienda sigue el idioma de la app o el del sistema.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Lang::from_system; la tienda sigue el idioma de Windows (documentado). Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="in-02"></a>
### IN-02 · No hay catálogo de textos: 253 `if en {…} else {…}` y `en: bool` por todas partes
- **Prioridad**: P2 · **Esfuerzo**: L · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: 253 `if en` (Dashboard 40, desinstalador 36, Herramientas 35…), 9 `match (…, en)`, 17 campos hermanos `_es`/`_en`,
  `label(en: bool)` en core; `i18n.rs` no tiene ni una cadena. Lo bueno: el compilador garantiza que existen las dos variantes. Lo
  malo: «Cancelar» está 10 veces, un tercer idioma toca unos 280 sitios, `bool` en vez de `Lang`, y cada idioma duplica el `format!`
  con argumentos posicionales que pueden desordenarse en uno solo.
- **Dónde**: `src/ui/views/*.rs`; `src/ui/mod.rs`; `src/main.rs`; `src/core/app_store.rs:55,84-85`; `src/core/dns.rs:41`;
  `src/ui/theme.rs:51`.
- **Propuesta**: `struct Strings` con un campo por texto y dos `static` (`ES`, `EN`) en `src/ui/i18n/{es,en}.rs`: si falta un
  campo no compila, así que se conserva la garantía. `lang.s()` en las vistas; textos con parámetros como métodos con argumentos
  nombrados; core expone enums e IDs. Frente a `rust-i18n` o `fluent`: ambos resuelven las claves en ejecución y una faltante no
  avisa al compilar; con dos idiomas y unas 400 cadenas el struct es más simple y más seguro. Migrar de a una vista, empezando por
  los botones comunes. Es independiente de `UI-11` (diferido).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): ui/i18n/strings.rs con garantía en compilación y 36 textos repetidos migrados; los textos únicos siguen como if en, también exigidos en los dos idiomas. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre parcial** (2026-11-25, rama `auditoria/buenas-practicas`): cada vista tiene su módulo en `src/ui/i18n/` (`archiver`, `cleaner`, `dashboard`, `debloater`, `dns`, `hardware`, `sensors`, `tools`, `uninstaller`, `mini_archive` y `shell` para el aviso de actualización y la cola) y se usa como `strings(en).<vista>.<texto>`; los textos con datos son `fn(&dyn Display…)` con marcadores con nombre. `if en` baja de 216 a 20 (techo de `tests/indicadores.rs`): quedan los diálogos previos a Dioxus (`main.rs`, `startup.rs`), `AppCategory::label(en)` en core, los pares bilingües de datos (`privacy_notice_en`, `description_en`) y falsos positivos como `if entry`. Verificado: `cargo test` (89 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; los textos dentro de `Msg::new(move |en| …)` usan el `en` de la clausura. Recorrido en la app sin elevar (2026-11-25): Inicio, Limpiador, Debloater, DNS, Drivers, Desinstalador, Hardware, Sensores y Herramientas y tienda en oscuro y español; las mismas en inglés y claro sin textos cortados ni mezclados. Sigue en español el detalle de errores de core (p. ej. el aviso del Debloater sin administrador), que es `AR-06`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): lo que faltaba. Los diálogos nativos de `startup.rs` (WebView2 que falta, error inesperado) y los títulos de ventana de `main.rs` salen de `ui/i18n/shell.rs`; `AppCategory::label` devuelve el español y la interfaz lo traduce con `tr` (sus 6 traducciones están en `core_text.rs` y las cubre `catalogos_de_core_traducidos`); los pares bilingües de DNS y de la tienda se eligen con `i18n::pick`. `if en` queda en 10 (techo de `tests/indicadores.rs`): `strings()`, `pick()` y falsos positivos como `if entry`. Verificado: `cargo test`, `clippy --all-targets -- -D warnings` y `fmt --check` en verde; títulos y diálogos nativos sin recorrer (el de WebView2 solo sale si falta).

<a id="in-03"></a>
### IN-03 · El estado guarda mensajes ya traducidos
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: los avisos se guardan como `String` con el idioma capturado al crearlos; si el usuario cambia de idioma, los que ya
  están en pantalla quedan en el anterior. Es otra fuente de mezcla de idiomas.
- **Dónde**: `src/ui/views/uninstaller.rs:27`; `debloater.rs:84`; `dns.rs:49`; `src/ui/mod.rs:132,181,197,278`.
- **Propuesta**: guardar el evento (`enum StatusMsg { DnsApplied { provider, count }, … }`) y traducirlo al renderizar (con `CL-10`).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Los avisos siguen guardando el texto ya traducido.
- **Observado** (2026-11-25): en Drivers, «237 dispositivos detectados.» sigue en español después de pasar a inglés; el resto de la
  pestaña sí cambia. Pasa igual en todas las vistas que guardan avisos.
- **Cierre** (2026-11-25, decisión del autor): `ui/i18n/msg.rs` con `Msg`, que guarda cómo armar el texto (`Fn(en) -> String`) y lo arma al dibujarse. Todos los avisos de las vistas (Archivador, mini ventana, Limpiador, Inicio, Debloater, DNS, Drivers, Hardware, Desinstalador, Herramientas y su cola, y la ventana principal) guardan `Msg` en vez de `String`; `ui::task` devuelve `Msg`. Test `el_aviso_cambia_de_idioma`. Verificado: `cargo test` (76 + 4 de integración), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; en la app sin elevar, «236 dispositivos detectados.» pasa a «236 devices detected.» al cambiar de idioma, y el error del Debloater sin admin pasa de «Administrator rights required: …» a «Hace falta ejecutar como administrador: …».

<a id="in-04"></a>
### IN-04 · Plurales con «(s)»
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Dónde**: `src/ui/mod.rs:198,200,286,287`; `src/ui/views/dns.rs:153,155,191,193`; `uninstaller.rs:121,127` (y en `ui/mod.rs:281,290`
  sí se concuerda).
- **Propuesta**: `plural(n, one, other)` o métodos de `Strings`; para ES y EN alcanza `n == 1`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): plural() en DNS, instalador y desinstalador. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="in-05"></a>
### IN-05 · Títulos de ventana y diálogo de error solo en español
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: «OtterSwiss — Archivador» y «OtterSwiss — Suite Nativa Win32» se ven en la barra de tareas y Alt+Tab; el diálogo de
  `panic` solo está en español, mientras `ensure_webview2`, en el mismo archivo, es bilingüe.
- **Dónde**: `src/main.rs:417,423-424,464,480`.
- **Propuesta**: leer el idioma antes de crear la ventana y el hook, y sacar los textos de `Strings`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Títulos de ventana y diálogo de error según el idioma. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="in-06"></a>
### IN-06 · Calidad de traducción y falta de guía de estilo
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: «Refresh» y «Update» se traducen ambos «Actualizar» en la misma vista; mayúsculas de título al estilo inglés
  («Escanear Rastros», «Bloatware Común y Telemetría (Recomendado)»); 41 «...» y ningún «…»; «ETA» sin traducir; el separador decimal
  es siempre «.» (válido en América, no en España).
- **Dónde**: `src/ui/views/uninstaller.rs:486,490-502`; `debloater.rs:452`; `archiver.rs:164`; `src/main.rs:255`.
- **Propuesta**: «Recargar lista» para Refresh; una guía breve en `AGENTS.md` (mayúscula solo inicial, «…», glosario); con
  `Strings`, un test que compare ES y EN (vacías, idénticas o con distinto número de `{}`).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): «…», «Recargar lista», mayúscula inicial, «Restante», «restos»; test del catálogo. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="in-07"></a>
### IN-07 · El documento no declara su idioma
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: no hay `<html lang>`; Narrador o NVDA leen el inglés con voz española o al revés.
- **Propuesta**: un `use_effect` que ponga `document.documentElement.lang` al cambiar el idioma.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): <html lang> sincronizado. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

# Herramientas, linters y repositorio (`HE-`)

<a id="he-01"></a>
### HE-01 · No hay CI
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: no existe `.github/`; la única puerta es el hook de agente (`check-rust.ps1`, solo clippy, solo dentro de una
  sesión). Nada garantiza que `main` compile, pase clippy y los tests, ni que `Cargo.lock` esté al día. Trampa: si el CI define
  `RUSTFLAGS` (lo habitual es `-D warnings`), Cargo descarta los `rustflags` de `.cargo/config.toml` y el `.exe` sale sin
  `+crt-static`: vuelve `ARQ-04` sin que nadie lo note. Comprobado al medir con `RUSTFLAGS` definido.
- **Propuesta**: `.github/workflows/ci.yml` en `windows-latest` con `fmt --check`, `clippy --all-targets --locked -- -D warnings` y
  `test --locked`, sin definir `RUSTFLAGS`; un job `msrv` con 1.93 (`HE-06`); `cargo-deny` (`HE-05`). Opcional: un job manual que
  corra `vm_sistema` (los runners son VMs desechables y admin, pero Windows Server: no cubren Appx).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): .github/workflows/ci.yml sin RUSTFLAGS. No se ha visto correr en GitHub (no hubo push). Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-02"></a>
### HE-02 · Sin `.gitattributes`: los finales de línea dependen de cada máquina
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: índice todo LF, árbol con 21 archivos CRLF y 47 LF, y `process.rs` mezclado; `core.autocrlf=true` viene de la
  configuración de esta máquina. `check-wsl.sh` saldría con CRLF en un clon con autocrlf y bash fallaría.
- **Propuesta**: `* text=auto eol=lf`, `*.ps1` e `*.iss` con `crlf`, `*.sh` con `lf`, imágenes `binary`; después
  `git add --renormalize .` en un commit propio.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): .gitattributes y árbol normalizado. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-03"></a>
### HE-03 · La versión se escribe a mano en seis sitios y el updater depende de que coincidan
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: `Cargo.toml`, cuatro literales en `otterswiss.iss`, `README.md`, `CHANGELOG.md` y el tag. El updater compara el tag
  con `CARGO_PKG_VERSION`: si se publica `v3.6.0` con `Cargo.toml` en 3.5.1, la app ofrece la actualización en bucle; si se olvida el
  `.iss`, Agregar o quitar programas muestra otra versión.
- **Dónde**: `Cargo.toml:5`; `otterswiss.iss:6,7,9,14`; `README.md:3`; `src/core/updater.rs:289`.
- **Propuesta**: el `.iss` lee la versión del binario (`GetStringFileInfo(…, "ProductVersion")`); un `release.yml` por tag `v*` que
  falle si el tag no coincide con `Cargo.toml`, compile, corra `ISCC` y publique; quitar la versión del título del README.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): El .iss toma la versión del .exe; release.yml falla si el tag no coincide. Sin probar: no hay Inno Setup en esta PC. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-04"></a>
### HE-04 · No hay `[lints]` ni `clippy.toml`: las reglas del proyecto no se hacen cumplir
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (medido) · **Estado**: Hecho
- **Qué pasa**: `.agents/rules/rust.md` prohíbe `unwrap` y `expect` fuera de tests, pero ningún lint lo comprueba. Hoy clippy por
  defecto da 0 avisos y `fmt --check` está limpio. Medido con lints extra: `unwrap_used`/`expect_used` 18, todos en tests;
  `allow_attributes_without_reason` 4; `unsafe_op_in_unsafe_fn` 10; `non_send_fields_in_send_ty` 2; `redundant_clone` 7;
  `let_underscore_must_use` 75 (es el patrón 6, «error tragado»); `pedantic` completo unos 1400, casi todo ruido de FFI.
- **Propuesta**:
  ```toml
  [lints.rust]
  unsafe_op_in_unsafe_fn = "warn"
  rust_2018_idioms = { level = "warn", priority = -1 }
  [lints.clippy]
  unwrap_used = "deny"
  expect_used = "deny"
  dbg_macro = "deny"
  todo = "deny"
  unimplemented = "deny"
  print_stdout = "warn"
  print_stderr = "warn"
  allow_attributes_without_reason = "warn"
  non_send_fields_in_send_ty = "warn"
  redundant_clone = "warn"
  ```
  y `clippy.toml` con `allow-unwrap-in-tests` y `allow-expect-in-tests` (unos 23 avisos para entrar; `vm_sistema` con su
  `#![allow]`). Fase 2: `let_underscore_must_use`, cada caso a `.ok()` comentado o `#[expect(…, reason)]`. No activar `pedantic`
  entero. `undocumented_unsafe_blocks` va en `CL-01`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): [lints] y clippy.toml; avisos de entrada arreglados. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-05"></a>
### HE-05 · Sin `cargo-deny` ni `cargo-audit`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa**: SEC-27 (RUSTSEC en `sevenz-rust`) se encontró a mano. Unos 395 crates en el árbol; los duplicados están en `AR-18`.
- **Propuesta**: `deny.toml` con `targets = ["x86_64-pc-windows-msvc"]`, advisories en `deny`, `multiple-versions = "warn"` y la
  lista de licencias; en el CI (`HE-01`).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): deny.toml con las licencias del árbol. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-06"></a>
### HE-06 · El MSRV 1.93 está declarado pero no se prueba
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (solo está instalada la stable 1.100) · **Estado**: Hecho
- **Propuesta**: el job `msrv` de `HE-01`, o `rust-toolchain.toml` con `channel = "1.93"` si se quiere reproducibilidad total.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Job msrv con 1.93 en el CI. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-07"></a>
### HE-07 · Documentación desactualizada respecto al código
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (README) · **Estado**: Hecho
- `README.md`: termina truncado dentro de un bloque `powershell` sin cerrar (no dice cómo compilar ni generar el instalador); pide
  Rust 1.82 (`:7,96`) y el mínimo es 1.93; promete purgar Prefetch (`:48`) y `cleaner.rs:260` no lo toca.
- `CHANGELOG.md:31-32`: dice que `.cargo/config.toml` deja de versionarse (volvió con `ARQ-04`) y habla de 36 tests (hay 37 y uno
  ignorado).
- `AGENTS.md:15,17`: manda `dx serve` (no está instalado ni hay `Dioxus.toml`) y `clippy` sin `--all-targets`; no avisa que
  `cargo run` sin elevar falla con 740 por el manifiesto; no menciona `docs/BUGS.md`, `tests/vm_sistema.rs`, la convención de
  idioma ni el estilo de commits.
- `.agents/rules/rust.md:15`: exige `anyhow`, que no es dependencia.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): README, CHANGELOG, AGENTS.md y reglas al día. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-08"></a>
### HE-08 · `.claude/settings.local.json` solo se ignora por la configuración de esta máquina
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (`git check-ignore -v`) · **Estado**: Hecho
- **Qué pasa**: lo ignora `~/.config/git/ignore`, no el `.gitignore`; en otro clon se podría commitear con permisos o rutas locales.
- **Propuesta**: agregarlo al `.gitignore` del repo.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): .claude/settings.local.json en .gitignore. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-09"></a>
### HE-09 · Migrar a la edición 2024
- **Prioridad**: P3 · **Esfuerzo**: M · **Evidencia**: observado (47 avisos de `rust-2024-compatibility`) · **Estado**: Hecho
- **Qué pasa**: 32 `tail_expr_drop_order` (informativos), 10 `unsafe_op_in_unsafe_fn`, 3 `if_let_rescope`, 1
  `missing_unsafe_on_extern` (`sys_tools.rs:11`) y 1 patrón. El MSRV 1.93 ya lo permite.
- **Propuesta**: `cargo fix --edition` y revisar a mano los 3 `if_let_rescope`, que pueden alargar la vida de guardas de `Mutex`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Edición 2024; salida de la búsqueda de restos idéntica antes y después. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-10"></a>
### HE-10 · Sin hook de pre-commit
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Propuesta**: `.githooks/pre-commit` con `fmt --check` y `clippy -D warnings`, activado con `git config core.hooksPath .githooks`
  y documentado; `fmt --check` también en `check-rust.ps1`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): .githooks/pre-commit y fmt en check-rust.ps1. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-11"></a>
### HE-11 · rustfmt no formatea el contenido de `rsx!`
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado (unas 171 líneas de más de 120 columnas en la UI) · **Estado**: Descartada
- **Propuesta**: `dx fmt --check` (dioxus-cli 0.7) como paso opcional.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Necesita instalar dioxus-cli.
- **Descartada** (2026-11-25): se instaló dioxus-cli 0.7.10 (decisión del autor) y `dx fmt` dejó `src/ui/app.rs` y `src/ui/views/dashboard.rs` sin compilar: dentro de un bloque `{ … }` de `rsx!` borra el `} else if en {` de una cadena `if`/`else if`/`else`. Un formateador que puede quitar una rama no se usa ni como paso opcional: si el resultado compilara, cambiaría la lógica sin avisar. Se revirtieron sus cambios. Volver a probar con una versión nueva de dioxus-cli, en un árbol limpio y compilando después. `dx` queda instalado en `~/.cargo/bin`.

<a id="he-12"></a>
### HE-12 · Metadatos de `Cargo.toml` y features de `windows` que podrían sobrar
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: faltan `license = "MIT"` (existe `LICENSE`), `repository` y `publish = false`. No se ve uso directo de las features
  `NetworkManagement_Dns`, `System_Memory`, `System_Ole` ni `System_Registry`; `ProcessIdToSessionId` está declarada a mano en vez de
  activar `Win32_System_RemoteDesktop`.
- **Dónde**: `Cargo.toml`; `src/core/sys_tools.rs:10-16`.
- **Propuesta**: completar los metadatos; quitar features una por una con `cargo check` (las de criptografía las exige
  `WINTRUST_DATA`, no tocarlas sin compilar).
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Metadatos completos; no se quitaron features de windows. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`, segunda pasada): Fuera las features `NetworkManagement_Dns`, `System_Memory` y `System_Ole`; entra `System_RemoteDesktop`. Verificado: `cargo test` (57) y `clippy --all-targets -- -D warnings` en verde en Windows.

<a id="he-13"></a>
### HE-13 · `.editorconfig` y otras herramientas de texto
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecho
- **Qué pasa / propuesta**: `.editorconfig` sí conviene para `.iss`, `.ps1`, `.md` y `.css`, que rustfmt no cubre (`README.md` no
  termina en salto de línea). `typos` aporta poco con textos en español; `taplo` tampoco con dos TOML.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): .editorconfig. Verificado: `cargo test` (54) y `clippy --all-targets -- -D warnings` en verde en Windows.

## Código de v4.2.0 (25/11/2026)

Misma auditoría sobre el código nuevo del commit `0a0f5ec` (fusionado en `4767abf`): `core/drivers.rs` y su vista, la tienda ampliada,
los cuatro temas nuevos y el puente de LibreHardwareMonitor. Conteos con `grep` sobre `4767abf` frente a `b80c8ef`:

| Medida | Antes | Ahora |
|---|---|---|
| `unsafe` sin `// SAFETY:` | 0 (fuera de `drivers.rs`) | 65, todos en `drivers.rs` |
| `source: None` | 96 | 142 |
| `style:` en `src/ui` | 50 | 95 |
| `if en {…}` en `src/ui` | 206 | 249 |
| Comentarios en `core/drivers.rs` + `views/drivers.rs` (2742 líneas) | — | 0 |
| Tests de lógica pura nueva | — | 0 |

Lo que está bien: `drivers.rs` solo importa de `platform` y `security`; no hay `unwrap`, `expect` ni mojibake; los temas nuevos pasan AA.
Los defectos están en `docs/BUGS.md` (`SEC-30`…`TST-02`).

<a id="or-06"></a>
### OR-06 · `drivers.rs` tiene 1857 líneas y seis responsabilidades
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: mezcla tipos y guardianes Win32, textos de problemas CM, enumeración SetupAPI, un cliente HTTP propio, parseo de
  HTML y versiones, Windows Update por COM, raspado del Catálogo, NVIDIA, Intel e instalación. No sigue el orden de `OR-04`.
- **Dónde**: `src/core/drivers.rs`; `src/core/mod.rs:15-20`.
- **Propuesta**: `core/drivers/{mod, devices, hwid, wua, catalog, install}.rs` y `vendor/{nvidia, intel}.rs`; `hwid` puro y con tests.
  Junto con `AR-21`…`AR-24`, que quitan unas 300 líneas.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `core/drivers/` con `mod.rs` (tipos públicos, fachada `DriverManager`, política del equipo y peticiones HTTPS comunes), `hwid.rs` (puro, con sus tests), `devices.rs`, `wua.rs`, `catalog.rs`, `history.rs`, `install.rs` y `vendor/{mod, nvidia, intel}.rs`; cada archivo tiene su `impl DriverManager` y sus tests, y la API pública no cambia. Sin cambios de lógica: se movió el código tal cual. El más largo queda en 360 líneas (`devices.rs`). Verificado: `cargo test` (69, los mismos 8 de drivers), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="or-07"></a>
### OR-07 · Los iconos de la tienda viven dentro de `tools.rs`
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `render_generic_icon` (85 líneas, 20 trazados SVG) es una `fn -> Element` dentro de la vista, estilada inline.
- **Dónde**: `src/ui/views/tools.rs:19-103`.
- **Propuesta**: `ui/components/icon.rs` con `#[component] AppIconBadge`, trazados en una tabla `const` y clase `.app-icon`.
- **Cierre** (2026-11-25): `ui/components/app_icon.rs` con `#[component] AppIconBadge` y `icon_paths`; el estilo pasa a `.app-icon` en el CSS (el trazo usa `--accent-text`, que cumple contraste en los 10 temas). Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="ar-21"></a>
### AR-21 · `drivers.rs` reimplementa el cliente HTTPS
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `wininet_get_bytes` (102 líneas) copia `security::http::get` + `read_to_end_capped`; `wininet_post_form` (91) agrega un
  POST que `http.rs` no tiene y corta en silencio a 4 MiB. Ninguno comprueba `final_url` contra una lista de hosts. User agents fijos
  (`OtterSwiss-Catalog/4.0`…) y `NvBackend/36.0.0.0` haciéndose pasar por el cliente de NVIDIA.
- **Dónde**: `src/core/drivers.rs:470-663`.
- **Propuesta**: `http::get` y un `http::post_form` nuevo en `security/http.rs`; cada llamador valida `final_url`. Causa de `ERR-19`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): fuera `wininet_get_bytes` y `wininet_post_form`: drivers usa `security::http` (`fetch_bytes`, `post_form` nuevo, `get_with_agent` solo para NVIDIA) con hosts exactos por servicio; desaparecen 10 `unsafe` sin SAFETY. Verificado: `cargo test` (67) y `clippy --all-targets -- -D warnings` en verde.

<a id="ar-22"></a>
### AR-22 · Descargas, caché y ZIP de drivers fuera de `StagedDownload`, `DownloadPolicy` y `ArchiveManager`
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: el .cab y el .zip se bajan a memoria y se escriben sin hash ni firma; la rama .exe arma a mano lo que ya hacen
  `StagedDownload` + `run_in_job`; el ZIP se extrae con `ZipArchive::extract`; la caché no se limpia. Causa de `SEC-30`, `SEC-33`,
  `SEC-34` y `ERR-22`.
- **Dónde**: `src/core/drivers.rs:1325-1350`, `1565-1673`.
- **Propuesta**: todo instalador por `StagedDownload::download` con política que exige firma; extracción con `ArchiveManager`; caché que
  se borra al soltarse.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): fabricante y Catálogo descargan con `download_to_file`/`StagedDownload` a `ScratchDir`, el ZIP se extrae con `ArchiveManager` y todo ejecutable o CAB pasa por Authenticode. Verificado: `cargo test` (67) y `clippy --all-targets -- -D warnings` en verde.

<a id="ar-23"></a>
### AR-23 · Procesos con `Command` directo y rutas de System32 a mano
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: 6 `Command::new` en `drivers.rs`: tres `.output()` sin el timeout de `run_captured` y tres `.spawn()` sin Job ni espera.
  `get_system32_tool` duplica `process::system32_dir` con un `C:\Windows\System32` literal; `CREATE_NO_WINDOW` redefinido;
  `resolve_winget_binary` acaba en `"winget.exe"` relativo (`SEC-31`).
- **Dónde**: `src/core/drivers.rs:51`, `364-372`, `868`, `1357`, `1382`, `1619`, `1628`, `1668`; `src/core/store/app_store.rs:1444-1458`.
- **Propuesta**: `run_captured` para herramientas de consola, `run_in_job` con `INSTALLER_TIMEOUT` para instaladores, `Err` si no hay
  ruta absoluta.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): instaladores con `run_in_job` y `expand`/`pnputil` con `run_captured`; winget solo con ruta absoluta del perfil. Queda `export_driver_store_backup` con `Command` y `get_system32_tool`. Verificado: `cargo test` (67) y `clippy --all-targets -- -D warnings` en verde.
- **Cierre** (2026-11-25): `export_driver_store_backup` usa `run_captured` con `PNPUTIL_TIMEOUT` y `system32_dir`; fuera `get_system32_tool`, `CREATE_NO_WINDOW` y el último `Command` de `drivers.rs`. Verificado: `cargo test` (68), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="ar-24"></a>
### AR-24 · Guardianes y helpers Win32 duplicados
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `ComApartmentGuard::new_sta` repite `ComGuard`; `DevInfoGuard` es RAII de plataforma dentro de una función de producto;
  `is_system_laptop` duplica `power::detect_battery_presence` con otro criterio; `decoded_vendor_name` es una segunda tabla PCI;
  `read_windows_build` inventa `"22631"` si falla.
- **Dónde**: `src/core/drivers.rs:137-160`, `266-307`, `716-733`.
- **Propuesta**: `ComGuard::init_sta()`, `DevInfoList` en `platform/handles.rs`, `power::detect_battery_presence`, tabla PCI compartida
  con `specs::gpu`, build como `Option<u32>`.
- **Cierre parcial** (2026-11-25): `ComGuard::init_sta` y `DevInfoList` en `platform/handles.rs` reemplazan a `ComApartmentGuard` y `DevInfoGuard`; `is_system_laptop` usa `power::detect_battery_presence`, que ahora mira el bit 128 como máscara (antes solo el valor exacto 128). `read_windows_build` ya devolvía `Option` (`ERR-23`). Falta la tabla PCI compartida con `hardware`. Verificado: `cargo test` (68), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.
- **Cierre final** (2026-11-25): `core/hardware/pci.rs` con la única tabla de fabricantes (`vendor_name`, `vendor_name_hex`), usada por `specs::gpu` y por `InstalledDriverDevice::decoded_vendor_name`; la GPU ahora reconoce los mismos fabricantes que la lista de dispositivos. Verificado: `cargo test` (75 + 4 de integración), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="ar-25"></a>
### AR-25 · Tres comparadores de versiones con semánticas distintas
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `drivers::is_version_newer` (da «más nueva» si la actual no es numérica), `app_store::parse_numeric_version` (quita
  ceros finales) y `updater::parse_version` (semver). `1.2.0` y `1.2` son iguales en uno y distintas en otro; solo uno tiene tests.
- **Dónde**: `src/core/drivers.rs:686-714`; `src/core/store/app_store.rs:1478-1496`; `src/core/updater.rs:106-131`.
- **Propuesta**: `core::version::NumericVersion: Ord` con tests e `is_newer -> Option<bool>`.
- **Cierre parcial** (2026-11-25): la tienda ya no compara versiones (`parse_numeric_version` y `find_newer_curated_version` se fueron con las versiones fijas; el Desinstalador ofrece «Buscar actualización» y winget decide). Quedan dos comparadores: `drivers::hwid::is_version_newer` y `updater::parse_version`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `core::platform::version::NumericVersion` (`Ord`, los ceros finales no cuentan, `is_newer -> Option<bool>` con `None` si no es numérica) con tres tests; `drivers::hwid::is_version_newer` pasa por él. El comparador de la tienda ya había desaparecido; el de `updater` sigue aparte porque su semántica es otra (semver con prelanzamiento: `1.2.0-beta` < `1.2.0`). Verificado: `cargo test` (92 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="ar-26"></a>
### AR-26 · Core arma textos y la UI los vuelve a leer como datos
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Parcial
- **Qué pasa**: `describe_cm_problem(code, en)` bilingüe en core (contra `AR-17`); rellenos en un idioma dentro de campos de datos
  («Desconocida», «Sin driver», «Latest», «Oficial Intel»); `install_*` devuelve `CoreResult<String>` y la vista busca «Reinicio» en el
  texto (`ARQ-05`); en sensores, `sensor_type` y la nota larga en español.
- **Dónde**: `src/core/drivers.rs:71-76`, `309-339`, `1186-1189`, `1397-1405`, `1780-1800`; `src/ui/views/drivers.rs:619-621`;
  `src/core/hardware/sensors/wmi.rs`.
- **Propuesta**: `CmProblem(u32)` y `Option<String>` en core, textos en `strings`; `DriverInstallOutcome`; `SensorSource` enum.
- **Cierre parcial** (2026-11-25): `describe_cm_problem` salió de core (`cm_problem` en el catálogo, `IN-08`); `DriverInstallReport` ya no trae una frase sino cantidades y `history_warning`, y la vista arma «N drivers instalados» o «Se quitó …» (`installed`, `removed`). Faltan el relleno «Desconocida» de NVIDIA y los textos de sensores. Verificado: `cargo test` (76 + 4 de integración), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; en la app sin elevar, «236 dispositivos detectados.» pasa a «236 devices detected.» al cambiar de idioma, y el error del Debloater sin admin pasa de «Administrator rights required: …» a «Hace falta ejecutar como administrador: …».

<a id="ar-27"></a>
### AR-27 · Booleanos y centinelas donde corresponde un enum
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `CuratedPackage` suma `is_winget` + `winget_id: Option`; las 15 UWP rellenan `url: ""`, `allowed_hosts: &[]`,
  `version: "Store"` y repiten 11 campos (unas 360 líneas). Se puede escribir un paquete directo sin verificación (`ERR-16`). `version`
  usa las cadenas mágicas «Evergreen» y «Store»; los códigos winget no tienen nombre. En drivers, `has_problem` + `problem_code`.
- **Dónde**: `src/core/store/app_store.rs:113-126`, `998-1357`, `1588-1625`; `src/core/drivers.rs:53-68`, `240-254`.
- **Propuesta**: `enum InstallSource { Direct { …, verify: Verify }, Store { id } }` con `enum Verify { Sha256, Signer }` para que «sin
  verificación» no compile; `enum CatalogVersion`; constantes para los códigos; `const fn store_app(...)`.
- **Cierre parcial** (2026-11-25): `InstallSource` reemplaza `is_winget` + `winget_id: Option`, `url: ""`, `version: "Evergreen"`/`"Store"` y los 11 campos de relleno de las UWP: un paquete sin verificación ya no se puede escribir. Los códigos de winget tienen nombre (`store/winget.rs`). Falta `has_problem` + `problem_code` en drivers.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `InstalledDriverDevice` pasa de `has_problem: bool` + `problem_code: u32` a `problem: Option<u32>` con el método `has_problem()`; el código 28 que dispara la búsqueda en el Catálogo tiene nombre (`CM_PROB_FAILED_INSTALL`). Con lo de la tienda (`InstallSource`, códigos de winget), no queda otro booleano con centinela. Verificado: `cargo test` (92 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="ar-28"></a>
### AR-28 · API pública de drivers más ancha de lo que usa la UI, y campos muertos
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `check_nvidia_updates`, `check_intel_updates`, `search_driver_updates` y `extract_ven_dev` son `pub` sin uso fuera;
  `driver_model`, `driver_manufacturer`, `is_downloaded`, `is_optional`, `products`, `classification` y `succeeded_count` no se leen;
  `notes` siempre está vacío.
- **Dónde**: `src/core/drivers.rs:112`, `205-264`, `889`, `1427`, `1522`, `1720`.
- **Propuesta**: `pub(crate)` solo para la fachada; borrar o mostrar los campos.
- **Cierre** (2026-11-25): fuera `driver_model`, `driver_manufacturer`, `is_downloaded` (WUA), `products` y `classification` (Catálogo), que no se leían; `search_driver_updates` y `check_nvidia_updates` pasan a privadas del módulo y `extract_ven_dev` a `pub(crate)`. `succeeded_count` y `notes` ya se usan (`AR-26`, `ERR-20`); `check_intel_updates` se fue con Intel (`ERR-25`). Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="ar-29"></a>
### AR-29 · La vista de drivers repite el ciclo de tareas en vez de usar `ui::task::run_blocking`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: 9 `spawn_blocking` directos, cada uno con su `match` y un «Error de hilo: {}» fijo en español.
- **Dónde**: `src/ui/views/drivers.rs:36`, `104`, `187`, `201`, `233`, `552`, `613`, `623`, `844`.
- **Propuesta**: `run_blocking` y un helper local que maneje `is_working` y el aviso.
- **Cierre** (2026-11-25): los 8 trabajos de la vista de drivers que devuelven `Result` usan `ui::task::run_blocking` (error ya convertido en `Msg`); queda `spawn_blocking` solo en la búsqueda completa, que no devuelve `Result`, con `thread_error`. Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="cl-19"></a>
### CL-19 · 65 `unsafe` sin `// SAFETY:` tapados por un `#[allow]` con motivo obsoleto
- **Prioridad**: P1 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `core/mod.rs` exime a `drivers` de `undocumented_unsafe_blocks` porque «ARQ-03: módulo sin uso». Ya se usa. Reparto:
  44 COM/WUA, 10 WinInet, 7 SetupAPI y guardianes, 4 resto.
- **Dónde**: `src/core/mod.rs:15-19`; `src/core/drivers.rs`.
- **Propuesta**: quitar el `#[allow]`; con `AR-21`, `AR-23` y `AR-24` desaparecen unos 20; los de COM se documentan en helpers.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): fuera el `#[allow]` de `core/mod.rs`; `drivers` queda bajo `undocumented_unsafe_blocks` como el resto. Los `unsafe` bajan de 57 a 16, todos con `// SAFETY:`: las llamadas COM de Windows Update se agrupan en `wua_pending_drivers`, `wua_candidate` y `wua_install_on_this_thread`, y los guardianes pasan a `platform/handles.rs` (`AR-24`). Verificado: `cargo test` (68), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; al quitar un `SAFETY` a propósito, clippy falla.

<a id="cl-20"></a>
### CL-20 · 46 `source: None` nuevos en `drivers.rs`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (96 → 142) · **Estado**: Hecha
- **Qué pasa**: 25 tienen el error en la mano y lo pierden; `SystemTool` como cajón de sastre (37 veces); `Trust` para errores HTTP.
- **Dónde**: `src/core/drivers.rs` (por ejemplo `764-767`, `901-931`, `1037-1167`, `1607-1616`).
- **Propuesta**: `CoreError::sys`/`trust` (`AR-06`), `Validation` para IDs inválidos.
- **Cierre** (2026-11-25): `source: None` en `core/drivers/` baja de 46 a 15 (cliente HTTPS común en `AR-21`, Intel fuera en `ERR-25`); `wua_error` conserva el HRESULT en `source`. Los que quedan son errores de validación o de herramientas sin un error de origen. Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="cl-21"></a>
### CL-21 · Funciones largas y bloques copiados (`DriversView` de 825 líneas)
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `DriversView` 825 líneas con 30 `let x = if en {…}` seguidos y 6 copias del bloque de tarea; `BatchAppStoreCard` 235;
  en core, `install_driver_updates` 179, `install_catalog_driver` 154 y cinco más de 80 a 130; apertura de sesión WUA copiada.
- **Dónde**: `src/ui/views/drivers.rs:61-885`; `src/ui/views/tools.rs:425-660`; `src/core/drivers.rs`.
- **Propuesta**: subcomponentes (`DriverToolbar`, `VendorUpdatesCard`, `CatalogCard`, `WuaCard`, `DeviceList`), `open_wua_searcher()`,
  y separar descarga, extracción e instalación.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `DriversView` pasa de 825 a 303 líneas (casi todo son los manejadores de escaneo, instalación y respaldo): `InstallConfirm`, `VendorUpdatesCard`, `HistoryCard`, `CatalogCard`, `WuaCard`, `DeviceListCard` y `DeviceRow` son componentes con sus señales como props; el filtro de dispositivos es `device_matches` (una lista de campos en vez de siete `contains` copiados) y la búsqueda en el Catálogo es `search_catalog`. Los filtros y la búsqueda viven en `DeviceListCard`. En core, `AR-29` y `CL-20` ya habían dejado `run_blocking` y la apertura de sesión WUA común. Verificado: `cargo test` (78 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="cl-22"></a>
### CL-22 · Lógica pura nueva sin tests
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: sin tests `short_hw_id`, `extract_ven_dev`, `friendly_gpu_version`, `is_version_newer`, `url_encode`, `strip_html_tags`,
  los parsers del Catálogo, NVIDIA e Intel, `is_cpu_sensor_name`, ni invariantes del catálogo (IDs únicos, verificación, `winget_id`).
- **Dónde**: `src/core/drivers.rs`; `src/core/hardware/sensors/wmi.rs`; `src/core/store/app_store.rs`.
- **Propuesta**: tests junto al código (`id_pci_se_acorta_a_ven_dev`, `catalogo_directo_siempre_verificado`,
  `sensor_gpu_no_cuenta_como_cpu`) y fixtures chicos en `tests/fixtures/`. Ver `TST-02`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): tests de IDs de hardware, versiones, GUID, enlace del CAB, sensores, winget, catálogo y Authenticode. Faltan `strip_html_tags`, `url_encode` y el JSON de Intel. Verificado: `cargo test` (67) y `clippy --all-targets -- -D warnings` en verde.
- **Cierre final** (2026-11-25): tests de `url_encode` (`url_encode_escapa_lo_reservado`) y `strip_html_tags` (`celda_html_a_texto`); el JSON de Intel ya no existe (`ERR-25`). Verificado: `cargo test` (75 + 4 de integración), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="cl-23"></a>
### CL-23 · Sin comentarios de documentación; la fusión de v4.2.0 borraba comentarios con ID
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: 0 comentarios en 2742 líneas de drivers. El commit original quitaba además los comentarios con ID (SEC-12, SEC-05,
  ERR-09, UI-15…) de los archivos que tocaba.
- **Dónde**: `src/core/drivers.rs`, `src/ui/views/drivers.rs`; `src/core/store/app_store.rs`, `src/core/optimize/registry_tweaks.rs`.
- **Propuesta**: documentar las funciones con invariantes (`get_device_property`, `short_hw_id`, `ComApartmentGuard`,
  `collect_intel_matches`).
- **Cierre parcial** (2026-11-25, fusión `4767abf`): la fusión conservó los comentarios de la rama en los archivos ya existentes
  (`registry_tweaks.rs`, archivador, debloater, `security/`); en `app_store.rs` se restauraron los de UI-15 e IN-01. Faltan los de
  drivers y los doc comments de `stage_copy_for_user`, `display_name_matches`, `parse_numeric_version` y `find_newer_curated_version`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): documentadas con su invariante `get_device_property`, `short_hw_id_of`, `display_name_matches`, `bstr_text`, `variant_flag`, `wua_update_id` y `wua_new_collection`; `ComApartmentGuard`, `collect_intel_matches`, `stage_copy_for_user`, `parse_numeric_version` y `find_newer_curated_version` ya no existen (`ERR-25`, `SEC-35`, `AR-29`). Verificado: `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="cl-24"></a>
### CL-24 · `read_wmi_f32` copia `read_wmi_i32`; umbrales mágicos y filtro de sensor por nombre
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: dos lectores con el mismo cuerpo; rangos `1.0..125.0`, `..150.0` y tope 256 sueltos; filtro por subcadena del nombre
  (`UI-22`) aunque LHM expone `Identifier`.
- **Dónde**: `src/core/hardware/sensors/wmi.rs`.
- **Propuesta**: `read_wmi<T>(obj, prop, conv)`, constantes con nombre y filtro por `Identifier`.
- **Cierre** (2026-11-25): `read_wmi<T>` con el conversor (`VariantToInt32`, `VariantToDouble`) reemplaza los dos lectores copiados; los umbrales tienen nombre (`CURRENT_TEMP_C`, `LIMIT_TEMP_C`, `MIN_CRITICAL_C`, `MAX_LHM_SENSORS`). El filtro de CPU ya miraba `Identifier` (`UI-22`). Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="cl-25"></a>
### CL-25 · Componentes y clases que ya existen no se usan
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: instalar drivers, firmware o un .cab sin `ConfirmDialog` (`SEC-37`); escaneo sin `LoadingState`; filtros sin resultados
  sin `EmptyState`; la fila con checkbox rehecha 5 veces; los buscadores reimplementan `.input` inline.
- **Dónde**: `src/ui/views/drivers.rs:537-880`; `src/ui/views/tools.rs:516-655`.
- **Propuesta**: `ConfirmDialog`, `LoadingState`, `EmptyState`, un `CheckRow` y `class: "input grow"`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `CheckRow` (`ui/components/check_row.rs`) reemplaza la fila con casilla rehecha en Drivers (respaldo y Windows Update), Limpiador y Debloater; `EmptyState` cuando el filtro de dispositivos o de la tienda no deja resultados y `LoadingState` mientras se leen los dispositivos por primera vez; el buscador del desinstalador usa `input grow-wide`. Los tres caminos de instalación de drivers ya pasaban por `ConfirmDialog` (`SEC-37`). Verificado: `cargo test` (78 + 4), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="te-25"></a>
### TE-25 · 45 estilos inline nuevos; la tienda revierte clases utilitarias a inline
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado (`style:` 50 → 95) · **Estado**: Hecha
- **Qué pasa**: 31 `style:` en drivers que equivalen a `.stack-*`, `.row .wrap`, `.fw-semibold`, `.input`, `.btn-sm`; la tarjeta de ítem
  copiada 5 veces. La tienda reemplazó `stack-3`, `row-between gap-3 wrap`, `flex gap-2`, `row gap-3` y `text-sm fw-semibold` por `style:`
  (regresión de `TE-02`).
- **Dónde**: `src/ui/views/drivers.rs:473-827`; `src/ui/views/tools.rs:513-648`.
- **Propuesta**: volver a las utilidades y crear `.item-row`; `style:` solo para valores calculados.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): clase `.item-row` para las filas con borde (la tarjeta que estaba copiada 5 veces), `.grow-wide` para buscadores y títulos de fila, `.modal-list.tall` y `.ml-5`; la tienda vuelve a `stack-3`, `row-between gap-3 wrap`, `flex gap-2` y `text-sm fw-semibold`. `style:` en `drivers.rs` pasa de 31 a 0; los 4 que quedan en `tools.rs` ya estaban antes de la fusión (icono, `OR-07`; tarjeta de progreso y menú contextual, `TE-02`). Verificado: `cargo test` (69), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; Drivers y Tienda recorridas en una copia sin elevar, en oscuro y ventana chica, y en claro, inglés y maximizada.

<a id="te-26"></a>
### TE-26 · `px` y colores derivados escritos en Rust
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: `min-width: 240px`, `max-height: 460px`/`480px`, `28px`, `16px`, `1px solid` y `color-mix(…)` inline (regla §2 de
  `ui-temas.md`).
- **Dónde**: `src/ui/views/drivers.rs:524-794`; `src/ui/views/tools.rs:90-92`, `519`, `580`, `626`.
- **Propuesta**: `.app-icon`, `.list-scroll` con token de alto y `--control-min-width`.
- **Cierre** (2026-11-25): los `px` y `color-mix` de drivers y de la tienda pasaron a clases (`.item-row`, `.grow-wide`, `.modal-list.tall` en `TE-25`; `.app-icon` en `OR-07`). Queda en `tools.rs` la tarjeta de progreso, anterior a la fusión (`TE-02`). `style:` baja de 47 a 45. Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="te-27"></a>
### TE-27 · Texto de error de la tienda con `--danger` (regresión de UI-18)
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Dónde**: `src/ui/views/tools.rs:648`.
- **Propuesta**: `class: "text-xs text-danger"`.
- **Cierre** (2026-11-25): el error de una app usa `text-xs text-danger` (`--danger-text`, AA). Verificado: `cargo test` (69), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; Drivers y Tienda recorridas en una copia sin elevar, en oscuro y ventana chica, y en claro, inglés y maximizada.

<a id="te-28"></a>
### TE-28 · Filtros marcados alternando `btn-primary` (regresión de TE-13)
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: los 3 filtros de dispositivos y los 7 de categoría cambian de clase en vez de usar `aria-pressed`.
- **Dónde**: `src/ui/views/drivers.rs:736-748`; `src/ui/views/tools.rs:523`, `532`.
- **Propuesta**: `btn btn-secondary btn-sm` con `aria_pressed`.
- **Cierre** (2026-11-25): los 3 filtros de dispositivos y los 7 de categoría son `btn btn-secondary btn-sm` con `aria-pressed`. Verificado: `cargo test` (69), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; Drivers y Tienda recorridas en una copia sin elevar, en oscuro y ventana chica, y en claro, inglés y maximizada.

<a id="te-29"></a>
### TE-29 · Temas nuevos: texto secundario igual al principal, peligro igual al acento y elevación invertida
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (WCAG sobre los hex) · **Estado**: Hecha
- **Qué pasa**: las paletas del compañero no pasaban AA (texto secundario de Dracula, Tokyo Night y Solarized, acento de Solarized). Al
  fusionar se subieron hasta 4,5:1 y se completaron los tokens que faltaban (`color-scheme`, `--surface-active`, `--surface-subtle`,
  `--control-border`, `--danger-hover`), pero quedó:
  - Solarized: `--text` y `--text-muted` casi iguales (1,01:1): no hay jerarquía. Con esa base, el muted no llega a 4,5:1 sin acercarse
    al texto; hay que subir `--text`.
  - Sakura: `--danger` = `--accent` (#eb6f92) y `--warning` = `--accent-hover`.
  - Dracula y Tokyo Night: `--surface` más oscuro que `--bg`, al revés que los demás temas oscuros.
  - Sakura es la paleta Rosé Pine: nombrarla así o dar el crédito. «Lo que hay que conservar» ya no son seis temas.
- **Dónde**: `assets/main.css` (bloques `dracula`, `tokyonight`, `solarized`, `sakura`).
- **Propuesta**: subir `--text` de Solarized, `--danger` propio en Sakura, `--surface` sobre `--bg`; test de distancia mínima entre
  `--danger` y `--accent` y entre `--text` y `--text-muted`.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): Solarized con `--text` #eee8d5 (base2) sobre el muted; Sakura con acento «iris» #c4a7e7 (peligro sigue en «love») y crédito a Rosé Pine (MIT) en el CSS; Dracula y Tokyo Night con `--bg` y `--surface` intercambiados. Al medir apareció un fallo más: en Claro, `--text-muted` sobre `--bg` daba 4,34:1; pasa a #5b6576. `tests/temas.rs` exige AA (4,5:1) para cada texto sobre `--bg` y `--surface` y para `--on-accent`/`--on-danger`, peligro distinto del acento, aviso distinto del hover del acento, texto y muted distinguibles y superficie por encima del fondo, en los 10 temas; con el CSS anterior falla. Recorrido: Sakura, Dracula, Solarizado, Tokyo Night y Claro en Inicio. Verificado: `cargo test` (75 + 4 de integración), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.

<a id="in-08"></a>
### IN-08 · 43 `if en` nuevos fuera del catálogo y regresión de `strings(en)` en la tienda
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado (206 → 249) · **Estado**: Hecha
- **Qué pasa**: 39 en drivers sin ninguna cadena en `strings.rs`; la tienda volvió de `strings(en).installed_app`/`installing` a texto
  suelto; `describe_cm_problem` lleva el idioma a core.
- **Dónde**: `src/ui/views/drivers.rs`; `src/ui/views/tools.rs:464-489`, `592`, `634-640`; `src/core/drivers.rs:309`.
- **Propuesta**: bloque `drivers` en `Strings` con `plural()` y volver a `strings(en)` en la tienda.
- **Cierre** (2026-11-25): bloque `drivers` en el catálogo (`ui/i18n/drivers.rs`, con `fn` para los textos con cantidad y `plural`); la vista de drivers queda sin `if en`. `describe_cm_problem` sale de core y pasa al catálogo como `cm_problem`. La tienda vuelve a `strings(en)` (`installed_app`, `installing`) y suma `queued`, `done`, título, buscador y «todas las categorías». «Error de hilo», que solo estaba en español, pasa a `internal_error`. Queda `IN-03`: un aviso ya mostrado no cambia de idioma. Verificado: `cargo test` (69), `clippy --all-targets -- -D warnings` y `fmt --check` en verde; Drivers y Tienda recorridas en una copia sin elevar, en oscuro y ventana chica, y en claro, inglés y maximizada.

<a id="in-09"></a>
### IN-09 · Textos en un solo idioma o mezclados
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: «Error de hilo» (7 veces), «FIRMWARE / BIOS», «OK», «MS Store» fijos; textos de core en los dos idiomas (`AR-26`); los 15
  nombres UWP solo en español; avisos guardados con el idioma del momento (`IN-03`).
- **Dónde**: `src/ui/views/drivers.rs`; `src/core/store/app_store.rs:999-1357`; `src/ui/views/tools.rs:590`.
- **Propuesta**: todo en `strings`; `name_en` para las UWP.
- **Cierre** (2026-11-25): «Error de hilo» pasó al catálogo (`IN-08`), «MS Store» a «Microsoft Store», y los 15 nombres de apps de la Store se traducen con `core_text::tr` (el test exige su traducción, salvo nombres de marca). «OK» y «FIRMWARE / BIOS» se dejan iguales en los dos idiomas a propósito. Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="in-10"></a>
### IN-10 · «...», «(s)», mayúsculas de título y «drivers/controladores»
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: 18 `...`, 10 plurales con `(s)` (`IN-04`), mayúsculas de título («Escaneo Completo de Drivers», «App Store Silenciosa y
  Restaurador UWP»…), «drivers» y «controladores» mezclados, anglicismos sin explicar («Feeds», «WHQL», «Backup»).
- **Dónde**: `src/ui/views/drivers.rs:26-466`; `src/core/store/app_store.rs:55-74`; `src/ui/views/tools.rs:464-489`.
- **Propuesta**: corregir al mover a `strings` y elegir un término.
- **Cierre** (2026-11-25): sin «...» en textos visibles (core y archivador pasan a «…»; los catálogos tienen test), sin plurales con «(s)» (`plural()`), y la pestaña Drivers usa «drivers» y frases en minúscula inicial. Verificado: `cargo test` (78 + 4) y `clippy --all-targets -- -D warnings` en verde.

<a id="he-14"></a>
### HE-14 · Nada frena las regresiones de una fusión
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Hecha
- **Qué pasa**: el `#[allow]` de `CL-19` sigue; nada impidió que `style:` (50 → 95), `if en` (206 → 249) y `source: None` (96 → 142)
  subieran ni que se borraran comentarios con ID.
- **Dónde**: `src/core/mod.rs:15-19`; CI (`.github/workflows/ci.yml`).
- **Propuesta**: script en `.agents/scripts/` (o test) que cuente `style:`, `if en` y `source: None` y falle si suben del umbral;
  `clippy::too_many_lines` en `warn` para core.
- **Cierre** (2026-11-25, rama `auditoria/buenas-practicas`): `tests/indicadores.rs` (corre en `cargo test` y en el CI) cuenta en `src/` `style:` (techo 47), `if en` (217) y `source: None` (118) y falla si alguno sube; otro test falla si algún archivo vuelve a eximirse de `undocumented_unsafe_blocks` (`CL-19`). `clippy::too_many_lines` queda en `warn` solo para core, con techo 180 en `clippy.toml` (la mayor hoy tiene 176; `CL-05` las va partiendo). No se cubre el borrado de comentarios con ID: no hay forma mecánica fiable de distinguirlo de una limpieza. Verificado: con un `style:` de más, con la exención agregada y con el techo en 170, cada freno falla; sin eso, `cargo test` (70 + 2), `clippy --all-targets -- -D warnings` y `fmt --check` en verde.
