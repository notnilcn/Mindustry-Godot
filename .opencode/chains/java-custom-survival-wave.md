---
id: java-custom-survival-wave
title: Java reference — custom survival game, trigger a wave, capture spawn/move/combat
status: verified
applies_when: >-
  A twin/parity run needs the Java side of the wave/unit/bullet runtime (Units/
  WaveSpawner/Groups liveness) as player-visible ground truth and the campaign
  sector is tutorial-gated or cannot trigger a wave quickly.
preconditions:
  - Loop manifest exported (`start-loop.sh`); Java client launched via
    `.opencode/loops/bin/run-java.sh`; computer-mcp tools present (DISPLAY set at
    opencode start).
  - Campaign Ground Zero (Java sector 170) cannot be used for timer/skip waves:
    its msav rules are `waveTimer:false, waveSending:false` and waves come from a
    world-processor script gated on `objectiveFlags[spawnWave]`. Use a default
    map with `map.spawns > 0` instead (survival mode needs spawns).
  - One client at a time on the loop display; Java window recorded once
    (reference run: 900x700+190+10 — coordinates below are screen coords for
    that geometry, re-derive from `xwininfo -root -children` otherwise).
tools: [computer-mcp_mouse_move, computer-mcp_click, computer-mcp_type, computer-mcp_key_press, computer-mcp_screenshot]
last_verified: 2026-10-08 a1ca816 (runs/20261008-040830-ev0048-live-unit-runtime-twin); 2026-10-08 1d8f75b loop-1 twin 1152x648 (runs/20261008-225009-ev0053-hud-twin)
---

# java-custom-survival-wave

## Steps

1. Main menu → Play (350,187) → Custom Game (645,328). The submenu/cards need a
   fresh `computer-mcp_screenshot` after each click (input-call screenshots lag
   one frame).
2. In the map list, click a survival-capable card (one-core icon; e.g. Domain at
   (426,548) preview). The gamemode dialog opens for that map.
3. Keep **Survival** selected (shortens nothing on its own: `play()` sets
   `wavetime = waveSpacing * 2` = 8 min by default). Do **not** use
   "Customize Rules" to edit numeric fields — injected keyboard input is dropped
   in that dialog (see `computer-mcp-learnings.md`); a mis-read checkbox can turn
   Waves off and the in-game status pane then renders empty. Click **Play**
   (747,628).
4. In game, read the top-left status pane: `Wave N / Wave in mm:ss` plus a play
   (skip) triangle at ~(500,53). The triangle is `HudFragment`'s skip button →
   `Logic.skipWave()` → `runWave()`.
5. Click the skip triangle, wait ~2 s, capture:
   `.opencode/skills/parity-eval/scripts/capture_screen.py --window-title Mindustry --output <run>/java/step-05-wave1-triggered.png`
   Expect `Wave N+1 / k Enemy Remaining`.
6. Follow the unit: it spawns at a map spawn point and paths to the player core.
   Zoom out with a one-shot pynput scroll if needed:
   `DISPLAY=:N "$MCP_VENV/bin/python" -c "from pynput.mouse import Controller; Controller().scroll(0,-6)"`
   (discrete events land; xdotool/computer-mcp have no scroll).
7. Capture approach and combat: full-window captures every few seconds until the
   unit is adjacent to the core; rapid core crops (10 x ~0.18 s) catch the
   muzzle/impact flash; the HUD banner `< Core is under attack! >` appears.
8. Leave the timer running: further waves spawn without a skip
   (`Logic` decrements `wavetime` and calls `runWave()` at 0). The Game Over
   stats (`Waves Defeated`, `Buildings Destroyed`) close the reference if the
   core dies.

## Success signals

- Skip click → `state.enemies` reflects a live group (`k Enemy Remaining`).
- The unit's sprite crosses the map between captures (spawn → core).
- Muzzle/impact flash on the core and/or the "Core is under attack!" banner
  (combat); the timer keeps advancing the wave counter.
- Evidence run: `runs/20261008-040830-ev0048-live-unit-runtime-twin/java/`
  (step-05, step-08/09, step-10-*/tmp-fire-grid, step-11; Wave 2 / 1 Enemy
  Remaining, ~278 px approach, flash, Game Over `Waves Defeated 3`).

## Verified run (loop-1 twin, 2026-10-08, 1152x648, Xwayland :2)

EV-0053 twin Java leg, JVM pid 126779, window +70+63 (run
`runs/20261008-225009-ev0053-hud-twin`). The persistent computer-mcp server was
Broken pipe at the first call; input used `parity-click.sh` plus a one-shot
pynput double-click (`/tmp/opencode/click2.py`, two clicks 120 ms apart) for
the submenu row:

1. `Play` screen (310,216) — one click opened the submenu.
2. `Custom Game` row screen (560,353) — a single click only highlighted; the
   120 ms double-click opened the map list.
3. Card preview screen (816,325) opened the MapPlayDialog for **Debris Field**
   (column 3; the survival map list has spawns, so it was kept instead of
   re-aiming for Domain). `Play` screen (755,654) loaded the map.
4. In-game HUD baseline at 1152x648: `Wave 1` + `Wave in 3:59` + white skip
   triangle at screen (380,106). Note the wave pane is top-left; the status
   text itself is the ground truth.
5. Click the skip triangle (380,106): `Wave 2` + `1 Enemy Remaining` +
   `Wave in 1:59`, and the triangle disappears (`canSkipWave` false;
   `imageDisabledColor = Color.clear` at `HudFragment.java:494-497`).
6. Quit with `kill -TERM` on the JVM pid (custom game, no save to preserve).

## Failure modes

- Editing Custom Rules number fields: typed digits are dropped; leave the rules
  untouched and use the skip button.
- Campaign Ground Zero: skip button is disabled (`waveSending:false`) and no
  timer wave arrives — do not wait there; use a default map in Custom Game.
- Whole-window captures read the game window directly (`--window-title
  Mindustry`, no xdotool needed); on Xwayland the monitor/root grab is black,
  so window capture is the only reliable path there.
- `computer-mcp_click`'s attached screenshot shows the previous frame (and is
  black on Xwayland); always re-capture with `capture_screen.py` before
  interpreting. On the Xvfb fallback Java needs a first key press to focus the
  window; send Escape/J twice.
- Killing the JVM with `kill -TERM <java-pid>` is fine for custom games (no
  sector save to preserve); main-menu Quit takes more blind clicks.

## Variants

- A resumed campaign sector save can be loaded from the main menu (Load Game)
  but does not serialize its units, so it cannot show the runtime directly.
- To keep the same rules `waveTimer`/`waveSending` values on both sides, mirror
  the scenario on the Godot side (custom game) or compare the runtime class
  rather than the scenario, as EV-0048 did.
