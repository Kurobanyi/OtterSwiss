use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE};
use winreg::{RegKey, RegValue};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation::ElevationManager;
use crate::core::platform::mutex::NamedMutex;
use crate::core::platform::registry::{reg_type_to_u32, u32_to_reg_type};
use crate::core::platform::safe_fs::{atomic_write_new, delete_verified_file, read_bounded_bytes};
use crate::core::security::trust::ensure_admin_only_data_dir;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DEBLOAT_MUTEX: &str = r"Global\OtterSwiss_Debloat_Lock";
const MAX_DEBLOAT_BACKUP_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebloatCategory {
    Recommended,
    OptionalApps,
    XboxGaming,
}

impl DebloatCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Recommended => "Bloatware Común y Telemetría (Recomendado)",
            Self::OptionalApps => "Aplicaciones Integradas Opcionales",
            Self::XboxGaming => "Xbox y Gaming (NO marcar si usas Xbox Game Pass)",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebloatRegHive {
    Hkcu,
    Hklm,
}

#[derive(Debug, Clone)]
pub struct DebloatRegEntry {
    pub hive: DebloatRegHive,
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

pub const DEBLOAT_REG_TWEAKS: &[DebloatRegTweakSpec] = &[
    DebloatRegTweakSpec {
        id: "no_start_ads",
        label: "Desactivar anuncios, sugerencias e instalación silenciosa de apps",
        description: "Deshabilita ContentDeliveryManager (apps patrocinadas, sugerencias en Inicio y pantalla de bloqueo)",
        default_selected: true,
        entries: &[
            DebloatRegEntry {
                hive: DebloatRegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SilentInstalledAppsEnabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: DebloatRegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SystemPaneSuggestionsEnabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: DebloatRegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SoftLandingEnabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: DebloatRegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SubscribedContent-338388Enabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: DebloatRegHive::Hkcu,
                subkey: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
                value_name: "SubscribedContent-338389Enabled",
                target_dword: 0,
            },
            DebloatRegEntry {
                hive: DebloatRegHive::Hkcu,
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
                hive: DebloatRegHive::Hkcu,
                subkey: r"Software\Policies\Microsoft\Windows\Explorer",
                value_name: "DisableSearchBoxSuggestions",
                target_dword: 1,
            },
            DebloatRegEntry {
                hive: DebloatRegHive::Hkcu,
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
            hive: DebloatRegHive::Hklm,
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
            hive: DebloatRegHive::Hkcu,
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
            hive: DebloatRegHive::Hkcu,
            subkey: r"Software\Policies\Microsoft\Windows\WindowsCopilot",
            value_name: "TurnOffWindowsCopilot",
            target_dword: 1,
        }],
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedBloatApp {
    pub id: &'static str,
    pub package_name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub category: DebloatCategory,
    pub default_selected: bool,
    pub installed: bool,
    pub full_names: Vec<String>,
    pub provisioned_names: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DebloatScanResult {
    pub apps: Vec<DetectedBloatApp>,
    pub has_reg_backup: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DebloatProgress {
    pub current_step: usize,
    pub total_steps: usize,
    pub current_label: String,
    pub percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebloatItemOutcome {
    pub label: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct DebloatReport {
    /// `true` si el usuario canceló: el informe incluye lo que ya se hizo antes de cancelar.
    pub cancelled: bool,
    pub removed_apps: usize,
    pub applied_reg_tweaks: usize,
    pub failed_count: usize,
    pub outcomes: Vec<DebloatItemOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DebloatRegSnapshot {
    hive: DebloatRegHive,
    subkey: String,
    value_name: String,
    existed: bool,
    /// Formato antiguo (solo DWORD). Se conserva para leer respaldos de versiones previas.
    #[serde(default)]
    previous_dword: Option<u32>,
    /// Tipo y bytes exactos del valor original, sea cual sea su tipo.
    #[serde(default)]
    previous_type: Option<u32>,
    #[serde(default)]
    previous_raw: Option<Vec<u8>>,
}

impl DebloatRegSnapshot {
    /// La entrada solo es válida si corresponde a un ajuste del catálogo embebido. Cualquier otra clave
    /// en el respaldo solo puede venir de un archivo manipulado y nunca se escribe en el registro.
    fn catalog_entry(&self) -> Option<&'static DebloatRegEntry> {
        DEBLOAT_REG_TWEAKS
            .iter()
            .flat_map(|t| t.entries.iter())
            .find(|e| {
                e.hive == self.hive
                    && e.subkey.eq_ignore_ascii_case(&self.subkey)
                    && e.value_name.eq_ignore_ascii_case(&self.value_name)
            })
    }

    fn is_valid(&self) -> bool {
        if self.catalog_entry().is_none() {
            return false;
        }
        if !self.existed {
            return self.previous_raw.is_none();
        }
        match (&self.previous_raw, self.previous_type, self.previous_dword) {
            (Some(raw), Some(t), _) => raw.len() <= 1024 && [1, 2, 3, 4, 11].contains(&t),
            (None, None, Some(_)) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DebloatBackupDoc {
    version: u32,
    entries: Vec<DebloatRegSnapshot>,
}

#[derive(Debug, Deserialize)]
struct PsAppxEntry {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "PackageFullName")]
    package_full_name: Option<String>,
    #[serde(rename = "DisplayName")]
    display_name: Option<String>,
    #[serde(rename = "PackageName")]
    package_name: Option<String>,
}

fn get_powershell_path() -> CoreResult<PathBuf> {
    let mut buf = [0u16; 260];
    let len =
        unsafe { windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut buf)) }
            as usize;
    if len == 0 || len >= buf.len() {
        return Err(CoreError::SystemTool {
            message: "No se pudo localizar System32 para PowerShell".into(),
            source: None,
        });
    }
    let sys_dir = String::from_utf16_lossy(&buf[..len]);
    let ps = Path::new(&sys_dir).join(r"WindowsPowerShell\v1.0\powershell.exe");
    if !ps.is_file() {
        return Err(CoreError::SystemTool {
            message: format!("No se encontró powershell.exe en '{}'", ps.display()),
            source: None,
        });
    }
    Ok(ps)
}

fn is_valid_package_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 220
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~'))
}

/// Ejecuta un script de PowerShell y devuelve su salida estándar. Las salidas se leen en hilos mientras el
/// proceso corre: si solo se leyeran al final, un error largo llenaría el pipe y PowerShell se quedaría
/// bloqueado hasta el timeout (ERR-01).
fn run_ps(ps_path: &Path, script: &str, timeout_ms: u64) -> CoreResult<String> {
    use std::io::Read;

    let mut child = Command::new(ps_path)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= Duration::from_millis(timeout_ms) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CoreError::SystemTool {
                        message: "Tiempo límite excedido ejecutando PowerShell".into(),
                        source: None,
                    });
                }
                thread::sleep(Duration::from_millis(150));
            }
            Err(e) => return Err(CoreError::Io(e)),
        }
    };

    let stdout = String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default())
        .trim()
        .to_string();
    if status.success() {
        Ok(stdout)
    } else {
        Err(CoreError::SystemTool {
            message: if stderr.is_empty() {
                format!("PowerShell finalizó con código {:?}", status.code())
            } else {
                stderr
            },
            source: None,
        })
    }
}

fn ps_scan_output(out: &std::process::Output, what: &str) -> CoreResult<String> {
    if !out.status.success() {
        return Err(CoreError::SystemTool {
            message: format!(
                "{} falló: {}",
                what,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            source: None,
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `ConvertTo-Json` devuelve un objeto suelto (no una lista) cuando hay un solo elemento.
fn parse_ps_json(txt: &str, what: &str) -> CoreResult<Vec<PsAppxEntry>> {
    let t = txt.trim();
    let parsed = if t.starts_with('{') {
        serde_json::from_str::<PsAppxEntry>(t).map(|e| vec![e])
    } else {
        serde_json::from_str::<Vec<PsAppxEntry>>(t)
    };
    parsed.map_err(|e| CoreError::SystemTool {
        message: format!("No se pudo interpretar la salida de {}: {}", what, e),
        source: None,
    })
}

fn debloat_backup_path() -> CoreResult<PathBuf> {
    Ok(ensure_admin_only_data_dir()?.join("debloat_backup.json"))
}

pub struct DebloaterManager;

impl DebloaterManager {
    pub fn scan_system() -> CoreResult<DebloatScanResult> {
        // ERR-05: sin elevación `Get-AppxPackage -AllUsers` falla con acceso denegado y, con
        // `-ErrorAction SilentlyContinue`, el escaneo diría «nada instalado».
        ElevationManager::require_elevated("Escanear las apps instaladas para todos los usuarios")?;
        let ps_path = get_powershell_path()?;

        let ps_installed = Command::new(&ps_path)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $p = @(Get-AppxPackage -AllUsers -ErrorAction SilentlyContinue | Select-Object Name, PackageFullName); if ($p.Count -eq 0) { '[]' } else { ConvertTo-Json -InputObject $p -Compress }",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        let mut full_map: HashMap<String, Vec<String>> = HashMap::new();
        {
            let txt = ps_scan_output(&ps_installed, "Get-AppxPackage")?;
            let entries = parse_ps_json(&txt, "Get-AppxPackage")?;
            {
                for e in entries {
                    if let (Some(n), Some(full)) = (e.name, e.package_full_name) {
                        if is_valid_package_ident(&full) {
                            let vec = full_map.entry(n.to_ascii_lowercase()).or_default();
                            if !vec.contains(&full) {
                                vec.push(full);
                            }
                        }
                    }
                }
            }
        }

        let ps_prov = Command::new(&ps_path)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $p = @(Get-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue | Select-Object DisplayName, PackageName); if ($p.Count -eq 0) { '[]' } else { ConvertTo-Json -InputObject $p -Compress }",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        let mut prov_map: HashMap<String, Vec<String>> = HashMap::new();
        {
            let txt = ps_scan_output(&ps_prov, "Get-AppxProvisionedPackage")?;
            let entries = parse_ps_json(&txt, "Get-AppxProvisionedPackage")?;
            {
                for e in entries {
                    if let (Some(dn), Some(pn)) = (e.display_name, e.package_name) {
                        if is_valid_package_ident(&pn) {
                            let vec = prov_map.entry(dn.to_ascii_lowercase()).or_default();
                            if !vec.contains(&pn) {
                                vec.push(pn);
                            }
                        }
                    }
                }
            }
        }

        let apps = DEBLOAT_APP_CATALOG
            .iter()
            .map(|spec| {
                let key = spec.package_name.to_ascii_lowercase();
                let full_names = full_map.get(&key).cloned().unwrap_or_default();
                let provisioned_names = prov_map.get(&key).cloned().unwrap_or_default();
                let installed = !full_names.is_empty() || !provisioned_names.is_empty();
                DetectedBloatApp {
                    id: spec.id,
                    package_name: spec.package_name,
                    display_name: spec.display_name,
                    description: spec.description,
                    category: spec.category,
                    default_selected: spec.default_selected,
                    installed,
                    full_names,
                    provisioned_names,
                }
            })
            .collect();

        let has_reg_backup = debloat_backup_path().map(|p| p.exists()).unwrap_or(false);

        Ok(DebloatScanResult {
            apps,
            has_reg_backup,
        })
    }

    pub fn execute_debloat(
        apps_to_remove: &[DetectedBloatApp],
        reg_tweak_ids: &[&'static str],
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<DebloatProgress>>>,
    ) -> CoreResult<DebloatReport> {
        ElevationManager::require_elevated("Ejecutar Debloater de Windows")?;
        let _lock = NamedMutex::acquire(DEBLOAT_MUTEX)?;
        let ps_path = get_powershell_path()?;

        let mut report = DebloatReport::default();
        let total_steps = (apps_to_remove.len() + reg_tweak_ids.len()).max(1);
        let mut step_idx = 0usize;

        let push_prog = |idx: usize, label: &str| {
            if let Some(p_arc) = progress {
                if let Ok(mut p) = p_arc.lock() {
                    p.current_step = idx;
                    p.total_steps = total_steps;
                    p.current_label = label.to_string();
                    p.percent = ((idx * 100) / total_steps).min(100) as u8;
                }
            }
        };

        for app in apps_to_remove {
            if cancel.load(Ordering::Acquire) {
                report.cancelled = true;
                return Ok(report);
            }

            step_idx += 1;
            push_prog(
                step_idx.saturating_sub(1),
                &format!("Eliminando {}...", app.display_name),
            );

            if !DEBLOAT_APP_CATALOG
                .iter()
                .any(|s| s.id == app.id && s.package_name == app.package_name)
            {
                report.failed_count += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: app.display_name.to_string(),
                    success: false,
                    message: "Rechazado: paquete fuera del catálogo autorizado".into(),
                });
                continue;
            }

            let mut app_ok = true;
            let mut last_err = String::new();
            let mut only_current_user = false;

            for full_name in &app.full_names {
                if !is_valid_package_ident(full_name) {
                    continue;
                }
                // Cada paquete se deja terminar: cancelar a mitad de Remove-AppxPackage puede dejarlo roto.
                let cmd = format!(
                    "try {{ Remove-AppxPackage -Package '{}' -AllUsers -ErrorAction Stop; 'SCOPE=ALL' }} catch {{ Remove-AppxPackage -Package '{}' -ErrorAction Stop; 'SCOPE=USER' }}",
                    full_name, full_name
                );
                match run_ps(&ps_path, &cmd, 90_000) {
                    Ok(out) if out.contains("SCOPE=USER") => only_current_user = true,
                    Ok(_) => {}
                    Err(e) => {
                        app_ok = false;
                        last_err = e.to_string();
                    }
                }
            }

            for prov_name in &app.provisioned_names {
                if !is_valid_package_ident(prov_name) {
                    continue;
                }
                let cmd = format!(
                    "Remove-AppxProvisionedPackage -Online -PackageName '{}' -ErrorAction Stop | Out-Null",
                    prov_name
                );
                if let Err(e) = run_ps(&ps_path, &cmd, 90_000) {
                    app_ok = false;
                    last_err = e.to_string();
                }
            }

            if app_ok {
                report.removed_apps += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: app.display_name.to_string(),
                    success: true,
                    message: if only_current_user {
                        "Eliminada solo para el usuario actual (otros perfiles la conservan)".into()
                    } else if app.provisioned_names.is_empty() {
                        "Eliminada para todos los usuarios".into()
                    } else {
                        "Eliminada para todos los usuarios y desaprovisionada".into()
                    },
                });
            } else {
                report.failed_count += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: app.display_name.to_string(),
                    success: false,
                    message: last_err,
                });
            }
            push_prog(step_idx, &format!("Procesado {}", app.display_name));
        }

        for &tweak_id in reg_tweak_ids {
            if cancel.load(Ordering::Acquire) {
                report.cancelled = true;
                return Ok(report);
            }
            step_idx += 1;

            let Some(spec) = DEBLOAT_REG_TWEAKS.iter().find(|t| t.id == tweak_id) else {
                continue;
            };
            push_prog(step_idx.saturating_sub(1), spec.label);

            if let Err(e) = Self::backup_reg_entries(spec.entries) {
                report.failed_count += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: spec.label.to_string(),
                    success: false,
                    message: format!("Error guardando respaldo: {}", e),
                });
                continue;
            }

            match Self::apply_reg_entries(spec.entries) {
                Ok(()) => {
                    report.applied_reg_tweaks += 1;
                    report.outcomes.push(DebloatItemOutcome {
                        label: spec.label.to_string(),
                        success: true,
                        message: "Aplicado en Registro con respaldo".into(),
                    });
                }
                Err(e) => {
                    report.failed_count += 1;
                    report.outcomes.push(DebloatItemOutcome {
                        label: spec.label.to_string(),
                        success: false,
                        message: e.to_string(),
                    });
                }
            }
            push_prog(step_idx, spec.label);
        }

        push_prog(total_steps, "Debloat finalizado");
        Ok(report)
    }

    /// Lee `debloat_backup.json`. Si existe y no se puede leer, falla (nunca se asume vacío); si trae
    /// entradas que no son del catálogo, falla: el archivo fue manipulado.
    fn load_backup_doc(path: &Path) -> CoreResult<DebloatBackupDoc> {
        if !path.exists() {
            return Ok(DebloatBackupDoc {
                version: 2,
                entries: Vec::new(),
            });
        }
        let bytes = read_bounded_bytes(path, MAX_DEBLOAT_BACKUP_BYTES)?;
        let doc: DebloatBackupDoc =
            serde_json::from_slice(&bytes).map_err(|e| CoreError::Backup {
                message: format!(
                    "debloat_backup.json está dañado ({}); no se tocará el registro para no perder el estado original",
                    e
                ),
                source: Some(Box::new(e)),
            })?;
        if let Some(bad) = doc.entries.iter().find(|e| !e.is_valid()) {
            return Err(CoreError::Backup {
                message: format!(
                    "debloat_backup.json contiene una entrada que no es del catálogo ({:?}\\{}\\{}); posible manipulación",
                    bad.hive, bad.subkey, bad.value_name
                ),
                source: None,
            });
        }
        Ok(doc)
    }

    fn backup_reg_entries(entries: &[DebloatRegEntry]) -> CoreResult<()> {
        let backup_file = debloat_backup_path()?;
        let mut doc = Self::load_backup_doc(&backup_file)?;
        doc.version = 2;

        for entry in entries {
            let already = doc.entries.iter().any(|s| {
                s.hive == entry.hive
                    && s.subkey.eq_ignore_ascii_case(entry.subkey)
                    && s.value_name.eq_ignore_ascii_case(entry.value_name)
            });
            if already {
                continue;
            }

            let root = RegKey::predef(match entry.hive {
                DebloatRegHive::Hkcu => HKEY_CURRENT_USER,
                DebloatRegHive::Hklm => HKEY_LOCAL_MACHINE,
            });
            let flags = match entry.hive {
                DebloatRegHive::Hkcu => KEY_READ,
                DebloatRegHive::Hklm => KEY_READ | KEY_WOW64_64KEY,
            };

            // Distinguir «no existe» de un error real: un error no puede guardarse como «no existía».
            let prev = match root.open_subkey_with_flags(entry.subkey, flags) {
                Ok(key) => match key.get_raw_value(entry.value_name) {
                    Ok(raw) => Some(raw),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => {
                        return Err(CoreError::Registry {
                            path: format!("{}\\{}", entry.subkey, entry.value_name),
                            message: format!("No se pudo leer el valor original: {}", e),
                            source: Some(e),
                        })
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    return Err(CoreError::Registry {
                        path: entry.subkey.to_string(),
                        message: format!("No se pudo abrir la clave original: {}", e),
                        source: Some(e),
                    })
                }
            };

            doc.entries.push(DebloatRegSnapshot {
                hive: entry.hive,
                subkey: entry.subkey.to_string(),
                value_name: entry.value_name.to_string(),
                existed: prev.is_some(),
                previous_dword: None,
                previous_type: prev.as_ref().map(|r| reg_type_to_u32(&r.vtype)),
                previous_raw: prev.map(|r| r.bytes),
            });
        }

        let json = serde_json::to_vec_pretty(&doc).map_err(|e| CoreError::Backup {
            message: format!("Error serializando debloat_backup.json: {}", e),
            source: Some(Box::new(e)),
        })?;
        atomic_write_new(&backup_file, &json)
    }

    fn apply_reg_entries(entries: &[DebloatRegEntry]) -> CoreResult<()> {
        for entry in entries {
            let root = RegKey::predef(match entry.hive {
                DebloatRegHive::Hkcu => HKEY_CURRENT_USER,
                DebloatRegHive::Hklm => HKEY_LOCAL_MACHINE,
            });
            let flags = match entry.hive {
                DebloatRegHive::Hkcu => KEY_WRITE,
                DebloatRegHive::Hklm => KEY_WRITE | KEY_WOW64_64KEY,
            };
            let (key, _) = root
                .create_subkey_with_flags(entry.subkey, flags)
                .map_err(|e| CoreError::Registry {
                    path: entry.subkey.to_string(),
                    message: format!("Error creando clave debloat: {}", e),
                    source: Some(e),
                })?;
            key.set_value(entry.value_name, &entry.target_dword)?;
        }
        Ok(())
    }

    /// `true` si hay ajustes de registro del Debloater pendientes de revertir.
    pub fn has_registry_backup() -> bool {
        debloat_backup_path().map(|p| p.exists()).unwrap_or(false)
    }

    /// Revierte los ajustes de registro del Debloater. Solo toca claves del catálogo, propaga cada error
    /// y conserva el respaldo si algo no se pudo revertir, para poder reintentar.
    pub fn revert_registry_debloat() -> CoreResult<usize> {
        ElevationManager::require_elevated("Revertir ajustes de registro del Debloater")?;
        let _lock = NamedMutex::acquire(DEBLOAT_MUTEX)?;
        let backup_file = debloat_backup_path()?;
        if !backup_file.exists() {
            return Err(CoreError::Backup {
                message: "No existe respaldo de ajustes del Debloater".into(),
                source: None,
            });
        }

        let doc = Self::load_backup_doc(&backup_file)?;

        let mut restored = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for snap in doc.entries.iter().rev() {
            match Self::restore_snapshot(snap) {
                Ok(()) => restored += 1,
                Err(e) => failures.push(format!("{}\\{}: {}", snap.subkey, snap.value_name, e)),
            }
        }

        if !failures.is_empty() {
            return Err(CoreError::Backup {
                message: format!(
                    "Se revirtieron {} ajustes, pero {} fallaron y el respaldo se conserva: {}",
                    restored,
                    failures.len(),
                    failures.join("; ")
                ),
                source: None,
            });
        }

        delete_verified_file(&backup_file)?;
        Ok(restored)
    }

    fn restore_snapshot(snap: &DebloatRegSnapshot) -> CoreResult<()> {
        let entry = snap.catalog_entry().ok_or_else(|| {
            CoreError::Validation("Entrada fuera del catálogo del Debloater".into())
        })?;
        let root = RegKey::predef(match entry.hive {
            DebloatRegHive::Hkcu => HKEY_CURRENT_USER,
            DebloatRegHive::Hklm => HKEY_LOCAL_MACHINE,
        });
        let flags = match entry.hive {
            DebloatRegHive::Hkcu => KEY_READ | KEY_WRITE,
            DebloatRegHive::Hklm => KEY_READ | KEY_WRITE | KEY_WOW64_64KEY,
        };

        if snap.existed {
            let (key, _) = root.create_subkey_with_flags(entry.subkey, flags)?;
            match (&snap.previous_raw, snap.previous_type, snap.previous_dword) {
                (Some(raw), Some(t), _) => key.set_raw_value(
                    entry.value_name,
                    &RegValue {
                        bytes: raw.clone(),
                        vtype: u32_to_reg_type(t),
                    },
                )?,
                (None, None, Some(dword)) => key.set_value(entry.value_name, &dword)?,
                _ => {
                    return Err(CoreError::Validation(
                        "Snapshot inconsistente: existía sin valor guardado".into(),
                    ))
                }
            }
            Ok(())
        } else {
            match root.open_subkey_with_flags(entry.subkey, flags) {
                Ok(key) => match key.delete_value(entry.value_name) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.into()),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> DebloatRegSnapshot {
        let entry = &DEBLOAT_REG_TWEAKS[0].entries[0];
        DebloatRegSnapshot {
            hive: entry.hive,
            subkey: entry.subkey.to_string(),
            value_name: entry.value_name.to_string(),
            existed: true,
            previous_dword: None,
            previous_type: Some(4),
            previous_raw: Some(vec![1, 0, 0, 0]),
        }
    }

    /// SEC-03 / SYS-11: solo se revierte lo del catálogo, con el tipo exacto o el formato viejo.
    #[test]
    fn snapshot_del_debloater_solo_del_catalogo() {
        assert!(snapshot().is_valid());

        let mut formato_viejo = snapshot();
        formato_viejo.previous_type = None;
        formato_viejo.previous_raw = None;
        formato_viejo.previous_dword = Some(1);
        assert!(formato_viejo.is_valid());

        let mut manipulado = snapshot();
        manipulado.subkey = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run".into();
        assert!(!manipulado.is_valid());

        let mut tipo_raro = snapshot();
        tipo_raro.previous_type = Some(7);
        assert!(!tipo_raro.is_valid());

        let mut no_existia_con_bytes = snapshot();
        no_existia_con_bytes.existed = false;
        assert!(!no_existia_con_bytes.is_valid());
    }

    #[test]
    fn identificadores_de_paquete_sin_inyeccion() {
        assert!(is_valid_package_ident(
            "Microsoft.BingNews_4.55.62231.0_x64__8wekyb3d8bbwe"
        ));
        assert!(!is_valid_package_ident("x'; Remove-Item C:\\ -Recurse; '"));
        assert!(!is_valid_package_ident(""));
        assert!(!is_valid_package_ident(&"a".repeat(300)));
    }

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
