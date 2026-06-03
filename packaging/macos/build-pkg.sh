#!/usr/bin/env bash
# Build a macOS .pkg installer for the Codec plugin.
# Installs the VST3 (+ CLAP) bundles system-wide into /Library/Audio/Plug-Ins,
# which is why a .pkg is the right tool — it handles the admin prompt for you.
#
# Usage:
#   ./packaging/macos/build-pkg.sh [--universal] [--skip-build]
#     --universal   build a fat arm64 + x86_64 plugin binary
#     --skip-build  reuse an existing target/bundled/* (don't re-run the bundler)
#
# Optional signing (a "Developer ID Installer" cert in your keychain):
#   INSTALLER_SIGN_ID="Developer ID Installer: Your Name (TEAMID)" ./packaging/macos/build-pkg.sh
#
# Optional notarization after building (a stored notarytool keychain profile):
#   NOTARY_PROFILE="my-profile" ./packaging/macos/build-pkg.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

PRODUCT_NAME="Codec"
PKG_IDENTIFIER="com.commit451.codec.installer"
VST3_NAME="Codec.vst3"
CLAP_NAME="Codec.clap"
# Base name for the produced installer file (dist/<basename>-<version>.pkg).
PKG_BASENAME="codec"

# Version comes from the plugin crate so the installer name tracks the plugin.
VERSION="$(grep -m1 '^version' plugin/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
VERSION="${VERSION:-0.0.0}"

UNIVERSAL=()
SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --universal)  UNIVERSAL=(--universal) ;;
        --skip-build) SKIP_BUILD=1 ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

BUNDLED="target/bundled"
DIST="dist"
PKGROOT="$DIST/pkgroot"
VST3_DEST="$PKGROOT/Library/Audio/Plug-Ins/VST3"
CLAP_DEST="$PKGROOT/Library/Audio/Plug-Ins/CLAP"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    echo "▶ Building release bundle (v$VERSION)…"
    cargo xtask bundle codec --release "${UNIVERSAL[@]}"
fi

if [[ ! -d "$BUNDLED/$VST3_NAME" ]]; then
    echo "✗ Missing '$BUNDLED/$VST3_NAME' — run without --skip-build first." >&2
    exit 1
fi

echo "▶ Staging payload…"
rm -rf "$PKGROOT"
mkdir -p "$VST3_DEST" "$CLAP_DEST"
# ditto --noextattr avoids copying quarantine / Finder-info xattrs and resource
# forks into the package. (macOS still stamps a com.apple.provenance xattr that
# can't be removed; the installer applies it to the real files, so no "._" sidecar
# files actually land on disk.) The code signature lives in the Mach-O and in
# _CodeSignature/, so it survives the copy.
ditto --noextattr --norsrc "$BUNDLED/$VST3_NAME" "$VST3_DEST/$VST3_NAME"
[[ -d "$BUNDLED/$CLAP_NAME" ]] && ditto --noextattr --norsrc "$BUNDLED/$CLAP_NAME" "$CLAP_DEST/$CLAP_NAME"

COMPONENT_PKG="$DIST/${PKG_BASENAME}-component.pkg"
FINAL_PKG="$DIST/${PKG_BASENAME}-${VERSION}.pkg"

echo "▶ pkgbuild…"
pkgbuild \
    --root "$PKGROOT" \
    --identifier "$PKG_IDENTIFIER" \
    --version "$VERSION" \
    --install-location "/" \
    "$COMPONENT_PKG"

echo "▶ productbuild…"
PRODUCTBUILD_ARGS=()
[[ -n "${INSTALLER_SIGN_ID:-}" ]] && PRODUCTBUILD_ARGS+=(--sign "$INSTALLER_SIGN_ID")
PRODUCTBUILD_ARGS+=(--package "$COMPONENT_PKG" "$FINAL_PKG")
productbuild "${PRODUCTBUILD_ARGS[@]}"

rm -f "$COMPONENT_PKG"
echo "✅ Built $FINAL_PKG"

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    echo "▶ Notarizing (this can take a few minutes)…"
    xcrun notarytool submit "$FINAL_PKG" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$FINAL_PKG"
    echo "✅ Notarized + stapled"
else
    echo "ℹ Unsigned/un-notarized. For public distribution set INSTALLER_SIGN_ID and NOTARY_PROFILE (see packaging/README.md)."
fi
