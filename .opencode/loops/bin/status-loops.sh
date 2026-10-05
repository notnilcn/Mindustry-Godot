#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Show every parity loop's display, bridge port, Xvfb state, and worktree.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"

printf '%-4s %-6s %-6s %-10s %-9s %s\n' LOOP DISPLAY BRIDGE XVFB LISTEN WORKTREE
for dir in "$PARITY_LOOPS_DIR"/run/loop-*; do
  [ -d "$dir" ] || continue
  id="${dir##*loop-}"
  parity_loop_vars "$id"
  xvfb="-"
  if [ "$id" -ge 2 ]; then
    xvfb="down"
    if [ -f "$dir/xvfb.pid" ] && kill -0 "$(cat "$dir/xvfb.pid")" 2>/dev/null; then
      xvfb="pid $(cat "$dir/xvfb.pid")"
    fi
  fi
  listen="-"
  if command -v ss >/dev/null 2>&1 && ss -ltn 2>/dev/null | grep -qE ":${PARITY_BRIDGE_PORT}[[:space:]]"; then
    listen="yes"
  fi
  printf '%-4s %-6s %-6s %-10s %-9s %s\n' \
    "$id" "$PARITY_DISPLAY" "$PARITY_BRIDGE_PORT" "$xvfb" "$listen" "$PARITY_WORKTREE"
done
