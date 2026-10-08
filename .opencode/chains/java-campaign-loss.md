---
id: java-campaign-loss
title: Java reference — campaign sector loss via pause Abandon → GameOverDialog
status: verified
applies_when: >-
  A finding needs the Java reference behavior for a campaign loss /
  GameOverEvent / GameOverDialog path (EV-0061 family); also the fastest way to
  reach a real campaign game over in the running client without waiting for
  waves.
preconditions:
  - Loop manifest exported; JDK 17; `../Mindustry/desktop/build/libs/Mindustry.jar` built; the loop's Java display up.
  - A campaign save with a captured sector exists in the loop's
    `xdg-data/Mindustry/saves` (the seeded dir has Ground Zero). A fresh dir
    first shows the "Select Starting Campaign" dialog (Serpulo photo → OK).
  - Persistent `computer-mcp_*` tools work on the loop display, or
    `parity-click.sh` / a pynput one-shot is used. A compositor restart breaks
    the persistent server's X connection (`Broken pipe`) while one-shots keep
    working (computer-mcp-learnings.md).
  - `Escape` reached the pause menu: on the Xvfb fallback the first key press
    only focuses the window (send it twice).
tools: [computer-mcp_mouse_move, computer-mcp_click, computer-mcp_key_press, parity-click.sh, capture_screen.py]
last_verified: 2026-10-08 1d8f75b (runs/20261008-215055-ev0061-game-over-twin, JVM pid 72553)
---

# java-campaign-loss

## Steps

1. Launch at the twin window size (screen coords below are window + (70,63) for
   1152x648; re-measure with `xwininfo -root -children` + `capture_screen.py
   --window-title Mindustry` for other sizes):

   ```
   setsid .opencode/loops/bin/run-java.sh -width 1152 -height 648 -maximized false \
     >"$RUN/java/game.log" 2>&1 </dev/null &
   ```

   Poll `game.log` for `Total time to load`; record the JVM pid
   (`pgrep -f Mindustry.jar`).

2. `mouse_move`/`parity-click.sh` `Play` (350,216 works; 300,213 is the button
   centre at 1152x648) → the Play submenu opens (it stays open until toggled
   off; a click that looks dead usually means the client froze, see Failure
   modes).

3. `Campaign` (548,213) → planet view. Serpulo is the selected planet.

4. Click the current sector hex, e.g. Ground Zero at the screen centre
   (646,387) with "1 under attack" → bottom panel appears. Note: button rows
   sit lower than a scaled screenshot suggests — hover-probe first
   (`.opencode/evals/runs/20261008-215055-ev0061-game-over-twin/java/probe.py`
   + a pynput move) or click the bottom bar row at screen y≈678.

5. `Go` (646,678) → the sector loads (Ground Zero has its player core).

6. Key tap `Escape` (pynput `Key.esc`; plain `"Escape"` raises ValueError) →
   pause menu. Its `Abandon` entry is only present while the sector is being
   played (`state.rules.sector`), and it calls
   `ui.planet.abandonSectorConfirm` (PausedDialog.java:82).

7. `Abandon` (760,278) → confirm dialog "This sector's core(s) will
   self-destruct. Continue?"; `OK` (749,431) → the cores self-destruct.

8. ~3 s (`Time.run(60f * 3f, () -> ui.restart.show(winner))`, Logic.java:438)
   → capture: `GameOverDialog` headline `@sector.lost <name>` ("Sector Ground
   Zero lost!"), stats rows, `Continue` button.

9. Prove liveness + action: hover `Continue` (646,663) and diff the capture
   (a highlight change proves input + repaint); click it → planet map with the
   difficulty-guide `Notice`.

## Success signals

- `GameOverDialog` with the `@sector.lost` headline and `Continue`, no
  exception in `java/game.log` (ALSA device warnings are noise).
- Hovering the dialog button changes pixels and the click lands (`Continue`
  opens the planet map) — the client kept running after the loss.

## Failure modes

- `n` (`Binding.planetMap`) did not open the planet map from the in-game state
  on this build; use Escape → `Abandon` instead.
- `SDL_GL_SwapWindow` freeze: the window stops animating and ignores input
  (jstack shows the main thread in `arc.backend.sdl.jni.SDL.SDL_GL_SwapWindow`).
  Kill the client and relaunch through `run-java.sh`; the wrapper restarts the
  Xwayland compositor when it is down. Killing the java weston also breaks the
  persistent computer-mcp server (`Broken pipe`) — use `parity-click.sh` /
  pynput one-shots for the rest of the leg.
- Button Y estimates from the attached screenshot can be ~50 px off; confirm
  the target with a hover capture diff before clicking.

## Variants

- A fresh Java data dir shows "Select Starting Campaign" before the planet
  page (Serpulo photo ~(477,310) → OK ~(640,673) at 1280x720, per
  java-reference-leg.md).
