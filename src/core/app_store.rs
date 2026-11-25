// src/core/app_store.rs

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
    UwpRestoration,
}

impl AppCategory {
    pub fn label(&self, en: bool) -> &'static str {
        if en {
            match self {
                Self::Gaming => "Gaming & Launchers",
                Self::Communication => "Communication & Media",
                Self::Utilities => "Utilities & System Tools",
                Self::Browsers => "Web Browsers",
                Self::Development => "Development & Editors",
                Self::UwpRestoration => "Windows UWP & Store (Restoration)",
            }
        } else {
            match self {
                Self::Gaming => "Gaming y Launchers",
                Self::Communication => "Comunicación y Multimedia",
                Self::Utilities => "Utilidades y Sistema",
                Self::Browsers => "Navegadores Web",
                Self::Development => "Desarrollo y Editores",
                Self::UwpRestoration => "Aplicaciones UWP y Windows (Restauración)",
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    Gamepad,
    Chat,
    Video,
    Browser,
    Terminal,
    Code,
    Archive,
    Download,
    Search,
    Wrench,
    Disk,
    Camera,
    Clock,
    Calculator,
    Microphone,
    Phone,
    Checklist,
    Store,
    Paint,
    Scissors,
}

#[derive(Debug, Clone)]
pub struct CuratedPackage {
    pub id: &'static str,
    pub name: &'static str,
    pub category: AppCategory,
    pub icon: AppIcon,
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
    pub is_winget: bool,
    pub winget_id: Option<&'static str>,
    pub run_unelevated: bool,
    pub silent_args: &'static str,
    pub max_bytes: u64,
    pub uninstall_display_prefix: &'static str,
}

static CURATED_CATALOG: &[CuratedPackage] = &[
    // --- GAMING & LAUNCHERS ---
    CuratedPackage {
        id: "steam",
        name: "Steam",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 15 * 1024 * 1024,
        uninstall_display_prefix: "Steam",
    },
    CuratedPackage {
        id: "epic_games",
        name: "Epic Games Launcher",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/qn /norestart",
        max_bytes: 150 * 1024 * 1024,
        uninstall_display_prefix: "Epic Games Launcher",
    },
    CuratedPackage {
        id: "gog_galaxy",
        name: "GOG Galaxy",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
        copyright_holder: "GOG sp. z o.o.",
        license: "Propietario",
        official_site: "https://www.gog.com/galaxy",
        description_es: "Cliente oficial sin DRM de GOG",
        description_en: "Official DRM-free GOG gaming client",
        version: "Evergreen",
        url: "https://cdn.gog.com/open/galaxy/client/setup_galaxy_2.0.75.1.exe",
        allowed_hosts: &["cdn.gog.com", "gog.com"],
        expected_sha256: None,
        signer_cn: Some("GOG sp. z o.o.|GOG Sp. z o.o."),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/VERYSILENT /NORESTART",
        max_bytes: 280 * 1024 * 1024,
        uninstall_display_prefix: "GOG GALAXY",
    },
    CuratedPackage {
        id: "ea_app",
        name: "EA App",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
        copyright_holder: "Electronic Arts Inc.",
        license: "Propietario",
        official_site: "https://www.ea.com/ea-app",
        description_es: "Plataforma oficial de juegos de Electronic Arts",
        description_en: "Official Electronic Arts gaming client",
        version: "Evergreen",
        url: "https://origin-a.akamaihd.net/EA-Desktop-Client-Download/installer-releases/EAappInstaller.exe",
        allowed_hosts: &["origin-a.akamaihd.net", "ea.com"],
        expected_sha256: None,
        signer_cn: Some("Electronic Arts, Inc.|Electronic Arts Inc."),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/quiet",
        max_bytes: 40 * 1024 * 1024,
        uninstall_display_prefix: "EA app",
    },
    CuratedPackage {
        id: "ubisoft_connect",
        name: "Ubisoft Connect",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 320 * 1024 * 1024,
        uninstall_display_prefix: "Ubisoft Connect",
    },
    CuratedPackage {
        id: "battlenet",
        name: "Battle.net",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "--lang=esES --installpath=\"C:\\Program Files (x86)\\Battle.net\"",
        max_bytes: 20 * 1024 * 1024,
        uninstall_display_prefix: "Battle.net",
    },
    CuratedPackage {
        id: "playnite",
        name: "Playnite",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/SP- /VERYSILENT /SUPPRESSMSGBOXES /NORESTART",
        max_bytes: 180 * 1024 * 1024,
        uninstall_display_prefix: "Playnite",
    },
    CuratedPackage {
        id: "moonlight",
        name: "Moonlight Game Streaming",
        category: AppCategory::Gaming,
        icon: AppIcon::Gamepad,
        copyright_holder: "Moonlight Stream Team",
        license: "GPL-3.0",
        official_site: "https://moonlight-stream.org/",
        description_es: "Streaming de juegos de ultra baja latencia",
        description_en: "Open-source low-latency game streaming client",
        version: "6.1.0",
        url: "https://github.com/moonlight-stream/moonlight-qt/releases/download/v6.1.0/MoonlightSetup-6.1.0.exe",
        allowed_hosts: &["github.com", "objects.githubusercontent.com"],
        expected_sha256: None,
        signer_cn: Some("Cameron Gutman"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 60 * 1024 * 1024,
        uninstall_display_prefix: "Moonlight",
    },

    // --- COMMUNICATION & MEDIA ---
    CuratedPackage {
        id: "discord",
        name: "Discord",
        category: AppCategory::Communication,
        icon: AppIcon::Chat,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: true,
        silent_args: "-s",
        max_bytes: 160 * 1024 * 1024,
        uninstall_display_prefix: "Discord",
    },
    CuratedPackage {
        id: "telegram",
        name: "Telegram Desktop",
        category: AppCategory::Communication,
        icon: AppIcon::Chat,
        copyright_holder: "Telegram FZ-LLC",
        license: "GPL-3.0",
        official_site: "https://desktop.telegram.org/",
        description_es: "Mensajería rápida, segura y ligera",
        description_en: "Fast and secure desktop messaging app",
        version: "Evergreen",
        url: "https://telegram.org/dl/desktop/win64",
        allowed_hosts: &["telegram.org", "updates.tdesktop.com"],
        expected_sha256: None,
        signer_cn: Some("Telegram FZ-LLC"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/VERYSILENT /NORESTART",
        max_bytes: 55 * 1024 * 1024,
        uninstall_display_prefix: "Telegram Desktop",
    },
    CuratedPackage {
        id: "slack",
        name: "Slack",
        category: AppCategory::Communication,
        icon: AppIcon::Chat,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/qn /norestart",
        max_bytes: 200 * 1024 * 1024,
        uninstall_display_prefix: "Slack",
    },
    CuratedPackage {
        id: "obs",
        name: "OBS Studio",
        category: AppCategory::Communication,
        icon: AppIcon::Video,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 230 * 1024 * 1024,
        uninstall_display_prefix: "OBS Studio",
    },
    CuratedPackage {
        id: "vlc",
        name: "VLC Media Player",
        category: AppCategory::Communication,
        icon: AppIcon::Video,
        copyright_holder: "VideoLAN Association",
        license: "GPL-2.0+",
        official_site: "https://www.videolan.org/vlc/",
        description_es: "Reproductor universal de audio y vídeo",
        description_en: "Universal media player for all audio & video formats",
        version: "3.0.21",
        url: "https://get.videolan.org/vlc/3.0.21/win64/vlc-3.0.21-win64.exe",
        allowed_hosts: &["get.videolan.org", "videolan.org"],
        expected_sha256: None,
        signer_cn: Some("VideoLAN"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 50 * 1024 * 1024,
        uninstall_display_prefix: "VLC media player",
    },
    CuratedPackage {
        id: "parsec",
        name: "Parsec",
        category: AppCategory::Communication,
        icon: AppIcon::Video,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/silent /norun /nocleanuser /allusers",
        max_bytes: 50 * 1024 * 1024,
        uninstall_display_prefix: "Parsec",
    },
    CuratedPackage {
        id: "spotify",
        name: "Spotify",
        category: AppCategory::Communication,
        icon: AppIcon::Video,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: true,
        silent_args: "/Silent",
        max_bytes: 200 * 1024 * 1024,
        uninstall_display_prefix: "Spotify",
    },

    // --- BROWSERS ---
    CuratedPackage {
        id: "firefox",
        name: "Mozilla Firefox",
        category: AppCategory::Browsers,
        icon: AppIcon::Browser,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "-ms /TaskbarShortcut=false",
        max_bytes: 120 * 1024 * 1024,
        uninstall_display_prefix: "Mozilla Firefox",
    },
    CuratedPackage {
        id: "chrome",
        name: "Google Chrome",
        category: AppCategory::Browsers,
        icon: AppIcon::Browser,
        copyright_holder: "Google LLC",
        license: "Propietario",
        official_site: "https://www.google.com/chrome/",
        description_es: "Navegador rápido y oficial de Google",
        description_en: "Fast, secure web browser built by Google",
        version: "Evergreen",
        url: "https://dl.google.com/chrome/install/GoogleChromeStandaloneEnterprise64.msi",
        allowed_hosts: &["dl.google.com", "google.com"],
        expected_sha256: None,
        signer_cn: Some("Google LLC"),
        cert_thumbprint_sha1: None,
        is_msi: true,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/qn /norestart",
        max_bytes: 130 * 1024 * 1024,
        uninstall_display_prefix: "Google Chrome",
    },
    CuratedPackage {
        id: "brave",
        name: "Brave Browser",
        category: AppCategory::Browsers,
        icon: AppIcon::Browser,
        copyright_holder: "Brave Software, Inc.",
        license: "MPL-2.0",
        official_site: "https://brave.com/",
        description_es: "Navegador rápido con bloqueo nativo de anuncios y rastreadores",
        description_en: "Privacy browser with built-in ad and tracker blocking",
        version: "Evergreen",
        url: "https://laptop-updates.brave.com/latest/winx64",
        allowed_hosts: &["laptop-updates.brave.com", "brave.com", "brave-browser-downloads.s3.brave.com"],
        expected_sha256: None,
        signer_cn: Some("Brave Software, Inc.|Brave Software Inc."),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/silent /install",
        max_bytes: 140 * 1024 * 1024,
        uninstall_display_prefix: "Brave",
    },
    CuratedPackage {
        id: "librewolf",
        name: "LibreWolf",
        category: AppCategory::Browsers,
        icon: AppIcon::Browser,
        copyright_holder: "LibreWolf Community",
        license: "MPL-2.0",
        official_site: "https://librewolf.net/",
        description_es: "Versión independiente de Firefox endurecida contra telemetría",
        description_en: "Custom independent version of Firefox focused on privacy",
        version: "135.0",
        url: "https://gitlab.com/api/v4/projects/24386000/packages/generic/librewolf/135.0-1/librewolf-135.0-1-windows-x86_64-setup.exe",
        allowed_hosts: &["gitlab.com"],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 110 * 1024 * 1024,
        uninstall_display_prefix: "LibreWolf",
    },

    // --- UTILITIES & SYSTEM TOOLS ---
    CuratedPackage {
        id: "7zip",
        name: "7-Zip",
        category: AppCategory::Utilities,
        icon: AppIcon::Archive,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 5 * 1024 * 1024,
        uninstall_display_prefix: "7-Zip",
    },
    CuratedPackage {
        id: "qbittorrent",
        name: "qBittorrent",
        category: AppCategory::Utilities,
        icon: AppIcon::Download,
        copyright_holder: "The qBittorrent Project",
        license: "GNU GPLv2",
        official_site: "https://www.qbittorrent.org/",
        description_es: "Cliente BitTorrent libre sin anuncios",
        description_en: "Ad-free open-source BitTorrent client",
        version: "5.1.4",
        url: "https://downloads.sourceforge.net/project/qbittorrent/qbittorrent-win32/qbittorrent-5.1.4/qbittorrent_5.1.4_x64_setup.exe",
        allowed_hosts: &["downloads.sourceforge.net", "*.dl.sourceforge.net"],
        expected_sha256: Some("b279e16d682299f218edf48ddf75f7c5802263b7d6a74132d9e31fd5a255aa9d"),
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 80 * 1024 * 1024,
        uninstall_display_prefix: "qBittorrent",
    },
    CuratedPackage {
        id: "everything",
        name: "Everything (Voidtools)",
        category: AppCategory::Utilities,
        icon: AppIcon::Search,
        copyright_holder: "David Carpenter",
        license: "Freeware / MIT",
        official_site: "https://www.voidtools.com/",
        description_es: "Motor de búsqueda de archivos instantáneo para Windows",
        description_en: "Locate files and folders by name instantly",
        version: "1.4.1.1026",
        url: "https://www.voidtools.com/Everything-1.4.1.1026.x64-Setup.exe",
        allowed_hosts: &["www.voidtools.com", "voidtools.com"],
        expected_sha256: None,
        signer_cn: Some("David Carpenter"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 6 * 1024 * 1024,
        uninstall_display_prefix: "Everything",
    },
    CuratedPackage {
        id: "sharex",
        name: "ShareX",
        category: AppCategory::Utilities,
        icon: AppIcon::Scissors,
        copyright_holder: "ShareX Team",
        license: "GPL-3.0",
        official_site: "https://getsharex.com/",
        description_es: "Captura de pantalla, grabación de GIF y utilidades de productividad",
        description_en: "Screen capture, file sharing and productivity tool",
        version: "17.0.0",
        url: "https://github.com/ShareX/ShareX/releases/download/v17.0.0/ShareX-17.0.0-setup.exe",
        allowed_hosts: &["github.com", "objects.githubusercontent.com"],
        expected_sha256: None,
        signer_cn: Some("ShareX Team|Jaex"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/VERYSILENT /NORESTART",
        max_bytes: 45 * 1024 * 1024,
        uninstall_display_prefix: "ShareX",
    },
    CuratedPackage {
        id: "powertoys",
        name: "Microsoft PowerToys",
        category: AppCategory::Utilities,
        icon: AppIcon::Wrench,
        copyright_holder: "Microsoft Corporation",
        license: "MIT",
        official_site: "https://github.com/microsoft/PowerToys",
        description_es: "Colección oficial de utilidades de productividad para Windows",
        description_en: "System utilities for Windows power users",
        version: "0.89.0",
        url: "https://github.com/microsoft/PowerToys/releases/download/v0.89.0/PowerToysSetup-0.89.0-x64.exe",
        allowed_hosts: &["github.com", "objects.githubusercontent.com"],
        expected_sha256: None,
        signer_cn: Some("Microsoft Corporation"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/install /quiet /norestart",
        max_bytes: 350 * 1024 * 1024,
        uninstall_display_prefix: "PowerToys",
    },
    CuratedPackage {
        id: "rufus",
        name: "Rufus",
        category: AppCategory::Utilities,
        icon: AppIcon::Disk,
        copyright_holder: "Pete Batard",
        license: "GPL-3.0",
        official_site: "https://rufus.ie/",
        description_es: "Creación de unidades USB de arranque e instalación de Windows",
        description_en: "Create bootable USB flash drives reliably",
        version: "4.6",
        url: "https://github.com/pbatard/rufus/releases/download/v4.6/rufus-4.6.exe",
        allowed_hosts: &["github.com", "objects.githubusercontent.com"],
        expected_sha256: None,
        signer_cn: Some("Akeo Consulting"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "",
        max_bytes: 6 * 1024 * 1024,
        uninstall_display_prefix: "Rufus",
    },
    CuratedPackage {
        id: "treesize_free",
        name: "TreeSize Free",
        category: AppCategory::Utilities,
        icon: AppIcon::Disk,
        copyright_holder: "JAM Software GmbH",
        license: "Freeware",
        official_site: "https://www.jam-software.com/treesize_free",
        description_es: "Analizador visual de espacio en disco rápido y eficiente",
        description_en: "Powerful hard disk space manager and visualizer",
        version: "Evergreen",
        url: "https://downloads.jam-software.de/treesize_free/TreeSizeFreeSetup.exe",
        allowed_hosts: &["downloads.jam-software.de", "jam-software.de"],
        expected_sha256: None,
        signer_cn: Some("JAM Software GmbH"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/VERYSILENT /SUPPRESSMSGBOXES",
        max_bytes: 30 * 1024 * 1024,
        uninstall_display_prefix: "TreeSize Free",
    },
    CuratedPackage {
        id: "bleachbit",
        name: "BleachBit",
        category: AppCategory::Utilities,
        icon: AppIcon::Wrench,
        copyright_holder: "Andrew Ziem",
        license: "GPL-3.0",
        official_site: "https://www.bleachbit.org/",
        description_es: "Limpiador de privacidad y espacio de almacenamiento de código abierto",
        description_en: "Open-source system cleaner and privacy manager",
        version: "4.6.2",
        url: "https://download.bleachbit.org/BleachBit-4.6.2-setup.exe",
        allowed_hosts: &["download.bleachbit.org", "bleachbit.org"],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 25 * 1024 * 1024,
        uninstall_display_prefix: "BleachBit",
    },

    // --- DEVELOPMENT & EDITORS ---
    CuratedPackage {
        id: "vscode",
        name: "Visual Studio Code",
        category: AppCategory::Development,
        icon: AppIcon::Code,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://code.visualstudio.com/",
        description_es: "Editor de código ligero y extensible con ecosistema de plugins",
        description_en: "Code editing redefined, free and extensible",
        version: "Evergreen",
        url: "https://update.code.visualstudio.com/latest/win32-x64-user/stable",
        allowed_hosts: &["update.code.visualstudio.com", "az764295.vo.msecnd.net", "vscode.download.prss.microsoft.com"],
        expected_sha256: None,
        signer_cn: Some("Microsoft Corporation"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: true,
        silent_args: "/VERYSILENT /NORESTART /MERGETASKS=!runcode,addcontextmenufiles,addcontextmenufolders",
        max_bytes: 120 * 1024 * 1024,
        uninstall_display_prefix: "Microsoft Visual Studio Code",
    },
    CuratedPackage {
        id: "vscodium",
        name: "VSCodium",
        category: AppCategory::Development,
        icon: AppIcon::Code,
        copyright_holder: "VSCodium Community",
        license: "MIT",
        official_site: "https://vscodium.com/",
        description_es: "Distribución binaria de VS Code 100% libre sin telemetría de Microsoft",
        description_en: "Community-driven, freely-licensed binary distribution of VS Code without telemetry",
        version: "1.97.2.25045",
        url: "https://github.com/VSCodium/vscodium/releases/download/1.97.2.25045/VSCodiumSetup-x64-1.97.2.25045.exe",
        allowed_hosts: &["github.com", "objects.githubusercontent.com"],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/VERYSILENT /NORESTART",
        max_bytes: 110 * 1024 * 1024,
        uninstall_display_prefix: "VSCodium",
    },
    CuratedPackage {
        id: "sublime_text",
        name: "Sublime Text 4",
        category: AppCategory::Development,
        icon: AppIcon::Code,
        copyright_holder: "Sublime HQ Pty Ltd",
        license: "Shareware",
        official_site: "https://www.sublimetext.com/",
        description_es: "Editor de texto ultrarrápido con aceleración por GPU",
        description_en: "Sophisticated text editor for code, markup and prose",
        version: "Build 4180",
        url: "https://download.sublimetext.com/sublime_text_build_4180_x64_setup.exe",
        allowed_hosts: &["download.sublimetext.com", "sublimetext.com"],
        expected_sha256: None,
        signer_cn: Some("Sublime HQ Pty Ltd"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/VERYSILENT /NORESTART",
        max_bytes: 25 * 1024 * 1024,
        uninstall_display_prefix: "Sublime Text",
    },
    CuratedPackage {
        id: "git",
        name: "Git for Windows",
        category: AppCategory::Development,
        icon: AppIcon::Terminal,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/VERYSILENT /NORESTART",
        max_bytes: 90 * 1024 * 1024,
        uninstall_display_prefix: "Git",
    },
    CuratedPackage {
        id: "notepadplusplus",
        name: "Notepad++",
        category: AppCategory::Development,
        icon: AppIcon::Code,
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
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/S",
        max_bytes: 15 * 1024 * 1024,
        uninstall_display_prefix: "Notepad++",
    },
    CuratedPackage {
        id: "nodejs_lts",
        name: "Node.js (LTS)",
        category: AppCategory::Development,
        icon: AppIcon::Terminal,
        copyright_holder: "OpenJS Foundation",
        license: "MIT",
        official_site: "https://nodejs.org/",
        description_es: "Entorno de ejecución de JavaScript para backend y herramientas",
        description_en: "JavaScript runtime built on Chrome's V8 engine",
        version: "22.14.0",
        url: "https://nodejs.org/dist/v22.14.0/node-v22.14.0-x64.msi",
        allowed_hosts: &["nodejs.org"],
        expected_sha256: None,
        signer_cn: Some("OpenJS Foundation"),
        cert_thumbprint_sha1: None,
        is_msi: true,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/qn /norestart",
        max_bytes: 40 * 1024 * 1024,
        uninstall_display_prefix: "Node.js",
    },
    CuratedPackage {
        id: "python",
        name: "Python 3.12",
        category: AppCategory::Development,
        icon: AppIcon::Terminal,
        copyright_holder: "Python Software Foundation",
        license: "PSF",
        official_site: "https://www.python.org/",
        description_es: "Lenguaje de programación versátil con instalador oficial de 64 bits",
        description_en: "Official 64-bit Python interpreter and standard library",
        version: "3.12.8",
        url: "https://www.python.org/ftp/python/3.12.8/python-3.12.8-amd64.exe",
        allowed_hosts: &["www.python.org", "python.org"],
        expected_sha256: None,
        signer_cn: Some("Python Software Foundation"),
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: false,
        winget_id: None,
        run_unelevated: false,
        silent_args: "/quiet InstallAllUsers=1 PrependPath=1",
        max_bytes: 35 * 1024 * 1024,
        uninstall_display_prefix: "Python 3.12",
    },

    // --- UWP RESTORATION (WINGET MSSTORE) ---
    CuratedPackage {
        id: "uwp_weather",
        name: "El Tiempo (MSN Weather)",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Store,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://www.msn.com/weather",
        description_es: "Reinstalar la app oficial de Clima desde Microsoft Store",
        description_en: "Reinstall official MSN Weather from Microsoft Store",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9WZDNCRFJ3Q2"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "MSN Weather",
    },
    CuratedPackage {
        id: "uwp_camera",
        name: "Cámara de Windows",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Camera,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Reinstalar la app oficial de Cámara de Windows",
        description_en: "Reinstall official Windows Camera app",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9WZDNCRFJBBG"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Windows Camera",
    },
    CuratedPackage {
        id: "uwp_clock",
        name: "Reloj y Alarmas",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Clock,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Temporizadores, alarmas y reloj de Windows",
        description_en: "Official Windows Clock & Alarms app",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9WZDNCRFJ3PR"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Windows Clock",
    },
    CuratedPackage {
        id: "uwp_calculator",
        name: "Calculadora de Windows",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Calculator,
        copyright_holder: "Microsoft Corporation",
        license: "MIT",
        official_site: "https://apps.microsoft.com/",
        description_es: "Calculadora estándar, científica y para programador",
        description_en: "Standard, scientific and programmer calculator",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9WZDNCRFJ364"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Calculator",
    },
    CuratedPackage {
        id: "uwp_recorder",
        name: "Grabadora de Voz",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Microphone,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Grabadora oficial de audio de Windows",
        description_en: "Official Windows Voice/Sound Recorder",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9WZDNCRFHWKN"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Voice Recorder",
    },
    CuratedPackage {
        id: "uwp_phonelink",
        name: "Enlace Móvil (Phone Link)",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Phone,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Sincronización de llamadas, SMS y notificaciones con el smartphone",
        description_en: "Sync notifications, messages and calls with your phone",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9NBLGGH4S547"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Phone Link",
    },
    CuratedPackage {
        id: "uwp_todos",
        name: "Microsoft To Do",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Checklist,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://todo.microsoft.com/",
        description_es: "Gestor de tareas y recordatorios sincronizado",
        description_en: "Cloud task manager and daily planner",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9NBLGGH5R558"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Microsoft To Do",
    },
    CuratedPackage {
        id: "uwp_xbox",
        name: "App de Xbox (PC Game Pass)",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Gamepad,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://www.xbox.com/apps/xbox-app-for-pc",
        description_es: "Acceso y gestión de juegos de PC Game Pass y comunidad Xbox",
        description_en: "Discover and download games with PC Game Pass",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9MV0B5HZVK9Z"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Xbox",
    },
    CuratedPackage {
        id: "uwp_gamebar",
        name: "Xbox Game Bar (Win+G)",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Gamepad,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Superposición de juegos con captura, audio y monitor de rendimiento",
        description_en: "Gaming overlay for screen capture and performance widgets",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9NZKPSTSNW4P"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Xbox Game Bar",
    },
    CuratedPackage {
        id: "uwp_clipchamp",
        name: "Microsoft Clipchamp",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Video,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://clipchamp.com/",
        description_es: "Editor de vídeo moderno integrado en Windows 11",
        description_en: "Quick video editor with templates and transitions",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9P1J8S7CCWWT"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Clipchamp",
    },
    CuratedPackage {
        id: "uwp_terminal",
        name: "Windows Terminal",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Terminal,
        copyright_holder: "Microsoft Corporation",
        license: "MIT",
        official_site: "https://github.com/microsoft/terminal",
        description_es: "Consola moderna con pestañas para PowerShell, CMD y WSL",
        description_en: "Modern tabbed terminal for command-line tools",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9N0DX20HK701"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Windows Terminal",
    },
    CuratedPackage {
        id: "uwp_photos",
        name: "Fotos de Windows",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Camera,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Visor y organizador oficial de fotos con edición básica",
        description_en: "Official Windows Photos viewer and editor",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9NBLGGH42THS"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Microsoft Photos",
    },
    CuratedPackage {
        id: "uwp_snipping",
        name: "Herramienta Recortes",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Scissors,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Captura de pantalla rápida y grabación de pantalla (Win+Shift+S)",
        description_en: "Fast screen capture and recording utility",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9MZ95KL8MR0L"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Snipping Tool",
    },
    CuratedPackage {
        id: "uwp_paint",
        name: "Paint de Windows",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Paint,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Clásico editor gráfico modernizado con soporte de capas y PNG",
        description_en: "Modernized classic Paint editor with transparency and layers",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9PCFS5B6T72H"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Paint",
    },
    CuratedPackage {
        id: "uwp_notepad",
        name: "Bloc de Notas Store",
        category: AppCategory::UwpRestoration,
        icon: AppIcon::Code,
        copyright_holder: "Microsoft Corporation",
        license: "Propietario",
        official_site: "https://apps.microsoft.com/",
        description_es: "Bloc de notas oficial con pestañas y guardado automático",
        description_en: "Modern tabbed Windows Notepad with auto-save",
        version: "Store",
        url: "",
        allowed_hosts: &[],
        expected_sha256: None,
        signer_cn: None,
        cert_thumbprint_sha1: None,
        is_msi: false,
        is_winget: true,
        winget_id: Some("9MSMLRH6LZF3"),
        run_unelevated: false,
        silent_args: "",
        max_bytes: 0,
        uninstall_display_prefix: "Windows Notepad",
    },
];

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

fn resolve_winget_binary() -> CoreResult<PathBuf> {
    if let Some(local) = crate::core::win_common::local_app_data_dir() {
        let candidate = local.join(r"Microsoft\WindowsApps\winget.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    if let Ok(p) = system32_dir() {
        let candidate = p.join("winget.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Ok(PathBuf::from("winget.exe"))
}

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

    pub fn find_newer_curated_version(
        display_name: &str,
        current_version: &str,
    ) -> Option<&'static CuratedPackage> {
        let cur_pkg = CURATED_CATALOG
            .iter()
            .find(|p| display_name_matches(display_name, p.uninstall_display_prefix))?;

        if cur_pkg.version == "Evergreen" || cur_pkg.is_winget {
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

        if pkg.is_winget {
            let wid = pkg.winget_id.ok_or_else(|| {
                CoreError::Validation("Paquete UWP sin identificador Winget/Store".into())
            })?;
            let winget_exe = resolve_winget_binary()?;
            let cmd = format!(
                "install --id {} --source msstore --accept-package-agreements --accept-source-agreements --disable-interactivity --silent",
                wid
            );
            let user_token = duplicate_desktop_shell_primary_token().ok();

            let exit = run_in_job(
                &Launch {
                    application: &winget_exe,
                    command_line: format!("\"{}\" {}", winget_exe.display(), cmd),
                    working_dir: winget_exe.parent().unwrap_or(Path::new(r"C:\Windows\System32")),
                    user_token: user_token.as_ref(),
                },
                Duration::from_secs(420),
                Some(cancel),
            )?;

            if !exit.is_success() && exit.exit_code != 0x8A15002B && exit.exit_code != 0x8A150014 {
                return Err(CoreError::SystemTool {
                    message: format!(
                        "Winget falló reinstalando '{}' (código {})",
                        pkg.name,
                        exit.code_display()
                    ),
                    source: None,
                });
            }
            return Ok(());
        }

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

        if !exit.is_success() && exit.exit_code != 1638 {
            return Err(CoreError::SystemTool {
                message: format!(
                    "Instalador de '{}' terminó con código {}",
                    pkg.name,
                    exit.code_display()
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
        assert!(
            AppStoreManager::find_newer_curated_version("7-Zip 99.00 (x64)", "99.00").is_none()
        );
    }

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