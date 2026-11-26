use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use winreg::RegKey;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::elevation;
use crate::core::platform::process::{
    Launch, duplicate_desktop_shell_primary_token, run_in_job, system32_dir,
};
use crate::core::store::winget::{
    WingetScope, machine_winget, repository_install_args, store_install_args, user_winget,
    winget_exit_is_installed,
};

/// winget descarga e instala: un launcher o un IDE grande puede tardar con una conexión lenta. Pasado el plazo se corta
/// el Job entero (winget y el instalador).
const WINGET_TIMEOUT: Duration = Duration::from_secs(30 * 60);

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
        source: InstallSource::Winget {
            id: "Valve.Steam",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "EpicGames.EpicGamesLauncher",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "GOG.Galaxy",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "ElectronicArts.EADesktop",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Ubisoft.Connect",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Blizzard.BattleNet",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Playnite.Playnite",
            id_es: None,
            scope: WingetScope::User,
        },
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
        source: InstallSource::Winget {
            id: "MoonlightGameStreamingProject.Moonlight",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Discord.Discord",
            id_es: None,
            scope: WingetScope::User,
        },
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
        source: InstallSource::Winget {
            id: "Telegram.TelegramDesktop",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "SlackTechnologies.Slack",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "OBSProject.OBSStudio",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "VideoLAN.VLC",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Parsec.Parsec",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Spotify.Spotify",
            id_es: None,
            scope: WingetScope::User,
        },
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
        source: InstallSource::Winget {
            id: "Mozilla.Firefox",
            id_es: Some("Mozilla.Firefox.es-ES"),
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Google.Chrome",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Brave.Brave",
            id_es: None,
            scope: WingetScope::Machine,
        },
        uninstall_display_prefix: "Brave",
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
        source: InstallSource::Winget {
            id: "7zip.7zip",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "qBittorrent.qBittorrent",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "voidtools.Everything",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "ShareX.ShareX",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Microsoft.PowerToys",
            id_es: None,
            scope: WingetScope::Machine,
        },
        uninstall_display_prefix: "PowerToys",
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
        source: InstallSource::Winget {
            id: "JAMSoftware.TreeSize.Free",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "BleachBit.BleachBit",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Microsoft.VisualStudioCode",
            id_es: None,
            scope: WingetScope::User,
        },
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
        source: InstallSource::Winget {
            id: "VSCodium.VSCodium",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "SublimeHQ.SublimeText.4",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Git.Git",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Notepad++.Notepad++",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "OpenJS.NodeJS.LTS",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Winget {
            id: "Python.Python.3.12",
            id_es: None,
            scope: WingetScope::Machine,
        },
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
        source: InstallSource::Store { id: "9WZDNCRFJ3Q2" },
        uninstall_display_prefix: "Microsoft.BingWeather",
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
        source: InstallSource::Store { id: "9WZDNCRFJBBG" },
        uninstall_display_prefix: "Microsoft.WindowsCamera",
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
        source: InstallSource::Store { id: "9WZDNCRFJ3PR" },
        uninstall_display_prefix: "Microsoft.WindowsAlarms",
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
        source: InstallSource::Store { id: "9WZDNCRFHVN5" },
        uninstall_display_prefix: "Microsoft.WindowsCalculator",
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
        source: InstallSource::Store { id: "9WZDNCRFHWKN" },
        uninstall_display_prefix: "Microsoft.WindowsSoundRecorder",
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
        source: InstallSource::Store { id: "9NMPJ99VJBWV" },
        uninstall_display_prefix: "Microsoft.YourPhone",
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
        source: InstallSource::Store { id: "9NBLGGH5R558" },
        uninstall_display_prefix: "Microsoft.Todos",
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
        source: InstallSource::Store { id: "9MV0B5HZVK9Z" },
        uninstall_display_prefix: "Microsoft.GamingApp",
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
        source: InstallSource::Store { id: "9NZKPSTSNW4P" },
        uninstall_display_prefix: "Microsoft.XboxGamingOverlay",
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
        source: InstallSource::Store { id: "9P1J8S7CCWWT" },
        uninstall_display_prefix: "Clipchamp.Clipchamp",
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
        source: InstallSource::Store { id: "9N0DX20HK701" },
        uninstall_display_prefix: "Microsoft.WindowsTerminal",
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
        source: InstallSource::Store { id: "9WZDNCRFJBH4" },
        uninstall_display_prefix: "Microsoft.Windows.Photos",
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
        source: InstallSource::Store { id: "9MZ95KL8MR0L" },
        uninstall_display_prefix: "Microsoft.ScreenSketch",
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
        source: InstallSource::Store { id: "9PCFS5B6T72H" },
        uninstall_display_prefix: "Microsoft.Paint",
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
        source: InstallSource::Store { id: "9MSMLRH6LZF3" },
        uninstall_display_prefix: "Microsoft.WindowsNotepad",
    },
];

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
    /// Nombre en español; la interfaz lo traduce con `ui::i18n::core_text::tr` (UI-11, IN-02).
    pub fn label(&self) -> &'static str {
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

/// De dónde sale una app del catálogo (AR-27: un enum, así que no hay paquete «sin verificación» posible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallSource {
    /// Repositorio comunitario de winget: última versión y SHA-256 del manifiesto. `id_es`, si existe, es el paquete
    /// en español, que se pide cuando Windows está en español (IN-01).
    Winget {
        id: &'static str,
        id_es: Option<&'static str>,
        scope: WingetScope,
    },
    /// Microsoft Store, siempre como el usuario de la sesión.
    Store { id: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
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
    pub source: InstallSource,
    /// Cómo se detecta instalada: prefijo del `DisplayName` en Uninstall, o, para las apps de la Store, nombre del
    /// paquete (`Microsoft.WindowsCalculator`).
    pub uninstall_display_prefix: &'static str,
}

pub fn get_catalog() -> &'static [CuratedPackage] {
    CURATED_CATALOG
}

pub fn find_package(package_id: &str) -> Option<&'static CuratedPackage> {
    CURATED_CATALOG
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(package_id))
}

/// App del catálogo instalable por winget que corresponde a una entrada de «Programas instalados». `winget install`
/// sobre una app ya instalada la actualiza, o termina con «ya está al día».
pub fn winget_package_for(display_name: &str) -> Option<&'static CuratedPackage> {
    CURATED_CATALOG.iter().find(|p| {
        matches!(p.source, InstallSource::Winget { .. })
            && display_name_matches(display_name, p.uninstall_display_prefix)
    })
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
                if let Ok(app_key) = uninst.open_subkey_with_flags(&sub_name, flags)
                    && let Ok(dn) = app_key.get_value::<String, _>("DisplayName")
                {
                    let trimmed = dn.trim().to_string();
                    if !trimmed.is_empty() {
                        display_names.push(trimmed);
                    }
                }
            }
        }
    }

    let store_packages = installed_store_packages();
    let mut installed = HashSet::new();
    for pkg in CURATED_CATALOG {
        let found = if matches!(pkg.source, InstallSource::Store { .. }) {
            let wanted = format!("{}_", pkg.uninstall_display_prefix.to_ascii_lowercase());
            store_packages.iter().any(|full| full.starts_with(&wanted))
        } else {
            display_names
                .iter()
                .any(|name| display_name_matches(name, pkg.uninstall_display_prefix))
        };
        if found {
            installed.insert(pkg.id);
        }
    }

    if check_spotify_presence_on_disk() {
        installed.insert("spotify");
    }

    installed
}

pub fn install_package_cancellable(package_id: &str, cancel: &AtomicBool) -> CoreResult<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(CoreError::Cancelled);
    }

    elevation::require_elevated("Instalar paquetes curados de software")?;

    let pkg = find_package(package_id).ok_or_else(|| {
        CoreError::Validation(format!(
            "Paquete '{}' no encontrado en el catálogo",
            package_id
        ))
    })?;

    let (args, scope) = match pkg.source {
        InstallSource::Store { id } => (store_install_args(id), WingetScope::User),
        InstallSource::Winget { id, id_es, scope } => {
            let id = if windows_ui_is_spanish() {
                id_es.unwrap_or(id)
            } else {
                id
            };
            (repository_install_args(id, scope), scope)
        }
    };
    run_winget(&args, scope, pkg.name, cancel)
}

/// IN-01: los instaladores se descargan en el idioma de Windows, no en el de la app: es el que el usuario verá
/// en el programa instalado.
fn windows_ui_is_spanish() -> bool {
    crate::core::platform::user_settings::user_ui_language_is_spanish()
}

fn check_spotify_presence_on_disk() -> bool {
    if let Some(roaming) = dirs::config_dir()
        && roaming.join(r"Spotify\Spotify.exe").is_file()
    {
        return true;
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
    if let Some(pf) = crate::core::platform::folders::program_files_dir()
        && pf.join(r"Spotify\Spotify.exe").is_file()
    {
        return true;
    }
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Spotify")
        .is_ok()
}

/// UI-20: paquetes de la Store instalados para el usuario (nombres completos en minúsculas). Las apps de la Store no
/// aparecen en `Uninstall`; su registro está en el repositorio de AppModel del perfil.
fn installed_store_packages() -> Vec<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages",
            KEY_READ,
        )
        .map(|k| k.enum_keys().flatten().map(|n| n.to_ascii_lowercase()).collect())
        .unwrap_or_default()
}

/// ¿El nombre de una entrada de «Programas y características» corresponde al paquete del catálogo? Igual al prefijo, o el
/// prefijo seguido de espacio o guion y una versión o un paréntesis; un prefijo que ya termina en versión (`Python 3.12`)
/// acepta más versión (`.8`). Así `Python 3.1` no coincide con `Python 3.12`.
fn display_name_matches(display_name: &str, prefix: &str) -> bool {
    let dn = display_name.trim().to_ascii_lowercase();
    let pref = prefix.trim().to_ascii_lowercase();
    if dn == pref {
        return true;
    }
    let Some(rest) = dn.strip_prefix(&pref) else {
        return false;
    };
    // Prefijo que ya es una versión («Python 3.12») seguido de más versión («.8 (64-bit)»).
    if pref.ends_with(|c: char| c.is_ascii_digit())
        && rest.starts_with('.')
        && rest[1..].starts_with(|c: char| c.is_ascii_digit())
    {
        return true;
    }
    let mut chars = rest.chars();
    match chars.next() {
        Some(' ') | Some('-') => {}
        _ => return false,
    }
    let after = chars.as_str().trim_start_matches(['v', 'V']);
    after.chars().next().is_some_and(|c| c.is_ascii_digit()) || after.starts_with('(')
}

/// Corre `winget <args>` dentro de un Job con plazo y cancelación. Como administrador, el winget protegido de App
/// Installer (verificado y bloqueado mientras corre); como usuario, su alias con su token, nunca elevado (SEC-31).
fn run_winget(args: &str, scope: WingetScope, name: &str, cancel: &AtomicBool) -> CoreResult<()> {
    let working_dir = system32_dir()?;
    let user_token = match scope {
        WingetScope::User => Some(duplicate_desktop_shell_primary_token()?),
        WingetScope::Machine => None,
    };
    let (winget_exe, _lock) = match user_token.as_ref() {
        Some(token) => (user_winget(token)?, None),
        None => {
            let (exe, lock) = machine_winget()?;
            (exe, Some(lock))
        }
    };
    let exit = run_in_job(
        &Launch {
            application: &winget_exe,
            command_line: format!("\"{}\" {}", winget_exe.display(), args),
            working_dir: &working_dir,
            user_token: user_token.as_ref(),
        },
        WINGET_TIMEOUT,
        Some(cancel),
    )?;
    if !winget_exit_is_installed(exit.exit_code) {
        return Err(CoreError::system_tool_msg(format!(
            "winget no pudo instalar '{name}' (código {})",
            exit.code_display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ERR-16, ERR-17, TST-02: invariantes del catálogo que el compilador no ve.
    #[test]
    fn catalogo_verificable() {
        let mut ids = HashSet::new();
        let mut sources = HashSet::new();
        for p in CURATED_CATALOG {
            assert!(ids.insert(p.id), "id repetido: {}", p.id);
            let (id, es) = match p.source {
                InstallSource::Store { id } => (id, None),
                InstallSource::Winget { id, id_es, .. } => (id, id_es),
            };
            for wid in std::iter::once(id).chain(es) {
                assert!(
                    !wid.is_empty() && !wid.contains(char::is_whitespace),
                    "{}: ID de winget inválido",
                    p.id
                );
                assert!(sources.insert(wid), "{}: ID de winget repetido", p.id);
            }
            assert!(
                !p.uninstall_display_prefix.is_empty(),
                "{}: sin detección",
                p.id
            );
        }
    }

    #[test]
    fn nombre_del_registro_coincide_solo_con_el_paquete() {
        assert!(display_name_matches("Git", "Git"));
        assert!(display_name_matches("Git 2.47.1", "Git"));
        assert!(!display_name_matches("Git LFS version 3.5", "Git"));
        assert!(!display_name_matches("GitHub Desktop", "Git"));
        assert!(display_name_matches("7-Zip 24.09 (x64)", "7-Zip"));
        assert!(display_name_matches("Notepad++ (64-bit x64)", "Notepad++"));
        assert!(display_name_matches(
            "Python 3.12.8 (64-bit)",
            "Python 3.12"
        ));
        assert!(!display_name_matches(
            "Python 3.13.1 (64-bit)",
            "Python 3.12"
        ));
        assert!(!display_name_matches("Python 3.120", "Python 3.12"));
    }
}
