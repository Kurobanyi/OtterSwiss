# Expedientes de hallazgos

Causa, evidencia y propuesta por ID. El índice, los abiertos y los patrones viven en `docs/BUGS.md`. Cada ID tiene una sola
sección `###` y un ancla `#id` en minúscula. Los hallazgos van agrupados por prefijo.

## Origen del registro

Auditoría estática completa del 24/11/2026 sobre `main` (`11cb659`, v3.5.1). Se hizo leyendo el código: en la máquina de auditoría
no hay toolchain de Rust, así que nada se compiló, ejecutó ni probó en vivo, y `cargo audit` no se corrió (las dependencias se
cruzaron a mano contra RustSec). Pasadas:

- **Revisión manual** del agente principal (SEC-01, SEC-02, ERR-01, ERR-02, ARQ-01, ARQ-02).
- **security-auditor** en dos áreas: archivos y procesos; configuración del sistema, red y dependencias.
- **code-auditor** en dos áreas: motor de tweaks y rollback; desinstalador, tienda, archivador, limpiador y hardware.
- **design-challenger** sobre todo el producto y la UI.

Los hallazgos duplicados entre agentes se fusionaron en un solo ID. Los marcados **confirmado** fueron re-leídos por el agente
principal contra el código antes de registrarlos. Agentes para repetir la auditoría: `.claude/agents/`.

Leyenda:

- **Afecta**: *Usuario* (quien usa la app), *Sistema* (la máquina queda mal configurada, insegura o con datos perdidos),
  *Atacante local* (un usuario sin privilegios o malware sin elevar puede aprovecharlo), *Desarrollador*.
- **Severidad**: Crítica (escalada de privilegios, ejecución de código o pérdida de datos) · Alta (flujo principal mal o deja el
  sistema inseguro) · Media (se usa pero falla o confunde) · Baja (detalle).
- **Evidencia**: *observado* (lectura citada) · *derivado* (conclusión con razonamiento a la vista) · *inferido* (sospecha) ·
  *confirmado* (re-leído por el agente principal).
- **Estado**: Pendiente · Parcial · Pendiente de decisión · Aceptado · Diferido · Resuelto.

Modelo de amenaza que se usa en `SEC-`: la app pide `requireAdministrator` (`build.rs:12`), así que todo corre como admin. Un
usuario estándar puede crear carpetas y archivos en `C:\ProgramData` (ACE `BUILTIN\Usuarios:(CI)(WD,AD)` y `CREATOR OWNER:(F)`,
observado con `icacls`) y en `C:\Windows\Temp`; malware sin elevar en la sesión del propio admin controla HKCU, `%APPDATA%`,
`%TEMP%` y `HKCU\Environment`. Lo que OtterSwiss lea de ahí y aplique como admin es superficie de escalada.

# Expedientes · Seguridad (`SEC-`)

<a id="sec-01"></a>
### SEC-01 · El borrado «verificado» borra el archivo aunque la validación de raíz falle
- **Afecta**: Sistema, Atacante local · **Severidad**: Crítica · **Origen**: Revisión manual · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: `delete_verified_file_under` y `delete_verified_empty_dir_under` abren con `FILE_FLAG_DELETE_ON_CLOSE` y después
  comprueban que la ruta final esté bajo `allowed_root`. Si falla, devuelven `Err`, pero al soltar el handle Windows borra igual.
- **Dónde**: `src/core/safe_fs.rs:202-240`, `:247-285`; lo usan `cleaner.rs:162,184`, `uninstaller.rs:1097`, `delete_tree_verified`.
- **Escenario**: en `C:\Windows\Temp` una subcarpeta se cambia por una junction a System32 entre el listado y la lectura; el
  limpiador elevado borra archivos de System32. Borrado arbitrario como admin = primitiva conocida de escalada a SYSTEM.
- **Propuesta**: abrir sin el flag, validar sobre el handle y marcar el borrado con `SetFileInformationByHandle(FileDispositionInfoEx)`.
  Test: ruta que escapa → `Err` y el archivo sigue existiendo. Incluye las instancias de `SEC-25`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `safe_fs` abre sin `DELETE_ON_CLOSE`, valida tipo y raíz sobre el handle y solo después marca el borrado (`FileDispositionInfo`); `path_is_strictly_under` compara por componentes. Test `borrado_fuera_de_raiz_no_borra_nada`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-02"></a>
### SEC-02 · El auto-updater ejecuta como administrador un .exe sin verificar firma ni hash
- **Afecta**: Sistema, Atacante local · **Severidad**: Crítica · **Origen**: Revisión manual · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: `download_and_install_update` descarga con `File::create` (sigue junctions), sin lista blanca de host final, sin tope
  de tamaño, sin SHA-256 ni Authenticode, usa el `asset_name` remoto como nombre y `launch_installer_and_exit` lo lanza con el token
  elevado. La tienda y los runtimes sí usan `trust::secure_download_and_verify`.
- **Dónde**: `src/core/updater.rs:227-388`; UI en `src/ui/mod.rs:312-345`.
- **Escenario**: un usuario estándar pre-crea `ProgramData\OtterSwiss\Updates` como junction (ver `SEC-04`) y reemplaza el instalador
  entre descarga y ejecución. Un compromiso de la cuenta de GitHub también da ejecución como admin en cada cliente.
- **Propuesta**: usar `secure_download_and_verify` con hosts de GitHub y Authenticode (firmar la release) o un hash firmado con clave
  incrustada; sanear `asset_name`; ejecutar desde el handle verificado.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `updater.rs` exige el SHA-256 del `digest` del asset de GitHub, hosts de GitHub, 200 MB máx., nombre saneado y ejecuta desde el handle verificado. Firmante: `UPDATE_SIGNER_CN` queda en `None` hasta que se firmen las releases. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-03"></a>
### SEC-03 · «Revertir» del Debloater escribe en HKLM cualquier clave y valor que diga un JSON envenenable
- **Afecta**: Sistema, Atacante local · **Severidad**: Crítica · **Origen**: security-auditor · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: `revert_registry_debloat` toma colmena, subclave, nombre y `previous_dword` de `debloat_backup.json` y los escribe
  (o borra el valor) sin compararlos con `DEBLOAT_REG_TWEAKS`. `backup_reg_entries` además conserva entradas previas al reescribir.
- **Dónde**: `src/core/debloater.rs:890-933`, `:811-865`; botón en `src/ui/views/debloater.rs:154-158,252-256`.
- **Escenario**: un usuario estándar crea `debloat_backup.json` en `ProgramData\OtterSwiss` (sin endurecer por `SEC-04`; queda como
  dueño aunque luego se endurezca la carpeta) con `HKLM\SOFTWARE\Policies\Microsoft\Windows\Installer\AlwaysInstallElevated=1`. El
  admin pulsa «Revertir» y el usuario instala un MSI como SYSTEM. Variantes: `EnableLUA=0`, borrar cualquier valor de HKLM.
- **Propuesta**: validar cada entrada contra el catálogo (clave, valor, tipo) antes de escribir; rechazar archivos de datos cuyo dueño
  no sea Administradores/SYSTEM.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El rollback del Debloater solo escribe entradas del catálogo, con tipo y bytes exactos; tests `snapshot_del_debloater_solo_del_catalogo`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-04"></a>
### SEC-04 · `ProgramData\OtterSwiss` se crea sin la ACL de solo-admin y no se sanean hijos preexistentes
- **Afecta**: Atacante local · **Severidad**: Alta · **Origen**: security-auditor (las dos áreas) · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: el dashboard, DNS, `--rollback` y `uninstall_self` crean o leen la carpeta con `create_dir_all` sin
  `ensure_admin_only_data_dir`. Y `ensure_admin_only_data_dir` solo cambia dueño y DACL de la raíz: los archivos o carpetas que un
  usuario creó antes conservan su dueño (que puede reescribir su DACL), y hay ventana entre `create_dir_all` y `apply_admin_only_acl`.
- **Dónde**: `src/ui/views/dashboard.rs:60-63,129-131`; `src/ui/views/dns.rs:84-87,130-132`; `src/main.rs:295-300`;
  `src/core/uninstaller.rs:1249-1256`; `src/core/trust.rs:93-207`.
- **Escenario**: es el vector de `SEC-02`, `SEC-03`, `SEC-05`, `SEC-09` y `SEC-10`.
- **Propuesta**: un único punto de acceso a la carpeta que la cree con DACL en `CreateDirectory`, tome posesión y resetee la DACL de
  todo hijo (o aborte si hay hijos con otro dueño). Usarlo en todos los sitios.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `ensure_admin_only_data_dir` crea con SDDL solo-admin, reinicia la ACL y falla ante reparse points o hijos de otro dueño. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-05"></a>
### SEC-05 · El instalador verificado se cierra y se vuelve a abrir por ruta para ejecutarlo (TOCTOU)
- **Afecta**: Atacante local · **Severidad**: Alta · **Origen**: security-auditor · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: tras `secure_download_and_verify` se hace `drop(downloaded_file)` y `CreateProcessW` abre el instalador por ruta. La
  carpeta `staging` puede haber sido pre-creada por un usuario (ver `SEC-04`).
- **Dónde**: `src/core/app_store.rs:724-731` → `:843`/`:822`; `src/core/dependencies.rs:299` → `:313`.
- **Escenario**: el usuario reemplaza el `.exe`/`.msi` entre la verificación y la ejecución → ejecución como admin.
- **Propuesta**: mantener el handle verificado abierto sin compartir escritura/borrado hasta que el proceso arranque; staging con nombre
  aleatorio creado con DACL; para MSI, verificar el archivo que abre msiexec.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `trust::reopen_verified_for_exec` reabre compartiendo solo lectura y repite SHA-256/Authenticode antes de ejecutar (tienda, runtimes, updater). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-06"></a>
### SEC-06 · La purga de restos borra árboles con raíces léxicas tomadas de variables de entorno y rutas de HKCU
- **Afecta**: Sistema, Atacante local · **Severidad**: Alta · **Origen**: security-auditor · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: las raíces permitidas salen de `std::env::var("ProgramFiles")` y similares, se comparan con `target.starts_with(r)`
  sin canonizar, y `delete_tree_verified` recibe como raíz el **padre** del objetivo, así que su chequeo canónico siempre pasa.
  `InstallLocation` puede venir de una entrada HKCU que controla malware sin elevar.
- **Dónde**: `src/core/uninstaller.rs:1195-1238`, `:738-746`; `src/core/safe_fs.rs:293-302`.
- **Escenario**: entrada HKCU Uninstall con `InstallLocation=%LOCALAPPDATA%\Evil\DriverStore`, donde `Evil` es junction a System32
  (o `C:\Program Files\..\Windows\...`). Al desinstalarla, la purga preseleccionada borra System32\DriverStore como admin.
- **Propuesta**: raíces fijas de `SHGetKnownFolderPath`, canonizar objetivo y raíz, rechazar `..` y reparse points en cada componente,
  pasar la raíz real (no el padre), ignorar `InstallLocation` de HKCU fuera del perfil del usuario.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Purga con raíces de `SHGetKnownFolderPath`, ruta canónica, rechazo de junctions y raíz real. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-07"></a>
### SEC-07 · Rutas privilegiadas (y la carga de `nvml.dll`) salen de variables de entorno controlables
- **Afecta**: Atacante local · **Severidad**: Alta · **Origen**: security-auditor (las dos áreas) · **Evidencia**: inferido · **Estado**: Resuelto
- **Qué pasa**: `nvml.dll` se carga desde `%ProgramFiles%\NVIDIA Corporation\NVSMI` sin verificar firma ni usar flags de búsqueda
  seguros; los `UninstallString` de HKLM se expanden con el entorno del proceso; el escaneo de restos usa `%ProgramFiles%`.
- **Dónde**: `src/core/sensors.rs:88-99`; `src/core/win_common.rs:112-120`; `src/core/uninstaller.rs:555-575,750-754,1197-1202`;
  `src/core/dependencies.rs:167-174,213-236`.
- **Escenario**: malware sin elevar define `ProgramFiles` en `HKCU\Environment` y planta una DLL; en un equipo sin NVIDIA OtterSwiss
  elevado la carga (bypass de UAC). Sin probar en vivo que el proceso elevado herede esa variable.
- **Propuesta**: `SHGetKnownFolderPath(FOLDERID_ProgramFiles…)`, expandir con el entorno del sistema, `LoadLibraryExW` con
  `LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR` y Authenticode de NVIDIA.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `win_common::known_folder`/`program_files_dir`; `nvml.dll` con firma de NVIDIA y `LoadLibraryExW` restringido; `UninstallString` expandido con el entorno del sistema o del usuario. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-08"></a>
### SEC-08 · La extracción elevada escribe en carpetas del usuario y sigue junctions intermedias
- **Afecta**: Atacante local · **Severidad**: Alta · **Origen**: security-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: «Extraer aquí» corre como admin. El staging `.extract.*.tmp` hereda la ACL del padre (que controla el usuario) y
  `create_verified_new_file` solo comprueba reparse en el componente final.
- **Dónde**: `src/core/archiver.rs:146-158,238-241,373-376,445-449,567-571`; `src/main.rs:261-271`.
- **Escenario**: una subcarpeta del staging se cambia por una junction a `C:\Windows\System32` a mitad de extracción → escritura de
  archivos nuevos (DLL fantasma) como admin.
- **Propuesta**: extraer des-elevado (token del shell), o abrir cada componente relativo a un handle de directorio verificado y fijar
  DACL de solo-admin en el staging.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Staging de extracción solo-admin y ACL heredada al final; no sigue junctions. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-09"></a>
### SEC-09 · «Restaurar DNS» aplica servidores leídos de un JSON sin validarlos
- **Afecta**: Sistema, Atacante local · **Severidad**: Alta · **Origen**: security-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: solo se valida el GUID; los servidores se aplican tal cual. Si el GUID ya está en el archivo, `record_adapter_before_apply`
  no guarda el estado real.
- **Dónde**: `src/core/dns.rs:456-472`, `:496-502`.
- **Escenario**: un usuario planta `dns_backup.json` con el GUID de la NIC y su DNS; el respaldo legítimo no se guarda y «Restaurar»
  fija el DNS del atacante en todas las NIC (secuestro persistente de resolución).
- **Propuesta**: validar cada IP con `validate_ipv4_dns`/`validate_ipv6_dns`, exigir que la interfaz exista y validar el dueño del archivo.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Restaurar DNS valida IPs y GUID antes de aplicar; tests `rechaza_direcciones_invalidas`, `presets_son_validos`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-10"></a>
### SEC-10 · El rollback del motor no valida bytes, tipo ni planes de energía del respaldo
- **Afecta**: Sistema, Atacante local · **Severidad**: Media · **Origen**: security-auditor (las dos áreas) · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: la lista blanca cubre clave y nombre, pero `raw_bytes`, `value_type` y `key_created` se aplican sin validar, y
  `power_history` solo pasa `parse_guid_strict`.
- **Dónde**: `src/core/registry.rs:147-158,186-188`; `src/core/backup.rs:28-36,110-143`; `src/core/power.rs:251-264`.
- **Escenario**: con `backup.json` envenenado (`SEC-04`): `AllowTelemetry=3`, borrar `HKLM\...\DataCollection` entera, borrar planes OEM.
- **Propuesta**: validar `value_type` contra `spec.value.kind()` y el tamaño de `raw_bytes`; aceptar solo GUID de energía creados por la app.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `RegistrySnapshot::is_valid` limita tipos y tamaño; test `snapshot_valida_catalogo_tipo_y_tamano`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-11"></a>
### SEC-11 · Una entrada HKCU decide qué se purga en HKLM y Program Files
- **Afecta**: Atacante local · **Severidad**: Media · **Origen**: security-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: para apps de HKCU, la búsqueda de restos por subcadena de `DisplayName`/`Publisher` recorre también HKLM, Program Files y ProgramData.
- **Dónde**: `src/core/uninstaller.rs:248-301,766-797,912-1014`.
- **Escenario**: malware crea una entrada HKCU con `DisplayName` = nombre de un antivirus; al «desinstalarla», se proponen
  (preseleccionados) `C:\Program Files\<AV>`, `HKLM\SOFTWARE\<AV>` y sus valores Run.
- **Propuesta**: para apps de HKCU, limitar restos a HKCU y al perfil; coincidencia exacta; nada preseleccionado fuera de la carpeta de instalación.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Apps de HKCU: solo se ofrecen restos en el perfil del usuario y HKCU. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-12"></a>
### SEC-12 · Authenticode sin revocación, solo por CN y con firmantes y mirrors demasiado amplios
- **Afecta**: Sistema · **Severidad**: Media · **Origen**: security-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: `WTD_REVOKE_NONE`; ningún paquete fija thumbprint; qBittorrent acepta el CN `SignPath Foundation` (firma muchos
  proyectos) desde `*.dl.sourceforge.net` (mirrors de terceros). Se acepta cualquier binario viejo firmado por la entidad (downgrade).
- **Dónde**: `src/core/trust.rs:524-534,608-611`; `src/core/app_store.rs:106-441` (qBittorrent `:360-362`); `src/core/dependencies.rs:40-117`.
- **Propuesta**: `WTD_REVOKE_WHOLECHAIN`, fijar SHA-256 en todo paquete versionado y thumbprint/emisor en los evergreen; quitar los mirrors comodín.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Revocación `WTD_REVOKE_WHOLECHAIN`; sin «SignPath Foundation». qBittorrent no trae firma Authenticode (PGP): se fijó el SHA-256 del instalador 5.1.4 publicado en SourceForge (TOFU, medido el 2026-11-24) y se quitó el firmante, que nunca coincidía. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-13"></a>
### SEC-13 · El menú contextual de HKLM apunta al .exe portable dondequiera que esté
- **Afecta**: Atacante local · **Severidad**: Media · **Origen**: security-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: el comando registrado es `current_exe()`; si vive en Descargas, cualquier proceso del usuario lo reemplaza o planta DLLs
  al lado. Cada «Extraer aquí» (visible para todos los usuarios) pide UAC para un binario sin firma.
- **Dónde**: `src/core/context_menu.rs:91-118`; `src/main.rs:304-306`.
- **Propuesta**: registrar solo si el exe está bajo Program Files con ACL de admin (o copiarlo ahí) y firmar el binario.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El menú contextual solo se registra si el exe está en Program Files. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-14"></a>
### SEC-14 · Discord y Spotify se preparan en `C:\Users\Public\OtterSwiss_Staging`
- **Afecta**: Atacante local · **Severidad**: Media · **Origen**: security-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: carpeta pública, pre-creable o convertible en junction; la copia no se re-verifica (`File::open` por ruta) y el directorio
  es el primero en la búsqueda de DLL del instalador.
- **Dónde**: `src/core/app_store.rs:472-490,552-559,747-760,801-839`.
- **Escenario**: otro usuario del equipo cambia la copia o planta una DLL → ejecución como el usuario de escritorio (movimiento lateral).
- **Propuesta**: preparar en `%LOCALAPPDATA%` del usuario del token del shell y re-verificar la firma desde el handle de la copia.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Los instaladores sin elevación se preparan en `%LOCALAPPDATA%\Temp\OtterSwiss_<nonce>` del usuario del Explorador. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-15"></a>
### SEC-15 · La extracción no propaga el Mark-of-the-Web a los archivos extraídos
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: security-auditor, code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: solo se escribe `Zone.Identifier` sobre la carpeta destino, con `HostUrl=archive://extracted`.
- **Dónde**: `src/core/archiver.rs:165`.
- **Escenario**: un zip bajado de internet entrega ejecutables y documentos sin MOTW: salta SmartScreen y la Vista protegida de Office.
- **Propuesta**: leer el `Zone.Identifier` del archivo origen y copiarlo a cada archivo extraído.
- **Cierre** (2026-11-24, rama `fix/auditoria`): La extracción propaga el `Zone.Identifier` a cada archivo extraído. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-16"></a>
### SEC-16 · La compresión elevada lee archivos fuera del origen (junctions y symlinks en tar)
- **Afecta**: Atacante local · **Severidad**: Media · **Origen**: security-auditor, code-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: ZIP salta symlinks al listar pero hay carrera entre `read_dir` y `File::open`; TAR.GZ usa `append_dir_all`, que sigue enlaces.
- **Dónde**: `src/core/archiver.rs:714-716,731,812`; 7z (`:775`) sin revisar.
- **Escenario**: una junction en la carpeta a comprimir mete en el archivo (que queda en una carpeta del usuario) datos solo legibles por
  admin; una junction cíclica cuelga la compresión.
- **Propuesta**: `follow_symlinks(false)`, abrir con `OPEN_REPARSE_POINT` y validar la ruta final, o comprimir des-elevado.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `tar.follow_symlinks(false)` y archivos de origen abiertos sin seguir reparse points. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-17"></a>
### SEC-17 · Todo el programa corre como administrador aunque la mitad de las funciones no lo necesitan
- **Afecta**: Sistema, Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Diferido
- **Qué pasa**: `requireAdministrator` en el manifiesto; visor de hardware, sensores, archivador y lista de apps no llaman
  `require_elevated`. Cada «Extraer aquí» abre UAC y extrae como admin; los tweaks HKCU caen en el perfil del admin si eleva otra cuenta.
- **Dónde**: `build.rs:12`; `src/core/context_menu.rs:24-56`; `src/core/registry.rs:266-270`.
- **Propuesta**: manifest `asInvoker` y un proceso auxiliar elevado solo para las acciones de sistema. Reduce la superficie de `SEC-08`,
  `SEC-13`, `SEC-16`, `SEC-19` y `SEC-20`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Decisión del autor (2026-11-24): separar un auxiliar elevado es un rediseño para una versión siguiente.


<a id="sec-18"></a>
### SEC-18 · La tienda instala versiones fijadas con vulnerabilidades conocidas
- **Afecta**: Usuario, Sistema · **Severidad**: Alta · **Origen**: design-challenger · **Evidencia**: observado (7-Zip), inferido (resto) · **Estado**: Resuelto
- **Qué pasa**: 7-Zip 24.09 es vulnerable a CVE-2025-11001/11002 (RCE por symlinks en ZIP, corregido en 25.00). Git 2.47.1 y
  Notepad++ 8.7.5 también están atrasados.
- **Dónde**: `src/core/app_store.rs:331-332,404,429`.
- **Propuesta**: actualizar pines y hashes y definir una revisión periódica; mostrar «versión fijada de AAAA-MM» en la UI.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Pines actualizados con SHA-256 de la API de GitHub: 7-Zip 26.06, Git 2.56.1, Notepad++ 8.9.9.1, OBS 33.0.3, Playnite 10.65. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-19"></a>
### SEC-19 · La persistencia de tema e idioma escribe como admin siguiendo junctions
- **Afecta**: Atacante local · **Severidad**: Baja · **Origen**: security-auditor · **Evidencia**: observado; explotación inferida · **Estado**: Resuelto
- **Qué pasa**: `std::fs::write` y `create_dir_all` sobre `%APPDATA%\OtterSwiss\{theme,lang}.json`.
- **Dónde**: `src/ui/theme.rs:55,74,77`; `src/ui/i18n.rs:32,51,54`.
- **Escenario**: junction a una carpeta del sistema → OtterSwiss elevado crea o trunca un archivo con contenido fijo (DoS).
- **Propuesta**: `atomic_write_new` con comprobación de reparse en el padre, o guardar en la carpeta de datos endurecida.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Tema e idioma se guardan con `safe_fs::write_file_inside`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-20"></a>
### SEC-20 · La exportación de especificaciones escribe en un Escritorio redirigible
- **Afecta**: Atacante local · **Severidad**: Baja · **Origen**: security-auditor · **Evidencia**: inferido · **Estado**: Resuelto
- **Dónde**: `src/ui/views/hardware_tab.rs:74-76`; `src/core/safe_fs.rs:156-198`.
- **Propuesta**: verificar la ruta canónica del padre o pedir la ruta con un diálogo de guardado.
- **Cierre** (2026-11-24, rama `fix/auditoria`): La exportación de especificaciones usa `safe_fs::write_file_inside`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-21"></a>
### SEC-21 · Lanzamiento de procesos frágil: Job asignado tarde y directorio actual heredado
- **Afecta**: Atacante local · **Severidad**: Baja · **Origen**: security-auditor, code-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: sin `CREATE_SUSPENDED`, los hijos tempranos escapan del Job; `lpCurrentDirectory = NULL` hereda el directorio de
  OtterSwiss (la carpeta del archivo, si vino del menú contextual) y lo mete en la búsqueda de DLL del instalador elevado.
- **Dónde**: `src/core/app_store.rs:851,864-869`; `src/core/dependencies.rs:337-339`; `src/core/uninstaller.rs:665-677,688-690`.
- **Propuesta**: `CREATE_SUSPENDED` → asignar → `ResumeThread`; directorio actual = staging o System32. Ver también `SYS-05`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `process::run_in_job`: proceso suspendido, Job asignado antes de reanudar y cwd explícito. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-22"></a>
### SEC-22 · `UninstallString` sin comillas se parte en el primer espacio y deja pasar UNC con `/`
- **Afecta**: Atacante local · **Severidad**: Baja · **Origen**: security-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Dónde**: `src/core/uninstaller.rs:303-324,586`.
- **Propuesta**: `CommandLineToArgvW` y rechazar rutas UNC con cualquier separador.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El ejecutable del desinstalador debe ser un archivo local absoluto; el corte sin comillas va tras «.exe» (test `separa_ejecutable_y_argumentos`). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-23"></a>
### SEC-23 · El mutex global de respaldo se puede ocupar de antemano y colgar respaldo y rollback
- **Afecta**: Atacante local · **Severidad**: Baja · **Origen**: security-auditor · **Evidencia**: inferido · **Estado**: Resuelto
- **Dónde**: `src/core/backup.rs:13,111` (`Global\OtterSwiss_Backup_Lock`, DACL por defecto); `src/core/win_common.rs:241-252` (INFINITE).
- **Propuesta**: timeout y DACL de admin, o lock de archivo dentro de la carpeta endurecida.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `NamedMutex` con DACL solo-admin y espera de 60 s. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-24"></a>
### SEC-24 · El updater lee la respuesta de GitHub sin límite de tamaño
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: security-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/updater.rs:169-185`.
- **Propuesta**: cortar en ~1 MB. Se resuelve junto con `SEC-02`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El JSON de GitHub se lee con un límite de 1 MB. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-25"></a>
### SEC-25 · Más instancias del patrón 1: borrados por ruta sin raíz en carpetas que puede controlar un atacante
- **Afecta**: Atacante local · **Severidad**: Baja · **Origen**: security-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Dónde**: `src/core/safe_fs.rs:144,195`; `src/core/app_store.rs:727-769`; `src/core/dependencies.rs:295-365`; `src/core/backup.rs:224`.
- **Propuesta**: pasar siempre una raíz a `delete_verified_file`; se arregla con `SEC-01`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Revisados todos los `delete_verified_file`: los que quedan borran dentro de la carpeta de datos solo-admin (staging, Updates, respaldos) o con raíz explícita (`delete_verified_file_under`), y la validación es sobre el handle (SEC-01). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-26"></a>
### SEC-26 · La extracción ZIP acepta `:` en los nombres (flujos ADS); 7z y tar los rechazan
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: security-auditor, code-auditor · **Evidencia**: observado; efecto inferido · **Estado**: Resuelto
- **Dónde**: `src/core/archiver.rs:63-91,371` frente a `:110`.
- **Propuesta**: validar con `sanitize_archive_rel_path` también en ZIP. Test: `validate_path_components("a:b")` → `Err`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): ZIP usa `sanitize_archive_rel_path`, que rechaza `:`; tests `saneo_rechaza_traversal_unidades_y_ads`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-27"></a>
### SEC-27 · `sevenz-rust` 0.6.1 está sin mantenimiento y tiene un path traversal sin parche
- **Afecta**: Desarrollador · **Severidad**: Baja · **Origen**: security-auditor (las dos áreas) · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: RUSTSEC-2026-0246 (sin mantenimiento) y RUSTSEC-2026-0245 (traversal en `decompress_impl`). No es alcanzable hoy:
  se usa `for_each_entries` con saneo propio. El resto de dependencias revisadas (zip 2.6.1, tar 0.4.46, tokio 1.54.1) no tiene
  advisories alcanzables; dioxus/wry/webview2-com sin advisories encontrados (no concluyente sin `cargo audit`).
- **Dónde**: `Cargo.toml`, `Cargo.lock`, `src/core/archiver.rs:413-437`.
- **Propuesta**: migrar a `sevenz-rust2` y correr `cargo audit` en CI.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Migrado a `sevenz-rust2` 0.25 (Apache-2.0, sin features por defecto: `compress` y `util`); `rust-version` sube a 1.93, la mínima que exige. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sec-28"></a>
### SEC-28 · Lecturas no alineadas de buffers como estructuras (comportamiento indefinido)
- **Afecta**: Desarrollador · **Severidad**: Baja · **Origen**: security-auditor, code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/win_common.rs:477,497`; `src/core/dns.rs:205,231`; `src/core/drivers.rs:104`; `src/core/specs.rs:321`; `src/core/services.rs:183`.
- **Propuesta**: `read_unaligned` o buffers alineados.

# Expedientes · Cambios al sistema y rollback (`SYS-`)
- **Cierre** (2026-11-24, rama `fix/auditoria`): `win_common::AlignedBuf` en dns, drivers, services, sensors, specs y win_common. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-01"></a>
### SYS-01 · Extraer borra entera y sin confirmar una carpeta destino que ya existe
- **Afecta**: Usuario · **Severidad**: Crítica · **Origen**: code-auditor, security-auditor · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: `finalize_staging_dir` hace `delete_tree_verified(dest_dir, parent, 16)` si el destino existe, y luego renombra el staging.
- **Dónde**: `src/core/archiver.rs:160-164`; `src/main.rs:30-46,265`; `src/ui/views/archiver.rs:31,57`.
- **Escenario**: «Extraer aquí» de `proyecto.zip` junto a una carpeta `proyecto\` editada la borra de forma permanente. En la vista,
  poner `C:\Users\X\Downloads` como destino borra toda Descargas. Si el borrado falla a mitad, el destino queda medio borrado.
- **Propuesta**: nunca sobrescribir: nombre único (`proyecto (2)`) o confirmación explícita. Test con archivo centinela.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `resolve_extract_destination` usa la carpeta hermana libre «nombre (2)» y `finalize_staging_dir` nunca borra el destino; test con centinela `destino_existente_usa_carpeta_hermana_y_conserva_el_centinela`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-02"></a>
### SYS-02 · Se ignora el código de salida del desinstalador y se ofrecen sus restos preseleccionados
- **Afecta**: Usuario · **Severidad**: Alta · **Origen**: code-auditor, design-challenger · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: `run_uninstaller` devuelve `Ok(exit_code)` con cualquier código y la vista trata `Ok(_)` como éxito: escanea restos y
  preselecciona todo, incluidas la carpeta de instalación y la entrada Uninstall. Muestra «desinstalado sin rastros» aunque fallara.
- **Dónde**: `src/core/uninstaller.rs:711-719,737-746`; `src/ui/views/uninstaller.rs:125-154`.
- **Escenario**: el usuario cancela el diálogo (MSI 1602) y un clic en «Eliminar» borra la carpeta del programa y su entrada en Apps.
- **Propuesta**: solo 0/3010/1641 son éxito; si no, error y nada de escaneo. Ofrecer carpeta y clave Uninstall solo si la clave desapareció.
- **Cierre** (2026-11-24, rama `fix/auditoria`): La vista trata como éxito solo 0, 3010 y 1641, no busca restos si falla y avisa si la app sigue registrada; test `codigos_de_salida_de_exito`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-03"></a>
### SYS-03 · La heurística de restos marca datos de otras apps, preseleccionados, sin papelera ni respaldo
- **Afecta**: Usuario · **Severidad**: Alta · **Origen**: code-auditor, design-challenger · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: `matches_any_token` usa `contains` con tokens de 4+ letras; si «no hay otra app del fabricante» se marca su carpeta
  entera; el `Publisher` crudo se compara contra el token saneado; las claves de HKLM se borran con `delete_subkey_all` sin exportar.
- **Dónde**: `src/core/uninstaller.rs:271-301,727-736,778-785,929,949-975,1137-1142`; `src/ui/views/uninstaller.rs:76-77,152-153,324`.
- **Escenario**: Firefox sin Thunderbird → `AppData\Roaming\Mozilla` (perfiles y contraseñas). «Python 3.11» → `Python312`, `%APPDATA%\Python`,
  `HKLM\Software\Python` de todas las versiones. `InstallLocation=D:\Games` → `Epic Games`, `Rockstar Games`. «Adobe» frente a «Adobe Inc.».
- **Propuesta**: preseleccionar solo coincidencias exactas; separar «datos de usuario» sin marcar; exportar `.reg` antes de borrar claves;
  avisar en el modal que es permanente. Tests de `build_app_tokens`/`matches_any_token` con estos casos.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Coincidencia exacta de tokens, `LeftoverItem.recommended` (solo lo seguro viene marcado) y claves exportadas a `RegBackups` antes de borrar; el modal avisa que es permanente. Tests `tokens_coinciden_solo_exactos`, `recomendados_excluyen_datos_de_usuario_y_fabricante`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-04"></a>
### SYS-04 · «Escanear rastros» sobre una app todavía instalada propone borrar su carpeta y su entrada Uninstall
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor, design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/ui/views/uninstaller.rs:51-94`; `src/core/uninstaller.rs:738-746,873-893`.
- **Propuesta**: si la app sigue presente, excluir `InstallLocation` y la clave Uninstall.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `is_still_registered`: con la app instalada no se ofrecen su carpeta ni su entrada Uninstall. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-05"></a>
### SYS-05 · El Job «kill on close» mata al desinstalador o instalador real cuando el lanzador sale
- **Afecta**: Usuario · **Severidad**: Alta · **Origen**: code-auditor · **Evidencia**: mecanismo observado; conducta de NSIS inferida · **Estado**: Resuelto
- **Qué pasa**: solo se espera `pi.hProcess`; al volver, el `JobObject` local se destruye y mata a todos los hijos.
- **Dónde**: `src/core/uninstaller.rs:605,688-693,719`; `src/core/win_common.rs:178`; `src/core/app_store.rs:792`; `src/core/dependencies.rs:301`.
- **Escenario**: un desinstalador NSIS se copia a %TEMP%, se relanza y el padre sale: el hijo muere, se devuelve `Ok(0)` y la app
  sigue instalada (y `SYS-02` ofrece borrar su carpeta). Igual con bootstrappers (Squirrel/Discord, Battle.net, WebView2).
- **Propuesta**: esperar a que el job quede vacío (`JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO`), no solo al proceso inicial.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `run_in_job` espera a que el Job quede vacío, no solo al proceso lanzador. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-06"></a>
### SYS-06 · Un respaldo corrupto o ilegible se reinicia en silencio y se pierde el estado original (motor, DNS, Debloater)
- **Afecta**: Sistema · **Severidad**: Alta · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: `load_or_init_unlocked` traga el error de lectura o parseo con `if let Ok` y empieza un documento vacío; la siguiente
  aplicación guarda como «original» los valores ya optimizados. El mismo patrón en DNS y Debloater.
- **Dónde**: `src/core/backup.rs:229-238` (usado en `:160,191,214`); `src/core/dns.rs:481-494`; `src/core/debloater.rs:813-826`.
- **Escenario**: respaldo bloqueado por un antivirus, de más de 2 MB o de otra versión → el estado de fábrica desaparece para siempre.
- **Propuesta**: si el archivo existe y no se puede leer, abortar con error y no tocarlo. Test con archivo corrupto.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Un respaldo existente que no se puede leer aborta (motor, DNS, Debloater); nunca se reinicia vacío. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-07"></a>
### SYS-07 · «Restaurar DNS» nunca devuelve el IPv6 a automático
- **Afecta**: Sistema · **Severidad**: Alta · **Origen**: code-auditor, design-challenger, security-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: al aplicar siempre se fija IPv6; el respaldo de un adaptador DHCP guarda `""`, y al restaurar `apply_interface_dns_typed`
  se salta IPv6 cuando `ipv6_csv` está vacío. La UI dice «Restored N NIC(s)».
- **Dónde**: `src/ui/views/dns.rs:78`; `src/core/dns.rs:279-303,519-539`.
- **Propuesta**: al restaurar, escribir IPv6 vacío explícitamente. Test: restaurar `{v4:"", v6:""}` emite dos llamadas.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Restaurar DNS devuelve IPv4 e IPv6 (vacío = automático). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-08"></a>
### SYS-08 · Un adaptador que ya no existe bloquea para siempre la restauración de DNS
- **Afecta**: Sistema · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado (fallo de la API inferido) · **Estado**: Resuelto
- **Dónde**: `src/core/dns.rs:464-475` (`?` dentro del bucle).
- **Propuesta**: restaurar el resto, reportar el adaptador ausente y quitarlo del respaldo.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Los adaptadores que ya no existen se omiten y el respaldo conserva solo los pendientes. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-09"></a>
### SYS-09 · Si falla la lectura del DNS previo, se guarda como «DHCP»
- **Afecta**: Sistema · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/dns.rs:286-302,433-434` (`.ok()…unwrap_or_default()`); solo se lee `NameServer` del registro.
- **Propuesta**: `capture` devuelve `Err` si la lectura falla; leer también la configuración IPv6.
- **Cierre** (2026-11-24, rama `fix/auditoria`): La captura del DNS previo es estricta: si falla, no se aplica. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-10"></a>
### SYS-10 · «Revertir» del Debloater cuenta como revertido lo que falló y borra el respaldo siempre
- **Afecta**: Sistema · **Severidad**: Alta · **Origen**: code-auditor, design-challenger · **Evidencia**: confirmado · **Estado**: Resuelto
- **Qué pasa**: `let _ = set_value/delete_value`, `restored += 1` con solo abrir la clave, y `delete_verified_file(&backup_file)` incondicional.
  La UI promete «Ajustes Anti-Publicidad (Reversibles)».
- **Dónde**: `src/core/debloater.rs:919-931`; `src/ui/views/debloater.rs:164-169,197`.
- **Propuesta**: propagar errores y conservar el respaldo si algo falla. Se arregla junto con `SEC-03`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Revertir del Debloater propaga errores y conserva el respaldo si algo falla; la vista muestra el error. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-11"></a>
### SYS-11 · El respaldo del Debloater pierde valores originales que no son DWORD o que no se pudieron leer
- **Afecta**: Sistema · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: `get_value::<u32>().ok()` convierte REG_SZ, QWORD o un error en «no existía»; al revertir, el valor se borra.
- **Dónde**: `src/core/debloater.rs:847-857,925`.
- **Propuesta**: guardar bytes y tipo como hace `registry.rs`, y distinguir `NotFound` de otros errores.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El respaldo del Debloater guarda tipo y bytes exactos (compatible con el formato viejo). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-12"></a>
### SYS-12 · El rollback borra el respaldo aunque haya ítems omitidos, y un `apply` fallido cuenta como «modificado por el usuario»
- **Afecta**: Sistema · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/engine.rs:566,574-579`; `src/core/registry.rs:396-401`; la UI siempre pasa `force=false` (`src/ui/views/dashboard.rs:135`).
- **Propuesta**: conservar el respaldo con `skipped_user_modified > 0` y ofrecer «forzar» en la UI.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El rollback conserva el respaldo si hubo ítems omitidos; la UI lo dice. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-13"></a>
### SYS-13 · Desinstalar OtterSwiss no revierte nada; `uninstall_self` es código muerto y además está mal
- **Afecta**: Sistema · **Severidad**: Alta · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: el instalador solo llama `--uninstall-context-menu`; quedan registro, servicios, energía, DNS y Debloater modificados.
  `--rollback` descarta el resultado. `uninstall_self` (sin llamadores) borraría los datos aunque el rollback tenga fallos, no revierte
  DNS ni Debloater, y falla si no existe `backup.json`.
- **Dónde**: `otterswiss.iss:52-53`; `src/core/uninstaller.rs:1246-1258`; `src/main.rs:295-301`; `src/core/engine.rs:507-511,583`.
- **Propuesta**: decidir si desinstalar revierte; si sí, un rollback único (motor + DNS + Debloater) que no borre datos con fallos.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `UninstallerManager::revert_all_changes` (motor + DNS + Debloater) en `--rollback`, llamado por `otterswiss.iss` al desinstalar; `uninstall_self` eliminado. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-14"></a>
### SYS-14 · El rollback borra una clave «creada» junto con los valores que otros escribieron después
- **Afecta**: Sistema · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: derivado · **Estado**: Resuelto
- **Dónde**: `src/core/registry.rs:443-445` (`let _ = root.delete_subkey`).
- **Propuesta**: borrar solo el valor propio y la clave solo si quedó vacía.
- **Cierre** (2026-11-24, rama `fix/auditoria`): La clave creada solo se borra si quedó vacía. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-15"></a>
### SYS-15 · El rollback de energía pisa el plan que el usuario eligió después
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/power.rs:231-249` (ignora `force`; GUID inválido cae a Equilibrado sin aviso).
- **Propuesta**: tratar el plan como los demás ítems: omitir si el usuario lo cambió.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El rollback de energía respeta el plan elegido después por el usuario (salvo `force`). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-16"></a>
### SYS-16 · Una segunda optimización con otro perfil no cambia el plan de energía pero informa «Aplicado»
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/power.rs:155-168`; `src/core/engine.rs:442-452`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Si ya hay un plan de OtterSwiss activo, el paso se omite y lo dice. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-17"></a>
### SYS-17 · El servicio queda deshabilitado aunque el paso diga «Error»
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: StartType se pone en DISABLED antes de detener; si la detención agota los 12 s, el paso se muestra fallido.
- **Dónde**: `src/core/services.rs:107-110`; `src/core/engine.rs:384-395`.
- **Propuesta**: reportar «aplicado parcialmente».
- **Cierre** (2026-11-24, rama `fix/auditoria`): `ServiceOpOutcome::AppliedPendingStop`: deshabilitado pero sin detener se informa como tal. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-18"></a>
### SYS-18 · «Automático (inicio retrasado)» vuelve como «Automático» si falla `QueryServiceConfig2W`
- **Afecta**: Sistema · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/services.rs:190-204`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Error si no se puede leer el inicio retrasado. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-19"></a>
### SYS-19 · Los portátiles de gama media reciben el plan Alto Rendimiento
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: con 4+ hilos y 7+ GB el perfil es MidRange, que usa `GUID_HIGH_PERFORMANCE` sin mirar la batería (solo HighEnd lo hace).
  La UI dice «Selecciona el plan óptimo según PC o portátil.»
- **Dónde**: `src/core/win_common.rs:331-336`; `src/core/engine.rs:404`; `src/ui/views/dashboard.rs:273`.
- **Propuesta**: con batería, siempre Equilibrado.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Portátil de gama media → plan Equilibrado. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-20"></a>
### SYS-20 · El Debloater elimina para todos los usuarios, sin confirmación, con Outlook, To Do y Enlace Móvil preseleccionados
- **Afecta**: Usuario, Sistema · **Severidad**: Alta · **Origen**: design-challenger, code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: `Remove-AppxPackage -AllUsers` + `Remove-AppxProvisionedPackage -Online`, sin respaldo ni ruta de reinstalación;
  «Aplicar (N)» ejecuta sin modal; 25 entradas `default_selected: true`; la `description` de cada app nunca se muestra.
- **Dónde**: `src/core/debloater.rs:213-218,718,735`; `src/ui/views/debloater.rs:15-33,77-152,206-217,378-379`.
- **Propuesta**: modal con la lista y «se reinstala desde Microsoft Store»; quitar de la preselección apps de uso real (Outlook, To Do,
  Enlace Móvil, Teams); mostrar la descripción.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Confirmación con la lista concreta y aviso de «todos los usuarios / se reinstala desde Microsoft Store»; Outlook, To Do, Enlace Móvil y Teams desmarcadas; se muestra la descripción. Test `apps_de_uso_real_no_vienen_marcadas`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-21"></a>
### SYS-21 · Borrar claves HKLM de 32 bits con un handle `KEY_WRITE` falla siempre
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: inferido (según la doc de `RegDeleteTree`) · **Estado**: Resuelto
- **Dónde**: `src/core/uninstaller.rs:1126-1137`.
- **Propuesta**: abrir con `DELETE | KEY_ENUMERATE_SUB_KEYS | KEY_QUERY_VALUE`. Test de integración con una clave WOW6432Node propia.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El padre se abre con `KEY_ALL_ACCESS` para borrar claves de 32 bits. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-22"></a>
### SYS-22 · Cancelar o fallar al comprimir deja un archivo truncado en el destino
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado (7z inferido) · **Estado**: Resuelto
- **Qué pasa**: el .zip/.tar.gz parcial se queda y el reintento falla con «ya existe»; 7z ignora `cancel`.
- **Dónde**: `src/core/archiver.rs:666,676,694,735,775,798`; `src/core/safe_fs.rs:134`.
- **Propuesta**: escribir a temporal y renombrar al terminar; borrar el temporal al cancelar.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Compresión a `.partial` y renombre al final; cancelar o fallar no deja archivo truncado. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-23"></a>
### SYS-23 · El limpiador vacía Prefetch y `SoftwareDistribution\Download` con Windows Update activo
- **Afecta**: Sistema · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: inferido · **Estado**: Resuelto
- **Qué pasa**: vaciar Prefetch ralentiza los próximos arranques; limpiar Download sin detener `wuauserv`/BITS puede romper una actualización en curso.
- **Dónde**: `src/core/cleaner.rs:250-255`.
- **Propuesta**: quitar Prefetch; limpiar Download solo con los servicios detenidos.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Prefetch fuera; Windows Update solo con `wuauserv` y `BITS` detenidos, y la vista muestra el motivo si se omite. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="sys-24"></a>
### SYS-24 · Con elevación «sobre el hombro», los tweaks de HKCU se aplican al perfil del administrador
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor, design-challenger · **Evidencia**: inferido · **Estado**: Diferido
- **Dónde**: `src/core/registry.rs:266-270`; `src/core/debloater.rs:838-841`.
- **Propuesta**: aplicar HKCU con el token del shell o avisar. Se resuelve con `SEC-17`.

# Expedientes · Manejo de errores y procesos (`ERR-`)
- **Cierre** (2026-11-24, rama `fix/auditoria`): Depende de `SEC-17` (diferido por decisión del autor).


<a id="err-01"></a>
### ERR-01 · El Debloater puede bloquearse si PowerShell escribe mucha salida
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: Revisión manual · **Evidencia**: derivado · **Estado**: Resuelto
- **Qué pasa**: `run_ps_cancellable` usa `Stdio::piped()` y solo lee al terminar; con más de ~4 KB PowerShell se bloquea, se agota el
  timeout, se mata el proceso (posible paquete a medias) y se reporta «Tiempo límite excedido».
- **Dónde**: `src/core/debloater.rs:510-560`.
- **Propuesta**: leer los pipes en hilos mientras se espera.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `run_ps` lee stdout y stderr en hilos mientras PowerShell corre. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-02"></a>
### ERR-02 · Las versiones pre-release no se detectan como nuevas
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: Revisión manual · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/updater.rs:80-91` (`3.5.2-rc1` → `(3,5,0)`).
- **Propuesta**: semver, o cortar en `-`/`+` antes de parsear.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Semver con pre-release; tests `compara_versiones_semver`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-03"></a>
### ERR-03 · Si falla un respaldo a mitad de la optimización, se aborta y se pierde el informe de lo ya aplicado
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/engine.rs:270-271,358-359,439-440`; `src/ui/views/dashboard.rs:101`.
- **Propuesta**: devolver el informe parcial con el error. Además, el texto «Transaccional con rollback seguro» no es cierto (ver `UI-01`).
- **Cierre** (2026-11-24, rama `fix/auditoria`): Si un respaldo falla a mitad, se detiene y devuelve el informe parcial. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-04"></a>
### ERR-04 · Leer el respaldo para encontrar planes de energía creados traga errores y duplica esquemas
- **Afecta**: Sistema · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/engine.rs:418-429`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Leer planes creados propaga errores y se descarta el esquema no usado. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-05"></a>
### ERR-05 · El escaneo del Debloater convierte un fallo de PowerShell en «nada instalado»
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/debloater.rs:587-601,616-630`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): el escaneo exige elevación (sin ella `Get-AppxPackage -AllUsers` fallaba en silencio por `-ErrorAction SilentlyContinue` y decía «0 detectadas»; visto al recorrer la app). Comprobado: sin admin la vista muestra el error. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-06"></a>
### ERR-06 · El Debloater informa «Eliminada y desaprovisionada» cuando solo se quitó para el usuario actual
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/debloater.rs:718,747-752`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Informa «solo para el usuario actual» cuando corresponde. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-07"></a>
### ERR-07 · La papelera viene marcada por defecto y su «espacio liberado» se cuenta antes de vaciarla
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor, design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: `bytes_freed` sale de `SHQueryRecycleBinW` antes de `SHEmptyRecycleBinW(SHERB_NOCONFIRMATION)`; si falla, la vista no
  muestra el error y suma los MB en verde. `clean_recycle` empieza en `true` y el modal no avisa que es permanente; el modo «> 24h» no aplica.
- **Dónde**: `src/core/cleaner.rs:206-220`; `src/ui/views/cleaner.rs:28,79-80,101-102,213,240`.
- **Propuesta**: papelera desmarcada; volver a consultar tras vaciar; avisar en el modal.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Papelera: bytes = antes − después; desmarcada por defecto, rotulada «(permanente)» y con aviso en la confirmación. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-08"></a>
### ERR-08 · El staging de extracción queda huérfano: la limpieza corta a 16 niveles y traga el error
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado; cierre de la mini-ventana inferido · **Estado**: Resuelto
- **Dónde**: `src/core/archiver.rs:395,399,475,486,590,594`; `src/main.rs:200-203`.
- **Propuesta**: limitar la profundidad al extraer igual que al limpiar; limpiar también en los `?`; no cerrar la mini-ventana hasta limpiar.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Profundidad máxima 48 en extracción y limpieza con el mismo límite; test `saneo_limita_la_profundidad`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-09"></a>
### ERR-09 · La tienda compara versiones como texto y por un prefijo laxo: ofrece downgrades y apps equivocadas
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: con 7-Zip 25.x instalado aparece «Actualizar» a 24.09; el prefijo «Git » coincide con «Git LFS».
- **Dónde**: `src/core/app_store.rs:617-635`; `src/ui/views/uninstaller.rs:498,513-536`.
- **Propuesta**: comparar versiones numéricamente e identificar apps por clave exacta.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `display_name_matches` + `parse_numeric_version`; tests en `app_store.rs`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-10"></a>
### ERR-10 · El contador «N eliminados» incluye lo que no existía y los bytes salen del escaneo
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/uninstaller.rs:1069-1071,1082,1089-1091,1147-1149,1180-1182`; `src/ui/views/uninstaller.rs:241`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Lo inexistente no cuenta como eliminado; la vista muestra también los fallidos. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-11"></a>
### ERR-11 · Los fallos de `assign` o de la espera dejan el proceso vivo y el instalador en staging
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/app_store.rs:867-869,879-891`; `src/core/dependencies.rs:337-356`; `src/core/uninstaller.rs:688-690`.
- **Qué pasa**: `WAIT_FAILED` gira 300 s en app_store y en dependencies termina en «código 259» con el proceso vivo.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `run_in_job` termina el Job en timeout o cancelación. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="err-12"></a>
### ERR-12 · Parsing de SMBIOS frágil: posible pánico y offsets equivocados
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: derivado (especificación SMBIOS) · **Estado**: Resuelto
- **Qué pasa**: `&raw[8..written]` entra en pánico si `written` supera el buffer; el voltaje se lee en 0x28 (Memory Technology, no el
  configurado en 0x26); la velocidad prioriza la nominal sobre la configurada; un módulo de 512 MB se muestra como «0 GB».
- **Dónde**: `src/core/specs.rs:430-439,597-607`; `src/ui/views/hardware_tab.rs:181`.

# Expedientes · Concurrencia (`CON-`)
- **Cierre** (2026-11-24, rama `fix/auditoria`): Buffer SMBIOS acotado a lo reservado, voltaje leído en 0x26, velocidad configurada antes que la nominal y módulos < 1 GB en MB. Tests `voltaje_configurado_en_0x26`, `estructura_truncada_no_entra_en_panico`, `capacidad_de_modulos_pequenos`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="con-01"></a>
### CON-01 · `NamedMutex` trata `WAIT_ABANDONED` como error y deja el mutex tomado
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: code-auditor (las dos áreas) · **Evidencia**: observado; disparo inferido · **Estado**: Resuelto
- **Dónde**: `src/core/win_common.rs:226-233,251-255`.
- **Propuesta**: `WAIT_ABANDONED` = adquirido con aviso.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `WAIT_ABANDONED` se trata como adquirido. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="con-02"></a>
### CON-02 · Las tareas siguen al cambiar de pestaña: se pierde el resultado y se puede lanzar una segunda en paralelo
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor, design-challenger · **Evidencia**: derivado · **Estado**: Resuelto
- **Dónde**: `src/ui/mod.rs:549-566`; `src/ui/views/archiver.rs:25,54-76`; `src/ui/views/cleaner.rs:31,53-63`; dashboard y debloater.
- **Propuesta**: estado de trabajo en curso global (o en core, con lock por destino) y resultados que sobrevivan al desmontaje.

# Expedientes · Arquitectura y repositorio (`ARQ-`)
- **Cierre** (2026-11-24, rama `fix/auditoria`): Las pestañas con tareas se montan al visitarlas y siguen vivas ocultas (`KEEP_ALIVE_TABS` en `ui/mod.rs`): la tarea en curso conserva su resultado y su botón sigue deshabilitado. Sensores se desmonta para no sondear. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="arq-01"></a>
### ARQ-01 · `.cargo/config.toml` expone el usuario de Windows del autor y sigue en el historial
- **Afecta**: Desarrollador · **Severidad**: Baja · **Origen**: Revisión manual, security-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: `C:\Users\Bran\...` en `--remap-path-prefix`; el archivo está en `.gitignore` pero ya entró en `3d9b30b`.
- **Propuesta**: config local fuera del repo o variable de entorno en CI; para borrarlo del historial habría que reescribirlo (decisión del autor).
- **Cierre** (2026-11-24, rama `fix/auditoria`): Fuera del índice (`git rm --cached`) y en `.gitignore`. Sigue en el historial publicado: reescribirlo no se hace (decisión del autor). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="arq-02"></a>
### ARQ-02 · El changelog se llama `# Changelog — OtterSwiss.txt` y está desactualizado
- **Afecta**: Desarrollador · **Severidad**: Baja · **Origen**: Revisión manual · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: el nombre incluye `#` y guion largo; llega a 3.1.0 y la versión es 3.5.1.
- **Propuesta**: `CHANGELOG.md` con 3.2–3.5.1.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Renombrado a `CHANGELOG.md` con 3.5.0, 3.5.1 y «Sin publicar». Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="arq-03"></a>
### ARQ-03 · `drivers.rs` es código muerto y devuelve siempre `None` en versión y fecha
- **Afecta**: Desarrollador · **Severidad**: Baja · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Diferido
- **Dónde**: `src/core/drivers.rs:15,54`; sin uso fuera del módulo.

# Expedientes · Interfaz (`UI-`)
- **Cierre** (2026-11-24, rama `fix/auditoria`): Decisión del autor (2026-11-24): se usará para actualizar drivers desde la app.


<a id="ui-01"></a>
### UI-01 · «Rollback Completado» y los informes de aplicación ocultan fallos, omisiones y advertencias
- **Afecta**: Usuario · **Severidad**: Alta · **Origen**: code-auditor, design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: la tarjeta solo muestra contadores verdes y nunca `failed_items`, `skipped_user_modified` ni `backup_deleted`; las
  `warnings` de la aplicación tampoco. El subtítulo «Transaccional con rollback seguro» / «Transactional with full rollback» no se cumple.
- **Dónde**: `src/ui/views/dashboard.rs:187-188,324-365`; `src/core/engine.rs:78-85`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): El rollback muestra restaurados, conservados, con error y si se borró el respaldo; el informe de aplicación muestra fallos y advertencias; sin «Transaccional». Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-02"></a>
### UI-02 · Banners de «OK» en verde aunque haya errores, y estados de éxito falsos
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: Debloater «OK — Apps: {} | Reg: {} | Err: {}» con `badge-success` y sin `item.message`; tienda «OK: {} | Error: {}»;
  apps que nunca estuvieron instaladas salen como «Eliminada»; «desinstalado sin rastros» tras un fallo (`SYS-02`).
- **Dónde**: `src/ui/views/debloater.rs:138-141,287,306,383-390`; `src/ui/mod.rs:238`; `src/ui/views/uninstaller.rs:146-150`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Componente `StatusBanner` con tono según el resultado (desinstalador, Debloater, tienda, limpiador, DNS); «No instalada» en vez de «Eliminada»; test `tono_del_aviso_sale_del_informe`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-03"></a>
### UI-03 · Cancelar el Debloater oculta las apps que ya se eliminaron
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor, design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/debloater.rs:687-689,722-723`; `src/ui/views/debloater.rs:148`.
- **Propuesta**: devolver el informe parcial junto con `Cancelled` y re-escanear.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Cancelar devuelve el informe parcial (`report.cancelled`), la vista lo muestra en amarillo y re-escanea. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-04"></a>
### UI-04 · El modal del updater se queda bloqueado si falla el lanzamiento del instalador
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/ui/mod.rs:325,340,529-540` (`let _ = launch_installer_and_exit`; el bucle de sondeo nunca termina).
- **Cierre** (2026-11-24, rama `fix/auditoria`): Si el instalador no se lanza, el modal muestra el error y se desbloquea. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-05"></a>
### UI-05 · Trabajo bloqueante en el hilo de UI (arranque, pestañas, salida de Sensores)
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor (las dos áreas) · **Evidencia**: observado; duración inferida · **Estado**: Resuelto
- **Dónde**: `src/ui/mod.rs:74-80,250`; `src/ui/views/dashboard.rs:22`; `src/ui/views/dns.rs:19,113,143`; `src/ui/views/tools.rs:23`;
  `src/ui/views/uninstaller.rs:170`; `src/core/sensors.rs:970-975` (el `Drop` hace `join` de un `poll_all` WMI de hasta ~38 s).
- **Cierre** (2026-11-24, rama `fix/auditoria`): Inventario de apps, hardware del Dashboard, adaptadores DNS y estado de la tienda se cargan con `spawn_blocking`; `SensorMonitor` ya no hace `join` al destruirse. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-06"></a>
### UI-06 · Hardware se queda en «Analizando...» para siempre ante un fallo parcial
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: code-auditor, design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/core/specs.rs:158-163`; `src/ui/mod.rs:256-260`; `src/ui/views/hardware_tab.rs:23-31`.
- **Propuesta**: devolver lo que sí se pudo leer y mostrar el error.
- **Cierre** (2026-11-24, rama `fix/auditoria`): `collect_system_specs` devuelve lo que pudo leer con `warnings`; Hardware muestra el aviso o un estado de error. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-07"></a>
### UI-07 · Faltan estados de vacío y error en DNS, y «Restaurar DNS» está activo sin respaldo
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/ui/views/dns.rs:19` (`unwrap_or_default`).
- **Cierre** (2026-11-24, rama `fix/auditoria`): DNS con estados cargando, vacío y error; «Restaurar» solo si `has_backup`. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-08"></a>
### UI-08 · Contraste por debajo de WCAG AA en cinco de seis temas
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: derivado (cuentas con los tokens de `assets/main.css:18-101`) · **Estado**: Resuelto
- **Qué pasa**: blanco sobre `--accent`: Cyberpunk 1.77, Nord 2.00, Emerald 2.54, Crimson 3.67, Dark 3.68 (Light 5.17 pasa). Blanco sobre
  `--danger`: Emerald 2.77, Crimson 2.69, Cyberpunk 3.62, Dark 3.76, Nord 4.09. Light: badge-warning 2.84, badge-success 3.26. Los
  tintes de badges usan RGB fijos (`main.css:222-224`).
- **Propuesta**: tokens `--on-accent` y `--on-danger` por tema.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Tokens `--on-accent`, `--on-danger` y `--*-text` por tema, calculados ≥ 4.5:1 (WCAG AA); insignias con `color-mix`. Contraste medido por fórmula y revisado en la app en los temas oscuro y claro. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-09"></a>
### UI-09 · Sin `:focus-visible` y modales sin `role="dialog"`, foco atrapado ni Escape
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `assets/main.css` (ninguna regla `:focus-visible`); los cinco modales (update, reinicio, limpieza, rastros, RAM).
- **Cierre** (2026-11-24, rama `fix/auditoria`): Componentes `Modal`/`ConfirmDialog` con `role="dialog"`, `aria-modal`, foco inicial, Escape y foco atrapado (script en `main.rs`); `:focus-visible` global. Los seis modales los usan. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-10"></a>
### UI-10 · 102 líneas con px, hex o `rgba()` sueltos en `src/ui/`, prohibidos por la regla de temas
- **Afecta**: Desarrollador · **Severidad**: Baja · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: hardware_tab 32, uninstaller 14, tools 13, dashboard 10, mod.rs 9, archiver 8, debloater 8; además
  `body { background: #0f111a !important }` hace parpadear el tema claro.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Espaciados y tipografía pasados a tokens; quedan solo bordes de 1px, radios de pastilla y tres anchos de layout (`minmax(210px)`, `min-width: 240px`, `height: 40px` de la mini ventana). El fondo inicial sale del tema guardado (`with_background_color`). Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-11"></a>
### UI-11 · Textos de core sin traducir y mezcla de idiomas
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Parcial
- **Qué pasa**: en inglés salen en español las etiquetas de tweaks, categorías del limpiador y de restos, nombres del Debloater, el
  menú contextual y todos los `CoreError`. En español salen en inglés «Restored {} NIC(s)», «No active physical adapters», «Thread error».
- **Dónde**: `src/ui/i18n.rs`; `src/core/tweaks_catalog.rs`; `src/ui/views/dns.rs:99,106,136`; `src/ui/views/dashboard.rs:346,376`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Traducidos los textos de la UI (DNS, tienda, «Thread error», «Cargando...», avisos de privacidad DNS). Los textos que vienen del núcleo (tweaks, categorías, Debloater, `CoreError`) siguen solo en español: diferido por decisión del autor. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-12"></a>
### UI-12 · El plan de optimización y el Debloater no explican lo que hacen
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/ui/views/dashboard.rs:371-387` (no se renderiza `item.detail`); `src/ui/views/debloater.rs:378-379`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Plan y resultados muestran `detail`; el Debloater muestra la descripción de apps y ajustes. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-13"></a>
### UI-13 · «Reinicio requerido» aparece cuando basta cerrar sesión, y «Reiniciar ahora» fuerza sin avisar
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Dónde**: `src/ui/views/dashboard.rs:202-203`; `src/core/win_common.rs:575-578` (`EWX_REBOOT|EWX_FORCEIFHUNG`).
- **Cierre** (2026-11-24, rama `fix/auditoria`): El modal dice «Cierra sesión o reinicia» y avisa que reiniciar cierra las apps abiertas. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-14"></a>
### UI-14 · Sensores: «CPU / Placa (WMI ACPI)» es una zona térmica ACPI y dos GPU iguales comparten `key`
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: design-challenger, code-auditor · **Evidencia**: inferido / observado · **Estado**: Resuelto
- **Dónde**: `src/core/sensors.rs:724,764`; `src/ui/views/sensors.rs:36,145`; `src/ui/views/hardware_tab.rs:128`.
- **Cierre** (2026-11-24, rama `fix/auditoria`): las zonas ACPI (WMI y contador de rendimiento) se llaman «ACPI (zona)» con tipo «Zona térmica ACPI» y la sección «Zonas térmicas ACPI»; nombres de GPU repetidos con « #n» y claves por índice; test `nombres_de_sensores_repetidos_se_distinguen`. Comprobado en la app. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ui-15"></a>
### UI-15 · Detalles de textos y catálogo
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: design-challenger, code-auditor · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: Steam como «vEvergreen» (`views/tools.rs:414`); Firefox siempre `lang=es-ES` y Battle.net `--lang=esES`
  (`app_store.rs:184,380`); el benchmark DNS mide ping ICMP y rotula «Mejor» (`views/dns.rs:57-59`); el subtítulo del Debloater dice
  cosas distintas por idioma (`views/debloater.rs:194`); «Extraer aquí» aparece en todo `.gz` aunque un .gz suelto falle
  (`context_menu.rs:48-52`); comprimir `mi.proyecto\` genera `mi.zip` (`main.rs:276-277`).

<a id="ui-16"></a>
### UI-16 · Hardware lista la misma GPU dos veces y muestra el adaptador de software de Microsoft
- **Afecta**: Usuario · **Severidad**: Baja · **Origen**: recorrido de la app (2026-11-24) · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: con un controlador de pantalla virtual (IddCx, p. ej. «Virtual Display Driver») `EnumAdapters1` devuelve la misma
  RX 7800 XT dos veces, con LUID distinto pero igual proveedor, dispositivo, subsistema y VRAM; `Win32_VideoController` solo ve una.
  También aparece «Microsoft Basic Render Driver» (1414:008C) con `Flags = 0`, que no es una GPU física.
- **Dónde**: `src/core/specs.rs` (`read_dxgi_gpus`).
- **Propuesta**: descartar el render de software de Microsoft y unir las entradas con el mismo hardware. Contra: dos GPU idénticas
  reales (multi-GPU del mismo modelo) se verían como una.

# Expedientes · Diseño y producto (`UX-`)
- **Cierre** (2026-11-24, rama `fix/auditoria`): `keep_gpu_adapter` descarta el render de software de Microsoft y une las entradas con el mismo hardware; test `gpu_repetida_y_render_de_software_se_descartan`. Comprobado en la app: Hardware muestra una sola RX 7800 XT. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ux-01"></a>
### UX-01 · «Desinstalar» no pide confirmación y desinstala en modo silencioso
- **Afecta**: Usuario · **Severidad**: Alta · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: el botón llama directo a `start_uninstall_and_scan`; el core prefiere `QuietUninstallString` y fuerza `/qn /norestart` en MSI.
- **Dónde**: `src/ui/views/uninstaller.rs:555-568`; `src/core/uninstaller.rs:571-597`.
- **Propuesta**: modal con el nombre de la app y «se desinstalará sin preguntar».
- **Cierre** (2026-11-24, rama `fix/auditoria`): Confirmación con el nombre de la app y «se ejecutará en modo silencioso». Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="ux-02"></a>
### UX-02 · «Optimizar Sistema» aplica sin confirmar tweaks sin base o con descripciones equivocadas
- **Afecta**: Usuario, Sistema · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: observado + fuentes · **Estado**: Diferido
- **Qué pasa**: `NetworkThrottlingIndex=0xFFFFFFFF` (sin documentación de Microsoft como optimización; análisis que miden más latencia
  DPC); `SystemResponsiveness` descrito como «para procesos en primer plano» cuando es la reserva de CPU de MMCSS; desactivar la
  aceleración del ratón (preferencia personal); SysMain deshabilitado; `dmwappushservice` deshabilitado (lo usa MDM/Intune, inferido).
- **Dónde**: `src/core/tweaks_catalog.rs:139-197,220-245`; `src/ui/views/dashboard.rs:282-293`.
- **Fuentes**: [Microsoft Learn MMCSS](https://learn.microsoft.com/en-us/windows/win32/procthread/multimedia-class-scheduler-service),
  [djdallmann GamingPCSetup](https://djdallmann.github.io/GamingPCSetup/CONTENT/RESEARCH/NETWORK).
- **Propuesta**: confirmación con el plan; sacar NetworkThrottlingIndex y ratón del perfil automático o dejarlos opcionales desmarcados.
- **Cierre** (2026-11-24, rama `fix/auditoria`): Decisión del autor (2026-11-24): qué ajustes salen del perfil automático queda para después. Ya hecho: «Optimizar Sistema» pide confirmación mostrando el plan con su detalle.


<a id="ux-03"></a>
### UX-03 · «Optimizar RAM» es una práctica desacreditada y se anuncia como mejora
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: fuente · **Estado**: Pendiente de decisión
- **Qué pasa**: `trim_working_sets` vacía el working set de todos los procesos; fuerza fallos de página y expulsa código útil (Russinovich,
  «The Memory-Optimization Hoax»). La UI dice «Optimización de RAM Completada».
- **Dónde**: `src/core/sys_tools.rs:66+`; `src/ui/views/tools.rs:146`.
- **Propuesta**: retirarlo o rotularlo «puede ralentizar temporalmente».
- **Cierre** (2026-11-24, rama `fix/auditoria`): El autor avisará qué hacer (2026-11-24).


<a id="ux-04"></a>
### UX-04 · «Aplicar DNS» cambia todos los adaptadores sin preguntar y oculta el aviso de privacidad
- **Afecta**: Usuario · **Severidad**: Media · **Origen**: design-challenger · **Evidencia**: observado · **Estado**: Resuelto
- **Qué pasa**: rompe DNS de dominio en PCs de empresa o un Pi-hole; `privacy_notice` existe por preset pero no se usa en `src/ui`.
- **Dónde**: `src/ui/views/dns.rs:92-96`; `src/core/dns.rs:43-72`.
- **Propuesta**: confirmar mostrando los adaptadores afectados y el aviso de privacidad.

# Expedientes · Pruebas (`TST-`)
- **Cierre** (2026-11-24, rama `fix/auditoria`): Confirmación con los adaptadores físicos afectados, el aviso de privacidad del preset y el riesgo en redes de empresa. Verificado: `cargo test` y `clippy --all-targets -- -D warnings` en verde en Windows; interfaz recorrida sin elevación (2026-11-24). Las acciones que cambian el sistema no se ejecutaron.


<a id="tst-01"></a>
### TST-01 · El repositorio no tiene ni un test
- **Afecta**: Desarrollador · **Severidad**: Alta · **Origen**: code-auditor (las dos áreas) · **Evidencia**: observado (`#[test]` y `cfg(test)` en `src/`: 0; no existe `tests/`) · **Estado**: Parcial
- **Qué pasa**: toda la lógica de respaldo, restauración, saneo de rutas y heurística de restos está sin red.
- **Propuesta**: empezar por funciones puras: `sanitize_archive_rel_path`, `validate_path_components`, `build_app_tokens`/`matches_any_token`,
  `parse_cmd_exe_and_args`, `is_safe_registry_subkey_to_delete`, `find_newer_curated_version`, `parse_version_tuple`, `parse_smbios_type17`;
  luego los de `SYS-06`, `SYS-07`, `SYS-10` y `SYS-12` y el de `SEC-01`.
- **Cierre parcial** (2026-11-24, rama `fix/auditoria`): 36 tests en 14 archivos (funciones puras de la lista, `SEC-01`, `SYS-01`, `SEC-03/10`, `SYS-02/20`, `UI-02/14/15/16`, `ERR-12`), en verde con `cargo test` en Windows. Se comprobó que protegen: al romper la validación de raíz de `SEC-01` y el destino libre de `SYS-01`, sus tests fallan. `build.rs` aplica el manifiesto de administrador solo al ejecutable (`compile_for`) y el binario tiene `test = false`, para que `cargo test` corra sin elevación. Pendiente: tests de `SYS-06/07/10/12`, que necesitan registro y respaldos.
