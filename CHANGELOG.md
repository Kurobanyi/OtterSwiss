# Changelog — OtterSwiss

## [5.0.0] — 2026-11-26

### Novedades
- **Interfaz nueva:** encabezado por sección, estilo abierto sin una caja por elemento, portada de Inicio con la nutria y el
  equipo, iconos de Windows y logos reales de las apps de la tienda (Simple Icons, CC0).
- **Configuración:** navegación arriba (predeterminada) o barra lateral contraíble, tema con vista previa de cada uno e idioma.
- **Arranca sin administrador:** avisa del modo estándar y ofrece «Reabrir como administrador»; cada acción que lo necesita lo
  pide.
- **Tienda por winget** (corre con la cuenta del usuario), detección de apps de la Store y avisos claros si winget está viejo.
- **Drivers:** Catálogo de Microsoft verificado, respeto de la política del equipo, confirmación con respaldo, historial para
  quitar lo instalado y búsqueda cancelable. Sale la fuente Intel.
- Confirmación antes de revertir el perfil, el registro del Debloater y de quitar el menú contextual; Archivos muestra el destino
  con «Abrir carpeta».

### Seguridad
- Descargas por el proxy de la máquina (nunca el del usuario), firmas encadenadas a las raíces del equipo y paquetes de fabricante
  verificados; con otra cuenta de administrador no se tocan los ajustes del usuario.

### Calidad
- 107 pruebas automáticas y pruebas de sistema en VM (optimizar, Debloater, DNS, limpiador, archivador, desinstalador, tienda,
  proxy y otra cuenta). 131 hallazgos de auditoría resueltos.

### Buenas prácticas (rama `auditoria/buenas-practicas`, detalle en `docs/PRACTICAS.md`)
- Defectos: el build falla si no puede incrustar el manifiesto de administrador; `--apply-profile` fuera de los flags de
  relanzamiento elevado; spinner de carga visible y error distinto de «sin datos»; textos de estado con contraste AA.
- Código ordenado por capas (`core/platform`, `security`, `hardware`, `optimize`, `debloat`, `uninstall`, `archive`, `store`),
  sin dependencias de una capa hacia la de encima, y módulos de más de 900 líneas partidos.
- Un solo cliente HTTPS, un solo camino para descargar y ejecutar instaladores, un solo lanzador de herramientas con timeout
  (el escaneo de Appx ya no puede colgarse) y un solo lector de respaldos JSON.
- Tema «Según Windows» y selector de tema; idioma inicial según Windows; `color-scheme` (barras y casillas oscuras en temas
  oscuros); `prefers-reduced-motion`; icono de 5 KB en lugar del GIF de 1,7 MB; catálogo de textos con plurales.
- CI (formato, clippy, tests, MSRV, `cargo-deny`), lints del proyecto, edición 2024, `.gitattributes` y publicación por tag;
  la versión del instalador sale del ejecutable.
- Sale `sysinfo`; `thiserror` 2 y `dirs` 6.

### Auditoría del 24/11/2026 (PR #1)
Arreglos de seguridad y robustez (detalle en `docs/BUGS.md` y `docs/BUGS-detalle.md`).
## [4.2.0] — 2026-11-25
### 🚀 Novedades y Expansión Masiva
- **Restaurador UWP / Microsoft Store (Winget):** Integración completa en la App Store para reinstalar de forma desatendida y silenciosa aplicaciones preinstaladas de Windows eliminadas durante el Debloat (Clima, Cámara, Reloj, Xbox, Enlace Móvil, etc.).
- **Ampliación del App Store:** Se han incorporado 20 aplicaciones esenciales adicionales, elevando el catálogo a **50 aplicaciones curadas** de escritorio y UWP (Google Chrome, Brave, VLC, Everything, PowerToys, ShareX, Telegram, WhatsApp, VS Code, Python, Node.js, etc.).
- **Iconos Vectoriales Genéricos:** Todas las aplicaciones del App Store ahora cuentan con iconos SVG inline minimalistas que se adaptan automáticamente a los colores del tema visual activo.
- **4 Nuevos Temas Visuales (Total 10):** Se suman los estilos **Dracula**, **Tokyo Night**, **Solarized Dark** y **Sakura (Rosé Pine)**, todos con contraste verificado WCAG AA (`--on-accent`, `--on-danger`).

## [4.0.3] — 2026-11-25
### 🔒 Seguridad y Blindaje de Arquitectura (Colaboración con @KenKenFS)
- **Migración total de ecosistema Rust:** Actualización a `sevenz-rust2 0.25`, `zip 9.5`, `sysinfo 0.40`, `sha2 0.11`, `thiserror 2.0`, `winreg 0.56` y `dirs 7.0`.
- **Mitigación integral de vulnerabilidades:**
  - `SEC-01`: Corrección de la condición de carrera por `DELETE_ON_CLOSE` en `safe_fs.rs`.
  - `SEC-02` / `SEC-05`: Verificación rigurosa de SHA-256 e instalación desde handles read-locked (`reopen_verified_for_exec`).
  - `SEC-12`: Comprobación de revocación completa de cadenas con `WTD_REVOKE_WHOLECHAIN`.
  - `SEC-28`: Uso de buffers alineados (`AlignedBuf`) para prevenir comportamientos indefinidos en llamadas Win32 (`_UB_`).
- **Robustez de Sistema:**
  - `SYS-01`: La extracción de archivos ahora busca carpetas hermanas libres (`nombre (2)`) evitando sobrescribir o borrar datos existentes.
  - `SYS-05`: El `JobObject` espera a que todos los procesos hijos terminen de forma segura.
  - `ERR-13`: Comprobación nativa previa de **WebView2** antes de levantar la interfaz gráfica de Dioxus.
  - Inserción de 37 tests unitarios y de integración para garantizar robustez ante regresiones.

### Cambios al sistema
- Extraer nunca borra una carpeta existente: usa «nombre (2)».
- Desinstalar pide confirmación, respeta el código de salida y solo marca los restos seguros; las claves se exportan a `RegBackups`.
- El Debloater pide confirmación con la lista y ya no preselecciona Outlook, To Do, Enlace Móvil ni Teams.
- Aplicar DNS confirma los adaptadores y muestra el aviso de privacidad; restaurar devuelve IPv4 e IPv6.
- Desinstalar OtterSwiss revierte motor, DNS y Debloater (`--rollback`).
- El limpiador ya no marca la Papelera por defecto ni toca Prefetch; Windows Update solo con sus servicios detenidos.

### Interfaz
- Avisos con el tono real del resultado (errores, omisiones, cancelación) e informes de rollback completos.
- Diálogos accesibles (rol, Escape, foco atrapado), `:focus-visible` y contraste AA en los seis temas.
- Las pestañas con tareas en curso conservan su estado al cambiar de pestaña; la carga pesada sale del hilo de UI.
- Hardware muestra lo que pudo leer y por qué falta el resto, sin GPU repetidas.

### Desarrollo
- Tests unitarios; `cargo test` corre sin pedir administrador. Recorrido de sistema como administrador en `tests/vm_sistema.rs`.
- `.cargo/config.toml` solo con el CRT estático; changelog renombrado a `CHANGELOG.md`.

## [3.5.1] - 2026-11-24
- Auto-actualizador nativo desde GitHub Releases.

## [3.5.0] - 2026-11-24
- Primera versión publicada en el repositorio: Dashboard con optimizador y rollback, limpiador, Debloater, DNS, archivador,
  herramientas y tienda de apps, desinstalador profundo, hardware y sensores.
- Las versiones 3.2 a 3.4 no tienen registro (no hay commits de ellas).

## [3.1.0] - 2026-11-24
### Añadido
- **Mini-ventana y modal de progreso en Archivador:** Telemetría en vivo (archivo actual, MB/s, ETA, conteo de archivos y cancelar) tanto en la app como desde el menú contextual (`460x235`).
- **App Store:** Añadidos **qBittorrent** y **Slack** (MSI Enterprise). Soporte de mirrors CDN de SourceForge (`*.dl.sourceforge.net`).
- **3 Temas nuevos (6 en total):** `Nord`, `Emerald` y `Crimson` junto a `Dark`, `Cyberpunk` y `Light`.
- **Idioma Inglés / Español (`ES / EN`):** Selector persistente en la barra superior y descripciones más concisas.
- **Desinstalador Profundo:** Cálculo asíncrono de peso en disco para apps que no reportan `EstimatedSize` en el Registro + ordenamiento alfabético y por tamaño.
- **Privacidad de build:** Remapeo de rutas locales en `.cargo/config.toml` (`--remap-path-prefix`) y hardening contra symlinks en CLI.

## [3.0.0] - 2026-11-23
- Interfaz sin marcos estilo launcher horizontal con GIF animado, Hardware Inspector SMBIOS/DXGI/CPUID (`specs.rs`), Debloater UWP seguro, menú contextual para ZIP/7Z/TAR.GZ y manifiesto `requireAdministrator`.
