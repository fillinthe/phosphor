; Phosphor for Windows: a Setup.exe made with Inno Setup (preinstalled on GitHub's Windows machines).
; Built by .github/workflows/windows.yml from the packaged folder:
;   ISCC /DAppVersion=0.2.95 /DSourceDir=dist\Phosphor /Odist scripts\windows\phosphor.iss
; Installs for the current user (no admin prompt) into their own Programs folder, where Phosphor's updater can
; replace its files. Start menu shortcut, optional desktop shortcut, .phosphor projects open in Phosphor, uninstaller.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\Phosphor"
#endif

[Setup]
AppId={{6A0C2E4B-5D1F-4E8A-9C3B-7F2D1E0A9B54}
AppName=Phosphor VJ
AppVersion={#AppVersion}
AppPublisher=Phosphor
AppPublisherURL=https://phosphorfx.com
AppSupportURL=https://github.com/fillinthe/phosphor/issues
DefaultDirName={localappdata}\Programs\Phosphor
DefaultGroupName=Phosphor VJ
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Phosphor-windows-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\Phosphor.exe
CloseApplications=yes
ChangesAssociations=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

; Shortcuts from before the name became Phosphor VJ (an update over an older install).
[InstallDelete]
Type: files; Name: "{userdesktop}\Phosphor.lnk"
Type: filesandordirs; Name: "{userprograms}\Phosphor"

[Icons]
Name: "{group}\Phosphor VJ"; Filename: "{app}\Phosphor.exe"
Name: "{group}\Uninstall Phosphor VJ"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Phosphor VJ"; Filename: "{app}\Phosphor.exe"; Tasks: desktopicon

[Registry]
; .phosphor projects open in Phosphor (per user).
Root: HKCU; Subkey: "Software\Classes\.phosphor"; ValueType: string; ValueData: "Phosphor.Project"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\Phosphor.Project"; ValueType: string; ValueData: "Phosphor project"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Phosphor.Project\DefaultIcon"; ValueType: string; ValueData: "{app}\Phosphor.exe,0"
Root: HKCU; Subkey: "Software\Classes\Phosphor.Project\shell\open\command"; ValueType: string; ValueData: """{app}\Phosphor.exe"" ""%1"""

[Run]
Filename: "{app}\Phosphor.exe"; Description: "Open Phosphor now"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Files the updater leaves behind ("<name>.old" while the old ones were in use).
Type: files; Name: "{app}\*.old"
