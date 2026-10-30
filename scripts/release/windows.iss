[Setup]
AppId={{A179BDDB-4AA7-4D7E-B67A-638D75CD8C43}
AppName=Saytwig
AppVersion={#AppVersion}
AppPublisher=Artisians
AppPublisherURL=https://github.com/Artisians/saytwig
DefaultDirName={localappdata}\Programs\Saytwig
DefaultGroupName=Saytwig
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\saytwig.exe
CloseApplications=yes
RestartApplications=no

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Saytwig"; Filename: "{app}\saytwig.exe"

[Run]
Filename: "{app}\saytwig.exe"; Description: "Open Saytwig"; Flags: nowait postinstall skipifsilent
