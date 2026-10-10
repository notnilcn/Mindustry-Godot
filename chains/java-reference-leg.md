---
id: java-reference-leg
title: Java reference — launch, drive a menu flow, capture, quit
status: verified
applies_when: >-
  A parity session needs the Java leg of a mirrored item (reference capture) on
  the shared display :10.
preconditions:
  - JDK 17 present (`bootstrap.sh --check`) and the reference jar built at `../Mindustry/desktop/build/libs/Mindustry.jar`.
  - "Java-leg shell helpers: scripts in `.opencode/skills/parity-eval/scripts/`, run with `$PARITY_PY` (bootstrap.sh) and `DISPLAY=:10`. The Godot editor shares :10, so always raise the Java window before capturing."
  - Persistent Java data dir `parity-java-home/` normalized (1152x648, UI scale 1.0, language en, volumes 0) and stable across runs.
tools: [java_input.py, window_ctl.py, capture_screen.py]
last_verified: 2026-10-09 session-1 twin PAR-0056 (16a58ca+working-tree, runs/20261009-141406-PAR-0056-twin/, JVM pid 134983; single-display :10 recipe; legacy loop-era runs are kept in the Verified-run sections below)
---

# java-reference-leg

## Steps

1. Create the run dir and launch detached (`setsid` survives the shell-tool
   timeout; a plain `nohup ... &` is killed with the tool's process group):

   ```bash
   RUN="runs/$(date -u +%Y%m%d-%H%M%S)-<item>"
   mkdir -p "$RUN/java"
   JAR=../Mindustry/desktop/build/libs/Mindustry.jar
   JHOME="$PWD/parity-java-home"          # persistent, gitignored
   setsid env DISPLAY=:10 HOME="$JHOME" \
     XDG_DATA_HOME="$JHOME/.local/share" XDG_CONFIG_HOME="$JHOME/.config" \
     XDG_CACHE_HOME="$JHOME/.cache" \
     /usr/lib/jvm/java-17-openjdk-amd64/bin/java -jar "$JAR" \
     >"$RUN/java/game.log" 2>&1 </dev/null &
   pgrep -f 'Mindustry[.]jar' > "$RUN/java/java.pid"
   ```

2. Wait for the readiness line instead of a fixed sleep:

   ```bash
   timeout 180 bash -c 'until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done' \
     || echo "launch gate timed out; see $RUN/java/game.log" >&2
   ```

3. Normalize the window before anything reads it: if it opened maximized,
   `window_ctl.py unmaximize Mindustry` first (a maximized window ignores
   `resize`), then `window_ctl.py resize Mindustry 1152 648`. Raise it (the
   Godot editor shares `:10`), then capture the initial frame with
   `capture_screen.py` (it prints mean/stddev, so a blank frame is caught before
   it becomes a finding). `--window-title` reads the game window directly:

   ```bash
   DISPLAY=:10 "$PARITY_PY" \
     .opencode/skills/parity-eval/scripts/window_ctl.py raise Mindustry
   DISPLAY=:10 "$PARITY_PY" \
     .opencode/skills/parity-eval/scripts/capture_screen.py \
     --window-title Mindustry --output "$RUN/java/step-01-menu.png"
   ```

   The report's `region` is the window origin/size in screen coordinates — use
   it to turn an element offset in the capture into a click coordinate.

4. Drive the flow with the Java-leg shell helpers (`java_input.py move`,
   `java_input.py click`, `java_input.py drag`, `java_input.py type`,
   `java_input.py key`, ...), re-capturing with `capture_screen.py` after each
   named step. Helper calls print JSON, never a frame — always capture
   separately before judging an action. For scroll and timed key combos use
   `java_input.py scroll` / `java_input.py seq` (one process, one focus; see
   `java-leg-learnings.md`).

5. Quit gracefully through the main-menu Quit button. If the process lingers,
   kill the recorded pid (`kill -TERM "$(cat "$RUN/java/java.pid")"`) and note
   it in `run.json`. Never `pkill -f` a loose jar pattern: it also matches the
   tool shell itself.

> Historical: the Verified-run sections below predate the single-display setup
> (retired loop wrappers, Xvfb/Xwayland loop displays). Keep them for the
> click-coordinate tables and flow ground truth; the launch steps above
> supersede them.

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
   `DISPLAY=:11 "$PARITY_PY"
   .opencode/skills/parity-eval/scripts/capture_screen.py --window-title Mindustry
   --output …` (bootstrap.sh creates the helper venv).
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
re-toggles the pause dialog closed (see java-leg-learnings.md). Quit with
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

## Verified run (loop-1 twin, 2026-10-08, EV-0059 host flow, 1152x648)

Launched with `run-java.sh -width 1152 -height 648 -maximized false`; the client
lands at `+70+63` (xwininfo absolute upper-left), so screen = window + (70,63).
Sequence that reached the host ground truth (run
`runs/20261008-105728-ev0059-host-twin`, JVM pid 25568/25570):

1. Menu `Play` (350,216) -> submenu `Custom Game` (547,356) -> map list.
   The MapPlayDialog's bottom `Back`/`Play` row was input-dead at this geometry
   (hover/focus never applied; verified pointer position), so the in-game entry
   used `Play` -> `Load Game` (546,425) -> click a save card (whole card loads;
   Archipelago survival at (370,513)). Verifying a loaded save works too.
2. In-game `key_down escape` + `key_up escape` (split calls) -> pause dialog.
   `Host Multiplayer Game` at (646,440); the dialog titled the same shows
   `Name:` + `Port: 6567` as field text.
3. `Host` click with an empty name -> `@noname` "Pick a player name first."
   (upstream guard). Java TextFields drop injected characters (caret moves, no
   text), so set the name offline through Arc `Settings` (`settings.bin`) or
   reuse a seeded data dir before launching.
4. With a name, `Host` -> `Opening server…` load fragment -> `net.host(6567)`:
   `ss -ltnup | grep 6567` shows TCP LISTEN + UDP owned by the JVM; the dialog
   closes and the pause `Host Multiplayer Game`/`Load Game` entries are
   disabled while `net.active()`.
5. Click reliability: verify the X pointer position (pynput read-back) before
   each click; a move+click with a short fixed sleep can land at the previous
   cursor position. Details in `java-leg-learnings.md`. Quit with
   `kill -TERM` on the recorded `java -jar ...Mindustry.jar` pid.

## Verified run (loop-1 twin, 2026-10-08, EV-0038, 1152x648)

Editor maps flow at 1152x648, window origin +70+63, JVM pid 84648 (run
`runs/20261008-121029-ev0038-editor-row-twin`). The persistent computer-mcp
server was already Broken pipe at leg start (display alive, window captures
fine), so every click went through `parity-click.sh`:

1. Main menu `Editor` (313,354) → Maps grid with the live registry
   (`java/step-03-editor-maps.png`).
2. `Archipelago` card (425,313) → Map Info dialog (Map Name/Author,
   `Open In Editor`, disabled `Delete`) (`java/step-05-archipelago-info.png`).
3. `Open In Editor` (490,518) → `Loading…` → MapEditor with the map rendered
   (`java/step-07-map-editor-loaded.png`).
4. Quit with `kill -TERM` on the JVM pid.

Both the Editor menu entry and the Archipelago card consumed the first click
after a fresh launch (the capture showed hover only); a second click opened the
dialog. Evidence: `java/step-02-after-editor-click.png`,
`java/step-04-archipelago-info.png` (hover-only).

## Verified run (loop-1 twin, 2026-10-08, EV-0039, 1152x648, Xwayland :2)

Custom Game map-list leg at 1152x648 (`run-java.sh -width 1152 -height 648
-maximized false`), window at +70+63, JVM pid 94767 (run
`runs/20261008-221916-ev0039-twin`). The persistent computer-mcp server was
Broken pipe at leg start (display alive, window captures fine); all input went
through `/tmp/opencode/click.py` (pynput one-shot: move, read the pointer back
within 1 px, 0.25 s settle, click) plus one-shot pynput scrolls.

1. `Play` (screen 302,213): the first click only highlighted the button; a
   second click opened the submenu (Campaign/Join Game/Custom Game/Load Game).
2. `Custom Game` row (screen 520,352; window 450,289): the first solo click
   closed the submenu without opening the grid; after reopening the submenu,
   sending two clicks 120 ms apart at the same point reached the grid and then
   the MapPlayDialog (title `Mud Flats`). Budget two sends and verify by
   capture. Submenu rows showed no hover highlight (unlike the main-menu
   buttons), so hover-diff probing cannot locate them — use the light-text
   pixel bands of `step-04-play-submenu.png`.
3. The grid scrolled with one-shot `pynput.mouse.Controller().scroll(0,-4)`
   x6 over the grid (no scroll action existed then — `java_input.py scroll`
   now covers it): row 3 Molten Lake/Mud Flats/
   Passage(PvP)/Shattered/Tendrils, row 4 Triad/Veins(PvP)/Wasteland = 18.
4. MapPlayDialog `Back` (screen 540,655) responded to a single click this run
   (the EV-0059 note about the input-dead bottom row did not reproduce).
5. Quit `kill -TERM` on the `java -jar .*Mindustry.jar` pid (gone within ~5 s).

## Verified run (loop-1 twin, 2026-10-08, EV-0051, 1152x648, Xwayland :2)

Campaign planet-map leg at 1152x648 (`run-java.sh -width 1152 -height 648
-maximized false`), window +70+63, JVM pid 108885 (run
`runs/20261008-223232-ev0051-campaign-live-views-twin`). The persistent
computer-mcp server was Broken pipe at leg start again; all input via
`/tmp/opencode/click.py` and one-shot pynput Escape. Screen coords =
window + (70,63):

1. Menu `Play` (screen 310,216) → submenu; `Campaign` (550,216): the first
   click on the row only highlights, a second click opens the planet page
   directly (the loop's data dir has `sector-serpulo-170.msav`; Serpulo
   `startSector = 170`, so no first-run chooser).
2. Ground Zero hex (655,388) opens the side panel (not owned: `Threat: Low`,
   Resources icons, `Launch`); hovering shows `[ Ground Zero ]`, while the
   white locked-preset icon (696,359) tooltips `[ Locked ]` and is not
   selectable.
3. The `Launch` button's text measures at window y 602-630 (screen y ~679);
   the attached PNG is scaled in the model view, so measure with PIL
   (`Image.open(...).convert("L")`, bright-pixel row bands) before clicking.
   Launching Ground Zero loads its tutorial (`Obtain: 0/15 Copper`).
4. In-game Escape (one-shot pynput press/release, no auto-repeat) → pause menu;
   `Save & Quit` (645,474) → confirm `OK` (747,420) → main menu; the campaign
   save is rewritten. Reopen Play → Campaign: the map shows the `Sectors /
   1 under attack` banner, the Ground Zero hex carries the yellow warning
   marker, and its panel reads `Under attack!`, `Stats`, action `Go`.

Evidence: `java/step-06-sector-click2.png` (not owned), `step-12-after-launch.png`
(tutorial), `step-17-planet-after.png` (warning marker), `step-19-gz-panel-after.png`
(`Under attack!`/`Go`), `java/notes.md`.

## Verified run (loop-2 twin, 2026-10-09, EV-0056, Xwayland :2, 1152x648)

The worktree wrapper resolves its own runtime dirs: from this worktree it
started a second java weston and Xwayland `:3`, while the session's persistent
computer-mcp server is pinned to `:2`. Pin the client to `:2` and
keep GLFW off Wayland:

```bash
(env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET PARITY_DISPLAY_OVERRIDE=:2 \
  setsid .opencode/loops/bin/run-java.sh -width 1152 -height 648 \
  -maximized false >"$RUN/java/game.log" 2>&1 </dev/null &)
```

Without `-u WAYLAND_DISPLAY` GLFW opens a native Wayland window on the Godot
compositor: the JVM runs but no X window exists for captures or clicks. The
window lands at +70+63; capture with `--window-title Mindustry` (DISPLAY=:2).
Custom-game navigation reused the recorded 1152x648 coordinates (Play
double-click 315,212; Custom Game double-click 550,354 -> MapPlayDialog; Play
750,641 -> world; ~30-45 s fade-in to a stable frame). One-shot XTEST friction:
each pynput process loses the window's keyboard focus, so key events need a
click first in the same process; without OCR (tesseract absent) and with a
non-vision model the catalog/selection state cannot be read, so this leg
reached a playable world but could not pixel-isolate the rotation path
(`runs/l2-20261009-001432-ev0056-rotation-twin/java/notes.md`).

## Verified run (session-1 twin, 2026-10-09, PAR-0056, xrdp Xorg :10, 1152x648)

The current single-display recipe in Steps 1-5 verified end-to-end: detached
`setsid env DISPLAY=:10 HOME=$PWD/parity-java-home …` launch (readiness
`Total time to load: 2597ms`), `window_ctl.py unmaximize Mindustry` +
`resize Mindustry 1152 648` + `move Mindustry 45 69` + `raise` → client area
**+50+98** 1152x648 (xfwm4 +5/+29; screen = image + (50,98)), then
Play → Campaign → chooser Serpulo → OK → Ground Zero hex → Launch → tutorial
(`Obtain: 0/15 Copper`) → Escape → Save & Quit → confirm OK → menu.

Recorded screen coords: Play (307,247), Campaign (527,249), Serpulo card
(464,338), OK (627,705), GZ hex (627,420), Launch (627,710), Save & Quit
(626,510), confirm OK (728,456), main-menu Quit (307,591).

Two surprises: `computer-mcp.click` did not open Play on the fresh window
(byte-identical frames twice) while explicit `button_down` + `button_up` did
(see java-leg-learnings.md, 2026-10-09); and a fresh profile needs more
than emptied saves — `campaignselect` in `settings.bin` must be false for the
first-run chooser (back up settings.bin, flip it offline). Save meta check:
zlib-decompress `sector-serpulo-170.msav` and assert `,loadout:[],` in the
meta region (helper: PAR-0056 run `java/verify_msav.py`). Quit cleanly via the
menu Quit button (pid gone < 3 s).

Evidence: `runs/20261009-141406-PAR-0056-twin/` (JVM pid 134983;
`java/fresh-launch.png`, `java/save-meta-loadout.txt`, `java/step-12-menu-after.png`).

## Success signals

- Readiness line present; the first capture is non-blank.
- Captures use the window size recorded in `run.json` (fixed comparison setup).
- Clean exit: the pid is gone and no orphan java process remains.

## Failure modes

- Java-leg helpers unavailable: `bootstrap.sh` has not been run (no
  `$PARITY_PY`) or a call ran without `DISPLAY=:10`; run the bootstrap / set
  the display and report the blocker instead of improvising a headless Java leg.
- Clicks land nowhere: another client owns the display, or the window moved.
  Re-read the screenshot and recompute the coordinate; never reuse coordinates
  blindly.
- Settings/saves differ from the Godot run: normalize inside the loop's seeded
  HOME (`window size, UI scale, language, volume`) and record the snapshot in
  `run.json`; `settings_backups/` keeps prior snapshots.
- The retired `computer-mcp` window tools returned
  `Window management not yet implemented for Linux` (historical). Use
  `window_ctl.py list` or `DISPLAY=:N xwininfo -root -children` for the window
  id/geometry; click screen coords = window origin + element offset in the
  screenshot.
- Launching with `nohup … &` inside a shell-tool command does not survive the
  tool timeout (the process group is killed). Use
  `setsid .opencode/loops/bin/run-java.sh >"$RUN/java/game.log" 2>&1 </dev/null &`.
- Black captures on the Java leg: on Xwayland the X root holds no GPU pixels,
  so root/`--rect`/monitor grabs come back black. Use
  `capture_screen.py --window-title Mindustry` (or a bare capture, which retries
  the window) and confirm the report's `blank` field is false before
  interpreting anything.
- On the Xvfb fallback the first key press after a click only focuses the
  window; send the binding twice (observed for Escape and J). A held left
  button can also stay logically down across helper calls (mining continues);
  send an explicit `java_input.py mouseup X Y left` (at the held click's
  coordinates) before expecting a fresh click to act.
- The loop wrappers resolve `PARITY_LOOP_DIR` against the **worktree-local**
  `.opencode/loops`, so the Java data dir is
  `<worktree>/.opencode/loops/run/loop-N/xdg-data/Mindustry`, not the
  `$PARITY_LOOP_DIR` exported by the session environment.

## Variants

- Keyboard-only navigation: send `java_input.py key NAME` sequences (one
  process per binding, or a single `seq` batch) when the path exists in both
  clients.
- Window geometry automation (`xdotool`/`wmctrl`) is optional; without it,
  capture the full screen and run one client at a time.
