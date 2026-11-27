# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-26

**Rama:** todo está en `main` (v5.0.0, 26/11/2026), subido a `origin/main`. Las ramas `auditoria/buenas-practicas` y
`fix/auditoria` se fusionaron y se borraron. No hay tag `v5.0.0` todavía: subirlo dispara `release.yml` y publica la release que
ofrece el actualizador a los usuarios; se hace cuando el autor lo pida.

**Hecho en esta rama:**
- Buenas prácticas: core por capas (`platform` → `security` → funciones), UI ordenada, catálogo de textos, edición 2024, CI y lints,
  SAFETY en todo `unsafe`, un solo cliente HTTPS y un solo camino de descarga e instalación.
- Fusión de v4.2.0 portada a esa arquitectura (tienda de 50 apps con restaurador UWP, pestaña Drivers, 4 temas ajustados a AA,
  puente de LibreHardwareMonitor, dependencias nuevas).
- Auditoría del código de v4.2.0 en tres pasadas (seguridad, bugs, buenas prácticas): 28 defectos (`SEC-30`…`TST-02`) y 27 mejoras
  (`OR-06`…`HE-14`). Cerrados 23 de los 28, entre ellos los tres de seguridad alta (`SEC-30` ZIP de Intel sin verificar, `SEC-31`
  winget elevado desde LocalAppData; `SEC-32` parcial con la firma encadenada a raíces de la máquina).
- Drivers: confirmación con lista y respaldo, respeto de la política y WSUS, Catálogo verificado (su búsqueda nunca había funcionado:
  el parser buscaba comillas dobles; probado en vivo con `catalogo_en_vivo`), historial de paquetes instalados y «Quitar».
- Tienda: winget solo como el usuario, códigos de salida con nombre, SHA-256 de VSCodium y BleachBit, fuera LibreWolf y Rufus, hosts
  de GitHub compartidos, test de invariantes del catálogo, detección de apps de la Store por paquete, IDs de la Store corregidos.
- Sesión del 25/11 (tarde): `CL-19` (fuera el `#[allow]`, 57 → 16 `unsafe` en drivers, todos con SAFETY; guardianes a
  `platform/handles.rs`), regresiones de la fusión (`TE-25`, `TE-27`, `TE-28`, `IN-08`: clases `.item-row`/`.grow-wide`, filtros
  con `aria-pressed`, catálogo `ui/i18n/drivers.rs`), `OR-06` (`core/drivers/` en 10 archivos), `CON-03` (botón Cancelar en la
  búsqueda) y `HE-14` (`tests/indicadores.rs` y `too_many_lines` en core). De paso `AR-23` hecha y `AR-24` parcial.
- Sesión del 25/11 (noche): `ERR-25` (la fuente Intel nunca funcionó: el servicio devuelve un ZIP de reglas de DSA; quitada, cierra
  `SYS-27` y `TST-02`), `SEC-32` (proxy WinHTTP de la máquina o directo, nunca el de HKCU), `CL-22`, `AR-24` (tabla PCI única en
  `core/hardware/pci.rs`), `TE-29` (paletas corregidas, Claro con muted en AA, `tests/temas.rs` exige AA en los 10 temas), `IN-03`
  (`ui/i18n/msg.rs`: los avisos se traducen al mostrarse) y parte de `UI-11`/`AR-17`/`AR-26` (títulos de error traducidos por
  `CoreError::kind()`, informe de drivers como datos). `HE-11` descartada: `dx fmt` 0.7.10 borra ramas `else if` dentro de `rsx!`.
- Sesión del 25/11 (cierre): tienda por `winget install` dentro del Job (`SEC-35`; `store/winget.rs`, `InstallSource`), catálogos de
  core traducidos en la UI (`ui/i18n/core_text.rs`, test `catalogos_de_core_traducidos`), `AR-28`, `OR-07` (`AppIconBadge`),
  `TE-26`, `CL-24` (`read_wmi<T>`), `AR-29` (`run_blocking` en drivers), `CL-20`, `IN-09` e `IN-10`.
- Sesión del 25/11 (refactors): `CL-21` (`DriversView` en subcomponentes), `CL-25` (`CheckRow`, `EmptyState`, `LoadingState`),
  `TE-02` (`style:` 45 → 2), `IN-02` parcial (un módulo de textos por vista en `ui/i18n/`, `if en` 216 → 20), `OR-04` (29
  archivos reordenados con script, excepciones en `AGENTS.md`), `AR-06` parcial (`source: None` 115 → 95; `Registry` y `Service`
  guardan `BoxedSource`), `CL-11`, `CL-23`, `AR-25` (`NumericVersion`) y `AR-27` (`problem: Option<u32>`). Techos de
  `tests/indicadores.rs` bajados: `style:` 2, `if en` 20, `source: None` 95.
- Sesión del 25-26/11 (cierre de todo lo diferido): `CL-05` (las seis vistas largas en subcomponentes), `AR-06`, `AR-02`, `AR-17`,
  `AR-18`, `AR-26`, `IN-02`, `UX-02` y `UX-03` hechas; `AR-19` (nueve structs sin estado pasaron a funciones de módulo, `DnsManager`
  sigue como struct); `SEC-17` (manifiesto `asInvoker`, aviso «Modo estándar» con «Reabrir como administrador», drivers con
  `require_elevated`) y `SYS-24` (`elevation::shell_user_differs` + `RegLocation::ensure_writable` bloquean HKCU con otra cuenta).
  Techos de `tests/indicadores.rs`: `style:` 2, `if en` 10, `source: None` 10. Ya no hay diferidos ni pendientes de decisión.
- Sesión del 26/11 (mañana): rediseño de la interfaz al estilo Windows 11 (`TE-30`, decisión del autor): navegación lateral,
  encabezado de página en el shell, `InfoBar`, tokens Fluent y las diez vistas rehechas (un agente Sonnet por vista, revisadas en la
  app). Reglas nuevas en `.agents/rules/ui-temas.md` §8. Sin probar uno por uno los otros 9 temas de color.
- Sesión del 26/11 (tarde): `TE-31`, navegación arriba por defecto (pedido del dueño) o lateral contraíble desde la nueva
  sección Configuración, estilo abierto sin cajas, portada de Inicio con la nutria. Visto en la app en ambos modos.
- Sesión del 26/11 (noche): confirmaciones de revertir y quitar (`UI-24`), Archivos con «Abrir carpeta» (`UI-25`), cancelar
  drivers sin liberar la vista antes de tiempo (`CON-04`), `TST-01` cerrado (tests de `SYS-06/07/10/12`), logos de marca en la
  tienda (Simple Icons, CC0), reapertura elevada con UAC probada (`SEC-17`) y, en una VM ya borrada, proxy de máquina (`SEC-32`)
  y otra cuenta (`SYS-24`). Falta solo instalar y quitar drivers en un equipo real.
- **Último resultado:** `cargo test` 99 + 4 de integración (más 3 ignorados y 8 de VM), `clippy --all-targets -- -D warnings` y
  `fmt --check` en verde. Recorrido en la app sin elevar hecho (2026-11-25, oscuro y claro, español e inglés, todas las pestañas; sin diferencias salvo que el detalle de errores de core sigue en español: `AR-06`). Antes la pantalla
  estaba bloqueada. No se probaron el diálogo de confirmación de Drivers ni la lista con errores (no hay dispositivos con error).

**Pendiente** (detalle en `docs/BUGS.md` y `docs/PRACTICAS.md`, sección «Siguiente» de cada uno):
- Recorrido de la interfaz sin elevar hecho el 26/11 (todas las pestañas, ES/EN, oscuro, claro y Tokyo Night, y al ancho mínimo de 960 px; salió `UI-23`). Falta la reapertura elevada con UAC (`SEC-17`), que acepta el usuario. Para repetirlo: lanzar el exe con `explorer.exe <ruta>`, con el monitor VDD activo; la ventana arranca minimizada (restaurarla con `ShowWindow`) y los clics van por el proceso `otter_swiss.exe` concedido en `request_access`.
- VM de Hyper-V `OtterTest` (hecho el 26/11, 8 de 8 de `tests/vm_sistema.rs`; `ERR-26` y `ERR-27` salieron de ahí). No se probó: drivers
  de fabricante (la VM no tiene hardware), proxy de máquina (`netsh winhttp set proxy`) ni `SYS-24` con una cuenta estándar que pide
  credenciales de administrador. Para repetirlo: la ISO está en `D:\VM\iso\win11-ent-eval-25h2.iso`; la VM necesita App Installer
  actualizado (winget 1.29) y la prueba de la tienda debe correr en la sesión de escritorio (tarea interactiva), no por PowerShell Direct.
- Subir la rama y abrir el PR cuando el usuario lo pida; avisar al compañero de `SEC-30`…`SEC-32`, de que LibreWolf, Rufus e Intel
  salieron, de la tienda por winget, del cambio de paletas y de que la app ya arranca sin administrador.

## Prompt para seguir

Pegar en una sesión nueva, en la carpeta del repo:

> Seguimos con OtterSwiss en la rama `auditoria/buenas-practicas` (commits locales, sin push). Lee `AGENTS.md`, `.agents/relevo.md`
> (sección «Estado actual»), `docs/BUGS.md` y `docs/PRACTICAS.md`, y `git status`. Verifica con `cargo fmt --check`,
> `cargo clippy --all-targets -- -D warnings` y `cargo test` en PowerShell. Lo que falta es solo la reapertura elevada con UAC. El recorrido y las pruebas de VM ya están hechos. Commitea en local,
> «Categoría: Verbo …», cerrando cada ID en `docs/*-detalle.md` con su línea **Cierre**. No hagas push ni PR sin que te lo pida.
> Español neutro, respuestas breves.

**VM:** VirtualBox no sirve en esta PC (el hipervisor de Windows está activo). Se usó Hyper-V (ya activado); la ISO
verificada está en `D:\VM\iso\win11-ent-eval-25h2.iso`, descargada de Microsoft.

**Windows Sandbox (si se reusa):** característica ya activada. Lanzar con `WindowsSandbox.exe <wsb>`. NUNCA matar sus procesos (deja VMs
huérfanas); se cierra con la X. La prueba eleva con `RunAs`: el usuario acepta el UAC dentro. El control de pantalla no ve esa ventana.

## Cómo verificar

El usuario **apagó Smart App Control** el 24/11: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build` y
`cargo fmt` corren en Windows (PowerShell; nunca desde Git Bash, donde `link` es el de coreutils). **Último resultado: 99
tests + 4 de integración y clippy en verde.** `.agents/scripts/check-wsl.sh` queda como alternativa desde WSL (solo compila).

Para recorrer la interfaz: desde `SEC-17` el `.exe` arranca sin elevar (`asInvoker`), así que se lanza con
`explorer.exe <ruta del exe>` para que quede en el mismo nivel que el control de pantalla (UIPI no deja controlar ventanas
elevadas). La captura de pantalla solo ve la ventana con el monitor virtual VDD activo. Recorrido hecho el 25/11: todas las
pestañas, oscuro y claro, español e inglés. Falta lo posterior (ver «Pendiente»), las acciones de sistema como admin (VM) y la
ventana en tamaño chico.

## Decisiones del usuario (2026-11-25)

- `ERR-25`: buscar otra fuente de Intel; si no hay una oficial y legible por máquina, quitarla y dejar Intel a Windows Update. No se
  encontró ninguna (solo catálogos de terceros que son copias de un momento): se quita.
- `SEC-32`: la app elevada usa el proxy de la máquina (WinHTTP, HKLM) o conexión directa, nunca el de HKCU.
- Tienda (`SEC-35`, `ERR-24`): primero se eligió `winget download` + verificación propia; al comparar, el autor eligió
  **`winget install` dentro de nuestro Job** (reimplementar los argumentos silenciosos de cada tipo de instalador era lo más frágil).
  Hecho: la tienda instala todo con `winget install` dentro de nuestro Job (plazo de 30 minutos y cancelación). Cada app del catálogo tiene una fuente (`InstallSource::Winget { id, id_es, scope }` o `Store { id }`) en vez de URL, hosts, hash, firmante, versión y argumentos. winget comprueba el SHA-256 del manifiesto revisado en el repositorio de Microsoft (como administrador no se puede saltar) y trae siempre la última versión con sus argumentos silenciosos. Como administrador corre el winget de App Installer de `Program Files\WindowsApps` (ruta por `GetPackagesByPackageFamily`, firma de Microsoft verificada y archivo bloqueado mientras corre); las apps por usuario (Discord, Spotify, Playnite, VS Code) y las de la Store corren con el token del usuario (`SEC-31`). Se pierde la segunda verificación de firma propia del instalador.
- `IN-03`: los avisos guardan el evento y se traducen al mostrarse. `UI-11`/`AR-17`/`AR-26`: se hacen ahora, junto con `IN-03`.
- `HE-11`: se instaló dioxus-cli 0.7.10, pero `dx fmt` borra ramas `else if` dentro de `rsx!` (rompió `app.rs` y `dashboard.rs`):
  descartado y revertido. No usar `dx fmt` sin revisar el diff y compilar.

## Decisiones del usuario (2026-11-24)

- `SEC-17` (asInvoker + auxiliar elevado) y con él `SYS-24`: diferidos.
- `UX-02` (qué ajustes salen del perfil automático): diferido. Se conserva la confirmación con el plan antes de optimizar.
- `UX-03` (Optimizar RAM): pendiente, el usuario avisará.
- `ARQ-01`: `.cargo/config.toml` fuera del índice y en `.gitignore`; no se reescribe el historial.
- `ARQ-02`: changelog renombrado a `CHANGELOG.md` y completado.
- `ARQ-03`: `drivers.rs` se conserva; se usará para actualizar drivers desde la app.
- `UI-11`: los textos del núcleo en inglés quedan para otro PR.
- qBittorrent: se descargó el instalador 5.1.4 y se fijó su SHA-256 (no trae Authenticode; firma PGP).

## Ojo

- Las acciones que cambian el sistema no se ejecutaron: probarlas como administrador antes de publicar una versión.
- `rust-version` subió a 1.93 por `sevenz-rust2` 0.25.
- `Cargo.lock` cambia por la versión del paquete (3.5.1) y la migración de 7z.
