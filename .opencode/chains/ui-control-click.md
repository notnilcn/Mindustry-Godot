---
id: ui-control-click
title: Click a runtime-created UI Control and verify pressed via read-back (menu Play, paused Settings)
status: verified
applies_when: Verifying that an injected mouse press reaches a runtime-created (ownerless) STOP-filter Button — menu buttons, dialog action buttons — instead of being swallowed by MindInput world input (EV-0062 class).
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Menu flow: the standalone menu is visible at boot (MenuGroup.visible true); wait for the boot fade (OverlayLayer/fade_in) to hide before clicking.
tools: [godot_exec, godot_input, godot_log, godot_screenshot]
last_verified: 2026-10-08 a1ca816 (runs/20261008-133538-ev0062-ui-click-godot; twin re-run runs/20261008-143432-ev0062-twin, pid 145410)
---

# ui-control-click

## Steps

1. Clear the log, take a pid-stamped baseline, and assert the frame loop is
   live (`Engine.get_process_frames()` advances between calls):

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar fade = ui_root.get_node_or_null(\"OverlayLayer/fade_in\")\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"menu_visible\": ui_root.get_node(\"MenuGroup\").visible, \"fade_visible\": (fade != null and fade.visible)}"}}
   ```

2. Resolve the target Button's live center and arm a pressed counter (runtime
   instrumentation, no game-code edit). Menu Play is the first child of the
   runtime-built sidebar (`Button.new()` in `menu_fragment.gd`, so
   `owner == null` — the exact EV-0062 case):

   ```
   godot_exec {"action":"eval","params":{"code":"var menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar b = menu.get_node(\"Sidebar/Buttons\").get_child(0)\nif not b.has_meta(\"click_armed\"):\n\tb.set_meta(\"click_armed\", true)\n\tb.set_meta(\"click_pressed\", 0)\n\tb.pressed.connect(func() -> void: b.set_meta(\"click_pressed\", int(b.get_meta(\"click_pressed\")) + 1))\nvar c = (b as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"path\": str(b.get_path()), \"owned\": b.owner != null, \"center\": {\"x\": int(c.x), \"y\": int(c.y)}, \"pressed\": int(b.get_meta(\"click_pressed\"))}"}}
   ```

3. Paused-dialog leg: hide the menu, press Escape, then locate Settings with
   `find_child` — do not assume a path (the button row is
   `.../paused/Center/Panel/Layout/Buttons/settings`, not `.../paused/settings`):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Ui/UiRoot","method":"set_menu_visible","args":[false]}}
   godot_input {"action":"key","params":{"key":"Escape","pressed":true}}
   godot_input {"action":"key","params":{"key":"Escape","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar paused = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/paused\")\nvar s = paused.find_child(\"settings\", true, false)\nvar c = (s as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"path\": str(s.get_path()), \"owned\": s.owner != null, \"center\": {\"x\": int(c.x), \"y\": int(c.y)}, \"stack\": str(get_node(\"/root/MindUi\").call(\"dialog_stack\"))}"}}
   ```

4. Drive the click at the resolved center with `coords: "viewport"`, one
   discrete call per event (do not split a pair into a batch):

   ```
   godot_input {"action":"mouse_motion","params":{"position":{"x":CX,"y":CY},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":false}}
   ```

5. Flush and read the structured outcome back (pressed counter, submenu,
   dialog stack) plus liveness:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar b = menu.get_node(\"Sidebar/Buttons\").get_child(0)\nvar hovered = get_tree().root.gui_get_hovered_control()\nreturn {\"pid\": OS.get_process_id(), \"hovered\": (\"\" if hovered == null else str(hovered.get_path())), \"pressed\": int(b.get_meta(\"click_pressed\", -1)), \"submenu_visible\": menu.get_node(\"Submenu\").visible, \"submenu_children\": menu.get_node(\"Submenu/Buttons\").get_child_count(), \"stack\": str(get_node(\"/root/MindUi\").call(\"dialog_stack\")), \"frames\": Engine.get_process_frames()}"}}
   ```

## Success signals

- `pressed` increments on the clicked runtime-created Button (`owner == null`);
  the hovered path equals the resolved path before the press.
- Menu Play: `submenu_visible` flips true with 5 children; `dialog_stack` stays
  empty.
- Paused Settings: `dialog_stack` becomes `["paused", "settings"]`, the settings
  dialog is visible and paused is hidden.
- `frames` advanced across every read-back; `godot_log errors` is empty.

## Failure modes

- A missed click on the wrong control: always resolve the center from
  `get_global_rect().get_center()`. While paused, the viewport center is covered
  by the STOP `HudGroup/hud/PausedBanner` and the lower-right by
  `HudGroup/placement/Panel`; read `gui_get_hovered_control()` before judging a
  missed UI or world click.
- An eval body that errors (e.g. `get_node` of a nonexistent dialog button path)
  trips break-on-error and stalls the frame loop: check
  `Engine.get_process_frames()` twice, then `godot_game stop` + `play` (restart,
  do not resume).
- `set_menu_visible(false)` shows HudGroup but the `menu` fragment's own
  `.visible` stays true; assert MenuGroup/HudGroup, not the fragment.
- While the standalone menu is visible, Escape does not open the paused dialog
  (`UiRoot._on_escape` returns false); hide the menu first.
- `gui_get_hovered_control()` is `""` after the settings dialog opens (the
  pointer sits under the new dialog), so read the stack/visibility, not hover,
  for the post-click assertion.
