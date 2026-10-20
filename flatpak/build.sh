#!/usr/bin/env bash
# Build and install the Flatpak for the current user.
#   flatpak/build.sh            release id  io.github.evandro777.FastMoji        (replaces an installed FastMoji!)
#   flatpak/build.sh --devel    dev id      io.github.evandro777.FastMoji.Devel  (installs next to it)
#   flatpak/build.sh --bundle   also write fastmoji.flatpak
# Needs: flatpak-builder, and once: flatpak install flathub org.freedesktop.Sdk//25.08 \
#        org.freedesktop.Sdk.Extension.rust-stable//25.08
set -euo pipefail
cd "$(dirname "$0")"
manifest=io.github.evandro777.FastMoji.yml
app=io.github.evandro777.FastMoji
devel=0; bundle=0
for arg in "$@"; do
  case "$arg" in
    --devel) devel=1 ;;
    --bundle) bundle=1 ;;
    *) echo "unknown option $arg" >&2; exit 2 ;;
  esac
done

echo ">> vendoring crates (needs network once)"
(cd .. && cargo vendor --locked flatpak/vendor >/dev/null)

if [ "$devel" = 1 ]; then
  app=$app.Devel
  # Same manifest with another app-id; paths are relative to the manifest, so keep it in flatpak/.
  sed "s/^app-id: .*/app-id: $app/" "$manifest" > "$app.yml"
  manifest=$app.yml
fi

echo ">> building $app"
flatpak-builder --user --install --force-clean --install-deps-from=flathub \
  --state-dir=.flatpak-builder build-dir "$manifest"

if [ "$bundle" = 1 ]; then
  flatpak-builder --user --force-clean --repo=repo --state-dir=.flatpak-builder build-dir "$manifest"
  flatpak build-bundle repo fastmoji.flatpak "$app"
  echo ">> wrote flatpak/fastmoji.flatpak"
fi
echo ">> run it with: flatpak run $app"
