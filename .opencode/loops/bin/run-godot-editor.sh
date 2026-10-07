#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Launch the Godot editor for this parity loop with display/port/user-data
# isolation. The addon inside the editor listens on $PARITY_BRIDGE_PORT
# (OPEN_GODOT_MCP_PORT wins over the shared EditorSettings), and the loop's
# MCP shim connects to the same port. The loop's Xvfb is started when the
# display is down.
#
# Usage: run-godot-editor.sh [extra godot args...]
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars "${PARITY_LOOP:-1}"

# The display must exist before the editor starts; create it when missing.
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
  OPEN_GODOT_MCP_PORT="$PARITY_BRIDGE_PORT" \
  XDG_DATA_HOME="$PARITY_LOOP_DIR/xdg-data" \
  XDG_CONFIG_HOME="$PARITY_LOOP_DIR/xdg-config" \
  XDG_CACHE_HOME="$PARITY_LOOP_DIR/xdg-cache" \
  "$godot" --editor --path "$project" "$@"
