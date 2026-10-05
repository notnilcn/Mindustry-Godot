#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Stop the autonomous runner processes launched by launch-parallel.sh.
# Editors, Xvfb displays, worktrees, and logs are left in place; use
# stop-loop.sh <id> for the loop infrastructure (and --remove-worktree to
# remove the checkout).
#
# Usage: parallel-stop.sh [loop-id ...]
#        parallel-stop.sh --all
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars 1

ids=()
stop_all=0
for arg in "$@"; do
  case "$arg" in
    --all) stop_all=1 ;;
    -h | --help)
      sed -n '3,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    '' | *[!0-9]*)
      echo "parallel-stop: unknown argument: $arg" >&2
      exit 2
      ;;
    *) ids+=("$arg") ;;
  esac
done

if [ "$stop_all" -eq 1 ] || [ "${#ids[@]}" -eq 0 ]; then
  for dir in "$PARITY_LOOPS_DIR"/run/loop-*; do
    [ -f "$dir/plan.json" ] || continue
    ids+=("${dir##*loop-}")
  done
fi
if [ "${#ids[@]}" -eq 0 ]; then
  echo "parallel-stop: no runners found"
  exit 0
fi

for id in "${ids[@]}"; do
  parity_loop_vars "$id"
  pid_file="$PARITY_LOOP_DIR/agent.pid"
  if [ ! -f "$pid_file" ]; then
    echo "[loop $id] no runner pid file"
    continue
  fi
  pid="$(cat "$pid_file")"
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid"
    for _ in $(seq 1 20); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.5
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
      echo "[loop $id] runner $pid killed (SIGKILL)"
    else
      echo "[loop $id] runner $pid stopped"
    fi
  else
    echo "[loop $id] runner not running"
  fi
done
