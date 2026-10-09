---
id: block-config-fragment
title: Place a configurable block, open its block_config fragment via tap or block_info, read the option list
status: verified
applies_when: Verifying a building's config panel (fragment visibility, picker options) for a placed block (EV-0045 sorter; unloader/router/route-config blocks next).
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - A configurable block placed at a known tile. The default no-scenario world (32x32 seed 1) works; res://scenarios/input_controls is absent.
tools: [godot_exec, godot_input]
last_verified: 2026-10-08 780efa9 (runs/l2-20261008-203338-input_controls-godot)
---

# block-config-fragment

## Steps

1. Hide the standalone menu and stamp the runtime:

   ```
   godot_exec {"action":"eval","params":{"code":"get_node(\"/root/Spine/Ui/UiRoot\").set_menu_visible(false)\nvar host = get_node(\"/root/Spine/SimHost\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": host.get_tick(), \"block\": JSON.parse_string(get_node(\"/root/Spine/Input\").get_input_state_json())[\"block\"]}"}}
   ```

2. Select and place the block (input selection first, then API place; the
   fragment opens on a left tap only when no placement block is selected):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Input","method":"select_block_by_name","args":["sorter"]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[18, 16, "sorter"]}}
   godot_exec {"action":"eval","params":{"code":"var s = JSON.parse_string(get_node(\"/root/Spine/SimHost\").get_state_json())\nreturn {\"pid\": OS.get_process_id(), \"tile\": s[\"world\"][\"tiles\"].filter(func(t): return t[\"x\"] == 18 and t[\"y\"] == 16)}"}}
   ```

3. Clear the placement selection (`tile_tapped` runs only with `block == null`):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Input","method":"clear_building"}}
   ```

4. Exhaust the hints fragment before clicking: after ~8 s of playtime its
   full-screen `Panel` (mouse_filter STOP) covers the viewport center and
   `MindInput.ui_captures_at` drops the press as a UI capture.

   ```
   godot_exec {"action":"eval","params":{"code":"var hints = get_node(\"/root/Spine/Ui/UiRoot/HudGroup/hints\")\nhints.set(\"_next\", 27)\nhints.complete_hint()\nreturn {\"pid\": OS.get_process_id(), \"visible\": hints.visible}"}}
   ```

5. Freeze the camera (the headless compositor parks the OS pointer at a window
   edge, so edge pan drifts the view every frame) and recenter, then resolve
   the click point in a **later** eval — the canvas transform updates at frame
   end, so a same-eval `tile_to_screen` after `center_on_tile` is stale:

   ```
   godot_exec {"action":"eval","params":{"code":"var cam = get_node(\"/root/Spine/World/Camera2D\")\ncam.set_process(false)\ncam.center_on_tile(18, 16)\nreturn {\"pid\": OS.get_process_id(), \"pos\": str(cam.position)}"}}
   godot_exec {"action":"eval","params":{"code":"var p = get_node(\"/root/Spine/World/Camera2D\").tile_to_screen(18, 16)\nreturn {\"pid\": OS.get_process_id(), \"point\": {\"x\": int(p.x), \"y\": int(p.y)}, \"window\": {\"w\": DisplayServer.window_get_size().x, \"h\": DisplayServer.window_get_size().y}}"}}
   ```

6. Open the fragment — either gesture lands on the same `config_spec` read
   model. Left tap (current `tileTapped`, EV-0043 moved config here from the
   right button), two discrete calls so the pair cannot coalesce:

   ```
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","coords":"viewport","position":{"x":576,"y":324},"pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","coords":"viewport","position":{"x":576,"y":324},"pressed":false}}
   ```

   or the `block_info` binding (default F1) at the same cursor tile, no click
   needed:

   ```
   godot_input {"action":"key","params":{"key":"F1","pressed":true}}
   godot_input {"action":"key","params":{"key":"F1","pressed":false}}
   ```

7. Read the fragment state (`Input.flush_buffered_events()` first for the
   tap path):

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar frag = get_node(\"/root/Spine/Ui/UiRoot/OverlayLayer/block_config\")\nvar cfg = frag.get(\"_config\")\nvar options = cfg.get(\"options\", []) if cfg is Dictionary else []\nreturn {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"visible\": frag.visible, \"block\": cfg.get(\"block\") if cfg is Dictionary else null, \"x\": cfg.get(\"x\") if cfg is Dictionary else null, \"y\": cfg.get(\"y\") if cfg is Dictionary else null, \"option_count\": options.size(), \"options\": options}"}}
   ```

8. Restore `cam.set_process(true)` before teardown.

## Success signals

- `visible == true`, `block == "sorter"`, `x`/`y` equal the placed tile.
- `_config.options` is the item list: 22 entries at 780efa9 (copper, lead,
  metaglass, graphite, sand, coal, titanium, thorium, scrap, silicon,
  plastanium, phase-fabric, surge-alloy, spore-pod, blast-compound, pyratite,
  beryllium, tungsten, oxide, carbide, fissile-matter, dormant-cyst).
- The tap and F1 paths report identical fragment state.

## Failure modes

- A right-click no longer opens config (EV-0043): it enters breaking mode.
  Use the left tap with no placement block selected, or `block_info`/F1.
- Left tap does nothing and `block_config.visible` stays false: probe the
  click point for STOP-filter controls
  (`get_tree().root.find_children("*", "Control", true, false).filter(...)`)
  before suspecting the input path; the hints panel is the usual hit.
- `_config.options` empty for a configurable block means the behavior did not
  declare a config kind and `def.has_items`/`has_liquids` are both false — the
  read-model gap EV-0045 fixed for the sorter (declare
  `BuildingBehavior::config_kinds`).
- A same-eval `tile_to_screen` after `center_on_tile` returns a stale point;
  always resolve in a later eval.
- No scenario file: do not call `load_scenario` for `input_controls`; use the
  default world.

## Variants

- Item containers open `OverlayLayer/block_inventory` instead (left tap only
  when `hasItems && items.total() > 0`).
- Logic blocks open the text editor path (`_config.text == true`).
