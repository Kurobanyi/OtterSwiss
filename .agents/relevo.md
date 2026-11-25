# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-25

**Rama:** `auditoria/buenas-practicas`, unos 120 commits locales **sin push ni PR** (el usuario todavía no quiere subirla). Ya trae
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
- **Último resultado:** `cargo test` 76 + 4 de integración (`indicadores`, `temas`; más 2 ignorados de red y 7 de VM),
  `clippy --all-targets -- -D warnings` y `fmt --check` en verde. Recorridos sin elevar: Drivers y Tienda; paletas Sakura, Dracula,
  Solarizado, Tokyo Night y Claro; avisos y errores cambiando de idioma.

**Pendiente** (detalle en `docs/BUGS.md` y `docs/PRACTICAS.md`, sección «Siguiente» de cada uno):
- **Tienda con `winget download` (decidido, `SEC-35` y `ERR-24`):** las 32 apps de descarga directa pasan a `winget download` (última
  versión, SHA-256 del manifiesto) más la verificación propia (firmante del fabricante por app, Job con plazo). Diseño pendiente:
  dónde corre winget (como admin, desde `C:\Program Files\WindowsApps\Microsoft.DesktopAppInstaller_*\winget.exe`, a una carpeta
  solo-admin, para que un proceso del usuario no cambie el instalador ni su manifiesto entre la descarga y la verificación),
  cómo leer los argumentos silenciosos del YAML del manifiesto, y qué hacer si winget falta. Registrar el diseño como decisión antes.
- `UI-11`/`AR-17`/`AR-26`, resto: los catálogos con texto en core (ajustes y categorías del motor, Debloater, limpiador, restos, menú
  contextual, rellenos de `specs`, «Desconocida» de NVIDIA, sensores). Sus informes y su progreso llevan la etiqueta en español y no el
  ID: hay que agregar el ID a `OptimizationStepResult`, `PlannedOptimizationItem`, `DebloatItemOutcome` y los progresos, y poner los
  textos (ES y EN) en `ui/i18n/`.
- Mecánicas de a un archivo: `AR-06`/`CL-20` (al bajar `source: None` bajar el techo de `tests/indicadores.rs`), `TE-02`, `IN-02`,
  `OR-04`; y `CL-21`, `CL-25`, `AR-25`, `AR-27`, `AR-28`, `AR-29`, `TE-26`, `IN-09`, `IN-10`, `OR-07`, `CL-24`.
- Decisiones del autor: `AR-19`, `UX-02`, `UX-03`, `SEC-17` (con `SYS-24`).
- Con una VM o Windows Sandbox, como administrador: `tests/vm_sistema.rs`, instalar y quitar un driver de cada camino, quitar una app
  Appx, `ERR-24` (Moonlight `/quiet`, versión de GOG; se resuelve con la tienda por winget), CAB del Catálogo con firma Authenticode,
  y las descargas detrás de un proxy de máquina (`netsh winhttp set proxy`).
- Subir la rama y abrir el PR cuando el usuario lo pida; avisar al compañero de `SEC-30`…`SEC-32`, de que LibreWolf, Rufus e Intel
  salieron, y del cambio de paletas.

## Prompt para seguir

Pegar en una sesión nueva, en la carpeta del repo:

> Seguimos con OtterSwiss en la rama `auditoria/buenas-practicas` (commits locales, sin push). Lee `AGENTS.md`, `.agents/relevo.md`
> (sección «Estado actual»), `docs/BUGS.md` y `docs/PRACTICAS.md`, y `git status`. Verifica con `cargo fmt --check`,
> `cargo clippy --all-targets -- -D warnings` y `cargo test` en PowerShell. Sigue con lo pendiente, en este orden: la tienda con
> `winget download` y verificación propia (ya decidida: primero investiga y registra el diseño, y pregúntame lo que cambie el
> comportamiento), el resto de `UI-11` (catálogos de core con ID y textos en la UI) y las mecánicas de a un archivo. Sin tocar lo
> diferido (`SEC-17`, `SYS-24`, `UX-02`, `UX-03`) ni `AR-19`. Commitea en local por fase con el formato «Categoría: Verbo …», cerrando
> cada ID en `docs/*-detalle.md` con su línea **Cierre**. No hagas push ni PR sin que te lo pida. Español neutro, respuestas breves.

**VM:** VirtualBox no sirve en esta PC (el hipervisor de Windows está activo: VirtualBox cae a NEM, el EFI se cuelga en `DXE_AP` con 2
núcleos y Windows 11 exige 2). La VM se borró; queda la ISO verificada `D:\VMs\Win11_Enterprise_Eval_es-es.iso`
(SHA-256 oficial `8F344BEA…E4C584`) por decisión del usuario. Para un Windows completo: activar Hyper-V (pide reinicio).

**Windows Sandbox (si se reusa):** característica ya activada. Lanzar con `WindowsSandbox.exe <wsb>`. NUNCA matar sus procesos (deja VMs
huérfanas); se cierra con la X. La prueba eleva con `RunAs`: el usuario acepta el UAC dentro. El control de pantalla no ve esa ventana.

## Cómo verificar

El usuario **apagó Smart App Control** el 24/11: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build` y
`cargo fmt` corren en Windows (PowerShell; nunca desde Git Bash, donde `link` es el de coreutils). **Último resultado: 76
tests + 4 de integración y clippy en verde.** `.agents/scripts/check-wsl.sh` queda como alternativa desde WSL (solo compila).

Para recorrer la interfaz: la app pide administrador y Windows (UIPI) no deja controlar ventanas elevadas, así que se compila
una copia con `asInvoker` en `$env:TEMP\otter-ui-target` (cambiando `build.rs` solo para esa compilación y restaurándolo) y se
recorre sin elevación. Recorrido hecho: todas las pestañas en oscuro, Dashboard y Limpiador en claro e inglés, diálogos con
foco atrapado y Escape. Falta: acciones de sistema como admin (mejor en VM o Windows Sandbox) y ventana en tamaño chico.

## Decisiones del usuario (2026-11-25)

- `ERR-25`: buscar otra fuente de Intel; si no hay una oficial y legible por máquina, quitarla y dejar Intel a Windows Update. No se
  encontró ninguna (solo catálogos de terceros que son copias de un momento): se quita.
- `SEC-32`: la app elevada usa el proxy de la máquina (WinHTTP, HKLM) o conexión directa, nunca el de HKCU.
- Tienda (`SEC-35`, `ERR-24`): las apps de descarga directa pasan a `winget download` (última versión, SHA-256 del manifiesto) más la
  verificación propia (firmante del fabricante, Job con plazo). Se acaban los enlaces y versiones a mano.
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
