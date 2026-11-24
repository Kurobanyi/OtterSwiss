# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-24

**Estado:** arreglos de la auditoría del 24/11 commiteados en **`fix/auditoria`** (15 commits sobre `11cb659`, cada árbol
compilado por separado) y en revisión en el PR https://github.com/Kurobanyi/OtterSwiss/pull/1. 83 de 90 hallazgos resueltos;
2 parciales (`TST-01`, `UI-11`), 4 diferidos (`SEC-17`, `SYS-24`, `ARQ-03`, `UX-02`) y 1 pendiente de decisión (`UX-03`).
El repo no tiene CI.

**Siguiente paso:** probar la app completa **como administrador en una máquina virtual** antes de fusionar: optimizar y
revertir, Debloater y revertir, aplicar y restaurar DNS, desinstalar y purgar, limpiar, extraer y comprimir, menú contextual,
y desinstalar OtterSwiss con `--rollback`. Lo más simple es Windows Sandbox (el usuario tiene que activar la característica,
pide admin y reinicio); dentro de la VM sí se puede hacer clic aunque la app esté elevada. Lo que falle se registra en
`docs/BUGS.md` antes de arreglarlo, en esta misma rama.

**Después de los bugs (pedido del usuario):** auditoría de buenas prácticas: arquitectura y orden del código, código limpio y
reutilizable, temas claro/oscuro y tokens bien hechos, i18n, linters y similares. Va en una tarea propia, no en este PR.

## Cómo verificar

El usuario **apagó Smart App Control** el 24/11: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build` y
`cargo fmt` corren en Windows (PowerShell; nunca desde Git Bash, donde `link` es el de coreutils). **Último resultado: 36
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
