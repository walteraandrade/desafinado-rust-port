#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./install.sh [--root DIR] [--debug] [--skip-build]
       ./install.sh --uninstall [--root DIR]

Installs the native Desafinado binary with cargo install, then a desktop entry
"Desafinado (native)" and its icon under ${XDG_DATA_HOME:-~/.local/share}.

  --root DIR    cargo install root (default: $CARGO_INSTALL_ROOT, else $CARGO_HOME, else ~/.cargo)
  --debug       build the dev profile instead of release
  --skip-build  only write the desktop entry and icons for an already installed binary
  --uninstall   remove the binary, the desktop entry and the icons
EOF
}

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}"
profile=()
build=1
action=install

while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="$2"; shift 2 ;;
    --root=*) root="${1#--root=}"; shift ;;
    --debug) profile=(--debug); shift ;;
    --skip-build) build=0; shift ;;
    --uninstall) action=uninstall; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

root="$(mkdir -p "$root" && cd "$root" && pwd)"
bin="$root/bin/desafinado"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
id="desafinado-native"
desktop="$data/applications/$id.desktop"
png="$data/icons/hicolor/512x512/apps/$id.png"
svg="$data/icons/hicolor/scalable/apps/$id.svg"

refresh() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data/applications" >/dev/null 2>&1 || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$data/icons/hicolor" >/dev/null 2>&1 || true
  fi
}

if [ "$action" = uninstall ]; then
  if command -v cargo >/dev/null 2>&1 && [ -x "$bin" ]; then
    cargo uninstall --root "$root" desafinado || rm -f "$bin"
  else
    rm -f "$bin"
  fi
  rm -f "$desktop" "$png" "$svg"
  refresh
  echo "Removed $bin, $desktop and the icons."
  echo "Your settings and key stay in ${XDG_CONFIG_HOME:-$HOME/.config}/desafinado-native."
  exit 0
fi

if [ "$build" = 1 ]; then
  cargo install --locked ${profile[@]+"${profile[@]}"} --path "$repo/crates/app" --root "$root"
fi

if [ ! -x "$bin" ]; then
  echo "No binary at $bin" >&2
  exit 1
fi

mkdir -p "$(dirname "$desktop")" "$(dirname "$png")" "$(dirname "$svg")"
install -m 644 "$repo/packaging/linux/$id.png" "$png"
install -m 644 "$repo/packaging/linux/$id.svg" "$svg"

case "$bin" in
  *[\\\"\`\$]*) echo "The install path must not contain \\, \", \` or \$: $bin" >&2; exit 1 ;;
esac
cat > "$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Desafinado (native)
GenericName=Markdown Editor
Comment=Markdown editor with an AI co-author (native build)
Exec="$bin" %f
TryExec=$bin
Icon=$id
Terminal=false
StartupNotify=true
StartupWMClass=$id
MimeType=text/markdown;text/x-markdown;
Categories=Utility;TextEditor;
Keywords=markdown;editor;ai;skills;
EOF
chmod 644 "$desktop"
refresh

echo "Installed $bin"
echo "Desktop entry: $desktop"
case ":$PATH:" in
  *":$root/bin:"*) ;;
  *) echo "Add $root/bin to PATH to run desafinado from a terminal." ;;
esac
