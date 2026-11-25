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
| Seguridad (`SEC-`) | 37 | 2 | 1 | 0 | 34 |
| Sistema y rollback (`SYS-`) | 27 | 2 | 1 | 0 | 24 |
| Errores y procesos (`ERR-`) | 24 | 1 | 0 | 0 | 23 |
| Concurrencia (`CON-`) | 3 | 1 | 0 | 0 | 2 |
| Arquitectura (`ARQ-`) | 5 | 0 | 0 | 0 | 5 |
| Interfaz (`UI-`) | 22 | 1 | 0 | 0 | 21 |
| Diseño y producto (`UX-`) | 4 | 0 | 1 | 1 | 2 |
| Pruebas (`TST-`) | 2 | 2 | 0 | 0 | 0 |
| **Total** | **124** | **9** | **3** | **1** | **111** |

La auditoría del código de v4.2.0 (25/11, al final de `docs/BUGS-detalle.md`) abrió 28; quedan abiertos `SEC-32` (parcial: el
proxy del usuario), `SEC-35`, `SYS-26`, `SYS-27`, `CON-03`, `ERR-24` (parcial) y `TST-02` (parcial). Sin probar instalando en una VM.

Las mejoras de arquitectura, código limpio, temas, i18n y herramientas (auditoría de buenas prácticas del 24/11) viven en
`docs/PRACTICAS.md`; aquí solo quedan los defectos que esa auditoría encontró.

## Abiertos

| ID | Título | Afecta | Sev. | Estado |
|---|---|---|---|---|
| [`TST-01`](BUGS-detalle.md#tst-01) | El repositorio no tiene ni un test | Desarrollador | Alta | Parcial |
| [`UI-11`](BUGS-detalle.md#ui-11) | Textos de core sin traducir y mezcla de idiomas | Usuario | Media | Parcial |
| [`SEC-32`](BUGS-detalle.md#sec-32) | Proxy y raíces de certificados de HKCU permiten interceptar el HTTPS elevado y falsificar firmas validadas solo por CN | Sistema | Alta | Parcial |
| [`SEC-35`](BUGS-detalle.md#sec-35) | Entradas versionadas de la tienda sin SHA-256 y firmantes sin confirmar | Sistema | Baja | Pendiente |
| [`SYS-26`](BUGS-detalle.md#sys-26) | Un driver instalado desde la app no se puede revertir | Sistema, Usuario | Media | Pendiente |
| [`SYS-27`](BUGS-detalle.md#sys-27) | El JSON de Intel puede ofrecer el paquete de otro dispositivo | Sistema, Usuario | Media | Pendiente |
| [`ERR-24`](BUGS-detalle.md#err-24) | Argumentos silenciosos, versión e IDs de la Store dudosos en el catálogo nuevo | Usuario | Media | Parcial |
| [`CON-03`](BUGS-detalle.md#con-03) | Windows Update se consulta e instala sin plazo ni cancelación | Usuario | Media | Pendiente |
| [`TST-02`](BUGS-detalle.md#tst-02) | El código nuevo de drivers, tienda y sensores no tiene tests | Desarrollador | Media | Parcial |

## Diferidos y pendientes de decisión

Conservan su expediente; se retoman por decisión del autor.

| ID | Título | Afecta | Sev. | Estado |
|---|---|---|---|---|
| [`SYS-24`](BUGS-detalle.md#sys-24) | Con elevación «sobre el hombro», los tweaks HKCU van al perfil del admin | Usuario | Baja | Diferido |
| [`SEC-17`](BUGS-detalle.md#sec-17) | Todo el programa corre como administrador aunque la mitad de las funciones no lo necesitan | Sistema, Usuario | Media | Diferido |
| [`UX-02`](BUGS-detalle.md#ux-02) | «Optimizar Sistema» aplica sin confirmar tweaks sin base o mal descritos | Usuario, Sistema | Media | Diferido |
| [`UX-03`](BUGS-detalle.md#ux-03) | «Optimizar RAM» es una práctica desacreditada | Usuario | Media | Pendiente de decisión |

## Siguiente

1. Decisión del autor: `SEC-32` (WinInet sin el proxy del usuario rompe redes con proxy obligatorio) y `SEC-35` (fijar SHA-256 en
   las entradas versionadas obliga a actualizarlos con cada versión).
2. Drivers: `SYS-26` (revertir un driver instalado), `CON-03` (Windows Update con cancelación) y `SYS-27` (formato real del JSON de Intel).
3. En una VM: instalar un driver de cada camino, quitar una app Appx, `ERR-24` (Moonlight, GOG) y la ventana en tamaño chico.
4. `TST-01`: tests de `SYS-06`, `SYS-07`, `SYS-10` y `SYS-12`. Decisiones del autor: `UX-03`, `UX-02`, `SEC-17` (con `SYS-24`), `UI-11`.
5. Mejoras de `docs/PRACTICAS.md`.

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
