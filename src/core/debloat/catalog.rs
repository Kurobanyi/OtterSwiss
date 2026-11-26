//! Catálogo del Debloater: apps Appx que se pueden quitar y ajustes de registro de privacidad y telemetría.

use crate::core::platform::registry::RegHive;

pub const DEBLOAT_APP_CATALOG: &[DebloatAppSpec] = &[
    DebloatAppSpec {
        id: "bing_news",
        package_name: "Microsoft.BingNews",
        display_name: "MSN Noticias",
        description: "Feed de noticias preinstalado con actualizaciones en segundo plano",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "bing_weather",
        package_name: "Microsoft.BingWeather",
        display_name: "El Tiempo (MSN Weather)",
        description: "Aplicación de clima con procesos en segundo plano",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "bing_search",
        package_name: "Microsoft.BingSearch",
        display_name: "Búsqueda Web Bing",
        description: "Integración de búsquedas web de Bing en el sistema",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "get_help",
        package_name: "Microsoft.GetHelp",
        display_name: "Obtener Ayuda (Get Help)",
        description: "Asistente de soporte preinstalado de Microsoft",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "get_started",
        package_name: "Microsoft.Getstarted",
        display_name: "Sugerencias / Primeros Pasos",
        description: "Notificaciones y guías de bienvenida de Windows",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "office_hub",
        package_name: "Microsoft.MicrosoftOfficeHub",
        display_name: "Hub promocional de Microsoft 365",
        description: "Acceso promocional a suscripción de Office (no borra Office instalado)",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "solitaire",
        package_name: "Microsoft.MicrosoftSolitaireCollection",
        display_name: "Microsoft Solitaire Collection",
        description: "Juego preinstalado con publicidad",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "people",
        package_name: "Microsoft.People",
        display_name: "Contactos (People)",
        description: "Libreta de contactos integrada heredada",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "power_automate",
        package_name: "Microsoft.PowerAutomateDesktop",
        display_name: "Power Automate Desktop",
        description: "Automatización de flujos empresariales preinstalada en Windows 11",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "todos",
        package_name: "Microsoft.Todos",
        display_name: "Microsoft To Do",
        description: "Lista de tareas sincronizada con cuenta Microsoft",
        category: DebloatCategory::Recommended,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "feedback_hub",
        package_name: "Microsoft.WindowsFeedbackHub",
        display_name: "Centro de Opiniones (Feedback Hub)",
        description: "Recopilación y envío de comentarios y diagnósticos",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "maps",
        package_name: "Microsoft.WindowsMaps",
        display_name: "Mapas de Windows",
        description: "Aplicación de mapas sin conexión con servicios en segundo plano",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "your_phone",
        package_name: "Microsoft.YourPhone",
        display_name: "Enlace Móvil (Phone Link)",
        description: "Sincronización continua con Android/iOS en segundo plano",
        category: DebloatCategory::Recommended,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "zune_music",
        package_name: "Microsoft.ZuneMusic",
        display_name: "Reproductor Multimedia / Groove",
        description: "Reproductor UWP preinstalado",
        category: DebloatCategory::Recommended,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "zune_video",
        package_name: "Microsoft.ZuneVideo",
        display_name: "Películas y TV",
        description: "Tienda/reproductor de alquiler de vídeo de Microsoft",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "clipchamp",
        package_name: "Clipchamp.Clipchamp",
        display_name: "Microsoft Clipchamp",
        description: "Editor de vídeo web preinstalado en Windows 11",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "teams_consumer",
        package_name: "MSTeams",
        display_name: "Microsoft Teams (Personal)",
        description: "Cliente preinstalado de Teams que arranca en segundo plano",
        category: DebloatCategory::Recommended,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "copilot_app",
        package_name: "Microsoft.Copilot",
        display_name: "Microsoft Copilot (App UWP)",
        description: "Aplicación asistente IA residente",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "cortana",
        package_name: "Microsoft.549981C3F5F10",
        display_name: "Cortana",
        description: "Asistente heredado descontinuado",
        category: DebloatCategory::Recommended,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "quick_assist",
        package_name: "MicrosoftCorporationII.QuickAssist",
        display_name: "Asistencia Rápida",
        description: "Herramienta de soporte remoto UWP",
        category: DebloatCategory::Recommended,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "new_outlook",
        package_name: "Microsoft.OutlookForWindows",
        display_name: "Nuevo Outlook para Windows",
        description: "Cliente web de Outlook preinstalado con anuncios",
        category: DebloatCategory::Recommended,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "sound_recorder",
        package_name: "Microsoft.WindowsSoundRecorder",
        display_name: "Grabadora de Voz",
        description: "Grabadora básica de audio",
        category: DebloatCategory::OptionalApps,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "camera",
        package_name: "Microsoft.WindowsCamera",
        display_name: "Cámara de Windows",
        description: "App UWP de cámara",
        category: DebloatCategory::OptionalApps,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "alarms",
        package_name: "Microsoft.WindowsAlarms",
        display_name: "Reloj y Alarmas",
        description: "App de temporizadores y alarmas",
        category: DebloatCategory::OptionalApps,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "mixed_reality",
        package_name: "Microsoft.MixedReality.Portal",
        display_name: "Portal de Realidad Mixta",
        description: "Entorno VR heredado de Windows",
        category: DebloatCategory::OptionalApps,
        default_selected: true,
    },
    DebloatAppSpec {
        id: "xbox_app",
        package_name: "Microsoft.GamingApp",
        display_name: "App de Xbox (Game Pass)",
        description: "Necesaria si utilizas PC Game Pass",
        category: DebloatCategory::XboxGaming,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "xbox_gamebar",
        package_name: "Microsoft.XboxGamingOverlay",
        display_name: "Xbox Game Bar (Win+G)",
        description: "Overlay y captura de Xbox (necesaria para invitaciones/Game Pass)",
        category: DebloatCategory::XboxGaming,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "xbox_tcui",
        package_name: "Microsoft.Xbox.TCUI",
        display_name: "Xbox Live UI",
        description: "Interfaz de inicio de sesión Xbox en juegos",
        category: DebloatCategory::XboxGaming,
        default_selected: false,
    },
    DebloatAppSpec {
        id: "xbox_speech",
        package_name: "Microsoft.XboxSpeechToTextOverlay",
        display_name: "Xbox Speech to Text",
        description: "Accesibilidad de voz a texto en juegos Xbox",
        category: DebloatCategory::XboxGaming,
        default_selected: false,
    },
];

pub const DEBLOAT_REG_TWEAKS: &[DebloatRegTweakSpec] = &[
    DebloatRegTweakSpec {
        id: "no_start_ads",
        label: "Desactivar anuncios, sugerencias e instalación silenciosa de apps",
        description: "Deshabilita ContentDeliveryManager (apps patrocinadas, sugerencias en Inicio y pantalla de bloqueo)",
        default_selected: true,
        entries: &[
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SilentInstalledAppsEnabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SystemPaneSuggestionsEnabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SoftLandingEnabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SubscribedContent-338388Enabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SubscribedContent-338389Enabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SubscribedContent-353698Enabled",
                target_dword: 0,
            },
        ],
    },
    DebloatRegTweakSpec {
        id: "no_bing_search",
        label: "Desactivar búsqueda web de Bing en la barra de Windows",
        description: "Evita que la búsqueda local del Menú Inicio envíe consultas a Bing",
        default_selected: true,
        entries: &[
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Policies\Microsoft\Windows\Explorer",
                value_name: "DisableSearchBoxSuggestions",
                target_dword: 1,
            },
            DebloatRegEntry {
                hive: RegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\Search",
                value_name: "BingSearchEnabled",
                target_dword: 0,
            },
        ],
    },
    DebloatRegTweakSpec {
        id: "no_widgets",
        label: "Desactivar Widgets y Noticias e Intereses",
        description: "Oculta y detiene el panel de noticias/clima en la barra de tareas",
        default_selected: true,
        entries: &[DebloatRegEntry {
            hive: RegHive::Hklm,
            subkey: r"SOFTWARE\Policies\Microsoft\Dsh",
            value_name: "AllowNewsAndInterests",
            target_dword: 0,
        }],
    },
    DebloatRegTweakSpec {
        id: "no_explorer_ads",
        label: "Ocultar anuncios y notificaciones de proveedores en el Explorador",
        description: "Fija ShowSyncProviderNotifications=0 en el Explorador de archivos",
        default_selected: true,
        entries: &[DebloatRegEntry {
            hive: RegHive::Hkcu,
            subkey: r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
            value_name: "ShowSyncProviderNotifications",
            target_dword: 0,
        }],
    },
    DebloatRegTweakSpec {
        id: "no_copilot_policy",
        label: "Desactivar Windows Copilot mediante política de usuario",
        description: "Fija TurnOffWindowsCopilot=1 (reversible)",
        default_selected: true,
        entries: &[DebloatRegEntry {
            hive: RegHive::Hkcu,
            subkey: r"Software\Policies\Microsoft\Windows\WindowsCopilot",
            value_name: "TurnOffWindowsCopilot",
            target_dword: 1,
        }],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebloatCategory {
    Recommended,
    OptionalApps,
    XboxGaming,
}

impl DebloatCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Recommended => "Bloatware común y telemetría (recomendado)",
            Self::OptionalApps => "Apps integradas opcionales",
            Self::XboxGaming => "Xbox y juegos (no las marques si usas Xbox Game Pass)",
        }
    }
}

/// SYS-20: solo se preseleccionan apps sin uso real habitual; Outlook, To Do, Enlace Móvil y Teams
/// quedan desmarcadas porque mucha gente las usa.
#[derive(Debug, Clone)]
pub struct DebloatAppSpec {
    pub id: &'static str,
    pub package_name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub category: DebloatCategory,
    pub default_selected: bool,
}

#[derive(Debug, Clone)]
pub struct DebloatRegEntry {
    pub hive: RegHive,
    pub subkey: &'static str,
    pub value_name: &'static str,
    pub target_dword: u32,
}

#[derive(Debug, Clone)]
pub struct DebloatRegTweakSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub entries: &'static [DebloatRegEntry],
    pub default_selected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SYS-20: Outlook, To Do, Enlace Móvil y Teams no vienen preseleccionadas.
    #[test]
    fn apps_de_uso_real_no_vienen_marcadas() {
        for name in [
            "Microsoft.OutlookForWindows",
            "Microsoft.Todos",
            "Microsoft.YourPhone",
            "MSTeams",
        ] {
            let spec = DEBLOAT_APP_CATALOG
                .iter()
                .find(|a| a.package_name == name)
                .expect(name);
            assert!(!spec.default_selected, "{} viene preseleccionada", name);
        }
    }
}
