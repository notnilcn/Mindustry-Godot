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
  - One client at a time on the loop display (`$PARITY_DISPLAY`; Xwayland with Weston's WM by default, Xvfb with no WM on the software fallback).
tools: [computer-mcp_mouse_move, computer-mcp_click, computer-mcp_drag, computer-mcp_type, computer-mcp_key_press, computer-mcp_key_down, computer-mcp_key_up, computer-mcp_screenshot, computer-mcp_list_windows, computer-mcp_get_window_info]
last_verified: 2026-10-08 9efd068 (runs/l2-20261007-163042-twin-java-ref); 2026-10-08 a1ca816 loop-1 (runs/20261008-143432-ev0062-twin); 2026-10-08 986cda3 loop-2 (runs/l2-20261008-150205-input_controls-twin)
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
   mean/stddev so a blank frame is caught before it becomes a finding). Read
   the game window directly with `--window-title`: on the Xwayland display the
   X root holds no GPU pixels, so mss/root captures come back black. A bare
   capture grabs the monitor and automatically retries the window when that is
   blank. The report's `region` is the window origin/size in screen
   coordinates — use it to turn an element offset in the capture into a click
   coordinate:

   ```bash
   DISPLAY="$PARITY_DISPLAY" "$HOME/.local/share/uv/tools/computer-mcp/bin/python" \
     .opencode/skills/parity-eval/scripts/capture_screen.py \
     --window-title Mindustry --output "$RUN/java/step-01-menu.png"
   ```

4. Drive the flow with the persistent computer-mcp MCP tools
   (`computer-mcp_mouse_move`, `computer-mcp_click`, `computer-mcp_drag`,
   `computer-mcp_type`, `computer-mcp_key_press`, ...), re-capturing with
   `capture_screen.py` after each named step (`computer-mcp screenshot`
   captures the X root and is black on Xwayland). For a scripted CLI click use
   `.opencode/loops/bin/parity-click.sh <x> <y> [button]` (single process); a
   bare `computer-mcp mouse move` followed by a separate `computer-mcp mouse
   click` does not work on Xvfb — the pointer resets to screen center when the
   one-shot XTEST client exits.

5. Quit gracefully through the main-menu Quit button. If the process lingers,
   kill the pid from `client.pid` and note it in `run.json`.

## Verified run (loop-2 twin, 2026-10-08, 9efd068)

Confirmed once end-to-end at 900x700 on Xvfb `:11` (run
`runs/l2-20261007-163042-twin-java-ref`), including the menu flows the twin
sweep needed:

1. Detached launch (the shell tool kills the group otherwise):
   `setsid .opencode/loops/bin/run-java.sh >"$RUN/java/game.log" 2>&1 </dev/null &`,
   then poll `game.log` for `Total time to load`. The JVM pid is the
   `pgrep -f Mindustry.jar` match, not the wrapper pid.
2. Geometry: `DISPLAY=:11 xwininfo -root -children` → `"Mindustry": 900x700+190+10`.
   Window-cropped captures via
   `DISPLAY=:11 "$HOME/.local/share/uv/tools/computer-mcp/bin/python"
   .opencode/skills/parity-eval/scripts/capture_screen.py --window-title Mindustry
   --output …` (mss lives in the computer-mcp uv tool).
3. Menu coordinates observed at 900x700 (screen coords): Play (350,187);
   submenu Campaign/Join/Custom/Load at y 188/258/328/397, x 645;
   Load Game Back (533,670); Custom Game map card (635,260); MapPlay Play (747,629);
   in-game Escape menu Save Game (525,347)/Save & Quit (640,478), confirm OK (740,394);
   main-menu Quit (305,536). Java campaign planet page: Difficulty rail (297,133),
   Serpulo hex (640,360), Launch (640,677); research opens with `J`; a locked node's
   hover popup shows the requirement (`Copper 0/10` → `10/10` → `Researched`).
4. Quit: `kill -TERM <java-pid>` (a desktop campaign has no Save Game; the sector
   save is written at launch and on exit). Screen clean afterwards.

## Verified run (loop-1 twin, 2026-10-08, EV-0062)

Ground truth for paused-dialog buttons, same 900x700 window at +190+10:
mouse `Play` (410,187) -> submenu `Custom Game` (648,327) -> `Archipelago` card
(640,200) -> MapPlay `Play` (747,628) -> `key_down escape` + `key_up escape`
(split calls) -> pause-dialog `Settings` (754,281) opens the Settings dialog.
Use split `key_down`/`key_up`: a held `key_press escape` auto-repeats and
re-toggles the pause dialog closed (see computer-mcp-learnings.md). Quit with
`kill -TERM` on the `java -jar .*Mindustry.jar` pid; the display is clean
afterwards. Run: `runs/20261008-143432-ev0062-twin` (JVM pid 140283,
`java/notes.md`).

## Verified run (loop-2 twin, 2026-10-08, EV-0044, 986cda3)

A fresh campaign on a fresh data dir has one extra first-run dialog before the
planet page: Play (405,185) -> Campaign (647,188) -> `Select Starting Campaign`:
Serpulo photo (477,310) -> OK (640,673) -> planet page with `Ground Zero`
selected -> Launch (640,677). In-game, `Shift` held swaps the bottom panel to
`Command Mode` / `[no units]` and the diamond command cursor appears; release
returns to the byte-identical baseline (run
`runs/l2-20261008-150205-input_controls-twin`, JVM pid 190927; exact sequence in
`chains/command-mode-hold-vs-tap.md`). Quit with `kill -TERM` on the JVM pid; the
Xvfb root only shows the loop's Godot editor window afterwards.

## Verified run (loop-1, 2026-10-08, Xwayland GPU path)

The java weston + Xwayland path was verified at the wrapper level (no scenario
run): `parity_ensure_display` started `mind1-java` and resolved
`PARITY_DISPLAY=:1` with `PARITY_DISPLAY_SERVER=xwayland`; `run-java.sh` logged
`[GL] Version: ... AMD Radeon 780M Graphics (radeonsi ...)` and
`Total available VRAM: 8.4 GB`; `capture_screen.py --window-title Mindustry`
returned a non-blank window frame (mean 85.7 / std 48.7) with a correct
`region`; a `parity-click.sh` click landed (the target Quit entry closed the
client, which exited cleanly). No Xvfb was involved.

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
- `computer-mcp_list_windows` / `get_window_info` return
  `Window management not yet implemented for Linux`. Use
  `DISPLAY=:N xwininfo -root -children` for the window id/geometry; click
  screen coords = window origin + element offset in the screenshot.
- Launching with `nohup … &` inside a shell-tool command does not survive the
  tool timeout (the process group is killed). Use
  `setsid .opencode/loops/bin/run-java.sh >"$RUN/java/game.log" 2>&1 </dev/null &`.
- Black captures on the Java leg: on Xwayland the X root holds no GPU pixels,
  so `computer-mcp screenshot` and `--rect`/monitor grabs come back black. Use
  `--window-title Mindustry` (or a bare capture, which retries the window) and
  confirm the report's `blank` field is false before interpreting anything.
- On the Xvfb fallback the first key press after a click only focuses the
  window; send the binding twice (observed for Escape and J). A held left
  button can also stay logically down across MCP calls (mining continues);
  send an explicit `computer-mcp_button_up` before expecting a fresh click to
  act.
- The loop wrappers resolve `PARITY_LOOP_DIR` against the **worktree-local**
  `.opencode/loops`, so the Java data dir is
  `<worktree>/.opencode/loops/run/loop-N/xdg-data/Mindustry`, not the
  `$PARITY_LOOP_DIR` exported by the session environment.

## Variants

- Keyboard-only navigation: send `computer-mcp_key_press` sequences when the
  path exists in both clients.
- Window geometry automation (`xdotool`/`wmctrl`) is optional; without it,
  capture the full screen and run one client at a time.
