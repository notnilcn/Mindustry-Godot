#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Start a parity session over SSH (or any shell with no usable DISPLAY).
#
# The current parity workflow (parity-eval skill, /goal + /loop prompts) assumes
# display :10 with the Godot editor on bridge 6970 and the Java reference
# launched by the session itself. Over SSH neither display nor editor exists, so
# this script:
#
#   1. ensures an X server on the workflow display (Xvfb, software GL; the
#      computer-mcp MCP entry in the global opencode config is pinned to
#      DISPLAY=:10.0, so :10 is the only drop-in display number),
#   2. optionally runs the Godot editor on a headless weston (Wayland/GL) when
#      --gpu-godot is passed; Java/tooling stay on :10,
#   3. launches the Godot editor on bridge port 6970 and waits until the bridge
#      port is listening,
#   4. prints the session environment and, with --run, execs opencode at the
#      workspace root (where the prompts' Mindustry-Godot/... paths resolve).
#
# The Java reference is NOT started here: the parity-eval skill §0 launches it
# per run (detached, with parity-java-home/), and a session must never have two
# clients racing for the same save state.
#
# Usage:
#   tools/parity-session.sh                 # bring up display + editor; print env
#   tools/parity-session.sh --run           # set up, then exec opencode
#   tools/parity-session.sh --stop          # stop what this script started
#   tools/parity-session.sh --help
#
# Options:
#   --run             exec opencode at the workspace root after setup
#   --no-auto         launch opencode without --auto (approval prompts enabled)
#   --no-editor       do not start the Godot editor
#   --reuse-bridge    do not fail when port 6970 is already listening (reuse
#                     the editor that is already up)
#   --gpu-godot       run the Godot editor on a headless weston socket
#                     (Wayland/GL) instead of Xvfb; needs weston + /dev/dri
#   --display :N      X display for the Java reference and X11 tooling
#                     (default :10; must match computer-mcp's DISPLAY)
#
# Environment:
#   PARITY_OPENCODE_AUTO=0   same as --no-auto
#   GODOT_BIN=<path>         Godot 4.7 binary (fallback: godot4, then godot)
#   PARITY_XVFB_GEOM=WxH     Xvfb geometry (default 1280x720)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_ROOT="$(dirname "$REPO_ROOT")"
RUNTIME="$REPO_ROOT/.opencode/parity-session"
RUNS="$REPO_ROOT/runs"
BRIDGE_PORT=6970

usage() {
  sed -n '3,46p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

run_opencode=0
auto_approve="${PARITY_OPENCODE_AUTO:-1}"
start_editor=1
reuse_bridge=0
gpu_godot=0
display=":10"
stop=0

for arg in "$@"; do
  case "$arg" in
    --run) run_opencode=1 ;;
    --auto) auto_approve=1 ;;
    --no-auto) auto_approve=0 ;;
    --no-editor) start_editor=0 ;;
    --reuse-bridge) reuse_bridge=1 ;;
    --gpu-godot) gpu_godot=1 ;;
    --stop) stop=1 ;;
    --display) ;;                    # value form: resolved in the second pass
    --display=*) display="${arg#--display=}" ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "parity-session: unknown argument: $arg (see --help)" >&2
      exit 2
      ;;
  esac
done

# Allow `--display :N` as two arguments.
args=("$@")
for i in "${!args[@]}"; do
  if [ "${args[$i]}" = "--display" ]; then
    next=$((i + 1))
    [ "$next" -lt "${#args[@]}" ] || { echo "parity-session: --display needs a value" >&2; exit 2; }
    display="${args[$next]}"
  fi
done

case "$display" in
  :[0-9]*) ;;
  *)
    echo "parity-session: display must be a local :N display (got '$display')" >&2
    exit 2
    ;;
esac
display_num="${display#:}"

mkdir -p "$RUNTIME" "$RUNS"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

log() { printf '[parity-session] %s\n' "$*"; }

# --- stop -------------------------------------------------------------------

stop_pidfile() {
  local name="$1" pidfile="$2" match="$3" pid=""
  [ -f "$pidfile" ] || return 0
  pid="$(cat "$pidfile" 2>/dev/null || true)"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null \
    && tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -q "$match"; then
    kill "$pid" 2>/dev/null || true
    log "stopped $name (pid $pid)"
  fi
  rm -f "$pidfile"
}

if [ "$stop" -eq 1 ]; then
  stop_pidfile editor "$RUNTIME/editor.pid" 'godot'
  stop_pidfile weston "$RUNTIME/weston.pid" 'weston'
  stop_pidfile xvfb "$RUNTIME/xvfb.pid" 'Xvfb'
  rm -f "$RUNTIME/wayland/wayland-parity" "$RUNTIME/wayland/wayland-parity.lock"
  log "session runtime preserved at $RUNTIME (logs kept)"
  exit 0
fi

# --- X display (:10) --------------------------------------------------------

x_up() {
  command -v xdpyinfo >/dev/null 2>&1 || return 0  # socket check below is enough
  xdpyinfo -display "$display" >/dev/null 2>&1
}

if x_up; then
  log "reusing existing X server on $display"
else
  if ! command -v Xvfb >/dev/null 2>&1; then
    echo "parity-session: $display is down and Xvfb is not installed." >&2
    echo "  Install Xvfb (or set PARITY_DISPLAY_OVERRIDE in the old workflow)." >&2
    exit 1
  fi
  log "starting Xvfb on $display (${PARITY_XVFB_GEOM:-1280x720}x24, software GL)"
  setsid nohup Xvfb "$display" -screen 0 "${PARITY_XVFB_GEOM:-1280x720}x24" -ac -nolisten tcp \
    >"$RUNTIME/xvfb.log" 2>&1 </dev/null &
  echo $! >"$RUNTIME/xvfb.pid"
  for _ in $(seq 1 100); do
    x_up && break
    sleep 0.1
  done
  if ! x_up; then
    echo "parity-session: Xvfb failed on $display; see $RUNTIME/xvfb.log" >&2
    exit 1
  fi
fi
export DISPLAY="$display"

# --- optional headless weston for the Godot editor --------------------------

wayland_env=()
if [ "$gpu_godot" -eq 1 ]; then
  if ! command -v weston >/dev/null 2>&1; then
    echo "parity-session: --gpu-godot needs weston; falling back to Xvfb for Godot" >&2
  else
    wdir="$RUNTIME/wayland"
    wsock="wayland-parity"
    mkdir -p "$wdir"
    chmod 700 "$wdir"
    if [ ! -S "$wdir/$wsock" ]; then
      rm -f "$wdir/$wsock" "$wdir/$wsock.lock"
      log "starting headless weston on $wsock (GL)"
      XDG_RUNTIME_DIR="$wdir" setsid nohup weston \
        --backend=headless-backend.so --renderer=gl \
        --width=1280 --height=720 --socket="$wsock" --no-config \
        >"$RUNTIME/weston.log" 2>&1 </dev/null &
      echo $! >"$RUNTIME/weston.pid"
      for _ in $(seq 1 100); do
        [ -S "$wdir/$wsock" ] && break
        sleep 0.1
      done
    fi
    if [ -S "$wdir/$wsock" ]; then
      wayland_env=("WAYLAND_DISPLAY=$wsock" "XDG_RUNTIME_DIR=$wdir")
      log "Godot editor will render on wayland ($wsock)"
    else
      echo "parity-session: weston failed; see $RUNTIME/weston.log (editor falls back to Xvfb)" >&2
    fi
  fi
fi

# --- Godot editor on bridge 6970 --------------------------------------------

port_listening() {
  ss -ltn 2>/dev/null | grep -qE ":${BRIDGE_PORT}[[:space:]]"
}

if port_listening; then
  log "bridge port $BRIDGE_PORT already listening; reusing the running editor"
elif [ "$start_editor" -eq 1 ]; then
  godot="${GODOT_BIN:-}"
  if [ -z "$godot" ]; then
    if command -v godot4 >/dev/null 2>&1; then godot="$(command -v godot4)"
    elif command -v godot >/dev/null 2>&1; then godot="$(command -v godot)"
    fi
  fi
  if [ -z "$godot" ]; then
    echo "parity-session: no Godot binary found; set GODOT_BIN" >&2
    exit 1
  fi
  [ -f "$REPO_ROOT/client/project.godot" ] \
    || { echo "parity-session: $REPO_ROOT/client is not a Godot project" >&2; exit 1; }

  log "starting Godot editor (bridge $BRIDGE_PORT, display $display)"
  editor_env=(env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET DISPLAY="$display")
  editor_args=(--editor --path client)
  if [ "${#wayland_env[@]}" -gt 0 ]; then
    editor_env=(env "${wayland_env[@]}" DISPLAY="$display")
    editor_args=(--editor --path client --display-driver wayland)
  fi
  (
    cd "$REPO_ROOT"
    OPEN_GODOT_MCP_PORT="$BRIDGE_PORT" setsid nohup \
      "${editor_env[@]}" "$godot" "${editor_args[@]}" \
      >>"$RUNS/editor-console.log" 2>&1 </dev/null &
    echo $! >"$RUNTIME/editor.pid"
  )
  for _ in $(seq 1 120); do
    port_listening && break
    sleep 0.5
  done
  if ! port_listening; then
    echo "parity-session: bridge port $BRIDGE_PORT never came up; see $RUNS/editor-console.log" >&2
    exit 1
  fi
  log "editor bridge listening on $BRIDGE_PORT (pid $(cat "$RUNTIME/editor.pid" 2>/dev/null || echo '?'))"
fi

# --- sanity checks -----------------------------------------------------------

if [ ! -f "$WORKSPACE_ROOT/Mindustry/desktop/build/libs/Mindustry.jar" ]; then
  echo "parity-session: warning: Java reference jar not found at" >&2
  echo "  $WORKSPACE_ROOT/Mindustry/desktop/build/libs/Mindustry.jar" >&2
fi

conf="$HOME/.config/opencode/opencode.jsonc"
if [ -f "$conf" ]; then
  conf_display="$(grep -o '"DISPLAY"[[:space:]]*:[[:space:]]*"[^"]*"' "$conf" \
    | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)"
  if [ -n "$conf_display" ] && [ "$conf_display" != "$display" ] && [ "$conf_display" != "$display.0" ]; then
    echo "parity-session: warning: computer-mcp in $conf uses DISPLAY=$conf_display," >&2
    echo "  but this session is $display; the skill assumes :10 everywhere." >&2
  fi
fi

# --- summary -----------------------------------------------------------------

java_display="Xvfb ($display, software GL)"
godot_display="x11 ($display, software)"
if [ "${#wayland_env[@]}" -gt 0 ]; then
  godot_display="wayland (${wayland_env[0]#WAYLAND_DISPLAY=}, weston/GL)"
fi

cat <<EOF
[parity-session] ready
  workspace:    $WORKSPACE_ROOT        (run the /goal or /loop prompt from here)
  port repo:    $REPO_ROOT
  display:      $display   (Java + X11 tooling; $java_display)
  godot:        $godot_display
  bridge port:  $BRIDGE_PORT   (open-godot-mcp connects here)
  editor log:   ${RUNS#$REPO_ROOT/}/editor-console.log
  ledger:       ${REPO_ROOT#$WORKSPACE_ROOT/}/parity-ledger.json
  chains:       ${REPO_ROOT#$WORKSPACE_ROOT/}/chains/
  run prefix:   '${REPO_ROOT#$WORKSPACE_ROOT/}/runs/'
  prompts:      ${REPO_ROOT#$WORKSPACE_ROOT/}/WORKFLOW_PROMPTS.md
  runtime dir:  ${RUNTIME#$REPO_ROOT/}   (logs/pids; --stop to tear down)
  auto-approve: $([ "$auto_approve" -eq 1 ] && echo "--auto (asks approved; explicit denies stay)" || echo "off (permission prompts enabled)")

  Next: the session launches the Java client itself (parity-eval skill §0);
  paste the /goal or /loop prompt from WORKFLOW_PROMPTS.md.
EOF

if [ "$run_opencode" -eq 1 ]; then
  if ! command -v opencode >/dev/null 2>&1; then
    echo "parity-session: opencode not found on PATH" >&2
    exit 1
  fi
  cd "$WORKSPACE_ROOT"
  if [ "$auto_approve" -eq 1 ]; then
    log "starting opencode --auto in $WORKSPACE_ROOT"
    exec opencode --auto
  fi
  log "starting opencode in $WORKSPACE_ROOT"
  exec opencode
fi
