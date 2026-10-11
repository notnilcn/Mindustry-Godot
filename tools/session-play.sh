#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# session-play.sh — one-call (re)start for a parity session's Godot client,
# for agents that drive the bridge through tools/gmcp.py.
#
#   tools/session-play.sh N            # restart editor (picks up a new build), play game.tscn
#   tools/session-play.sh N --import   # ...then Game Data import of the full-tech-tree profile
#   tools/session-play.sh N --play     # only (re)play the scene, keep the editor
#
# Session 1 = main checkout, bridge 6970, display :10. N >= 2 = worktree
# ../Mindustry-Godot-sessionN, bridge 6970+10(N-1), display :(9+N), isolated
# XDG dirs (so user:// is per-session). Prints the env line to export for gmcp.
set -euo pipefail
N="${1:?session number}"; shift || true
MODE="restart"; IMPORT=0
for a in "$@"; do
  case "$a" in
    --import) IMPORT=1 ;;
    --play) MODE="play" ;;
  esac
done
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MAIN="$(cd "$(dirname "$(git -C "$HERE" rev-parse --path-format=absolute --git-common-dir)")" && pwd)"
ZIP="${PROFILE_ZIP:-$(cd "$MAIN/.." && pwd)/mindustry.save.full.tech.tree.zip}"
PORT=$((6970 + 10 * (N - 1)))
export OPEN_GODOT_MCP_PORT="$PORT"
G() { timeout "${T:-90}" python3 "$MAIN/tools/gmcp.py" --timeout 80 "$@"; }

if [ "$MODE" = restart ]; then
  if [ "$N" -eq 1 ]; then
    pid=$(pgrep -f "godot --editor --path client$" | head -1 || true)
    [ -n "$pid" ] && kill "$pid" && sleep 3
    (cd "$MAIN" && OPEN_GODOT_MCP_PORT=$PORT DISPLAY=:10 setsid godot --editor --path client >>runs/editor-console.log 2>&1 </dev/null &)
  else
    bash "$MAIN/tools/parity-session.sh" --session "$N" --stop >/dev/null 2>&1 || true
    bash "$MAIN/tools/parity-session.sh" --session "$N" --no-seed-builds >/dev/null
  fi
  for _ in $(seq 1 40); do
    sleep 2
    G godot_health '{"action":"check"}' 2>/dev/null | grep -q '"bridge_connected": true' && break
  done
fi

play() {
  G godot_game '{"action":"stop"}' >/dev/null 2>&1 || true
  G godot_editor_edit '{"action":"open_scene","params":{"path":"res://scenes/game.tscn"}}' \
    godot_game '{"action":"play","params":{"scene":"res://scenes/game.tscn"}}' >/dev/null
  for _ in $(seq 1 30); do
    sleep 2
    G godot_game '{"action":"status"}' 2>/dev/null | grep -q '"runtime_connected": true' && break
  done
  sleep 4
}
play
if [ "$IMPORT" = 1 ]; then
  G --eval "return get_node(\"/root/MindUi\").data_import(\"$ZIP\")" || true
  sleep 4
  play
fi
G --eval 'return [OS.get_process_id(), ProjectSettings.globalize_path("user://"), get_node("/root/Spine/MindCampaign").get_tech_state()]'
echo "export OPEN_GODOT_MCP_PORT=$PORT"
