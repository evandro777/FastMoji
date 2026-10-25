#!/usr/bin/env bash
# Build FastMoji-<version>-x86_64.AppImage (+ .zsync for delta updates) in the repo root.
# Needs: the system deps from AGENTS.md, a Rust toolchain, and `appimagetool` on PATH.
# Host libraries (fontconfig, xkbcommon, X11) are not bundled: every X11 desktop has them.
set -euo pipefail
cd "$(dirname "$0")/.."
id=io.github.evandro777.FastMoji
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
out=FastMoji-$version-x86_64.AppImage

cargo build --release --locked

rm -rf appimage/AppDir
d=appimage/AppDir
install -Dm755 target/release/fastmoji "$d/usr/bin/fastmoji"
install -Dm644 flatpak/$id.desktop "$d/usr/share/applications/$id.desktop"
install -Dm644 flatpak/$id.svg "$d/usr/share/icons/hicolor/scalable/apps/$id.svg"
install -Dm644 flatpak/$id.metainfo.xml "$d/usr/share/metainfo/$id.metainfo.xml"
# appimagetool wants the desktop file and icon at the root too.
cp "$d/usr/share/applications/$id.desktop" "$d/usr/share/icons/hicolor/scalable/apps/$id.svg" "$d/"
ln -s usr/bin/fastmoji "$d/AppRun"

# Update info: AppImageUpdate-style tools read it from the file and fetch the .zsync from the latest release.
ARCH=x86_64 appimagetool --no-appstream \
  -u "gh-releases-zsync|evandro777|FastMoji|latest|FastMoji-*x86_64.AppImage.zsync" "$d" "$out"
echo ">> $out and $out.zsync"
