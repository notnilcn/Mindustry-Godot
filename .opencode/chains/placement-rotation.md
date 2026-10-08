---
id: placement-rotation
title: Select a conveyor, press R, place at a rotation, and rotate the placed building via R (state-dump rot oracle)
status: verified
applies_when: Verifying block rotation (rotate key, rotated placement, `Call.rotateBlock` on placed buildings) in the no-scenario default world.
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - The editor process must postdate the built libmind_gdext.so (kill/relaunch after tools/build.sh).
tools: [godot_exec, godot_input, godot_game, godot_log, godot_screenshot]
last_verified: 2026-10-08 996f288 (runs/l2-20261008-185444-ev0056-rotation-godot)
---

# placement-rotation

## Steps

1. Clear the shared log buffer, hide the standalone menu, park the pointer at the
   viewport center and center the camera. The camera edge-pans while the OS
   pointer sits at (0,0), so park it first (half the window size; 1152x648 here):

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Ui/UiRoot","method":"set_menu_visible","args":[false]}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":576,"y":324},"coords":"viewport"}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/World/Camera2D","method":"center_on_tile","args":[16,16]}}
   ```

   The boot world is 32x32 (seed 1); center on tile (16,16) and use tiles around
   it. Zero non-air tiles means every nearby tile is placeable.

2. Select the block through the input bridge and pause for deterministic ticks:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Input","method":"select_block_by_name","args":["conveyor"]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}
   ```

3. Rotate the pending placement with R (discrete down+up calls; plain key name
   `R`, not `KEY_R`):

   ```
   godot_input {"action":"key","params":{"key":"R","pressed":true}}
   godot_input {"action":"key","params":{"key":"R","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar s = JSON.parse_string(get_node(\"/root/Spine/Input\").get_input_state_json())\nreturn {\"pid\": OS.get_process_id(), \"block\": s[\"block\"], \"rotation\": s[\"rotation\"], \"last_action\": s[\"last_action\"], \"action_count\": s[\"action_count\"]}"}}
   ```

   `InputState.rotation` starts at 1 (`InputHandler.rotation = 1`); after one R
   expect 2, `last_action=rotate`, `action_count` +1.

4. Resolve the target tile from the camera, press-hold to read the pending plan,
   then release (one discrete call per event):

   ```
   godot_exec {"action":"eval","params":{"code":"var p = get_node(\"/root/Spine/World/Camera2D\").tile_to_screen(15, 15)\nreturn {\"x\": int(p.x), \"y\": int(p.y)}"}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":X,"y":Y},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nreturn {\"pid\": OS.get_process_id(), \"preview\": get_node(\"/root/Spine/Input\").placement_preview_json()}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   ```

   The preview must read `[{"x":15,"y":15,"rotation":2,"block":257}]` at rotation
   2 (257 = conveyor raw id). Before 996f288 the state dump hardcoded tile
   `rot: 0`, so the dump assertion below is the finding's symptom.

5. Drain the queued `SimCommand::Place` and assert the tile rotation in the
   canonical dump:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar h = get_node(\"/root/Spine/SimHost\")\nvar pending = h.pending_command_count()\nvar tick = h.step(1)\nvar st = JSON.parse_string(h.get_state_json())\nvar tile = {}\nfor t in st[\"world\"][\"tiles\"]:\n\tif t[\"x\"] == 15 and t[\"y\"] == 15:\n\t\ttile = {\"block\": t[\"block\"], \"rot\": t[\"rot\"]}\nreturn {\"pid\": OS.get_process_id(), \"pending_before_step\": pending, \"tick\": tick, \"tile\": tile}"}}
   ```

   Expect `pending_before_step=1`, `tile.rot=2`, `commands_applied` +1.

6. Rotate the placed building: clear the block selection (otherwise R rotates
   the pending placement), park the cursor over the tile, and press R once per
   90°:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Input","method":"clear_building","args":[]}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":X,"y":Y},"coords":"viewport"}}
   godot_input {"action":"key","params":{"key":"R","pressed":true}}
   godot_input {"action":"key","params":{"key":"R","pressed":false}}
   ```

   Then repeat the step-5 eval (without `pending_before_step` expectations drift:
   each press leaves exactly one command). With the block cleared the fired
   action is `last_action=rotateplaced`; the dump rot cycles
   `2->3->0->1->2` over four presses (`commands_applied` +1 each).

## Success signals

- Pending rotation: `get_input_state_json().rotation` increments per R,
  `last_action=rotate`.
- Placement: `placement_preview_json()` and the dump tile `rot` both carry the
  pending rotation; `commands_applied` grows.
- Placed building: `last_action=rotateplaced`, one pending command per press,
  dump `rot = mod(rot+1,4)` (2->3->0->1->2 verified at 996f288).

## Failure modes

- Camera edge-pan: the OS pointer at (0,0) pans the camera away (position
  (128,128) -> (-7507,-7507) in ~2 min); `tile_to_screen` then returns
  off-screen coordinates. Park the pointer at the viewport center before
  resolving tile coordinates.
- `MindSimHost.apply_sim_command_json` has no `rotate` op (only `place`/`break`);
  drive rotation through `MindInput.rotate_placed()`/R instead of the JSON
  helper.
- The paused pump does not drain the queue; `step(1)` (or unpause) after each
  press before reading the dump.
- Assets are not packed in the worktree (`sprites.atlas.json` missing), so the
  renderer draws placeholder quads without rotation; judge on the state dump,
  not the frame.
- `Input.flush_buffered_events()` after each `godot_input` call; an unflushed
  click looks dead.
