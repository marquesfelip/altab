; Instalador do altab (Inno Setup 6). Compilar com:
;   ISCC.exe /DAppVersion=0.1.0 installer\altab.iss
; Instala por usuário (sem UAC) em %LOCALAPPDATA%\Programs\altab.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{8E1F5A3C-2B7D-4C69-9A0E-5D4B7C1A9F20}
AppName=altab
AppVersion={#AppVersion}
AppVerName=altab {#AppVersion}
AppPublisher=marquesfelip
AppPublisherURL=https://github.com/marquesfelip/altab
AppSupportURL=https://github.com/marquesfelip/altab/issues
AppUpdatesURL=https://github.com/marquesfelip/altab/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\altab
DefaultGroupName=altab
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=altab-setup
SetupIconFile=..\assets\icon\altab.ico
UninstallDisplayIcon={app}\altab.exe
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Files]
Source: "..\target\release\altab.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\fonts\Inter-LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\altab"; Filename: "{app}\altab.exe"

[Registry]
; O próprio app grava a entrada de início automático na primeira execução (e
; respeita a opção "Iniciar com o Windows"). Aqui só garantimos a remoção ao desinstalar.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "altab"; Flags: dontcreatekey uninsdeletevalue

[Run]
; Inicia o app logo após instalar (inclusive em instalações silenciosas, como a do winget).
Filename: "{app}\altab.exe"; Description: "{cm:LaunchProgram,altab}"; Flags: nowait postinstall runasoriginaluser

[Code]
procedure StopRunningApp;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM altab.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopRunningApp;
  Result := '';
end;

function InitializeUninstall: Boolean;
begin
  StopRunningApp;
  Result := True;
end;
