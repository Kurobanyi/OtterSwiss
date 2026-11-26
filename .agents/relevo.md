# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-25

**Rama:** `auditoria/buenas-practicas`, unos 130 commits locales **sin push ni PR** (el usuario todavía no quiere subirla). Ya trae
`origin/main` con la v4.2.0 del compañero (`0a0f5ec`, fusionada en `4767abf`) y `windows` 0.61 (`10a585b`). `main` local está
viejo (`11cb659`); `origin/main` es `0a0f5ec`. `tmp/windows-061` ya está dentro y se puede borrar. Árbol limpio.

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
- **Último resultado:** `cargo test` 78 + 4 de integración (`indicadores`, `temas`; más 3 ignorados y 7 de VM),
  `clippy --all-targets -- -D warnings` y `fmt --check` en verde. **Sin recorrer en la app**: la Tienda nueva sí; los catálogos
  traducidos en inglés (Inicio, Debloater, Limpiador, restos) no, porque otra ventana tapaba la pantalla.

**Pendiente** (detalle en `docs/BUGS.md` y `docs/PRACTICAS.md`, sección «Siguiente» de cada uno):
- Recorrer en inglés Inicio (plan y resultado), Debloater, Limpiador y los restos del Desinstalador (catálogos de `core_text.rs`).
- En una VM de Hyper-V (Windows Sandbox no trae winget): instalar una app de la tienda para el equipo y una por usuario, cancelar una
  a mitad, «Buscar actualización» con una app al día; `tests/vm_sistema.rs`; instalar y quitar un driver de cada camino; quitar una
  app Appx; descargas detrás de un proxy de máquina (`netsh winhttp set proxy`).
- Refactors grandes, de a un archivo y sin cambiar comportamiento: `CL-21` (partir `DriversView`), `CL-25`, `TE-02` (45 `style:`),
  `IN-02` (216 `if en`), `OR-04`, `AR-06` (116 `source: None`), `CL-05`, `AR-02`, `CL-11`, `CL-23`, `AR-18`, `AR-25`, `AR-27`, y el
  resto de `AR-17`/`AR-26`/`UI-11` (detalles con datos del motor). Al bajar un indicador, bajar su techo en `tests/indicadores.rs`.
- Decisiones del autor: `AR-19`, `UX-02`, `UX-03`, `SEC-17` (con `SYS-24`).
- Subir la rama y abrir el PR cuando el usuario lo pida; avisar al compañero de `SEC-30`…`SEC-32`, de que LibreWolf, Rufus e Intel
  salieron, de la tienda por winget y del cambio de paletas.

## Prompt para seguir

Pegar en una sesión nueva, en la carpeta del repo:

> Seguimos con OtterSwiss en la rama `auditoria/buenas-practicas` (commits locales, sin push). Lee `AGENTS.md`, `.agents/relevo.md`
> (sección «Estado actual»), `docs/BUGS.md` y `docs/PRACTICAS.md`, y `git status`. Verifica con `cargo fmt --check`,
> `cargo clippy --all-targets -- -D warnings` y `cargo test` en PowerShell. Sigue con lo pendiente, en este orden: recorrer en inglés
> las vistas con catálogos traducidos, y los refactors grandes de a un archivo (`CL-21`, `CL-25`, `TE-02`, `IN-02`, `OR-04`, `AR-06`…)
> sin cambiar comportamiento. Sin tocar lo diferido (`SEC-17`, `SYS-24`, `UX-02`, `UX-03`) ni `AR-19`. Commitea en local por fase,
> «Categoría: Verbo …», cerrando
> cada ID en `docs/*-detalle.md` con su línea **Cierre**. No hagas push ni PR sin que te lo pida. Español neutro, respuestas breves.

**VM:** VirtualBox no sirve en esta PC (el hipervisor de Windows está activo: VirtualBox cae a NEM, el EFI se cuelga en `DXE_AP` con 2
núcleos y Windows 11 exige 2). La VM se borró; queda la ISO verificada `D:\VMs\Win11_Enterprise_Eval_es-es.iso`
(SHA-256 oficial `8F344BEA…E4C584`) por decisión del usuario. Para un Windows completo: activar Hyper-V (pide reinicio).

**Windows Sandbox (si se reusa):** característica ya activada. Lanzar con `WindowsSandbox.exe <wsb>`. NUNCA matar sus procesos (deja VMs
huérfanas); se cierra con la X. La prueba eleva con `RunAs`: el usuario acepta el UAC dentro. El control de pantalla no ve esa ventana.

## Cómo verificar

El usuario **apagó Smart App Control** el 24/11: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build` y
`cargo fmt` corren en Windows (PowerShell; nunca desde Git Bash, donde `link` es el de coreutils). **Último resultado: 78
tests + 4 de integración y clippy en verde.** `.agents/scripts/check-wsl.sh` queda como alternativa desde WSL (solo compila).

Para recorrer la interfaz: la app pide administrador y Windows (UIPI) no deja controlar ventanas elevadas, así que se compila
una copia con `asInvoker` en `$env:TEMP\otter-ui-target` (cambiando `build.rs` solo para esa compilación y restaurándolo) y se
recorre sin elevación. Recorrido hecho: todas las pestañas en oscuro, Dashboard y Limpiador en claro e inglés, diálogos con
foco atrapado y Escape. Falta: acciones de sistema como admin (mejor en VM o Windows Sandbox) y ventana en tamaño chico.

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
