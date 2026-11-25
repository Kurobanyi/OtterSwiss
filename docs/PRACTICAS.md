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

Avance al 25/11/2026: **75 hechas, 9 parciales y 30 pendientes** de 114. Las 27 últimas (`OR-06` en adelante) son del código de
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
| [`TE-02`](PRACTICAS-detalle.md#te-02) | 185 estilos inline: el CSS no tiene clases de layout ni de componente | M/L | Parcial |
| [`CL-05`](PRACTICAS-detalle.md#cl-05) | Funciones de unas 400 líneas | M | Parcial |
| [`AR-02`](PRACTICAS-detalle.md#ar-02) | Dos pilas de respaldo de registro y tres almacenes JSON casi iguales | M | Parcial |
| [`OR-06`](PRACTICAS-detalle.md#or-06) | `drivers.rs` tiene 1857 líneas y seis responsabilidades | M | Pendiente |
| [`AR-21`](PRACTICAS-detalle.md#ar-21) | `drivers.rs` reimplementa el cliente HTTPS | M | Pendiente |
| [`AR-22`](PRACTICAS-detalle.md#ar-22) | Descargas, caché y ZIP de drivers fuera de `StagedDownload`, `DownloadPolicy` y `ArchiveManager` | M | Pendiente |
| [`AR-23`](PRACTICAS-detalle.md#ar-23) | Procesos con `Command` directo y rutas de System32 a mano | S | Pendiente |
| [`AR-26`](PRACTICAS-detalle.md#ar-26) | Core arma textos y la UI los vuelve a leer como datos | M | Pendiente |
| [`CL-19`](PRACTICAS-detalle.md#cl-19) | 65 `unsafe` sin `// SAFETY:` tapados por un `#[allow]` con motivo obsoleto | S/M | Pendiente |
| [`CL-21`](PRACTICAS-detalle.md#cl-21) | Funciones largas y bloques copiados (`DriversView` de 825 líneas) | M | Pendiente |
| [`CL-22`](PRACTICAS-detalle.md#cl-22) | Lógica pura nueva sin tests | M | Pendiente |
| [`TE-25`](PRACTICAS-detalle.md#te-25) | 45 estilos inline nuevos; la tienda revierte clases utilitarias a inline | M | Pendiente |
| [`TE-27`](PRACTICAS-detalle.md#te-27) | Texto de error de la tienda con `--danger` (regresión de UI-18) | S | Pendiente |

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
| [`AR-24`](PRACTICAS-detalle.md#ar-24) | Guardianes y helpers Win32 duplicados | S | Pendiente |
| [`AR-25`](PRACTICAS-detalle.md#ar-25) | Tres comparadores de versiones con semánticas distintas | S | Pendiente |
| [`AR-27`](PRACTICAS-detalle.md#ar-27) | Booleanos y centinelas donde corresponde un enum | M | Pendiente |
| [`AR-28`](PRACTICAS-detalle.md#ar-28) | API pública de drivers más ancha de lo que usa la UI, y campos muertos | S | Pendiente |
| [`AR-29`](PRACTICAS-detalle.md#ar-29) | La vista de drivers repite el ciclo de tareas en vez de usar `ui::task::run_blocking` | S | Pendiente |
| [`CL-20`](PRACTICAS-detalle.md#cl-20) | 46 `source: None` nuevos en `drivers.rs` | S | Pendiente |
| [`CL-23`](PRACTICAS-detalle.md#cl-23) | Sin comentarios de documentación; la fusión de v4.2.0 borraba comentarios con ID | S | Parcial |
| [`CL-25`](PRACTICAS-detalle.md#cl-25) | Componentes y clases que ya existen no se usan | S | Pendiente |
| [`TE-26`](PRACTICAS-detalle.md#te-26) | `px` y colores derivados escritos en Rust | S | Pendiente |
| [`TE-28`](PRACTICAS-detalle.md#te-28) | Filtros marcados alternando `btn-primary` (regresión de TE-13) | S | Pendiente |
| [`TE-29`](PRACTICAS-detalle.md#te-29) | Temas nuevos: texto secundario igual al principal, peligro igual al acento y elevación invertida | S | Pendiente |
| [`IN-08`](PRACTICAS-detalle.md#in-08) | 43 `if en` nuevos fuera del catálogo y regresión de `strings(en)` en la tienda | M | Pendiente |
| [`IN-09`](PRACTICAS-detalle.md#in-09) | Textos en un solo idioma o mezclados | S | Pendiente |
| [`HE-14`](PRACTICAS-detalle.md#he-14) | Nada frena las regresiones de una fusión | S | Pendiente |

## P3

| ID | Título | Esf. | Estado |
|---|---|---|---|
| [`AR-19`](PRACTICAS-detalle.md#ar-19) | Tres estilos de API en core | M | Pendiente |
| [`HE-11`](PRACTICAS-detalle.md#he-11) | rustfmt no formatea `rsx!` | S | Pendiente |
| [`OR-07`](PRACTICAS-detalle.md#or-07) | Los iconos de la tienda viven dentro de `tools.rs` | S | Pendiente |
| [`CL-24`](PRACTICAS-detalle.md#cl-24) | `read_wmi_f32` copia `read_wmi_i32`; umbrales mágicos y filtro de sensor por nombre | S | Pendiente |
| [`IN-10`](PRACTICAS-detalle.md#in-10) | «...», «(s)», mayúsculas de título y «drivers/controladores» | S | Pendiente |

## Siguiente

1. Código de v4.2.0, junto con los defectos de `docs/BUGS.md`: `OR-06` con `AR-21`…`AR-23` (cliente HTTP, descargas y procesos por
   las primitivas de `security/` y `platform/`, que cierra de paso `SEC-30`, `SEC-33`, `SEC-34`, `ERR-19`, `ERR-22`), `CL-19` y `CL-22`.
2. Regresiones de la fusión: `TE-25`, `TE-27`, `TE-28`, `IN-08`, y el freno `HE-14`.
3. Con una VM desechable: correr `tests/vm_sistema.rs` sobre el motor y la búsqueda de restos ya partidos (`CL-05`).
4. De a un archivo y sin cambiar comportamiento: `AR-06`/`CL-20`, `TE-02`, `IN-02` y `OR-04`.
5. Decisiones del autor: `AR-17` (con `UI-11` y `AR-26`), `AR-19`, `IN-03` y si instalar `dioxus-cli` para `HE-11`.

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
