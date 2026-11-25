<div align="center">
  <img src="assets/brand/logo.gif" alt="OtterSwiss" width="228" height="128" />
  <h1>OtterSwiss v3.5.1</h1>
  <p><strong>Suite portable de optimización, diagnóstico y utilidades nativas para Windows 10 / 11 (x64)</strong></p>

  <p>
    <img src="https://img.shields.io/badge/Rust-1.82%2B-orange?logo=rust" alt="Rust Version" />
    <img src="https://img.shields.io/badge/GUI-Dioxus%200.7-blue" alt="Dioxus 0.7" />
    <img src="https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011%20(x64)-0078D6?logo=windows" alt="Platform Windows" />
    <img src="https://img.shields.io/badge/Architecture-Native%20Win32%20RAII-success" alt="Win32 Architecture" />
    <img src="https://img.shields.io/badge/License-MIT-green" alt="MIT License" />
  </p>
</div>

---

## ⚡ ¿Qué es OtterSwiss?

**OtterSwiss** es una suite de mantenimiento y optimización para Windows desarrollada íntegramente en **Rust** con interfaz gráfica moderna sin bordes (*frameless*) en **Dioxus 0.7**. 

A diferencia de las herramientas convencionales basadas en scripts opacos de PowerShell o utilidades que aplican ajustes contraproducentes ("placebos"), OtterSwiss opera mediante **llamadas nativas a la API Win32**, transacciones atómicas con reversión obligatoria (*Rollback*) y arquitecturas seguras contra condiciones de carrera (TOCTOU).

---

## 🚀 Módulos y Capacidades

### 1. 🛡️ Dashboard de Optimización Transaccional
* **Perfiles adaptativos:** Detección de hardware (`LowEnd`, `MidRange`, `HighEnd`) y discriminación automática entre PC de sobremesa y portátil.
* **Respaldo atómico obligatorio (`backup.json`):** Cada cambio en el Registro o servicios se almacena previamente con `sync_all()`. Permite reversión completa (*Rollback*) en cualquier momento o mediante el parámetro `--rollback`.
* **Detección de reinicio pendiente:** Notificación nativa si Windows requiere reinicio para consolidar cambios.

### 2. 🔍 Inspector de Hardware Nativo
* **CPU:** Marca, modelo, familia, stepping, conjunto de instrucciones y distinción física de **P-Cores vs. E-Cores** mediante la instrucción intrínseca `__cpuid` y `GetLogicalProcessorInformationEx`.
* **Placa Base / BIOS / RAM:** Parseo directo de la tabla SMBIOS (`RSMB`) vía `GetSystemFirmwareTable` (lectura ranura por ranura: capacidad, fabricante, velocidad MT/s y voltaje).
* **GPU:** Enumeración de adaptadores físicos vía DXGI (`IDXGIFactory1`), descartando adaptadores virtuales o emulados.
* **Almacenamiento:** Consulta de discos físicos vía `IOCTL_STORAGE_QUERY_PROPERTY`.
* Exportación a formato JSON y copia rápida de ficha técnica al portapapeles.

### 3. 🌡️ Monitor Térmico en Tiempo Real
* Hilo dedicado con sesión persistente y cancelación cooperativa con `AtomicBool`.
* **NVMe / SSD:** Lectura de temperatura en disco físico vía `STORAGE_TEMPERATURE_DATA_DESCRIPTOR`.
* **GPU NVIDIA:** Consulta en vivo vía `nvml.dll` (`nvmlInit_v2`).
* **GPU AMD:** Consulta vía `atiadlxx.dll` con `ADL2_New_QueryPMLogData_Get` (temperaturas de Edge, Hotspot y VRAM) con fallbacks dinámicos a OverdriveN/OD5.
* **Zonas ACPI:** Telemetría de plataforma mediante contadores de sistema y WMI.

### 4. 🧹 Limpiador de Sistema Seguro
* Mitigación de symlinks y condiciones de carrera (TOCTOU) eliminando exclusivamente a través de `HANDLE` con `FILE_FLAG_BACKUP_SEMANTICS`.
* Purga de temporales de usuario y Windows, Prefetch, descargas pendientes de Windows Update y Papelera de reciclaje.
* Selector de umbral de seguridad: modo seguro (> 24 horas) o limpieza total.

### 5. 🔲 Debloater Seguro de Windows
* Desinstalación y desaprovisionamiento de paquetes AppX/UWP (`Get-AppxProvisionedPackage -Online`) ejecutados sin consolas emergentes (`CREATE_NO_WINDOW`).
* Ajustes de privacidad (bloqueo de publicidad en Menú Inicio, sugerencias de Bing, Widgets y Copilot) con respaldo en `debloat_backup.json` y restauración en 1 clic.

### 6. 🌐 Optimizador DNS con Test de Latencia
* Medición de latencia en vivo por Ping ICMP/TCP:53 contra resolvers seguros (Cloudflare, Quad9, Google).
* Configuración de adaptadores físicos con la API tipada de Windows `SetInterfaceDnsSettings` (soporte completo IPv4 e IPv6) y purga de caché con `DnsFlushResolverCache`.

### 7. 📦 App Store y Runtimes Gamer
* Catálogo curado de software esencial y librerías de tiempo de ejecución (DirectX, Visual C++ x64/x86, .NET 8, WebView2).
* Descargas HTTPS exclusivas desde servidores oficiales de cada fabricante.
* Verificación criptográfica obligatoria (SHA-256 o validación de firma Authenticode vía `WinTrust`).
* **Des-elevación por Token:** Aplicaciones que rechazan ejecutarse como administrador (Spotify, Discord) clonan el token del proceso del shell (`explorer.exe`) mediante `DuplicateTokenEx` y `CreateProcessWithTokenW`.

### 8. 🗑️ Desinstalador Profundo
* Inventario de aplicaciones registradas en `HKLM` y `HKCU` ordenable por nombre o tamaño.
* Cálculo asíncrono de tamaño real en disco para instaladores que no reportan `EstimatedSize`.
* Escaneo y purga de restos tras la desinstalación oficial (carpetas en `ProgramData`/`AppData`, claves del Registro y accesos directos huérfanos).

### 9. 🗜️ Archivador 100% Código Abierto
* Compresión y extracción nativa de formatos **ZIP**, **7Z** y **TAR.GZ** (libre de librerías privativas).
* Telemetría en vivo: velocidad en MB/s, porcentaje, archivo actual y tiempo estimado (ETA).
* Integrado en el menú contextual de Windows con ventana compacta de progreso.

### 10. 🔄 Auto-Actualizador de GitHub Releases
* Detección automática de nuevas versiones mediante la API de GitHub Releases con protocolos nativos WinInet.
* Indicador interactivo en la barra superior de la app y panel modal con el registro de cambios (*Changelog*).
* Descarga del instalador con barra de progreso y ejecución automatizada en un clic.

---

## 🛡️ Filosofía Anti-Placebo

OtterSwiss rechaza las prácticas engañosas comunes en herramientas de "optimización". **Por diseño de seguridad, este software NO:**
- Desactiva la compresión de memoria de Windows (`MemoryCompression`).
- Elimina ni desactiva el archivo de paginación (`pagefile.sys`).
- Manipula temporizadores de alta precisión (`HPET` / `useplatformclock`).
- Altera negativamente el algoritmo TCP Auto-Tuning (se mantiene en su estándar óptimo `normal`).
- Deshabilita Windows Defender ni rompe los canales de parches de seguridad de Windows Update.

---

## 🛠️ Compilación y Desarrollo

### Requisitos previos:
* [Rust](https://rustup.rs/) (versión 1.82 o superior, toolchain `stable-x86_64-pc-windows-msvc`).
* [Inno Setup 6+](https://jrsoftware.org/isdl.php) (opcional, solo para generar el instalador final).

### 1. Clonar el repositorio:
```powershell
git clone https://github.com/Kurobanyi/OtterSwiss.git
cd OtterSwiss