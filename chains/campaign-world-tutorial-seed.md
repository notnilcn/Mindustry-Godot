---
id: campaign-world-tutorial-seed
title: Launch into Ground Zero (sector 15) on both legs — in-game start state, tutorial prompts, HUD
status: verified
applies_when: >-
  A campaign world/tutorial sweep or item needs both clients in-game on Ground
  Zero after the planet-page launch.
preconditions:
  - `chains/campaign-planet-page-seed.md` replayed to the planet page (Java fresh
    or progressed; Godot state aligned when pixel comparison matters).
  - Java window raised immediately before every capture; Godot bridge on 6970.
  - Godot launch flow: planet page → sector action (576,613) → Configure Loadout
    → Launch (367,503). Java: Launch (621,676) free-launches directly.
tools: [java_input.py, window_ctl.py, capture_screen.py, frame_diff.py, godot_input, godot_exec, godot_screenshot, godot_log]
last_verified: 2026-10-09 runs/20261009-052658-seed-campaign (iter 3 world-tutorial sweep, PAR-0028; iter 4 fresh relaunch + camera recenter, PAR-0031)
---

# campaign-world-tutorial-seed

## Steps

1. Java free launch (from the fresh planet page, window +45+69):
   `java_input.py move 621 676` + `java_input.py click 621 676 left` → loading bar → tutorial
   world. Current step-1 UI: objective `Obtain: 0/15 Copper`; prompts
   `Use [WASD] to move.` / `Move near the copper ore on the ground and click to
   begin mining.`; `Skip`.

2. Godot launch (from the same planet page, viewport coords):
   `godot_input` press/release at the sector action `(576,613)` → the
   `launch_loadout` dialog; press/release `Launch` `(367,503)` → sector starts.
   During the load the planet dialog may still be visible and
   `sector_state.campaign` still false — poll until
   `get_sector_state()` reports `campaign:true, hasCore:true, sector:15` and
   `SimHost.get_tick()` advances. The `MindCampaign::start_sector() received 3
   arguments` log line is the known loadout-arg fallback
   (`client/ui/dialogs/planet_dialog.gd:747`).

3. Captures (raise Java first):
   `capture_screen.py --window-title Mindustry --output <run>/seed/step-NN-tutorial-java.png`
   `godot_screenshot {"action":"game"}` → copy to `<run>/seed/step-NN-tutorial-godot.png`.

4. Gate:
   `frame_diff.py --java <java> --godot <godot> --out-json <run>/diff/diff-13-tutorial.json`
   Observed: changed 0.719, mean_abs [70.4,71.3,72.6], coarse 69.4, palette 0.393
   → mismatch (PAR-0028).

## Observed divergences (2026-10-09)

- Java is at tutorial step 1 (objective/copper prompts); Godot runs waves
  (`Wave 1/10`, `Wave in 3:59`) with a shooting prompt — a different
  tutorial/wave state (PAR-0028).
- Godot prompt markup bug: `[color=#ffd37fff][lb][lb]Left-click][/color] to
  shoot.` renders as `][[Left-click] to shoot.` (literal brackets).
- Godot frame carries debug readouts (`HudGroup/hud/FpsBox/{Fps,Memory,Ping}`
  = `FPS: 38`/`210 MB`/`0 ms`, and `HudGroup/performance/Panel/Label` =
  `fps 38 cpu 31.34 ms mem 210.3 MB`) with no Java counterpart; hide those two
  nodes before pixel work.
- Camera pose differs: Godot zoom 4, screen center tile (1028,436).
- Bottom HUD: Java shows the bottom-right icon bar + minimap; Godot shows an
  empty core-items panel (no icon bar/minimap in frame).

## Iteration-4 notes (2026-10-09, fresh relaunch)

- `clear_campaign_saves` + stop/play + the UI launch path reproduce the same
  start-state divergence (waves + hints, no tutorial objective panel, no
  movement/mining prompts): tutorial progression cannot be mirrored from a
  common start (PAR-0028; frontier popped after the attempt). Fresh pair:
  `diff/diff-16-start.json` changed 0.692, mean_abs [63.4,65.3,67.1].
- Camera drift (PAR-0031): with the OS pointer parked near the game-window edge
  the port edge-pans continuously; after idle minutes the camera sat at
  `(1028, 32870)` on the 256x256 map and `godot_screenshot` returned an empty
  world. Parking the pointer at the window center stops the drift; recovery is
  an eval calling `get_node("/root/Spine/World/Camera2D").recenter_player()`
  (snaps to and follows the best core; `is_following()` -> true).
- UI freshness: after `stop` + `play`, wait for the menu before tapping; do the
  Play -> Campaign -> chooser -> OK -> action -> loadout sequence with an eval
  check between hops (a blind tap pass can end at the menu).

## Failure modes

- Concluding "no launch" from the first poll: the planet dialog stays up during
  the launch; wait for `campaign:true`.
- Comparing before both clients settle: give the Godot fade ~10 s.
- The Godot in-game frame includes debug overlays (see above) — distinct from
  Java; do not read them as parity break by themselves.
- Blank Godot world with a live HUD: the camera drifted off-map (edge-pan);
  probe `Camera2D.get_screen_center_position()` and `recenter_player()`.
