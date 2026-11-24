use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, KEY_WRITE,
};
use winreg::RegKey;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, TokenPrimary, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE,
    TOKEN_QUERY,
};
use windows::Win32::System::Environment::{
    CreateEnvironmentBlock, DestroyEnvironmentBlock, ExpandEnvironmentStringsW,
};
use windows::Win32::System::Threading::{
    CreateProcessWithTokenW, GetExitCodeProcess, OpenProcess, OpenProcessToken,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, LOGON_WITH_PROFILE, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Public, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

use crate::core::elevation::ElevationManager;
use crate::core::engine::OptimizationEngine;
use crate::core::error::{CoreError, CoreResult};
use crate::core::safe_fs::{
    canonical_dos_path, delete_tree_verified, delete_verified_file_under, is_safe_non_root_dir,
};
use crate::core::trust::get_program_data_dir;
use crate::core::win_common::{to_wide_null, wide_ptr_to_string_and_free, JobObject, SafeHandle};

pub struct EnvironmentBlockGuard(*mut std::ffi::c_void);

impl Drop for EnvironmentBlockGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = DestroyEnvironmentBlock(self.0);
            }
        }
    }
}

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

fn duplicate_desktop_shell_primary_token() -> CoreResult<SafeHandle> {
    let shell_hwnd = unsafe { GetShellWindow() };
    if shell_hwnd.0.is_null() {
        return Err(CoreError::Uninstaller {
            message: "No se encontró el Explorador de escritorio para des-elevar".into(),
            source: None,
        });
    }
    let mut shell_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(shell_hwnd, Some(&mut shell_pid));
    }
    if shell_pid == 0 {
        return Err(CoreError::Uninstaller {
            message: "PID de explorer.exe inválido".into(),
            source: None,
        });
    }

    let proc_raw =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, shell_pid) }.map_err(|e| {
            CoreError::Uninstaller {
                message: format!("OpenProcess(explorer.exe) falló: {}", e),
                source: None,
            }
        })?;
    let proc_h = SafeHandle::new(proc_raw)
        .ok_or_else(|| CoreError::from_win32("Handle de explorer.exe inválido"))?;

    let mut tok_raw = HANDLE::default();
    unsafe { OpenProcessToken(proc_h.raw(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut tok_raw) }.map_err(
        |e| CoreError::Uninstaller {
            message: format!("OpenProcessToken(explorer.exe) falló: {}", e),
            source: None,
        },
    )?;
    let tok_h = SafeHandle::new(tok_raw)
        .ok_or_else(|| CoreError::from_win32("Token de explorer.exe inválido"))?;

    let mut primary_raw = HANDLE::default();
    unsafe {
        DuplicateTokenEx(
            tok_h.raw(),
            TOKEN_ALL_ACCESS,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut primary_raw,
        )
    }
    .map_err(|e| CoreError::Uninstaller {
        message: format!("DuplicateTokenEx falló: {}", e),
        source: None,
    })?;

    SafeHandle::new(primary_raw)
        .ok_or_else(|| CoreError::from_win32("Token primario duplicado inválido"))
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
            push_token(folder);
        }
    }

    let pub_clean = sanitize_token(&app.publisher);
    (names, pub_clean)
}

fn matches_any_token(candidate: &str, tokens: &[String]) -> bool {
    let low = candidate.trim().to_ascii_lowercase();
    if low.len() < 3 || is_protected_name(&low) {
        return false;
    }
    tokens.iter().any(|t| {
        let tl = t.to_ascii_lowercase();
        low == tl || (tl.len() >= 4 && low.contains(&tl))
    })
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
                        let publisher: String =
                            app_key.get_value("Publisher").unwrap_or_default();
                        let loc_str: String =
                            app_key.get_value("InstallLocation").unwrap_or_default();
                        let install_location = if !loc_str.trim().is_empty() {
                            Some(PathBuf::from(loc_str.trim().trim_matches('"')))
                        } else {
                            None
                        };

                        let icon_str: String =
                            app_key.get_value("DisplayIcon").unwrap_or_default();
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
                    let expanded = Self::expand_environment_string(&app.uninstall_string).ok()?;
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

    pub fn expand_environment_string(raw: &str) -> CoreResult<String> {
        let wide = to_wide_null(raw);
        let needed = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), None) };
        if needed == 0 {
            return Ok(raw.to_string());
        }
        let mut buf = vec![0u16; needed as usize];
        let written = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut buf)) };
        if written == 0 {
            return Ok(raw.to_string());
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(String::from_utf16_lossy(&buf[..end]))
    }

    pub fn run_uninstaller(app: &InstalledApp, timeout: Duration) -> CoreResult<u32> {
        let raw_cmd = app
            .quiet_uninstall_string
            .as_deref()
            .unwrap_or(&app.uninstall_string);
        let expanded = Self::expand_environment_string(raw_cmd)?;
        let trimmed = expanded.trim();

        let (exe, raw_args) = parse_cmd_exe_and_args(trimmed);
        let exe_path = PathBuf::from(exe);
        let is_msi = exe_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("msiexec.exe") || n.eq_ignore_ascii_case("msiexec"))
            .unwrap_or(false);

        if !is_msi && (!exe_path.is_absolute() || exe.starts_with(r"\\")) {
            return Err(CoreError::Uninstaller {
                message: format!(
                    "Ruta de desinstalador no permitida (debe ser absoluta local): {}",
                    exe
                ),
                source: None,
            });
        }

        let mut args = raw_args.to_string();
        if is_msi {
            args = args.replace("/I{", "/X{").replace("/i{", "/X{");
            let low_args = args.to_ascii_lowercase();
            if !low_args.contains("/q") {
                args.push_str(" /qn /norestart");
            }
        }

        let job = JobObject::new_kill_on_close()?;
        let cmd_line = if args.is_empty() {
            format!("\"{}\"", exe)
        } else {
            format!("\"{}\" {}", exe, args)
        };
        let mut cmd_wide = to_wide_null(&cmd_line);

        let mut pi = PROCESS_INFORMATION::default();
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };

        if app.is_per_user && ElevationManager::is_elevated().unwrap_or(false) {
            let user_token = duplicate_desktop_shell_primary_token()?;
            let mut env_block: *mut std::ffi::c_void = std::ptr::null_mut();
            unsafe { CreateEnvironmentBlock(&mut env_block, user_token.raw(), false) }.map_err(
                |e| CoreError::Uninstaller {
                    message: format!("CreateEnvironmentBlock falló: {}", e),
                    source: None,
                },
            )?;
            let _env_guard = EnvironmentBlockGuard(env_block);

            unsafe {
                CreateProcessWithTokenW(
                    user_token.raw(),
                    LOGON_WITH_PROFILE,
                    PCWSTR::null(),
                    PWSTR(cmd_wide.as_mut_ptr()),
                    CREATE_UNICODE_ENVIRONMENT,
                    Some(env_block),
                    PCWSTR::null(),
                    &si,
                    &mut pi,
                )
            }
            .map_err(|e| CoreError::Uninstaller {
                message: format!("CreateProcessWithTokenW falló: {}", e),
                source: None,
            })?;
        } else {
            let wide_exe = if is_msi {
                let mut sys_buf = [0u16; 260];
                let len = unsafe {
                    windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(
                        &mut sys_buf,
                    ))
                } as usize;
                if len > 0 && len < sys_buf.len() {
                    let sys = String::from_utf16_lossy(&sys_buf[..len]);
                    to_wide_null(Path::new(&sys).join("msiexec.exe"))
                } else {
                    to_wide_null(&exe_path)
                }
            } else {
                to_wide_null(&exe_path)
            };

            unsafe {
                windows::Win32::System::Threading::CreateProcessW(
                    PCWSTR(wide_exe.as_ptr()),
                    PWSTR(cmd_wide.as_mut_ptr()),
                    None,
                    None,
                    false,
                    Default::default(),
                    None,
                    PCWSTR::null(),
                    &si,
                    &mut pi,
                )
            }
            .map_err(|e| CoreError::Uninstaller {
                message: format!("CreateProcessW falló: {}", e),
                source: None,
            })?;
        }

        let _proc_handle = SafeHandle::new(pi.hProcess);
        let _thread_handle = SafeHandle::new(pi.hThread);

        if let Some(h) = _proc_handle.as_ref() {
            job.assign(h.raw())?;
        }

        let wait_ms = timeout.as_millis() as u32;
        let wait_res = unsafe { WaitForSingleObject(pi.hProcess, wait_ms) };

        if wait_res == WAIT_TIMEOUT {
            job.terminate_all(1);
            return Err(CoreError::Uninstaller {
                message: "El proceso de desinstalación excedió el tiempo límite y fue terminado"
                    .into(),
                source: None,
            });
        }

        if wait_res != WAIT_OBJECT_0 {
            return Err(CoreError::Uninstaller {
                message: "Error esperando terminación del proceso de desinstalación".into(),
                source: None,
            });
        }

        let mut exit_code = 0u32;
        unsafe { GetExitCodeProcess(pi.hProcess, &mut exit_code) }.map_err(|e| {
            CoreError::Uninstaller {
                message: format!("GetExitCodeProcess falló: {}", e),
                source: None,
            }
        })?;

        Ok(exit_code)
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
                    other.id != app.id && other.publisher.eq_ignore_ascii_case(pub_name)
                })
        } else {
            true
        };

        if let Some(ref loc) = app.install_location {
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
        if let Ok(pf) = std::env::var("ProgramFiles") {
            scan_roots.push(("Archivos de Programa (x64)", PathBuf::from(pf)));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            scan_roots.push(("Archivos de Programa (x86)", PathBuf::from(pf86)));
        }
        if let Ok(pd) = get_program_data_dir() {
            scan_roots.push(("ProgramData", pd));
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
        if let Some(pub_dir) = known_folder(&FOLDERID_Public) {
            shortcut_roots.push(("Escritorio Público", pub_dir.join("Desktop")));
        }
        if let Some(roaming) = dirs::config_dir() {
            shortcut_roots.push((
                "Menú Inicio (Usuario)",
                roaming.join(r"Microsoft\Windows\Start Menu\Programs"),
            ));
        }
        if let Ok(pd) = get_program_data_dir() {
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
        if RegKey::predef(uninst_hive)
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
                        let uid = format!("{}:{}:{}", is_hkcu, wow32, sub_path.to_ascii_lowercase());
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
                                    });
                                }
                            } else if let Ok(pub_key) = sw.open_subkey_with_flags(&key_name, flags) {
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
                        });
                    }
                }
            }
        }

        Ok(items)
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
                        report.removed_count += 1;
                        continue;
                    }
                    if let Err(e) = Self::purge_leftovers_verified_dir(target) {
                        if !target.exists() {
                            report.removed_count += 1;
                        } else {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", target.display(), e));
                        }
                    } else {
                        report.removed_count += 1;
                        report.bytes_freed = report.bytes_freed.saturating_add(item.size_bytes);
                    }
                }
                LeftoverKind::File => {
                    let Some(ref target) = item.path else {
                        continue;
                    };
                    if !target.exists() {
                        report.removed_count += 1;
                        continue;
                    }
                    let res = target
                        .parent()
                        .ok_or_else(|| CoreError::Validation("Archivo sin padre".into()))
                        .and_then(|p| canonical_dos_path(p))
                        .and_then(|canon| delete_verified_file_under(target, Some(&canon)));
                    if let Err(e) = res {
                        if !target.exists() {
                            report.removed_count += 1;
                        } else {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", target.display(), e));
                        }
                    } else {
                        report.removed_count += 1;
                        report.bytes_freed = report.bytes_freed.saturating_add(item.size_bytes);
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
                    let del_res = if !item.reg_is_hkcu && item.reg_wow64_32 {
                        if let Some((parent_path, leaf)) = subkey.rsplit_once('\\') {
                            root.open_subkey_with_flags(parent_path, flags)
                                .and_then(|pk| pk.delete_subkey_all(leaf))
                        } else {
                            root.delete_subkey_all(subkey)
                        }
                    } else {
                        root.delete_subkey_all(subkey)
                    };

                    match del_res {
                        Ok(()) => report.removed_count += 1,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            report.removed_count += 1;
                        }
                        Err(e) => {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", item.target_display, e));
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
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            report.removed_count += 1;
                        }
                        Err(e) => {
                            report.failed_count += 1;
                            report.errors.push(format!("{}: {}", item.target_display, e));
                        }
                    }
                }
            }
        }

        report
    }

    fn purge_leftovers_verified_dir(target: &Path) -> CoreResult<()> {
        let mut allowed_roots: Vec<PathBuf> = Vec::new();
        if let Ok(pf) = std::env::var("ProgramFiles") {
            allowed_roots.push(PathBuf::from(pf));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            allowed_roots.push(PathBuf::from(pf86));
        }
        if let Ok(pd) = get_program_data_dir() {
            allowed_roots.push(pd.join(r"Microsoft\Windows\Start Menu\Programs"));
            allowed_roots.push(pd);
        }
        if let Some(roaming) = dirs::config_dir() {
            allowed_roots.push(roaming.join(r"Microsoft\Windows\Start Menu\Programs"));
            allowed_roots.push(roaming);
        }
        if let Some(local) = known_folder(&FOLDERID_LocalAppData) {
            allowed_roots.push(local);
        }
        if let Some(desk) = dirs::desktop_dir() {
            allowed_roots.push(desk);
        }

        let parent = target
            .parent()
            .ok_or_else(|| CoreError::Validation("Ruta sin padre".into()))?;
        let under_allowed = allowed_roots.iter().any(|r| target.starts_with(r));

        if !under_allowed || !is_safe_non_root_dir(target, 2) {
            return Err(CoreError::Validation(format!(
                "Ruta fuera de raíces permitidas para purga: '{}'",
                target.display()
            )));
        }

        let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_protected_name(name) {
            return Err(CoreError::Validation(format!(
                "Directorio protegido contra purga: '{}'",
                target.display()
            )));
        }

        delete_tree_verified(target, parent, 12)?;
        Ok(())
    }

    pub fn purge_leftovers(target: &Path) -> CoreResult<()> {
        Self::purge_leftovers_verified_dir(target)
    }

    pub fn uninstall_self() -> CoreResult<()> {
        ElevationManager::require_elevated("Desinstalar OtterSwiss y revertir cambios")?;

        let data_dir = get_program_data_dir()?.join("OtterSwiss");
        if data_dir.exists() {
            let engine = OptimizationEngine::new(&data_dir);
            engine.rollback(false)?;
            let parent = data_dir
                .parent()
                .ok_or_else(|| CoreError::Validation("ProgramData sin padre".into()))?;
            delete_tree_verified(&data_dir, parent, 6)?;
        }

        Ok(())
    }
}

fn is_safe_registry_subkey_to_delete(subkey: &str) -> bool {
    let low = subkey.trim().to_ascii_lowercase();
    if !low.starts_with("software\\") {
        return false;
    }
    let rest = &low["software\\".len()..];
    if rest.is_empty()
        || rest == "microsoft"
        || rest == "classes"
        || rest == "policies"
        || rest == "wow6432node"
        || rest == r"microsoft\windows\currentversion\uninstall"
        || (rest.starts_with("microsoft\\windows\\")
            && !rest.starts_with(r"microsoft\windows\currentversion\uninstall\"))
    {
        return false;
    }
    true
}