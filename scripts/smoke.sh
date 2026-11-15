#!/usr/bin/env bash
# Starts the app with a fresh user data dir and a sample file, and fails if it
# exits or panics within the first seconds. Run it under a display (xvfb-run in CI).
set -euo pipefail

bin="${1:-target/debug/desafinado}"
seconds="${SMOKE_SECONDS:-15}"
work="$(mktemp -d)"
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$work"' EXIT

printf '# Smoke\n\nA paragraph to open.\n' > "$work/smoke.md"
export DESAFINADO_USER_DATA_DIR="$work/user-data"
export DESAFINADO_FAKE_AI=1
unset WAYLAND_DISPLAY
export XDG_SESSION_TYPE=x11

"$bin" "$work/smoke.md" > "$work/log" 2>&1 &
pid=$!

for _ in $(seq "$seconds"); do
  sleep 1
  if ! kill -0 "$pid" 2>/dev/null; then
    cat "$work/log"
    echo "The app exited during startup." >&2
    exit 1
  fi
done

if grep -q 'panicked' "$work/log"; then
  cat "$work/log"
  echo "The app panicked during startup." >&2
  exit 1
fi

if [ ! -e "$DESAFINADO_USER_DATA_DIR/instance.lock" ]; then
  cat "$work/log"
  echo "The app did not take its instance lock." >&2
  exit 1
fi

echo "The app is running after ${seconds}s."
