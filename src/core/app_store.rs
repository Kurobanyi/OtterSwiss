use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::RegKey;

use crate::core::elevation::ElevationManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::process::{
    duplicate_desktop_shell_primary_token, run_in_job, system32_dir, user_profile_dir, Launch,
};
use crate::core::safe_fs::{
    canonical_dos_path, create_verified_new_file, delete_verified_empty_dir_under,
    delete_verified_file, delete_verified_file_under, path_is_strictly_under,
};
use crate::core::trust::{
    ensure_admin_only_data_dir, reopen_verified_for_exec, secure_download_and_verify,
    DownloadPolicy,
};
use crate::core::win_common::SafeHandle;

/// UI-15: la URL y los argumentos del catálogo vienen en español; si Windows no está en español se
/// piden en inglés (Firefox `lang=en-US`, Battle.net `--lang=enUS`).
fn localize_catalog_text(text: &str, spanish: bool) -> String {
    if spanish {
        text.to_string()
    } else {
        text.replace("lang=es-ES", "lang=en-US")
            .replace("--lang=esES", "--lang=enUS")
    }
}

fn windows_ui_is_spanish() -> bool {
    const LANG_SPANISH: u16 = 0x0a;
    let lang_id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    lang_id & 0x3ff == LANG_SPANISH
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppCategory {
    Gaming,
    Communication,
    Utilities,
    Browsers,
    Development,
}

impl AppCategory {
    pub fn label(&self, en: bool) -> &'static str {
        if en {
            match self {
                Self::Gaming => "Gaming & Launchers",
                Self::Communication => "Communication & Media",
                Self::Utilities => "Utilities",
                Self::Browsers => "Browsers",
                Self::Development => "Development",
            }
        } else {
            match self {
                Self::Gaming => "Gaming y Launchers",
                Self::Communication => "Comunicación y Multimedia",
                Self::Utilities => "Utilidades",
                Self::Browsers => "Navegadores",
                Self::Development => "Desarrollo",
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CuratedPackage {
    pub id: &'static str,
    pub name: &'static str,
    pub category: AppCategory,
    pub copyright_holder: &'static str,
    pub license: &'static str,
    pub official_site: &'static str,
    pub description_es: &'static str,
    pub description_en: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub allowed_hosts: &'static [&'static str],
    pub expected_sha256: Option<&'static str>,
    pub signer_cn: Option<&'static str>,
    pub cert_thumbprint_sha1: Option<&'static str>,
    pub is_msi: bool,
    pub run_unelevated: bool,
    pub silent_args: &'static str,
    pub max_bytes: u64,
    pub uninstall_display_prefix: &'static str,
}

static CURATED_CATALOG: &[CuratedPackage] = &[
    CuratedPackage {
        id: "steam",
        name: "Steam",
        category: AppCategory::Gaming,
        copyright_holder: "Valve Corporation",
        license: "Propietario",
        official_site: "https://store.steampowered.com/",
        description_es: "Plataforma oficial de juegos PC",
        description_en: "Official PC gaming platform",
        version: "Evergreen",
        url: "https://cdn.akamai.steamstatic.com/client/installer/SteamSetup.exe",
        allowed_hosts: &[
            "cdn.akamai.steamstatic.com",
            "cdn.fastly.steamstatic.com",
            "steamcdn-a.akamaihd.net",
        ],
        expected_sha256: None,
        signer_cn: Some("Valve Corp.|Valve Corporation"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 15 * 1024 * 1024,
        uninstall_display_prefix: "Steam",
    },
    CuratedPackage {
        id: "epic_games",
        name: "Epic Games Launcher",
        category: AppCategory::Gaming,
        copyright_holder: "Epic Games, Inc.",
        license: "Propietario",
        official_site: "https://store.epicgames.com/",
        description_es: "Tienda y launcher oficial de Epic",
        description_en: "Official Epic Games launcher",
        version: "Evergreen",
        url: "https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/installer/download/EpicGamesLauncherInstaller.msi",
        allowed_hosts: &[
            "launcher-public-service-prod06.ol.epicgames.com",
            "epicgames-download1.akamaized.net",
            "fastly-download.epicgames.com",
            "download.epicgames.com",
        ],
        expected_sha256: None,
        signer_cn: Some("Epic Games Inc.|Epic Games, Inc."),
        cert_thumbprint_sha1: None,
        is_msi: true,
        run_unelevated: false,
        silent_args: "/qn /norestart",
        max_bytes: 150 * 1024 * 1024,
        uninstall_display_prefix: "Epic Games Launcher",
    },
    CuratedPackage {
        id: "ubisoft_connect",
        name: "Ubisoft Connect",
        category: AppCategory::Gaming,
        copyright_holder: "Ubisoft Entertainment",
        license: "Propietario",
        official_site: "https://ubisoftconnect.com/",
        description_es: "Launcher oficial para títulos Ubisoft",
        description_en: "Official Ubisoft launcher",
        version: "Evergreen",
        url: "https://static3.cdn.ubi.com/orbit/launcher_installer/UbisoftConnectInstaller.exe",
        allowed_hosts: &["static3.cdn.ubi.com", "ubistatic3-a.akamaihd.net"],
        expected_sha256: None,
        signer_cn: Some("Ubisoft|UBISOFT ENTERTAINMENT INC.|Ubisoft Entertainment"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 320 * 1024 * 1024,
        uninstall_display_prefix: "Ubisoft Connect",
    },
    CuratedPackage {
        id: "battlenet",
        name: "Battle.net",
        category: AppCategory::Gaming,
        copyright_holder: "Blizzard Entertainment, Inc.",
        license: "Propietario",
        official_site: "https://shop.battle.net/",
        description_es: "Cliente oficial de Blizzard",
        description_en: "Official Blizzard client",
        version: "Evergreen",
        url: "https://www.battle.net/download/getInstallerForGame?os=win&gameProgram=BATTLENET_APP&version=Live",
        allowed_hosts: &[
            "www.battle.net",
            "battle.net",
            "downloader.battle.net",
            "blzddist1-a.akamaihd.net",
        ],
        expected_sha256: None,
        signer_cn: Some("Blizzard Entertainment, Inc.|Blizzard Entertainment"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "--lang=esES --installpath=\"C:\\Program Files (x86)\\Battle.net\"",
        max_bytes: 20 * 1024 * 1024,
        uninstall_display_prefix: "Battle.net",
    },
    CuratedPackage {
        id: "playnite",
        name: "Playnite",
        category: AppCategory::Gaming,
        copyright_holder: "Josef Nemec",
        license: "MIT",
        official_site: "https://playnite.link/",
        description_es: "Gestor unificado de juegos open-source",
        description_en: "Open-source game library manager",
        version: "10.65",
        url: "https://github.com/JosefNemec/Playnite/releases/download/10.65/Playnite1065.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("99959aabf2feae11c71b8a4dbd4286f01071ed2a865b02f3c069974acf8244f1"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/SP- /VERYSILENT /SUPPRESSMSGBOXES /NORESTART",
        max_bytes: 180 * 1024 * 1024,
        uninstall_display_prefix: "Playnite",
    },
    CuratedPackage {
        id: "discord",
        name: "Discord",
        category: AppCategory::Communication,
        copyright_holder: "Discord Inc.",
        license: "Propietario",
        official_site: "https://discord.com/",
        description_es: "Chat de voz y comunidades",
        description_en: "Voice chat & gaming communities",
        version: "Evergreen",
        url: "https://discord.com/api/downloads/distributions/app/installers/latest?channel=stable&platform=win&arch=x64",
        allowed_hosts: &["discord.com", "dl.discordapp.net", "stable.dl2.discordapp.net"],
        expected_sha256: None,
        signer_cn: Some("Discord Inc."),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: true,
        silent_args: "-s",
        max_bytes: 160 * 1024 * 1024,
        uninstall_display_prefix: "Discord",
    },
    CuratedPackage {
        id: "slack",
        name: "Slack",
        category: AppCategory::Communication,
        copyright_holder: "Slack Technologies, LLC",
        license: "Propietario",
        official_site: "https://slack.com/",
        description_es: "Mensajería para equipos (MSI oficial)",
        description_en: "Team messaging (Official MSI)",
        version: "Evergreen",
        url: "https://slack.com/ssb/download-win64-msi",
        allowed_hosts: &["slack.com", "downloads.slack-edge.com"],
        expected_sha256: None,
        signer_cn: Some("Slack Technologies, LLC|Slack Technologies, Inc."),
        cert_thumbprint_sha1: None,
        is_msi: true,
        run_unelevated: false,
        silent_args: "/qn /norestart",
        max_bytes: 200 * 1024 * 1024,
        uninstall_display_prefix: "Slack",
    },
    CuratedPackage {
        id: "obs",
        name: "OBS Studio",
        category: AppCategory::Communication,
        copyright_holder: "OBS Project",
        license: "GNU GPLv2",
        official_site: "https://obsproject.com/",
        description_es: "Grabación y streaming libre",
        description_en: "Open-source recording & streaming",
        version: "33.0.3",
        url: "https://github.com/obsproject/obs-studio/releases/download/33.0.3/OBS-Studio-33.0.3-Windows-x64-Installer.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("700be33c01468c2a1e43820bbcc49ba914f570f0f82250022bb680fba43b2a67"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 230 * 1024 * 1024,
        uninstall_display_prefix: "OBS Studio",
    },
    CuratedPackage {
        id: "parsec",
        name: "Parsec",
        category: AppCategory::Communication,
        copyright_holder: "Parsec Cloud, Inc.",
        license: "Propietario",
        official_site: "https://parsec.app/",
        description_es: "Escritorio remoto de baja latencia",
        description_en: "Low-latency remote desktop",
        version: "Evergreen",
        url: "https://builds.parsec.app/package/parsec-windows.exe",
        allowed_hosts: &["builds.parsec.app"],
        expected_sha256: None,
        signer_cn: Some("Unity Technologies SF|Parsec Cloud, Inc.|Unity Technologies|Unity Technologies ApS"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/silent /norun /nocleanuser /allusers",
        max_bytes: 50 * 1024 * 1024,
        uninstall_display_prefix: "Parsec",
    },
    CuratedPackage {
        id: "spotify",
        name: "Spotify",
        category: AppCategory::Communication,
        copyright_holder: "Spotify AB",
        license: "Propietario",
        official_site: "https://www.spotify.com/",
        description_es: "Música y podcasts en escritorio",
        description_en: "Desktop music & podcasts",
        version: "Evergreen",
        url: "https://download.scdn.co/SpotifyFullSetup.exe",
        allowed_hosts: &["download.scdn.co", "upgrade.scdn.co"],
        expected_sha256: None,
        signer_cn: Some("Spotify AB"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: true,
        silent_args: "/Silent",
        max_bytes: 200 * 1024 * 1024,
        uninstall_display_prefix: "Spotify",
    },
    CuratedPackage {
        id: "7zip",
        name: "7-Zip",
        category: AppCategory::Utilities,
        copyright_holder: "Igor Pavlov",
        license: "GNU LGPL / BSD",
        official_site: "https://www.7-zip.org/",
        description_es: "Compresor de archivos de alto ratio",
        description_en: "High-ratio file archiver",
        version: "26.06",
        url: "https://github.com/ip7z/7zip/releases/download/26.06/7z2606-x64.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("9b697f65ecc2d9b0152f5ee96d72dde522a7827690349d9307935dacf40f42ec"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 5 * 1024 * 1024,
        uninstall_display_prefix: "7-Zip",
    },
    CuratedPackage {
        id: "qbittorrent",
        name: "qBittorrent",
        category: AppCategory::Utilities,
        copyright_holder: "The qBittorrent Project",
        license: "GNU GPLv2",
        official_site: "https://www.qbittorrent.org/",
        description_es: "Cliente BitTorrent libre sin anuncios",
        description_en: "Ad-free open-source BitTorrent client",
        version: "5.1.4",
        url: "https://downloads.sourceforge.net/project/qbittorrent/qbittorrent-win32/qbittorrent-5.1.4/qbittorrent_5.1.4_x64_setup.exe",
        allowed_hosts: &["downloads.sourceforge.net", "*.dl.sourceforge.net"],
        // SEC-12: el instalador no trae firma Authenticode (qBittorrent firma con PGP), así que se fija
        // el SHA-256 del archivo publicado en SourceForge (descargado y medido el 2026-11-24).
        expected_sha256: Some("b279e16d682299f218edf48ddf75f7c5802263b7d6a74132d9e31fd5a255aa9d"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 80 * 1024 * 1024,
        uninstall_display_prefix: "qBittorrent",
    },
    CuratedPackage {
        id: "firefox",
        name: "Mozilla Firefox",
        category: AppCategory::Browsers,
        copyright_holder: "Mozilla Corporation",
        license: "MPL-2.0",
        official_site: "https://www.mozilla.org/firefox/",
        description_es: "Navegador libre enfocado en privacidad",
        description_en: "Privacy-focused open browser",
        version: "Evergreen",
        url: "https://download.mozilla.org/?product=firefox-latest-ssl&os=win64&lang=es-ES",
        allowed_hosts: &[
            "download.mozilla.org",
            "download-installer.cdn.mozilla.net",
        ],
        expected_sha256: None,
        signer_cn: Some("Mozilla Corporation"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "-ms /TaskbarShortcut=false",
        max_bytes: 120 * 1024 * 1024,
        uninstall_display_prefix: "Mozilla Firefox",
    },
    CuratedPackage {
        id: "git",
        name: "Git for Windows",
        category: AppCategory::Development,
        copyright_holder: "Software Freedom Conservancy / Git",
        license: "GNU GPLv2",
        official_site: "https://gitforwindows.org/",
        description_es: "Control de versiones distribuido",
        description_en: "Distributed version control",
        version: "2.56.1",
        url: "https://github.com/git-for-windows/git/releases/download/v2.56.1.windows.1/Git-2.56.1-64-bit.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("c330c72e10b408cc39994bfefdb63542b0d93d21699325c3e0bd874af8a5a559"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/VERYSILENT /NORESTART",
        max_bytes: 90 * 1024 * 1024,
        uninstall_display_prefix: "Git",
    },
    CuratedPackage {
        id: "notepadplusplus",
        name: "Notepad++",
        category: AppCategory::Development,
        copyright_holder: "Don Ho / Notepad++ Team",
        license: "GNU GPLv3",
        official_site: "https://notepad-plus-plus.org/",
        description_es: "Editor rápido de texto y código",
        description_en: "Fast source & text editor",
        version: "8.9.9.1",
        url: "https://github.com/notepad-plus-plus/notepad-plus-plus/releases/download/v8.9.9.1/npp.8.9.9.1.Installer.x64.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("ce3267f5c4e7e42f847e00f4b20234da6150647203033cdde34a69593efc3152"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 15 * 1024 * 1024,
        uninstall_display_prefix: "Notepad++",
    },
];

/// Copia de un instalador preparada en el `Temp` del usuario del Explorador, para que un instalador que corre
/// sin elevación pueda leerla. Se borra al soltarse.
struct UserStagedCopy {
    dir: PathBuf,
    path: PathBuf,
}

impl Drop for UserStagedCopy {
    fn drop(&mut self) {
        let _ = delete_verified_file_under(&self.path, Some(&self.dir));
        if let Some(parent) = self.dir.parent() {
            let _ = delete_verified_empty_dir_under(&self.dir, Some(parent));
        }
    }
}

/// Prepara la copia en `%USERPROFILE%\AppData\Local\Temp\OtterSwiss_<nonce>` del usuario de `token`.
/// La carpeta se crea nueva (falla si existe) y se comprueba que no escape del perfil por una junction.
fn stage_copy_for_user(
    token: &SafeHandle,
    src: &Path,
    file_name: &str,
    nonce: u128,
) -> CoreResult<UserStagedCopy> {
    let profile = user_profile_dir(token)?;
    let profile_canon = canonical_dos_path(&profile)?;
    let dir = profile.join(r"AppData\Local\Temp").join(format!(
        "OtterSwiss_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir(&dir)?;
    let dir_canon = canonical_dos_path(&dir)?;
    if !path_is_strictly_under(&dir_canon, &profile_canon) {
        let _ = std::fs::remove_dir(&dir);
        return Err(CoreError::Validation(format!(
            "La carpeta temporal del usuario escapa de su perfil: '{}'",
            dir_canon.display()
        )));
    }
    let path = dir.join(file_name);
    let staged = UserStagedCopy {
        dir: dir.clone(),
        path: path.clone(),
    };
    let mut src_file = File::open(src)?;
    let (mut dst, _) = create_verified_new_file(&path, false)?;
    std::io::copy(&mut src_file, &mut dst)?;
    dst.flush()?;
    dst.sync_all()?;
    Ok(staged)
}

fn check_spotify_presence_on_disk() -> bool {
    if let Some(roaming) = dirs::config_dir() {
        if roaming.join(r"Spotify\Spotify.exe").is_file() {
            return true;
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        if local.join(r"Microsoft\WindowsApps\Spotify.exe").is_file() {
            return true;
        }
        let packages = local.join("Packages");
        if let Ok(entries) = std::fs::read_dir(packages) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("spotifyab.spotifymusic")
                {
                    return true;
                }
            }
        }
    }
    if let Some(pf) = crate::core::win_common::program_files_dir() {
        if pf.join(r"Spotify\Spotify.exe").is_file() {
            return true;
        }
    }
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Spotify")
        .is_ok()
}

/// `true` si el `DisplayName` del registro es el del paquete: el prefijo exacto, o el prefijo seguido de
/// espacio/guion y un número de versión. «Git 2.47» sí; «Git LFS» o «GitHub Desktop», no.
fn display_name_matches(display_name: &str, prefix: &str) -> bool {
    let dn = display_name.trim().to_ascii_lowercase();
    let pref = prefix.trim().to_ascii_lowercase();
    if dn == pref {
        return true;
    }
    let Some(rest) = dn.strip_prefix(&pref) else {
        return false;
    };
    let mut chars = rest.chars();
    match chars.next() {
        Some(' ') | Some('-') => {}
        _ => return false,
    }
    let after = chars.as_str().trim_start_matches(['v', 'V']);
    after.chars().next().is_some_and(|c| c.is_ascii_digit()) || after.starts_with('(')
}

/// `"25.01"` → `[25, 1]`; `"2.47.1.windows.1"` → `[2, 47, 1]`. `None` si no empieza con un número.
fn parse_numeric_version(v: &str) -> Option<Vec<u64>> {
    let clean = v.trim().trim_start_matches(['v', 'V']);
    let mut parts = Vec::new();
    for seg in clean.split(['.', '-', ' ']) {
        match seg.parse::<u64>() {
            Ok(n) => parts.push(n),
            Err(_) => break,
        }
    }
    if parts.is_empty() {
        return None;
    }
    while parts.len() > 1 && parts.last() == Some(&0) {
        parts.pop();
    }
    Some(parts)
}

pub struct AppStoreManager;

impl AppStoreManager {
    pub fn get_catalog() -> &'static [CuratedPackage] {
        CURATED_CATALOG
    }

    pub fn find_package(package_id: &str) -> Option<&'static CuratedPackage> {
        CURATED_CATALOG
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(package_id))
    }

    /// Paquete del catálogo con una versión **mayor** que la instalada. La versión se compara por números,
    /// no como texto: con 7-Zip 25 instalado no se ofrece «actualizar» a una versión anterior (ERR-09).
    pub fn find_newer_curated_version(
        display_name: &str,
        current_version: &str,
    ) -> Option<&'static CuratedPackage> {
        let cur_pkg = CURATED_CATALOG
            .iter()
            .find(|p| display_name_matches(display_name, p.uninstall_display_prefix))?;

        if cur_pkg.version == "Evergreen" {
            return None;
        }

        let installed = parse_numeric_version(current_version)?;
        let catalog = parse_numeric_version(cur_pkg.version)?;
        if catalog > installed {
            Some(cur_pkg)
        } else {
            None
        }
    }

    pub fn detect_installed_ids() -> HashSet<&'static str> {
        let mut display_names = Vec::<String>::new();
        let hives = [
            (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, KEY_READ),
        ];

        for (hive, flags) in hives {
            let root = RegKey::predef(hive);
            if let Ok(uninst) = root.open_subkey_with_flags(
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                flags,
            ) {
                for sub_name in uninst.enum_keys().flatten() {
                    if let Ok(app_key) = uninst.open_subkey_with_flags(&sub_name, flags) {
                        if let Ok(dn) = app_key.get_value::<String, _>("DisplayName") {
                            let trimmed = dn.trim().to_string();
                            if !trimmed.is_empty() {
                                display_names.push(trimmed);
                            }
                        }
                    }
                }
            }
        }

        let mut installed = HashSet::new();
        for pkg in CURATED_CATALOG {
            let found = display_names
                .iter()
                .any(|name| display_name_matches(name, pkg.uninstall_display_prefix));
            if found {
                installed.insert(pkg.id);
            }
        }

        if check_spotify_presence_on_disk() {
            installed.insert("spotify");
        }

        installed
    }

    pub fn install_package(package_id: &str) -> CoreResult<()> {
        let dummy_cancel = AtomicBool::new(false);
        Self::install_package_cancellable(package_id, &dummy_cancel)
    }

    pub fn install_package_cancellable(package_id: &str, cancel: &AtomicBool) -> CoreResult<()> {
        if cancel.load(Ordering::Acquire) {
            return Err(CoreError::Cancelled);
        }

        ElevationManager::require_elevated("Instalar paquetes curados de software")?;

        let pkg = Self::find_package(package_id).ok_or_else(|| {
            CoreError::Validation(format!(
                "Paquete '{}' no encontrado en el catálogo",
                package_id
            ))
        })?;

        let staging = ensure_admin_only_data_dir()?.join("staging");
        std::fs::create_dir_all(&staging)?;
        let ext = if pkg.is_msi { "msi" } else { "exe" };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let file_name = format!("{}_{}_{}.{}", pkg.id, std::process::id(), nonce, ext);
        let installer_path = staging.join(&file_name);

        let url = localize_catalog_text(pkg.url, windows_ui_is_spanish());
        let policy = DownloadPolicy {
            url: &url,
            allowed_final_hosts: pkg.allowed_hosts,
            expected_sha256_hex: pkg.expected_sha256,
            expected_cert_thumbprint_sha1: pkg.cert_thumbprint_sha1,
            expected_signer_cn: pkg.signer_cn,
            max_bytes: pkg.max_bytes,
        };

        match secure_download_and_verify(&installer_path, &policy) {
            Ok(f) => drop(f),
            Err(e) => {
                let _ = delete_verified_file(&installer_path);
                return Err(e);
            }
        }

        let result =
            Self::run_verified_installer(pkg, &installer_path, &file_name, nonce, &policy, cancel);
        let _ = delete_verified_file(&installer_path);
        result
    }

    fn run_verified_installer(
        pkg: &CuratedPackage,
        installer_path: &Path,
        file_name: &str,
        nonce: u128,
        policy: &DownloadPolicy<'_>,
        cancel: &AtomicBool,
    ) -> CoreResult<()> {
        if cancel.load(Ordering::Acquire) {
            return Err(CoreError::Cancelled);
        }

        let user_token = if pkg.run_unelevated {
            Some(duplicate_desktop_shell_primary_token()?)
        } else {
            None
        };
        let user_copy = match user_token.as_ref() {
            Some(tok) => Some(stage_copy_for_user(tok, installer_path, file_name, nonce)?),
            None => None,
        };
        let run_path: &Path = user_copy
            .as_ref()
            .map_or(installer_path, |c| c.path.as_path());

        // Mientras este handle viva nadie puede reemplazar el instalador (SEC-05).
        let _exec_lock = reopen_verified_for_exec(run_path, policy)?;
        let working_dir = run_path.parent().unwrap_or(run_path);

        let silent_args = localize_catalog_text(pkg.silent_args, windows_ui_is_spanish());
        let (application, command_line) = if pkg.is_msi {
            let msiexec = system32_dir()?.join("msiexec.exe");
            let cmd = format!(
                "\"{}\" /i \"{}\" {}",
                msiexec.display(),
                run_path.display(),
                silent_args
            );
            (msiexec, cmd)
        } else {
            let cmd = format!("\"{}\" {}", run_path.display(), silent_args);
            (run_path.to_path_buf(), cmd)
        };

        let exit = run_in_job(
            &Launch {
                application: &application,
                command_line,
                working_dir,
                user_token: user_token.as_ref(),
            },
            Duration::from_secs(300),
            Some(cancel),
        )?;

        // 1638 = otra versión del producto ya instalada.
        if !exit.is_success() && exit.exit_code != 1638 {
            return Err(CoreError::SystemTool {
                message: format!(
                    "Instalador de '{}' terminó con código {}",
                    pkg.name, exit.exit_code
                ),
                source: None,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_del_registro_coincide_solo_con_el_paquete() {
        assert!(display_name_matches("Git", "Git"));
        assert!(display_name_matches("Git 2.47.1", "Git"));
        assert!(!display_name_matches("Git LFS version 3.5", "Git"));
        assert!(!display_name_matches("GitHub Desktop", "Git"));
        assert!(display_name_matches("7-Zip 24.09 (x64)", "7-Zip"));
        assert!(display_name_matches("Notepad++ (64-bit x64)", "Notepad++"));
    }

    #[test]
    fn version_numerica_no_ofrece_downgrade() {
        assert!(parse_numeric_version("25.01") > parse_numeric_version("24.09"));
        assert!(parse_numeric_version("2.47.1.windows.1") < parse_numeric_version("2.56.1"));
        assert_eq!(parse_numeric_version("8.7"), parse_numeric_version("8.7.0"));
        assert_eq!(parse_numeric_version("Evergreen"), None);
        // ERR-09: con una versión instalada más nueva que la del catálogo no hay «Actualizar».
        assert!(
            AppStoreManager::find_newer_curated_version("7-Zip 99.00 (x64)", "99.00").is_none()
        );
    }

    /// UI-15: con Windows en otro idioma, Firefox y Battle.net se piden en inglés.
    #[test]
    fn catalogo_se_localiza_segun_idioma() {
        let url = "https://download.mozilla.org/?product=firefox-latest-ssl&os=win64&lang=es-ES";
        assert_eq!(localize_catalog_text(url, true), url);
        assert!(localize_catalog_text(url, false).ends_with("lang=en-US"));
        assert!(
            localize_catalog_text("--lang=esES --installpath=x", false).starts_with("--lang=enUS")
        );
        assert_eq!(localize_catalog_text("/S", false), "/S");
    }
}
