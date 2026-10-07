---
id: pause-step-interact
title: Deterministic pause/step plus API and mouse place/break
status: seeded
applies_when: Verifying placement, input routing, or any state that must advance by exact ticks.
preconditions:
  - boot-and-identity completed (or golden-checksum for a loaded scenario).
tools: [godot_exec, godot_input, godot_runtime_state]
last_verified: not yet (seeded from .opencode/skills/playtest/SKILL.md §Recipes 3, 4, 5)
---

# pause-step-interact

## Steps

1. Pause for deterministic work:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}
   ```

   `set_paused` does NOT disable `_input`; mouse place/break still applies
   while paused.

2. API place/break (Rust `#[func]` path):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[3, 5, "stone-wall"]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"break_block","args":[3, 5]}}
   ```

3. Mouse place/break (real input path), coordinate resolved from the camera,
   never hardcoded:

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/World/Camera2D\").tile_to_screen(7, 7)"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   ```

   Then flush and assert in one eval:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nreturn get_node(\"/root/Spine/SimHost\").get_state_json()"}}
   ```

4. Prove the paused pump advances by exactly one tick after input:

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").step(1)"}}
   ```

5. Prefer `godot_runtime_state {"action":"inspect",...}` over another full
   `get_state_json` when only one property changed.

## Success signals

- API and mouse paths mutate the same tile; the state dump shows the block at
  `(7,7)`.
- `step(1)` increases the tick by exactly 1.

## Failure modes

- An injected `godot_input` click that is never flushed looks like a dead UI;
  always flush and assert the target state.
- If `godot_input` still does not land, build the events in eval and call
  `get_viewport().push_input(down)` / `push_input(up)` synchronously; record the
  fallback in `run.json`.
- `godot_input sequence` awaits `process_frame` and can hit the 15 s timeout;
  prefer discrete `mouse_button`/`mouse_motion`/`key` calls.
