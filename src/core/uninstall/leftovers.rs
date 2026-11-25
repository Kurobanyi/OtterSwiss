//! Búsqueda de restos después de desinstalar: carpetas, accesos directos y claves de registro con el nombre de la app.

use std::collections::HashSet;
use std::path::PathBuf;

use winreg::enums::KEY_READ;

use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_Public};

use super::inventory::dir_size_bounded;
use super::{InstalledApp, LeftoverItem, LeftoverKind, UninstallerManager};
use crate::core::error::CoreResult;
use crate::core::platform::folders::known_folder;
use crate::core::platform::folders::{program_files_dir, program_files_x86_dir};
use crate::core::platform::registry::{RegHive, RegLocation, RegView};
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
        reg_location: reg_location(false, false),
        reg_subkey: None,
        reg_value_name: None,
        size_bytes: sz,
        recommended: true,
    });
}

/// CL-04: convierte los datos de la entrada Uninstall (por usuario, 32 bits) a una ubicación de registro.
pub(super) fn reg_location(is_hkcu: bool, wow64_32: bool) -> RegLocation {
    RegLocation {
        hive: if is_hkcu {
            RegHive::Hkcu
        } else {
            RegHive::Hklm
        },
        view: if wow64_32 && !is_hkcu {
            RegView::Wow64_32
        } else {
            RegView::Native
        },
    }
}

impl UninstallerManager {
    pub fn scan_leftovers(app: &InstalledApp) -> CoreResult<Vec<LeftoverItem>> {
        let mut scan = LeftoverScan::new(app);
        scan.install_location();
        scan.folders();
        scan.shortcuts();
        scan.software_keys();
        scan.run_values();
        Ok(scan.finish())
    }

    /// `true` si la entrada Uninstall de la app sigue en el registro.
    pub fn is_still_registered(app: &InstalledApp) -> bool {
        let loc = reg_location(app.is_per_user, app.is_wow64);
        loc.root()
            .open_subkey_with_flags(&app.registry_subkey, loc.flags(KEY_READ))
            .is_ok()
    }
}

/// CL-05: estado de una búsqueda de restos. Cada fuente (carpetas, accesos directos, claves `Software`, valores
/// `Run`) es un método; `finish` aplica las reglas de SYS-04 y marca los recomendados.
struct LeftoverScan<'a> {
    app: &'a InstalledApp,
    tokens: Vec<String>,
    publisher_tok: Option<String>,
    /// Si otra app instalada comparte fabricante, su carpeta de fabricante no es un resto entero.
    other_apps_same_publisher: bool,
    /// Si la app sigue registrada (el desinstalador falló, se canceló o aún no se ejecutó), su carpeta de
    /// instalación y su entrada Uninstall no son «rastros»: borrarlas rompería una app instalada.
    still_registered: bool,
    /// Una entrada de HKCU la puede crear cualquier proceso del usuario: no decide qué se borra fuera de su
    /// propio perfil (Program Files, ProgramData, HKLM).
    user_scope_only: bool,
    items: Vec<LeftoverItem>,
    seen_paths: HashSet<String>,
    seen_reg: HashSet<String>,
}

impl<'a> LeftoverScan<'a> {
    fn new(app: &'a InstalledApp) -> Self {
        let (tokens, publisher_tok) = build_app_tokens(app);
        let other_apps_same_publisher = match publisher_tok {
            Some(ref pub_name) => UninstallerManager::list_installed_apps()
                .unwrap_or_default()
                .into_iter()
                .any(|other| {
                    other.id != app.id
                        && sanitize_token(&other.publisher)
                            .is_some_and(|p| p.eq_ignore_ascii_case(pub_name))
                }),
            None => true,
        };
        Self {
            app,
            tokens,
            publisher_tok,
            other_apps_same_publisher,
            still_registered: UninstallerManager::is_still_registered(app),
            user_scope_only: app.is_per_user,
            items: Vec::new(),
            seen_paths: HashSet::new(),
            seen_reg: HashSet::new(),
        }
    }

    fn push_dir(&mut self, category: &'static str, path: PathBuf) {
        push_dir_candidate(category, path, &mut self.seen_paths, &mut self.items);
    }

    fn is_publisher_name(&self, name: &str) -> bool {
        self.publisher_tok
            .as_ref()
            .is_some_and(|p| name.eq_ignore_ascii_case(p))
    }

    fn install_location(&mut self) {
        if self.still_registered {
            return;
        }
        if let Some(loc) = self.app.install_location.clone() {
            if loc.is_dir() && is_safe_non_root_dir(&loc, 2) {
                self.push_dir("Carpeta de instalación", loc);
            }
        }
    }

    /// Carpetas con el nombre de la app (o de su fabricante) en Program Files, ProgramData y AppData.
    fn folders(&mut self) {
        let mut roots: Vec<(&'static str, PathBuf)> = Vec::new();
        if !self.user_scope_only {
            if let Some(pf) = program_files_dir() {
                roots.push(("Archivos de Programa (x64)", pf));
            }
            if let Some(pf86) = program_files_x86_dir() {
                roots.push(("Archivos de Programa (x86)", pf86));
            }
            if let Ok(pd) = get_program_data_dir() {
                roots.push(("ProgramData", pd));
            }
        }
        if let Some(roaming) = dirs::config_dir() {
            roots.push(("AppData Roaming", roaming));
        }
        if let Some(local) = known_folder(&FOLDERID_LocalAppData) {
            roots.push(("AppData Local", local));
        }

        for (cat, root) in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                let fname = entry.file_name().to_string_lossy().into_owned();
                if !p.is_dir() || is_protected_name(&fname) {
                    continue;
                }
                if matches_any_token(&fname, &self.tokens) {
                    self.push_dir(cat, p);
                } else if self.is_publisher_name(&fname) {
                    if !self.other_apps_same_publisher {
                        self.push_dir(cat, p);
                    } else if let Ok(sub_entries) = std::fs::read_dir(&p) {
                        for sub in sub_entries.flatten() {
                            let sp = sub.path();
                            let sname = sub.file_name().to_string_lossy().into_owned();
                            if sp.is_dir() && matches_any_token(&sname, &self.tokens) {
                                self.push_dir(cat, sp);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Accesos directos y carpetas del menú Inicio y del Escritorio.
    fn shortcuts(&mut self) {
        let mut roots = Vec::new();
        if let Some(desk) = dirs::desktop_dir() {
            roots.push(("Escritorio", desk));
        }
        if let Some(pub_dir) = known_folder(&FOLDERID_Public).filter(|_| !self.user_scope_only) {
            roots.push(("Escritorio Público", pub_dir.join("Desktop")));
        }
        if let Some(roaming) = dirs::config_dir() {
            roots.push((
                "Menú Inicio (Usuario)",
                roaming.join(r"Microsoft\Windows\Start Menu\Programs"),
            ));
        }
        if let Some(pd) = get_program_data_dir()
            .ok()
            .filter(|_| !self.user_scope_only)
        {
            roots.push((
                "Menú Inicio (Sistema)",
                pd.join(r"Microsoft\Windows\Start Menu\Programs"),
            ));
        }

        for (cat, root) in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                let fname = entry.file_name().to_string_lossy().into_owned();
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| fname.clone());
                if p.is_dir() {
                    if matches_any_token(&fname, &self.tokens) {
                        self.push_dir(cat, p);
                    }
                } else if fname.to_ascii_lowercase().ends_with(".lnk")
                    && matches_any_token(&stem, &self.tokens)
                    && self
                        .seen_paths
                        .insert(p.to_string_lossy().to_ascii_lowercase())
                {
                    let sz = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    self.items.push(LeftoverItem {
                        id: format!("file:{}", p.display()),
                        kind: LeftoverKind::File,
                        category: cat.to_string(),
                        target_display: p.display().to_string(),
                        path: Some(p),
                        reg_location: reg_location(false, false),
                        reg_subkey: None,
                        reg_value_name: None,
                        size_bytes: sz,
                        recommended: true,
                    });
                }
            }
        }
    }

    /// Una clave `Software\...` como resto, sin repetir.
    fn push_reg_key(&mut self, loc: RegLocation, category: &str, subkey: String) {
        let wow32 = loc.view == RegView::Wow64_32;
        let is_hkcu = loc.hive == RegHive::Hkcu;
        let uid = format!("{}:{}:{}", is_hkcu, wow32, subkey.to_ascii_lowercase());
        if !self.seen_reg.insert(uid) {
            return;
        }
        self.items.push(LeftoverItem {
            id: format!("reg:{}:{}:{}", is_hkcu, wow32, subkey),
            kind: LeftoverKind::RegistryKey,
            category: category.to_string(),
            target_display: format!("{}\\{}", loc.hive.as_str(), subkey),
            path: None,
            reg_location: loc,
            reg_subkey: Some(subkey),
            reg_value_name: None,
            size_bytes: 0,
            recommended: true,
        });
    }

    /// Claves `Software\<app>` (o `Software\<fabricante>\<app>`) en HKCU y en las dos vistas de HKLM.
    fn software_keys(&mut self) {
        let scopes = [
            (reg_location(true, false), "Registro HKCU\\Software"),
            (reg_location(false, false), "Registro HKLM\\SOFTWARE"),
            (
                reg_location(false, true),
                "Registro HKLM\\SOFTWARE (32-bit)",
            ),
        ];
        for (loc, cat) in scopes {
            if self.user_scope_only && loc.hive != RegHive::Hkcu {
                continue;
            }
            let flags = loc.flags(KEY_READ);
            let Ok(sw) = loc.root().open_subkey_with_flags("Software", flags) else {
                continue;
            };
            for key_name in sw.enum_keys().flatten() {
                if is_protected_name(&key_name)
                    || ["classes", "policies", "wow6432node"]
                        .iter()
                        .any(|k| key_name.eq_ignore_ascii_case(k))
                {
                    continue;
                }
                let sub_path = format!(r"Software\{}", key_name);
                if matches_any_token(&key_name, &self.tokens) {
                    self.push_reg_key(loc, cat, sub_path);
                } else if self.is_publisher_name(&key_name) {
                    if !self.other_apps_same_publisher {
                        self.push_reg_key(loc, cat, sub_path);
                    } else if let Ok(pub_key) = sw.open_subkey_with_flags(&key_name, flags) {
                        for sub_name in pub_key.enum_keys().flatten() {
                            if matches_any_token(&sub_name, &self.tokens) {
                                self.push_reg_key(loc, cat, format!(r"{}\{}", sub_path, sub_name));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Valores de inicio automático (`Run`) que apuntan a la carpeta de la app o llevan su nombre.
    fn run_values(&mut self) {
        const RUN_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
        for loc in [
            reg_location(true, false),
            reg_location(false, false),
            reg_location(false, true),
        ] {
            if self.user_scope_only && loc.hive != RegHive::Hkcu {
                continue;
            }
            let Ok(run_key) = loc
                .root()
                .open_subkey_with_flags(RUN_PATH, loc.flags(KEY_READ))
            else {
                continue;
            };
            for (val_name, val_data) in run_key.enum_values().flatten() {
                let cmd = val_data.to_string().to_ascii_lowercase();
                let loc_match = self.app.install_location.as_ref().is_some_and(|dir| {
                    let ls = dir.to_string_lossy().to_ascii_lowercase();
                    !ls.is_empty() && is_safe_non_root_dir(dir, 2) && cmd.contains(&ls)
                });
                if loc_match || matches_any_token(&val_name, &self.tokens) {
                    self.items.push(LeftoverItem {
                        id: format!(
                            "run:{}:{}:{}",
                            loc.hive == RegHive::Hkcu,
                            loc.view == RegView::Wow64_32,
                            val_name
                        ),
                        kind: LeftoverKind::RegistryValue,
                        category: "Inicio automático (Run)".into(),
                        target_display: format!(
                            "{}\\{} -> {}",
                            loc.hive.as_str(),
                            RUN_PATH,
                            val_name
                        ),
                        path: None,
                        reg_location: loc,
                        reg_subkey: Some(RUN_PATH.to_string()),
                        reg_value_name: Some(val_name),
                        size_bytes: 0,
                        recommended: true,
                    });
                }
            }
        }
    }

    fn finish(mut self) -> Vec<LeftoverItem> {
        // SYS-04: con la app aún registrada, su carpeta de instalación (y lo que cuelga de ella) tampoco
        // puede entrar por el barrido de Program Files por nombre: borrarla rompería una app instalada.
        if self.still_registered {
            if let Some(loc) = self.app.install_location.as_ref() {
                let loc_low = loc
                    .to_string_lossy()
                    .trim_end_matches('\\')
                    .to_ascii_lowercase();
                self.items.retain(|i| {
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

        let app_tokens_low: Vec<String> =
            self.tokens.iter().map(|t| t.to_ascii_lowercase()).collect();
        let publisher_low = self.publisher_tok.as_ref().map(|p| p.to_ascii_lowercase());
        for item in &mut self.items {
            item.recommended =
                is_recommended_leftover(item, &app_tokens_low, publisher_low.as_deref());
        }
        self.items
    }
}
