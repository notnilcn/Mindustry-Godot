---
id: rts-select-orders
title: RTS command mode, drag/tap selection and right-click orders (get_input_state_json + commands_applied)
status: verified
applies_when: Verifying RTS selection/order findings (EV-0055) when no scenario populates the client with units; the default empty world is the stage and selectable units are arranged through the documented set_selectable_units_json seam.
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - No production unit feed exists in the client (live unit runtime is EV-0048 scope); call MindInput.set_selectable_units_json yourself.
  - The standalone menu is hidden (input-controls step 2); world clicks at viewport center are dropped while paused because the PausedBanner is a STOP Control (see open-godot-mcp-learnings.md, 2026-10-08).
tools: [godot_exec, godot_input, godot_game, godot_log]
last_verified: 2026-10-08 986cda3 (runs/l2-20261008-051657-ev0055-rts-select-twin; chain corrected: hold Shift for a persistent command mode and put unit 102 at the tap point)
---

# rts-select-orders

## Steps

1. Clear the log and hide the standalone menu:

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Ui/UiRoot","method":"set_menu_visible","args":[false]}}
   ```

2. Enter command mode **through the real key path** and confirm the live state
   field (`get_input_state_json` is on `/root/Spine/Input`, not the SimHost).
   `commandmodehold` defaults true, so hold Shift and sample while held — the
   panel/no-unit assertion from `input-controls.md` step 6 applies:

   ```
   godot_input {"action":"key","params":{"key":"Shift","pressed":true}}
   godot_exec {"action":"eval","params":{"code":"var s = JSON.parse_string(get_node(\"/root/Spine/Input\").get_input_state_json())\nreturn {\"pid\": OS.get_process_id(), \"command_mode\": s[\"command_mode\"], \"selected_units\": s[\"selected_units\"], \"command_rect\": s[\"command_rect\"], \"last_action\": s[\"last_action\"]}"}}
   ```

   `command_mode: true`. Keep Shift held for every later input step, then
   release at teardown. `MindInput.toggle_command_mode()` is a valid positive
   control **only if read in the same eval**: it returns true and the field is
   true live, but the next frame's hold recompute (`commandmodehold=true`)
   drops it back to false while Shift is up.

3. Arrange selectable units. Coordinates are **world pixels** at
   `TILESIZE=8`: tile (tx,ty) center = `((tx+0.5)*8, (ty+0.5)*8)`. Put unit 102
   at tile (14,10) = world (116,84) so the step-6 tap point actually hits it;
   include an out-of-rect own-team unit and a team-1 unit to prove filtering:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Input","method":"set_selectable_units_json","args":["[{\"id\":101,\"type\":3,\"x\":84,\"y\":84,\"team\":0,\"commandable\":true},{\"id\":102,\"type\":3,\"x\":116,\"y\":84,\"team\":0,\"commandable\":true},{\"id\":103,\"type\":3,\"x\":84,\"y\":84,\"team\":1,\"commandable\":true}]"]}}
   ```

   Returns the parsed count (3).

4. Resolve drag/tap/target points through the camera (screen pixels; the
   runtime lifts `coords: "viewport"` to window space):

   ```
   godot_exec {"action":"eval","params":{"code":"var cam = get_node(\"/root/Spine/World/Camera2D\")\nreturn {\"a\": cam.tile_to_screen(8, 8), \"b\": cam.tile_to_screen(12, 12), \"tap102\": cam.tile_to_screen(14, 10), \"target\": cam.tile_to_screen(16, 10)}"}}
   ```

5. Drag-rect select with real input — motion to A, press at A, motion to B
   with the left button mask, release at B:

   ```
   godot_input {"action":"mouse_motion","params":{"position":{"x":320,"y":68},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":320,"y":68},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":448,"y":196},"coords":"viewport","button_mask":["MOUSE_BUTTON_LEFT"]}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":448,"y":196},"coords":"viewport","pressed":false}}
   ```

   Read back: `selected_units` = in-rect team-0 commandable ids only,
   `command_rect` = `[x1*8, y1*8, (x2-x1+1)*8, (y2-y1+1)*8]` (here
   `[64,64,40,40]`).

6. Tap select (one sequence — press+release in the same call so the click
   lands; `command_rect` clears on the tap path). The tap point must be within
   `UNIT_TAP_RADIUS` (11 world px) of the unit — unit 102 is at world (116,84),
   i.e. screen (512,132):

   ```
   godot_input {"action":"sequence","params":{"steps":[{"type":"mouse_motion","params":{"position":{"x":512,"y":132},"coords":"viewport"}},{"type":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":512,"y":132},"coords":"viewport","pressed":true}},{"type":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":512,"y":132},"coords":"viewport","pressed":false}}],"frame_delay":1}}
   ```

   Measured semantics: a tap **replaces** the selection with the tapped unit
   (drag `[101]`, then tap 102, reads `[102]`); a tap on empty ground clears it
   (`[]`). Double-tap typed select: repeat press+release twice inside one
   sequence (both clicks < 300 ms apart, `UNIT_TAP_INTERVAL_MS`): selects every
   own-team commandable unit of the tapped type (here `[101,102]`).

7. Order observable. Pause first so the queued command is inspectable
   (`pending_command_count` / `get_state_json().commands_applied` are on
   `/root/Spine/SimHost`):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nvar d = JSON.parse_string(host.get_state_json())\nreturn {\"pid\": OS.get_process_id(), \"pending\": host.pending_command_count(), \"commands_applied\": d[\"commands_applied\"], \"tick\": host.get_tick()}"}}
   ```

   Check the target point has no visible STOP Control first (the viewport
   center is covered by `PausedBanner` while paused) and right-click:

   ```
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_RIGHT","position":{"x":576,"y":132},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_RIGHT","position":{"x":576,"y":132},"coords":"viewport","pressed":false}}
   ```

   `pending` 0 -> 1 unchanged `commands_applied`. Then apply and assert:

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nvar t = host.step(1)\nvar d = JSON.parse_string(host.get_state_json())\nreturn {\"pid\": OS.get_process_id(), \"tick\": t, \"pending\": host.pending_command_count(), \"commands_applied\": d[\"commands_applied\"]}"}}
   ```

   `pending` 0, `commands_applied` +1, tick +1.

8. Teardown: toggle command mode off
   (`godot_exec call /root/Spine/Input toggle_command_mode`), unpause
   (`set_paused(false)`), then `godot_game {"action":"stop"}`.

## Success signals

- Shift held → `command_mode: true` while held (hold-to-command default), and
  `toggle_command_mode()` read in the same eval reads true (was hardcoded
  false pre-fix), `last_action=command_mode`.
- Drag-rect assigns exactly the in-rect own-team commandable units and sets a
  world-pixel `command_rect`; the out-of-rect and enemy units stay out.
- Tap selects the tapped unit (replacing a previous selection); a tap on empty
  ground clears; double-tap typed-selects its type.
- A right-click with a non-empty selection queues one `SimCommand::UnitCommand`
  (`pending` +1 while paused) and increments `commands_applied` when the next
  tick drains it; with the pump running the increment lands without a manual
  step.
- `godot_log errors` has no input-path entries (a
  `move_to_foreground()` deprecation entry from a screenshot-forcing eval is
  not an input failure).

## Failure modes

- `toggle_command_mode()` is transient while `commandmodehold=true`: the return
  value and a same-eval read are true, but the next frame's
  `command_mode = keyDown(shift)` recompute drops it. Hold Shift for the whole
  drag/tap/order sequence (the player path) or re-toggle immediately before the
  input call.
- A tap more than `UNIT_TAP_RADIUS` (11 world px) from a unit's world position
  reads as a tap on empty ground and clears the selection. Unit 102 must be at
  the tap point's world coords (tile 14,10 = world 116,84), not merely in the
  JSON registration order.
- Viewport center while paused: `PausedBanner` is a visible STOP Control
  covering a 64x26 rect at the center, so a world press there is consumed as UI
  by `MindInput.ui_captures_at`; the click looks like a no-op. Probe STOP hits
  or click off-center.
- The bottom-right/bottom HUD (`placement/Panel` etc.) is also STOP; resolve
  points over the free world region, not under HUD panels.
- World-unit conversion is `TILESIZE=8`, not the screen scale (32 px/tile at
  the default zoom); feeding screen-pixel deltas into `set_selectable_units_json`
  puts every unit outside the rect.
- `get_input_state_json` lives on `/root/Spine/Input`; `/root/Spine/SimHost`
  has the sim-side observables (`pending_command_count`, `step`,
  `get_state_json`).
- No production selectable-unit feed exists yet (EV-0048 scope): a drag in a
  plain player session selects nothing until something calls
  `set_selectable_units_json`. Do not report that as this chain failing.
