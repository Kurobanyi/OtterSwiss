//! Búsqueda de restos después de desinstalar: carpetas, accesos directos y claves de registro con el nombre de la app.

use std::collections::HashSet;
use std::path::PathBuf;

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::RegKey;

use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_Public};

use super::inventory::dir_size_bounded;
use super::{InstalledApp, LeftoverItem, LeftoverKind, UninstallerManager};
use crate::core::error::CoreResult;
use crate::core::platform::folders::known_folder;
use crate::core::platform::folders::{program_files_dir, program_files_x86_dir};
use crate::core::platform::safe_fs::is_safe_non_root_dir;
use crate::core::security::data_dir::get_program_data_dir;

pub(super) const PROTECTED_DIR_NAMES: &[&str] = &[
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

pub(super) fn is_protected_name(name: &str) -> bool {
    let clean = name.trim().to_ascii_lowercase();
    if clean.len() < 3 {
        return true;
    }
    PROTECTED_DIR_NAMES.iter().any(|&p| clean == p)
}

pub(super) fn sanitize_token(raw: &str) -> Option<String> {
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

pub(super) fn build_app_tokens(app: &InstalledApp) -> (Vec<String>, Option<String>) {
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
pub(super) fn matches_any_token(candidate: &str, tokens: &[String]) -> bool {
    let low = candidate.trim().to_ascii_lowercase();
    if low.len() < 3 || is_protected_name(&low) {
        return false;
    }
    tokens.iter().any(|t| low == t.to_ascii_lowercase())
}

/// Nombres de carpeta demasiado genéricos para atribuirlos a una sola app.
pub(super) const GENERIC_FOLDER_TOKENS: &[&str] = &[
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
pub(super) fn is_recommended_leftover(
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

pub(super) fn push_dir_candidate(
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

impl UninstallerManager {
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
}
