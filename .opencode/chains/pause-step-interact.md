---
id: pause-step-interact
title: Deterministic pause/step plus API and mouse place/break (click and drag)
status: verified
applies_when: Verifying placement, input routing, mouse click/drag place or break, or any state that must advance by exact ticks.
preconditions:
  - boot-and-identity completed (or golden-checksum for a loaded scenario).
tools: [godot_exec, godot_input, godot_runtime_state]
last_verified: 2026-10-08 761a483 (runs/20261008-020931-input_controls-twin, runs/20261008-015146-input_controls-godot)
---

# pause-step-interact

## Steps

1. Select the block through the input bridge, not the sim host (precondition
   for every mouse place/drag):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Input","method":"select_block_by_name","args":["conveyor"]}}
   godot_exec {"action":"eval","params":{"code":"var s = JSON.parse_string(get_node(\"/root/Spine/Input\").get_input_state_json())\nreturn {\"pid\": OS.get_process_id(), \"block\": s[\"block\"], \"is_building\": s[\"is_building\"]}"}}
   ```

   Assert `block` equals the requested name. `SimHost.select_block` only
   changes the sim host selection; the input controller keeps its previous
   block (settings-persisted), so a drag silently places the old block or
   nothing.

2. Pause for deterministic work:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}
   ```

   `set_paused` does NOT disable `_input`: events land and enqueue
   `SimCommand`s. The paused pump does **not** drain that queue
   (`SimHost.pending_command_count()` grows, `get_state_json()` stays
   unchanged), so mouse place/break only lands in the world after
   `set_paused(false)` or a `step(1)` — see Failure modes.

3. API place/break (Rust `#[func]` path):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[3, 5, "stone-wall"]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"break_block","args":[3, 5]}}
   ```

4. Mouse left-drag place (real input path), coordinates resolved from the
   camera, never hardcoded:

   ```
   godot_exec {"action":"eval","params":{"code":"var cam = get_node(\"/root/Spine/World/Camera2D\")\nvar a = cam.tile_to_screen(10, 16)\nvar b = cam.tile_to_screen(15, 16)\nreturn {\"pid\": OS.get_process_id(), \"a\": {\"x\": int(a.x), \"y\": int(a.y)}, \"b\": {\"x\": int(b.x), \"y\": int(b.y)}"}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":AX,"y":AY},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":AX,"y":AY},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":X11,"y":AY},"coords":"viewport","button_mask":["MOUSE_BUTTON_LEFT"]}}
   # ... one masked motion per intermediate tile, last one at BX,BY
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":BX,"y":BY},"coords":"viewport","pressed":false}}
   ```

   `placement_preview_json()` updates on every masked motion (assert it grows
   tile by tile); after the release flush and assert the dump:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar s = JSON.parse_string(get_node(\"/root/Spine/SimHost\").get_state_json())\nreturn {\"pid\": OS.get_process_id(), \"tiles\": s[\"world\"][\"tiles\"], \"commands_applied\": s[\"commands_applied\"]}"}}
   ```

5. Right-drag break: repeat step 4 with `MOUSE_BUTTON_RIGHT` and
   `button_mask:["MOUSE_BUTTON_RIGHT"]`. The press must enter breaking mode; the
   release removes every non-air tile swept. Assert the survivors in the dump
   (`commands_applied` must grow).

6. Prove the paused pump advances by exactly one tick after input:

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").step(1)"}}
   ```

7. Prefer `godot_runtime_state {"action":"inspect",...}` over another full
   `get_state_json` when only one property changed.

## Success signals

- API and mouse paths mutate the same tile; the state dump shows the block at
  `(7,7)`.
- The preview line matches the swept rect; the released dump contains exactly
  those tiles (place) or none of them (break).
- A right-drag over empty item-capacity blocks leaves `OverlayLayer/
  block_inventory` hidden (upstream opens it only on left tap and only when
  `hasItems && items.total() > 0`).
- `step(1)` increases the tick by exactly 1.

## Failure modes

- An injected `godot_input` click that is never flushed looks like a dead UI;
  always flush and assert the target state.
- If `godot_input` still does not land, build the events in eval and call
  `get_viewport().push_input(down)` / `push_input(up)` synchronously; record the
  fallback in `run.json`.
- `godot_input sequence` awaits `process_frame` and can hit the 15 s timeout;
  prefer discrete `mouse_button`/`mouse_motion`/`key` calls.
- `placement_preview_json` shows the drag rect even over occupied/blocked
  tiles; the sim silently leaves those tiles unchanged, so a "successful" drag
  can place nothing visible. Trust `commands_applied` plus the dump, not the
  preview.
- Selecting with `SimHost.select_block` does not sync the input controller; a
  drag then places the previous block (first drags after boot use the persisted
  settings block). Use step 1.
- If a right-press on an empty conveyor/turret opens `block_inventory` instead
  of breaking, that is the EV-0043-class gap (fixed at eaf7c6a), not a setup
  error.
- A paused pump hides landed input: after a drag, `pending_command_count() > 0`
  and the state dump shows nothing. Unpause or `step(1)` before judging a
  place/break (verified 2026-10-08 eaf7c6a, EV-0043).
- Do not send a sweep's final masked motion and its release in the same
  `godot_batch` call: the pair can coalesce and the last tile survives (6-tile
  right-drag broke 5, `commands_applied` 6 -> 11; a follow-up discrete
  right-press broke the survivor). One discrete `godot_input` call per event
  for the sweep tail; a clean 6-tile sweep then reads `commands_applied`
  18 -> 24 (verified 2026-10-08 761a483, twin pid 249867).
