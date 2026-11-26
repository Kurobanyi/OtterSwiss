//! UI-11: traducción de los textos que arma core (catálogo de ajustes, Debloater, limpiador, restos y pasos del motor).
//! El texto en español de core es la clave, como en gettext: así core no cambia y un test (abajo) recorre sus catálogos
//! y falla si alguno queda sin traducir. Los detalles con datos que no están aquí se muestran como vienen, igual que
//! el detalle técnico de un error.

use std::borrow::Cow;

/// Texto de core en el idioma de la ventana.
pub fn tr(es: &str, en: bool) -> Cow<'_, str> {
    if !en {
        return Cow::Borrowed(es);
    }
    if let Some(text) = english(es) {
        return Cow::Borrowed(text);
    }
    for (es_pre, es_suf, en_pre, en_suf) in TEMPLATES {
        if let Some(middle) = es
            .strip_prefix(es_pre)
            .and_then(|rest| rest.strip_suffix(es_suf))
        {
            return Cow::Owned(format!("{en_pre}{middle}{en_suf}"));
        }
    }
    Cow::Borrowed(es)
}

/// Frases de core con un dato en medio: (prefijo, sufijo) en español y en inglés.
const TEMPLATES: &[(&str, &str, &str, &str)] = &[
    ("Registro (", ")", "Registry (", ")"),
    ("[Registro: ", "]", "[Registry: ", "]"),
    ("[Servicio: ", "]", "[Service: ", "]"),
    (
        "Servicio '",
        "': deshabilitar y detener",
        "Service '",
        "': disable and stop",
    ),
    (
        "Servicio '",
        "' omitido por perfil",
        "Service '",
        "' skipped by the profile",
    ),
    (
        "Servicio '",
        "' no instalado en este sistema",
        "Service '",
        "' is not installed on this PC",
    ),
    (
        "Servicio '",
        "' detenido y deshabilitado",
        "Service '",
        "' stopped and disabled",
    ),
    (
        "Servicio '",
        "' deshabilitado; se detendrá al reiniciar",
        "Service '",
        "' disabled; it stops after a restart",
    ),
    ("Servicio '", "' omitido", "Service '", "' skipped"),
    ("Activado para perfil ", "", "Enabled for profile ", ""),
    ("Restaurando servicio '", "'…", "Restoring service '", "'…"),
    ("Restaurando clave '", "'…", "Restoring key '", "'…"),
    ("Eliminando ", "…", "Removing ", "…"),
];

fn english(es: &str) -> Option<&'static str> {
    Some(match es {
        // Rellenos en español que core pone en campos de datos.
        "Desconocida" => "Unknown",
        "N/D" => "N/A",
        "Placa Base Estándar" => "Standard motherboard",
        "Zona térmica ACPI (WMI)" => "ACPI thermal zone (WMI)",
        "Zona térmica ACPI (PerfCounter)" => "ACPI thermal zone (PerfCounter)",
        "Ajuste de usuario omitido: OtterSwiss corre con otra cuenta que la del escritorio y lo aplicaría al perfil equivocado. Ábrelo con tu propia cuenta de administrador." => {
            "User setting skipped: OtterSwiss is running as a different account than the desktop and would apply it to the wrong profile. Open it with your own administrator account."
        }
        // Tienda: categorías de apps.
        "Gaming y Launchers" => "Gaming & Launchers",
        "Comunicación y Multimedia" => "Communication & Media",
        "Utilidades y Sistema" => "Utilities & System Tools",
        "Navegadores Web" => "Web Browsers",
        "Desarrollo y Editores" => "Development & Editors",
        "Aplicaciones UWP y Windows (Restauración)" => "Windows UWP & Store (Restoration)",
        // Motor: categorías y pasos.
        "Servicio Windows" => "Windows service",
        "Plan de Energía" => "Power plan",
        "Esquema energético de Windows" => "Windows power scheme",
        "Omitido por perfil de hardware" => "Skipped for this hardware profile",
        "Activar esquema energético óptimo" => "Enable the best power scheme",
        "Aplicar optimizaciones del sistema" => "Apply system optimizations",
        "Instalar drivers de Windows Update" => "Install Windows Update drivers",
        "Instalar un driver del Catálogo de Microsoft" => {
            "Install a Microsoft Update Catalog driver"
        }
        "Instalar una actualización de driver del fabricante" => "Install a vendor driver update",
        "Exportar el respaldo del Driver Store" => "Export the Driver Store backup",
        "Quitar un driver instalado por OtterSwiss" => "Remove a driver installed by OtterSwiss",
        "Revertir optimizaciones del sistema" => "Revert system optimizations",
        "Configurando Plan de Energía" => "Setting up the power plan",
        "Optimización completada" => "Optimization complete",
        "Restaurando Plan de Energía original…" => "Restoring the original power plan…",
        "Reversión completada" => "Revert complete",
        "Plan Equilibrado de Windows (estabilidad térmica)" => {
            "Windows Balanced plan (thermal stability)"
        }
        "Plan Alto Rendimiento de Windows" => "Windows High Performance plan",
        "Plan Alto Rendimiento (portátil/batería detectado)" => {
            "High Performance plan (laptop or battery detected)"
        }
        "Plan Máximo Rendimiento (Ultimate Performance)" => "Ultimate Performance plan",
        // Ajustes de registro y servicios.
        "Desactivar grabación en segundo plano (GameDVR)" => {
            "Turn off background recording (GameDVR)"
        }
        "Deshabilita AppCaptureEnabled en GameDVR para evitar sobrecarga de captura." => {
            "Sets AppCaptureEnabled to off in GameDVR to avoid capture overhead."
        }
        "Optimizar modo pantalla completa (GameConfigStore)" => {
            "Optimize fullscreen mode (GameConfigStore)"
        }
        "Configura GameDVR_FSEBehaviorMode=2 para reducir latencia en juegos." => {
            "Sets GameDVR_FSEBehaviorMode=2 to reduce latency in games."
        }
        "Programación de GPU acelerada por hardware (HAGS)" => {
            "Hardware-accelerated GPU scheduling (HAGS)"
        }
        "Activa HwSchMode=2 en el subsistema gráfico del kernel." => {
            "Sets HwSchMode=2 in the kernel graphics subsystem."
        }
        "Eliminar limitación de red multimedia (NetworkThrottlingIndex)" => {
            "Remove multimedia network throttling (NetworkThrottlingIndex)"
        }
        "Fija NetworkThrottlingIndex=0xFFFFFFFF en Multimedia\\SystemProfile." => {
            "Sets NetworkThrottlingIndex=0xFFFFFFFF in Multimedia\\SystemProfile."
        }
        "Reservar menos CPU a tareas multimedia (SystemResponsiveness)" => {
            "Reserve less CPU for multimedia tasks (SystemResponsiveness)"
        }
        "Reduce SystemResponsiveness a 10%: deja libre para el resto una parte menor de la CPU que MMCSS reserva a tareas multimedia." => {
            "Lowers SystemResponsiveness to 10%: frees for everything else the share of CPU that MMCSS would hold back for multimedia tasks."
        }
        "Desactivar aceleración del puntero (curva lineal)" => {
            "Turn off pointer acceleration (linear curve)"
        }
        "Fija MouseSpeed=0, MouseThreshold1=0 y MouseThreshold2=0 (requiere reiniciar sesión)." => {
            "Sets MouseSpeed=0, MouseThreshold1=0 and MouseThreshold2=0 (requires signing in again)."
        }
        "Umbral 1 de aceleración de ratón a cero" => "Mouse acceleration threshold 1 to zero",
        "Umbral 2 de aceleración de ratón a cero" => "Mouse acceleration threshold 2 to zero",
        "Parte del perfil lineal de ratón en Control Panel\\Mouse." => {
            "Part of the linear mouse profile in Control Panel\\Mouse."
        }
        "Reducir nivel de telemetría diagnóstica" => "Lower the diagnostic telemetry level",
        "Establece AllowTelemetry=1 (mínimo efectivo en ediciones Home/Pro; 0 solo aplica en Enterprise/Education)." => {
            "Sets AllowTelemetry=1 (the effective minimum on Home/Pro; 0 only applies to Enterprise/Education)."
        }
        "Experiencias del usuario y telemetría conectadas" => {
            "Connected user experiences and telemetry"
        }
        "Servicio de enrutamiento de mensajes push WAP" => "WAP push message routing service",
        "Superfetch / SysMain (precarga en memoria)" => "Superfetch / SysMain (memory preloading)",
        // Debloater: categorías.
        "Bloatware Común y Telemetría (Recomendado)" => {
            "Common bloatware and telemetry (recommended)"
        }
        "Aplicaciones Integradas Opcionales" => "Optional built-in apps",
        "Xbox y Gaming (NO marcar si usas Xbox Game Pass)" => {
            "Xbox and gaming (do NOT check if you use Xbox Game Pass)"
        }
        // Debloater: apps.
        "MSN Noticias" => "MSN News",
        "Feed de noticias preinstalado con actualizaciones en segundo plano" => {
            "Preinstalled news feed that updates in the background"
        }
        "El Tiempo (MSN Weather)" => "MSN Weather",
        "Aplicación de clima con procesos en segundo plano" => {
            "Weather app with background processes"
        }
        "Búsqueda Web Bing" => "Bing web search",
        "Integración de búsquedas web de Bing en el sistema" => {
            "Bing web search built into Windows"
        }
        "Obtener Ayuda (Get Help)" => "Get Help",
        "Asistente de soporte preinstalado de Microsoft" => {
            "Microsoft's preinstalled support assistant"
        }
        "Sugerencias / Primeros Pasos" => "Tips / Get Started",
        "Notificaciones y guías de bienvenida de Windows" => {
            "Windows welcome notifications and guides"
        }
        "Hub promocional de Microsoft 365" => "Microsoft 365 promotional hub",
        "Acceso promocional a suscripción de Office (no borra Office instalado)" => {
            "Promotional Office subscription shortcut (does not remove installed Office)"
        }
        "Juego preinstalado con publicidad" => "Preinstalled game with ads",
        "Contactos (People)" => "People",
        "Libreta de contactos integrada heredada" => "Legacy built-in contacts app",
        "Automatización de flujos empresariales preinstalada en Windows 11" => {
            "Business workflow automation preinstalled on Windows 11"
        }
        "Lista de tareas sincronizada con cuenta Microsoft" => {
            "To-do list synced with a Microsoft account"
        }
        "Centro de Opiniones (Feedback Hub)" => "Feedback Hub",
        "Recopilación y envío de comentarios y diagnósticos" => {
            "Collects and sends feedback and diagnostics"
        }
        "Mapas de Windows" => "Windows Maps",
        "Aplicación de mapas sin conexión con servicios en segundo plano" => {
            "Offline maps app with background services"
        }
        "Enlace Móvil (Phone Link)" => "Phone Link",
        "Sincronización continua con Android/iOS en segundo plano" => {
            "Constant background sync with Android/iOS"
        }
        "Reproductor Multimedia / Groove" => "Media Player / Groove",
        "Reproductor UWP preinstalado" => "Preinstalled UWP player",
        "Películas y TV" => "Movies & TV",
        "Tienda/reproductor de alquiler de vídeo de Microsoft" => {
            "Microsoft's video rental store and player"
        }
        "Editor de vídeo web preinstalado en Windows 11" => {
            "Web video editor preinstalled on Windows 11"
        }
        "Microsoft Teams (Personal)" => "Microsoft Teams (personal)",
        "Cliente preinstalado de Teams que arranca en segundo plano" => {
            "Preinstalled Teams client that starts in the background"
        }
        "Microsoft Copilot (App UWP)" => "Microsoft Copilot (UWP app)",
        "Aplicación asistente IA residente" => "Always-running AI assistant app",
        "Asistente heredado descontinuado" => "Discontinued legacy assistant",
        "Asistencia Rápida" => "Quick Assist",
        "Herramienta de soporte remoto UWP" => "UWP remote support tool",
        "Nuevo Outlook para Windows" => "New Outlook for Windows",
        "Cliente web de Outlook preinstalado con anuncios" => {
            "Preinstalled Outlook web client with ads"
        }
        "Grabadora de Voz" => "Voice Recorder",
        "Grabadora básica de audio" => "Basic audio recorder",
        "Cámara de Windows" => "Windows Camera",
        "App UWP de cámara" => "UWP camera app",
        "Reloj y Alarmas" => "Clock & Alarms",
        "App de temporizadores y alarmas" => "Timers and alarms app",
        "Portal de Realidad Mixta" => "Mixed Reality Portal",
        "Entorno VR heredado de Windows" => "Legacy Windows VR environment",
        "App de Xbox (Game Pass)" => "Xbox app (Game Pass)",
        "Necesaria si utilizas PC Game Pass" => "Needed if you use PC Game Pass",
        "Overlay y captura de Xbox (necesaria para invitaciones/Game Pass)" => {
            "Xbox overlay and capture (needed for invites and Game Pass)"
        }
        "Interfaz de inicio de sesión Xbox en juegos" => "Xbox sign-in interface in games",
        "Accesibilidad de voz a texto en juegos Xbox" => {
            "Speech-to-text accessibility in Xbox games"
        }
        // Debloater: ajustes de registro.
        "Desactivar anuncios, sugerencias e instalación silenciosa de apps" => {
            "Turn off ads, suggestions and silent app installs"
        }
        "Deshabilita ContentDeliveryManager (apps patrocinadas, sugerencias en Inicio y pantalla de bloqueo)" => {
            "Turns off ContentDeliveryManager (sponsored apps, Start and lock screen suggestions)"
        }
        "Desactivar búsqueda web de Bing en la barra de Windows" => {
            "Turn off Bing web search in the Windows search box"
        }
        "Evita que la búsqueda local del Menú Inicio envíe consultas a Bing" => {
            "Stops Start menu searches from being sent to Bing"
        }
        "Desactivar Widgets y Noticias e Intereses" => "Turn off Widgets and News and Interests",
        "Oculta y detiene el panel de noticias/clima en la barra de tareas" => {
            "Hides and stops the news and weather panel on the taskbar"
        }
        "Ocultar anuncios y notificaciones de proveedores en el Explorador" => {
            "Hide ads and sync provider notifications in File Explorer"
        }
        "Fija ShowSyncProviderNotifications=0 en el Explorador de archivos" => {
            "Sets ShowSyncProviderNotifications=0 in File Explorer"
        }
        "Desactivar Windows Copilot mediante política de usuario" => {
            "Turn off Windows Copilot with a user policy"
        }
        "Fija TurnOffWindowsCopilot=1 (reversible)" => "Sets TurnOffWindowsCopilot=1 (reversible)",
        // Tienda: apps de Microsoft Store (IN-09).
        "Calculadora de Windows" => "Windows Calculator",
        "App de Xbox (PC Game Pass)" => "Xbox app (PC Game Pass)",
        "Fotos de Windows" => "Windows Photos",
        "Herramienta Recortes" => "Snipping Tool",
        "Paint de Windows" => "Windows Paint",
        "Bloc de Notas Store" => "Notepad (Store)",
        // Limpiador.
        "Temporales de Usuario" => "User temporary files",
        "Temporales de Windows" => "Windows temporary files",
        "Descargas de Windows Update" => "Windows Update downloads",
        "Caché de Internet" => "Internet cache",
        "Papelera de reciclaje" => "Recycle Bin",
        "Limpieza completada" => "Cleanup complete",
        // Restos de una desinstalación.
        "Carpeta de instalación" => "Install folder",
        "Archivos de Programa (x64)" => "Program Files (x64)",
        "Archivos de Programa (x86)" => "Program Files (x86)",
        "Escritorio" => "Desktop",
        "Escritorio Público" => "Public desktop",
        "Menú Inicio (Usuario)" => "Start menu (user)",
        "Menú Inicio (Sistema)" => "Start menu (all users)",
        "Registro HKCU\\Software" => "Registry HKCU\\Software",
        "Registro HKLM\\SOFTWARE" => "Registry HKLM\\SOFTWARE",
        "Registro HKLM\\SOFTWARE (32-bit)" => "Registry HKLM\\SOFTWARE (32-bit)",
        "Inicio automático (Run)" => "Startup (Run)",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::debloat::{DEBLOAT_APP_CATALOG, DEBLOAT_REG_TWEAKS, DebloatCategory};
    use crate::core::optimize::tweaks_catalog::{REGISTRY_TWEAKS, SERVICE_TWEAKS};
    use crate::core::store::app_store::{self, AppCategory, InstallSource};

    /// Nombres de marca: se escriben igual en los dos idiomas.
    const BRAND_NAMES: &[&str] = &[
        "Microsoft Solitaire Collection",
        "Power Automate Desktop",
        "Microsoft To Do",
        "Microsoft Clipchamp",
        "Cortana",
        "Xbox Game Bar (Win+G)",
        "Xbox Live UI",
        "Xbox Speech to Text",
        "Windows Terminal",
    ];

    fn translated(es: &str) -> bool {
        tr(es, true) != es || english(es).is_some()
    }

    /// UI-11: cada texto de los catálogos de core tiene traducción. Al agregar un ajuste o una app, este test pide su
    /// texto en inglés.
    #[test]
    fn catalogos_de_core_traducidos() {
        let mut missing = Vec::new();
        let mut check = |es: &str| {
            if !translated(es) {
                missing.push(es.to_string());
            }
        };
        for t in REGISTRY_TWEAKS {
            check(t.label);
            check(t.description);
        }
        for t in SERVICE_TWEAKS {
            check(t.label);
        }
        for a in DEBLOAT_APP_CATALOG {
            // Los nombres de marca (Microsoft Clipchamp, Cortana…) se escriben igual en los dos idiomas.
            if !BRAND_NAMES.contains(&a.display_name) {
                check(a.display_name);
            }
            check(a.description);
        }
        for c in [
            AppCategory::Gaming,
            AppCategory::Communication,
            AppCategory::Utilities,
            AppCategory::Browsers,
            AppCategory::Development,
            AppCategory::UwpRestoration,
        ] {
            check(c.label());
        }
        for p in app_store::get_catalog() {
            if matches!(p.source, InstallSource::Store { .. }) && !BRAND_NAMES.contains(&p.name) {
                check(p.name);
            }
        }
        for c in [
            DebloatCategory::Recommended,
            DebloatCategory::OptionalApps,
            DebloatCategory::XboxGaming,
        ] {
            check(c.label());
        }
        for t in DEBLOAT_REG_TWEAKS {
            check(t.label);
            check(t.description);
        }
        assert!(
            missing.is_empty(),
            "Textos de core sin traducir:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn plantillas_con_un_dato() {
        assert_eq!(
            tr("Servicio 'SysMain' detenido y deshabilitado", true),
            "Service 'SysMain' stopped and disabled"
        );
        assert_eq!(tr("Registro (HKCU)", true), "Registry (HKCU)");
        assert_eq!(tr("Registro (HKCU)", false), "Registro (HKCU)");
        assert_eq!(tr("texto desconocido", true), "texto desconocido");
    }
}
