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

| ID | Título | Esf. |
|---|---|---|
| [`OR-01`](PRACTICAS-detalle.md#or-01) | `src/core` plano de 26 archivos: pasa a carpetas por capa | L |
| [`HE-01`](PRACTICAS-detalle.md#he-01) | No hay CI (y definir `RUSTFLAGS` en él anularía el CRT estático) | M |
| [`HE-02`](PRACTICAS-detalle.md#he-02) | Sin `.gitattributes`: los finales de línea dependen de cada máquina | S |
| [`HE-03`](PRACTICAS-detalle.md#he-03) | La versión se escribe a mano en seis sitios y el updater depende de que coincidan | M |
| [`IN-01`](PRACTICAS-detalle.md#in-01) | El idioma por defecto es español fijo y hay dos fuentes de idioma | S |
| [`TE-01`](PRACTICAS-detalle.md#te-01) | Sin `color-scheme` ni `accent-color`: scrollbars y checkboxes claros en temas oscuros | S |
| [`TE-02`](PRACTICAS-detalle.md#te-02) | 185 estilos inline: el CSS no tiene clases de layout ni de componente | M/L |
| [`CL-01`](PRACTICAS-detalle.md#cl-01) | 213 bloques `unsafe` sin ningún comentario `SAFETY` | M |
| [`CL-02`](PRACTICAS-detalle.md#cl-02) | Wrappers RAII duplicados y liberaciones a mano | S/M |
| [`CL-03`](PRACTICAS-detalle.md#cl-03) | Cliente HTTP de WinInet duplicado y user-agent desactualizado | M |
| [`CL-04`](PRACTICAS-detalle.md#cl-04) | Ubicación de registro de un resto modelada con dos booleanos | M |
| [`CL-05`](PRACTICAS-detalle.md#cl-05) | Funciones de unas 400 líneas | M |
| [`AR-01`](PRACTICAS-detalle.md#ar-01) | La vista del desinstalador decide el éxito y orquesta el flujo | M |
| [`AR-02`](PRACTICAS-detalle.md#ar-02) | Dos pilas de respaldo de registro y tres almacenes JSON casi iguales | M |
| [`AR-03`](PRACTICAS-detalle.md#ar-03) | Todo es `pub`: atajos que saltan validaciones y código muerto invisible | M |
| [`AR-04`](PRACTICAS-detalle.md#ar-04) | Detección de hardware escrita tres veces | M |
| [`AR-13`](PRACTICAS-detalle.md#ar-13) | «Tarea bloqueante con progreso» copiada seis veces; `JoinError` de cinco maneras | M |

## P2

| ID | Título | Esf. |
|---|---|---|
| [`OR-02`](PRACTICAS-detalle.md#or-02) | Todos los componentes en un solo `components/mod.rs` | S |
| [`OR-03`](PRACTICAS-detalle.md#or-03) | Nombres y lugares fuera de convención (`hardware_tab`, mini ventana en `main.rs`, `.iss` en la raíz) | S |
| [`OR-04`](PRACTICAS-detalle.md#or-04) | El orden interno de los archivos no sigue una regla | M |
| [`AR-05`](PRACTICAS-detalle.md#ar-05) | Código muerto que el compilador no puede señalar | S |
| [`AR-06`](PRACTICAS-detalle.md#ar-06) | `CoreError` por módulo y no por causa; 161 `source: None` | L |
| [`AR-07`](PRACTICAS-detalle.md#ar-07) | La carpeta de datos la resuelve la UI y sus nombres están repartidos | S/M |
| [`AR-08`](PRACTICAS-detalle.md#ar-08) | `revert_all_changes` vive en el desinstalador | S |
| [`AR-09`](PRACTICAS-detalle.md#ar-09) | «Descargar, verificar y ejecutar un instalador» escrito dos veces | M |
| [`AR-10`](PRACTICAS-detalle.md#ar-10) | Cinco formas de lanzar procesos | M |
| [`AR-11`](PRACTICAS-detalle.md#ar-11) | Módulos de más de 900 líneas (particiones propuestas) | L |
| [`AR-12`](PRACTICAS-detalle.md#ar-12) | `main.rs` hace demasiado y los flags de la CLI son literales sueltos | M |
| [`AR-14`](PRACTICAS-detalle.md#ar-14) | `ui/mod.rs` mezcla shell, estado global y worker de instalación | S/M |
| [`AR-15`](PRACTICAS-detalle.md#ar-15) | Reglas de negocio en las vistas | S |
| [`AR-16`](PRACTICAS-detalle.md#ar-16) | Números mágicos repetidos entre UI y core | S |
| [`AR-17`](PRACTICAS-detalle.md#ar-17) | Textos de presentación generados en core | M |
| [`AR-18`](PRACTICAS-detalle.md#ar-18) | Tres versiones del crate `windows` y dependencias duplicadas | M |
| [`CL-06`](PRACTICAS-detalle.md#cl-06) | Helpers Win32 escritos varias veces (`known_folder` cuatro) | S |
| [`CL-07`](PRACTICAS-detalle.md#cl-07) | Reporte de progreso por pasos copiado en core | S |
| [`CL-08`](PRACTICAS-detalle.md#cl-08) | Apertura del SCM y del servicio duplicada | S |
| [`CL-09`](PRACTICAS-detalle.md#cl-09) | Consulta WMI duplicada | S |
| [`CL-10`](PRACTICAS-detalle.md#cl-10) | Cuatro modelos de «mensaje de estado» en la UI | M |
| [`CL-11`](PRACTICAS-detalle.md#cl-11) | Faltan `ProgressBar`, `CheckRow` y `CardHeader`; `rsx!` repetido | M |
| [`CL-12`](PRACTICAS-detalle.md#cl-12) | Formato de tamaños y duraciones en tres versiones | S |
| [`CL-13`](PRACTICAS-detalle.md#cl-13) | Strings mágicos donde va un enum | S/M |
| [`CL-14`](PRACTICAS-detalle.md#cl-14) | Dos `too_many_arguments` que tapan un diseño mejorable | S |
| [`TE-03`](PRACTICAS-detalle.md#te-03) | Sin opción «según el sistema»; el tema se elige en un ciclo de seis | M |
| [`TE-04`](PRACTICAS-detalle.md#te-04) | Los tokens viven en un `div` y `:root` no tiene colores | S |
| [`TE-05`](PRACTICAS-detalle.md#te-05) | Un solo nivel de tokens y `--bg` copiado a mano en Rust | S/M |
| [`TE-06`](PRACTICAS-detalle.md#te-06) | Estados de interacción casi invisibles | S |
| [`TE-07`](PRACTICAS-detalle.md#te-07) | Fondos blancos translúcidos fijos que desaparecen en Claro | S |
| [`TE-08`](PRACTICAS-detalle.md#te-08) | Bordes de controles por debajo de 3:1 | S |
| [`TE-09`](PRACTICAS-detalle.md#te-09) | Sin `prefers-reduced-motion` y logo animado en bucle | S |
| [`TE-10`](PRACTICAS-detalle.md#te-10) | Logo de 1,7 MB en base64 para un icono de 30×30 | S |
| [`TE-11`](PRACTICAS-detalle.md#te-11) | Sin reset base ni `box-sizing` | S |
| [`TE-12`](PRACTICAS-detalle.md#te-12) | Componentes que se esquivan: `Card`, `StatusBanner`, `badge` | M |
| [`TE-13`](PRACTICAS-detalle.md#te-13) | Controles segmentados y pestañas sin estado accesible | S/M |
| [`TE-14`](PRACTICAS-detalle.md#te-14) | Objetivos táctiles chicos y tipografía fuera de la escala | S |
| [`TE-15`](PRACTICAS-detalle.md#te-15) | La barra superior probablemente no entra a 960px (por verificar) | S/M |
| [`IN-02`](PRACTICAS-detalle.md#in-02) | No hay catálogo de textos: 253 `if en {…} else {…}` | L |
| [`IN-03`](PRACTICAS-detalle.md#in-03) | El estado guarda mensajes ya traducidos | M |
| [`IN-04`](PRACTICAS-detalle.md#in-04) | Plurales con «(s)» | S |
| [`IN-05`](PRACTICAS-detalle.md#in-05) | Títulos de ventana y diálogo de error solo en español | S |
| [`IN-06`](PRACTICAS-detalle.md#in-06) | Calidad de traducción y falta de guía de estilo | S |
| [`IN-07`](PRACTICAS-detalle.md#in-07) | El documento no declara su idioma | S |
| [`HE-04`](PRACTICAS-detalle.md#he-04) | No hay `[lints]` ni `clippy.toml` (set propuesto y medido) | S |
| [`HE-05`](PRACTICAS-detalle.md#he-05) | Sin `cargo-deny` ni `cargo-audit` | S |
| [`HE-06`](PRACTICAS-detalle.md#he-06) | El MSRV 1.93 está declarado pero no se prueba | S |
| [`HE-07`](PRACTICAS-detalle.md#he-07) | README truncado y documentación desactualizada | S |
| [`HE-08`](PRACTICAS-detalle.md#he-08) | `.claude/settings.local.json` solo se ignora por la configuración de esta máquina | S |

## P3

| ID | Título | Esf. |
|---|---|---|
| [`OR-05`](PRACTICAS-detalle.md#or-05) | Dos carpetas de configuración de agentes sin explicar | S |
| [`AR-19`](PRACTICAS-detalle.md#ar-19) | Tres estilos de API en core | M |
| [`AR-20`](PRACTICAS-detalle.md#ar-20) | Persistencia de tema e idioma duplicada | S |
| [`CL-15`](PRACTICAS-detalle.md#cl-15) | Rutas tratadas como `String` con pérdida | S |
| [`CL-16`](PRACTICAS-detalle.md#cl-16) | Detalles sueltos | S |
| [`CL-17`](PRACTICAS-detalle.md#cl-17) | Comentarios que cuentan historia en vez de la regla | S |
| [`CL-18`](PRACTICAS-detalle.md#cl-18) | Tests: temporales sin limpieza y test de sistema monolítico | S/M |
| [`TE-16`](PRACTICAS-detalle.md#te-16) | `!important` y `pointer-events` sin motivo | S |
| [`TE-17`](PRACTICAS-detalle.md#te-17) | Valores sueltos sin token | S |
| [`TE-18`](PRACTICAS-detalle.md#te-18) | `transition: all` | S |
| [`TE-19`](PRACTICAS-detalle.md#te-19) | Fuentes y glifos de ventana que no son los de Windows | S |
| [`TE-20`](PRACTICAS-detalle.md#te-20) | `user-select: none` global impide copiar rutas y errores | S |
| [`TE-21`](PRACTICAS-detalle.md#te-21) | Nord: `--text-muted` casi igual a `--text` | S |
| [`TE-22`](PRACTICAS-detalle.md#te-22) | CSS inyectado dos veces y mini ventana sin color de fondo inicial | S |
| [`TE-23`](PRACTICAS-detalle.md#te-23) | `main.css` sin secciones | S |
| [`TE-24`](PRACTICAS-detalle.md#te-24) | Deshabilitado solo con opacidad | S |
| [`HE-09`](PRACTICAS-detalle.md#he-09) | Migrar a la edición 2024 | M |
| [`HE-10`](PRACTICAS-detalle.md#he-10) | Sin hook de pre-commit | S |
| [`HE-11`](PRACTICAS-detalle.md#he-11) | rustfmt no formatea `rsx!` | S |
| [`HE-12`](PRACTICAS-detalle.md#he-12) | Metadatos de `Cargo.toml` y features de `windows` que podrían sobrar | S |
| [`HE-13`](PRACTICAS-detalle.md#he-13) | `.editorconfig` y herramientas de texto | S |

## Orden de ataque sugerido

1. **Barandas primero** (S, sin tocar lógica): los defectos `ERR-14`, `SEC-29`, `UI-17`, `UI-18`; luego `HE-02`, `HE-04`, `HE-08`,
   `CL-01` (solo el `forbid(unsafe_code)` en la UI) y `HE-01`. Desde ahí, cada refactor queda protegido por el CI.
2. **CSS base** (S): `TE-04`, `TE-01`, `TE-11`, `TE-09`, `TE-10`, `IN-01`, `IN-07`. Arreglos visibles para el usuario y baratos.
3. **Helpers compartidos en core** (S/M): `CL-02`, `CL-06`, `CL-03`, `AR-08`, `AR-05`, `AR-09`, `AR-10`. Quitan duplicación de los
   caminos sensibles (patrones 2, 3 y 5 de `docs/BUGS.md`).
4. **Modelo de datos**: `AR-02`, `CL-04`, `AR-01`, `AR-07`, `AR-06` (gradual).
5. **UI**: `AR-13`, `CL-10`, `CL-11`, `TE-02` y `TE-12` juntos, vista por vista; después `IN-02` e `IN-03`.
6. **Ordenar y partir** (`OR-01` a `OR-04`, `AR-11`, `CL-05`, `AR-12`), ya con lo anterior hecho, de a uno y sin cambiar comportamiento.

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
