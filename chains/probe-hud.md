---
id: probe-hud
title: Structured state, inspector, screenshot, logs
status: seeded
applies_when: Checking HUD/UI state or capturing evidence after an interaction.
preconditions:
  - boot-and-identity completed; teardown restores pause/camera after the probe.
tools: [godot_exec, godot_runtime_state, godot_screenshot, godot_log]
last_verified: not yet (seeded from .opencode/skills/playtest/SKILL.md §Recipes 6 and 8)
---

# probe-hud

## Steps

1. Canonical state dump (same schema as `mind-headless --dump`):

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").get_state_json()"}}
   ```

   Assert on parsed JSON, never substrings. The dump is heavy on a loaded
   sector; use it for assertions, not repeated polling.

2. Cheap live values:

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nreturn {\"tick\": host.get_tick(), \"checksum\": str(host.get_checksum()), \"pid\": OS.get_process_id()}"}}
   ```

   The inspector label at `/root/Spine/Ui/StateInspector/Label` refreshes from
   the same cheap accessors while visible.

3. HUD/UI models: query the `MindHud` / `MindUi` properties or JSON endpoints
   exposed on the `/root/Spine/Ui` subtree with `godot_runtime_state
   {"action":"digest","params":{"groups":[...]}}`; record which endpoint fed
   each assertion.

4. Screenshot (only when pixels are the evidence):

   ```
   godot_exec {"action":"eval","params":{"code":"get_window().grab_focus()"}}
   godot_screenshot {"action":"game"}
   ```

   Focus the game window first to avoid a stale presented frame when the editor
   overlaps it. Use `get_window().grab_focus()`: `move_to_foreground()` is
   deprecated on Godot 4.7.2 and logs an error-level deprecation entry that
   pollutes the "no new error-log entries" check (seen in run
   `l2-20261007-160101-ev0041-campaign-rules-godot`).

5. Logs: clear before the step, then check after:

   ```
   godot_log {"action":"clear"}
   # ...step...
   godot_log {"action":"errors"}
   ```

   A scenario passes only with an empty error log.

6. Teardown: restore pause/camera, `godot_game {"action":"stop"}`, then record
   the log.

## Success signals

- Tick/checksum move as expected; error log empty; screenshots non-blank
  (verify with `capture_screen.py`/`frame_diff.py` before treating a black frame
  as a finding).

## Failure modes

- `godot_screenshot game` can return identical bytes across different states
  while the editor covers the window; use `move_to_foreground` or capture the X
  screen with computer-mcp and crop.
- `godot_screenshot burst` blocks its round-trip for the whole burst; keep
  bursts tiny (≈4 frames / ≤500 ms).
- An eval body that errors stops in the editor Debugger; clear the log buffer
  before the scenario so stale errors are not attributed to the run.
