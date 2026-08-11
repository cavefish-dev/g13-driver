; g13-driver per-user installer. Compile with:
;   ISCC.exe /DAppVersion=<version> packaging\g13-driver.iss
#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif

[Setup]
AppId={{7C4B2E90-4E1A-4C2E-9E7A-9F1D3B0A6C13}
AppName=g13-driver
AppVersion={#AppVersion}
AppVerName=g13-driver {#AppVersion}
AppPublisher=cavefish-dev
DefaultDirName={localappdata}\Programs\g13-driver
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..
OutputBaseFilename=g13-driver-v{#AppVersion}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
#if FileExists(AddBackslash(SourcePath) + "g13.ico")
SetupIconFile=g13.ico
#endif

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\target\release\g13-driver.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\config.toml"; DestDir: "{app}"; Flags: onlyifdoesntexist
Source: "..\profiles\*"; DestDir: "{app}\profiles"; Flags: onlyifdoesntexist recursesubdirs createallsubdirs
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\g13-driver"; Filename: "{app}\g13-driver.exe"
Name: "{autodesktop}\g13-driver"; Filename: "{app}\g13-driver.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\g13-driver.exe"; Description: "Launch g13-driver"; Flags: nowait postinstall skipifsilent
