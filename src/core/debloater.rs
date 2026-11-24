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
use winreg::RegKey;

use crate::core::elevation::ElevationManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::safe_fs::{atomic_write_new, delete_verified_file, read_bounded_bytes};
use crate::core::trust::ensure_admin_only_data_dir;
use crate::core::win_common::NamedMutex;

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
        default_selected: true,
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
        default_selected: true,
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
        default_selected: true,
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
        default_selected: true,
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
    previous_dword: Option<u32>,
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
    let len = unsafe {
        windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut buf))
    } as usize;
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

fn run_ps_cancellable(ps_path: &Path, script: &str, cancel: &AtomicBool, timeout_ms: u64) -> CoreResult<()> {
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
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CoreError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let out = child.wait_with_output()?;
                let err_txt = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return Err(CoreError::SystemTool {
                    message: if err_txt.is_empty() {
                        format!("PowerShell finalizó con código {:?}", status.code())
                    } else {
                        err_txt
                    },
                    source: None,
                });
            }
            Ok(None) => {
                if started.elapsed() >= Duration::from_millis(timeout_ms) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CoreError::SystemTool {
                        message: "Tiempo límite excedido ejecutando desinstalación Appx".into(),
                        source: None,
                    });
                }
                thread::sleep(Duration::from_millis(150));
            }
            Err(e) => return Err(CoreError::Io(e)),
        }
    }
}

fn debloat_backup_path() -> CoreResult<PathBuf> {
    Ok(ensure_admin_only_data_dir()?.join("debloat_backup.json"))
}

pub struct DebloaterManager;

impl DebloaterManager {
    pub fn scan_system() -> CoreResult<DebloatScanResult> {
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
        if ps_installed.status.success() {
            let txt = String::from_utf8_lossy(&ps_installed.stdout);
            if let Ok(entries) = serde_json::from_str::<Vec<PsAppxEntry>>(txt.trim()) {
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
        if ps_prov.status.success() {
            let txt = String::from_utf8_lossy(&ps_prov.stdout);
            if let Ok(entries) = serde_json::from_str::<Vec<PsAppxEntry>>(txt.trim()) {
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
                return Err(CoreError::Cancelled);
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

            for full_name in &app.full_names {
                if !is_valid_package_ident(full_name) {
                    continue;
                }
                let cmd = format!(
                    "try {{ Remove-AppxPackage -Package '{}' -AllUsers -ErrorAction Stop }} catch {{ Remove-AppxPackage -Package '{}' -ErrorAction Stop }}",
                    full_name, full_name
                );
                if let Err(e) = run_ps_cancellable(&ps_path, &cmd, cancel, 90_000) {
                    if matches!(e, CoreError::Cancelled) {
                        return Err(e);
                    }
                    app_ok = false;
                    last_err = e.to_string();
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
                if let Err(e) = run_ps_cancellable(&ps_path, &cmd, cancel, 90_000) {
                    if matches!(e, CoreError::Cancelled) {
                        return Err(e);
                    }
                    app_ok = false;
                    last_err = e.to_string();
                }
            }

            if app_ok {
                report.removed_apps += 1;
                report.outcomes.push(DebloatItemOutcome {
                    label: app.display_name.to_string(),
                    success: true,
                    message: "Eliminada y desaprovisionada".into(),
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
                return Err(CoreError::Cancelled);
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

    fn backup_reg_entries(entries: &[DebloatRegEntry]) -> CoreResult<()> {
        let backup_file = debloat_backup_path()?;
        let mut doc = if backup_file.exists() {
            read_bounded_bytes(&backup_file, MAX_DEBLOAT_BACKUP_BYTES)
                .ok()
                .and_then(|b| serde_json::from_slice::<DebloatBackupDoc>(&b).ok())
                .unwrap_or(DebloatBackupDoc {
                    version: 1,
                    entries: Vec::new(),
                })
        } else {
            DebloatBackupDoc {
                version: 1,
                entries: Vec::new(),
            }
        };

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

            let prev = root
                .open_subkey_with_flags(entry.subkey, flags)
                .ok()
                .and_then(|k| k.get_value::<u32, _>(entry.value_name).ok());

            doc.entries.push(DebloatRegSnapshot {
                hive: entry.hive,
                subkey: entry.subkey.to_string(),
                value_name: entry.value_name.to_string(),
                existed: prev.is_some(),
                previous_dword: prev,
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

        let bytes = read_bounded_bytes(&backup_file, MAX_DEBLOAT_BACKUP_BYTES)?;
        let doc: DebloatBackupDoc =
            serde_json::from_slice(&bytes).map_err(|e| CoreError::Backup {
                message: format!("Error leyendo debloat_backup.json: {}", e),
                source: Some(Box::new(e)),
            })?;

        let mut restored = 0usize;
        for snap in doc.entries.iter().rev() {
            let root = RegKey::predef(match snap.hive {
                DebloatRegHive::Hkcu => HKEY_CURRENT_USER,
                DebloatRegHive::Hklm => HKEY_LOCAL_MACHINE,
            });
            let flags = match snap.hive {
                DebloatRegHive::Hkcu => KEY_WRITE,
                DebloatRegHive::Hklm => KEY_WRITE | KEY_WOW64_64KEY,
            };

            if let Ok(key) = root.open_subkey_with_flags(&snap.subkey, flags) {
                if snap.existed {
                    if let Some(prev) = snap.previous_dword {
                        let _ = key.set_value(&snap.value_name, &prev);
                    }
                } else {
                    let _ = key.delete_value(&snap.value_name);
                }
                restored += 1;
            }
        }

        delete_verified_file(&backup_file)?;
        Ok(restored)
    }
}