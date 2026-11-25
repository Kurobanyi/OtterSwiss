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

Avance al 25/11/2026: **75 hechas, 8 parciales y 4 pendientes** de 87. Las hechas salen de las
tablas de abajo; su cierre está en el detalle.

| Tipo | Total | P1 | P2 | P3 |
|---|---|---|---|---|
| Orden de archivos (`OR-`) | 5 | 1 | 3 | 1 |
| Arquitectura (`AR-`) | 20 | 5 | 13 | 2 |
| Código limpio (`CL-`) | 18 | 5 | 9 | 4 |
| Temas, tokens y CSS (`TE-`) | 24 | 2 | 13 | 9 |
| Idiomas (`IN-`) | 7 | 1 | 6 | 0 |
| Herramientas (`HE-`) | 13 | 3 | 5 | 5 |
| **Total** | **87** | **17** | **49** | **21** |

Punto de partida: `clippy` por defecto da 0 avisos, `fmt --check` está limpio, no hay `unwrap`, `expect` ni `panic!` fuera de los
tests, el grafo de módulos de core no tiene ciclos, las vistas no llaman a Win32 ni al registro, la UI no tiene `unsafe` y las 16
dependencias directas se usan.

## P1

| ID | Título | Esf. | Estado |
|---|---|---|---|
| [`TE-02`](PRACTICAS-detalle.md#te-02) | 185 estilos inline: el CSS no tiene clases de layout ni de componente | M/L | Parcial |
| [`CL-05`](PRACTICAS-detalle.md#cl-05) | Funciones de unas 400 líneas | M | Parcial |
| [`AR-02`](PRACTICAS-detalle.md#ar-02) | Dos pilas de respaldo de registro y tres almacenes JSON casi iguales | M | Parcial |

## P2

| ID | Título | Esf. | Estado |
|---|---|---|---|
| [`OR-04`](PRACTICAS-detalle.md#or-04) | El orden interno de los archivos no sigue una regla | M | Parcial |
| [`AR-06`](PRACTICAS-detalle.md#ar-06) | `CoreError` por módulo y no por causa; 161 `source: None` | L | Parcial |
| [`AR-17`](PRACTICAS-detalle.md#ar-17) | Textos de presentación generados en core | M | Pendiente |
| [`AR-18`](PRACTICAS-detalle.md#ar-18) | Tres versiones del crate `windows` y dependencias duplicadas | M | Parcial |
| [`CL-11`](PRACTICAS-detalle.md#cl-11) | Faltan `ProgressBar`, `CheckRow` y `CardHeader`; `rsx!` repetido | M | Parcial |
| [`IN-02`](PRACTICAS-detalle.md#in-02) | No hay catálogo de textos: 253 `if en {…} else {…}` | L | Parcial |
| [`IN-03`](PRACTICAS-detalle.md#in-03) | El estado guarda mensajes ya traducidos | M | Pendiente |

## P3

| ID | Título | Esf. | Estado |
|---|---|---|---|
| [`AR-19`](PRACTICAS-detalle.md#ar-19) | Tres estilos de API en core | M | Pendiente |
| [`HE-11`](PRACTICAS-detalle.md#he-11) | rustfmt no formatea `rsx!` | S | Pendiente |

## Siguiente

1. Unir `tmp/windows-061` (`AR-18`) a esta rama cuando el autor lo apruebe.
2. Con una VM desechable: correr `tests/vm_sistema.rs` sobre el motor y la búsqueda de restos ya partidos (`CL-05`).
3. De a un archivo y sin cambiar comportamiento: los `source: None` que quedan (`AR-06`), los estilos inline (`TE-02`), el
   catálogo de textos (`IN-02`) y el orden interno (`OR-04`).
4. Decisiones del autor: `AR-17` (con `UI-11`), `AR-19`, `IN-03` y si instalar `dioxus-cli` para `HE-11`.

## Lo que hay que conservar

- `win_common` con RAII sólidos (`SafeHandle`, `ScHandle`, `JobObject`, `NamedMutex`, `ComGuard`) y `safe_fs` como punto único de
  E/S verificada; `AlignedBuf` y `SHGetKnownFolderPath` en vez de variables de entorno.
- Catálogos embebidos con lista blanca y respaldos validados contra ellos; `atomic_write_new` y `read_bounded_bytes`.
- `thiserror` con `CoreResult` como base del rediseño de errores; `catch_unwind` que se convierte en `CoreError::Panic`.
- `Modal`, `ConfirmDialog`, `StatusBanner` y `Tone`: extender ese criterio, no reemplazarlo.
- Los seis temas definen los mismos 18 tokens, todos los `var(--…)` usados existen y todas las clases definidas se usan.
- Traducciones completas por construcción (el compilador exige ES y EN): cualquier catálogo nuevo debe conservar esa garantía.
- `build.rs` con `compile_for` y `test = false` en el binario; perfil release coherente; `Cargo.lock` versionado.
- Comentarios con el porqué y el ID del hallazgo; tests junto al código con nombres que describen la regla.
