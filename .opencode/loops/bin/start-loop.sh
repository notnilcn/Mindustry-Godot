#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Prepare one parity loop and optionally start an opencode session in it.
#
# Usage:
#   start-loop.sh <loop-id>            # set up display/worktree/dirs, print env
#   start-loop.sh <loop-id> --run      # set up, then exec opencode in the worktree
#
# Loop 1 is the main checkout on the current display. Loops >= 2 get their own
# Xvfb display (:11, :12, ...), their own git worktree (branch parity/loop-N),
# and their own bridge port (6970, 6980, 6990, ...).
#
# Options:
#   --run             exec opencode in the loop worktree when setup succeeds
#   --no-auto         launch opencode without --auto (approval prompts enabled)
#   --reuse-bridge    do not fail when the loop's bridge port is already in
#                     use (launch-parallel.sh reuses a running editor)
#   --no-seed-builds  do not copy client/bin/rust and client/.godot into a new
#                     worktree (a from-scratch Rust/Godot build is expensive)
#
# Loop sessions launch with `opencode --auto` by default: permission asks are
# auto-approved inside the loop, while explicit deny rules (the evaluator's
# game-code/commit/push blocks) still hold. Set PARITY_OPENCODE_AUTO=0 or pass
# --no-auto to keep prompting.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"

usage() {
  sed -n '3,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

run_opencode=0
seed_builds=1
auto_approve="${PARITY_OPENCODE_AUTO:-1}"
reuse_bridge=0
id=""
for arg in "$@"; do
  case "$arg" in
    --run) run_opencode=1 ;;
    --auto) auto_approve=1 ;;
    --no-auto) auto_approve=0 ;;
    --reuse-bridge) reuse_bridge=1 ;;
    --no-seed-builds) seed_builds=0 ;;
    -h | --help)
      usage
      exit 0
      ;;
    '' | *[!0-9]*)
      echo "start-loop: unknown argument: $arg" >&2
      usage
      exit 2
      ;;
    *) id="$arg" ;;
  esac
done
parity_loop_vars "${id:-${PARITY_LOOP:-1}}"

mkdir -p "$PARITY_LOOP_DIR"/{logs,xdg-data,xdg-config,xdg-cache,home}

# 1) dedicated X display for loops >= 2
if [ "$PARITY_LOOP" -ge 2 ]; then
  x11_sock="/tmp/.X11-unix/X${PARITY_DISPLAY#:}"
  if [ -S "$x11_sock" ]; then
    echo "[loop $PARITY_LOOP] X display $PARITY_DISPLAY already up"
  else
    echo "[loop $PARITY_LOOP] starting Xvfb on $PARITY_DISPLAY (1280x720x24)"
    nohup Xvfb "$PARITY_DISPLAY" -screen 0 1280x720x24 -ac -nolisten tcp \
      >"$PARITY_LOOP_DIR/xvfb.log" 2>&1 &
    echo "$!" >"$PARITY_LOOP_DIR/xvfb.pid"
    for _ in $(seq 1 100); do
      [ -S "$x11_sock" ] && break
      sleep 0.1
    done
    if [ ! -S "$x11_sock" ]; then
      echo "[loop $PARITY_LOOP] ERROR: Xvfb failed on $PARITY_DISPLAY; see $PARITY_LOOP_DIR/xvfb.log" >&2
      exit 1
    fi
  fi
fi

# 2) git worktree for loops >= 2
if [ "$PARITY_LOOP" -ge 2 ]; then
  if git -C "$PARITY_MAIN" worktree list --porcelain | grep -qxF "worktree $PARITY_WORKTREE"; then
    echo "[loop $PARITY_LOOP] worktree present: $PARITY_WORKTREE"
  elif [ -e "$PARITY_WORKTREE" ]; then
    echo "[loop $PARITY_LOOP] ERROR: $PARITY_WORKTREE exists but is not a registered worktree" >&2
    exit 1
  elif git -C "$PARITY_MAIN" show-ref --verify --quiet "refs/heads/parity/loop-$PARITY_LOOP"; then
    git -C "$PARITY_MAIN" worktree add "$PARITY_WORKTREE" "parity/loop-$PARITY_LOOP"
  else
    git -C "$PARITY_MAIN" worktree add -b "parity/loop-$PARITY_LOOP" "$PARITY_WORKTREE" HEAD
  fi

  if [ "$seed_builds" -eq 1 ]; then
    for rel in client/bin/rust client/.godot; do
      if [ ! -e "$PARITY_WORKTREE/$rel" ] && [ -e "$PARITY_MAIN/$rel" ]; then
        echo "[loop $PARITY_LOOP] seeding $rel from the main checkout (first run only)"
        mkdir -p "$(dirname "$PARITY_WORKTREE/$rel")"
        cp -a "$PARITY_MAIN/$rel" "$PARITY_WORKTREE/$rel"
      fi
    done
  fi

  if [ ! -f "$PARITY_WORKTREE/.opencode/loops/lib/loop-vars.sh" ]; then
    echo "[loop $PARITY_LOOP] WARNING: worktree .opencode lacks loop tooling." >&2
    echo "  Commit the loop tooling on main (or rebase parity/loop-$PARITY_LOOP) so the" >&2
    echo "  session loads the loop-aware skills and commands." >&2
  fi
fi

# 3) bridge-port ownership for loops >= 2 (loop 1 may already hold 6970)
if [ "$PARITY_LOOP" -ge 2 ] && command -v ss >/dev/null 2>&1; then
  if ss -ltn 2>/dev/null | grep -qE ":${PARITY_BRIDGE_PORT}[[:space:]]"; then
    if [ "$reuse_bridge" -eq 1 ]; then
      echo "[loop $PARITY_LOOP] bridge port $PARITY_BRIDGE_PORT already in use; reusing the running editor"
    else
      echo "[loop $PARITY_LOOP] ERROR: bridge port $PARITY_BRIDGE_PORT is already in use." >&2
      echo "  A stale editor may be holding it; stop it or choose another loop id" >&2
      echo "  (or pass --reuse-bridge to use the editor that is already listening)." >&2
      exit 1
    fi
  fi
fi

# 4) session environment
export PATH="$PARITY_MCP_BIN:$PATH"
export DISPLAY="$PARITY_DISPLAY"

cat <<EOF
[parity-loop $PARITY_LOOP] ready
  display:      $PARITY_DISPLAY
  bridge port:  $PARITY_BRIDGE_PORT  (editor addon listens; MCP shim connects)
  worktree:     $PARITY_WORKTREE
  evals dir:    $PARITY_EVALS_DIR
  ledger:       $PARITY_LEDGER
  run prefix:   '${PARITY_RUN_PREFIX}'  (prefix run dirs with this)
  runtime dir:  $PARITY_LOOP_DIR
  MCP shims:    $PARITY_MCP_BIN  (prepended to PATH)
  auto-approve: $([ "$auto_approve" -eq 1 ] && echo "--auto (asks approved; explicit denies stay)" || echo "off (permission prompts enabled)")

  editor:  $script_dir/run-godot-editor.sh &
  java:    $script_dir/run-java.sh
EOF

if [ "$run_opencode" -eq 1 ]; then
  if [ "$auto_approve" -eq 1 ]; then
    echo "[parity-loop $PARITY_LOOP] starting opencode --auto in $PARITY_WORKTREE"
    cd "$PARITY_WORKTREE"
    exec opencode --auto
  fi
  echo "[parity-loop $PARITY_LOOP] starting opencode in $PARITY_WORKTREE"
  cd "$PARITY_WORKTREE"
  exec opencode
fi
