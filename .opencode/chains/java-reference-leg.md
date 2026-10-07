---
id: java-reference-leg
title: Java reference — launch, drive a menu flow, capture, quit
status: seeded
applies_when: >-
  The twin evaluator needs the Java leg of a reproduction (or any session needs
  a reference capture) inside a parity loop.
preconditions:
  - Loop manifest exported (`start-loop.sh`): PARITY_DISPLAY, PARITY_LOOP_DIR, PARITY_EVALS_DIR, PARITY_RUN_PREFIX.
  - JDK 17 present (`bootstrap.sh --check`) and the reference jar built at `../Mindustry/desktop/build/libs/Mindustry.jar`.
  - computer-mcp registered and opencode started with DISPLAY set (it imports pynput at module load; without an X connection its tools vanish).
  - One client at a time on the loop display (Xvfb has no window manager).
tools: [computer-mcp_mouse_move, computer-mcp_click, computer-mcp_drag, computer-mcp_type, computer-mcp_key_press, computer-mcp_key_down, computer-mcp_key_up, computer-mcp_screenshot, computer-mcp_list_windows, computer-mcp_get_window_info]
last_verified: not yet
---

# java-reference-leg

## Steps

1. Create the run dir and launch through the loop wrapper (it sets DISPLAY, the
   per-loop seeded HOME and the loop's XDG dirs):

   ```bash
   RUN="$PARITY_EVALS_DIR/runs/${PARITY_RUN_PREFIX}<stamp>-<scenario>"
   mkdir -p "$RUN/java"
   nohup .opencode/loops/bin/run-java.sh >"$RUN/java/game.log" 2>&1 &
   echo $! > "$RUN/java/client.pid"
   ```

2. Wait for the readiness line instead of a fixed sleep:

   ```bash
   until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done
   ```

3. Capture the initial frame (bootstrap venv; `capture_screen.py` prints
   mean/stddev so a blank frame is caught before it becomes a finding):

   ```bash
   "$MCP_VENV/bin/python" .opencode/skills/parity-eval/scripts/capture_screen.py \
     --output "$RUN/java/step-01-menu.png"
   ```

4. Drive the flow with the persistent computer-mcp MCP tools
   (`computer-mcp_mouse_move`, `computer-mcp_click`, `computer-mcp_drag`,
   `computer-mcp_type`, `computer-mcp_key_press`, ...), re-capturing after each
   named step. For a scripted CLI click use
   `.opencode/loops/bin/parity-click.sh <x> <y> [button]` (single process); a
   bare `computer-mcp mouse move` followed by a separate `computer-mcp mouse
   click` does not work on Xvfb — the pointer resets to screen center when the
   one-shot XTEST client exits.

5. Quit gracefully through the main-menu Quit button. If the process lingers,
   kill the pid from `client.pid` and note it in `run.json`.

## Success signals

- Readiness line present; the first capture is non-blank.
- Captures use the window size recorded in `run.json` (fixed comparison setup).
- Clean exit: the pid is gone and no orphan java process remains.

## Failure modes

- `computer-mcp_*` tools missing: opencode was started without `DISPLAY`;
  relaunch with the loop shims and report the blocker instead of improvising a
  headless Java leg.
- Clicks land nowhere: another client owns the display, or the window moved.
  Re-read the screenshot and recompute the coordinate; never reuse coordinates
  blindly.
- Settings/saves differ from the Godot run: normalize inside the loop's seeded
  HOME (`window size, UI scale, language, volume`) and record the snapshot in
  `run.json`; `settings_backups/` keeps prior snapshots.

## Variants

- Keyboard-only navigation: send `computer-mcp_key_press` sequences when the
  path exists in both clients.
- Window geometry automation (`xdotool`/`wmctrl`) is optional; without it,
  capture the full screen and run one client at a time.
