# Registro de hallazgos

Índice corto de lo que **no está resuelto**. Causa, evidencia y cierre de cada hallazgo (también los resueltos) viven en
`docs/BUGS-detalle.md`, con ancla `#id` en minúscula. Origen: auditoría estática completa del 24/11/2026 (v3.5.1, `11cb659`),
sin compilar ni ejecutar. Agentes para repetirla: `.claude/agents/security-auditor.md`, `code-auditor.md`, `design-challenger.md`.

## Cómo leer

- **IDs**: `B-` bug · `M-` mejora · `SEC-` seguridad · `SYS-` cambio al sistema y rollback · `ERR-` manejo de errores y procesos ·
  `CON-` concurrencia · `ARQ-` arquitectura · `UI-` interfaz · `UX-` diseño y producto · `TST-` pruebas. Correlativos por prefijo,
  sin reutilizar.
- **Severidad** (bugs): Crítica · Alta · Media · Baja. **Prioridad** (mejoras): P1 antes de la próxima release · P2 siguiente versión · P3 pulido.
- **Estado**: Pendiente · Parcial · Pendiente de decisión · Aceptado · Diferido · Resuelto. Sin evidencia no se cierra.
- Un hallazgo nuevo se documenta en el detalle antes de arreglarlo. Al cerrarlo se actualiza su estado allí y los conteos de abajo;
  deja de aparecer en este índice.

## Resumen

Estado al 24/11/2026, rama `fix/auditoria`: `cargo test` (37 tests) y `clippy --all-targets -- -D warnings` en verde en Windows, e
interfaz recorrida sin elevación en tema oscuro y claro, en español e inglés. **Acciones que cambian el sistema probadas como
administrador** en Windows Sandbox con `tests/vm_sistema.rs` (`cargo test --release --test vm_sistema -- --ignored --nocapture
--test-threads=1`, solo en una máquina desechable): optimizar y revertir, Debloater (registro) y revertir, DNS IPv4/IPv6, limpiador con
junction trampa, comprimir y extraer, desinstalar y purgar una app falsa, revertir todo y `--rollback`: 0 fallos. Sin probar: quitar una
app Appx (el Sandbox no trae ninguna del catálogo) y la ventana en tamaño chico.

| Tipo | Total | Abiertos o parciales | Diferidos | Pendientes de decisión | Resueltos |
|---|---|---|---|---|---|
| Seguridad (`SEC-`) | 29 | 1 | 1 | 0 | 27 |
| Sistema y rollback (`SYS-`) | 24 | 0 | 1 | 0 | 23 |
| Errores y procesos (`ERR-`) | 14 | 1 | 0 | 0 | 13 |
| Concurrencia (`CON-`) | 2 | 0 | 0 | 0 | 2 |
| Arquitectura (`ARQ-`) | 4 | 0 | 1 | 0 | 3 |
| Interfaz (`UI-`) | 18 | 3 | 0 | 0 | 15 |
| Diseño y producto (`UX-`) | 4 | 0 | 1 | 1 | 2 |
| Pruebas (`TST-`) | 1 | 1 | 0 | 0 | 0 |
| **Total** | **96** | **6** | **4** | **1** | **85** |

Las mejoras de arquitectura, código limpio, temas, i18n y herramientas (auditoría de buenas prácticas del 24/11) viven en
`docs/PRACTICAS.md`; aquí solo quedan los defectos que esa auditoría encontró.

## Abiertos

| ID | Título | Afecta | Sev. | Estado |
|---|---|---|---|---|
| [`TST-01`](BUGS-detalle.md#tst-01) | El repositorio no tiene ni un test | Desarrollador | Alta | Parcial |
| [`ERR-14`](BUGS-detalle.md#err-14) | `build.rs` ignora el fallo al incrustar el manifiesto y el `.exe` puede salir sin `requireAdministrator` | Usuario, Desarrollador | Media | Pendiente |
| [`UI-17`](BUGS-detalle.md#ui-17) | `LoadingState` no muestra el spinner y `ErrorState` se ve igual que «no hay datos» | Usuario | Media | Pendiente |
| [`UI-18`](BUGS-detalle.md#ui-18) | Texto en colores de estado y acento por debajo de 4,5:1 | Usuario | Media | Pendiente |
| [`UI-11`](BUGS-detalle.md#ui-11) | Textos de core sin traducir y mezcla de idiomas | Usuario | Media | Parcial |
| [`SEC-29`](BUGS-detalle.md#sec-29) | La lista blanca de relanzamiento elevado acepta `--apply-profile`, que ningún código maneja | Desarrollador, Sistema | Baja | Pendiente |

## Diferidos y pendientes de decisión

Conservan su expediente; se retoman por decisión del autor.

| ID | Título | Afecta | Sev. | Estado |
|---|---|---|---|---|
| [`SYS-24`](BUGS-detalle.md#sys-24) | Con elevación «sobre el hombro», los tweaks HKCU van al perfil del admin | Usuario | Baja | Diferido |
| [`ARQ-03`](BUGS-detalle.md#arq-03) | `drivers.rs` es código muerto | Desarrollador | Baja | Diferido |
| [`SEC-17`](BUGS-detalle.md#sec-17) | Todo el programa corre como administrador aunque la mitad de las funciones no lo necesitan | Sistema, Usuario | Media | Diferido |
| [`UX-02`](BUGS-detalle.md#ux-02) | «Optimizar Sistema» aplica sin confirmar tweaks sin base o mal descritos | Usuario, Sistema | Media | Diferido |
| [`UX-03`](BUGS-detalle.md#ux-03) | «Optimizar RAM» es una práctica desacreditada | Usuario | Media | Pendiente de decisión |

## Siguiente

1. `ERR-14`, `SEC-29`, `UI-17` y `UI-18`: arreglos chicos que salieron de la auditoría de buenas prácticas.
2. En un Windows completo: quitar una app Appx con el Debloater y ver la ventana en tamaño chico.
3. `TST-01`: tests de `SYS-06`, `SYS-07`, `SYS-10` y `SYS-12`, que necesitan registro y archivos de respaldo.
4. Decisiones del autor: `UX-03`, `UX-02`, `SEC-17` (y con él `SYS-24`), `UI-11` en el núcleo.

## Patrones

Familias de falla que ya ocurrieron en este código, con sus IDs como evidencia. Léelos antes de escribir código y, al cerrar un
hallazgo, busca las demás instancias del mismo patrón.

| # | Patrón | Evidencia | Qué comprobar |
|---|---|---|---|
| 1 | Efecto irreversible antes de la validación | `SEC-01`, `SEC-25`, `SYS-17` | Que ningún handle se abra con `DELETE_ON_CLOSE`, `CREATE_ALWAYS` o `TRUNCATE`, ni se cambie un estado del sistema, antes de validar; la validación va sobre el handle y el efecto después. |
| 2 | Una ruta de descarga o ejecución que esquiva `trust.rs`, o que verifica y luego reabre por ruta | `SEC-02`, `SEC-05`, `SEC-14` | Que todo binario descargado o ejecutado pase por `secure_download_and_verify` y se ejecute desde el mismo handle verificado. Buscar `File::create`, `InternetOpenUrlW`, `Command::new` y `CreateProcessW` fuera de `trust.rs`. |
| 3 | Proceso hijo con pipes que no se leen mientras corre, o del que solo se espera al padre | `ERR-01`, `SYS-05`, `ERR-11` | Que cada `Stdio::piped()` tenga un lector concurrente o use `output()`; que la espera sea por el Job completo. |
| 4 | Un JSON de `ProgramData` que se aplica como admin sin validar su contenido ni su dueño | `SEC-03`, `SEC-09`, `SEC-10`, `SEC-04` | Que cada entrada leída de un respaldo se compare con el catálogo embebido antes de escribir registro, DNS o servicios, y que el archivo sea de Administradores o SYSTEM. |
| 5 | Rutas privilegiadas tomadas de variables de entorno, HKCU o comparación léxica | `SEC-06`, `SEC-07`, `SEC-11`, `SEC-19`, `SEC-20` | Usar `SHGetKnownFolderPath`, canonizar ambos lados antes de comparar, rechazar reparse points en cada componente y no fiarse de HKCU para decidir qué tocar fuera del perfil. |
| 6 | Un error tragado que acaba sobrescribiendo o borrando el estado original | `SYS-06`, `SYS-09`, `SYS-10`, `SYS-11` | Buscar `let _ =`, `.ok()`, `if let Ok` y `unwrap_or_default()` en respaldo y rollback: si falla leer el original, se aborta, nunca se asume «vacío». |
| 7 | La UI anuncia éxito sin mirar el resultado real | `UI-01`, `UI-02`, `ERR-07`, `ERR-10`, `SYS-02`, `SYS-16` | Que cada banner y contador salga de los campos de fallo del informe (`failed_items`, `error`, exit code) y no solo del `Ok`. |
| 8 | Acción destructiva sin confirmación y con todo preseleccionado | `UX-01`, `SYS-20`, `SYS-03`, `ERR-07`, `SYS-01`, `UX-04` | Toda acción irreversible pide confirmación con la lista concreta y deja sin marcar lo que no es seguro por defecto. |
