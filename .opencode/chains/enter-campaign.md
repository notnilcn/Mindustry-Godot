---
id: enter-campaign
title: Main menu → Campaign → planet → sector → launch
status: seeded
applies_when: Entering or observing the campaign flow, from menu to launched sector HUD.
preconditions:
  - boot-and-identity completed (game in the main menu, runtime connected).
  - UI clicks resolve control centers via eval; never hardcode coordinates.
  - Java reference leg (optional comparison) needs computer-mcp with DISPLAY set at opencode process start.
tools: [godot_exec, godot_input, godot_screenshot, godot_log]
last_verified: not yet (UI variant seeded from .opencode/skills/playtest/SKILL.md + menu findings; facade variant from run l2-20261007-064916-campaign_spine)
---

# enter-campaign

## Steps (UI variant)

1. Resolve the Play button center from the live scene tree (paths are
   illustrative; list the actual `Control` nodes first):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"<menu path>/Play\")\nvar p = (c as Control).get_global_rect().get_center()\nreturn {\"x\": int(p.x), \"y\": int(p.y)}"}}
   ```

2. Click it and flush:

   ```
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nreturn get_tree().root.gui_get_hovered_control().name if get_tree().root.gui_get_hovered_control() else null"}}
   ```

3. Repeat for Campaign and the sector row; the planet dialog is only reachable
   from the main menu (there is no in-game pause-menu route while EV-0035-family
   campaign gaps are open).

4. Launch and confirm the HUD:

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": host.get_tick(), \"checksum\": str(host.get_checksum())}"}}
   godot_screenshot {"action":"game"}
   ```

## Steps (campaign facade variant)

Use when the UI flow is blocked but the campaign model is the subject:

1. Query the facade:

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").start_sector(\"serpulo\", 170)"}}
   ```

   (`start_sector` lives on the `MindCampaign` facade used by
   `.opencode/evals/runs/l2-20261007-064916-campaign_spine`; confirm the node
   path/method in the current scene before relying on it.)

2. Read back the launch result and HUD values (rules, `hasCore`, group counts)
   via `get_state_json` / facade getters, then screenshot.

## Success signals

- Planet dialog visible with sector rows; after a successful launch the world
  renders and the HUD shows a core, wave counter, and toolbar.
- No error log entries during the sequence.

## Failure modes

- A click that is never flushed leaves the menu unchanged; the flush in step 2
  is mandatory.
- Hover state is read on the NEXT frame after `godot_input`; use
  `gui_get_hovered_control()` in a follow-up eval, not the same call.
- The UI variant is seeded, not verified: campaign bootstrap/launch gaps
  (EV-0035 family) can make step 3 fail before any click is wrong. Record the
  facade result and stop; do not hunt random coordinates.
- Java comparison: computer-mcp crashes at import without `DISPLAY` (pynput),
  and opencode silently drops its tools; the Godot leg is unaffected.
