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
- **Prioridad**: P1 · **Esfuerzo**: L · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: primitivas Win32 (`win_common`, `safe_fs`, `process`, `registry`, `services`, `power`), seguridad (`trust`), hardware
  (`specs`, `sensors`), funciones (`archiver`, `cleaner`, `debloater`, `uninstaller`…) y datos (`tweaks_catalog`) están al mismo
  nivel, sin que la carpeta diga qué capa depende de cuál.
- **Propuesta** (aprobada por el usuario el 24/11): `core/{platform, security, hardware, optimize, debloat, uninstall, archive,
  store}/` más `error.rs`, `cli.rs`, `rollback.rs` y las funciones chicas sueltas (`cleaner`, `dns`, `updater`, `context_menu`,
  `sys_tools`). `drivers.rs` queda donde está (`ARQ-03`, diferido).

<a id="or-02"></a>
### OR-02 · Todos los componentes de UI en un solo `components/mod.rs`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Propuesta**: un archivo por componente (`modal.rs`, `card.rs`, `status.rs`, `state.rs`, `progress.rs`, `check_row.rs`) y
  `mod.rs` solo con los `pub use`.

<a id="or-03"></a>
### OR-03 · Nombres y lugares que no siguen la convención
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `views/hardware_tab.rs` es la única vista con sufijo; la mini ventana del archivador (un componente) vive en
  `main.rs`; `ui/mod.rs` es a la vez módulo y shell de la app; `otterswiss.iss` está suelto en la raíz; los logos están mezclados
  con `main.css` en `assets/`.
- **Propuesta**: `views/hardware.rs`, `views/mini_archive.rs`, `ui/app.rs` y `ui/nav.rs`, `installer/otterswiss.iss`,
  `assets/brand/`.

<a id="or-04"></a>
### OR-04 · El orden interno de los archivos no sigue una regla
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: constantes entre funciones (`GENERIC_FOLDER_TOKENS` en `uninstaller.rs:234`, entre helpers), helpers antes y después
  del `impl`, tipos públicos al final (`FullRevertReport` en la línea 1349), cabeceras `// src/…` que repiten la ruta.
- **Propuesta**: regla en `AGENTS.md`: `use` → constantes → tipos públicos → `impl` → funciones privadas → `#[cfg(test)]`; sin
  cabeceras de ruta.

<a id="or-05"></a>
### OR-05 · Dos carpetas de configuración de agentes sin explicar
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `.agents/` (reglas, skills, scripts, hook, relevo) y `.claude/agents/` (auditores, commit-writer); `AGENTS.md` no
  menciona ninguna.
- **Propuesta**: documentar en `AGENTS.md` qué va en cada una.

# Arquitectura y orden (`AR-`)

<a id="ar-01"></a>
### AR-01 · La vista del desinstalador decide el éxito y orquesta el flujo
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: confirmado · **Estado**: Pendiente
- **Qué pasa**: `uninstall_exit_is_success` (`matches!(code, 0 | 3010 | 1641)`) vive en la vista y repite `ProcessExit::is_success`.
  `run_uninstaller` devuelve solo `exit_code` y la secuencia «ejecutar → `is_still_registered` → `scan_leftovers`», con su timeout de
  300 s, está en la UI. Dos fuentes de verdad, y el flujo central no se puede probar ni usar desde la CLI.
- **Dónde**: `src/ui/views/uninstaller.rs:30-32,142-277`; `src/core/process.rs:166-168`; `src/core/uninstaller.rs:603,678`.
- **Propuesta**: `run_uninstaller` devuelve `ProcessExit`; `UninstallerManager::uninstall_and_scan(app) -> UninstallOutcome`
  (`Failed(code)`, `StillRegistered`, `Removed { reboot_required, leftovers }`); la vista solo traduce el enum.

<a id="ar-02"></a>
### AR-02 · Dos pilas de respaldo de registro y tres almacenes JSON casi iguales
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: el Debloater tiene su propio `DebloatRegHive`, `DebloatRegSnapshot`, captura, aplicación y restauración, que duplican
  `RegHive`, `RegistrySnapshot` y `registry::capture/apply/restore`. La validación está copiada con números mágicos
  (`[1, 2, 3, 4, 11]` y `1024` en los dos lados) y ya divergen: el snapshot del Debloater no tiene `key_created` ni `applied_bytes`.
  Además, `backup.rs`, `debloater.rs` y `dns.rs` repiten «leer acotado → `from_slice` → validar → `to_vec_pretty` →
  `atomic_write_new`». El patrón 4 y el 6 de `docs/BUGS.md` se arreglaron tres veces por separado.
- **Dónde**: `src/core/debloater.rs:290,459-503,911-1120`; `src/core/backup.rs:15-16,119-277`; `src/core/registry.rs:9,153-336`;
  `src/core/dns.rs:554-585`; `src/core/tweaks_catalog.rs:5`.
- **Propuesta**: `JsonStore<T>` con `load_or(default, max, validate)` y `save`; el Debloater expresa sus entradas como
  `RegistryTweakSpec` y usa `registry` y un `BackupManager` parametrizado; un lector del formato viejo solo para migrar.

<a id="ar-03"></a>
### AR-03 · Todo es `pub`: hay atajos que saltan las validaciones y el compilador no ve el código muerto
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `lib.rs` exporta `core` y `ui` enteros y cada módulo es `pub mod`. `ArchiveManager::compress_to_zip`, `extract_zip`,
  `extract_7z`, `extract_tar*` y compañía son `pub` sin usos externos, y llamarlas directamente se salta `validate_cli_paths`, el «no
  sobrescribir» y el `.partial` con renombre (SYS-22). Como todo es alcanzable, `rustc` no avisa `dead_code` (ver `AR-05`).
- **Dónde**: `src/lib.rs`; `src/core/mod.rs`; `src/core/archiver.rs:421-921,713-762`.
- **Propuesta**: `pub(crate)` o privado por defecto; `pub` solo la fachada que usan `main.rs` y `tests/vm_sistema.rs`. Módulo por
  módulo.

<a id="ar-04"></a>
### AR-04 · La detección de hardware está escrita tres veces y el motor paga la completa para leer el tier
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `HardwareProfile::detect` (CPU, placa, GPU, discos por `\\.\PhysicalDrive{n}`), `specs.rs` (lo mismo con más
  detalle) y `sensors.rs` (discos otra vez) se solapan, con dos tablas de `BusType` de números mágicos y dos lectores de C-string. El
  motor llama a `detect()` completo solo para `tier` (núcleos y RAM). `sysinfo` entra solo para esto (ver `AR-18`).
- **Dónde**: `src/core/win_common.rs:422-630,543,604`; `src/core/specs.rs:233,467,685,746-848`; `src/core/sensors.rs:593-710`;
  `src/core/engine.rs:89-120,199`.
- **Propuesta**: `HardwareTier::detect()` aparte; la descripción en `specs` (y el Dashboard muestra `SystemSpecs`);
  `core/hw/storage.rs` con la enumeración de discos que comparten `specs` y `sensors`, y `BusType` con las constantes del crate.

<a id="ar-05"></a>
### AR-05 · Código muerto que el compilador no puede señalar
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (`config.rs`) · **Estado**: Pendiente
- **Qué pasa**: sin usos fuera de su archivo: `core/config.rs` entero (`AppConfig`, `DefaultArchiveFormat`, que además duplica
  `ArchiveFormat`), `specs::export_specs_json` (mientras `hardware_tab.rs:89-104` reimplementa la exportación),
  `safe_fs::duplicate_file_handle` (ver `CL-01`), `TrimWorkingSetsReport::explanatory_note`, `CoreError::Sensor`,
  `AppStoreManager::install_package` y `find_package`. `drivers.rs` queda fuera: es `ARQ-03`, diferido por decisión.
- **Dónde**: `src/core/config.rs`; `src/core/specs.rs:191`; `src/core/safe_fs.rs:485-488`; `src/core/sys_tools.rs:28,80`;
  `src/core/app_store.rs:662`.
- **Propuesta**: borrarlo, o conectar `AppConfig` si se va a usar (ver `AR-20`). Tras `AR-03`, clippy marcará lo que quede.

<a id="ar-06"></a>
### AR-06 · `CoreError` clasifica por módulo y no por causa, y casi nunca guarda el error de origen
- **Prioridad**: P2 · **Esfuerzo**: L · **Evidencia**: confirmado (161 `source: None`) · **Estado**: Pendiente
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

<a id="ar-07"></a>
### AR-07 · La carpeta de datos la resuelve la UI y sus nombres están repartidos
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: las vistas llaman a `ensure_admin_only_data_dir()` y construyen los managers; `OptimizationEngine` y `DnsManager`
  reciben la ruta, el Debloater la fija por dentro. `"staging"`, `"RegBackups"`, `"backup.json"`, `"dns_backup.json"` y
  `"debloat_backup.json"` están en cinco archivos, y la raíz vive en `trust.rs` mezclada con las descargas.
- **Dónde**: `src/ui/views/dashboard.rs:74,143`; `src/ui/views/dns.rs:25,132,179`; `src/core/debloater.rs:654-655`;
  `src/core/app_store.rs:681`; `src/core/dependencies.rs:229`; `src/core/uninstaller.rs:287`; `src/core/trust.rs:79-127`.
- **Propuesta**: `core/data_dir.rs` con `DataDir::open()` y métodos con nombre (`staging()`, `reg_backups()`,
  `optimization_backup()`…); `open_default()` en los managers; el Debloater recibe el `DataDir` como los demás.

<a id="ar-08"></a>
### AR-08 · `revert_all_changes` vive en el desinstalador
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: orquesta motor, DNS y Debloater, y es la única razón de que `uninstaller` dependa de esos tres módulos. Esconde el
  `--rollback` del producto en un archivo de 1509 líneas.
- **Dónde**: `src/core/uninstaller.rs:18-21,1296-1365`.
- **Propuesta**: `core/rollback.rs` con `revert_all_changes` y `FullRevertReport`.

<a id="ar-09"></a>
### AR-09 · «Descargar, verificar y ejecutar un instalador» está escrito dos veces
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: la tienda y las dependencias repiten staging con nonce de `SystemTime` (el nonce aparece 8 veces en total),
  `DownloadPolicy`, `secure_download_and_verify` y borrado si falla, `reopen_verified_for_exec`, `run_in_job(…, 300 s)`,
  `exit_code != 1638` y borrado final. Es el camino del patrón 2: una corrección de seguridad tendría que hacerse dos veces.
  `updater.rs:361-380` repite la parte de descarga.
- **Dónde**: `src/core/app_store.rs:681-780`; `src/core/dependencies.rs:229-290`; `src/core/updater.rs:361-380`.
- **Propuesta**: `trust::download_to_staging(…) -> StagedFile` (su `Drop` borra) y `download_and_run(policy, args, opts) ->
  ProcessExit`; `const ERROR_PRODUCT_VERSION: u32 = 1638`; `fn unique_suffix()`.

<a id="ar-10"></a>
### AR-10 · Hay cinco formas de lanzar procesos
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `process::run_in_job` (Job y timeout); `debloater::run_ps` (lectores propios, mata solo al padre); `Command::output()`
  sin timeout en el escaneo del Debloater, con dos bloques casi iguales; `Command` con `reg.exe` en el desinstalador; `Command` en el
  updater. Las garantías del patrón 3 dependen de qué camino tome cada llamador. `get_powershell_path` duplica `system32_dir` y
  `CREATE_NO_WINDOW` se define dos veces.
- **Dónde**: `src/core/debloater.rs:21,522-624,667-722`; `src/core/uninstaller.rs:285,310`; `src/core/updater.rs:397`.
- **Propuesta**: `process::run_in_job_capture(launch, timeout, cancel) -> (ProcessExit, stdout, stderr)` y migrar PowerShell y
  `reg.exe`; `ps_json_query(script)` sobre ella; una sola constante.

<a id="ar-11"></a>
### AR-11 · Módulos de más de 900 líneas con responsabilidades mezcladas
- **Prioridad**: P2 · **Esfuerzo**: L · **Evidencia**: observado · **Estado**: Pendiente
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

<a id="ar-12"></a>
### AR-12 · `main.rs` hace demasiado y los flags de la CLI son literales sueltos
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: confirmado · **Estado**: Pendiente
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

<a id="ar-13"></a>
### AR-13 · El ciclo «tarea bloqueante con progreso» está copiado seis veces y el `JoinError` se maneja de cinco maneras
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `Arc<Mutex<P>>` → `spawn_blocking` → `while !handle.is_finished() { lock; set; sleep }` aparece en archiver,
  cleaner, dashboard (dos), debloater y `main.rs`, con 100, 120 o 150 ms. El `JoinError` va por `unwrap_or_else`, por
  `.map_err(to_string)`, por `.ok()` y por `unwrap_or_default()` (un panic del worker se ve como «vacío»); `CoreError::Panic` casi
  no se usa. Las cargas iniciales usan `use_hook(spawn)`, `use_future` o `use_effect` sin criterio, ninguna `use_resource`. Un arreglo
  de concurrencia se aplicaría en una sola copia.
- **Dónde**: `src/main.rs:117-158`; `src/ui/views/archiver.rs:58-97`; `cleaner.rs:50-75`; `dashboard.rs:69-92,138-158`;
  `debloater.rs:146-170`; `src/ui/mod.rs:116,157,172,222,269,389-398`; `uninstaller.rs:96,171,223,287,321`; `dns.rs:73`.
- **Propuesta**: `ui/task.rs` con `run_blocking<T>(f) -> CoreResult<T>` (JoinError → `CoreError::Panic`) y
  `spawn_with_progress(signal, interval, f)`; `use_resource` para las cargas.

<a id="ar-14"></a>
### AR-14 · `ui/mod.rs` mezcla el shell, el estado global y el worker de instalación
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `ToolsSharedState` junta el progreso de instalación con el `tool_log` de RAM y menú contextual;
  `run_installer_worker` (175 líneas) vive en el módulo raíz; los 9 botones de navegación están copiados a mano; `dep_states` se
  calcula dos veces; el `impl Default` manual equivale a `#[derive(Default)]`.
- **Dónde**: `src/ui/mod.rs:73-99,109-114,126-301,165-170,452-499`.
- **Propuesta**: `ui/installer.rs` para worker y tipos; `tool_log` aparte; navegación desde `const TABS`.

<a id="ar-15"></a>
### AR-15 · Reglas de negocio en las vistas
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: DNS arma el CSV y filtra adaptadores virtuales en la vista; el Debloater decide la preselección recomendada en la
  vista; Hardware decide ruta y nombre de la exportación.
- **Dónde**: `src/ui/views/dns.rs:125-141`; `src/ui/views/debloater.rs:15-33`; `src/ui/views/hardware_tab.rs:89-104`.
- **Propuesta**: `DnsManager::apply_preset_to_physical(&preset)` con error tipado; `DebloatScanResult::recommended_selection()`;
  usar `specs::export_specs_json`.

<a id="ar-16"></a>
### AR-16 · Números mágicos repetidos entre la UI y el core
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: cuota de extracción `50 GiB` en la vista y en `main.rs`; `total_steps: 6` en la vista del limpiador cuando el core
  cuenta como máximo 5; `total_steps: 13` en el Dashboard; `SensorMonitor::new(1000)` en la vista frente a 1500 en `AppConfig`;
  timeout de 300 s en tres sitios; hosts de GitHub repetidos en el catálogo y el updater.
- **Dónde**: `src/ui/views/archiver.rs:69`; `src/main.rs:127`; `src/ui/views/cleaner.rs:43`; `src/core/cleaner.rs:288`;
  `src/ui/views/dashboard.rs:58,127`; `src/ui/views/sensors.rs:18,45`; `src/core/app_store.rs:209-345,766`; `src/core/updater.rs:38`.
- **Propuesta**: constantes en core (`DEFAULT_EXTRACT_QUOTA`, `INSTALLER_TIMEOUT`, `DEFAULT_POLL_MS`, `GITHUB_HOSTS`); el progreso
  inicial con `total_steps: 0` y que el core lo rellene.

<a id="ar-17"></a>
### AR-17 · Textos de presentación generados en core
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `specs::to_compact_summary` tiene etiquetas en español; `win_common` usa textos de relleno como valores («Placa Base
  Estándar», «Unidad Física {}»); `AppCategory::label(en)` es bilingüe y `DebloatCategory::label()` solo español: dos estrategias de
  idioma en core. Es el diseño que hay detrás de `UI-11` (diferido); este ID solo propone la regla para el código nuevo.
- **Dónde**: `src/core/specs.rs:133-160`; `src/core/win_common.rs:510,537,621,624`; `src/core/app_store.rs:55`;
  `src/core/debloater.rs:33`.
- **Propuesta**: core devuelve datos (`Option<String>`, enums) y la UI pone el texto (`IN-02`).

<a id="ar-18"></a>
### AR-18 · Tres versiones del crate `windows` y dependencias duplicadas
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: confirmado (`Cargo.lock`) · **Estado**: Pendiente
- **Qué pasa**: `windows` 0.57 (por `sysinfo` 0.31), 0.58 (propia) y 0.61 (Dioxus y `webview2-com`); `windows-sys` en cuatro
  versiones; `thiserror` 1 y 2; `dirs` 5 y 6. `sysinfo` solo cuenta núcleos, RAM y marca del CPU, que `specs` ya lee. Más tiempo de
  compilación, binario más grande y tres juegos de bindings.
- **Dónde**: `Cargo.toml`; `Cargo.lock`; `src/core/win_common.rs:424-471`.
- **Propuesta**: quitar `sysinfo` (con `AR-04`); subir `windows` a 0.61, `thiserror` a 2 y `dirs` a 6, revisando los cambios de API.

<a id="ar-19"></a>
### AR-19 · Tres estilos de API en core
- **Prioridad**: P3 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: structs unitarios con solo funciones asociadas (`SysTools`, `ArchiveManager`, `UninstallerManager`…), managers con
  estado (`DnsManager::new(dir)`) y funciones libres (`registry`, `services`, `power`). Cuesta descubrir la API y causa la diferencia
  de `AR-07`.
- **Propuesta**: funciones libres para lo que no tiene estado; struct solo cuando hay estado (`DataDir`, mutex).

<a id="ar-20"></a>
### AR-20 · La persistencia de tema e idioma está duplicada
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `i18n.rs:31-54` y `theme.rs:66-89` son el mismo código con otro nombre de archivo, con un `path.exists()` redundante
  y lectura sin límite, mientras `AppConfig` (sin uso) sí lee acotado.
- **Propuesta**: `UiPrefs { theme, lang }` en un solo `prefs.json`, con `read_bounded_bytes`.

# Código limpio y reutilizable (`CL-`)

<a id="cl-01"></a>
### CL-01 · 213 bloques `unsafe` sin ningún comentario `SAFETY`
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: confirmado · **Estado**: Pendiente
- **Qué pasa**: 213 `unsafe {` (40 en `sensors.rs`, 31 en `trust.rs`, 29 en `win_common.rs`) y cero `SAFETY`; un solo `# Safety`.
  `resolve_proc<T>` hace `transmute_copy` a cualquier `T` sin comprobar tamaño ni documentar el contrato; dos `unsafe impl Send` sin
  justificar; `read_wmi_*` son `unsafe fn` que no exigen nada al llamador. `safe_fs::duplicate_file_handle` es una `fn` segura que
  hace `File::from_raw_handle` (posible doble cierre) y su nombre y comentario no corresponden a lo que hace. Con este volumen de FFI,
  el comentario es lo único que permite revisar un cambio sin releer la API Win32. La UI tiene 0 `unsafe`.
- **Dónde**: `src/core/sensors.rs:67,98,324,907,921`; `src/core/safe_fs.rs:485-488`; `src/core/win_common.rs:187`.
- **Propuesta**: `#![forbid(unsafe_code)]` en `src/ui/mod.rs` (gratis hoy); `clippy::undocumented_unsafe_blocks` y
  `missing_safety_doc` módulo por módulo, empezando por `trust`, `safe_fs`, `process` y `elevation`; `debug_assert` de tamaño en
  `resolve_proc`; `read_wmi_*` seguras; justificar los `Send`; borrar `duplicate_file_handle`.

<a id="cl-02"></a>
### CL-02 · Wrappers RAII duplicados y liberaciones a mano
- **Prioridad**: P1 · **Esfuerzo**: S/M · **Evidencia**: confirmado (`WinInetHandle`) · **Estado**: Pendiente
- **Qué pasa**: `WinInetHandle` definido igual en `trust.rs` y `updater.rs`; `OwnedHandle` en `specs.rs` duplica `SafeHandle`;
  `LocalFree` manual en seis sitios (un `?` insertado en medio fuga); `CoTaskMemFree` manual en `sensors.rs` aunque `dependencies.rs`
  ya usa `CoTaskMemPWSTR`; el descriptor desde SDDL está escrito dos veces.
- **Dónde**: `src/core/trust.rs:53-76,131-153,239,363`; `src/core/updater.rs:51-74`; `src/core/specs.rs:31-41`;
  `src/core/power.rs:113,183`; `src/core/win_common.rs:298-321`; `src/core/sensors.rs:932`.
- **Propuesta**: `WinInetHandle` en `win_common`; borrar `OwnedHandle`; `LocalMem<T>` con `Drop`;
  `SecurityDescriptor::from_sddl(&str)`.

<a id="cl-03"></a>
### CL-03 · Cliente HTTP de WinInet duplicado y user-agent desactualizado
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `updater.rs` repite casi línea por línea a `trust.rs`: `InternetOpenW`, los tres timeouts, `InternetOpenUrlW` con
  los mismos flags, estado 200 y bucle de lectura con tope. `trust.rs` manda `"OtterSwiss/3.0"` fijo; `updater.rs` usa
  `CARGO_PKG_VERSION`.
- **Dónde**: `src/core/updater.rs:184-280`; `src/core/trust.rs:438-600`.
- **Propuesta**: `core/http.rs` con `Session::open`, `open_secure(url) -> Response` (`status`, `final_url`, `content_length`) y
  `read_chunks(max, cancel, f)`; `trust` y `updater` quedan como consumidores.

<a id="cl-04"></a>
### CL-04 · La ubicación de registro de un resto se modela con dos booleanos y campos opcionales sueltos
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `LeftoverItem` lleva `path`, `reg_is_hkcu`, `reg_wow64_32`, `reg_subkey` y `reg_value_name`, válidos según `kind`
  (25 usos de los booleanos). La conversión a colmena y flags se reescribe cuatro veces y el `id` se arma con
  `format!("reg:{}:{}:{}", …)`. El `match` de colmena del Debloater se repite tres veces más.
- **Dónde**: `src/core/uninstaller.rs:60-74,888,1005,1155-1169,1195-1206`; `src/core/debloater.rs:959,1011,1080`.
- **Propuesta**: `RegLocation { hive: RegHive, view: RegView }` con `root()` y `flags(access)`;
  `enum LeftoverTarget { Path, RegKey, RegValue }` del que se deriva el `id`.

<a id="cl-05"></a>
### CL-05 · Funciones de unas 400 líneas
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `apply_profile_inner` (387 líneas; el bloque «resultado → push → progreso» 15 veces y el booleano `backup_failed`
  consultado en tres sitios), `scan_leftovers` (389, nueve niveles de anidamiento, el literal `LeftoverItem` tres veces),
  `AdlSession::poll` (199), `execute_debloat`, `poll_wmi_acpi`, `secure_download_and_verify_with_progress` (261). Las vistas
  `UninstallerView` (524), `DashboardView` (515), `DebloaterView` (438), `DnsView` (364), `App` (344) y `ToolsView` (314). No se
  pueden probar por partes y cuesta revisar en ellas los patrones 1 y 6.
- **Dónde**: `src/core/engine.rs:194-583`; `src/core/uninstaller.rs:680-1071`; `src/core/sensors.rs:365-566,725-880`;
  `src/core/debloater.rs:756-916`; `src/core/trust.rs`; `src/ui/views/*.rs`.
- **Propuesta**: `StepRecorder` (`skip`, `fail`, `applied`) y una función por tipo de tweak que devuelva `ControlFlow`; un
  `scan_*` por fuente de restos; las vistas en subcomponentes (resultados, modal de restos, lista de apps).

<a id="cl-06"></a>
### CL-06 · Helpers Win32 escritos varias veces
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (`known_folder`) · **Estado**: Pendiente
- **Qué pasa**: `known_folder` cuatro veces (una idéntica byte a byte en el desinstalador); System32 o Windows tres veces;
  `read_cstr_at_offset` y `read_cstr_at`; `to_wide_null` reescrito a mano en `specs.rs`. Son rutinas del patrón 5: cada copia es otro
  sitio donde un arreglo de seguridad puede no llegar.
- **Dónde**: `src/core/win_common.rs:73,543`; `src/core/uninstaller.rs:130`; `src/core/cleaner.rs:43`; `src/core/trust.rs:78`;
  `src/core/process.rs:40`; `src/core/debloater.rs:523-533`; `src/main.rs:77-83`; `src/core/specs.rs:751-754,839`.
- **Propuesta**: un solo `known_folder(id) -> CoreResult<PathBuf>`, `system32_dir()` y `read_cstr_at` en `win_common`.

<a id="cl-07"></a>
### CL-07 · El reporte de progreso por pasos está copiado en core
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: el mismo bloque `lock → current_step, total_steps, label, percent` cuatro veces, con tope `min(100)` o `min(99)`;
  `CleanProgress`, `DebloatProgress` y `EngineProgress` repiten los cuatro campos.
- **Dónde**: `src/core/engine.rs:221,618`; `src/core/debloater.rs:770`; `src/core/cleaner.rs:290`.
- **Propuesta**: `struct StepProgress` embebido y `fn report_step(progress, idx, total, label)`.

<a id="cl-08"></a>
### CL-08 · Apertura del SCM y del servicio duplicada
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Dónde**: `src/core/services.rs:42-63,223-245`.
- **Propuesta**: `open_service(name, access) -> CoreResult<Option<ScHandle>>` sin lista blanca, y que `open_allowed_service` valide
  y la llame.

<a id="cl-09"></a>
### CL-09 · Consulta WMI duplicada
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `ConnectServer`, `CoSetProxyBlanket`, `ExecQuery` y el bucle `Next` dos veces; décimas de kelvin a grados tres veces.
- **Dónde**: `src/core/sensors.rs:735-800,803-870,774,784,832`.
- **Propuesta**: `wmi_query(locator, ns, wql) -> Vec<IWbemClassObject>` y `decikelvin_to_c`.

<a id="cl-10"></a>
### CL-10 · Cuatro modelos distintos de «mensaje de estado» en la UI
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `Option<(String, Tone)>` (desinstalador, DNS); `Option<String>` más `is_error: bool` (archivador, con badge propio
  sin `role`); `error_banner` y `status_banner` a la vez (Debloater); `tool_log` con el tono deducido por
  `msg.starts_with("Error")`; Hardware muestra errores en color de acento.
- **Dónde**: `src/ui/views/uninstaller.rs:27`; `dns.rs:49`; `archiver.rs:25-26,272-276`; `debloater.rs:86`; `tools.rs:229`;
  `hardware_tab.rs:23,72`.
- **Propuesta**: `struct Notice { kind, tone }` (con `IN-03`, el `kind` es un evento y no texto) y siempre `StatusBanner`.

<a id="cl-11"></a>
### CL-11 · Faltan componentes y el mismo `rsx!` se repite
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: barra de progreso siete veces (con pistas `--surface` o `--bg` y transiciones de 0,15, 0,2 o 0,25 s, sin
  `role="progressbar"`); fila con checkbox seis veces; cabecera de tarjeta dos veces en `components`; Hardware arma `div.card` a mano;
  el Dashboard copia el markup de `Card` porque no admite acciones.
- **Dónde**: `src/ui/mod.rs:583`; `src/main.rs:248`; `archiver.rs:157`; `cleaner.rs:157,167,229`; `dashboard.rs:313-320,405`;
  `debloater.rs:378,428,473`; `tools.rs:218,446`; `uninstaller.rs:567`; `components/mod.rs:7-13,127-133`;
  `hardware_tab.rs:129-213`.
- **Propuesta**: `ProgressBar { percent }` accesible, `CheckRow`, `CardHeader`, y `Card` con `actions` y `tone` (ver `TE-12`).

<a id="cl-12"></a>
### CL-12 · Formato de tamaños y duraciones en tres versiones
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `format_mb`, `format_size_mb` y `format_capacity`, más divisiones por `1024.0 * 1024.0` sueltas; los formatos
  divergen («< 0.01 MB» y «0.00 MB»). El ETA `"~{}s"` está dos veces y da «~3600s».
- **Dónde**: `src/ui/views/cleaner.rs:11`; `uninstaller.rs:43`; `hardware_tab.rs:251`; `archiver.rs:102,121-126`;
  `src/main.rs:203-208`.
- **Propuesta**: `ui/format.rs` con `bytes(n)` y `duration(secs)` (con sus tests; ya hay en `hardware_tab.rs:267`).

<a id="cl-13"></a>
### CL-13 · Strings mágicos donde va un enum
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: el modo del archivador es `use_signal(|| "extract")` comparado en siete sitios, y `MiniArchiveJob` usa
  `is_extract: bool`; los ids de runtime (`"webview2"`, `"vcredist_x64"`) son strings.
- **Dónde**: `src/ui/views/archiver.rs:19-215`; `src/main.rs:24,370`; `src/core/dependencies.rs:206-211,291`.
- **Propuesta**: `enum ArchiveMode` y `enum RuntimeId` en core. Los flags de CLI van en `AR-12`.

<a id="cl-14"></a>
### CL-14 · Dos `too_many_arguments` que tapan un diseño mejorable
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Dónde**: `src/ui/views/uninstaller.rs:141` (10 parámetros, 8 `Signal`); `src/core/archiver.rs:332` (8).
- **Propuesta**: `#[derive(Clone, Copy)] struct UninstallerSignals`; `struct ExtractCtx`.

<a id="cl-15"></a>
### CL-15 · Rutas tratadas como `String` con pérdida
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `wide_ptr_to_string_and_free` devuelve `String` lossy también para rutas; `to_string_lossy().to_ascii_lowercase()` se
  usa como clave de identidad de ruta (no pliega mayúsculas fuera de ASCII); el menú contextual escribe la ruta lossy en el registro.
- **Dónde**: `src/core/win_common.rs:53`; `src/core/uninstaller.rs:358,397,809,1014`; `src/core/archiver.rs:402`;
  `src/core/context_menu.rs:87,117`.
- **Propuesta**: `OsString::from_wide` para rutas, un `path_key(&Path)` único y `OsStr` hasta el borde.

<a id="cl-16"></a>
### CL-16 · Detalles sueltos
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- `context_menu.rs:88-89`: `cmd.contains(&expected) || cmd.contains(&current_exe)`; la primera condición sobra.
- `main.rs:320`: `native_message(…, ask_yes_no: bool, is_error: bool)`; mejor enums o dos funciones.
- `format!("{}", e)` en vez de `e.to_string()` (4 casos); 7 `redundant_clone`.
- Clones completos en cada render: `uninstaller.rs:~410` (`filtered`) y `debloater.rs:115`.

<a id="cl-17"></a>
### CL-17 · Comentarios
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: los 92 comentarios con ID de `BUGS.md` dan trazabilidad, pero algunos cuentan la historia en vez de la regla
  (`components/mod.rs:23`); «Transversal 6» no se define en ningún sitio (`backup.rs:119`, `tweaks_catalog.rs:107`); ocho archivos
  abren con `// src/…`, que repite la ruta.
- **Propuesta**: la regla primero y el ID entre paréntesis; definir o quitar «Transversal»; borrar las cabeceras de ruta.

<a id="cl-18"></a>
### CL-18 · Tests: carpetas temporales sin limpieza y un test de sistema monolítico
- **Prioridad**: P3 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: dos helpers de carpeta temporal duplicados y sin guarda RAII (si una aserción falla, queda en `%TEMP%`);
  `vm_sistema.rs` usa nombres fijos (`otter-arch`) y es un solo `#[test]` de 480 líneas, así que no se puede correr un área sola.
  Funciones puras que viven en vistas (`uninstall_exit_is_success`, `recommended_ids`, `format_*`) no se pueden probar sin Dioxus.
- **Dónde**: `src/core/archiver.rs:976`; `src/core/safe_fs.rs:518`; `tests/vm_sistema.rs:275,303,468`.
- **Propuesta**: `test_util::TempDir` con `Drop` (o `tempfile` como dev-dependency); un `#[test] #[ignore]` por área con helper
  común; mover esas funciones a core (`AR-01`, `AR-15`).

# Temas, tokens y CSS (`TE-`)

Contrastes calculados con la fórmula WCAG sobre los hex de `assets/main.css` y recalculados por el agente principal. Los defectos
de contraste de texto están en `UI-18` (`docs/BUGS.md`).

<a id="te-01"></a>
### TE-01 · Sin `color-scheme` ni `accent-color`: barras de desplazamiento y checkboxes claros en los temas oscuros
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: confirmado (0 resultados de `color-scheme`) · **Estado**: Pendiente
- **Qué pasa**: WebView2 dibuja scrollbars, checkboxes e inputs en esquema claro si no se declara otro: gris claro y azul de
  Windows sobre fondos casi negros, y el checkbox no sigue el acento del tema.
- **Dónde**: `assets/main.css` (bloques `[data-theme]`); listas con scroll en `:295,441,449`; checkboxes en `cleaner.rs`,
  `debloater.rs`, `uninstaller.rs`.
- **Propuesta**: `color-scheme: dark` en los cinco temas oscuros y `light` en Claro; `accent-color: var(--accent)`;
  `scrollbar-color: var(--border) transparent`. Depende de `TE-04` para que llegue a `html`.

<a id="te-02"></a>
### TE-02 · 185 estilos inline: el CSS no tiene clases de layout ni de componente
- **Prioridad**: P1 · **Esfuerzo**: M/L · **Evidencia**: confirmado (185 `style:`) · **Estado**: Pendiente
- **Qué pasa**: `flex column gap-2` 19 veces, `xs muted` 19, fila con `border-bottom` 9, botón chico 9, inputs con estilo completo
  3. UI-10 pasó los valores a tokens pero los dejó inline: un cambio de diseño toca de 7 a 19 sitios, las copias ya divergen y el
  inline no admite `:hover`, `:disabled`, `:focus-visible` ni media queries. `AGENTS.md:10` dice que `main.css` concentra los
  estilos.
- **Propuesta**: capa de utilidades (`.stack-2`, `.stack-3`, `.row`, `.row-between`, `.wrap`, `.text-sm`, `.text-muted`) y de
  componentes (`.progress`, `.list-row`, `.input`, `.btn-sm`, `.card-kicker`, `.select-row`); migrar vista por vista junto con
  `CL-11`.

<a id="te-03"></a>
### TE-03 · No hay opción «según el sistema» y el selector de tema es un ciclo de seis
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: sin `theme.json` la app abre en Oscuro; se ignoran `prefers-color-scheme`, `AppsUseLightTheme` y el contraste alto.
  Para ir de Oscuro a Claro hay que pulsar cinco veces el mismo botón.
- **Dónde**: `src/ui/theme.rs:7,28-37,80`; `src/ui/mod.rs:508-515`.
- **Propuesta**: `Theme::System` por defecto; un menú de temas en vez del ciclo; `@media (forced-colors: active)`.

<a id="te-04"></a>
### TE-04 · Los tokens viven en un `div` y `:root` no tiene colores
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `data-theme` está en `div.app-root-frameless`, no en `<html>`; por eso `html, body { color: var(--text) }` no hace
  nada y `color-scheme` no puede leer el tema. `rgba(var(--accent-rgb, 59, 130, 246), …)` lleva un fallback fijo que solo existe
  porque `:root` no define colores; un `data-theme` desconocido deja la app sin colores.
- **Dónde**: `assets/main.css:18,157,241,328`; `src/ui/mod.rs:426`; `src/main.rs:215`.
- **Propuesta**: poner el atributo en `document.documentElement`, o `:root, [data-theme="dark"] { … }` como respaldo; quitar los
  fallbacks literales.

<a id="te-05"></a>
### TE-05 · Un solo nivel de tokens y `--bg` copiado a mano en Rust
- **Prioridad**: P2 · **Esfuerzo**: S (test) / M (capas) · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: cada tema define 18 colores semánticos con hex directo, sin paleta primitiva ni tokens de componente.
  `background_rgba` repite los seis `--bg` y nada comprueba que coincidan: si se desincronizan vuelve el parpadeo de UI-10.
  `--accent-rgb` duplica `--accent` cuando ya se usa `color-mix`.
- **Dónde**: `assets/main.css:19-148,404-407`; `src/ui/theme.rs:40-48`.
- **Propuesta**: test que lea `include_str!("../../assets/main.css")` y compare `--bg` con `background_rgba()`; quitar
  `--accent-rgb`; a mediano plazo `--control-bg`, `--control-border`, `--track-bg`.

<a id="te-06"></a>
### TE-06 · Estados de interacción casi invisibles
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: derivado · **Estado**: Pendiente
- **Qué pasa**: `--surface-hover` contra `--surface` da 1,05 en Claro y 1,08–1,18 en el resto: el hover del menú y de los botones
  de ventana casi no se ve. `.btn-danger:hover` usa `opacity: 0.9` en vez de un token. Ningún botón tiene `:active`. (El contraste
  del texto en el hover primario está en `UI-18`.)
- **Dónde**: `assets/main.css:234,263,283,366-368,386`.
- **Propuesta**: `--surface-active` y `--danger-hover` con diferencia perceptible; `:active` en botones.

<a id="te-07"></a>
### TE-07 · Fondos blancos translúcidos fijos que desaparecen en Claro
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Dónde**: `assets/main.css:210` (`.top-version`), `:251` (`.theme-toggle-btn`).
- **Propuesta**: `color-mix(in srgb, var(--text) 6%, transparent)` o un `--surface-subtle` por tema.

<a id="te-08"></a>
### TE-08 · Bordes de controles por debajo de 3:1 (WCAG 1.4.11)
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: derivado · **Estado**: Pendiente
- **Qué pasa**: `--border` sobre `--surface` va de 1,21 (Oscuro) a 1,52 (Esmeralda) y es el único borde de inputs,
  `.btn-secondary` y filas seleccionables: los campos de texto apenas se distinguen.
- **Propuesta**: `--border` decorativo como está y un `--control-border` ≥ 3:1 para controles.

<a id="te-09"></a>
### TE-09 · Sin `prefers-reduced-motion` y logo animado en bucle
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (0 `@media`) · **Estado**: Pendiente
- **Dónde**: transiciones en `assets/main.css:230,259,280,323,358` y en las barras inline; `logo.gif` (30 fotogramas) en
  `src/ui/mod.rs:346-350`.
- **Propuesta**: `@media (prefers-reduced-motion: reduce)` que anule transiciones y animaciones; marca estática bajo `reduce`.

<a id="te-10"></a>
### TE-10 · Logo de 1,7 MB en base64 para mostrarse a 30×30
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (1 716 706 bytes) · **Estado**: Pendiente
- **Qué pasa**: `logo.gif` (640×360) va codificado en base64 en el binario y el DOM (unos 2,3 MB de texto) para un icono de 30px
  recortado de 16:9 a cuadrado. El README lo pinta a 128×128 y lo deforma.
- **Dónde**: `src/ui/mod.rs:349`; `assets/main.css:193-198`; `README.md:2`.
- **Propuesta**: `brand-32.png`/`brand-64.png` o un SVG cuadrado de pocos KB con `asset!()`; el GIF solo para el README.

<a id="te-11"></a>
### TE-11 · Sin reset base: márgenes del navegador y sin `box-sizing`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (la mini ventana: inferido) · **Estado**: Pendiente
- **Qué pasa**: el `<h3>` de `Card` conserva `margin-block: 1em` y `hardware_tab.rs:63` lo compensa a mano: ritmo vertical distinto
  entre tarjetas. La mini ventana suma `border: 1px` a `100vw × 100vh` con `content-box`, así que probablemente recorta los bordes
  derecho e inferior.
- **Dónde**: `assets/main.css:166-167,337`; `src/main.rs:216`.
- **Propuesta**: `*, *::before, *::after { box-sizing: border-box }`, `h1–h4, p { margin: 0 }`, espaciado con `gap`.

<a id="te-12"></a>
### TE-12 · Componentes que se esquivan: `Card`, `StatusBanner` y `badge`
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: Hardware arma `div.card` con un padding redundante; Archivos y la mini ventana usan `badge` (pastilla de 999px) como
  mensaje de varias líneas sin `role`; el botón de actualización mezcla `badge-warning` con borde de acento; Herramientas pinta una
  tarjeta de aviso inline.
- **Dónde**: `src/ui/views/hardware_tab.rs:129,155,174,197,213`; `archiver.rs:273-276`; `src/main.rs:240`; `src/ui/mod.rs:443-444`;
  `tools.rs:208-211`.
- **Propuesta**: `Card` con `actions` y `tone`, `StatusBanner` en Archivos y la mini ventana, `.btn-sm`/`.chip`, `.card-warning`.

<a id="te-13"></a>
### TE-13 · Controles segmentados y pestañas sin estado accesible
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: la opción elegida se marca alternando `btn-primary`/`btn-secondary` y la pestaña activa solo con la clase `active`; 0
  `aria-pressed` y 0 `aria-current`. El estado solo se comunica con color, y un primario de llamada a la acción se confunde con una
  selección.
- **Dónde**: `src/ui/views/archiver.rs:182,188,236-248`; `uninstaller.rs:706-724`; `dns.rs:302`; `sensors.rs:121`;
  `src/ui/mod.rs:453-496`.
- **Propuesta**: `.segmented > button[aria-pressed="true"]` y `aria-current="page"`; estilar por atributo.

<a id="te-14"></a>
### TE-14 · Objetivos táctiles chicos y tipografía fuera de la escala
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `.theme-toggle-btn` mide unos 21px de alto y los botones chicos inline unos 22px (WCAG 2.5.8 pide 24px); 11px fijos
  en `.top-version`, `.top-nav-item` y `.badge` (menos que `--font-size-xs`); 14px y 12px fijos en vez de tokens; `--font-size-lg`
  no se usa.
- **Dónde**: `assets/main.css:201,207,227,253-255,277,400`; `tools.rs:312`; `uninstaller.rs:707-725`.
- **Propuesta**: `min-height: 28px` en controles; `--font-size-2xs` o subir a 12px; usar los tokens y borrar los sin uso.

<a id="te-15"></a>
### TE-15 · La barra superior probablemente no entra a 960px
- **Prioridad**: P2 · **Esfuerzo**: S/M · **Evidencia**: inferido (estimado unos 1150px con los textos en español) · **Estado**: Pendiente
- **Qué pasa**: nueve botones con `white-space: nowrap`, sin `overflow`, `flex-shrink` ni media queries, y ventana mínima de 960px.
  Se cruza con la prueba pendiente de la ventana en tamaño chico.
- **Dónde**: `src/main.rs:482`; `assets/main.css:215-232`.
- **Propuesta**: verificarlo en la app; `@media (max-width: 1150px)` que oculte la versión y reduzca el padding, o `overflow-x: auto`.

<a id="te-16"></a>
### TE-16 · `!important` y `pointer-events` sin motivo
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: 10 `!important`; tres `pointer-events: auto !important` sin ningún `none` que anular; cinco en `html, body`; dos en
  el hover del botón de cerrar, que se resuelve con especificidad.
- **Dónde**: `assets/main.css:151-158,171,289-290,298,324`.

<a id="te-17"></a>
### TE-17 · Valores sueltos sin token
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: z-index 100 y 9999; `0.15s ease` siete veces más 0,2 y 0,25 inline; pesos 400–700 y `bold`; radios de 999px, 8px y
  4px fuera de `--radius-*`; `#e81123` del botón de cerrar; overlay `rgb(0 0 0 / 0.72)`; medidas de layout; `monospace` y
  `min-width: 220px` inline.
- **Propuesta**: `--z-titlebar`, `--z-modal`, `--duration-fast`, `--ease`, `--fw-*`, `--radius-pill`, `--overlay`, `--font-mono`,
  `--win-close`, `--titlebar-h`, `--content-max`.

<a id="te-18"></a>
### TE-18 · `transition: all`
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Dónde**: `assets/main.css:230,358`.
- **Propuesta**: listar `background-color`, `border-color`, `color`, `opacity`.

<a id="te-19"></a>
### TE-19 · Pila de fuentes pensada para la web y glifos de ventana con caracteres de texto
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Dónde**: `assets/main.css:2` (`-apple-system, BlinkMacSystemFont, Roboto…`); `src/ui/mod.rs:519-530`; `src/main.rs:233`
  («─», «□», «✕»).
- **Propuesta**: `"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`; iconos con `Segoe Fluent Icons`/`Segoe MDL2 Assets`.

<a id="te-20"></a>
### TE-20 · `user-select: none` global impide copiar rutas y errores
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: no se pueden copiar rutas de restos, mensajes de error, notas de versión ni el comando del Dashboard.
- **Dónde**: `assets/main.css:159`; `uninstaller.rs:589`; `src/ui/mod.rs:573-577`; `dashboard.rs:359`.
- **Propuesta**: `none` solo en la barra de título y botones; `text` en `.card`, `code`, `.modal-list`, `.status-banner`.

<a id="te-21"></a>
### TE-21 · Nord: `--text-muted` casi igual a `--text`
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: derivado (1,17:1 entre ambos; 2–3,75 en los demás temas) · **Estado**: Pendiente
- **Dónde**: `assets/main.css:89-90`.
- **Propuesta**: un `--text-muted` más apagado que siga ≥ 4,5:1 sobre `--surface`.

<a id="te-22"></a>
### TE-22 · CSS inyectado como `<style>` en dos sitios y la mini ventana sin color de fondo inicial
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado (el destello: inferido) · **Estado**: Pendiente
- **Dónde**: `src/ui/mod.rs:422`; `src/main.rs:211,465-469,490`.
- **Propuesta**: cargar el CSS una vez (`with_custom_head` o `document::Stylesheet` con `asset!()`);
  `.with_background_color(Theme::load_persisted().background_rgba())` en la mini ventana.

<a id="te-23"></a>
### TE-23 · `main.css` sin secciones
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Propuesta**: cabeceras para tokens, temas, reset y base, layout, componentes, utilidades, accesibilidad y medios; la regla de
  foco (`:420`) sale de entre los componentes.

<a id="te-24"></a>
### TE-24 · Deshabilitado solo con opacidad y cursor de mano en labels deshabilitados
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Dónde**: `assets/main.css:390`; `cleaner.rs:157`; `debloater.rs:428`.
- **Propuesta**: `.btn:disabled` con fondo y texto propios; `label:has(input:disabled) { cursor: not-allowed; opacity: .6 }`.

# Idiomas (`IN-`)

<a id="in-01"></a>
### IN-01 · El idioma por defecto es español fijo y hay dos fuentes de idioma
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: confirmado · **Estado**: Pendiente
- **Qué pasa**: sin `lang.json`, `load_persisted` devuelve `Es`. El instalador detecta el idioma del sistema pero no se lo pasa a
  la app, y el core usa `GetUserDefaultUILanguage` para la tienda. Un usuario con Windows en inglés instala en inglés y la app abre
  en español; con la app en inglés y Windows en español, la tienda baja Firefox en español.
- **Dónde**: `src/ui/i18n.rs:35-46`; `otterswiss.iss:24-26`; `src/core/app_store.rs:39-43`.
- **Propuesta**: `load_persisted() -> Option<Lang>` y, sin archivo, `Lang::from_system()` (la función de `app_store`); decidir y
  documentar si la tienda sigue el idioma de la app o el del sistema.

<a id="in-02"></a>
### IN-02 · No hay catálogo de textos: 253 `if en {…} else {…}` y `en: bool` por todas partes
- **Prioridad**: P2 · **Esfuerzo**: L · **Evidencia**: observado · **Estado**: Pendiente
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

<a id="in-03"></a>
### IN-03 · El estado guarda mensajes ya traducidos
- **Prioridad**: P2 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: los avisos se guardan como `String` con el idioma capturado al crearlos; si el usuario cambia de idioma, los que ya
  están en pantalla quedan en el anterior. Es otra fuente de mezcla de idiomas.
- **Dónde**: `src/ui/views/uninstaller.rs:27`; `debloater.rs:84`; `dns.rs:49`; `src/ui/mod.rs:132,181,197,278`.
- **Propuesta**: guardar el evento (`enum StatusMsg { DnsApplied { provider, count }, … }`) y traducirlo al renderizar (con `CL-10`).

<a id="in-04"></a>
### IN-04 · Plurales con «(s)»
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Dónde**: `src/ui/mod.rs:198,200,286,287`; `src/ui/views/dns.rs:153,155,191,193`; `uninstaller.rs:121,127` (y en `ui/mod.rs:281,290`
  sí se concuerda).
- **Propuesta**: `plural(n, one, other)` o métodos de `Strings`; para ES y EN alcanza `n == 1`.

<a id="in-05"></a>
### IN-05 · Títulos de ventana y diálogo de error solo en español
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: «OtterSwiss — Archivador» y «OtterSwiss — Suite Nativa Win32» se ven en la barra de tareas y Alt+Tab; el diálogo de
  `panic` solo está en español, mientras `ensure_webview2`, en el mismo archivo, es bilingüe.
- **Dónde**: `src/main.rs:417,423-424,464,480`.
- **Propuesta**: leer el idioma antes de crear la ventana y el hook, y sacar los textos de `Strings`.

<a id="in-06"></a>
### IN-06 · Calidad de traducción y falta de guía de estilo
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: «Refresh» y «Update» se traducen ambos «Actualizar» en la misma vista; mayúsculas de título al estilo inglés
  («Escanear Rastros», «Bloatware Común y Telemetría (Recomendado)»); 41 «...» y ningún «…»; «ETA» sin traducir; el separador decimal
  es siempre «.» (válido en América, no en España).
- **Dónde**: `src/ui/views/uninstaller.rs:486,490-502`; `debloater.rs:452`; `archiver.rs:164`; `src/main.rs:255`.
- **Propuesta**: «Recargar lista» para Refresh; una guía breve en `AGENTS.md` (mayúscula solo inicial, «…», glosario); con
  `Strings`, un test que compare ES y EN (vacías, idénticas o con distinto número de `{}`).

<a id="in-07"></a>
### IN-07 · El documento no declara su idioma
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: no hay `<html lang>`; Narrador o NVDA leen el inglés con voz española o al revés.
- **Propuesta**: un `use_effect` que ponga `document.documentElement.lang` al cambiar el idioma.

# Herramientas, linters y repositorio (`HE-`)

<a id="he-01"></a>
### HE-01 · No hay CI
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: no existe `.github/`; la única puerta es el hook de agente (`check-rust.ps1`, solo clippy, solo dentro de una
  sesión). Nada garantiza que `main` compile, pase clippy y los tests, ni que `Cargo.lock` esté al día. Trampa: si el CI define
  `RUSTFLAGS` (lo habitual es `-D warnings`), Cargo descarta los `rustflags` de `.cargo/config.toml` y el `.exe` sale sin
  `+crt-static`: vuelve `ARQ-04` sin que nadie lo note. Comprobado al medir con `RUSTFLAGS` definido.
- **Propuesta**: `.github/workflows/ci.yml` en `windows-latest` con `fmt --check`, `clippy --all-targets --locked -- -D warnings` y
  `test --locked`, sin definir `RUSTFLAGS`; un job `msrv` con 1.93 (`HE-06`); `cargo-deny` (`HE-05`). Opcional: un job manual que
  corra `vm_sistema` (los runners son VMs desechables y admin, pero Windows Server: no cubren Appx).

<a id="he-02"></a>
### HE-02 · Sin `.gitattributes`: los finales de línea dependen de cada máquina
- **Prioridad**: P1 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: índice todo LF, árbol con 21 archivos CRLF y 47 LF, y `process.rs` mezclado; `core.autocrlf=true` viene de la
  configuración de esta máquina. `check-wsl.sh` saldría con CRLF en un clon con autocrlf y bash fallaría.
- **Propuesta**: `* text=auto eol=lf`, `*.ps1` e `*.iss` con `crlf`, `*.sh` con `lf`, imágenes `binary`; después
  `git add --renormalize .` en un commit propio.

<a id="he-03"></a>
### HE-03 · La versión se escribe a mano en seis sitios y el updater depende de que coincidan
- **Prioridad**: P1 · **Esfuerzo**: M · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: `Cargo.toml`, cuatro literales en `otterswiss.iss`, `README.md`, `CHANGELOG.md` y el tag. El updater compara el tag
  con `CARGO_PKG_VERSION`: si se publica `v3.6.0` con `Cargo.toml` en 3.5.1, la app ofrece la actualización en bucle; si se olvida el
  `.iss`, Agregar o quitar programas muestra otra versión.
- **Dónde**: `Cargo.toml:5`; `otterswiss.iss:6,7,9,14`; `README.md:3`; `src/core/updater.rs:289`.
- **Propuesta**: el `.iss` lee la versión del binario (`GetStringFileInfo(…, "ProductVersion")`); un `release.yml` por tag `v*` que
  falle si el tag no coincide con `Cargo.toml`, compile, corra `ISCC` y publique; quitar la versión del título del README.

<a id="he-04"></a>
### HE-04 · No hay `[lints]` ni `clippy.toml`: las reglas del proyecto no se hacen cumplir
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (medido) · **Estado**: Pendiente
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

<a id="he-05"></a>
### HE-05 · Sin `cargo-deny` ni `cargo-audit`
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: SEC-27 (RUSTSEC en `sevenz-rust`) se encontró a mano. Unos 395 crates en el árbol; los duplicados están en `AR-18`.
- **Propuesta**: `deny.toml` con `targets = ["x86_64-pc-windows-msvc"]`, advisories en `deny`, `multiple-versions = "warn"` y la
  lista de licencias; en el CI (`HE-01`).

<a id="he-06"></a>
### HE-06 · El MSRV 1.93 está declarado pero no se prueba
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (solo está instalada la stable 1.100) · **Estado**: Pendiente
- **Propuesta**: el job `msrv` de `HE-01`, o `rust-toolchain.toml` con `channel = "1.93"` si se quiere reproducibilidad total.

<a id="he-07"></a>
### HE-07 · Documentación desactualizada respecto al código
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: confirmado (README) · **Estado**: Pendiente
- `README.md`: termina truncado dentro de un bloque `powershell` sin cerrar (no dice cómo compilar ni generar el instalador); pide
  Rust 1.82 (`:7,96`) y el mínimo es 1.93; promete purgar Prefetch (`:48`) y `cleaner.rs:260` no lo toca.
- `CHANGELOG.md:31-32`: dice que `.cargo/config.toml` deja de versionarse (volvió con `ARQ-04`) y habla de 36 tests (hay 37 y uno
  ignorado).
- `AGENTS.md:15,17`: manda `dx serve` (no está instalado ni hay `Dioxus.toml`) y `clippy` sin `--all-targets`; no avisa que
  `cargo run` sin elevar falla con 740 por el manifiesto; no menciona `docs/BUGS.md`, `tests/vm_sistema.rs`, la convención de
  idioma ni el estilo de commits.
- `.agents/rules/rust.md:15`: exige `anyhow`, que no es dependencia.

<a id="he-08"></a>
### HE-08 · `.claude/settings.local.json` solo se ignora por la configuración de esta máquina
- **Prioridad**: P2 · **Esfuerzo**: S · **Evidencia**: observado (`git check-ignore -v`) · **Estado**: Pendiente
- **Qué pasa**: lo ignora `~/.config/git/ignore`, no el `.gitignore`; en otro clon se podría commitear con permisos o rutas locales.
- **Propuesta**: agregarlo al `.gitignore` del repo.

<a id="he-09"></a>
### HE-09 · Migrar a la edición 2024
- **Prioridad**: P3 · **Esfuerzo**: M · **Evidencia**: observado (47 avisos de `rust-2024-compatibility`) · **Estado**: Pendiente
- **Qué pasa**: 32 `tail_expr_drop_order` (informativos), 10 `unsafe_op_in_unsafe_fn`, 3 `if_let_rescope`, 1
  `missing_unsafe_on_extern` (`sys_tools.rs:11`) y 1 patrón. El MSRV 1.93 ya lo permite.
- **Propuesta**: `cargo fix --edition` y revisar a mano los 3 `if_let_rescope`, que pueden alargar la vida de guardas de `Mutex`.

<a id="he-10"></a>
### HE-10 · Sin hook de pre-commit
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Propuesta**: `.githooks/pre-commit` con `fmt --check` y `clippy -D warnings`, activado con `git config core.hooksPath .githooks`
  y documentado; `fmt --check` también en `check-rust.ps1`.

<a id="he-11"></a>
### HE-11 · rustfmt no formatea el contenido de `rsx!`
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado (unas 171 líneas de más de 120 columnas en la UI) · **Estado**: Pendiente
- **Propuesta**: `dx fmt --check` (dioxus-cli 0.7) como paso opcional.

<a id="he-12"></a>
### HE-12 · Metadatos de `Cargo.toml` y features de `windows` que podrían sobrar
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa**: faltan `license = "MIT"` (existe `LICENSE`), `repository` y `publish = false`. No se ve uso directo de las features
  `NetworkManagement_Dns`, `System_Memory`, `System_Ole` ni `System_Registry`; `ProcessIdToSessionId` está declarada a mano en vez de
  activar `Win32_System_RemoteDesktop`.
- **Dónde**: `Cargo.toml`; `src/core/sys_tools.rs:10-16`.
- **Propuesta**: completar los metadatos; quitar features una por una con `cargo check` (las de criptografía las exige
  `WINTRUST_DATA`, no tocarlas sin compilar).

<a id="he-13"></a>
### HE-13 · `.editorconfig` y otras herramientas de texto
- **Prioridad**: P3 · **Esfuerzo**: S · **Evidencia**: observado · **Estado**: Pendiente
- **Qué pasa / propuesta**: `.editorconfig` sí conviene para `.iss`, `.ps1`, `.md` y `.css`, que rustfmt no cubre (`README.md` no
  termina en salto de línea). `typos` aporta poco con textos en español; `taplo` tampoco con dos TOML.
