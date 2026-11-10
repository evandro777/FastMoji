#!/usr/bin/env bash
# Renders the picker, Preferences and About on a private Xvfb display and saves PNGs to
# target/ui-snapshots/<page>-<mode>.png. FastMoji has no list data to fake (the emoji table is built in), so the modes
# are the Appearance setting; the real settings, clipboard and windows are never touched.
# Usage: scripts/ui-snapshot.sh [page ...]   (main settings about; default all)   MODES="light dark" to narrow.
# Debug build only: the FASTMOJI_PAGE hook that opens a dialog at start is compiled out of release builds.
set -euo pipefail
cd "$(dirname "$0")/.."

PAGES=(main settings about)
MODES=${MODES:-"light dark"}
OUT=target/ui-snapshots
mkdir -p "$OUT"

cargo build --quiet

# Xvfb picks a free display and reports its number, so the script can never capture someone else's screen.
SCRATCH=$(mktemp -d)   # short path: a Unix socket path is limited to 108 bytes
DISPLAY_FILE=$SCRATCH/display
Xvfb -displayfd 3 -screen 0 1280x800x24 3>"$DISPLAY_FILE" >/dev/null 2>&1 &
XVFB=$!
APP=
cleanup() { [ -n "$APP" ] && kill "$APP" 2>/dev/null || true; kill "$XVFB" 2>/dev/null || true; rm -rf "$SCRATCH"; }
trap cleanup EXIT
for _ in $(seq 50); do [ -s "$DISPLAY_FILE" ] && break; sleep 0.1; done
[ -s "$DISPLAY_FILE" ] || { echo "Xvfb did not start" >&2; exit 1; }
export DISPLAY=:$(head -n1 "$DISPLAY_FILE")
# A private profile and runtime dir, so the owner's own FastMoji (settings, socket) is never involved.
export XDG_DATA_HOME=$SCRATCH/data XDG_RUNTIME_DIR=$SCRATCH/run
mkdir -p "$XDG_DATA_HOME/FastMoji" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

[ $# -gt 0 ] && names=("$@") || names=("${PAGES[@]}")

for name in "${names[@]}"; do
  for mode in $MODES; do
    echo "theme = \"$mode\"" > "$XDG_DATA_HOME/FastMoji/settings.toml"
    FASTMOJI_PAGE=$name target/debug/fastmoji >/dev/null 2>&1 &
    APP=$!
    for _ in $(seq 50); do
      xdotool search --onlyvisible --class fastmoji >/dev/null 2>&1 && break
      sleep 0.2
    done
    sleep 1
    import -window root "$OUT/$name-$mode.png"
    kill "$APP" 2>/dev/null || true
    wait "$APP" 2>/dev/null || true
    APP=
    echo "$OUT/$name-$mode.png"
  done
done
