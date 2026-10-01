; The Windows installer (ADR-0093), built by Inno Setup 6:
;
;   ISCC.exe /DAppVersion=1.0.0 /DSourceDir=<staged> /DOutputDir=<out> bachelorpad-lite.iss
;
; scripts/New-Release.ps1 runs it on the directory it has just staged and
; zipped, so the installer and the zip hold the same files.
;
; **Per-user, and it never asks for administrator rights.** PrivilegesRequired
; is "lowest" and nothing may override it, so it installs to
; %LOCALAPPDATA%\Programs, writes only under HKEY_CURRENT_USER, and an
; unsigned installer (ADR-0055) costs the person running it a SmartScreen
; warning and no elevation prompt -- which is what ADR-0067 found an
; installer could not avoid, and what made "no installer" the answer then.
;
; **It never takes a file association** (ADR-0012). The one optional task
; offers the program in Open With and in Settings > Default apps, unticked;
; choosing it as the default stays the user's act, in Windows' own settings.
; The keys it writes are generated from the same code as File > Set as
; Default Editor -- registry.iss, below -- so the two cannot register
; different things.
;
; The uninstaller removes the program, its shortcuts and the keys this
; installer wrote. It does not touch the settings, recent files, recovery
; journal or note store in your profile (ADR-0070); docs/user/01-installation.md
; lists where those are.

#ifndef AppVersion
  #error Pass /DAppVersion=x.y.z -- the version the staged binary reports
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=<the staged release directory>
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
; Never change AppId: it is how Windows knows a later installer is an upgrade
; of this program rather than a second copy of it.
AppId={{1C01DB82-40AB-4DE3-9412-72F5B871C9B8}
AppName=BachelorPad+ Lite
AppVersion={#AppVersion}
AppVerName=BachelorPad+ Lite {#AppVersion}
AppPublisher=Daniel Boles
AppPublisherURL=https://bpad.prompt-forge.dev
AppSupportURL=https://github.com/dboles99/bachelorpluslite/issues
AppUpdatesURL=https://github.com/dboles99/bachelorpluslite/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\BachelorPad+ Lite
DefaultGroupName=BachelorPad+ Lite
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 and 11 (ADR-0001), and nothing older.
MinVersion=10.0
LicenseFile={#SourceDir}\LICENSE
SetupIconFile={#SourceDir}\bachelorpad.ico
UninstallDisplayIcon={app}\bachelorpad.ico
UninstallDisplayName=BachelorPad+ Lite
OutputDir={#OutputDir}
OutputBaseFilename=bachelorpad-lite-{#AppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes

[Tasks]
Name: "openwith"; Description: "Offer BachelorPad+ Lite in Open with, and in Settings > Default apps, for text and Markdown files. It does not make itself the default."; Flags: unchecked
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\BachelorPad+ Lite"; Filename: "{app}\bachelorpad.exe"; IconFilename: "{app}\bachelorpad.ico"
Name: "{autodesktop}\BachelorPad+ Lite"; Filename: "{app}\bachelorpad.exe"; IconFilename: "{app}\bachelorpad.ico"; Tasks: desktopicon

#include "registry.iss"

[Run]
Filename: "{app}\bachelorpad.exe"; Description: "Start BachelorPad+ Lite"; Flags: nowait postinstall skipifsilent
