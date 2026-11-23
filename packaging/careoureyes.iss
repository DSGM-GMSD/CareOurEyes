; Instalador de CareOurEyes para Windows (Inno Setup 6).
;
; Se instala solo para el usuario actual, sin pedir permisos de administrador,
; en %LOCALAPPDATA%\Programs\CareOurEyes. Lo compila release.yml:
;   ISCC packaging\careoureyes.iss /DAppVersion=2.0.0-alpha.1 /DNumericVersion=2.0.0
; Las dos variables tienen un valor de reserva para compilarlo a mano.

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef NumericVersion
  #define NumericVersion "0.0.0"
#endif

[Setup]
; El identificador no debe cambiar nunca: con él Windows reconoce las actualizaciones.
AppId={{6C1E2B7A-4F3D-4B8E-9A55-0C2F7E3D9B41}
AppName=CareOurEyes
AppVersion={#AppVersion}
AppVerName=CareOurEyes {#AppVersion}
AppPublisher=DSGM-GMSD
AppPublisherURL=https://github.com/DSGM-GMSD/CareOurEyes
AppSupportURL=https://github.com/DSGM-GMSD/CareOurEyes/issues
AppUpdatesURL=https://github.com/DSGM-GMSD/CareOurEyes/releases
VersionInfoVersion={#NumericVersion}
LicenseFile=..\LICENSE
PrivilegesRequired=lowest
DefaultDirName={autopf}\CareOurEyes
DefaultGroupName=CareOurEyes
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=CareOurEyes-{#AppVersion}-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\CareOurEyes.exe
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Si la app está abierta, se cierra antes de instalar o desinstalar
; (al cerrarse devuelve las pantallas a su color original).
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "nl"; MessagesFile: "compiler:Languages\Dutch.isl"
Name: "tr"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\CareOurEyes.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{group}\CareOurEyes"; Filename: "{app}\CareOurEyes.exe"
Name: "{autodesktop}\CareOurEyes"; Filename: "{app}\CareOurEyes.exe"; Tasks: desktopicon

[Registry]
; La app crea esta entrada si se activa «Arrancar con Windows». Al desinstalar
; se borra, para que Windows no intente abrir un programa que ya no existe.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "CareOurEyes"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\CareOurEyes.exe"; Description: "{cm:LaunchProgram,CareOurEyes}"; Flags: nowait postinstall skipifsilent
