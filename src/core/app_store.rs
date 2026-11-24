use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, TokenPrimary, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE,
    TOKEN_QUERY,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::Threading::{
    CreateProcessWithTokenW, GetExitCodeProcess, OpenProcess, OpenProcessToken,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, LOGON_WITH_PROFILE, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{FOLDERID_Public, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::RegKey;

use crate::core::elevation::ElevationManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::safe_fs::{create_verified_new_file, delete_verified_file};
use crate::core::trust::{ensure_admin_only_data_dir, secure_download_and_verify, DownloadPolicy};
use crate::core::win_common::{
    to_wide_null, wide_ptr_to_string_and_free, JobObject, SafeHandle,
};

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
        version: "10.63",
        url: "https://github.com/JosefNemec/Playnite/releases/download/10.63/Playnite1063.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("675816d029dfdaa1fa458316a5ef726d27dc9527e3d7041bf76cb9fe6037cedc"),
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
        version: "31.1.2",
        url: "https://github.com/obsproject/obs-studio/releases/download/31.1.2/OBS-Studio-31.1.2-Windows-x64-Installer.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("2505c88de000799c274e8abd09743295b23a4b5c5bf5d3a3c294b2e001585dba"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 200 * 1024 * 1024,
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
        version: "24.09",
        url: "https://www.7-zip.org/a/7z2409-x64.exe",
        allowed_hosts: &[
            "www.7-zip.org",
            "7-zip.org",
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: Some("bdd1a33de78618d16ee4ce148b849932c05d0015491c34887846d431d29f308e"),
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
        expected_sha256: None,
        signer_cn: Some("Open Source Developer, Christophe Dumez|Christophe Dumez|SignPath Foundation|qBittorrent"),
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
        version: "2.47.1",
        url: "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.1/Git-2.47.1-64-bit.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: None,
        signer_cn: Some("Johannes Schindelin"),
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
        version: "8.7.5",
        url: "https://github.com/notepad-plus-plus/notepad-plus-plus/releases/download/v8.7.5/npp.8.7.5.Installer.x64.exe",
        allowed_hosts: &[
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ],
        expected_sha256: None,
        signer_cn: Some("Notepad++"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 15 * 1024 * 1024,
        uninstall_display_prefix: "Notepad++",
    },
];

struct EnvBlockGuard(*mut std::ffi::c_void);
impl Drop for EnvBlockGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = DestroyEnvironmentBlock(self.0);
            }
        }
    }
}

fn get_msiexec_path() -> CoreResult<PathBuf> {
    let mut buf = [0u16; 260];
    let len = unsafe {
        windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut buf))
    } as usize;
    if len == 0 || len >= buf.len() {
        return Err(CoreError::SystemTool {
            message: "No se pudo localizar System32 para msiexec.exe".into(),
            source: None,
        });
    }
    let sys_dir = String::from_utf16_lossy(&buf[..len]);
    Ok(PathBuf::from(sys_dir).join("msiexec.exe"))
}

fn get_public_unelevated_staging_dir() -> CoreResult<PathBuf> {
    let raw_ptr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Public, KF_FLAG_DEFAULT, None) }.map_err(|e| {
            CoreError::SystemTool {
                message: format!("SHGetKnownFolderPath(FOLDERID_Public) falló: {}", e),
                source: None,
            }
        })?;
    let pub_dir = wide_ptr_to_string_and_free(raw_ptr);
    if pub_dir.is_empty() {
        return Err(CoreError::SystemTool {
            message: "FOLDERID_Public devolvió ruta vacía".into(),
            source: None,
        });
    }
    let staging = PathBuf::from(pub_dir).join("OtterSwiss_Staging");
    std::fs::create_dir_all(&staging)?;
    Ok(staging)
}

fn duplicate_desktop_shell_primary_token() -> CoreResult<SafeHandle> {
    let shell_hwnd = unsafe { GetShellWindow() };
    if shell_hwnd.0.is_null() {
        return Err(CoreError::SystemTool {
            message: "No se encontró el Explorador de escritorio para ejecutar sin elevación"
                .into(),
            source: None,
        });
    }

    let mut shell_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(shell_hwnd, Some(&mut shell_pid));
    }
    if shell_pid == 0 {
        return Err(CoreError::SystemTool {
            message: "No se pudo obtener el PID de explorer.exe".into(),
            source: None,
        });
    }

    let shell_proc_raw =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, shell_pid) }.map_err(|e| {
            CoreError::SystemTool {
                message: format!("OpenProcess(explorer.exe) falló: {}", e),
                source: None,
            }
        })?;
    let shell_proc = SafeHandle::new(shell_proc_raw)
        .ok_or_else(|| CoreError::from_win32("Handle de explorer.exe inválido"))?;

    let mut shell_tok_raw = HANDLE::default();
    unsafe { OpenProcessToken(shell_proc.raw(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut shell_tok_raw) }
        .map_err(|e| CoreError::SystemTool {
            message: format!("OpenProcessToken(explorer.exe) falló: {}", e),
            source: None,
        })?;
    let shell_tok = SafeHandle::new(shell_tok_raw)
        .ok_or_else(|| CoreError::from_win32("Token de explorer.exe inválido"))?;

    let mut primary_raw = HANDLE::default();
    unsafe {
        DuplicateTokenEx(
            shell_tok.raw(),
            TOKEN_ALL_ACCESS,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut primary_raw,
        )
    }
    .map_err(|e| CoreError::SystemTool {
        message: format!("DuplicateTokenEx falló: {}", e),
        source: None,
    })?;

    SafeHandle::new(primary_raw)
        .ok_or_else(|| CoreError::from_win32("Token primario duplicado inválido"))
}

fn copy_verified_to_path(src_path: &Path, dst_path: &Path) -> CoreResult<()> {
    let mut src = File::open(src_path)?;
    let (mut dst, _) = create_verified_new_file(dst_path, true)?;
    std::io::copy(&mut src, &mut dst)?;
    dst.flush()?;
    dst.sync_all()?;
    Ok(())
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
    if let Ok(pf) = std::env::var("ProgramFiles") {
        if Path::new(&pf).join(r"Spotify\Spotify.exe").is_file() {
            return true;
        }
    }
    if RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Spotify")
        .is_ok()
    {
        return true;
    }
    false
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

    pub fn find_newer_curated_version(
        display_name: &str,
        current_version: &str,
    ) -> Option<&'static CuratedPackage> {
        let dn_low = display_name.to_ascii_lowercase();
        let cur_pkg = CURATED_CATALOG.iter().find(|p| {
            let pref = p.uninstall_display_prefix.to_ascii_lowercase();
            dn_low == pref
                || dn_low.starts_with(&format!("{} ", pref))
                || dn_low.starts_with(&format!("{}-", pref))
        })?;

        if cur_pkg.version == "Evergreen" {
            return None;
        }

        let installed_clean = current_version.trim().trim_start_matches('v');
        let catalog_clean = cur_pkg.version.trim().trim_start_matches('v');

        if !installed_clean.is_empty() && installed_clean != catalog_clean {
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
            let pref = pkg.uninstall_display_prefix.to_ascii_lowercase();
            let found = display_names.iter().any(|name| {
                let low = name.to_ascii_lowercase();
                low == pref
                    || low.starts_with(&format!("{} ", pref))
                    || low.starts_with(&format!("{}-", pref))
            });
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

        let secure_dir = ensure_admin_only_data_dir()?.join("staging");
        std::fs::create_dir_all(&secure_dir)?;
        let ext = if pkg.is_msi { "msi" } else { "exe" };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let installer_path =
            secure_dir.join(format!("{}_{}_{}.{}", pkg.id, std::process::id(), nonce, ext));

        let policy = DownloadPolicy {
            url: pkg.url,
            allowed_final_hosts: pkg.allowed_hosts,
            expected_sha256_hex: pkg.expected_sha256,
            expected_cert_thumbprint_sha1: pkg.cert_thumbprint_sha1,
            expected_signer_cn: pkg.signer_cn,
            max_bytes: pkg.max_bytes,
        };

        let downloaded_file = match secure_download_and_verify(&installer_path, &policy) {
            Ok(f) => f,
            Err(e) => {
                let _ = delete_verified_file(&installer_path);
                return Err(e);
            }
        };
        drop(downloaded_file);

        if cancel.load(Ordering::Acquire) {
            let _ = delete_verified_file(&installer_path);
            return Err(CoreError::Cancelled);
        }

        let mut unelevated_copy: Option<PathBuf> = None;
        let run_path = if pkg.run_unelevated {
            let pub_staging = match get_public_unelevated_staging_dir() {
                Ok(d) => d,
                Err(e) => {
                    let _ = delete_verified_file(&installer_path);
                    return Err(e);
                }
            };
            let target = pub_staging.join(format!(
                "{}_{}_{}.{}",
                pkg.id,
                std::process::id(),
                nonce,
                ext
            ));
            if let Err(e) = copy_verified_to_path(&installer_path, &target) {
                let _ = delete_verified_file(&target);
                let _ = delete_verified_file(&installer_path);
                return Err(e);
            }
            unelevated_copy = Some(target.clone());
            target
        } else {
            installer_path.clone()
        };

        let cleanup_files = || {
            if let Some(ref copy_p) = unelevated_copy {
                let _ = delete_verified_file(copy_p);
            }
            let _ = delete_verified_file(&installer_path);
        };

        let (exe_to_run, cmd_line) = if pkg.is_msi {
            let msiexec = match get_msiexec_path() {
                Ok(p) => p,
                Err(e) => {
                    cleanup_files();
                    return Err(e);
                }
            };
            let cmd = format!(
                "\"{}\" /i \"{}\" {}",
                msiexec.display(),
                run_path.display(),
                pkg.silent_args
            );
            (msiexec, cmd)
        } else {
            let cmd = format!("\"{}\" {}", run_path.display(), pkg.silent_args);
            (run_path.clone(), cmd)
        };

        let job = JobObject::new_kill_on_close()?;
        let mut cmd_wide = to_wide_null(&cmd_line);

        let mut pi = PROCESS_INFORMATION::default();
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };

        if pkg.run_unelevated {
            let user_token = match duplicate_desktop_shell_primary_token() {
                Ok(t) => t,
                Err(e) => {
                    cleanup_files();
                    return Err(e);
                }
            };

            let mut env_block: *mut std::ffi::c_void = std::ptr::null_mut();
            if let Err(e) = unsafe { CreateEnvironmentBlock(&mut env_block, user_token.raw(), false) }
            {
                cleanup_files();
                return Err(CoreError::SystemTool {
                    message: format!("CreateEnvironmentBlock falló: {}", e),
                    source: None,
                });
            }
            let _env_guard = EnvBlockGuard(env_block);

            if let Err(e) = unsafe {
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
            } {
                cleanup_files();
                return Err(CoreError::SystemTool {
                    message: format!("CreateProcessWithTokenW para '{}' falló: {}", pkg.name, e),
                    source: None,
                });
            }
        } else {
            let wide_exe = to_wide_null(&exe_to_run);
            if let Err(e) = unsafe {
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
            } {
                cleanup_files();
                return Err(CoreError::SystemTool {
                    message: format!("Error ejecutando instalador de '{}': {}", pkg.name, e),
                    source: None,
                });
            }
        }

        let _proc_handle = SafeHandle::new(pi.hProcess);
        let _thread_handle = SafeHandle::new(pi.hThread);

        if let Some(h) = _proc_handle.as_ref() {
            job.assign(h.raw())?;
        }

        let mut elapsed_ms = 0u32;
        let max_wait_ms = 300_000u32;
        loop {
            if cancel.load(Ordering::Acquire) {
                job.terminate_all(1);
                cleanup_files();
                return Err(CoreError::Cancelled);
            }
            let wait_res = unsafe { WaitForSingleObject(pi.hProcess, 500) };
            if wait_res == windows::Win32::Foundation::WAIT_OBJECT_0 {
                break;
            }
            elapsed_ms = elapsed_ms.saturating_add(500);
            if elapsed_ms >= max_wait_ms {
                job.terminate_all(1);
                cleanup_files();
                return Err(CoreError::SystemTool {
                    message: format!("Tiempo límite excedido instalando '{}'", pkg.name),
                    source: None,
                });
            }
        }

        let mut exit_code = 0u32;
        if let Err(e) = unsafe { GetExitCodeProcess(pi.hProcess, &mut exit_code) } {
            cleanup_files();
            return Err(CoreError::SystemTool {
                message: format!("GetExitCodeProcess falló: {}", e),
                source: None,
            });
        }

        cleanup_files();

        if exit_code != 0 && exit_code != 3010 && exit_code != 1641 && exit_code != 1638 {
            return Err(CoreError::SystemTool {
                message: format!("Instalador de '{}' terminó con código {}", pkg.name, exit_code),
                source: None,
            });
        }

        Ok(())
    }
}