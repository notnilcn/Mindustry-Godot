#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Launch the Godot editor for this parity loop with display/port/user-data
# isolation. The addon inside the editor listens on $PARITY_BRIDGE_PORT
# (OPEN_GODOT_MCP_PORT wins over the shared EditorSettings), and the loop's
# MCP shim connects to the same port.
#
# Godot renders on this loop's headless weston compositor (Wayland + GPU):
# Xvfb has no DRI3, so Godot on Xvfb falls back to llvmpipe software rendering
# and pins the CPU. The Java reference stays on Xvfb; see ../README.md. Set
# PARITY_GODOT_DISPLAY=x11 to force the X11/Xvfb path.
#
# Usage: run-godot-editor.sh [extra godot args...]
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars "${PARITY_LOOP:-1}"

# GPU path first: the editor runs under the loop's weston when available.
display_env=()
driver_args=()
if parity_ensure_wayland; then
  display_env=(
    "WAYLAND_DISPLAY=$PARITY_WAYLAND_SOCKET"
    "XDG_RUNTIME_DIR=$PARITY_WAYLAND_DIR"
  )
  driver_args=(--display-driver wayland)
else
  echo "[parity-loop $PARITY_LOOP] editor: using Xvfb $PARITY_DISPLAY (software rendering)" >&2
fi

# Xvfb still backs the Java reference leg and X11-only tooling.
parity_ensure_display

godot="${GODOT_BIN:-}"
if [ -z "$godot" ]; then
  if command -v godot4 >/dev/null 2>&1; then
    godot="$(command -v godot4)"
  elif command -v godot >/dev/null 2>&1; then
    godot="$(command -v godot)"
  else
    echo "run-godot-editor: godot binary not found; set GODOT_BIN" >&2
    exit 1
  fi
fi

project="$PARITY_WORKTREE/client"
[ -f "$project/project.godot" ] || { echo "run-godot-editor: not a Godot project: $project" >&2; exit 1; }

mkdir -p "$PARITY_LOOP_DIR"/{xdg-data,xdg-config,xdg-cache,logs}
echo "[parity-loop $PARITY_LOOP] editor: display=$PARITY_DISPLAY bridge=$PARITY_BRIDGE_PORT project=$project" >&2

exec env \
  DISPLAY="$PARITY_DISPLAY" \
  "${display_env[@]}" \
  OPEN_GODOT_MCP_PORT="$PARITY_BRIDGE_PORT" \
  XDG_DATA_HOME="$PARITY_LOOP_DIR/xdg-data" \
  XDG_CONFIG_HOME="$PARITY_LOOP_DIR/xdg-config" \
  XDG_CACHE_HOME="$PARITY_LOOP_DIR/xdg-cache" \
  "$godot" --editor --path "$project" "${driver_args[@]}" "$@"
