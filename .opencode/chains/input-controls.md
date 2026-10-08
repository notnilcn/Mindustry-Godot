---
id: input-controls
title: Enter the no-scenario world and verify keyboard input paths against get_input_state_json
status: verified
applies_when: Verifying input/command findings (hotkeys, RTS command mode, clear/pause building) when res://scenarios/<name> does not exist in the client; the default empty world is the stage.
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - res://scenarios/input_controls is absent from client/scenarios; do not block on load_scenario.
tools: [godot_exec, godot_input, godot_game, godot_log]
last_verified: 2026-10-08 986cda3 (runs/l2-20261008-150205-input_controls-twin twin-verified; runs/l2-20261008-140421-input_controls-godot)
---

# input-controls

## Steps

1. Clear the shared log buffer before the run (stale boot errors are otherwise
   attributed to the step):

   ```
   godot_log {"action":"clear"}
   ```

2. Enter the world without a scenario file — hide the standalone menu:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Ui/UiRoot","method":"set_menu_visible","args":[false]}}
   godot_exec {"action":"eval","params":{"code":"var ui = get_node(\"/root/Spine/Ui/UiRoot\")\nreturn {\"pid\": OS.get_process_id(), \"menu_group_visible\": ui.get_node(\"MenuGroup\").visible, \"hud_group_visible\": ui.get_node(\"HudGroup\").visible}"}}
   ```

   `UiRoot.visible` does not change; assert the `MenuGroup`/`HudGroup`
   children (menu hidden, HUD shown).

3. Liveness + baseline input state (`get_input_state_json` is on
   `/root/Spine/Input`; `block` is settings-persisted, `duo` on this host):

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nvar s = JSON.parse_string(get_node(\"/root/Spine/Input\").get_input_state_json())\nreturn {\"pid\": OS.get_process_id(), \"tick\": host.get_tick(), \"frames\": Engine.get_process_frames(), \"block\": s[\"block\"], \"command_mode\": s[\"command_mode\"], \"action_count\": s[\"action_count\"], \"last_action\": s[\"last_action\"], \"is_building\": s[\"is_building\"]}"}}
   ```

4. Inject a key press as discrete down+up calls (plain key names; `Shift`
   normalizes to the `shiftLeft` binding):

   ```
   godot_input {"action":"key","params":{"key":"Q","pressed":true}}
   godot_input {"action":"key","params":{"key":"Q","pressed":false}}
   ```

5. Re-read the same fields; assert on the `action_count`/`last_action` delta,
   not just the payload. Q -> `last_action=clear_building`, `block=null`.

6. For command-mode keys, sample while held AND after release in separate
   round-trips (the event dispatches on the next frame). `commandmodehold`
   defaults true, so with `block=null` a held Shift must read
   `command_mode=true` (`action_count` +1, `last_action=command_mode`) and a
   released Shift must read `false`. For the tap branch, turn the setting off,
   tap, ignore the release edge, then restore it:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/MindUi","method":"settings_set","args":["commandmodehold", false]}}
   godot_input {"action":"key","params":{"key":"Shift","pressed":true}}   # toggles
   godot_input {"action":"key","params":{"key":"Shift","pressed":false}}  # ignored
   godot_exec {"action":"call","params":{"node_path":"/root/MindUi","method":"settings_set","args":["commandmodehold", true]}}
   ```

   A dead path leaves `command_mode` false with `action_count`/`last_action`
   unchanged.

7. API comparison (proves the state field itself works and localizes a gap to
   the key path):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Input","method":"toggle_command_mode","args":[]}}
   ```

   Returns the new bool and records `last_action=command_mode`; read
   `get_input_state_json()` to confirm.

8. Persist the samples for the run dir by appending one JSON line per probe from
   the eval with `FileAccess.open("user://<name>.jsonl", FileAccess.READ_WRITE)`
   + `seek_end()`/`store_line()`, then copy the file out of
   `<worktree>/.opencode/loops/run/loop-N/xdg-data/godot/app_userdata/Mindustry-Godot/`.
   Each line should carry `pid`, `tick`, `frames`, `ms`, `block`,
   `command_mode`, `action_count`, `last_action`.

## Success signals

- Q clears the block: `block=null`, `action_count` +1, `last_action=clear_building`.
- The command-mode key sets `command_mode=true` while held by default
  (`last_action=command_mode`, `action_count` +1), or toggles on the press edge
  with `commandmodehold=false`; release drops it only in hold mode.
- `toggle_command_mode()` returns a bool and records `last_action=command_mode`.
- Tick and frames advance between evals before any key call is trusted.

## Failure modes

- No scenario file: `res://scenarios/input_controls` is absent from
  `client/scenarios`; do not call `load_scenario` — rebuild the leg on the
  default world and say so in the report.
- A `godot_input key` returning `ok: true` proves nothing; assert the state
  delta and re-check liveness (`Engine.get_process_frames()` vs
  `Time.get_ticks_msec()`) because a stalled frame loop makes key calls
  silently no-op.
- An eval body that errors trips break-on-error and stalls the loop;
  `godot_game stop` + `play` is the recovery (`SimHost.is_paused()` may still
  read false while frames are pinned). An eval that *times out* (e.g. a
  `while` loop body) stalls the loop the same way — check
  `Engine.get_process_frames()` twice before trusting the next input call, and
  restart rather than resume (verified 2026-10-08, twin run pids 231551/245330
  discarded for pids 245330/249867). An eval body containing `await` without
  the eval `await` param also errors (`Trying to call an async function without
  "await".`) and stalls the loop the same way (2026-10-08, EV-0044 twin, pid
  197940 recovered from pid 197260).
- The game runs **embedded** in the editor; `DisplayServer.window_set_size`
  is refused (`Embedded window can't be resized.`) — record the actual
  1152x648 window instead of matching another client's size.
- Command-mode key semantics are hold-to-command by default
  (`commandmodehold=true`, `commandMode = input.keyDown`, `DesktopInput.java:303-304`);
  the tap/toggle branch at 305-306 runs only when the setting is false. Assert
  the held and released samples separately — sampling only after release reads
  `false` and looks dead. A toggle-on-release observation means a stale
  extension is loaded: rebuild and restart the editor before re-running
  (`open-godot-mcp-learnings.md`, 2026-10-08). EV-0044 was twin-verified at
  986cda3 with this chain's exact steps.
- Key input is gated by dialogs/fields (`focus.has_dialog`/`has_field`); hide
  the menu first.
- The persisted block is settings-owned; for placement use
  `MindInput.select_block_by_name` (`chains/pause-step-interact.md` step 1).
