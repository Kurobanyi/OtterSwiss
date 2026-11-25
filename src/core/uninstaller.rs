use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, KEY_READ, KEY_WOW64_32KEY,
    KEY_WOW64_64KEY, KEY_WRITE,
};
use winreg::RegKey;

use windows::core::PCWSTR;
use windows::Win32::System::Environment::ExpandEnvironmentStringsForUserW;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_Public, FOLDERID_RoamingAppData,
    SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

use crate::core::debloater::DebloaterManager;
use crate::core::dns::DnsManager;
use crate::core::elevation::ElevationManager;
use crate::core::engine::{OptimizationEngine, RollbackReport};
use crate::core::error::{CoreError, CoreResult};
use crate::core::process::{
    duplicate_desktop_shell_primary_token, run_in_job, system32_dir, Launch,
};
use crate::core::safe_fs::{
    canonical_dos_path, delete_tree_verified, delete_verified_file_under, is_safe_non_root_dir,
    path_is_strictly_under,
};
use crate::core::trust::{ensure_admin_only_data_dir, get_program_data_dir};
use crate::core::win_common::{
    program_files_dir, program_files_x86_dir, to_wide_null, wide_ptr_to_string_and_free, SafeHandle,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub display_name: String,
    pub display_version: String,
    pub publisher: String,
    pub uninstall_string: String,
    pub quiet_uninstall_string: Option<String>,
    pub install_location: Option<PathBuf>,
    pub display_icon: Option<PathBuf>,
    pub estimated_size_kb: Option<u64>,
    pub is_per_user: bool,
    pub is_wow64: bool,
    pub registry_subkey: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftoverKind {
    Directory,
    File,
    RegistryKey,
    RegistryValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftoverItem {
    pub id: String,
    pub kind: LeftoverKind,
    pub category: String,
    pub target_display: String,
    pub path: Option<PathBuf>,
    pub reg_is_hkcu: bool,
    pub reg_wow64_32: bool,
    pub reg_subkey: Option<String>,
    pub reg_value_name: Option<String>,
    pub size_bytes: u64,
    /// Si la UI lo marca por defecto. Solo lo es lo que con seguridad pertenece a la app: datos de usuario
    /// (AppData, HKCU) y carpetas de todo un fabricante se muestran pero quedan sin marcar.
    pub recommended: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PurgeReport {
    pub removed_count: usize,
    pub failed_count: usize,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
}

const PROTECTED_DIR_NAMES: &[&str] = &[
    "microsoft",
    "windows",
    "windows nt",
    "windowsapps",
    "common files",
    "system32",
    "syswow64",
    "otterswiss",
    "package cache",
    "temp",
    "tmp",
    "desktop",
    "documents",
    "downloads",
    "start menu",
    "programs",
    "users",
    "public",
    "programdata",
    "program files",
    "program files (x86)",
    "appdata",
    "local",
    "roaming",
    "internet explorer",
    "windows defender",
    "windows mail",
    "windows media player",
    "windows powershell",
    "msbuild",
    "dotnet",
    "nvidia corporation",
    "amd",
    "intel",
    "realtek",
];

fn is_protected_name(name: &str) -> bool {
    let clean = name.trim().to_ascii_lowercase();
    if clean.len() < 3 {
        return true;
    }
    PROTECTED_DIR_NAMES.iter().any(|&p| clean == p)
}

fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    let s = wide_ptr_to_string_and_free(raw);
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

fn dir_size_bounded(dir: &Path, max_depth: usize) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((cur, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push((entry.path(), depth + 1));
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

fn sanitize_token(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.len() < 3 || is_protected_name(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

fn build_app_tokens(app: &InstalledApp) -> (Vec<String>, Option<String>) {
    let mut names = Vec::new();
    let mut seen = HashSet::new();

    let mut push_token = |raw: &str| {
        if let Some(tok) = sanitize_token(raw) {
            let low = tok.to_ascii_lowercase();
            if seen.insert(low) {
                names.push(tok);
            }
        }
    };

    push_token(&app.display_name);

    let stripped_parens = app
        .display_name
        .split('(')
        .next()
        .unwrap_or(&app.display_name)
        .trim();
    push_token(stripped_parens);

    let words: Vec<&str> = stripped_parens.split_whitespace().collect();
    if words.len() >= 2 {
        let last = words[words.len() - 1];
        if last.chars().next().is_some_and(|c| c.is_ascii_digit())
            || (last.starts_with('v')
                && last[1..].chars().next().is_some_and(|c| c.is_ascii_digit()))
        {
            push_token(&words[..words.len() - 1].join(" "));
        }
    }

    if let Some(ref loc) = app.install_location {
        if let Some(folder) = loc.file_name().and_then(|n| n.to_str()) {
            if !GENERIC_FOLDER_TOKENS.contains(&folder.trim().to_ascii_lowercase().as_str()) {
                push_token(folder);
            }
        }
    }

    let pub_clean = sanitize_token(&app.publisher);
    (names, pub_clean)
}

/// Coincidencia **exacta** (sin distinguir mayúsculas). Una subcadena marcaba datos de otras apps:
/// «Python» coincidía con `Python312`, «Office» con `LibreOffice`, «Games» con `Epic Games`.
fn matches_any_token(candidate: &str, tokens: &[String]) -> bool {
    let low = candidate.trim().to_ascii_lowercase();
    if low.len() < 3 || is_protected_name(&low) {
        return false;
    }
    tokens.iter().any(|t| low == t.to_ascii_lowercase())
}

/// Nombres de carpeta demasiado genéricos para atribuirlos a una sola app.
const GENERIC_FOLDER_TOKENS: &[&str] = &[
    "games",
    "apps",
    "app",
    "programs",
    "software",
    "tools",
    "bin",
    "data",
    "files",
    "steam",
    "steamapps",
    "common",
    "launcher",
    "client",
];

/// Qué se marca por defecto en el modal de rastros.
fn is_recommended_leftover(
    item: &LeftoverItem,
    app_tokens: &[String],
    publisher: Option<&str>,
) -> bool {
    // Datos y ajustes del usuario: se muestran, pero el usuario decide.
    if item.category.starts_with("AppData") || item.category == "Registro HKCU\\Software" {
        return false;
    }
    // Carpeta o clave de todo el fabricante (puede contener datos de otras apps suyas).
    let leaf = item
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .or_else(|| {
            item.reg_subkey
                .as_ref()
                .and_then(|k| k.rsplit('\\').next())
                .map(|n| n.to_ascii_lowercase())
        });
    if let (Some(leaf), Some(publisher)) = (leaf, publisher) {
        if leaf == publisher && !app_tokens.contains(&leaf) {
            return false;
        }
    }
    true
}

/// Exporta `subkey` con `reg.exe export` a `ProgramData\OtterSwiss\RegBackups`, para poder restaurarla
/// con doble clic si se borró algo que no se debía.
fn export_registry_key(is_hkcu: bool, wow64_32: bool, subkey: &str) -> CoreResult<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let dir = ensure_admin_only_data_dir()?.join("RegBackups");
    std::fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let leaf: String = subkey
        .rsplit('\\')
        .next()
        .unwrap_or("clave")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let file = dir.join(format!("{}_{}.reg", stamp, leaf));
    let full_key = format!("{}\\{}", if is_hkcu { "HKCU" } else { "HKLM" }, subkey);

    let mut cmd = std::process::Command::new(system32_dir()?.join("reg.exe"));
    cmd.arg("export").arg(&full_key).arg(&file).arg("/y");
    if !is_hkcu {
        cmd.arg(if wow64_32 { "/reg:32" } else { "/reg:64" });
    }
    let out = cmd.creation_flags(CREATE_NO_WINDOW).output()?;
    if !out.status.success() || !file.is_file() {
        return Err(CoreError::Uninstaller {
            message: format!(
                "No se pudo respaldar '{}' antes de borrarla; no se borró: {}",
                full_key,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            source: None,
        });
    }
    Ok(file)
}

fn parse_cmd_exe_and_args(trimmed: &str) -> (&str, &str) {
    if let Some(stripped) = trimmed.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            return (&stripped[..end], stripped[end + 1..].trim());
        }
        return (trimmed, "");
    }

    let lower = trimmed.to_ascii_lowercase();
    if let Some(pos) = lower.find(".exe") {
        let split_at = pos + 4;
        if split_at == trimmed.len() || trimmed.as_bytes()[split_at].is_ascii_whitespace() {
            return (&trimmed[..split_at], trimmed[split_at..].trim());
        }
    }

    let mut parts = trimmed.splitn(2, ' ');
    (
        parts.next().unwrap_or(trimmed),
        parts.next().unwrap_or("").trim(),
    )
}

fn push_dir_candidate(
    cat: &str,
    dir: PathBuf,
    seen_paths: &mut HashSet<String>,
    items: &mut Vec<LeftoverItem>,
) {
    let key = dir.to_string_lossy().to_ascii_lowercase();
    if !seen_paths.insert(key) {
        return;
    }
    if !dir.is_dir() || !is_safe_non_root_dir(&dir, 2) {
        return;
    }
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    if is_protected_name(name) {
        return;
    }
    let sz = dir_size_bounded(&dir, 10);
    items.push(LeftoverItem {
        id: format!("dir:{}", dir.display()),
        kind: LeftoverKind::Directory,
        category: cat.to_string(),
        target_display: dir.display().to_string(),
        path: Some(dir),
        reg_is_hkcu: false,
        reg_wow64_32: false,
        reg_subkey: None,
        reg_value_name: None,
        size_bytes: sz,
        recommended: true,
    });
}

fn is_safe_app_folder_for_size(dir: &Path) -> bool {
    if !dir.is_dir() || !is_safe_non_root_dir(dir, 2) {
        return false;
    }
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if is_protected_name(name) {
        return false;
    }
    let low = dir.to_string_lossy().to_ascii_lowercase();
    !low.contains(r"\windows\system32") && !low.contains(r"\windows\syswow64")
}

pub struct UninstallerManager;

impl UninstallerManager {
    pub fn list_installed_apps() -> CoreResult<Vec<InstalledApp>> {
        let mut apps = Vec::new();
        let mut seen_ids = HashSet::new();

        let hives = [
            (
                HKEY_LOCAL_MACHINE,
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                false,
                false,
                KEY_READ | KEY_WOW64_64KEY,
            ),
            (
                HKEY_LOCAL_MACHINE,
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                false,
                true,
                KEY_READ | KEY_WOW64_32KEY,
            ),
            (
                HKEY_CURRENT_USER,
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                true,
                false,
                KEY_READ,
            ),
        ];

        for (hive, path, is_per_user, is_wow64, flags) in hives {
            let root = RegKey::predef(hive);
            if let Ok(uninstall_key) = root.open_subkey_with_flags(path, flags) {
                for key_name in uninstall_key.enum_keys().flatten() {
                    if let Ok(app_key) = uninstall_key.open_subkey_with_flags(&key_name, flags) {
                        let sys_comp: u32 = app_key.get_value("SystemComponent").unwrap_or(0);
                        if sys_comp == 1 {
                            continue;
                        }
                        let parent_key: String =
                            app_key.get_value("ParentKeyName").unwrap_or_default();
                        if !parent_key.is_empty() {
                            continue;
                        }

                        let display_name: String =
                            app_key.get_value("DisplayName").unwrap_or_default();
                        let display_name = display_name.trim().to_string();
                        if display_name.is_empty() {
                            continue;
                        }

                        let uninstall_string: String =
                            app_key.get_value("UninstallString").unwrap_or_default();
                        let uninstall_string = uninstall_string.trim().to_string();
                        if uninstall_string.is_empty() {
                            continue;
                        }

                        let quiet_uninstall_string: Option<String> = app_key
                            .get_value::<String, _>("QuietUninstallString")
                            .ok()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty());

                        let display_version: String =
                            app_key.get_value("DisplayVersion").unwrap_or_default();
                        let publisher: String = app_key.get_value("Publisher").unwrap_or_default();
                        let loc_str: String =
                            app_key.get_value("InstallLocation").unwrap_or_default();
                        let install_location = if !loc_str.trim().is_empty() {
                            Some(PathBuf::from(loc_str.trim().trim_matches('"')))
                        } else {
                            None
                        };

                        let icon_str: String = app_key.get_value("DisplayIcon").unwrap_or_default();
                        let icon_clean = icon_str
                            .trim()
                            .split(',')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .trim_matches('"');
                        let display_icon = if !icon_clean.is_empty() {
                            Some(PathBuf::from(icon_clean))
                        } else {
                            None
                        };

                        let estimated_size_kb: Option<u64> = app_key
                            .get_value::<u32, _>("EstimatedSize")
                            .ok()
                            .filter(|&v| v > 0)
                            .map(|v| v as u64);

                        let registry_subkey = format!(r"{}\{}", path, key_name);
                        let id = format!(
                            "{}:{}:{}",
                            if is_per_user { "HKCU" } else { "HKLM" },
                            if is_wow64 { "32" } else { "64" },
                            key_name
                        );
                        if !seen_ids.insert(id.clone()) {
                            continue;
                        }

                        apps.push(InstalledApp {
                            id,
                            display_name,
                            display_version: display_version.trim().to_string(),
                            publisher: publisher.trim().to_string(),
                            uninstall_string,
                            quiet_uninstall_string,
                            install_location,
                            display_icon,
                            estimated_size_kb,
                            is_per_user,
                            is_wow64,
                            registry_subkey,
                        });
                    }
                }
            }
        }

        apps.sort_by(|a, b| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        });
        Ok(apps)
    }

    /// Calcula el tamaño real en KB en segundo plano para apps que no declararon `EstimatedSize`.
    pub fn estimate_missing_sizes(apps: Vec<InstalledApp>) -> HashMap<String, u64> {
        let mut sizes = HashMap::new();

        for app in apps {
            if app.estimated_size_kb.is_some() {
                continue;
            }

            let candidate = app
                .install_location
                .as_ref()
                .filter(|p| is_safe_app_folder_for_size(p))
                .cloned()
                .or_else(|| {
                    app.display_icon
                        .as_ref()
                        .and_then(|p| p.parent())
                        .filter(|p| is_safe_app_folder_for_size(p))
                        .map(Path::to_path_buf)
                })
                .or_else(|| {
                    let expanded =
                        Self::expand_environment_string(&app.uninstall_string, None).ok()?;
                    let (exe, _) = parse_cmd_exe_and_args(expanded.trim());
                    let exe_path = Path::new(exe);
                    let parent = exe_path.parent()?;
                    if is_safe_app_folder_for_size(parent) {
                        Some(parent.to_path_buf())
                    } else {
                        None
                    }
                });

            if let Some(dir) = candidate {
                let bytes = dir_size_bounded(&dir, 6);
                if bytes > 0 {
                    sizes.insert(app.id, (bytes / 1024).max(1));
                }
            }
        }

        sizes
    }

    /// Expande `%VAR%` con el entorno del **sistema** (sin token) o del usuario dueño de `token`, nunca con
    /// el del propio proceso: un proceso sin elevar puede redefinir `%ProgramFiles%` en `HKCU\Environment`
    /// y hacer que OtterSwiss ejecute como administrador un binario suyo.
    pub fn expand_environment_string(raw: &str, token: Option<&SafeHandle>) -> CoreResult<String> {
        if !raw.contains('%') {
            return Ok(raw.to_string());
        }
        let wide = to_wide_null(raw);
        let mut buf = vec![0u16; 32_768];
        let h = token.map(|t| t.raw()).unwrap_or_default();
        unsafe { ExpandEnvironmentStringsForUserW(h, PCWSTR(wide.as_ptr()), &mut buf) }.map_err(
            |e| CoreError::Uninstaller {
                message: format!("No se pudo expandir '{}': {}", raw, e),
                source: None,
            },
        )?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(String::from_utf16_lossy(&buf[..end]))
    }

    /// Ejecuta el desinstalador de `app` y espera a que terminen él y todos sus hijos.
    /// Devuelve el código de salida; el llamador decide si es éxito (`is_uninstall_success`).
    pub fn run_uninstaller(app: &InstalledApp, timeout: Duration) -> CoreResult<u32> {
        let raw_cmd = app
            .quiet_uninstall_string
            .as_deref()
            .unwrap_or(&app.uninstall_string);

        // Las entradas de HKCU se ejecutan sin elevación, como el usuario del escritorio.
        let user_token = if app.is_per_user && ElevationManager::is_elevated().unwrap_or(false) {
            Some(duplicate_desktop_shell_primary_token()?)
        } else {
            None
        };

        let expanded = Self::expand_environment_string(raw_cmd, user_token.as_ref())?;
        let trimmed = expanded.trim();

        let (exe, raw_args) = parse_cmd_exe_and_args(trimmed);
        let exe_path = PathBuf::from(exe);
        let is_msi = exe_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("msiexec.exe") || n.eq_ignore_ascii_case("msiexec"))
            .unwrap_or(false);

        let sys32 = system32_dir()?;
        let application = if is_msi {
            sys32.join("msiexec.exe")
        } else {
            let is_unc = exe.starts_with(r"\\") || exe.starts_with("//");
            if !exe_path.is_absolute() || is_unc || !exe_path.is_file() {
                return Err(CoreError::Uninstaller {
                    message: format!(
                        "Ruta de desinstalador no permitida (debe ser un archivo local con ruta absoluta): {}",
                        exe
                    ),
                    source: None,
                });
            }
            exe_path
        };

        let mut args = raw_args.to_string();
        if is_msi {
            args = args.replace("/I{", "/X{").replace("/i{", "/X{");
            let low_args = args.to_ascii_lowercase();
            if !low_args.contains("/q") {
                args.push_str(" /qn /norestart");
            }
        }

        let command_line = if args.trim().is_empty() {
            format!("\"{}\"", application.display())
        } else {
            format!("\"{}\" {}", application.display(), args.trim())
        };
        let working_dir = if is_msi {
            sys32
        } else {
            application
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| sys32.clone())
        };

        let exit = run_in_job(
            &Launch {
                application: &application,
                command_line,
                working_dir: &working_dir,
                user_token: user_token.as_ref(),
            },
            timeout,
            None,
        )?;
        Ok(exit.exit_code)
    }

    pub fn scan_leftovers(app: &InstalledApp) -> CoreResult<Vec<LeftoverItem>> {
        let mut items = Vec::new();
        let mut seen_paths = HashSet::<String>::new();
        let (tokens, publisher_tok) = build_app_tokens(app);

        let other_apps_same_publisher = if let Some(ref pub_name) = publisher_tok {
            Self::list_installed_apps()
                .unwrap_or_default()
                .into_iter()
                .any(|other| {
                    other.id != app.id
                        && sanitize_token(&other.publisher)
                            .is_some_and(|p| p.eq_ignore_ascii_case(pub_name))
                })
        } else {
            true
        };

        // Si la app sigue registrada (el desinstalador falló, se canceló o aún no se ejecutó), su carpeta de
        // instalación y su entrada Uninstall no son «rastros»: borrarlas rompería una app instalada.
        let still_registered = Self::is_still_registered(app);
        // Una entrada de HKCU la puede crear cualquier proceso del usuario: no decide qué se borra fuera de
        // su propio perfil (Program Files, ProgramData, HKLM).
        let user_scope_only = app.is_per_user;

        if let Some(loc) = app.install_location.as_ref().filter(|_| !still_registered) {
            if loc.is_dir() && is_safe_non_root_dir(loc, 2) {
                push_dir_candidate(
                    "Carpeta de instalación",
                    loc.clone(),
                    &mut seen_paths,
                    &mut items,
                );
            }
        }

        let mut scan_roots: Vec<(&'static str, PathBuf)> = Vec::new();
        if !user_scope_only {
            if let Some(pf) = program_files_dir() {
                scan_roots.push(("Archivos de Programa (x64)", pf));
            }
            if let Some(pf86) = program_files_x86_dir() {
                scan_roots.push(("Archivos de Programa (x86)", pf86));
            }
            if let Ok(pd) = get_program_data_dir() {
                scan_roots.push(("ProgramData", pd));
            }
        }
        if let Some(roaming) = dirs::config_dir() {
            scan_roots.push(("AppData Roaming", roaming));
        }
        if let Some(local) = known_folder(&FOLDERID_LocalAppData) {
            scan_roots.push(("AppData Local", local));
        }

        for (cat, root) in scan_roots {
            if !root.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if !p.is_dir() {
                        continue;
                    }
                    let fname = entry.file_name().to_string_lossy().into_owned();
                    if is_protected_name(&fname) {
                        continue;
                    }
                    if matches_any_token(&fname, &tokens) {
                        push_dir_candidate(cat, p, &mut seen_paths, &mut items);
                    } else if let Some(ref pub_tok) = publisher_tok {
                        if fname.eq_ignore_ascii_case(pub_tok) {
                            if !other_apps_same_publisher {
                                push_dir_candidate(cat, p, &mut seen_paths, &mut items);
                            } else if let Ok(sub_entries) = std::fs::read_dir(&p) {
                                for sub in sub_entries.flatten() {
                                    let sp = sub.path();
                                    let sname = sub.file_name().to_string_lossy().into_owned();
                                    if sp.is_dir() && matches_any_token(&sname, &tokens) {
                                        push_dir_candidate(cat, sp, &mut seen_paths, &mut items);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut shortcut_roots = Vec::new();
        if let Some(desk) = dirs::desktop_dir() {
            shortcut_roots.push(("Escritorio", desk));
        }
        if let Some(pub_dir) = known_folder(&FOLDERID_Public).filter(|_| !user_scope_only) {
            shortcut_roots.push(("Escritorio Público", pub_dir.join("Desktop")));
        }
        if let Some(roaming) = dirs::config_dir() {
            shortcut_roots.push((
                "Menú Inicio (Usuario)",
                roaming.join(r"Microsoft\Windows\Start Menu\Programs"),
            ));
        }
        if let Some(pd) = get_program_data_dir().ok().filter(|_| !user_scope_only) {
            shortcut_roots.push((
                "Menú Inicio (Sistema)",
                pd.join(r"Microsoft\Windows\Start Menu\Programs"),
            ));
        }

        for (cat, sc_root) in shortcut_roots {
            if !sc_root.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&sc_root) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let fname = entry.file_name().to_string_lossy().into_owned();
                    let stem = p
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| fname.clone());
                    if p.is_dir() {
                        if matches_any_token(&fname, &tokens) {
                            push_dir_candidate(cat, p, &mut seen_paths, &mut items);
                        }
                    } else if fname.to_ascii_lowercase().ends_with(".lnk")
                        && matches_any_token(&stem, &tokens)
                    {
                        let key = p.to_string_lossy().to_ascii_lowercase();
                        if seen_paths.insert(key) {
                            let sz = entry.metadata().map(|m| m.len()).unwrap_or(0);
                            items.push(LeftoverItem {
                                id: format!("file:{}", p.display()),
                                kind: LeftoverKind::File,
                                category: cat.to_string(),
                                target_display: p.display().to_string(),
                                path: Some(p),
                                reg_is_hkcu: false,
                                reg_wow64_32: false,
                                reg_subkey: None,
                                reg_value_name: None,
                                size_bytes: sz,
                                recommended: true,
                            });
                        }
                    }
                }
            }
        }

        let uninst_hive = if app.is_per_user {
            HKEY_CURRENT_USER
        } else {
            HKEY_LOCAL_MACHINE
        };
        let uninst_flags = if app.is_per_user {
            KEY_READ
        } else if app.is_wow64 {
            KEY_READ | KEY_WOW64_32KEY
        } else {
            KEY_READ | KEY_WOW64_64KEY
        };
        if !still_registered
            && RegKey::predef(uninst_hive)
                .open_subkey_with_flags(&app.registry_subkey, uninst_flags)
                .is_ok()
        {
            items.push(LeftoverItem {
                id: format!("reg_uninst:{}", app.id),
                kind: LeftoverKind::RegistryKey,
                category: "Registro (Entrada Uninstall)".into(),
                target_display: format!(
                    "{}\\{}",
                    if app.is_per_user { "HKCU" } else { "HKLM" },
                    app.registry_subkey
                ),
                path: None,
                reg_is_hkcu: app.is_per_user,
                reg_wow64_32: app.is_wow64,
                reg_subkey: Some(app.registry_subkey.clone()),
                reg_value_name: None,
                size_bytes: 0,
                recommended: true,
            });
        }

        let reg_scopes = [
            (true, false, "Registro HKCU\\Software", KEY_READ),
            (
                false,
                false,
                "Registro HKLM\\SOFTWARE",
                KEY_READ | KEY_WOW64_64KEY,
            ),
            (
                false,
                true,
                "Registro HKLM\\SOFTWARE (32-bit)",
                KEY_READ | KEY_WOW64_32KEY,
            ),
        ];
        let mut seen_reg = HashSet::<String>::new();

        for (is_hkcu, wow32, cat, flags) in reg_scopes {
            if user_scope_only && !is_hkcu {
                continue;
            }
            let root = RegKey::predef(if is_hkcu {
                HKEY_CURRENT_USER
            } else {
                HKEY_LOCAL_MACHINE
            });
            if let Ok(sw) = root.open_subkey_with_flags("Software", flags) {
                for key_name in sw.enum_keys().flatten() {
                    if is_protected_name(&key_name)
                        || key_name.eq_ignore_ascii_case("classes")
                        || key_name.eq_ignore_ascii_case("policies")
                        || key_name.eq_ignore_ascii_case("wow6432node")
                    {
                        continue;
                    }

                    let sub_path = format!(r"Software\{}", key_name);
                    if matches_any_token(&key_name, &tokens) {
                        let uid =
                            format!("{}:{}:{}", is_hkcu, wow32, sub_path.to_ascii_lowercase());
                        if seen_reg.insert(uid) {
                            items.push(LeftoverItem {
                                id: format!("reg:{}:{}:{}", is_hkcu, wow32, sub_path),
                                kind: LeftoverKind::RegistryKey,
                                category: cat.to_string(),
                                target_display: format!(
                                    "{}\\{}",
                                    if is_hkcu { "HKCU" } else { "HKLM" },
                                    sub_path
                                ),
                                path: None,
                                reg_is_hkcu: is_hkcu,
                                reg_wow64_32: wow32,
                                reg_subkey: Some(sub_path),
                                reg_value_name: None,
                                size_bytes: 0,
                                recommended: true,
                            });
                        }
                    } else if let Some(ref pub_tok) = publisher_tok {
                        if key_name.eq_ignore_ascii_case(pub_tok) {
                            if !other_apps_same_publisher {
                                let uid = format!(
                                    "{}:{}:{}",
                                    is_hkcu,
                                    wow32,
                                    sub_path.to_ascii_lowercase()
                                );
                                if seen_reg.insert(uid) {
                                    items.push(LeftoverItem {
                                        id: format!("reg:{}:{}:{}", is_hkcu, wow32, sub_path),
                                        kind: LeftoverKind::RegistryKey,
                                        category: cat.to_string(),
                                        target_display: format!(
                                            "{}\\{}",
                                            if is_hkcu { "HKCU" } else { "HKLM" },
                                            sub_path
                                        ),
                                        path: None,
                                        reg_is_hkcu: is_hkcu,
                                        reg_wow64_32: wow32,
                                        reg_subkey: Some(sub_path),
                                        reg_value_name: None,
                                        size_bytes: 0,
                                        recommended: true,
                                    });
                                }
                            } else if let Ok(pub_key) = sw.open_subkey_with_flags(&key_name, flags)
                            {
                                for sub_name in pub_key.enum_keys().flatten() {
                                    if matches_any_token(&sub_name, &tokens) {
                                        let full_sub = format!(r"{}\{}", sub_path, sub_name);
                                        let uid = format!(
                                            "{}:{}:{}",
                                            is_hkcu,
                                            wow32,
                                            full_sub.to_ascii_lowercase()
                                        );
                                        if seen_reg.insert(uid) {
                                            items.push(LeftoverItem {
                                                id: format!(
                                                    "reg:{}:{}:{}",
                                                    is_hkcu, wow32, full_sub
                                                ),
                                                kind: LeftoverKind::RegistryKey,
                                                category: cat.to_string(),
                                                target_display: format!(
                                                    "{}\\{}",
                                                    if is_hkcu { "HKCU" } else { "HKLM" },
                                                    full_sub
                                                ),
                                                path: None,
                                                reg_is_hkcu: is_hkcu,
                                                reg_wow64_32: wow32,
                                                reg_subkey: Some(full_sub),
                                                reg_value_name: None,
                                                size_bytes: 0,
                                                recommended: true,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let run_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        for (is_hkcu, wow32, flags) in [
            (true, false, KEY_READ),
            (false, false, KEY_READ | KEY_WOW64_64KEY),
            (false, true, KEY_READ | KEY_WOW64_32KEY),
        ] {
            if user_scope_only && !is_hkcu {
                continue;
            }
            let root = RegKey::predef(if is_hkcu {
                HKEY_CURRENT_USER
            } else {
                HKEY_LOCAL_MACHINE
            });
            if let Ok(run_key) = root.open_subkey_with_flags(run_path, flags) {
                for (val_name, val_data) in run_key.enum_values().flatten() {
                    let cmd = val_data.to_string().to_ascii_lowercase();
                    let loc_match = app.install_location.as_ref().is_some_and(|loc| {
                        let ls = loc.to_string_lossy().to_ascii_lowercase();
                        !ls.is_empty() && is_safe_non_root_dir(loc, 2) && cmd.contains(&ls)
                    });
                    if loc_match || matches_any_token(&val_name, &tokens) {
                        items.push(LeftoverItem {
                            id: format!("run:{}:{}:{}", is_hkcu, wow32, val_name),
                            kind: LeftoverKind::RegistryValue,
                            category: "Inicio automático (Run)".into(),
                            target_display: format!(
                                "{}\\{} -> {}",
                                if is_hkcu { "HKCU" } else { "HKLM" },
                                run_path,
                                val_name
                            ),
                            path: None,
                            reg_is_hkcu: is_hkcu,
                            reg_wow64_32: wow32,
                            reg_subkey: Some(run_path.to_string()),
                            reg_value_name: Some(val_name),
                            size_bytes: 0,
                            recommended: true,
                        });
                    }
                }
            }
        }

        // SYS-04: con la app aún registrada, su carpeta de instalación (y lo que cuelga de ella) tampoco
        // puede entrar por el barrido de Program Files por nombre: borrarla rompería una app instalada.
        if still_registered {
            if let Some(loc) = app.install_location.as_ref() {
                let loc_low = loc
                    .to_string_lossy()
                    .trim_end_matches('\\')
                    .to_ascii_lowercase();
                items.retain(|i| {
                    i.path.as_ref().is_none_or(|p| {
                        let p_low = p
                            .to_string_lossy()
                            .trim_end_matches('\\')
                            .to_ascii_lowercase();
                        p_low != loc_low && !p_low.starts_with(&format!("{loc_low}\\"))
                    })
                });
            }
        }

        let app_tokens_low: Vec<String> = tokens.iter().map(|t| t.to_ascii_lowercase()).collect();
        let publisher_low = publisher_tok.as_ref().map(|p| p.to_ascii_lowercase());
        for item in &mut items {
            item.recommended =
                is_recommended_leftover(item, &app_tokens_low, publisher_low.as_deref());
        }
        Ok(items)
    }

    /// `true` si la entrada Uninstall de la app sigue en el registro.
    pub fn is_still_registered(app: &InstalledApp) -> bool {
        let (hive, flags) = if app.is_per_user {
            (HKEY_CURRENT_USER, KEY_READ)
        } else if app.is_wow64 {
            (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY)
        } else {
            (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY)
        };
        RegKey::predef(hive)
            .open_subkey_with_flags(&app.registry_subkey, flags)
            .is_ok()
    }

    pub fn purge_leftover_items(items: &[LeftoverItem]) -> PurgeReport {
        let mut report = PurgeReport::default();

        for item in items {
            match item.kind {
                LeftoverKind::Directory => {
                    let Some(ref target) = item.path else {
                        continue;
                    };
                    if !target.exists() {
                        continue;
                    }
                    match Self::purge_leftovers_verified_dir(target) {
                        Ok(()) => {
                            report.removed_count += 1;
                            report.bytes_freed = report.bytes_freed.saturating_add(item.size_bytes);
                        }
                        Err(e) => {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", target.display(), e));
                        }
                    }
                }
                LeftoverKind::File => {
                    let Some(ref target) = item.path else {
                        continue;
                    };
                    if !target.exists() {
                        continue;
                    }
                    let res = target
                        .parent()
                        .ok_or_else(|| CoreError::Validation("Archivo sin padre".into()))
                        .and_then(canonical_dos_path)
                        .and_then(|canon| delete_verified_file_under(target, Some(&canon)));
                    match res {
                        Ok(()) => {
                            report.removed_count += 1;
                            report.bytes_freed = report.bytes_freed.saturating_add(item.size_bytes);
                        }
                        Err(e) => {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", target.display(), e));
                        }
                    }
                }
                LeftoverKind::RegistryKey => {
                    let Some(ref subkey) = item.reg_subkey else {
                        continue;
                    };
                    if !is_safe_registry_subkey_to_delete(subkey) {
                        report.failed_count += 1;
                        report
                            .errors
                            .push(format!("Clave protegida omitida: {}", subkey));
                        continue;
                    }
                    match Self::delete_registry_key_with_backup(item, subkey) {
                        Ok(true) => report.removed_count += 1,
                        Ok(false) => {}
                        Err(e) => {
                            report.failed_count += 1;
                            report
                                .errors
                                .push(format!("{}: {}", item.target_display, e));
                        }
                    }
                }
                LeftoverKind::RegistryValue => {
                    let (Some(ref subkey), Some(ref val_name)) =
                        (&item.reg_subkey, &item.reg_value_name)
                    else {
                        continue;
                    };
                    let hive = if item.reg_is_hkcu {
                        HKEY_CURRENT_USER
                    } else {
                        HKEY_LOCAL_MACHINE
                    };
                    let flags = if item.reg_is_hkcu {
                        KEY_WRITE
                    } else if item.reg_wow64_32 {
                        KEY_WRITE | KEY_WOW64_32KEY
                    } else {
                        KEY_WRITE | KEY_WOW64_64KEY
                    };
                    let root = RegKey::predef(hive);
                    match root
                        .open_subkey_with_flags(subkey, flags)
                        .and_then(|k| k.delete_value(val_name))
                    {
                        Ok(()) => report.removed_count += 1,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            report.failed_count += 1;
                            report
                                .errors
                                .push(format!("{}: {}", item.target_display, e));
                        }
                    }
                }
            }
        }

        report
    }

    /// Exporta la clave a un `.reg` en la carpeta de datos y después la borra. Si la exportación falla, no
    /// se borra nada. Devuelve `Ok(false)` si la clave ya no existía.
    fn delete_registry_key_with_backup(item: &LeftoverItem, subkey: &str) -> CoreResult<bool> {
        let hive = if item.reg_is_hkcu {
            HKEY_CURRENT_USER
        } else {
            HKEY_LOCAL_MACHINE
        };
        let view = if item.reg_is_hkcu {
            0
        } else if item.reg_wow64_32 {
            KEY_WOW64_32KEY
        } else {
            KEY_WOW64_64KEY
        };
        let root = RegKey::predef(hive);
        let (parent_path, leaf) = subkey
            .rsplit_once('\\')
            .ok_or_else(|| CoreError::Validation(format!("Clave sin padre: {}", subkey)))?;
        // RegDeleteTree necesita DELETE, KEY_ENUMERATE_SUB_KEYS y KEY_QUERY_VALUE sobre el padre (SYS-21).
        let parent = match root.open_subkey_with_flags(parent_path, KEY_ALL_ACCESS | view) {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        if parent
            .open_subkey_with_flags(leaf, KEY_READ | view)
            .is_err()
        {
            return Ok(false);
        }

        export_registry_key(item.reg_is_hkcu, item.reg_wow64_32, subkey)?;
        parent.delete_subkey_all(leaf)?;
        Ok(true)
    }

    /// Borra un árbol de restos solo si su ruta **canónica** está dentro de una raíz conocida (obtenida con
    /// `SHGetKnownFolderPath`, nunca de variables de entorno) y ningún componente es una junction o symlink.
    fn purge_leftovers_verified_dir(target: &Path) -> CoreResult<()> {
        let mut roots: Vec<PathBuf> = Vec::new();
        for r in [
            program_files_dir(),
            program_files_x86_dir(),
            get_program_data_dir().ok(),
            known_folder(&FOLDERID_RoamingAppData),
            known_folder(&FOLDERID_LocalAppData),
            known_folder(&FOLDERID_Desktop),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(c) = canonical_dos_path(&r) {
                roots.push(c);
            }
        }

        let target_canon = canonical_dos_path(target)?;
        // Si la ruta canónica difiere de la pedida, algún componente es una junction o symlink.
        let lexical = target
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        let canon = target_canon
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        if lexical != canon {
            return Err(CoreError::Validation(format!(
                "La ruta pasa por un enlace o junction ('{}' → '{}'); no se borra",
                target.display(),
                target_canon.display()
            )));
        }

        let Some(root) = roots
            .iter()
            .filter(|r| path_is_strictly_under(&target_canon, r))
            .max_by_key(|r| r.as_os_str().len())
        else {
            return Err(CoreError::Validation(format!(
                "Ruta fuera de raíces permitidas para purga: '{}'",
                target.display()
            )));
        };

        let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_protected_name(name) || !is_safe_non_root_dir(&target_canon, 2) {
            return Err(CoreError::Validation(format!(
                "Directorio protegido contra purga: '{}'",
                target.display()
            )));
        }

        delete_tree_verified(&target_canon, root, 12)?;
        Ok(())
    }

    pub fn purge_leftovers(target: &Path) -> CoreResult<()> {
        Self::purge_leftovers_verified_dir(target)
    }

    /// Revierte todo lo que OtterSwiss cambió en el sistema (optimización, DNS y ajustes del Debloater).
    /// Lo usa `--rollback`, que llama el desinstalador. La carpeta de datos solo se borra si todo se
    /// revirtió y no queda ningún respaldo pendiente; si algo falla, los respaldos se conservan.
    pub fn revert_all_changes() -> CoreResult<FullRevertReport> {
        ElevationManager::require_elevated("Revertir todos los cambios de OtterSwiss")?;
        let data_dir = ensure_admin_only_data_dir()?;
        let mut report = FullRevertReport::default();

        let engine = OptimizationEngine::new(&data_dir);
        if engine.has_backup() {
            match engine.rollback(false) {
                Ok(rb) => {
                    report.failures.extend(
                        rb.failed_items
                            .iter()
                            .map(|f| format!("[Optimización] {}", f)),
                    );
                    report.engine = Some(rb);
                }
                Err(e) => report.failures.push(format!("[Optimización] {}", e)),
            }
        }

        let dns = DnsManager::new(&data_dir);
        if dns.has_backup() {
            match dns.restore_dns() {
                Ok(n) => report.dns_restored = n,
                Err(e) => report.failures.push(format!("[DNS] {}", e)),
            }
        }

        if DebloaterManager::has_registry_backup() {
            match DebloaterManager::revert_registry_debloat() {
                Ok(n) => report.debloat_restored = n,
                Err(e) => report.failures.push(format!("[Debloater] {}", e)),
            }
        }

        let pending_backups =
            engine.has_backup() || dns.has_backup() || DebloaterManager::has_registry_backup();
        if report.failures.is_empty() && !pending_backups {
            let parent = data_dir
                .parent()
                .ok_or_else(|| CoreError::Validation("ProgramData sin padre".into()))?;
            match delete_tree_verified(&data_dir, parent, 8) {
                Ok(()) => report.data_dir_deleted = true,
                Err(e) => report.failures.push(format!("[Datos] {}", e)),
            }
        }

        Ok(report)
    }
}

/// Resultado de `UninstallerManager::revert_all_changes`.
#[derive(Debug, Default)]
pub struct FullRevertReport {
    pub engine: Option<RollbackReport>,
    pub dns_restored: usize,
    pub debloat_restored: usize,
    pub failures: Vec<String>,
    pub data_dir_deleted: bool,
}

impl FullRevertReport {
    /// `true` si no hubo fallos ni ítems omitidos por cambios del usuario.
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
            && self
                .engine
                .as_ref()
                .is_none_or(|e| e.skipped_user_modified == 0)
    }
}

fn is_safe_registry_subkey_to_delete(subkey: &str) -> bool {
    let low = subkey.trim().to_ascii_lowercase();
    if !low.starts_with("software\\") {
        return false;
    }
    let rest = &low["software\\".len()..];
    !(rest.is_empty()
        || rest == "microsoft"
        || rest == "classes"
        || rest == "policies"
        || rest == "wow6432node"
        || rest == r"microsoft\windows\currentversion\uninstall"
        || (rest.starts_with("microsoft\\windows\\")
            && !rest.starts_with(r"microsoft\windows\currentversion\uninstall\")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, publisher: &str, location: Option<&str>) -> InstalledApp {
        InstalledApp {
            id: name.into(),
            display_name: name.into(),
            display_version: String::new(),
            publisher: publisher.into(),
            uninstall_string: String::new(),
            quiet_uninstall_string: None,
            install_location: location.map(PathBuf::from),
            display_icon: None,
            estimated_size_kb: None,
            is_per_user: false,
            is_wow64: false,
            registry_subkey: String::new(),
        }
    }

    /// SYS-03: la coincidencia es exacta; una subcadena marcaba datos de otras apps.
    #[test]
    fn tokens_coinciden_solo_exactos() {
        let tokens = vec!["Python".to_string(), "Office".to_string()];
        assert!(matches_any_token("python", &tokens));
        assert!(!matches_any_token("Python312", &tokens));
        assert!(!matches_any_token("LibreOffice", &tokens));
    }

    #[test]
    fn tokens_quitan_version_y_carpetas_genericas() {
        let (tokens, publisher) = build_app_tokens(&app(
            "Foo Editor 2.1 (x64)",
            "Foo Inc.",
            Some(r"C:\Program Files\Games"),
        ));
        assert!(tokens.iter().any(|t| t == "Foo Editor"));
        assert!(!tokens.iter().any(|t| t.eq_ignore_ascii_case("games")));
        assert_eq!(publisher.as_deref(), Some("Foo Inc."));
    }

    #[test]
    fn nombres_protegidos_no_son_tokens() {
        let (tokens, _) = build_app_tokens(&app("Microsoft", "Microsoft", Some(r"C:\Windows")));
        assert!(tokens.is_empty());
    }

    #[test]
    fn separa_ejecutable_y_argumentos() {
        assert_eq!(
            parse_cmd_exe_and_args(r#""C:\Program Files\App\unins000.exe" /SILENT"#),
            (r"C:\Program Files\App\unins000.exe", "/SILENT")
        );
        // SEC-22: sin comillas, el corte va tras «.exe», no en el primer espacio.
        assert_eq!(
            parse_cmd_exe_and_args(r"C:\Program Files\App\uninst.exe /S"),
            (r"C:\Program Files\App\uninst.exe", "/S")
        );
        assert_eq!(
            parse_cmd_exe_and_args("MsiExec.exe /X{ABC}"),
            ("MsiExec.exe", "/X{ABC}")
        );
    }

    #[test]
    fn solo_se_borran_subclaves_de_software_concretas() {
        assert!(!is_safe_registry_subkey_to_delete("Software"));
        assert!(!is_safe_registry_subkey_to_delete(r"Software\Microsoft"));
        assert!(!is_safe_registry_subkey_to_delete(r"Software\Classes"));
        assert!(!is_safe_registry_subkey_to_delete(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall"
        ));
        assert!(!is_safe_registry_subkey_to_delete(
            r"Software\Microsoft\Windows\Shell"
        ));
        assert!(!is_safe_registry_subkey_to_delete(
            r"System\CurrentControlSet\Services\x"
        ));
        assert!(is_safe_registry_subkey_to_delete(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\FooApp"
        ));
        assert!(is_safe_registry_subkey_to_delete(
            r"Software\Foo Inc\Foo Editor"
        ));
    }

    fn leftover(category: &str, path: &str) -> LeftoverItem {
        LeftoverItem {
            id: path.into(),
            kind: LeftoverKind::Directory,
            category: category.into(),
            target_display: path.into(),
            path: Some(PathBuf::from(path)),
            reg_is_hkcu: false,
            reg_wow64_32: false,
            reg_subkey: None,
            reg_value_name: None,
            size_bytes: 0,
            recommended: false,
        }
    }

    /// SYS-03: AppData y la carpeta de todo el fabricante no vienen marcadas.
    #[test]
    fn recomendados_excluyen_datos_de_usuario_y_fabricante() {
        // El llamador pasa tokens y fabricante en minúsculas.
        let tokens = vec!["foo editor".to_string()];
        let publisher = Some("foo inc");
        assert!(!is_recommended_leftover(
            &leftover("AppData Roaming", r"C:\Users\u\AppData\Roaming\Foo Editor"),
            &tokens,
            publisher
        ));
        assert!(!is_recommended_leftover(
            &leftover("Program Files", r"C:\Program Files\Foo Inc"),
            &tokens,
            publisher
        ));
        assert!(is_recommended_leftover(
            &leftover("Program Files", r"C:\Program Files\Foo Editor"),
            &tokens,
            publisher
        ));
    }
}
