; otterswiss.iss

[Setup]
AppId={{8D4F6A2E-9E3B-4E12-9F54-7A2C1B9E4401}
AppName=OtterSwiss
AppVersion=3.5.1
AppVerName=OtterSwiss v3.5.1
AppPublisher=OtterSwiss
VersionInfoVersion=3.5.1.0
DefaultDirName={autopf}\OtterSwiss
DefaultGroupName=OtterSwiss
DisableProgramGroupPage=yes
OutputDir=dist
OutputBaseFilename=OtterSwiss-Setup-v3.5.1
SetupIconFile=assets\icon.ico
UninstallDisplayIcon={app}\otter_swiss.exe
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
CloseApplicationsFilter=otter_swiss.exe
RestartApplications=no

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
spanish.CtxMenuTask=Integrar Archivador en el menú contextual de Windows
english.CtxMenuTask=Integrate Archiver into Windows context menu
spanish.LaunchAfterInstall=Ejecutar OtterSwiss al finalizar
english.LaunchAfterInstall=Launch OtterSwiss after installation

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "contextmenu"; Description: "{cm:CtxMenuTask}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "target\release\otter_swiss.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\OtterSwiss"; Filename: "{app}\otter_swiss.exe"
Name: "{autodesktop}\OtterSwiss"; Filename: "{app}\otter_swiss.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\otter_swiss.exe"; Parameters: "--install-context-menu"; Tasks: contextmenu; Flags: runhidden waituntilterminated
Filename: "{app}\otter_swiss.exe"; Description: "{cm:LaunchAfterInstall}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\otter_swiss.exe"; Parameters: "--rollback"; Flags: runhidden waituntilterminated; RunOnceId: "RevertAllChanges"
Filename: "{app}\otter_swiss.exe"; Parameters: "--uninstall-context-menu"; Flags: runhidden waituntilterminated; RunOnceId: "UninstallContextMenu"