#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Start a parity session over SSH (or any shell with no usable DISPLAY).
#
# A session is one isolated client environment: an X display (Xvfb, software
# GL; session 1 stays on :10, the display the shared reference frames were
# captured on), an editor bridge port, optionally
# a headless weston for the Godot editor, and, for N >= 2, a git worktree with
# its own build caches and Java data dir, so several SSH sessions can run
# agents in parallel on this host.
#
# Session 1 is the main checkout (`Mindustry-Godot/`) on display :10 and
# bridge 6970. `--session N` (N >= 2) prepares a parallel session in the
# sibling worktree `../Mindustry-Godot-sessionN` on display :(9+N) and bridge
# 6970+10(N-1), with session-local MCP wiring (PATH shims + `opencode.json` in
# the worktree). The ledger (`parity-ledger.json`) and `runs/` stay shared with
# the main checkout through symlinks, so claims and reference frames are
# visible to every session; everything else is per-session.
#
# The Java reference is NOT started here: the parity-eval skill §0 launches it
# per run (detached, with parity-java-home/), and a session must never have two
# clients racing for the same save state.
#
# Usage:
#   tools/parity-session.sh                    # session 1: display + editor; print env
#   tools/parity-session.sh --run              # set up, then exec opencode
#   tools/parity-session.sh --session 2 --run  # parallel session in a worktree
#   tools/parity-session.sh --session 2 --stop # stop that session's display/editor
#   tools/parity-session.sh --help
#
# Options:
#   --session N       run session N (default 1: main checkout, display :10,
#                     bridge 6970). N >= 2 derives display :(9+N), bridge
#                     6970+10(N-1), worktree ../Mindustry-Godot-sessionN,
#                     seeded build caches + parity-java-home, and session-local
#                     MCP wiring
#   --run             exec opencode after setup (in the session worktree)
#   --no-auto         launch opencode without --auto (approval prompts enabled)
#   --no-editor       do not start the Godot editor
#   --reuse-bridge    do not fail when the session's bridge port is already
#                     listening (reuse the editor that is already up)
#   --no-seed-builds  session N >= 2 only: do not copy client/bin/rust and
#                     client/.godot from the main checkout (a from-scratch
#                     Rust/Godot build is expensive)
#   --gpu-godot       run the Godot editor on a headless weston socket
#                     (Wayland/GL) instead of Xvfb; needs weston + /dev/dri
#   --display :N      X display for the Java reference and X11 tooling
#                     (default :10, or :(9+N) for --session N >= 2)
#   --stop            stop what this session started
#
# Environment:
#   PARITY_OPENCODE_AUTO=0   same as --no-auto
#   GODOT_BIN=<path>         Godot 4.7 binary (fallback: godot4, then godot)
#   PARITY_XVFB_GEOM=WxH     Xvfb geometry (default 1280x720)
#   PARITY_BRIDGE_PORT=N     override the derived editor bridge port
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_ROOT="$(dirname "$REPO_ROOT")"
RUNS=""
RUNTIME=""
SHIM_DIR=""

# The main checkout owns the shared ledger/runs; a session worktree resolves
# back to it through the git common dir.
git_common="$(git -C "$REPO_ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
if [ -n "$git_common" ]; then
  MAIN_REPO="$(dirname "$git_common")"
else
  MAIN_REPO="$REPO_ROOT"
fi

log() { printf '[parity-session] %s\n' "$*"; }

usage() {
  sed -n '3,56p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

die() {
  echo "parity-session: $*" >&2
  exit 2
}

run_opencode=0
auto_approve="${PARITY_OPENCODE_AUTO:-1}"
start_editor=1
reuse_bridge=0
gpu_godot=0
stop=0
seed_builds=1
session=1
display=":10"
display_explicit=0

while [ $# -gt 0 ]; do
  case "$1" in
    --run) run_opencode=1 ;;
    --auto) auto_approve=1 ;;
    --no-auto) auto_approve=0 ;;
    --no-editor) start_editor=0 ;;
    --reuse-bridge) reuse_bridge=1 ;;
    --no-seed-builds) seed_builds=0 ;;
    --gpu-godot) gpu_godot=1 ;;
    --stop) stop=1 ;;
    --session)
      shift
      [ $# -gt 0 ] || die "--session needs a value"
      session="$1"
      ;;
    --session=*) session="${1#--session=}" ;;
    --display)
      shift
      [ $# -gt 0 ] || die "--display needs a value"
      display="$1"
      display_explicit=1
      ;;
    --display=*) display="${1#--display=}"; display_explicit=1 ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1 (see --help)"
      ;;
  esac
  shift
done

case "$session" in
  '' | *[!0-9]*) die "session must be a positive integer (got '$session')" ;;
esac
[ "$session" -ge 1 ] || die "session must be >= 1 (got $session)"

if [ "$session" -ge 2 ] && [ "$display_explicit" -eq 0 ]; then
  display=":$((9 + session))"
fi
case "$display" in
  :[0-9]*) ;;
  *) die "display must be a local :N display (got '$display')" ;;
esac
display_num="${display#:}"

BRIDGE_PORT="${PARITY_BRIDGE_PORT:-$((6970 + 10 * (session - 1)))}"

if [ "$session" -ge 2 ]; then
  SESSION_REPO="$WORKSPACE_ROOT/Mindustry-Godot-session$session"
  RUNTIME="$MAIN_REPO/.opencode/parity-session/session-$session"
  SHIM_DIR="$RUNTIME/bin"
  WAYLAND_SOCKET="wayland-parity-$session"
  EDITOR_LOG="$RUNTIME/editor-console.log"
else
  SESSION_REPO="$REPO_ROOT"
  RUNTIME="$REPO_ROOT/.opencode/parity-session"
  WAYLAND_SOCKET="wayland-parity"
  EDITOR_LOG=""
fi
WAYLAND_DIR="$RUNTIME/wayland"
RUNS="$SESSION_REPO/runs"

if [ "$session" -eq 1 ] && [ "$REPO_ROOT" != "$MAIN_REPO" ]; then
  die "this is a session worktree; run with --session N (or from the main checkout)"
fi

mkdir -p "$RUNTIME"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

# --- shared artifacts (session N >= 2) ---------------------------------------

# link_shared <path> <relative target>: point a worktree path at the main
# checkout's file. A freshly checked-out tracked file that matches main is
# replaced; a local copy that differs is preserved under the runtime dir.
link_shared() {
  local path="$1" target="$2" base
  base="$(basename "$path")"
  if [ -L "$path" ]; then
    [ "$(readlink "$path")" = "$target" ] || ln -sfn "$target" "$path"
    return 0
  fi
  if [ -e "$path" ]; then
    if [ -e "$MAIN_REPO/$base" ] && cmp -s "$path" "$MAIN_REPO/$base"; then
      rm -f "$path"
      ln -s "$target" "$path"
    else
      log "WARNING: $path differs from main; keeping it at $RUNTIME/shared-$base.local and linking to main"
      mv "$path" "$RUNTIME/shared-$base.local"
      ln -s "$target" "$path"
    fi
  else
    ln -s "$target" "$path"
  fi
}

session_prepare() {
  # 1) git worktree on its own branch.
  if git -C "$MAIN_REPO" worktree list --porcelain | grep -qxF "worktree $SESSION_REPO"; then
    log "session worktree present: $SESSION_REPO"
  elif [ -e "$SESSION_REPO" ]; then
    echo "parity-session: $SESSION_REPO exists but is not a registered worktree" >&2
    exit 1
  else
    if git -C "$MAIN_REPO" show-ref --verify --quiet "refs/heads/parity/session-$session"; then
      log "creating worktree $SESSION_REPO (branch parity/session-$session)"
      git -C "$MAIN_REPO" worktree add "$SESSION_REPO" "parity/session-$session"
    else
      log "creating worktree $SESSION_REPO (new branch parity/session-$session from $(git -C "$MAIN_REPO" rev-parse --short HEAD))"
      git -C "$MAIN_REPO" worktree add -b "parity/session-$session" "$SESSION_REPO" HEAD
    fi
  fi

  # 2) build caches so the first editor launch does not redo the Rust build.
  if [ "$seed_builds" -eq 1 ]; then
    local rel
    for rel in client/bin/rust client/.godot; do
      if [ ! -e "$SESSION_REPO/$rel" ] && [ -e "$MAIN_REPO/$rel" ]; then
        log "seeding $rel from the main checkout (first run; --no-seed-builds skips)"
        mkdir -p "$(dirname "$SESSION_REPO/$rel")"
        cp -a --reflink=auto "$MAIN_REPO/$rel" "$SESSION_REPO/$rel"
      fi
    done
  fi

  # 3) isolated Java data dir (settings/saves) seeded from the main checkout,
  #    so two sessions never share one data dir.
  if [ ! -e "$SESSION_REPO/parity-java-home" ] && [ -e "$MAIN_REPO/parity-java-home" ]; then
    log "seeding parity-java-home from the main checkout"
    cp -a --reflink=auto "$MAIN_REPO/parity-java-home" "$SESSION_REPO/parity-java-home"
  fi

  # 4) shared queue and run store: one ledger, one flock, one set of reference
  #    frames across sessions. `runs/` is gitignored, so a symlink is invisible
  #    to git; `parity-ledger.json` is tracked, so hide the symlink from
  #    status/add with skip-worktree.
  link_shared "$SESSION_REPO/parity-ledger.json" "../Mindustry-Godot/parity-ledger.json"
  link_shared "$SESSION_REPO/parity-ledger.json.lock" "../Mindustry-Godot/parity-ledger.json.lock"
  [ -e "$SESSION_REPO/runs" ] || ln -s "../Mindustry-Godot/runs" "$SESSION_REPO/runs"
  if git -C "$SESSION_REPO" ls-files --error-unmatch parity-ledger.json >/dev/null 2>&1; then
    git -C "$SESSION_REPO" update-index --skip-worktree -- parity-ledger.json
  fi

  # 5) session-local Godot MCP wiring. The opencode project config overrides
  #    the global open-godot-mcp entry (project config wins over global); the
  #    PATH shim is a fallback for processes that inherit this shell's
  #    environment. The Java leg needs no wiring: the shell helpers read the
  #    session's exported DISPLAY.
  cat >"$SESSION_REPO/opencode.json" <<EOF
{
  "\$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "open-godot-mcp": {
        "type": "local",
        "command": ["open-godot-mcp", "--bridge-port", "$BRIDGE_PORT"],
        "environment": { "DISPLAY": "$display.0" }
      }
    }
  }
}
EOF
  local exclude
  exclude="$(git -C "$SESSION_REPO" rev-parse --git-path info/exclude 2>/dev/null || true)"
  if [ -n "$exclude" ]; then
    mkdir -p "$(dirname "$exclude")"
    for pat in /opencode.json /runs /parity-ledger.json.lock; do
      grep -qxF "$pat" "$exclude" 2>/dev/null || echo "$pat" >>"$exclude"
    done
  fi

  mkdir -p "$SHIM_DIR" "$RUNTIME"/{xdg-data,xdg-config,xdg-cache}
  cat >"$SHIM_DIR/open-godot-mcp" <<EOF
#!/usr/bin/env bash
# Session $session MCP shim (generated by tools/parity-session.sh): pin
# open-godot-mcp to bridge port $BRIDGE_PORT and display $display.
set -euo pipefail
self_dir="\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
clean_path="\$(printf '%s' "\$PATH" | tr ':' '\n' | grep -vFx "\$self_dir" | paste -sd: -)"
real="\${OPEN_GODOT_MCP_REAL:-\$(PATH="\$clean_path" command -v open-godot-mcp || true)}"
if [ -z "\$real" ]; then
  echo "parity-session shim: open-godot-mcp not found on PATH" >&2
  exit 127
fi
exec env DISPLAY="$display" OPEN_GODOT_MCP_PORT="$BRIDGE_PORT" \\
  XDG_DATA_HOME="$RUNTIME/xdg-data" XDG_CONFIG_HOME="$RUNTIME/xdg-config" \\
  XDG_CACHE_HOME="$RUNTIME/xdg-cache" "\$real" "\$@"
EOF
  chmod +x "$SHIM_DIR/open-godot-mcp"
}

# --- stop --------------------------------------------------------------------

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
  rm -f "$WAYLAND_DIR/$WAYLAND_SOCKET" "$WAYLAND_DIR/$WAYLAND_SOCKET.lock"
  log "session $session stopped (runtime preserved at $RUNTIME)"
  exit 0
fi

# --- session setup -----------------------------------------------------------

if [ "$session" -ge 2 ]; then
  session_prepare
fi
mkdir -p "$RUNS"

# --- X display ----------------------------------------------------------------

x_up() {
  command -v xdpyinfo >/dev/null 2>&1 || return 0  # socket check below is enough
  xdpyinfo -display "$display" >/dev/null 2>&1
}

if x_up; then
  log "reusing existing X server on $display (session $session)"
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

# --- optional headless weston for the Godot editor ---------------------------

wayland_env=()
if [ "$gpu_godot" -eq 1 ]; then
  if ! command -v weston >/dev/null 2>&1; then
    echo "parity-session: --gpu-godot needs weston; falling back to Xvfb for Godot" >&2
  else
    mkdir -p "$WAYLAND_DIR"
    chmod 700 "$WAYLAND_DIR"
    if [ ! -S "$WAYLAND_DIR/$WAYLAND_SOCKET" ]; then
      rm -f "$WAYLAND_DIR/$WAYLAND_SOCKET" "$WAYLAND_DIR/$WAYLAND_SOCKET.lock"
      log "starting headless weston on $WAYLAND_SOCKET (GL)"
      XDG_RUNTIME_DIR="$WAYLAND_DIR" setsid nohup weston \
        --backend=headless-backend.so --renderer=gl \
        --width=1280 --height=720 --socket="$WAYLAND_SOCKET" --no-config \
        >"$RUNTIME/weston.log" 2>&1 </dev/null &
      echo $! >"$RUNTIME/weston.pid"
      for _ in $(seq 1 100); do
        [ -S "$WAYLAND_DIR/$WAYLAND_SOCKET" ] && break
        sleep 0.1
      done
    fi
    if [ -S "$WAYLAND_DIR/$WAYLAND_SOCKET" ]; then
      wayland_env=("WAYLAND_DISPLAY=$WAYLAND_SOCKET" "XDG_RUNTIME_DIR=$WAYLAND_DIR")
      log "Godot editor will render on wayland ($WAYLAND_SOCKET)"
    else
      echo "parity-session: weston failed; see $RUNTIME/weston.log (editor falls back to Xvfb)" >&2
    fi
  fi
fi

# --- Godot editor on the session's bridge port --------------------------------

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
  [ -f "$SESSION_REPO/client/project.godot" ] \
    || { echo "parity-session: $SESSION_REPO/client is not a Godot project" >&2; exit 1; }

  editor_env=(env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET DISPLAY="$display")
  editor_args=(--editor --path "$SESSION_REPO/client")
  if [ "${#wayland_env[@]}" -gt 0 ]; then
    editor_env=(env "${wayland_env[@]}" DISPLAY="$display")
    editor_args=(--editor --path "$SESSION_REPO/client" --display-driver wayland)
  fi
  if [ "$session" -ge 2 ]; then
    editor_env+=(
      "XDG_DATA_HOME=$RUNTIME/xdg-data"
      "XDG_CONFIG_HOME=$RUNTIME/xdg-config"
      "XDG_CACHE_HOME=$RUNTIME/xdg-cache"
    )
  fi
  if [ -z "$EDITOR_LOG" ]; then
    EDITOR_LOG="$RUNS/editor-console.log"
  fi

  log "starting Godot editor (session $session, bridge $BRIDGE_PORT, display $display)"
  (
    cd "$SESSION_REPO"
    OPEN_GODOT_MCP_PORT="$BRIDGE_PORT" setsid nohup \
      "${editor_env[@]}" "$godot" "${editor_args[@]}" \
      >>"$EDITOR_LOG" 2>&1 </dev/null &
    echo $! >"$RUNTIME/editor.pid"
  )
  for _ in $(seq 1 120); do
    port_listening && break
    sleep 0.5
  done
  if ! port_listening; then
    echo "parity-session: bridge port $BRIDGE_PORT never came up; see $EDITOR_LOG" >&2
    exit 1
  fi
  log "editor bridge listening on $BRIDGE_PORT (pid $(cat "$RUNTIME/editor.pid" 2>/dev/null || echo '?'))"
fi

# --- sanity checks -------------------------------------------------------------

if [ ! -f "$WORKSPACE_ROOT/Mindustry/desktop/build/libs/Mindustry.jar" ]; then
  echo "parity-session: warning: Java reference jar not found at" >&2
  echo "  $WORKSPACE_ROOT/Mindustry/desktop/build/libs/Mindustry.jar" >&2
fi

if [ "$session" -ge 2 ]; then
  log "session wiring: open-godot-mcp pinned to bridge $BRIDGE_PORT; Java helpers take DISPLAY=$display per call"
fi

parity_py="${PARITY_PY:-${PARITY_VENV:-$HOME/.local/share/parity-eval/venv}/bin/python}"
if [ ! -x "$parity_py" ]; then
  echo "parity-session: warning: Java-leg helper venv missing ($parity_py);" >&2
  echo "  run .opencode/skills/parity-eval/scripts/bootstrap.sh" >&2
fi

# --- summary -------------------------------------------------------------------

java_display="Xvfb ($display, software GL)"
godot_display="x11 ($display, software)"
if [ "${#wayland_env[@]}" -gt 0 ]; then
  godot_display="wayland (${wayland_env[0]#WAYLAND_DISPLAY=}, weston/GL)"
fi

if [ -z "$EDITOR_LOG" ]; then
  EDITOR_LOG="$RUNS/editor-console.log"
fi

cat <<EOF
[parity-session $session] ready
  workspace:    $WORKSPACE_ROOT        (run the /goal or /loop prompt from here)
  session repo: $SESSION_REPO$([ "$session" -ge 2 ] && echo "    (worktree, branch parity/session-$session)" || echo "    (main checkout)")
  display:      $display   (Java + X11 tooling; $java_display)
  godot:        $godot_display
  bridge port:  $BRIDGE_PORT   (open-godot-mcp connects here)
  editor log:   ${EDITOR_LOG#"$REPO_ROOT"/}
  runtime dir:  $RUNTIME   (logs/pids; --stop to tear down)
  auto-approve: $([ "$auto_approve" -eq 1 ] && echo "--auto (asks approved; explicit denies stay)" || echo "off (permission prompts enabled)")
EOF
if [ "$session" -ge 2 ]; then
  cat <<EOF
  ledger:       shared with main (${MAIN_REPO#"$WORKSPACE_ROOT"/}/parity-ledger.json, symlinked)
  runs:         shared with main (${MAIN_REPO#"$WORKSPACE_ROOT"/}/runs, symlinked)
  java home:    $SESSION_REPO/parity-java-home   (isolated copy)
  MCP shims:    $SHIM_DIR   (open-godot-mcp; prepended to PATH by --run)
  MCP config:   $SESSION_REPO/opencode.json   (Godot bridge override, git-excluded)
  stop:         session worktree script: tools/parity-session.sh --session $session --stop
EOF
else
  cat <<EOF
  ledger:       ${MAIN_REPO#"$WORKSPACE_ROOT"/}/parity-ledger.json   (items embed path/reproduce/steps)
  run prefix:   '${MAIN_REPO#"$WORKSPACE_ROOT"/}/runs/'
  prompts:      ${MAIN_REPO#"$WORKSPACE_ROOT"/}/WORKFLOW_PROMPTS.md
EOF
fi
cat <<EOF

  Next: ${SESSION_REPO#"$WORKSPACE_ROOT"/} launches the Java client itself (parity-eval skill §0);
  paste the /goal or /loop prompt from WORKFLOW_PROMPTS.md$([ "$session" -ge 2 ] && echo " with the root prefix dropped" || echo "").
EOF

if [ "$run_opencode" -eq 1 ]; then
  if ! command -v opencode >/dev/null 2>&1; then
    echo "parity-session: opencode not found on PATH" >&2
    exit 1
  fi
  if [ "$session" -ge 2 ]; then
    export PATH="$SHIM_DIR:$PATH"
    export DISPLAY="$display"
    cd "$SESSION_REPO"
  else
    cd "$WORKSPACE_ROOT"
  fi
  if [ "$auto_approve" -eq 1 ]; then
    log "starting opencode --auto in $(pwd)"
    exec opencode --auto
  fi
  log "starting opencode in $(pwd)"
  exec opencode
fi
