# Changelog — OtterSwiss

## [Sin publicar]

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

### Seguridad
- Borrado verificado sobre el handle antes de marcarlo; carpeta de datos `ProgramData\OtterSwiss` solo para administradores.
- Instaladores, runtimes y auto-actualizador se ejecutan desde el mismo handle verificado (SHA-256 / Authenticode con revocación).
- Respaldos de motor, DNS y Debloater validados contra el catálogo antes de escribir nada; un respaldo ilegible aborta.
- Rutas privilegiadas desde `SHGetKnownFolderPath`, no desde variables de entorno ni HKCU.
- Extracción y compresión sin seguir junctions ni symlinks, con Mark-of-the-Web propagado.
- `sevenz-rust` 0.6 (sin mantenimiento) reemplazado por `sevenz-rust2` 0.25; Rust mínimo 1.93.
- Pines de la tienda actualizados (7-Zip 26.06, Git 2.56.1, Notepad++ 8.9.9.1, OBS 33.0.3, Playnite 10.65) y SHA-256 fijo para qBittorrent.

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
