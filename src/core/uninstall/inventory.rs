//! Apps instaladas (claves Uninstall de HKLM y HKCU) y estimación de su tamaño.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::RegKey;

use super::leftovers::is_protected_name;
use super::runner::parse_cmd_exe_and_args;
use super::{InstalledApp, UninstallerManager};
use crate::core::error::CoreResult;
use crate::core::platform::safe_fs::is_safe_non_root_dir;

pub(super) fn dir_size_bounded(dir: &Path, max_depth: usize) -> u64 {
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

pub(super) fn is_safe_app_folder_for_size(dir: &Path) -> bool {
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
}
