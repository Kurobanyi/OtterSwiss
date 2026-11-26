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

Estado al 26/11/2026, rama `auditoria/buenas-practicas`: `cargo test` (99 unitarios y 4 de integración, más 3 ignorados y 8 de VM) y
`clippy --all-targets -- -D warnings` en verde en Windows. La interfaz se recorrió sin elevación en tema oscuro y claro, en español e
inglés, el 25/11 y de nuevo el 26/11 con las vistas ya partidas, el aviso «Modo estándar» y la ventana al ancho mínimo (`UI-23`). **Acciones que cambian el sistema probadas como
administrador** en Windows Sandbox con `tests/vm_sistema.rs` (`cargo test --release --test vm_sistema -- --ignored --nocapture
--test-threads=1`, solo en una máquina desechable): optimizar y revertir, Debloater (registro) y revertir, DNS IPv4/IPv6, limpiador con
junction trampa, comprimir y extraer, desinstalar y purgar una app falsa, revertir todo y `--rollback`: 0 fallos. El 26/11 se repitió en una
VM de Hyper-V (Windows 11 Enterprise 25H2): 7 de 7, con una app Appx real quitada (`ERR-26`), y la tienda con winget real (`paso_8_tienda`: instalar por equipo y por usuario, cancelar, quitar; `ERR-27`). Sin probar: drivers de fabricante, proxy de máquina y la ventana en tamaño chico.

| Tipo | Total | Abiertos o parciales | Diferidos | Pendientes de decisión | Resueltos |
|---|---|---|---|---|---|
| Seguridad (`SEC-`) | 37 | 0 | 0 | 0 | 37 |
| Sistema y rollback (`SYS-`) | 27 | 0 | 0 | 0 | 27 |
| Errores y procesos (`ERR-`) | 27 | 0 | 0 | 0 | 27 |
| Concurrencia (`CON-`) | 4 | 0 | 0 | 0 | 4 |
| Arquitectura (`ARQ-`) | 5 | 0 | 0 | 0 | 5 |
| Interfaz (`UI-`) | 25 | 1 | 0 | 0 | 24 |
| Diseño y producto (`UX-`) | 4 | 0 | 0 | 0 | 4 |
| Pruebas (`TST-`) | 2 | 1 | 0 | 0 | 1 |
| **Total** | **131** | **1** | **0** | **0** | **130** |

La auditoría del código de v4.2.0 (25/11, al final de `docs/BUGS-detalle.md`) abrió 28; todos están resueltos: `ERR-24` se cerró probando la tienda en la VM y `ERR-25` (Intel) quitando esa fuente.

Las mejoras de arquitectura, código limpio, temas, i18n y herramientas (auditoría de buenas prácticas del 24/11) viven en
`docs/PRACTICAS.md`; aquí solo quedan los defectos que esa auditoría encontró.

## Abiertos

| ID | Título | Afecta | Sev. | Estado |
|---|---|---|---|---|
| [`TST-01`](BUGS-detalle.md#tst-01) | El repositorio no tiene ni un test | Desarrollador | Alta | Parcial |
| [`UI-11`](BUGS-detalle.md#ui-11) | Textos de core sin traducir y mezcla de idiomas | Usuario | Media | Resuelto |

## Diferidos y pendientes de decisión

Ninguno. Los cuatro que quedaban (`SEC-17`, `SYS-24`, `UX-02` y `UX-03`) se resolvieron el 26/11 por decisión del autor; cada
expediente conserva el motivo del enfoque elegido y lo descartado.

## Siguiente

1. Pendiente solo lo que no se pudo probar: instalar y quitar un driver de cada camino (hace falta hardware real) y la reapertura elevada con UAC (`SEC-17`), que acepta el usuario.
2. `UI-11` en inglés con detalles del motor: los textos de errores de core (p. ej. la nota de Sensores) siguen en español con la app en inglés.
3. `TST-01`: tests de `SYS-06`, `SYS-07`, `SYS-10` y `SYS-12`. `SYS-24`: el caso «otra cuenta» solo se reproduce con una cuenta estándar que pide credenciales de administrador (UAC); no se probó.
4. Mejoras de `docs/PRACTICAS.md`: no queda ninguna pendiente.

## Patrones

Familias de falla que ya ocurrieron en este código, con sus IDs como evidencia. Léelos antes de escribir código y, al cerrar un
hallazgo, busca las demás instancias del mismo patrón.

| # | Patrón | Evidencia | Qué comprobar |
|---|---|---|---|
| 1 | Efecto irreversible antes de la validación | `SEC-01`, `SEC-25`, `SYS-17` | Que ningún handle se abra con `DELETE_ON_CLOSE`, `CREATE_ALWAYS` o `TRUNCATE`, ni se cambie un estado del sistema, antes de validar; la validación va sobre el handle y el efecto después. |
| 2 | Una ruta de descarga o ejecución que esquiva `security/download.rs`, o que verifica y luego reabre por ruta | `SEC-02`, `SEC-05`, `SEC-14`, `SEC-30`, `SEC-33` | Que todo binario descargado o ejecutado pase por `secure_download_and_verify` y se ejecute desde el mismo handle verificado. Buscar `File::create`, `InternetOpenUrlW`, `Command::new` y `CreateProcessW` fuera de `trust.rs`. |
| 3 | Proceso hijo con pipes que no se leen mientras corre, o del que solo se espera al padre | `ERR-01`, `SYS-05`, `ERR-11` | Que cada `Stdio::piped()` tenga un lector concurrente o use `output()`; que la espera sea por el Job completo. |
| 4 | Un JSON de `ProgramData` que se aplica como admin sin validar su contenido ni su dueño | `SEC-03`, `SEC-09`, `SEC-10`, `SEC-04` | Que cada entrada leída de un respaldo se compare con el catálogo embebido antes de escribir registro, DNS o servicios, y que el archivo sea de Administradores o SYSTEM. |
| 5 | Rutas privilegiadas tomadas de variables de entorno, HKCU o comparación léxica | `SEC-06`, `SEC-07`, `SEC-11`, `SEC-19`, `SEC-20` | Usar `SHGetKnownFolderPath`, canonizar ambos lados antes de comparar, rechazar reparse points en cada componente y no fiarse de HKCU para decidir qué tocar fuera del perfil. |
| 6 | Un error tragado que acaba sobrescribiendo o borrando el estado original | `SYS-06`, `SYS-09`, `SYS-10`, `SYS-11` | Buscar `let _ =`, `.ok()`, `if let Ok` y `unwrap_or_default()` en respaldo y rollback: si falla leer el original, se aborta, nunca se asume «vacío». |
| 7 | La UI anuncia éxito sin mirar el resultado real | `UI-01`, `UI-02`, `ERR-07`, `ERR-10`, `SYS-02`, `SYS-16`, `ERR-15`, `ERR-18`, `ERR-20` | Que cada banner y contador salga de los campos de fallo del informe (`failed_items`, `error`, exit code) y no solo del `Ok`. |
| 8 | Acción destructiva sin confirmación y con todo preseleccionado | `UX-01`, `SYS-20`, `SYS-03`, `ERR-07`, `SYS-01`, `UX-04`, `SEC-37` | Toda acción irreversible pide confirmación con la lista concreta y deja sin marcar lo que no es seguro por defecto. |
| 9 | Binario que se ejecuta elevado desde una ruta que el usuario puede escribir, o con un token que puede fallar en silencio | `SEC-31`, `SEC-06` | Rutas absolutas bajo System32 o Program Files, nunca `LocalAppData` ni nombres relativos; si el token del usuario falla, abortar en vez de elevar. |
