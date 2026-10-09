---
id: ground-zero-wasd-movement
title: Ground Zero — launch, WASD-move the player mech, hint transition and camera follow
status: verified
applies_when: >-
  Driving the campaign player unit (Godot side) for the WASD-movement reference
  items (PAR-0044 family) or any probe that needs `player.unit()` live in a
  launched sector.
preconditions:
  - Session-2 style isolated client: display :11, bridge 6980, own XDG data dir.
  - Editor launched with the current `libmind_gdext.so` (relaunch after tools/build.sh).
  - No leftover sector save when a fresh fork is wanted (`clear_planet_campaign_saves` + stop/play).
tools:
  - godot_health
  - godot_editor_read
  - godot_game
  - godot_exec
  - godot_input
  - godot_screenshot
  - godot_debugger
path: boot -> Play > Campaign -> chooser OK -> planet page Launch -> (loadout Launch) -> launched Ground Zero
last_verified: 2026-10-09 working tree `eab320c2`+PAR-0044 round 1 (runs/20261009-092821-PAR-0044/, pid 142646) — signals verified; frame gate blocked on PAR-0028/zoom
---

# ground-zero-wasd-movement

The Godot-side walk that launches Ground Zero and mirrors the Java R02 movement
action. Round-2 run: `runs/20261009-092821-PAR-0044/`.

## Path (fast-forward)

Navigation only; replay back-to-back, confirm once at the end.

1. `godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}`
   then `godot_game {"action":"status"}` until `runtime_connected: true`.
2. Click the menu sidebar Play at viewport `(230, 149)`, then Campaign at
   `(460, 149)` (discrete `mouse_button` press+release, then
   `godot_exec Input.flush_buffered_events()`).
3. First-run chooser (`DialogLayer/planet`, title `Select Starting Campaign`):
   Serpulo card `(412, 194)`, then OK `(499, 634)`; wait ~12 s for the planet page.
4. Planet page (sector 15 preselected): action button `(576, 613)`. A fresh
   launch opens `launch_loadout`; click its Launch `(367, 503)`. A resume launch
   goes straight to the world.
5. Wait ~15 s for the world; confirm with
   `godot_exec`: `sector_state {campaign:true, hasCore:true, sector:15}`,
   `SimHost.has_player_unit() == true`, `HudGroup.visible == true`.

## Steps (under test)

Sim-tick-exact holds: `godot_input sequence` counts process frames, so pause and
step instead; keep movement + capture in one tool block.

1. Hide the debug overlays:
   `HudGroup/hud/FpsBox.visible = false`, `HudGroup/performance.visible = false`.
2. Fork capture: `godot_screenshot {"action":"game"}`.
3. Hold S for 54 sim ticks:
   `godot_input {"action":"key","params":{"key":"s","pressed":true}}` →
   `godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nh.set_paused(true)\nvar t = h.step(54)\nreturn t"}}` →
   `godot_input {... "key":"s","pressed":false}` →
   `godot_exec {... "code":"var h = ...\nh.set_paused(false)\nreturn h.get_tick()"}`.
4. Let the camera lerp settle (a few dummy evals / ~1 s), then screenshot.
5. Hold W for 54 ticks the same way, settle, screenshot.

## Success signals

- `has_player_unit: true` and `unit` group count > 0 after launch; the mech
  sprite is drawn (currently at the port's zoom 4 → a large sprite).
- Camera follows: after S the camera leaves the core (`(1028, ~436)` →
  `(1028, ~110)`); after W it returns (`(1028, ~433)`).
- Hint transitions on the first movement: `HudGroup/hints/.../Label` goes
  `hint.desktopMove` → `hint.zoom` and stays there (round-2 frames).
- No `MindCampaign::start_sector` arity error in the log (PAR-0054 fix).

## Failure modes

- `godot_exec` bodies: no nested `var` in `if` blocks and no multi-statement
  `if` blocks — the wrapper errors and break-on-error pauses the frame loop
  (`godot_debugger sessions` → `paused: true`); recover `stop` + `play`
  (`resume` does not stick).
- Timer hints keep running between tool calls; capture in the same block as the
  movement or the 15 s fallback advances past `zoom`.
- A `godot_input sequence` hold length is fps-dependent; use pause+step for
  tick-exact holds.
- Frame comparison is blocked by the fork divergences owned by PAR-0028
  (tutorial objective HUD, wave state, terrain) and the camera zoom item
  (port zoom 4 vs the Java reference's zoom): the §4 gate cannot pass yet.
