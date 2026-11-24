#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/auto-install.sh enable | disable | status | run [--force]

Keeps the installed desafinado binary on the latest origin/main. A systemd user
timer checks origin/main every 5 minutes; when it moved, a private clone builds
it with ./install.sh and replaces ~/.cargo/bin/desafinado. Your checkout and any
running window are left alone: reopen the app to use the new version. A desktop
notification says when a version is installed or a build fails; a failed build
keeps the old binary.

  enable    clone main, install and start the timer
  disable   stop and remove the timer (the clone stays in the state dir)
  status    show the timer, the installed commit and the last failure
  run       check once now (--force rebuilds even a commit that failed)

Optional: ${XDG_CONFIG_HOME:-~/.config}/desafinado-native/auto-install.env is
sourced before each run, for proxy or toolchain variables.
EOF
}

unit=desafinado-auto-install
state="${XDG_STATE_HOME:-$HOME/.local/state}/desafinado-native/auto-install"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
env_file="${XDG_CONFIG_HOME:-$HOME/.config}/desafinado-native/auto-install.env"
clone="$state/repo"

log() { echo "auto-install: $*"; }

notify() {
  local urgency="$1" title="$2" body="$3"
  log "$title: $body"
  if command -v notify-send >/dev/null 2>&1; then
    notify-send -a Desafinado -i desafinado-native -u "$urgency" "$title" "$body" || true
  fi
}

ensure_clone() {
  if [ -d "$clone/.git" ]; then
    return
  fi
  local remote
  remote="$(cat "$state/remote")"
  git clone --quiet "$remote" "$clone"
}

run_once() {
  local force="${1:-}"
  mkdir -p "$state"
  exec 9>"$state/lock"
  if ! flock -n 9; then
    log "another run is in progress"
    return 0
  fi
  if [ -r "$env_file" ]; then
    # shellcheck source=/dev/null
    . "$env_file"
  fi
  if pgrep -f '[c]argo install .*crates/app' >/dev/null; then
    log "a cargo install is already running; trying again on the next tick"
    return 0
  fi

  ensure_clone
  git -C "$clone" fetch --quiet origin main
  local head installed failed
  head="$(git -C "$clone" rev-parse FETCH_HEAD)"
  installed="$(cat "$state/installed" 2>/dev/null || true)"
  failed="$(cat "$state/failed" 2>/dev/null || true)"
  if [ "$head" = "$installed" ]; then
    return 0
  fi
  if [ "$head" = "$failed" ] && [ "$force" != --force ]; then
    return 0
  fi

  git -C "$clone" checkout --quiet --force --detach "$head"
  git -C "$clone" clean -fdq
  local short subject
  short="$(git -C "$clone" rev-parse --short HEAD)"
  subject="$(git -C "$clone" log -1 --format=%s)"
  log "building $short: $subject"

  if CARGO_TARGET_DIR="$state/target" "$clone/install.sh" >"$state/build.log" 2>&1; then
    echo "$head" >"$state/installed"
    rm -f "$state/failed"
    if [ -f "$clone/scripts/auto-install.sh" ]; then
      install -m 755 "$clone/scripts/auto-install.sh" "$state/auto-install.sh.new"
      mv -f "$state/auto-install.sh.new" "$state/auto-install.sh"
    fi
    notify normal "Desafinado updated" "$short $subject. Reopen the app to use it."
  else
    echo "$head" >"$state/failed"
    tail -n 40 "$state/build.log"
    notify critical "Desafinado build failed" "$short $subject. The old version stays. Log: $state/build.log"
    return 1
  fi
}

write_units() {
  mkdir -p "$units"
  cat >"$units/$unit.service" <<EOF
[Unit]
Description=Build and install Desafinado when origin/main moves

[Service]
Type=oneshot
ExecStart=$state/auto-install.sh run
Environment=PATH=$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin
Nice=10
CPUWeight=20
TimeoutStartSec=90min
EOF
  cat >"$units/$unit.timer" <<EOF
[Unit]
Description=Check origin/main for a new Desafinado every 5 minutes

[Timer]
OnBootSec=3min
OnUnitInactiveSec=5min

[Install]
WantedBy=timers.target
EOF
}

enable() {
  local repo
  repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  mkdir -p "$state"
  git -C "$repo" remote get-url origin >"$state/remote"
  ensure_clone
  install -m 755 "$repo/scripts/auto-install.sh" "$state/auto-install.sh"
  write_units
  systemctl --user daemon-reload
  systemctl --user enable --now "$unit.timer"
  log "enabled: $units/$unit.timer and $unit.service"
  log "logs: journalctl --user -u $unit.service"
  log "turn off: scripts/auto-install.sh disable"
}

disable() {
  systemctl --user disable --now "$unit.timer" 2>/dev/null || true
  rm -f "$units/$unit.timer" "$units/$unit.service"
  systemctl --user daemon-reload
  log "disabled; remove $state to free the clone and its build cache"
}

status() {
  systemctl --user list-timers "$unit.timer" --all --no-pager || true
  echo "installed: $(cat "$state/installed" 2>/dev/null || echo none)"
  echo "last failure: $(cat "$state/failed" 2>/dev/null || echo none)"
}

main() {
  case "${1:-}" in
    enable) enable ;;
    disable) disable ;;
    status) status ;;
    run) run_once "${2:-}" ;;
    -h|--help) usage ;;
    *) usage >&2; return 2 ;;
  esac
}

main "$@"
exit
