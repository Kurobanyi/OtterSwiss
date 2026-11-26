# Registro de buenas prácticas

Índice de las mejoras de arquitectura, código, estilos, idiomas y herramientas que **no están hechas**. Evidencia y propuesta de
cada una viven en `docs/PRACTICAS-detalle.md`, con ancla `#id` en minúscula. Origen: auditoría de buenas prácticas del 24/11/2026
sobre `main` (`adf794d`). Los defectos que encontró (no mejoras) están en `docs/BUGS.md`: `SEC-29`, `ERR-14`, `UI-17` y `UI-18`.

## Cómo leer

- **IDs**: `OR-` orden de archivos y carpetas · `AR-` arquitectura · `CL-` código limpio y reutilizable · `TE-` temas, tokens y CSS · `IN-` idiomas ·
  `HE-` herramientas, linters y repositorio. Correlativos por prefijo, sin reutilizar.
- **Prioridad**: P1 antes de que crezca el código o de la próxima release · P2 siguiente versión · P3 pulido.
  **Esfuerzo**: S horas · M un día o dos · L varios días, por etapas.
- **Estado**: Pendiente · En curso · Hecho · Descartado. Al hacerla se anota el cierre en el detalle y sale de este índice.
- Cambian estructura, no comportamiento: cada una se hace en su propio commit, con `cargo test` y `clippy -D warnings` en verde
  antes y después.

## Resumen

Avance al 25/11/2026: **105 hechas, 7 parciales, 1 pendiente y 1 descartada** de 114. Las 27 últimas (`OR-06` en adelante) son del código de
v4.2.0, auditado tras fusionarlo (sección final del detalle). Las hechas salen de las tablas de abajo; su cierre está en el detalle.

| Tipo | Total | P1 | P2 | P3 |
|---|---|---|---|---|
| Orden de archivos (`OR-`) | 7 | 2 | 3 | 2 |
| Arquitectura (`AR-`) | 29 | 9 | 18 | 2 |
| Código limpio (`CL-`) | 25 | 8 | 12 | 5 |
| Temas, tokens y CSS (`TE-`) | 29 | 4 | 16 | 9 |
| Idiomas (`IN-`) | 10 | 1 | 8 | 1 |
| Herramientas (`HE-`) | 14 | 3 | 6 | 5 |
| **Total** | **114** | **27** | **63** | **24** |

Punto de partida: `clippy` por defecto da 0 avisos, `fmt --check` está limpio, no hay `unwrap`, `expect` ni `panic!` fuera de los
tests, el grafo de módulos de core no tiene ciclos, las vistas no llaman a Win32 ni al registro, la UI no tiene `unsafe` y las 16
dependencias directas se usan.

## P1

| ID | Título | Esf. | Estado |
|---|---|---|---|
| [`CL-05`](PRACTICAS-detalle.md#cl-05) | Funciones de unas 400 líneas | M | Parcial |
| [`AR-02`](PRACTICAS-detalle.md#ar-02) | Dos pilas de respaldo de registro y tres almacenes JSON casi iguales | M | Parcial |
| [`AR-26`](PRACTICAS-detalle.md#ar-26) | Core arma textos y la UI los vuelve a leer como datos | M | Parcial |

## P2

| ID | Título | Esf. | Estado |
|---|---|---|---|
| [`AR-06`](PRACTICAS-detalle.md#ar-06) | `CoreError` por módulo y no por causa; 161 `source: None` | L | Parcial |
| [`AR-17`](PRACTICAS-detalle.md#ar-17) | Textos de presentación generados en core | M | Parcial |
| [`AR-18`](PRACTICAS-detalle.md#ar-18) | Tres versiones del crate `windows` y dependencias duplicadas | M | Parcial |
| [`IN-02`](PRACTICAS-detalle.md#in-02) | No hay catálogo de textos: 253 `if en {…} else {…}` | L | Hecha |

## P3

| ID | Título | Esf. | Estado |
|---|---|---|---|
| [`AR-19`](PRACTICAS-detalle.md#ar-19) | Tres estilos de API en core | M | Pendiente |
| [`HE-11`](PRACTICAS-detalle.md#he-11) | rustfmt no formatea `rsx!` | S | Descartada |

## Siguiente

1. Con una VM desechable: correr `tests/vm_sistema.rs` sobre el motor y la búsqueda de restos ya partidos (`CL-05`).
2. `CL-05`: partir `UninstallerView`, `DashboardView`, `DebloaterView`, `DnsView`, `App` y `ToolsView`. `AR-06`: variantes por causa
   (`NotFound`, `AccessDenied`, `ExitCode`) para los 95 `source: None` que quedan. `IN-02`: los diálogos de `main.rs`/`startup.rs`
   y `AppCategory::label`. `AR-02`: el snapshot del Debloater sobre `registry`. `AR-17`/`AR-26`: detalles con datos del motor.
3. Recorrer en la app, en español e inglés y en los dos temas, lo hecho el 25/11 (`CL-21`, `CL-25`, `TE-02`, `IN-02`, `CL-11`).
   Decisión del autor: `AR-19`.

## Lo que hay que conservar

- `win_common` con RAII sólidos (`SafeHandle`, `ScHandle`, `JobObject`, `NamedMutex`, `ComGuard`) y `safe_fs` como punto único de
  E/S verificada; `AlignedBuf` y `SHGetKnownFolderPath` en vez de variables de entorno.
- Catálogos embebidos con lista blanca y respaldos validados contra ellos; `atomic_write_new` y `read_bounded_bytes`.
- `thiserror` con `CoreResult` como base del rediseño de errores; `catch_unwind` que se convierte en `CoreError::Panic`.
- `Modal`, `ConfirmDialog`, `StatusBanner` y `Tone`: extender ese criterio, no reemplazarlo.
- Los diez temas definen los mismos tokens y `tests/temas.rs` les exige AA (TE-29), todos los `var(--…)` usados existen y todas las clases definidas se usan.
- Traducciones completas por construcción (el compilador exige ES y EN): cualquier catálogo nuevo debe conservar esa garantía.
- `build.rs` con `compile_for` y `test = false` en el binario; perfil release coherente; `Cargo.lock` versionado.
- Comentarios con el porqué y el ID del hallazgo; tests junto al código con nombres que describen la regla.
