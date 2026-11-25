# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-24

**Estado:** `fix/auditoria` fusionado en `main` (`adf794d`; PR #1 cerrado como fusionado). 85 de 96 hallazgos resueltos. Abiertos:
`TST-01` y `UI-11` parciales, y los cuatro defectos nuevos de la auditoría de buenas prácticas (`ERR-14`, `SEC-29`, `UI-17`,
`UI-18`). Diferidos `SEC-17`, `SYS-24`, `ARQ-03`, `UX-02`; `UX-03` pendiente de decisión. El repo no tiene CI.

**Auditoría de buenas prácticas (24/11, rama `auditoria/buenas-practicas`, sin commitear):** cuatro pasadas en paralelo
(arquitectura; código limpio; temas, tokens y CSS; i18n, linters y herramientas). 87 mejoras en `docs/PRACTICAS.md` (índice, orden
de ataque) y `docs/PRACTICAS-detalle.md` (expedientes): 17 P1, 49 P2, 21 P3. Los defectos van en `docs/BUGS.md`. También se
devolvieron a su expediente siete líneas **Cierre** de `BUGS-detalle.md` que habían quedado bajo el título de la sección siguiente.

**Avance 25/11 (rama `auditoria/buenas-practicas`, commits locales sin push):** buenas prácticas 75 de 87 hechas, 8
parciales y 4 pendientes (ver `docs/PRACTICAS.md`, sección «Siguiente»). Core por capas sin dependencias hacia arriba, UI
ordenada, catálogo de textos, edición 2024, CI y lints, SAFETY en todo bloque `unsafe` (salvo `drivers.rs`, diferido). 57 tests
y clippy en verde; recorrido visual de Inicio, Hardware, Herramientas, Desinstalador, temas y mini ventana. Sin probar como
administrador desde que se partieron el motor y la búsqueda de restos (la salida de restos y de discos se comparó igual en esta PC).
`windows` 0.61 (`AR-18`) está en la rama aparte `tmp/windows-061` (`db8c18c`), en verde y sin unir.
**Fusión de v4.2.0 (25/11, `4767abf`):** el compañero publicó v4.2.0 en `main` (`0a0f5ec`: pestaña Drivers, 50 apps con
restaurador UWP por winget, 4 temas, puente de LibreHardwareMonitor, dependencias nuevas). Se fusionó sobre esta rama portando sus
cambios a la arquitectura por capas (tienda reconstruida desde su catálogo; `drivers.rs` tal cual, con rutas nuevas; temas completados
y ajustados a AA). 57 tests y clippy en verde. Su código se auditó en tres pasadas: 28 defectos (`SEC-30`…`TST-02` en
`docs/BUGS.md`, tres de seguridad alta: ZIP de Intel sin verificar, winget desde LocalAppData, proxy y raíces de HKCU) y 27 mejoras
(`OR-06`…`HE-14` en `docs/PRACTICAS.md`). Nada de eso está arreglado todavía.
**Siguiente:** arreglar `SEC-30`, `SEC-31`, `SEC-32` y la tienda que no instala (`ERR-15`…`ERR-17`) antes de publicar; luego el orden de «Siguiente» en `docs/BUGS.md`.

**VM:** VirtualBox no sirve en esta PC (el hipervisor de Windows está activo: VirtualBox cae a NEM, el EFI se cuelga en `DXE_AP` con 2
núcleos y Windows 11 exige 2). La VM se borró; queda la ISO verificada `D:\VMs\Win11_Enterprise_Eval_es-es.iso`
(SHA-256 oficial `8F344BEA…E4C584`) por decisión del usuario. Para un Windows completo: activar Hyper-V (pide reinicio).

**Windows Sandbox (si se reusa):** característica ya activada. Lanzar con `WindowsSandbox.exe <wsb>`. NUNCA matar sus procesos (deja VMs
huérfanas); se cierra con la X. La prueba eleva con `RunAs`: el usuario acepta el UAC dentro. El control de pantalla no ve esa ventana.

## Cómo verificar

El usuario **apagó Smart App Control** el 24/11: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build` y
`cargo fmt` corren en Windows (PowerShell; nunca desde Git Bash, donde `link` es el de coreutils). **Último resultado: 57
tests y clippy en verde.** `.agents/scripts/check-wsl.sh` queda como alternativa desde WSL (solo compila).

Para recorrer la interfaz: la app pide administrador y Windows (UIPI) no deja controlar ventanas elevadas, así que se compila
una copia con `asInvoker` en `$env:TEMP\otter-ui-target` (cambiando `build.rs` solo para esa compilación y restaurándolo) y se
recorre sin elevación. Recorrido hecho: todas las pestañas en oscuro, Dashboard y Limpiador en claro e inglés, diálogos con
foco atrapado y Escape. Falta: acciones de sistema como admin (mejor en VM o Windows Sandbox) y ventana en tamaño chico.

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
