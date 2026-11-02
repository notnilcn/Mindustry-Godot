#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# twin_item.sh — per-item evidence folders for twin verification (2026-11-02).
#
# One folder per twin item, one subfolder per leg, timestamped frames, and any
# accompanying log/txt/md/json file at the item root:
#
#   runs/PAR-00042/
#     java/   20261102-112233-01-menu.png
#     godot/  20261102-112233-01-menu.png
#     20261102-112233-run.md          (written by the agent via `note`)
#     20261102-112233-java-game.log   (written via `log`)
#
# The existing runs/archive/** tree is frozen reference evidence: never move or
# copy its images into these folders. Only frames captured in this session go
# under runs/PAR-####/.
#
# Usage:
#   tools/twin_item.sh dir PAR-00042           # create the folders, print the root
#   tools/twin_item.sh stamp                   # 20261102-112233
#   tools/twin_item.sh godot-shot PAR-00042 label
#   tools/twin_item.sh java-shot  PAR-00042 label
#   tools/twin_item.sh note PAR-00042 label [text]   # create/write the run report
#   tools/twin_item.sh log  PAR-00042 label [cmd ...] # capture a command's output
#
# `label` is sanitised into the filename (spaces and slashes become '-').
# Environment:
#   TWIN_RUNS            runs root (default <repo>/runs)
#   OPEN_GODOT_MCP_PORT  MCP bridge port (default 6970; also read by gmcp.py)
#   PARITY_DISPLAY       X display for the Java leg (default :10)
#   TWIN_JAVA_TITLE      Java window title (default Mindustry)
#   PARITY_PY            parity-eval helper venv python (auto-resolved)
#   PARITY_SKILL         parity-eval skill dir (auto-resolved)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNS="${TWIN_RUNS:-$ROOT/runs}"
DISPLAY="${PARITY_DISPLAY:-:10}"
JAVA_TITLE="${TWIN_JAVA_TITLE:-Mindustry}"

die() { echo "twin_item: $*" >&2; exit 2; }

usage() { sed -n '3,44p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

stamp() { date +%Y%m%d-%H%M%S; }

slug() {
  printf '%s' "$1" | tr ' /' '--' | tr -cd 'A-Za-z0-9._-'
}

item_root() {
  local id="${1:-}"
  [[ "$id" =~ ^PAR-[0-9]{4}$ ]] || die "item id must look like PAR-00042 (got '$id')"
  printf '%s/%s' "$RUNS" "$id"
}

ensure_dir() {
  local root
  root="$(item_root "${1:-}")"
  mkdir -p "$root/java" "$root/godot"
  printf '%s\n' "$root"
}

find_parity_py() {
  if [ -n "${PARITY_PY:-}" ] && [ -x "$PARITY_PY" ]; then
    printf '%s\n' "$PARITY_PY"
    return 0
  fi
  local candidate
  for candidate in \
    "$HOME/.local/share/parity-eval/venv/bin/python" \
    "$HOME/.local/share/parity-eval/venv/bin/python3"; do
    if [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

find_parity_skill() {
  local candidate
  for candidate in \
    "${PARITY_SKILL:-}" \
    "$(dirname "$ROOT")/.claude/skills/parity-eval" \
    "$HOME/.claude/skills/parity-eval"; do
    if [ -n "$candidate" ] && [ -d "$candidate/scripts" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

cmd="${1:-}"; shift || true
case "$cmd" in
  dir|ensure)
    ensure_dir "$@"
    ;;
  stamp)
    stamp
    ;;
  godot-shot)
    id="${1:-}"; label="${2:-action}"; shift 2 || true
    root="$(ensure_dir "$id")"
    out="$root/godot/$(stamp)-$(slug "$label").png"
    python3 "$ROOT/tools/gmcp.py" --screenshot "$out" >/dev/null
    printf '%s\n' "$out"
    ;;
  java-shot)
    id="${1:-}"; label="${2:-action}"; shift 2 || true
    root="$(ensure_dir "$id")"
    out="$root/java/$(stamp)-$(slug "$label").png"
    py="$(find_parity_py)" || die "parity-eval helper venv python not found (set PARITY_PY)"
    skill="$(find_parity_skill)" || die "parity-eval scripts not found (set PARITY_SKILL)"
    DISPLAY="$DISPLAY" "$py" "$skill/scripts/capture_screen.py" \
      --window-title "$JAVA_TITLE" --output "$out" >/dev/null
    printf '%s\n' "$out"
    ;;
  note)
    id="${1:-}"; label="${2:-run}"; shift 2 || true
    root="$(ensure_dir "$id")"
    out="$root/$(stamp)-$(slug "$label").md"
    if [ "$#" -gt 0 ]; then
      printf '%s\n' "$*" >"$out"
    else
      : >"$out"
    fi
    printf '%s\n' "$out"
    ;;
  log)
    id="${1:-}"; label="${2:-log}"; shift 2 || true
    root="$(ensure_dir "$id")"
    out="$root/$(stamp)-$(slug "$label").log"
    if [ "$#" -gt 0 ]; then
      "$@" >"$out" 2>&1 || true
    else
      cat >"$out"
    fi
    printf '%s\n' "$out"
    ;;
  -h|--help|help|"")
    usage
    ;;
  *)
    die "unknown action '$cmd' (see --help)"
    ;;
esac
