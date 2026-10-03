#!/usr/bin/env bash
# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# Plan 22 M5 §6.7: opt-in file/URI association installer for `.msav`, `.msch`
# and the dual `mindustry://` / `mindustry-godot://` scheme. The game never
# installs these itself. Mirrors `mind_core::platform::assoc` (source of truth).
#
# Usage:
#   tools/associate.sh --install [--linux] [--windows] [--exe PATH]
#   tools/associate.sh --uninstall [--linux] [--windows]
#   tools/associate.sh --print [--windows]        # emit the file body only

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Mindustry-Godot"
EXE="${MIND_EXE:-$REPO_ROOT/build/export/$APP_NAME}"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
DESKTOP_FILE="$DESKTOP_DIR/mindustry-godot.desktop"
MIME_SAVE="application/x-mindustry-save"
MIME_SCHEMATIC="application/x-mindustry-schematic"

ACTION=""
PLATFORM=""
while [[ $# -gt 0 ]]; do
  arg="$1"
  case "$arg" in
    --install) ACTION="install" ;;
    --uninstall) ACTION="uninstall" ;;
    --print) ACTION="print" ;;
    --linux) PLATFORM="linux" ;;
    --windows) PLATFORM="windows" ;;
    --exe=*) EXE="${arg#--exe=}" ;;
    --exe)
      [[ $# -ge 2 ]] || { echo "associate.sh: --exe needs a path" >&2; exit 2; }
      EXE="$2"
      shift
      ;;
    *) echo "associate.sh: unknown option $arg" >&2; exit 2 ;;
  esac
  shift
done
[[ -z "$PLATFORM" ]] && PLATFORM="linux"
[[ -z "$ACTION" ]] && { echo "associate.sh: need --install|--uninstall|--print" >&2; exit 2; }

desktop_body() {
  cat <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=Saves, schematics and join links for $APP_NAME
Exec="$EXE" %f
Terminal=false
Categories=Game;
MimeType=$MIME_SAVE;$MIME_SCHEMATIC;x-scheme-handler/mindustry;x-scheme-handler/mindustry-godot;
EOF
}

windows_reg_body() {
  # Escape backslashes for the .reg format.
  local esc="${EXE//\\/\\\\}"
  cat <<EOF
Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\Classes\.msav]
@="MindustryGodot.Save"

[HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Save\DefaultIcon]
@="\\"$esc\\",0"

[HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Save\shell\open\command]
@="\\"$esc\\" \\"%1\\""

[HKEY_CURRENT_USER\Software\Classes\.msch]
@="MindustryGodot.Schematic"

[HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Schematic\shell\open\command]
@="\\"$esc\\" \\"%1\\""

[HKEY_CURRENT_USER\Software\Classes\mindustry]
@="URL:Mindustry-Godot Protocol"
"URL Protocol"=""

[HKEY_CURRENT_USER\Software\Classes\mindustry\shell\open\command]
@="\\"$esc\\" \\"%1\\""

[HKEY_CURRENT_USER\Software\Classes\mindustry-godot]
@="URL:Mindustry-Godot Protocol"
"URL Protocol"=""

[HKEY_CURRENT_USER\Software\Classes\mindustry-godot\shell\open\command]
@="\\"$esc\\" \\"%1\\""
EOF
}

windows_unreg_body() {
  cat <<'EOF'
Windows Registry Editor Version 5.00

[-HKEY_CURRENT_USER\Software\Classes\.msav]
[-HKEY_CURRENT_USER\Software\Classes\.msch]
[-HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Save]
[-HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Schematic]
[-HKEY_CURRENT_USER\Software\Classes\mindustry]
[-HKEY_CURRENT_USER\Software\Classes\mindustry-godot]
EOF
}

if [[ "$PLATFORM" == "linux" ]]; then
  case "$ACTION" in
    print) desktop_body ;;
    install)
      mkdir -p "$DESKTOP_DIR"
      desktop_body > "$DESKTOP_FILE"
      if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
      fi
      if command -v xdg-mime >/dev/null 2>&1; then
        xdg-mime default mindustry-godot.desktop "$MIME_SAVE" "$MIME_SCHEMATIC" || true
      fi
      echo "associate.sh: installed $DESKTOP_FILE"
      ;;
    uninstall)
      rm -f "$DESKTOP_FILE"
      if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
      fi
      echo "associate.sh: removed $DESKTOP_FILE"
      ;;
  esac
else
  # Windows: run the .reg through reg.exe via interop from WSL.
  case "$ACTION" in
    print) windows_reg_body ;;
    install)
      tmp="$(mktemp --suffix=.reg)"
      windows_reg_body > "$tmp"
      winpath="$(wslpath -w "$tmp")"
      reg.exe import "$winpath"
      rm -f "$tmp"
      echo "associate.sh: registered HKCU .msav/.msch + mindustry(-godot)://"
      ;;
    uninstall)
      tmp="$(mktemp --suffix=.reg)"
      windows_unreg_body > "$tmp"
      winpath="$(wslpath -w "$tmp")"
      reg.exe import "$winpath"
      rm -f "$tmp"
      echo "associate.sh: removed HKCU associations"
      ;;
  esac
fi
