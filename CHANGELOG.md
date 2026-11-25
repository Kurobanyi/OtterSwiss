# Changelog — OtterSwiss

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

## [3.5.1] — 2026-11-24
- Integración nativa del Auto-Actualizador conectado a GitHub Releases mediante WinInet.