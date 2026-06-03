; Inno Setup script for Compose VST (Windows).
; Inno Setup is free: https://jrsoftware.org/isinfo.php
;
; Build the plugin bundle ON a Windows machine first (cross-compiling baseview
; from macOS/Linux is impractical), then compile this installer:
;
;   cargo xtask bundle compose-vst-plugin --release
;   iscc packaging\windows\installer.iss        ; or open this file in the Inno Setup IDE
;
; Produces: dist\codec-<version>-setup.exe

#define ProductName "Compose VST"
#define ProductVersion "0.1.0"
#define ProductPublisher "compose-vst"
; Bundled plugin output, relative to this .iss file.
#define BundledDir "..\..\target\bundled"

[Setup]
; NOTE: generate your own GUID (Tools > Generate GUID in the Inno IDE) and keep it stable.
AppId={{8B5E2A14-7C3D-4E9F-A1B2-3C4D5E6F7A8B}
AppName={#ProductName}
AppVersion={#ProductVersion}
AppPublisher={#ProductPublisher}
; The per-file DestDirs below are absolute, so {app} is unused — keep the page hidden.
DefaultDirName={commoncf64}\VST3
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=codec-{#ProductVersion}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#ProductName}

[Files]
; The VST3 is a folder bundle on Windows too → Common Files\VST3
Source: "{#BundledDir}\Compose VST.vst3\*"; DestDir: "{commoncf64}\VST3\Compose VST.vst3"; \
    Flags: recursesubdirs createallsubdirs ignoreversion
; CLAP bundle → Common Files\CLAP (skipped if you didn't build a .clap)
Source: "{#BundledDir}\Compose VST.clap\*"; DestDir: "{commoncf64}\CLAP\Compose VST.clap"; \
    Flags: recursesubdirs createallsubdirs ignoreversion skipifsourcedoesntexist

[UninstallDelete]
Type: filesandordirs; Name: "{commoncf64}\VST3\Compose VST.vst3"
Type: filesandordirs; Name: "{commoncf64}\CLAP\Compose VST.clap"
