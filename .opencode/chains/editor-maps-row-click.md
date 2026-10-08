---
id: editor-maps-row-click
title: Menu > Editor > map row click opens the statically-instanced map editor with the map loaded
status: verified
applies_when: Verifying the Editor > Maps registry and the row -> map-editor flow (`ui.editor.show()`), or any EV-0038-class regression where a runtime-created map row must reach `MindEditor.begin_edit_map`.
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Standalone menu visible and the boot fade hidden (MenuGroup.visible true, OverlayLayer/fade_in hidden).
  - `MindPreview.map_count() > 0`; only a non-empty live registry gives rows with real `path` values (empty registry falls back to name-only fixtures).
tools: [godot_exec, godot_input, godot_log, godot_screenshot]
last_verified: 2026-10-08 afd380f (runs/20261008-190113-ev0038-editor-row-godot); 2026-10-08 1d8f75b twin (runs/20261008-121029-ev0038-editor-row-twin)
---

# editor-maps-row-click

## Steps

1. Clear the log and stamp the runtime; wait out the boot fade:

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar fade = ui_root.get_node_or_null(\"OverlayLayer/fade_in\")\nvar preview = get_node_or_null(\"/root/Spine/MindPreview\")\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"menu_visible\": ui_root.get_node(\"MenuGroup\").visible, \"fade_visible\": (fade != null and fade.visible), \"map_count\": (int(preview.call(\"map_count\")) if preview != null else -1)}"}}
   ```

   Require `menu_visible` true, `fade_visible` false, `map_count` 18 on this
   checkout.

2. Resolve the menu `Editor` button through its label (the runtime buttons carry
   the display text in a `Label` grandchild, not in `Button.text`) and click it:

   ```
   godot_exec {"action":"eval","params":{"code":"var menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar labels = menu.find_children(\"*\", \"Label\", true, false)\nvar hits = labels.filter(func(l): return String(l.text) == \"Editor\")\nif hits.is_empty():\n\treturn {\"pid\": OS.get_process_id(), \"found\": false, \"labels\": labels.map(func(l): return String(l.text))}\nvar b: Control = hits[0].get_parent().get_parent()\nvar c = b.get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"found\": true, \"path\": str(b.get_path()), \"owned\": b.owner != null, \"center\": {\"x\": int(c.x), \"y\": int(c.y)}}"}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":CX,"y":CY},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":false}}
   ```

3. Flush and confirm the Maps dialog opened with live rows; resolve the target
   row from the live registry by name (do not assume fixture order — the live
   registry sorts by plain name, PvP maps last, so `Archipelago` is the second
   row and `Ancient Caldera` the first):

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar dlg = ui_root.get_node_or_null(\"DialogLayer/editor_maps\")\nvar preview = get_node(\"/root/Spine/MindPreview\")\nvar rows = preview.call(\"maps_list\")\nvar target = \"Archipelago\"\nvar buttons = dlg.find_children(\"*\", \"Button\", true, false)\nvar hits = buttons.filter(func(b): return String(b.text) == target)\nif hits.is_empty():\n\treturn {\"pid\": OS.get_process_id(), \"found\": false, \"stack\": str(ui.call(\"dialog_stack\"))}\nvar b: Control = hits[0]\nvar c = b.get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"editor_maps_visible\": (dlg != null and dlg.visible), \"row_count\": rows.size(), \"first_row\": str(rows[0].get(\"name\", \"\")), \"path\": str(b.get_path()), \"center\": {\"x\": int(c.x), \"y\": int(c.y)}}"}}
   ```

4. Click the row at the resolved center (same three `godot_input` calls as step
   2), then flush and read the editor back. The shell is
   `/root/Spine/Ui/EditorDialog` (a **sibling** of `UiRoot`), and the model is
   `/root/Spine/MindEditor`:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar editor = get_node(\"/root/Spine/MindEditor\")\nvar dlg = ui_root.get_node_or_null(\"DialogLayer/editor_maps\")\nvar shell = get_node_or_null(\"/root/Spine/Ui/EditorDialog\")\nvar status = editor.call(\"status\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"editor_maps_visible\": (dlg != null and dlg.visible), \"editor_dialog_visible\": (shell != null and shell.visible), \"editor_status\": {\"file\": str(status.get(\"file\", \"\")), \"width\": float(status.get(\"width\", 0)), \"height\": float(status.get(\"height\", 0)), \"last_error\": str(status.get(\"last_error\", \"\"))}, \"last_error\": str(editor.call(\"last_error\"))}"}}
   godot_screenshot {"action":"game","params":{"format":"png"}}
   ```

5. Check the log stayed clean:

   ```
   godot_log {"action":"errors","params":{"include_warnings":true,"max":50}}
   ```

## Success signals

- `dialog_stack == ["editor_maps"]` with `editor_maps.visible == true` and 18
  registry rows after the menu Editor click.
- After the row click: `dialog_stack == []`, `editor_maps.visible == false`,
  `EditorDialog.visible == true`, and `MindEditor.status()` has a non-empty
  `file` (e.g. `.../assets/maps/default/archipelago.msav`), `width`/`height` > 0
  and `last_error == ""` (`last_error()` returns the empty dict `{}`).
- `godot_log errors` stays empty — in particular no
  `failed to read meta ... Unknown save version` entry.

## Failure modes

- **Wrong shell path.** `EditorDialog` is not under `UiRoot`; reading
  `/root/Spine/Ui/UiRoot/EditorDialog` returns null and looks like the dialog
  never opened. Use `/root/Spine/Ui/EditorDialog`.
- **Fixture rows.** If `MindPreview.map_count() == 0` (EV-0039 class), the list
  still shows name-only fixture rows and the row click cannot resolve a real
  path; assert the live registry before judging the click.
- **Row order.** The live registry is sorted by plain name with PvP maps last
  (`Ancient Caldera` first, `Archipelago` second); the old fixtures listed
  `archipelago` first. Resolve rows by name, not by index/coords.
- **Runtime-Control clicks.** The menu and row buttons are code-created
  (`owner == null`); a press that only hovers means the EV-0062 input capture
  regressed. Flush with `Input.flush_buffered_events()` and read back the
  dialog stack before judging.
- **Scroll.** Rows 16+ (Glacier/Passage/Veins) sit below a 648 px window; click
  the first visible rows or scroll the list.
- **Screenshot staleness vs layering.** Captures can stay near-identical across
  different loaded maps and grid toggles: `scenes/editor/map_view.tscn` layers a
  full-rect `SubViewportContainer`/`EditorWorldView` over the `MapView` script's
  `_draw`, so terrain/border/grid never composite into the window (plan-19
  render stub; twin run: 500x500 vs 256x256 map frames differ in 31/746496 px).
  Prove capture liveness with a modulate eval on `EditorDialog` before blaming a
  stale frame; judge the flow on the structured editor status.
- **EditorDialog shell covers UiRoot dialogs.** While
  `/root/Spine/Ui/EditorDialog` is visible, it is layered above the UiRoot
  DialogLayer. Opening `editor_maps` underneath via
  `MindUi.open_dialog("editor_maps", "{}")` leaves the list input-dead: the row
  click never fires and `gui_get_hovered_control()` at the row center returns
  `/root/Spine/Ui/EditorDialog/MapView`. Exit the editor with
  `LeftTools/BackButton` before clicking a row again.

## Variants

- Back out of the editor with `LeftTools/BackButton` (`_exit_to_maps`): the
  map list reopens and a second row click can be exercised in the same run (the
  row buttons are rebuilt — resolve the center again). Do not re-open the list
  with `MindUi.open_dialog` while the editor shell is up (see failure modes).
- Import/Export go through the `file_chooser` dialog and are not part of the
  row -> editor flow.
