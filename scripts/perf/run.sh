#!/usr/bin/env bash
# Runs one performance scenario against a perf-probe build under Xvfb.
# Usage: scripts/perf/run.sh NAME DOC SCENARIO [BIN]
# Needs: Xvfb on $PERF_DISPLAY (default :97), tools/xdrive built, a binary built with
#   cargo build --release -p desafinado --features perf-probe
# Runs are serial: one display, one window. Use another PERF_DISPLAY and PERF_DIR to run two at once.
# SEED_UD=dir copies a user data folder in first; SETTLE=seconds waits after the first frame.
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
work=${PERF_DIR:-$HOME/.cache/desafinado-perf}
name=${1:?name} doc=${2:?doc} script=${3:?scenario} bin=${4:-$repo/target/release/desafinado}
driver=${XDRIVE:-$repo/tools/xdrive/target/release/xdrive}
out=$work/runs/$name
rm -rf "$out"; mkdir -p "$out"
cp "$doc" "$out/doc.md"
# The single-instance socket lives in the user data folder and needs a short path.
ud=$(mktemp -d -t dp.XXXXXX)
[ -n "${SEED_UD:-}" ] && cp -r "$SEED_UD"/. "$ud"/
export DISPLAY=${PERF_DISPLAY:-:97} XDG_SESSION_TYPE=x11 DESAFINADO_USER_DATA_DIR="$ud" DESAFINADO_FAKE_AI=1 DESAFINADO_PERF_OUT="$out/perf.jsonl"
unset WAYLAND_DISPLAY
t0=$(date +%s%N)
"$bin" "$out/doc.md" > "$out/log" 2>&1 &
pid=$!
for _ in $(seq 400); do
  grep -q presented "$out/perf.jsonl" 2>/dev/null && break
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "$name: the app exited before its first frame; see $out/log" >&2
    rm -rf "$ud"; exit 1
  fi
  sleep 0.05
done
sleep "${SETTLE:-3}"
# utime + stime, read after the ")" that ends the command name (names may hold spaces).
ticks() { sed 's/.*) //' "$1" 2>/dev/null | awk '{print $12+$13}'; }
read_cpu() { ticks "/proc/$pid/stat"; }
threads() { for t in /proc/$pid/task/*; do c=$(cat "$t/comm" 2>/dev/null) s=$(ticks "$t/stat") && [ -n "$s" ] && printf '%s\t%s\n' "$c" "$s"; done; }
threads > "$out/threads0"; c0=$(read_cpu); w0=$(date +%s%N)
"$driver" < "$script"
sleep 1
c1=$(read_cpu); w1=$(date +%s%N); threads > "$out/threads1"; echo "$(((w0-t0)/1000000))" > "$out/drive_start_ms"
grep -E "VmHWM|VmRSS" "/proc/$pid/status" > "$out/mem"
echo "cpu_ticks=$((c1-c0)) wall_ms=$(((w1-w0)/1000000))" > "$out/cpu"
kill "$pid"; wait "$pid" 2>/dev/null
rm -rf "$ud"
echo "$name done: $(cat "$out/cpu") $(tr '\n' ' ' < "$out/mem")"
