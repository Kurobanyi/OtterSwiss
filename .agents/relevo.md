# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-24

**Tarea en curso:** arreglos de la auditoría del 24/11 en la rama local **`fix/auditoria`** (sale de `main` en `11cb659`,
v3.5.1). **Código terminado y comprobado**: 83 de 90 hallazgos resueltos (incluye `UI-16`, hallado al recorrer la app),
2 parciales (`TST-01`, `UI-11`), 4 diferidos (`SEC-17`, `SYS-24`, `ARQ-03`, `UX-02`) y 1 pendiente de decisión (`UX-03`).
**Nada commiteado todavía.**

**Siguiente paso:** el usuario tiene que aprobar la partición en 15 commits que propuso `commit-writer` (se le mostró el
24/11; agregar `build.rs` y `Cargo.toml` `test = false` al commit que corresponda, `UI-16` va con Hardware). Con su visto
bueno: commitear en `fix/auditoria` comprobando que cada árbol compile, `cargo fmt` como commit `Estilo` aparte, push y **un**
PR en `Kurobanyi/OtterSwiss`. Trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; el cuerpo del PR termina
con `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

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
