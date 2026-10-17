#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# twin.sh — sourced Java+Godot twin-walk helpers (J10 T1).
#
# Encodes the 2026-10-16 ad-hoc round-J walk (docs/J-round-twin-verification.md)
# as reusable bash functions, so the twin walk is reproducible from a script
# instead of hand-typed shell. Source it from a bash shell in the checkout you
# are driving:
#
#   source tools/twin.sh 5          # session 5: display :14, bridge 7010
#   source tools/twin.sh            # session inferred from the worktree name
#                                   # (<repo>-sessionN -> N, else 1)
#
# Environment overrides (set before sourcing):
#   TWIN_RUN=<dir>            run dir for evidence (default runs/<stamp>-s<N>-twin)
#   TWIN_DISPLAY=:14          X display for the Java leg
#   OPEN_GODOT_MCP_PORT       bridge port for the Godot leg (default 6970+10(N-1))
#   TWIN_JHOME=<dir>          Java data dir (default <checkout>/parity-java-home)
#   TWIN_JAVA=<java>          JVM binary (default the JDK 17 path the skill uses)
#   TWIN_JAR=<jar>            Java reference jar (default ../Mindustry/.../Mindustry.jar)
#   TWIN_SKILL_SCRIPTS=<dir>  parity-eval helper scripts dir (auto-resolved)
#   TWIN_PARITY_PY=<python>   parity-eval helper venv python (mss/PIL/python-xlib/pynput)
#   TWIN_POINTER_PY=<python>  python with pynput for the real-pointer warp
#   TWIN_GMCP=<file>          gmcp.py used for godot_* calls (default tools/gmcp.py)
#   TWIN_GODOT_USER=<dir>     Godot user:// dir (default derived from the session)
#   TWIN_RAISE_JAVA=0         do not restack the Java window before Java actions
#   TWIN_RAISE_GODOT=0        do not restack the Godot editor before GP
#   TWIN_JW_HOVER=0           JW clicks without the hover-first dwell (L08 T5)
#   TWIN_JW_HOVER_S=0.4       JW hover dwell in seconds before the click
#   TWIN_G_POINTER=0          GC/GW do not move the real pointer first (L08 T5)
#   TWIN_GODOT_VIEW_W/H       Godot design viewport (1152x648) for GC/GW's
#                             viewport -> game-window pointer mapping
#
# Java leg (window-relative coordinates; the origin is re-read from
# `xwininfo -name <title>` on EVERY call — never from window_ctl.py's printout,
# which is stale after a move; see java-leg-learnings.md 2026-10-16):
#   JW x y [button]     click (left|right|middle); hovers the target ~0.4 s
#                       first (TWIN_JW_HOVER=0 to skip): the first click after
#                       the pointer crossed another window is otherwise
#                       sometimes swallowed (java-leg-learnings 2026-10-17)
#   JM x y              move the pointer (hover)
#   JD x1 y1 x2 y2 [button] [ms]   drag
#   JK key NAME [ms] / JK keydown NAME / JK keyup NAME
#   JS name             raise + capture the window -> $RUN/java/name.png
#   JT 'text' [@Key ...]  type into Java by XSendEvent (tools/java_xkeys.py):
#                       XTEST keys never reach Java on the session Xvfb
#                       (the X focus stays on Godot); focus the field with JW
#                       first. `JT @BackSpace`, `JT @Escape`, `JT @space`.
#
# Godot leg (viewport coordinates by default):
#   GC x y [button] [coords]   dialog click — one godot_input `sequence`
#   GW x y [button] [coords]   world click — separate press/release calls
#                              GC/GW first warp the REAL pointer to the target
#                              (GP) so hover state matches what a user sees;
#                              TWIN_G_POINTER=0 opts out (L08 T5)
#   GK NAME [down|up]          key by Godot name ("Escape"); default tap
#   GT 'text'                  type into the focused Control (gmcp --text: one
#                              unicode key press+release per char; L01 T4).
#                              Focus the field with GC first; GK Backspace clears
#   GS name                    game screenshot -> $RUN/godot/name.png
#   GE 'expr'                  single-statement eval (gmcp --eval)
#   GP x y                     warp the REAL pointer over the embedded game
#                              (hover UI; origin from the game window's xwininfo)
#
# Compare / crop:
#   SIDE name                  frame_diff.py JSON + heatmap + side-by-side -> $RUN/diff/
#   ZOOM name x0 y0 x1 y1      paint a 2x crop pair -> $RUN/diff/name-zoom.png
#
# Lifecycle:
#   twin_env                   print the resolved session/run/paths
#   twin_launch                back up settings.bin, snapshot saves, launch Java
#                              (setsid), gate on "Total time to load", normalize
#                              (1152x648), place clear of the Godot game window
#   twin_place_java [x y]      (re)place the Java window after the game window appears
#   twin_stop                  stop Java, move session-created saves out of both
#                              profiles into $RUN/{java,godot}-session-saves/,
#                              restore settings.bin

_twin_self="${BASH_SOURCE[0]}"
TWIN_ROOT="$(cd "$(dirname "$_twin_self")/.." && pwd)"

# --- session / display / bridge ----------------------------------------------

if [ -n "${1:-}" ]; then
  TWIN_SESSION="$1"
elif [ -n "${TWIN_SESSION:-}" ]; then
  :
elif [[ "$(basename "$TWIN_ROOT")" =~ -session([0-9]+)$ ]]; then
  TWIN_SESSION="${BASH_REMATCH[1]}"
else
  TWIN_SESSION=1
fi
TWIN_DISPLAY="${TWIN_DISPLAY:-$([ "$TWIN_SESSION" -eq 1 ] && echo ":10" || echo ":$((9 + TWIN_SESSION))")}"
OPEN_GODOT_MCP_PORT="${OPEN_GODOT_MCP_PORT:-$((6970 + 10 * (TWIN_SESSION - 1)))}"
export OPEN_GODOT_MCP_PORT

_twin_common="$(git -C "$TWIN_ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
TWIN_MAIN="${_twin_common:+$(dirname "$_twin_common")}"
TWIN_MAIN="${TWIN_MAIN:-$TWIN_ROOT}"

TWIN_RUN="${TWIN_RUN:-$TWIN_ROOT/runs/$(date +%Y%m%d-%H%M%S)-s${TWIN_SESSION}-twin}"
mkdir -p "$TWIN_RUN/java" "$TWIN_RUN/godot" "$TWIN_RUN/diff"

# --- tools and binaries ------------------------------------------------------

TWIN_PARITY_PY="${TWIN_PARITY_PY:-${PARITY_PY:-$HOME/.local/share/parity-eval/venv/bin/python}}"

_twin_ws="$(dirname "$TWIN_MAIN")"
TWIN_SKILL_SCRIPTS="${TWIN_SKILL_SCRIPTS:-}"
if [ -z "$TWIN_SKILL_SCRIPTS" ]; then
  for _twin_cand in \
    "$TWIN_MAIN/.opencode/skills/parity-eval/scripts" \
    "$_twin_ws/.claude/skills/parity-eval/scripts" \
    "$HOME/.claude/skills/parity-eval/scripts"; do
    if [ -d "$_twin_cand" ]; then
      TWIN_SKILL_SCRIPTS="$_twin_cand"
      break
    fi
  done
fi

TWIN_JAR="${TWIN_JAR:-$_twin_ws/Mindustry/desktop/build/libs/Mindustry.jar}"
TWIN_JAVA="${TWIN_JAVA:-/usr/lib/jvm/java-17-openjdk-amd64/bin/java}"
TWIN_JHOME="${TWIN_JHOME:-$TWIN_ROOT/parity-java-home}"
TWIN_GMCP="${TWIN_GMCP:-$TWIN_ROOT/tools/gmcp.py}"

# twin_pointer.py needs pynput: prefer a system python3 that has it, else the
# parity-eval venv (which does).
TWIN_POINTER_PY="${TWIN_POINTER_PY:-}"
if [ -z "$TWIN_POINTER_PY" ]; then
  if DISPLAY="$TWIN_DISPLAY" python3 -c 'import pynput' >/dev/null 2>&1; then
    TWIN_POINTER_PY=python3
  else
    TWIN_POINTER_PY="$TWIN_PARITY_PY"
  fi
fi

# Window titles. The embedded game is the editor's child window; the editor
# itself is the top-level "…(DEBUG)" window that owns it.
TWIN_JAVA_TITLE="${TWIN_JAVA_TITLE:-Mindustry}"
TWIN_GODOT_GAME_TITLE="${TWIN_GODOT_GAME_TITLE:-Mindustry-Godot (DEBUG)}"
TWIN_GODOT_EDITOR_TITLE="${TWIN_GODOT_EDITOR_TITLE:-Mindustry-Godot (DEBUG)}"

twin_warn() { printf 'twin: %s\n' "$*" >&2; }

twin_env() {
  cat <<EOF
twin session:   $TWIN_SESSION   (checkout $TWIN_ROOT)
display:        $TWIN_DISPLAY
godot bridge:   $OPEN_GODOT_MCP_PORT   (gmcp: $TWIN_GMCP)
run dir:        $TWIN_RUN
java home:      $TWIN_JHOME
java jar:       $TWIN_JAR
parity py:      $TWIN_PARITY_PY
pointer py:     $TWIN_POINTER_PY
skill scripts:  $TWIN_SKILL_SCRIPTS
EOF
}

# --- X helpers ---------------------------------------------------------------

# twin_win_geometry_by_id 0xID -> "x y w h" (absolute upper-left + size), or rc 1.
twin_win_geometry_by_id() {
  DISPLAY="$TWIN_DISPLAY" timeout 10 xwininfo -id "$1" 2>/dev/null | awk -F: '
    /Absolute upper-left X/ {x=$2+0}
    /Absolute upper-left Y/ {y=$2+0}
    /^  Width/              {w=$2+0}
    /^  Height/             {h=$2+0}
    END { if (w>0 && h>0) { printf "%d %d %d %d\n", x, y, w, h } else { exit 1 } }'
}

# twin_win_geometry TITLE -> "x y w h" (absolute upper-left + size), or rc 1.
# Reads xwininfo directly; window_ctl.py's printed geometry can be stale after
# a move (java-leg-learnings.md 2026-10-16).
twin_win_geometry() {
  local title="$1" out id
  out="$(DISPLAY="$TWIN_DISPLAY" timeout 10 xwininfo -name "$title" 2>/dev/null || true)"
  id="$(printf '%s\n' "$out" | awk '/Window id:/ {print $4; exit}')"
  if [ -z "$id" ]; then
    id="$(DISPLAY="$TWIN_DISPLAY" timeout 10 xwininfo -root -tree 2>/dev/null \
      | awk -v pat="\"$title\"" 'index($0, pat) {print $1; exit}')"
  fi
  [ -n "$id" ] || return 1
  twin_win_geometry_by_id "$id"
}

# The game process for THIS checkout (its cmdline carries --path <root>/client).
twin_godot_game_pid() {
  local pid
  for pid in $(pgrep -f '[r]emote-debug' 2>/dev/null || true); do
    [ -r "/proc/$pid/cmdline" ] || continue
    if tr '\0' '\n' <"/proc/$pid/cmdline" 2>/dev/null | grep -qxF -- "$TWIN_ROOT/client"; then
      echo "$pid"
      return 0
    fi
  done
  return 1
}

# The engine window of the game pid: the window the X pointer actually lands
# on. An embedded run exposes TWO windows with the same WM_NAME — the host
# window (WM_CLASS "Godot_Editor", the --wid target) and the real engine
# window (WM_CLASS "Godot_Engine"). `xwininfo -name` can resolve to the host
# window, which is why the origin must come from here.
twin_godot_game_window_id() {
  local pid
  pid="$(twin_godot_game_pid || true)"
  [ -n "$pid" ] || return 1
  DISPLAY="$TWIN_DISPLAY" "$TWIN_PARITY_PY" - "$pid" <<'PY'
import sys

from Xlib import display

pid = int(sys.argv[1])
d = display.Display()
root = d.screen().root
atom = d.intern_atom("_NET_WM_PID")
best = None


def walk(win):
    global best
    for c in win.query_tree().children:
        try:
            prop = c.get_full_property(atom, 0)
            if prop is not None and prop.value.tolist() and prop.value.tolist()[0] == pid:
                cls = c.get_wm_class() or ("", "")
                g = c.get_geometry()
                pref = 1 if cls[0] == "Godot_Engine" else 0
                score = (pref, g.width * g.height)
                if best is None or score > best[0]:
                    best = (score, c.id)
        except Exception:
            pass
        walk(c)


walk(root)
if best is None:
    sys.exit(1)
print(hex(best[1]))
PY
}

# twin_godot_game_window -> "x y w h" of the engine window, with a title-search
# fallback for setups without a separate engine window.
twin_godot_game_window() {
  local id
  id="$(twin_godot_game_window_id || true)"
  if [ -n "$id" ]; then
    twin_win_geometry_by_id "$id" && return 0
  fi
  twin_win_geometry "$TWIN_GODOT_GAME_TITLE"
}

twin_screen_size() { # -> "W H"
  DISPLAY="$TWIN_DISPLAY" timeout 10 xdpyinfo 2>/dev/null \
    | awk '/dimensions:/ {gsub(/x/," "); print $2, $3; exit}'
}

twin_java_pid() { # pid of the JVM on THIS display + home, or rc 1
  local pid
  for pid in $(pgrep -f 'Mindustry[.]jar' 2>/dev/null || true); do
    [ -r "/proc/$pid/environ" ] || continue
    if tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null | grep -qxF "DISPLAY=$TWIN_DISPLAY" \
      && tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null | grep -qxF "HOME=$TWIN_JHOME"; then
      echo "$pid"
      return 0
    fi
  done
  return 1
}

twin_java_click() { # twin_java_click x y [button] — screen coordinates
  DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/java_input.py" click "$1" "$2" "${3:-left}"
}

twin_win_raise_id() { # 0xID — restack a window above its siblings
  DISPLAY="$TWIN_DISPLAY" "$TWIN_PARITY_PY" - "$1" <<'PY'
import sys

from Xlib import display, X

wid = int(sys.argv[1], 0)
d = display.Display()
win = d.create_resource_object("window", wid)
win.configure(stack_mode=X.Above)
d.sync()
print("raised", hex(wid))
PY
}

twin_godot_raise() {
  local id
  id="$(twin_godot_game_window_id || true)"
  if [ -n "$id" ]; then
    twin_win_raise_id "$id" >/dev/null 2>&1 || true
  else
    DISPLAY="$TWIN_DISPLAY" timeout 10 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/window_ctl.py" \
      raise "$TWIN_GODOT_EDITOR_TITLE" >/dev/null 2>&1 || true
  fi
}

twin_java_raise() {
  DISPLAY="$TWIN_DISPLAY" timeout 10 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/window_ctl.py" raise "$TWIN_JAVA_TITLE" >/dev/null 2>&1 || true
}

# --- Java leg ----------------------------------------------------------------

JW() { # JW x y [button] — window-relative click, hover-first by default
  local x="${1:?JW x y [button]}" y="${2:?}" btn="${3:-left}" g
  g="$(twin_win_geometry "$TWIN_JAVA_TITLE")" || {
    twin_warn "JW: no '$TWIN_JAVA_TITLE' window on $TWIN_DISPLAY"
    return 1
  }
  [ "${TWIN_RAISE_JAVA:-1}" = 1 ] && twin_java_raise
  set -- $g
  # L08 T5: hover the target before clicking (round L's JWC). After the pointer
  # crossed another window the first click is otherwise sometimes swallowed
  # (java-leg-learnings.md 2026-10-17).
  if [ "${TWIN_JW_HOVER:-1}" = 1 ]; then
    DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/java_input.py" \
      hover "$(($1 + x))" "$(($2 + y))" >/dev/null || true
    sleep "${TWIN_JW_HOVER_S:-0.4}"
  fi
  twin_java_click "$(($1 + x))" "$(($2 + y))" "$btn"
}

JM() { # JM x y — window-relative pointer move (hover)
  local x="${1:?JM x y}" y="${2:?}" g
  g="$(twin_win_geometry "$TWIN_JAVA_TITLE")" || {
    twin_warn "JM: no '$TWIN_JAVA_TITLE' window on $TWIN_DISPLAY"
    return 1
  }
  [ "${TWIN_RAISE_JAVA:-1}" = 1 ] && twin_java_raise
  set -- $g
  DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/java_input.py" \
    hover "$(($1 + x))" "$(($2 + y))"
}

JD() { # JD x1 y1 x2 y2 [button] [ms] — window-relative drag
  local x1="${1:?JD x1 y1 x2 y2 [button] [ms]}" y1="$2" x2="$3" y2="$4" btn="${5:-left}" ms="${6:-300}" g
  g="$(twin_win_geometry "$TWIN_JAVA_TITLE")" || {
    twin_warn "JD: no '$TWIN_JAVA_TITLE' window on $TWIN_DISPLAY"
    return 1
  }
  [ "${TWIN_RAISE_JAVA:-1}" = 1 ] && twin_java_raise
  set -- $g
  DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/java_input.py" \
    drag "$(($1 + x1))" "$(($2 + y1))" "$(($1 + x2))" "$(($2 + y2))" "$btn" "$ms"
}

JK() { # JK key NAME [hold_ms] | JK keydown NAME | JK keyup NAME
  local mode=key name hold=""
  case "${1:-}" in
    key|keydown|keyup) mode="$1"; name="${2:?JK $1 NAME [hold_ms]}"; hold="${3:-}" ;;
    *) name="${1:?JK NAME [hold_ms] or JK key|keydown|keyup NAME}" ;;
  esac
  if [ "${TWIN_RAISE_JAVA:-1}" = 1 ]; then
    twin_java_raise
  fi
  if [ "$mode" = key ] && [ -n "$hold" ]; then
    DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/java_input.py" key "$name" "$hold"
  else
    DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/java_input.py" "$mode" "$name"
  fi
}

JS() { # JS name — raise + capture into $RUN/java/name.png
  local name="${1:?JS name}"
  mkdir -p "$TWIN_RUN/java"
  twin_java_raise
  sleep 0.2
  DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/capture_screen.py" \
    --window-title "$TWIN_JAVA_TITLE" --output "$TWIN_RUN/java/$name.png"
}

JT() { # JT 'text'|@Keysym ... — keys to the Java window by XSendEvent
  DISPLAY="$TWIN_DISPLAY" timeout 30 "$TWIN_PARITY_PY" -I "$TWIN_ROOT/tools/java_xkeys.py" \
    --title "$TWIN_JAVA_TITLE" "${@:?JT text|@Keysym ...}"
}

# --- Godot leg ---------------------------------------------------------------

twin_gmcp() {
  timeout "${TWIN_GMCP_TIMEOUT:-90}" python3 "$TWIN_GMCP" --timeout "${TWIN_GMCP_CALL_TIMEOUT:-80}" "$@"
}

# twin_g_pointer x y coords — L08 T5: warp the REAL pointer to a GC/GW target
# (viewport coords scale onto the game window; window coords map 1:1). Hover UI
# reads the real pointer, so without this the injected click lands while hover
# state still reflects wherever the pointer was. Never fails the click.
twin_g_pointer() {
  [ "${TWIN_G_POINTER:-1}" = 1 ] || return 0
  local x="$1" y="$2" coords="$3" g gw gh vw="${TWIN_GODOT_VIEW_W:-1152}" vh="${TWIN_GODOT_VIEW_H:-648}"
  if [ "$coords" = viewport ] && g="$(twin_godot_game_window 2>/dev/null)"; then
    set -- $g
    gw="$3"
    gh="$4"
    x="$(awk -v a="$x" -v w="$gw" -v v="$vw" 'BEGIN { printf "%d", a * w / v + 0.5 }')"
    y="$(awk -v a="$y" -v h="$gh" -v v="$vh" 'BEGIN { printf "%d", a * h / v + 0.5 }')"
  fi
  GP "${x%.*}" "${y%.*}" >/dev/null || twin_warn "GC/GW: real-pointer warp failed (click still injected)"
}

GC() { # GC x y [button] [coords] — dialog click = one godot_input sequence
  local x="${1:?GC x y [button] [coords]}" y="$2" btn="${3:-MOUSE_BUTTON_LEFT}" coords="${4:-viewport}"
  twin_g_pointer "$x" "$y" "$coords"
  twin_gmcp godot_input \
    "{\"action\":\"sequence\",\"params\":{\"frame_delay\":1,\"steps\":[{\"type\":\"mouse_motion\",\"params\":{\"position\":{\"x\":$x,\"y\":$y},\"coords\":\"$coords\"}},{\"type\":\"mouse_button\",\"params\":{\"button\":\"$btn\",\"position\":{\"x\":$x,\"y\":$y},\"coords\":\"$coords\",\"pressed\":true}},{\"type\":\"mouse_button\",\"params\":{\"button\":\"$btn\",\"position\":{\"x\":$x,\"y\":$y},\"coords\":\"$coords\",\"pressed\":false}}]}}"
}

GW() { # GW x y [button] [coords] — world click = separate press/release calls
  local x="${1:?GW x y [button] [coords]}" y="$2" btn="${3:-MOUSE_BUTTON_LEFT}" coords="${4:-viewport}"
  twin_g_pointer "$x" "$y" "$coords"
  twin_gmcp \
    godot_input "{\"action\":\"mouse_button\",\"params\":{\"button\":\"$btn\",\"position\":{\"x\":$x,\"y\":$y},\"coords\":\"$coords\",\"pressed\":true}}" \
    godot_input "{\"action\":\"mouse_button\",\"params\":{\"button\":\"$btn\",\"position\":{\"x\":$x,\"y\":$y},\"coords\":\"$coords\",\"pressed\":false}}"
}

GK() { # GK NAME [down|up] — Godot key by name; default tap (press+release)
  local name="${1:?GK NAME [down|up]}" mode="${2:-tap}"
  case "$mode" in
    down) twin_gmcp godot_input "{\"action\":\"key\",\"params\":{\"key\":\"$name\",\"pressed\":true}}" ;;
    up)   twin_gmcp godot_input "{\"action\":\"key\",\"params\":{\"key\":\"$name\",\"pressed\":false}}" ;;
    tap)  twin_gmcp \
            godot_input "{\"action\":\"key\",\"params\":{\"key\":\"$name\",\"pressed\":true}}" \
            godot_input "{\"action\":\"key\",\"params\":{\"key\":\"$name\",\"pressed\":false}}" ;;
    *) twin_warn "GK: mode must be tap|down|up (got '$mode')"; return 1 ;;
  esac
}

GT() { # GT 'text' — type into the GUI focus owner (real key events, L01 T4)
  twin_gmcp --text "${1:?GT 'text'}"
}

GS() { # GS name — game screenshot into $RUN/godot/name.png
  local name="${1:?GS name}"
  mkdir -p "$TWIN_RUN/godot"
  twin_gmcp --screenshot "$TWIN_RUN/godot/$name.png"
}

GE() { # GE 'expr' — single-statement eval (gmcp --eval)
  twin_gmcp --eval "${1:?GE 'expr'}"
}

GP() { # GP x y — move the REAL pointer over the embedded Godot game window
  local x="${1:?GP x y}" y="${2:?}" g extra=()
  g="$(twin_godot_game_window)" || {
    twin_warn "GP: no Godot game window on $TWIN_DISPLAY (is the game playing?)"
    return 1
  }
  if [ "${TWIN_RAISE_GODOT:-1}" = 1 ]; then
    twin_godot_raise
    sleep 0.2
  fi
  set -- $g
  [ "${TWIN_JIGGLE:-1}" = 1 ] && extra+=(--jiggle)
  DISPLAY="$TWIN_DISPLAY" timeout 15 "$TWIN_POINTER_PY" "$TWIN_ROOT/tools/twin_pointer.py" \
    move "$(($1 + x))" "$(($2 + y))" "${extra[@]}"
}

# --- compare / crop ----------------------------------------------------------

SIDE() { # SIDE name — frame_diff.py with JSON, heatmap and side-by-side
  local name="${1:?SIDE name}"
  mkdir -p "$TWIN_RUN/diff"
  DISPLAY="$TWIN_DISPLAY" timeout 60 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/frame_diff.py" \
    --java "$TWIN_RUN/java/$name.png" \
    --godot "$TWIN_RUN/godot/$name.png" \
    --out-json "$TWIN_RUN/diff/$name.json" \
    --out-heatmap "$TWIN_RUN/diff/$name-heatmap.png" \
    --out-side-by-side "$TWIN_RUN/diff/$name-side.png"
}

ZOOM() { # ZOOM name x0 y0 x1 y1 — 2x crop pair (Java left, Godot right)
  local name="${1:?ZOOM name x0 y0 x1 y1}" x0="$2" y0="$3" x1="$4" y1="$5"
  mkdir -p "$TWIN_RUN/diff"
  DISPLAY="$TWIN_DISPLAY" timeout 60 "$TWIN_PARITY_PY" - "$name" "$x0" "$y0" "$x1" "$y1" "$TWIN_RUN" <<'PY'
import json, sys
from PIL import Image

name, x0, y0, x1, y1, run = sys.argv[1:7]
box = (int(x0), int(y0), int(x1), int(y1))
java = Image.open(f"{run}/java/{name}.png").convert("RGB").crop(box)
godot = Image.open(f"{run}/godot/{name}.png").convert("RGB").crop(box)
scale = 2


def up(img):
    return img.resize((img.width * scale, img.height * scale), Image.Resampling.NEAREST)


j2, g2 = up(java), up(godot)
canvas = Image.new("RGB", (j2.width * 2 + 8, j2.height), (24, 24, 24))
canvas.paste(j2, (0, 0))
canvas.paste(g2, (j2.width + 8, 0))
out = f"{run}/diff/{name}-zoom.png"
canvas.save(out)
print(json.dumps({"zoom": out, "box": box, "scale": scale,
                  "java_crop": list(java.size), "godot_crop": list(godot.size)}))
PY
}

# --- launch / teardown -------------------------------------------------------

twin_godot_user_dir() { # Godot user:// dir for this session (saves teardown)
  if [ -n "${TWIN_GODOT_USER:-}" ]; then
    echo "$TWIN_GODOT_USER"
  elif [ "$TWIN_SESSION" -ge 2 ]; then
    echo "$TWIN_MAIN/.opencode/parity-session/session-$TWIN_SESSION/xdg-data/godot/app_userdata/Mindustry-Godot"
  else
    echo "$HOME/.local/share/godot/app_userdata/Mindustry-Godot"
  fi
}

_twin_save_files() { # every save file in both profiles, one path per line
  local dir
  for dir in "$TWIN_JHOME/.local/share/Mindustry/saves" "$(twin_godot_user_dir)/saves"; do
    [ -d "$dir" ] || continue
    find "$dir" -maxdepth 1 -type f \
      \( -name '*.msav' -o -name '*.msav-backup.msav' -o -name '*.msav.tmp' \) 2>/dev/null
  done
}

twin_snapshot_saves() {
  _twin_save_files | sort >"$TWIN_RUN/saves-before.txt"
  printf 'twin: snapshotted %s save file(s) -> %s\n' \
    "$(wc -l <"$TWIN_RUN/saves-before.txt")" "$TWIN_RUN/saves-before.txt"
}

twin_move_session_saves() {
  local before="$TWIN_RUN/saves-before.txt" dir label d f moved=0 out
  [ -f "$before" ] || return 0
  for dir in "java:$TWIN_JHOME/.local/share/Mindustry/saves" "godot:$(twin_godot_user_dir)/saves"; do
    label="${dir%%:*}"
    d="${dir#*:}"
    [ -d "$d" ] || continue
    out="$TWIN_RUN/$label-session-saves"
    while IFS= read -r f; do
      [ -f "$f" ] || continue
      if grep -qxF "$f" "$before"; then
        continue # existed before the run (may have been rewritten in place; left alone)
      fi
      mkdir -p "$out"
      mv -f "$f" "$out/" && moved=$((moved + 1))
    done < <(find "$d" -maxdepth 1 -type f \
      \( -name '*.msav' -o -name '*.msav-backup.msav' -o -name '*.msav.tmp' \) 2>/dev/null)
  done
  printf 'twin: moved %d session-created save file(s) out of the profiles\n' "$moved"
}

twin_java_move_to() { # x y — host-level move; the client may apply an offset,
  # so the caller must read the truth back with twin_win_geometry afterwards.
  local x="$1" y="$2"
  DISPLAY="$TWIN_DISPLAY" timeout 10 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/window_ctl.py" \
    move "$TWIN_JAVA_TITLE" "$x" "$y" >/dev/null 2>&1 || true
  sleep 0.4
  DISPLAY="$TWIN_DISPLAY" timeout 10 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/window_ctl.py" \
    move "$TWIN_JAVA_TITLE" "$x" "$y" >/dev/null 2>&1 || true
  sleep 0.4
}

twin_place_java() { # twin_place_java [x y] — place clear of the Godot game window
  local jg gg sw sh jx jy jw jh gx gy gw gh
  jg="$(twin_win_geometry "$TWIN_JAVA_TITLE")" || {
    twin_warn "twin_place_java: no '$TWIN_JAVA_TITLE' window on $TWIN_DISPLAY"
    return 1
  }
  read -r jx jy jw jh <<<"$jg"
  read -r sw sh < <(twin_screen_size)
  sw="${sw:-1280}"
  sh="${sh:-720}"
  gg="$(twin_godot_game_window || true)"

  local bx=0 by=0 bscore=-1 cx cy ix iy area dx dy dist score cand
  if [ "$#" -eq 2 ]; then
    bx="$1"
    by="$2"
  elif [ -z "$gg" ]; then
    twin_warn "twin_place_java: no Godot game window found; placing at 0 0 (re-run twin_place_java once the game is playing)"
    bx=0
    by=0
  else
    read -r gx gy gw gh <<<"$gg"
    for cand in \
      "0 0" \
      "$((sw - jw)) 0" \
      "0 $((sh - jh))" \
      "$((sw - jw)) $((sh - jh))" \
      "$(((sw - jw) / 2)) $(((sh - jh) / 2))" \
      "$((sw - jw)) $(((sh - jh) / 2))" \
      "0 $(((sh - jh) / 2))"; do
      set -- $cand
      cx="$1"
      cy="$2"
      area=0
      ix=$(((cx + jw < gx + gw ? cx + jw : gx + gw) - (cx > gx ? cx : gx)))
      iy=$(((cy + jh < gy + gh ? cy + jh : gy + gh) - (cy > gy ? cy : gy)))
      if [ "$ix" -gt 0 ] && [ "$iy" -gt 0 ]; then
        area=$((ix * iy))
      fi
      dx=$((cx + jw / 2 - (gx + gw / 2)))
      dy=$((cy + jh / 2 - (gy + gh / 2)))
      dist=$((dx * dx + dy * dy))
      score=$((area * 1000000 + dist))
      if [ "$bscore" -lt 0 ] || [ "$score" -lt "$bscore" ]; then
        bscore="$score"
        bx="$cx"
        by="$cy"
      fi
    done
  fi

  twin_java_move_to "$bx" "$by"
  jg="$(twin_win_geometry "$TWIN_JAVA_TITLE")"
  read -r jx jy jw jh <<<"$jg"
  if [ -n "$gg" ]; then
    read -r gx gy gw gh <<<"$gg"
    ix=$(((jx + jw < gx + gw ? jx + jw : gx + gw) - (jx > gx ? jx : gx)))
    iy=$(((jy + jh < gy + gh ? jy + jh : gy + gh) - (jy > gy ? jy : gy)))
    if [ "$ix" -gt 0 ] && [ "$iy" -gt 0 ]; then
      twin_warn "twin_place_java: Java overlap with the game window is $((ix))x$((iy)) px (display too small for a clear placement); JW-family actions raise the Java window and GP raises the editor, so actions still land"
    else
      printf 'twin: Java window at %s,%s clears the game window at %s,%s (%sx%s)\n' \
        "$jx" "$jy" "$gx" "$gy" "$gw" "$gh"
    fi
  else
    printf 'twin: Java window at %s,%s (game window not found)\n' "$jx" "$jy"
  fi
  printf '%s\n' "$jg" >"$TWIN_RUN/java-geometry.txt"
}

twin_launch() { # the launch preamble: backup, launch, gate, normalize, place, snapshot
  mkdir -p "$TWIN_RUN/java"
  if [ ! -f "$TWIN_JAR" ]; then
    twin_warn "twin_launch: jar not found: $TWIN_JAR"
    return 1
  fi
  if twin_java_pid >/dev/null 2>&1; then
    twin_warn "twin_launch: a Java client is already running on $TWIN_DISPLAY (pid $(twin_java_pid)); not launching a second one"
  else
    local settings="$TWIN_JHOME/.local/share/Mindustry/settings.bin" log="$TWIN_RUN/java-console.log"
    if [ -f "$settings" ]; then
      cp -f "$settings" "$TWIN_RUN/settings.bin.bak"
      printf 'twin: backed up settings.bin -> %s\n' "$TWIN_RUN/settings.bin.bak"
    else
      twin_warn "twin_launch: no settings.bin yet at $settings (fresh profile; nothing to back up)"
    fi
    twin_snapshot_saves
    : >"$log"
    setsid env DISPLAY="$TWIN_DISPLAY" HOME="$TWIN_JHOME" \
      XDG_DATA_HOME="$TWIN_JHOME/.local/share" \
      XDG_CONFIG_HOME="$TWIN_JHOME/.config" \
      XDG_CACHE_HOME="$TWIN_JHOME/.cache" \
      "$TWIN_JAVA" -jar "$TWIN_JAR" >>"$log" 2>&1 </dev/null &
    disown 2>/dev/null || true
    printf 'twin: Java launching; gating on "Total time to load" in %s\n' "$log"
    if ! timeout 180 bash -c 'until grep -q "Total time to load" "$1"; do sleep 2; done' _ "$log"; then
      twin_warn "twin_launch: launch gate timed out; see $log"
      return 1
    fi
  fi
  local pid
  pid="$(twin_java_pid || true)"
  printf 'twin: Java pid %s (log %s/java-console.log)\n' "${pid:-?}" "$TWIN_RUN"
  # Normalize the shared frame size (a maximized window ignores resize).
  DISPLAY="$TWIN_DISPLAY" timeout 10 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/window_ctl.py" \
    unmaximize "$TWIN_JAVA_TITLE" >/dev/null 2>&1 || true
  sleep 0.2
  DISPLAY="$TWIN_DISPLAY" timeout 10 "$TWIN_PARITY_PY" "$TWIN_SKILL_SCRIPTS/window_ctl.py" \
    resize "$TWIN_JAVA_TITLE" 1152 648 >/dev/null 2>&1 || true
  sleep 0.3
  twin_place_java || true
}

twin_stop() { # stop Java; move session-created saves out; restore settings.bin
  local pid
  pid="$(twin_java_pid || true)"
  if [ -n "$pid" ]; then
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.5
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
      sleep 0.5
    fi
    printf 'twin: Java pid %s stopped\n' "$pid"
  else
    twin_warn "twin_stop: no Java client found on $TWIN_DISPLAY"
  fi
  twin_move_session_saves
  if [ -f "$TWIN_RUN/settings.bin.bak" ]; then
    cp -f "$TWIN_RUN/settings.bin.bak" "$TWIN_JHOME/.local/share/Mindustry/settings.bin"
    printf 'twin: settings.bin restored from %s\n' "$TWIN_RUN/settings.bin.bak"
  fi
}

# Sourced, not executed.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  cat >&2 <<'EOF'
twin.sh is meant to be sourced:
  source tools/twin.sh [session]
then use JW/JM/JD/JK/JS (Java), GC/GW/GK/GS/GE/GP (Godot), SIDE/ZOOM,
twin_launch / twin_place_java / twin_stop. Run `twin_env` for the resolved paths.
EOF
  exit 2
fi
