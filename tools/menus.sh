#!/usr/bin/env bash
# Records the File and Edit menus and the Review popover arriving and leaving,
# in a virtual display: Esc, a click outside and a chosen row each play the way
# out, and a menu replaced by the Review popover goes at once.
# Usage: tools/menus.sh [OUT.mp4] [BIN]
# Needs: Xvfb, ffmpeg, tools/xdrive built (cargo build --release in tools/xdrive),
#   and the app built (cargo build -p desafinado).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
out=${1:-$repo/menus.mp4}
bin=${2:-${CARGO_TARGET_DIR:-$repo/target}/debug/desafinado}
driver=${XDRIVE:-$here/xdrive/target/release/xdrive}
display=${MENUS_DISPLAY:-:98}
work=$(mktemp -d -t dm.XXXXXX)
pids=()
finish() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  rm -rf "$work"
}
trap finish EXIT

printf '# Menus\n\nA paragraph to write in while the menus come and go.\n' > "$work/menus.md"
Xvfb "$display" -screen 0 1280x800x24 -nolisten tcp > /dev/null 2>&1 &
pids+=($!)
sleep 1
export DISPLAY=$display XDG_SESSION_TYPE=x11 DESAFINADO_USER_DATA_DIR="$work/ud" DESAFINADO_FAKE_AI=1
unset WAYLAND_DISPLAY
"$bin" "$work/menus.md" > "$work/log" 2>&1 &
app=$!
pids+=("$app")
sleep 6
kill -0 "$app" || { cat "$work/log"; echo "The app exited." >&2; exit 1; }

ffmpeg -loglevel error -y -f x11grab -framerate 60 -video_size 1280x800 -i "$display" \
  -c:v libx264 -preset veryfast -pix_fmt yuv420p "$out" &
recorder=$!
pids+=("$recorder")
sleep 1

"$driver" <<'SCRIPT'
focus
click 640 400
sleep 600
key alt+f
sleep 900
key esc
sleep 900
key alt+e
sleep 900
click 900 600
sleep 900
key alt+f
sleep 300
key alt+e
sleep 900
key esc
sleep 900
key alt+e
sleep 300
key up
sleep 600
key enter
sleep 900
key ctrl+alt+r
sleep 600
key alt+e
sleep 600
key enter
sleep 1200
key esc
sleep 1200
SCRIPT

kill -INT "$recorder"
wait "$recorder" || true
grep -q panicked "$work/log" && { cat "$work/log"; echo "The app panicked." >&2; exit 1; }
echo "Recorded $out"
