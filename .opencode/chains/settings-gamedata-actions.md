---
id: settings-gamedata-actions
title: Menu > Settings > Game Data — dump the nine action rows and drive each action's click path (confirm prompts + backend logs)
status: verified
applies_when: Verifying the Settings > Game Data action list/backends (EV-0024 class), dialog action-button clicks, confirm prompts, or the data export/import/crash-log endpoints.
preconditions:
  - boot-and-identity completed (res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Desktop mode (MindUi.is_mobile() false); the standalone menu is visible and the boot fade is gone.
  - The headless loop has no native file chooser (`DisplayServer.has_feature(DisplayServer.FEATURE_NATIVE_DIALOG)` is false), so Export/Import/Crash-Logs cannot complete their chooser step; drive the endpoints the handlers call instead.
tools: [godot_exec, godot_input, godot_log, godot_screenshot]
last_verified: 2026-10-08 7991049 (runs/l2-20261008-195122-ev0024-gamedata-godot)
---

# settings-gamedata-actions

## Steps

1. Open Settings from the standalone menu. The sidebar buttons are
   runtime-created; resolve by label, not index (desktop index 4):

   ```
   godot_exec {"action":"eval","params":{"code":"var menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar btns = menu.get_node(\"Sidebar/Buttons\").get_children()\nvar matches = btns.filter(func(b):\n\tvar texts = b.find_children(\"*\", \"Label\", true, false).map(func(l): return l.text)\n\treturn texts.has(\"Settings\"))\nvar b = matches[0]\nvar c = (b as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"path\": str(b.get_path()), \"center\": {\"x\": int(c.x), \"y\": int(c.y)}}"}}
   ```

   Then drive one discrete motion/press/release at the center
   (`coords: "viewport"`), flush, and read the stack back:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/settings\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(get_node(\"/root/MindUi\").call(\"dialog_stack\")), \"settings_visible\": dlg.visible, \"selected\": dlg._selected}"}}
   ```

   Expect `["settings"]` and `selected == "game"`.

2. Click the `Game Data` category rail entry (`dlg._rail`, runtime-created
   icon buttons) and verify `selected == "data"` after flushing.

3. Dump the action rows: `dlg._table_host` children are the buttons in order.
   `MindWidgets.icon_button` leaves `Button.text` empty (glyph in the first
   child Label, text in the second), so take the last Label text:

   ```
   godot_exec {"action":"eval","params":{"code":"var dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/settings\")\nvar rows = dlg._table_host.get_children().map(func(b):\n\tvar texts = b.find_children(\"*\", \"Label\", true, false).map(func(l): return l.text)\n\tvar label = b.text if not b.text.is_empty() else (texts[texts.size() - 1] if texts.size() > 0 else \"\")\n\tvar c = (b as Control).get_global_rect().get_center()\n\treturn {\"text\": label, \"visible\": b.visible, \"disabled\": b.disabled, \"center\": {\"x\": int(c.x), \"y\": int(c.y)}})\nreturn {\"pid\": OS.get_process_id(), \"selected\": dlg._selected, \"count\": dlg._table_host.get_children().size(), \"actions\": rows}"}}
   ```

   Expect nine rows in upstream order: Clear Game Data..., Clear Planet Data,
   Clear Saves, Clear Research, Clear Campaign Saves, Export Data, Import Data,
   Open Data Folder, Export Crash Logs.

4. Click an action. Destructive ones raise a confirm prompt as a transient
   ColorRect + panel under `UiRoot/OverlayLayer` (no manifest node). Read it via
   the RichTextLabel body and click Yes/No by their live centers:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar prompts = ui_root.get_node(\"OverlayLayer\").get_children().filter(func(n): return n.find_children(\"*\", \"Button\", true, false).size() > 0)\nif prompts.is_empty():\n\treturn {\"error\": \"no confirm prompt\"}\nvar p = prompts[0]\nvar texts = p.find_children(\"*\", \"RichTextLabel\", true, false).map(func(l): return l.get(\"text\"))\nvar btns = p.find_children(\"*\", \"Button\", true, false)\nvar info = btns.map(func(b):\n\tvar c = (b as Control).get_global_rect().get_center()\n\treturn {\"text\": b.text, \"center\": {\"x\": int(c.x), \"y\": int(c.y)}})\nreturn {\"pid\": OS.get_process_id(), \"prompt_text\": texts, \"buttons\": info}"}}
   ```

   Clear Planet Data does not confirm; it swaps the table to the planet pane
   (Planet selector + Clear Planet Research + Clear Planet Campaign Saves +
   Back) and sets `selected == "planet-data"`. Clear Game Data... and Import
   Data **exit the process** after their backend runs (upstream `Core.app.exit`
   / exit-on-import), so read `godot_log` right after and expect
   `instance_count: 0` from `godot_game status`.

5. Backends when the native chooser is unavailable. Globalize first — the Rust
   endpoints take literal filesystem paths, `user://` is not resolved:

   ```
   godot_exec {"action":"eval","params":{"code":"var ui = get_node(\"/root/MindUi\")\nvar abs_path = ProjectSettings.globalize_path(\"user://eval-data-export.zip\")\nvar export_ok = ui.call(\"data_export\", abs_path)\nreturn {\"pid\": OS.get_process_id(), \"export_ok\": export_ok, \"path\": abs_path}"}}
   ```

   Then assert the artifact from GDScript (`FileAccess.file_exists` / size),
   `ui.call("crash_logs_available")` + `export_crash_logs(<abs>)`, and
   `ui.call("settings_action", "open-folder")`. `data_import(<abs>)` returns
   true only with a `settings.bin` entry in the zip and then quits.

## Success signals

- Nine rows dumped with the upstream labels, all visible/enabled.
- Confirm bodies match the upstream bundle text; backend logs:
  `cleared non-sector saves`, `cleared research`, `cleared campaign saves for
  N planet(s)`, `[ui] cleared game data (...)`, `[ui] exported ...`,
  `[ui] imported N data file(s); restarting`, `[ui] exported crash logs (...)`.
- No `[ui] settings_action '<id>' is not ported` entry.

## Failure modes

- `find_children("*", "Label")` misses the dialog labels: `MindWidgets.label`
  builds a `MindLabel`, which extends `MindRichLabel`/`RichTextLabel`. Use the
  `RichTextLabel` type (or match on any node with a `text` property).
- `MindWidgets.icon_button` stores its text in a child Label; `Button.text` is
  empty. Read the last label, and click through the button's global rect.
- `godot_log {"action":"get","params":{"count":N}}` returns the **oldest** N
  entries, not the newest. Ask for `count` ≥ the buffer size (or use
  `godot_log errors`), otherwise fresh backend lines look missing.
- `user://` passed to `MindUi.data_export/import/export_crash_logs` is written
  as a literal relative `user:/` path under the process CWD. Always
  `ProjectSettings.globalize_path` first.
- Clear Game Data... and Import Data quit the game by design; plan the log
  read-back and the following `godot_game play` accordingly (the log buffer
  spans both processes; pids change).
- `native_dialog` false on the headless weston loop: the Export/Import/Crash
  Logs buttons reach `DisplayServer.file_dialog_show`, which cannot complete;
  this is an environment limitation, not a missing backend.

## Variants

- Open Data Folder: `settings_action("open-folder")` -> `Os.shell_open(data root)`.
- Full upstream coverage: also click Clear Saves/Research/Campaign Saves and
  read the campaign facade's log lines; Clear Planet Research/Saves confirm with
  the planet name via `MindWidgets.markup_format`.
