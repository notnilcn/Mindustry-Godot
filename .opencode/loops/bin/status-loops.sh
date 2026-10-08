#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Show every parity loop's Java X display (Xwayland or Xvfb), bridge port,
# compositor state, and worktree.
# Loop 1 is always listed; loops >= 2 are listed once they have a runtime dir.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"

parity_loop_vars 1 # populate PARITY_LOOPS_DIR for the runtime-dir scan

ids=(1)
for dir in "$PARITY_LOOPS_DIR"/run/loop-*; do
  [ -d "$dir" ] || continue
  id="${dir##*loop-}"
  [ "$id" = 1 ] && continue
  ids+=("$id")
done

printf '%-4s %-6s %-6s %-16s %-16s %-9s %s\n' LOOP DISPLAY BRIDGE JAVA-X11 WESTON LISTEN WORKTREE
for id in "${ids[@]}"; do
  parity_loop_vars "$id"
  dir="$PARITY_LOOP_DIR"
  display="$PARITY_DISPLAY"
  java="down"
  jpid=""
  [ -f "$dir/java-weston.pid" ] && jpid="$(cat "$dir/java-weston.pid" 2>/dev/null || true)"
  if [ -n "$jpid" ] && kill -0 "$jpid" 2>/dev/null \
    && tr '\0' ' ' <"/proc/$jpid/cmdline" 2>/dev/null | grep -q 'weston'; then
    xd="$(parity_xwayland_display)" || xd=""
    [ -n "$xd" ] && display="$xd"
    java="xway(pid $jpid)"
  elif [ -S "$PARITY_JAVA_WAYLAND_DIR/$PARITY_JAVA_WAYLAND_SOCKET" ]; then
    xd="$(parity_xwayland_display)" || xd=""
    if [ -n "$xd" ]; then
      display="$xd"
      java="xway"
    fi
  fi
  if [ "$java" = "down" ]; then
    if [ -f "$dir/xvfb.pid" ] && kill -0 "$(cat "$dir/xvfb.pid")" 2>/dev/null; then
      java="xvfb(pid $(cat "$dir/xvfb.pid"))"
    elif [ -S "/tmp/.X11-unix/X${display#:}" ]; then
      java="xvfb"
    fi
  fi
  weston="down"
  if [ -S "$PARITY_WAYLAND_DIR/$PARITY_WAYLAND_SOCKET" ]; then
    weston="up"
  fi
  if [ -f "$dir/weston.pid" ] && kill -0 "$(cat "$dir/weston.pid")" 2>/dev/null; then
    weston="pid $(cat "$dir/weston.pid")"
  fi
  listen="-"
  if command -v ss >/dev/null 2>&1 && ss -ltn 2>/dev/null | grep -qE ":${PARITY_BRIDGE_PORT}[[:space:]]"; then
    listen="yes"
  fi
  printf '%-4s %-6s %-6s %-16s %-16s %-9s %s\n' \
    "$id" "$display" "$PARITY_BRIDGE_PORT" "$java" "$weston" "$listen" "$PARITY_WORKTREE"
done
