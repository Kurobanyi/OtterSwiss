# Contexto y directrices del proyecto

Aplicación de escritorio para Windows en Rust con **Dioxus 0.7** (renderer desktop, WebView2). Corre siempre como
administrador (manifiesto `requireAdministrator` en `build.rs`).

## Arquitectura

- `src/main.rs`: solo despacha la línea de comandos (`core::cli::CliCommand`) y abre la ventana. `src/startup.rs`: diálogos
  nativos, comprobación de WebView2 y errores inesperados antes de Dioxus.
- `src/core/`: lógica, sin Dioxus ni nada de presentación. Capas, de abajo hacia arriba; una capa no importa de las de encima:
  - `error.rs`, `cli.rs`, `progress.rs`
  - `platform/`: primitivas de Windows (cadenas anchas, handles RAII, carpetas conocidas, archivos verificados, procesos,
    elevación, registro, servicios, JSON de respaldo).
  - `security/`: carpeta de datos solo-admin, ACL, cliente HTTPS, descargas verificadas, Authenticode.
  - funciones: `hardware/`, `optimize/`, `debloat/`, `uninstall/`, `archive/`, `store/`, `cleaner`, `dns`, `updater`,
    `context_menu`, `sys_tools`, `rollback`. `drivers.rs` está sin uso por decisión del autor (ARQ-03).
- `src/ui/`: `app.rs` (ventana), `installer.rs` (cola de instalación), `task.rs` (tareas largas con progreso), `document.rs`,
  `format.rs`, `theme.rs`, `prefs.rs`, `i18n/` (catálogo de textos), `components/` (un componente por archivo), `views/`.
- `assets/main.css`: tokens por tema y clases; `assets/brand/`: logos. `installer/otterswiss.iss`: instalador.

## Orden dentro de un archivo

`//!` con el propósito del módulo → `use` → constantes → tipos públicos → `impl` → funciones privadas → `#[cfg(test)]`. Sin
cabeceras que repitan la ruta. Un comentario explica la regla y cita el ID entre paréntesis (por ejemplo `(SYS-04)`).

## Comandos

- `cargo build` / `cargo build --release`. `cargo run` necesita una terminal elevada (sin elevar falla con el error 740).
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (todo esto lo corre también el CI).
- Recorrido de sistema como administrador, solo en una VM desechable:
  `cargo test --release --test vm_sistema -- --ignored --nocapture --test-threads=1`.
- Hook opcional de pre-commit: `git config core.hooksPath .githooks`.

## Registros

- `docs/BUGS.md` y `docs/BUGS-detalle.md`: defectos. `docs/PRACTICAS.md` y `docs/PRACTICAS-detalle.md`: buenas prácticas.
  Un hallazgo nuevo se registra antes de arreglarlo; al cerrarlo se anota el cierre con la evidencia.
- `.agents/relevo.md`: estado y siguiente paso entre sesiones.

## Configuración de agentes

- `.agents/`: reglas (`rules/`), skills, scripts de verificación, hook de parada y relevo; los lee cualquier asistente.
- `.claude/agents/`: subagentes de Claude Code (`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## Commits

Asunto `Categoría: Verbo …` en español, un tema por commit y cada árbol compilando. No se hace commit ni push sin pedido.

## Consulta obligatoria de documentación de versión

Dioxus cambió mucho entre versiones mayores (signals, hooks, estado, backend desktop). Consultar la documentación de la
versión exacta de `Cargo.toml` en vez de programar de memoria contra APIs anteriores.
