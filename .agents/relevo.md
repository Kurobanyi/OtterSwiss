# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-25

**Rama:** `auditoria/buenas-practicas`, unos 105 commits locales **sin push ni PR** (el usuario todavía no quiere subirla). Ya trae
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
- **Último resultado:** `cargo test` 70 + 2 de indicadores (más 2 ignorados de red y 7 de VM), `clippy --all-targets -- -D warnings`
  y `fmt --check` en verde. Drivers y Tienda recorridas sin elevar (oscuro y ventana chica; claro, inglés y maximizada; cancelar).

**Pendiente** (detalle en `docs/BUGS.md` y `docs/PRACTICAS.md`, sección «Siguiente» de cada uno):
- Decisiones del autor: `SEC-32` (WinInet usa el proxy del usuario; quitarlo rompe redes con proxy obligatorio), `SEC-35` (fijar
  SHA-256 en las apps con versión obliga a actualizarlos), `AR-17`/`AR-26` con `UI-11` (textos en core), `AR-19`, `IN-03`, `UX-02`,
  `UX-03`, `SEC-17` (con `SYS-24`), y si instalar `dioxus-cli` para `HE-11`.
- Sin decisión, en el código: `SYS-27` (JSON de Intel; la búsqueda real muestra «Respuesta inesperada del servicio de drivers»),
  `TST-02` (Intel), `CL-22` (tests de `strip_html_tags`, `url_encode`), resto de `AR-24` (tabla PCI compartida), `TE-29` (paletas
  nuevas), y las mejoras mecánicas grandes (`IN-02`, `TE-02`, `OR-04`, `AR-06`/`CL-20`). Al bajar un indicador, bajar su techo en
  `tests/indicadores.rs`.
- Con una VM o Windows Sandbox, como administrador: `tests/vm_sistema.rs`, instalar y quitar un driver de cada camino, quitar una app
  Appx, `ERR-24` (Moonlight `/quiet`, versión de GOG), confirmar que los CAB del Catálogo traen firma Authenticode.
- Subir la rama y abrir el PR cuando el usuario lo pida; avisar al compañero de `SEC-30`…`SEC-32` (v4.2.0 ya está etiquetada en
  `main`), y de que LibreWolf y Rufus salieron del catálogo.

## Prompt para seguir

Pegar en una sesión nueva, en la carpeta del repo:

> Seguimos con OtterSwiss en la rama `auditoria/buenas-practicas` (commits locales, sin push). Lee `AGENTS.md`, `.agents/relevo.md`
> (sección «Estado actual»), `docs/BUGS.md` y `docs/PRACTICAS.md`, y `git status`. Verifica con `cargo fmt --check`,
> `cargo clippy --all-targets -- -D warnings` y `cargo test` en PowerShell. Sigue con lo pendiente que no depende de mí (en este orden:
> `CL-22`, resto de `AR-24`, `TE-29`, luego las mecánicas de a un archivo: `AR-06`/`CL-20`, `TE-02`, `IN-02`, `OR-04`), sin tocar lo
> diferido (`SEC-17`, `SYS-24`, `UX-02`, `UX-03`, `UI-11`) ni las decisiones abiertas (`SEC-32`, `SEC-35`, `IN-03`). Commitea en local por fase con el formato «Categoría: Verbo …», cerrando
> cada ID en `docs/*-detalle.md` con su línea **Cierre**. No hagas push ni PR sin que te lo pida. Español neutro, respuestas breves.

**VM:** VirtualBox no sirve en esta PC (el hipervisor de Windows está activo: VirtualBox cae a NEM, el EFI se cuelga en `DXE_AP` con 2
núcleos y Windows 11 exige 2). La VM se borró; queda la ISO verificada `D:\VMs\Win11_Enterprise_Eval_es-es.iso`
(SHA-256 oficial `8F344BEA…E4C584`) por decisión del usuario. Para un Windows completo: activar Hyper-V (pide reinicio).

**Windows Sandbox (si se reusa):** característica ya activada. Lanzar con `WindowsSandbox.exe <wsb>`. NUNCA matar sus procesos (deja VMs
huérfanas); se cierra con la X. La prueba eleva con `RunAs`: el usuario acepta el UAC dentro. El control de pantalla no ve esa ventana.

## Cómo verificar

El usuario **apagó Smart App Control** el 24/11: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build` y
`cargo fmt` corren en Windows (PowerShell; nunca desde Git Bash, donde `link` es el de coreutils). **Último resultado: 70
tests + 2 de indicadores y clippy en verde.** `.agents/scripts/check-wsl.sh` queda como alternativa desde WSL (solo compila).

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
- `HE-11`: instalar dioxus-cli y usar `dx fmt` como paso opcional (sin CI).

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
