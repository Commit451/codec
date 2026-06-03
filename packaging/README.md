# Packaging & installers

These build end-user installers that drop the bundled plugin into the system
plugin folders. Build the bundle first (from the repo root):

```bash
cargo xtask bundle codec --release
```

## macOS — `.pkg`

```bash
./packaging/macos/build-pkg.sh              # → dist/codec-<version>.pkg
./packaging/macos/build-pkg.sh --universal  # fat arm64 + x86_64 binary
./packaging/macos/build-pkg.sh --skip-build # reuse an existing target/bundled/*
```

Installs:

- `Codec.vst3` → `/Library/Audio/Plug-Ins/VST3/`
- `Codec.clap` → `/Library/Audio/Plug-Ins/CLAP/`

Tools: `pkgbuild` / `productbuild`, included with the Xcode Command Line Tools
(`xcode-select --install`).

### Signing & notarization (for public distribution)

Unsigned packages run fine locally but Gatekeeper will warn other users. To sign
and notarize you need an Apple Developer account:

```bash
# One-time: store notarization credentials in the keychain
xcrun notarytool store-credentials "codec" \
    --apple-id "you@example.com" --team-id "TEAMID" --password "app-specific-password"

# Build, sign the installer, and notarize + staple
INSTALLER_SIGN_ID="Developer ID Installer: Your Name (TEAMID)" \
NOTARY_PROFILE="codec" \
    ./packaging/macos/build-pkg.sh
```

The plugin binary itself is ad-hoc signed by `cargo xtask bundle`; for full
distribution you'd also code-sign it with a "Developer ID Application" cert
before packaging (`codesign --deep --options runtime`).

## Windows — Inno Setup

Build the bundle **on Windows** (cross-compiling the GUI from macOS/Linux is
impractical), then compile the installer with [Inno Setup](https://jrsoftware.org/isinfo.php):

```bat
cargo xtask bundle codec --release
iscc packaging\windows\installer.iss
```

Produces `dist\codec-<version>-setup.exe`, which installs:

- `Codec.vst3` → `C:\Program Files\Common Files\VST3\`
- `Codec.clap` → `C:\Program Files\Common Files\CLAP\`

Before shipping, generate a stable `AppId` GUID (Inno IDE → *Tools → Generate GUID*)
and bump `ProductVersion` in `installer.iss`. Optional Authenticode signing is done
with `signtool` (configure `SignTool` in Inno Setup).

## Keeping versions in sync

The macOS script reads the version from `plugin/Cargo.toml`. For Windows, update
`ProductVersion` in `installer.iss` to match. Both install to the standard plugin
folders, so installing a new version overwrites the old bundle in place.
