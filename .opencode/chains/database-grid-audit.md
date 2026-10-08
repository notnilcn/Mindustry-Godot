---
id: database-grid-audit
title: Menu -> Database submenu -> Core Database -> planet tabs + category/tag lock grid dump -> unlocked cell opens ContentInfoDialog
status: verified
applies_when: Verifying the Core Database dialog (EV-0028 class): the planet tab set (All/Erekir/Serpulo), the populated databaseCategory/databaseTag content grid with locked/unlocked cells, tab filtering, and the unlocked-cell click path.
preconditions:
  - boot-and-identity completed (res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Menu visible with the boot fade gone; `MindCampaign` ready (it owns `database_json`).
  - The dialog's runtime rows are named `tab-<planet>`, `grid-<category>-<tag>` and `content-<name>`; `find_children` needs `owned=false` for those runtime nodes.
tools: [godot_log, godot_exec, godot_input, godot_screenshot]
last_verified: 2026-10-08 b656e03 (runs/l2-20261008-201236-ui_dialogs-godot)
---

# database-grid-audit

## Steps

1. Clear the log, stamp the pid and confirm the standalone menu is interactive:

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar fade = ui_root.get_node_or_null(\"OverlayLayer/fade_in\")\nreturn {\"pid\": OS.get_process_id(), \"menu_visible\": ui_root.get_node(\"MenuGroup\").visible, \"fade_visible\": (fade != null and fade.visible), \"stack\": str(get_node(\"/root/MindUi\").call(\"dialog_stack\")), \"frames\": Engine.get_process_frames()}"}}
   ```

2. Resolve the Database sidebar entry (child 1 of `Sidebar/Buttons`; the Button
   text is empty because the label lives in a child row) and click it:

   ```
   godot_exec {"action":"eval","params":{"code":"var menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar b = menu.get_node(\"Sidebar/Buttons\").get_child(1)\nvar c = (b as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"center\": {\"x\": int(c.x), \"y\": int(c.y)}, \"frames\": Engine.get_process_frames()}"}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":CX,"y":CY},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":false}}
   ```

3. Flush, confirm the submenu and resolve Core Database (child 2: spacer,
   Schematics, Core Database, About) then click it:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar b = menu.get_node(\"Submenu/Buttons\").get_child(2)\nvar c = (b as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"submenu_visible\": menu.get_node(\"Submenu\").visible, \"label\": b.get_child(0).get_child(1).text, \"center\": {\"x\": int(c.x), \"y\": int(c.y)}}"}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":CX,"y":CY},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":false}}
   ```

   `label` is `Core Database`; the resolved center matches the finding's repro
   coordinate (460,289) at 1152x648.

4. Flush and dump the tabs, category/tag headers, and every grid with its cell
   count (named runtime nodes; `owned=false`):

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/database\")\nvar tabs = dlg.find_children(\"tab-*\", \"Button\", true, false)\nvar entries = dlg.find_children(\"content-*\", \"Button\", true, false)\nvar grids = dlg.find_children(\"grid-*\", \"HFlowContainer\", true, false)\nvar labels = dlg.find_children(\"\", \"RichTextLabel\", true, false)\nreturn {\"pid\": OS.get_process_id(), \"visible\": dlg.visible, \"stack\": str(get_node(\"/root/MindUi\").call(\"dialog_stack\")), \"tabs\": tabs.map(func(b): return {\"name\": str(b.name), \"text\": b.text, \"checked\": b.button_pressed}), \"labels\": labels.map(func(l): return l.text), \"grids\": grids.map(func(g): return {\"name\": str(g.name), \"cells\": g.get_child_count()}), \"entries\": entries.size(), \"unlocked\": entries.filter(func(b): return bool(b.get_meta(\"unlocked\", false))).size(), \"locked\": entries.filter(func(b): return not bool(b.get_meta(\"unlocked\", false))).size(), \"unlocked_names\": entries.filter(func(b): return bool(b.get_meta(\"unlocked\", false))).map(func(b): return String(b.name).trim_prefix(\"content-\"))}"}}
   ```

   At b656e03 the All tab is 321 entries / 15 grids with 5 unlocked roots
   (copper, sand, core-bastion, water, alpha); the headers are Items, Blocks,
   Fluids, Units and the Block tags.

5. Tab filter: resolve the `tab-serpulo` button, click it, re-dump; entries drop
   to 189 and the checked tab becomes Serpulo.

6. Click an unlocked cell (e.g. `content-copper`). Scroll the Body to 0 in one
   eval, then resolve the center in the **next** call (the scroll re-layout
   applies a frame later; a same-eval `get_global_rect()` returns the pre-scroll
   position):

   ```
   godot_exec {"action":"eval","params":{"code":"var dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/database\")\nvar b = dlg.find_child(\"content-copper\", true, false)\nvar c = (b as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"unlocked\": bool(b.get_meta(\"unlocked\", false)), \"center\": {\"x\": int(c.x), \"y\": int(c.y)}}"}}
   ```

   Then the motion/press/release trio at that center, flush, and read back:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar content = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/content\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"content_visible\": (content != null and content.visible), \"content_context\": (str(content.context()) if content != null else \"\")}"}}
   ```

   Expect `["database","content"]`, the dialog visible, and the content name in
   the context; the dialog's own Back button returns to `["database"]`.

7. Screenshot (`godot_screenshot {"action":"game"}` after
   `get_window().move_to_foreground()`), then `godot_log {"action":"errors"}`
   and `godot_game stop`.

## Success signals

- Tabs are the planet set (`All`, `Erekir`, `Serpulo`), not ContentType names,
  with the planet colors on the planet tabs.
- Every grid is non-empty and named `grid-<category>-<tag>`; locked cells carry
  `meta("unlocked") == false`, unlocked cells open `ContentInfoDialog`.
- Switching tabs re-filters entries (All 321 -> Serpulo 189 at b656e03).
- No error entries from the dialog (the boot `[shaders] index missing` and
  menu-background warnings pre-date the run).

## Failure modes

- `find_children("content-*", "Button", true, false)` is empty when `owned`
  defaults to true: runtime-created nodes have no owner.
- Same-eval scroll + resolve returns a stale center (observed y=-64 before the
  scroll re-layout): resolve the center in a follow-up call.
- `move_to_foreground()` logs an error-level deprecation entry on Godot 4.7.2;
  use it only for screenshots and do not count it as a dialog error.
- The shared dialog panel keeps its 560x400 minimum from
  `mind_dialog_base.tscn`; scroll `Center/Panel/Layout/Body` to audit tag
  sections below the fold.

## Variants

- Search filter: set `LineEdit.text` + emit `text_changed` via eval when MCP
  text injection does not reach the field (see `open-godot-mcp-learnings.md`).
