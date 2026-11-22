#!/usr/bin/env bash
# Screenshot the bouquet in each output mode: .github/assets/<mode>.png.
#
# Needs KDE Plasma (spectacle, qdbus6), wezterm and cargo; oxipng is used when installed.
# Each shot opens a window, raises it with a KWin script and captures it. Do not use
# the desktop while it runs: a window you open can still cover the shot.
#
#   scripts/screenshots.sh              # every mode
#   scripts/screenshots.sh sextant      # only these
#
# Modes: sextant halfblock ascii
#
# Settings (environment): TBLOOM_FONT ("FiraCode Nerd Font"), TBLOOM_FONT_SIZE (13),
# TBLOOM_COLS (110), TBLOOM_ROWS (36), TBLOOM_DELAY (seconds to wait for the window, default 4).
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
delay=${TBLOOM_DELAY:-4}
all=(sextant halfblock ascii)
modes=("${@:-${all[@]}}")

for tool in wezterm spectacle qdbus6 cargo; do
  command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }
done

cargo build --release --quiet --manifest-path "$repo/Cargo.toml"
export TBLOOM_BIN="$repo/target/release/tbloom"
mkdir -p "$repo/.github/assets"

# Raise the window titled "tbloom ...": KDE does not give a new window the focus while
# another one gets input, and spectacle captures the active window.
raise() {
  local js
  js=$(mktemp --suffix=.js)
  echo 'for (const w of workspace.windowList()) if (w.caption.startsWith("tbloom")) workspace.activeWindow = w;' >"$js"
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" tbloom-raise >/dev/null
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.start
  sleep 0.5
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript tbloom-raise >/dev/null
  rm -f "$js"
}

shoot() { # mode, tbloom arguments
  local out="$repo/.github/assets/$1.png"
  # --aa: the smooth frame, drawn once the input is quiet
  TBLOOM_ARGS="$2 --aa" TBLOOM_TITLE="tbloom${2:+ $2}" \
    wezterm --config-file "$repo/scripts/screenshot.wezterm.lua" start --always-new-process &
  local pid=$!
  sleep "$delay"
  raise
  spectacle --activewindow --background --nonotify --output "$out"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  if command -v oxipng >/dev/null; then oxipng -q -o4 --strip all "$out"; fi
  echo "$out"
}

for mode in "${modes[@]}"; do
  case $mode in
    sextant) shoot sextant "--nerd" ;;
    halfblock) shoot halfblock "" ;;
    ascii) shoot ascii "--ascii" ;;
    *) echo "unknown mode: $mode (modes: ${all[*]})" >&2; exit 2 ;;
  esac
done
