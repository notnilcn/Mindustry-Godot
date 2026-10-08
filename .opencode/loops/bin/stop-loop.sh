#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Stop a parity loop's display servers - the dedicated java weston/Xwayland
# compositor (GPU), the Godot weston compositor, and a leftover Xvfb software
# fallback (and optionally remove its worktree / runtime dir). It never kills
# editors, game clients, or opencode sessions; stop those from the loop
# session first.
#
# Usage: stop-loop.sh <loop-id> [--remove-worktree] [--purge]
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"

remove_worktree=0
purge=0
id=""
for arg in "$@"; do
  case "$arg" in
    --remove-worktree) remove_worktree=1 ;;
    --purge) purge=1 ;;
    -h | --help)
      sed -n '3,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    '' | *[!0-9]*)
      echo "stop-loop: unknown argument: $arg" >&2
      exit 2
      ;;
    *) id="$arg" ;;
  esac
done
parity_loop_vars "${id:-${PARITY_LOOP:-1}}"

if [ -f "$PARITY_LOOP_DIR/xvfb.pid" ]; then
  pid="$(cat "$PARITY_LOOP_DIR/xvfb.pid")"
  if kill -0 "$pid" 2>/dev/null && tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -q 'Xvfb'; then
    echo "[loop $PARITY_LOOP] stopping Xvfb $PARITY_DISPLAY (pid $pid)"
    kill "$pid"
  fi
  rm -f "$PARITY_LOOP_DIR/xvfb.pid"
else
  echo "[loop $PARITY_LOOP] no Xvfb pidfile"
fi

if [ -f "$PARITY_LOOP_DIR/java-weston.pid" ]; then
  pid="$(cat "$PARITY_LOOP_DIR/java-weston.pid")"
  if kill -0 "$pid" 2>/dev/null && tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -q 'weston'; then
    echo "[loop $PARITY_LOOP] stopping java weston/Xwayland $PARITY_JAVA_WAYLAND_SOCKET (pid $pid)"
    kill "$pid"
  fi
  rm -f "$PARITY_LOOP_DIR/java-weston.pid"
else
  echo "[loop $PARITY_LOOP] no java weston pidfile"
fi

if [ -f "$PARITY_LOOP_DIR/weston.pid" ]; then
  pid="$(cat "$PARITY_LOOP_DIR/weston.pid")"
  if kill -0 "$pid" 2>/dev/null && tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -q 'weston'; then
    echo "[loop $PARITY_LOOP] stopping weston $PARITY_WAYLAND_SOCKET (pid $pid)"
    kill "$pid"
  fi
  rm -f "$PARITY_LOOP_DIR/weston.pid"
else
  echo "[loop $PARITY_LOOP] no weston pidfile"
fi

if [ "$remove_worktree" -eq 1 ] && [ "$PARITY_LOOP" -ge 2 ]; then
  if ! git -C "$PARITY_MAIN" worktree list --porcelain | grep -qxF "worktree $PARITY_WORKTREE"; then
    echo "[loop $PARITY_LOOP] no worktree registered at $PARITY_WORKTREE"
  elif [ -n "$(git -C "$PARITY_WORKTREE" status --porcelain)" ]; then
    echo "[loop $PARITY_LOOP] refusing to remove dirty worktree (commit or discard changes first)" >&2
    exit 1
  else
    echo "[loop $PARITY_LOOP] removing worktree $PARITY_WORKTREE"
    git -C "$PARITY_MAIN" worktree remove "$PARITY_WORKTREE"
  fi
fi

if [ "$purge" -eq 1 ]; then
  case "$PARITY_LOOP_DIR" in
    "$PARITY_LOOPS_DIR"/run/loop-*) rm -rf "$PARITY_LOOP_DIR" ;;
    *) echo "stop-loop: refusing to purge unexpected path $PARITY_LOOP_DIR" >&2; exit 1 ;;
  esac
  echo "[loop $PARITY_LOOP] purged runtime dir"
fi
