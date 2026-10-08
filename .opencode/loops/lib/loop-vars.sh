#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Shared resource map for parallel parity loops (see ../README.md).
#
# Usage:
#   . "/path/to/.opencode/loops/lib/loop-vars.sh"
#   parity_loop_vars [loop-id]     # defaults to $PARITY_LOOP or 1
#
# Exported per loop:
#   PARITY_LOOP        numeric loop id (1-based)
#   PARITY_DISPLAY     X display backing the Java reference and X11-only
#                      tooling: the Xwayland display of the loop's dedicated
#                      java weston when available (GPU; resolved at startup),
#                      otherwise Xvfb (:10 or an inherited display for loop 1,
#                      :(9+N) for loop N). PARITY_JAVA_DISPLAY=x11 forces the
#                      Xvfb path; PARITY_DISPLAY_OVERRIDE pins a server.
#   PARITY_WAYLAND_SOCKET  Wayland socket (wayland-mind<N>) for the loop's
#                      headless weston compositor; hosts the Godot editor/game
#                      so rendering runs on the GPU (Xvfb has no DRI3)
#   PARITY_WAYLAND_DIR runtime dir owning that socket (weston-managed)
#   PARITY_JAVA_WAYLAND_SOCKET  Wayland socket (mind<N>-java) for the loop's
#                      dedicated java weston compositor with the Xwayland
#                      module; hosts the Java reference on the GPU
#   PARITY_JAVA_WAYLAND_DIR  runtime dir owning that socket
#   PARITY_BRIDGE_PORT open-godot-mcp bridge port (editor addon listens)
#   PARITY_DAP_PORT    documented DAP port for this loop (inert, Godot-owned)
#   PARITY_LSP_PORT    documented LSP port for this loop (inert, Godot-owned)
#   PARITY_WORKTREE    git worktree / checkout this loop runs in
#   PARITY_EVALS_DIR   shared evals directory (main checkout)
#   PARITY_LEDGER      shared findings.json (flock-protected)
#   PARITY_LOOP_DIR    per-loop runtime dir (pid/log/xdg/home)
#   PARITY_RUN_PREFIX  run-dir prefix ("" for loop 1, "l<N>-" otherwise)
#   PARITY_MCP_BIN     PATH shim dir (pins DISPLAY/bridge port for MCP)

_parity_loops_dir() {
  local src="${BASH_SOURCE[0]}"
  (cd "$(dirname "$src")/.." && pwd)
}

parity_loop_vars() {
  local id="${1:-${PARITY_LOOP:-1}}"
  case "$id" in
    '' | *[!0-9]*) echo "invalid loop id: '$id'" >&2; return 2 ;;
  esac
  [ "$id" -ge 1 ] || { echo "loop id must be >= 1" >&2; return 2; }

  local loops_dir main inherited default_display
  loops_dir="$(_parity_loops_dir)"
  main="$(cd "$loops_dir/../.." && pwd)"
  inherited="${DISPLAY:-:10}"
  inherited="${inherited%%.*}"

  PARITY_LOOP="$id"
  PARITY_LOOPS_DIR="$loops_dir"
  PARITY_MAIN="$main"
  if [ "$id" -eq 1 ]; then
    default_display="$inherited"
    PARITY_WORKTREE="$main"
    PARITY_RUN_PREFIX=""
  else
    default_display=":$((9 + id))"
    PARITY_WORKTREE="$(dirname "$main")/Mindustry-Godot-loop$id"
    PARITY_RUN_PREFIX="l${id}-"
  fi
  # Idempotent across repeated calls in one shell: only an explicit
  # PARITY_DISPLAY_OVERRIDE beats the per-loop default.
  PARITY_DISPLAY="${PARITY_DISPLAY_OVERRIDE:-$default_display}"
  PARITY_BRIDGE_PORT=$((6970 + 10 * (id - 1)))
  PARITY_DAP_PORT=$((6006 + 10 * (id - 1)))
  PARITY_LSP_PORT=$((6005 + 10 * (id - 1)))
  PARITY_EVALS_DIR="$main/.opencode/evals"
  PARITY_LEDGER="$PARITY_EVALS_DIR/findings.json"
  PARITY_LOOP_DIR="$loops_dir/run/loop-$id"
  PARITY_WAYLAND_SOCKET="wayland-mind$id"
  PARITY_WAYLAND_DIR="$PARITY_LOOP_DIR/wayland"
  # The Java reference gets its own headless weston (GL renderer + Xwayland):
  # Xvfb has no DRI3, so Mesa GLX there falls back to llvmpipe and the game
  # renders on the CPU. The Xwayland display number is discovered at startup.
  # Names stay short: runtime-dir + socket paths must fit the 108-byte UNIX
  # socket limit even in the loop-N worktrees.
  PARITY_JAVA_WAYLAND_SOCKET="mind$id-java"
  PARITY_JAVA_WAYLAND_DIR="$PARITY_LOOP_DIR/xwayland"
  PARITY_MCP_BIN="$loops_dir/mcp-bin"

  export PARITY_LOOP PARITY_LOOPS_DIR PARITY_MAIN PARITY_DISPLAY \
    PARITY_WORKTREE PARITY_RUN_PREFIX PARITY_BRIDGE_PORT PARITY_DAP_PORT \
    PARITY_LSP_PORT PARITY_EVALS_DIR PARITY_LEDGER PARITY_LOOP_DIR \
    PARITY_WAYLAND_SOCKET PARITY_WAYLAND_DIR PARITY_JAVA_WAYLAND_SOCKET \
    PARITY_JAVA_WAYLAND_DIR PARITY_MCP_BIN
}

parity_ensure_xvfb() {
  # Software fallback: start a managed Xvfb when the display is down. No-op
  # when a server already owns the display. Only local `:N` displays can be
  # created here; a host-qualified display is reported as unusable.
  local display="${PARITY_DISPLAY:-}"
  if [ -z "$display" ]; then
    echo "parity_ensure_display: PARITY_DISPLAY is not set" >&2
    return 2
  fi
  case "$display" in
    :[0-9]*) ;;
    *)
      echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: display $display is not a local :N display; cannot start Xvfb" >&2
      return 1
      ;;
  esac
  local num="${display#:}"
  case "$num" in
    '' | *[!0-9]*)
      echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: display $display is not a local :N display; cannot start Xvfb" >&2
      return 1
      ;;
  esac

  local sock="/tmp/.X11-unix/X$num"
  PARITY_DISPLAY_SERVER="xvfb"
  export PARITY_DISPLAY_SERVER
  [ -S "$sock" ] && return 0
  if ! command -v Xvfb >/dev/null 2>&1; then
    echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: $display is down and Xvfb is not installed" >&2
    return 1
  fi

  mkdir -p "$PARITY_LOOP_DIR"
  echo "[parity-loop ${PARITY_LOOP:-?}] starting Xvfb on $display (1280x720x24, software GL)" >&2
  nohup Xvfb "$display" -screen 0 1280x720x24 -ac -nolisten tcp \
    >"$PARITY_LOOP_DIR/xvfb.log" 2>&1 &
  echo "$!" >"$PARITY_LOOP_DIR/xvfb.pid"
  local _
  for _ in $(seq 1 100); do
    [ -S "$sock" ] && return 0
    sleep 0.1
  done
  echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: Xvfb failed on $display; see $PARITY_LOOP_DIR/xvfb.log" >&2
  return 1
}

parity_xwayland_display() {
  # Echo the display number the java weston's Xwayland server listens on, or
  # return 1 when it is unknown (compositor down or Xwayland failed to start).
  # Weston's xwayland module logs "xserver listening on display :N" once up.
  local log="$PARITY_LOOP_DIR/java-weston.log" display
  [ -f "$log" ] || return 1
  display="$(grep -o 'xserver listening on display :[0-9][0-9]*' "$log" 2>/dev/null \
    | tail -1 | grep -o ':[0-9][0-9]*')"
  [ -n "$display" ] || return 1
  printf '%s\n' "$display"
}

parity_ensure_java_wayland() {
  # Ensure this loop's dedicated java compositor is up: a headless weston with
  # the GL renderer and the Xwayland module, so the Java reference gets a
  # hardware-accelerated X display instead of Xvfb's llvmpipe. Returns 1 -
  # callers then fall back to Xvfb - when weston/Xwayland is unavailable,
  # PARITY_JAVA_DISPLAY=x11 is set, or a live compositor without Xwayland
  # already owns the socket.
  if [ "${PARITY_JAVA_DISPLAY:-xwayland}" = "x11" ]; then
    return 1
  fi
  local socket="${PARITY_JAVA_WAYLAND_SOCKET:-}"
  local dir="${PARITY_JAVA_WAYLAND_DIR:-}"
  if [ -z "$socket" ] || [ -z "$dir" ]; then
    echo "parity_ensure_java_wayland: PARITY_JAVA_WAYLAND_SOCKET/DIR not set" >&2
    return 2
  fi
  command -v weston >/dev/null 2>&1 || return 1
  command -v Xwayland >/dev/null 2>&1 || return 1

  local pidfile="$PARITY_LOOP_DIR/java-weston.pid"
  local log="$PARITY_LOOP_DIR/java-weston.log"
  local pid=""
  [ -f "$pidfile" ] && pid="$(cat "$pidfile" 2>/dev/null || true)"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null \
    && tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -q 'weston'; then
    [ -S "$dir/$socket" ] && [ -n "$(parity_xwayland_display)" ] && return 0
    # Live compositor without an Xwayland display: leave it alone.
    [ -S "$dir/$socket" ] && return 1
  fi
  # A killed compositor can leave a stale socket that blocks weston's bind.
  rm -f "$dir/$socket" "$dir/$socket.lock"

  mkdir -p "$dir"
  chmod 700 "$dir"
  echo "[parity-loop ${PARITY_LOOP:-?}] starting java weston on $socket (GL + Xwayland)" >&2
  XDG_RUNTIME_DIR="$dir" nohup weston \
    --backend=headless-backend.so --renderer=gl \
    --width=1280 --height=720 --socket="$socket" --no-config \
    --xwayland >"$log" 2>&1 &
  echo "$!" >"$pidfile"
  local _
  for _ in $(seq 1 100); do
    if [ -S "$dir/$socket" ] && [ -n "$(parity_xwayland_display)" ]; then
      return 0
    fi
    sleep 0.1
  done
  echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: java weston/Xwayland failed on $socket; see $log" >&2
  return 1
}

parity_ensure_display() {
  # Resolve and start this loop's Java X display. Preferred: the Xwayland
  # display of the loop's dedicated java weston (GPU; number discovered at
  # startup and exported as PARITY_DISPLAY). Falls back to Xvfb (software GL)
  # when weston/Xwayland is unavailable or PARITY_JAVA_DISPLAY=x11.
  # PARITY_DISPLAY_OVERRIDE pins an existing server; when that server is down,
  # Xvfb is started on the override display.
  case "${PARITY_DISPLAY_OVERRIDE:-}" in
    '') ;;
    :[0-9]*)
      PARITY_DISPLAY="${PARITY_DISPLAY_OVERRIDE%%.*}"
      export PARITY_DISPLAY
      if [ -S "/tmp/.X11-unix/X${PARITY_DISPLAY#:}" ]; then
        PARITY_DISPLAY_SERVER="x11"
        export PARITY_DISPLAY_SERVER
        return 0
      fi
      parity_ensure_xvfb
      return
      ;;
    *)
      echo "parity_ensure_display: PARITY_DISPLAY_OVERRIDE must be a local :N display" >&2
      return 1
      ;;
  esac

  if parity_ensure_java_wayland; then
    local display
    display="$(parity_xwayland_display)" || display=""
    if [ -n "$display" ]; then
      PARITY_DISPLAY="$display"
      PARITY_DISPLAY_SERVER="xwayland"
      export PARITY_DISPLAY PARITY_DISPLAY_SERVER
      return 0
    fi
  fi

  parity_ensure_xvfb
}

parity_ensure_wayland() {
  # Ensure this loop's headless weston compositor is up (GL renderer on the
  # GPU). Godot's editor and games run on this Wayland socket: Xvfb has no
  # DRI3, so Godot there falls back to llvmpipe software rendering and pins
  # the CPU. Returns 1 when weston is missing or the caller forces X11 with
  # PARITY_GODOT_DISPLAY=x11 (Godot then runs on PARITY_DISPLAY as before).
  if [ "${PARITY_GODOT_DISPLAY:-wayland}" = "x11" ]; then
    return 1
  fi
  local socket="${PARITY_WAYLAND_SOCKET:-}"
  local dir="${PARITY_WAYLAND_DIR:-}"
  if [ -z "$socket" ] || [ -z "$dir" ]; then
    echo "parity_ensure_wayland: PARITY_WAYLAND_SOCKET/DIR not set" >&2
    return 2
  fi

  # A live compositor: the pidfile process first, a listening socket as the
  # fallback for one started outside this function.
  local pid=""
  [ -f "$PARITY_LOOP_DIR/weston.pid" ] && pid="$(cat "$PARITY_LOOP_DIR/weston.pid" 2>/dev/null || true)"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null \
    && tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -q 'weston'; then
    [ -S "$dir/$socket" ] && return 0
  fi
  if [ -S "$dir/$socket" ] && command -v ss >/dev/null 2>&1 \
    && ss -xl 2>/dev/null | grep -qF "$dir/$socket"; then
    return 0
  fi
  # A killed compositor can leave a stale socket that blocks weston's bind.
  rm -f "$dir/$socket" "$dir/$socket.lock"

  if ! command -v weston >/dev/null 2>&1; then
    echo "[parity-loop ${PARITY_LOOP:-?}] weston is not installed; Godot would software-render on Xvfb" >&2
    return 1
  fi

  mkdir -p "$dir"
  chmod 700 "$dir"
  echo "[parity-loop ${PARITY_LOOP:-?}] starting weston headless on $socket (GL, 1280x720)" >&2
  XDG_RUNTIME_DIR="$dir" nohup weston \
    --backend=headless-backend.so --renderer=gl \
    --width=1280 --height=720 --socket="$socket" --no-config \
    >"$PARITY_LOOP_DIR/weston.log" 2>&1 &
  echo "$!" >"$PARITY_LOOP_DIR/weston.pid"
  local _
  for _ in $(seq 1 100); do
    [ -S "$dir/$socket" ] && return 0
    sleep 0.1
  done
  echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: weston failed on $socket; see $PARITY_LOOP_DIR/weston.log" >&2
  return 1
}

parity_loop_java() {
  # Echo the path to a usable JDK 17 java binary, or return 1.
  local candidate
  for candidate in "${JAVA17_HOME:-}" "${JAVA_HOME:-}" \
    /usr/lib/jvm/java-17-openjdk-amd64 /usr/lib/jvm/java-17-openjdk \
    /usr/lib/jvm/temurin-17-jdk-amd64; do
    if [ -n "$candidate" ] && [ -x "$candidate/bin/java" ] \
      && "$candidate/bin/java" -version 2>&1 | grep -q '"17\.'; then
      printf '%s\n' "$candidate/bin/java"
      return 0
    fi
  done
  if command -v java >/dev/null 2>&1 && java -version 2>&1 | grep -q '"17\.'; then
    readlink -f "$(command -v java)"
    return 0
  fi
  return 1
}
