---
id: paused-dialog-buttons
title: Campaign sector launch -> Escape -> paused dialog entry dump (desktop campaign set) + @objective full text
status: verified
applies_when: Verifying the in-game pause menu entries/visibility (desktop campaign branch), the @objective full-text dialog, or the desktop-vs-mobile campaign split (EV-0063 class).
preconditions:
  - boot-and-identity completed (res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Launch through the campaign facade; `serpulo:15` is the groundZero preset (has a description), `serpulo:170` is a generated sector with no preset.
  - Generated zero-core sectors game-over quickly; call `SimHost.set_paused(true)` in the same eval as the launch and press Escape right after.
  - The pause dialog's `shown()` runs inside `MindUi.open_dialog`'s mutable bind; it must not call back into `MindUi` (EV-0063).
tools: [godot_log, godot_exec, godot_input, godot_screenshot]
last_verified: 2026-10-08 7991049 (runs/l2-20261008-193444-ev0063-paused-dialog-godot)
---

# paused-dialog-buttons

## Steps

1. Clear the log, then launch a campaign sector and park the sim (one eval, so
   no frame can advance between the launch and the pause):

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar loadout = JSON.stringify([{\"item\": \"copper\", \"amount\": 500}, {\"item\": \"lead\", \"amount\": 500}])\nvar ok = c.start_sector_with_loadout(\"serpulo\", 15, loadout)\nroot.call(\"set_menu_visible\", false)\nh.set_paused(true)\nvar state = c.get_sector_state()\nreturn {\"pid\": OS.get_process_id(), \"started\": ok, \"sector\": state.get(\"sector\"), \"campaign\": state.get(\"campaign\"), \"presetDescription_len\": str(state.get(\"presetDescription\", \"\")).length(), \"paused\": h.is_paused()}"}}
   ```

   `presetDescription_len` is 127 for groundZero (the `@objective` gate needs a
   non-empty description). `start_sector("serpulo", 170)` gives 0 and no
   objective, matching the generated-sector branch.

2. One real Escape opens the pause dialog; flush and read the stack:

   ```
   godot_input {"action":"key","params":{"key":"Escape"}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar paused = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/paused\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": ui.call(\"dialog_stack\"), \"paused_visible\": paused != null and paused.visible, \"is_paused\": h.is_paused(), \"tick\": h.get_tick()}"}}
   ```

   Expect `stack == ["paused"]`, `paused_visible` true, `is_paused` true, and
   the tick held.

3. Dump every entry with its text, visibility, disabled flag and global rect
   (recursive `find_child`; the buttons sit under the dialog's own row column):

   ```
   godot_exec {"action":"eval","params":{"code":"var paused = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/paused\")\nvar panel = paused.get_node(\"Center/Panel\") as Control\nvar buttons = paused.find_children(\"*\", \"Button\", true, false).map(func(b):\n\tvar texts = b.find_children(\"*\", \"\", true, false).map(func(n): return str(n.get(\"text\")) if n.get(\"text\") != null else \"\").filter(func(t): return not t.is_empty())\n\tvar direct = str(b.text)\n\tif not direct.is_empty():\n\t\ttexts.push_front(direct)\n\tvar r = (b as Control).get_global_rect()\n\treturn {\"name\": str(b.name), \"texts\": texts, \"rect\": {\"x\": int(r.position.x), \"y\": int(r.position.y), \"w\": int(r.size.x), \"h\": int(r.size.y)}, \"visible\": b.visible, \"disabled\": b.disabled})\nvar pr = panel.get_global_rect()\nreturn {\"pid\": OS.get_process_id(), \"title\": str(paused.get_node(\"Center/Panel/Layout/Title\").text), \"panel_rect\": {\"x\": int(pr.position.x), \"y\": int(pr.position.y), \"w\": int(pr.size.x), \"h\": int(pr.size.y)}, \"buttons\": buttons}"}}
   ```

   Desktop campaign set: `objective` (only with a description), `abandon`,
   `back`, `settings`, `hostserver`, `quit` visible; `planetmap` hidden and
   taking no row space. The panel stays 560x400 inside the viewport.

4. Optional: click `@objective` and read the full-text body:

   ```
   godot_exec {"action":"eval","params":{"code":"var b = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/paused\").find_child(\"objective\", true, false) as Control\nvar c = b.get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"x\": int(c.x), \"y\": int(c.y)}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar ft = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/full_text\")\nvar body = \"\"\nif ft != null:\n\tvar texts = ft.find_children(\"*\", \"\", true, false).map(func(n): return str(n.get(\"text\")) if n.get(\"text\") != null else \"\").filter(func(s): return s.length() > 30)\n\tbody = texts[0] if texts.size() > 0 else \"\"\nreturn {\"pid\": OS.get_process_id(), \"stack\": ui.call(\"dialog_stack\"), \"full_text_visible\": ft != null and ft.visible, \"title\": str(ft.find_child(\"Title\", true, false).text) if ft != null else \"\", \"body_len\": body.length()}"}}
   ```

5. Screenshot (`godot_screenshot {"action":"game"}`) and finish with
   `godot_log {"action":"errors"}`; then close via Escape, `set_paused(false)`,
   `godot_game stop`.

## Success signals

- Escape lands the paused dialog with no panic and no new error entries.
- The desktop campaign set renders in upstream rows (objective+abandon /
  back+settings / hostserver / quit), planetmap hidden.
- The objective click opens `full_text` with the sector preset's description
  (groundZero: 127 chars, "The optimal location to begin once more. …").

## Failure modes

- A dialog `shown()` that calls back into `MindUi` (`is_mobile()`,
  `dialog_stack()`) panics `Gd<T>::bind() failed, already bound` while
  `open_dialog` holds the mutable bind; every later `MindUi` eval then times
  out. Restart the game; defer the callback (`call_deferred`) or resolve the
  value in `_ready` (EV-0063 b568977, EV-0061 4b02254).
- `presetDescription` is empty on a preset sector when the live bundle is not
  loaded: `[assets] ready ok=false` means `assets/sprites/sprites.atlas.json`
  is missing (loop worktrees do not carry the pack; copy the generated atlas
  from the main checkout). The campaign registry itself boots with an empty
  `MemoryBundle`, so the description always comes from the live bundle
  (`sector.<preset>.description`, EV-0063 0185994).
- A generated zero-core sector game-overs before Escape unless the launch eval
  also calls `set_paused(true)`; a `paused` dialog already on the stack keeps
  the sim paused, so close it before the next launch.
- Do not read the entries with `get_node("Center/Panel/Layout/Buttons")`
  children; the dialog builds its own row column inside that HBox. Use
  `find_children("*", "Button", true, false)` (also because `back` stores its
  text on the Button while `MindWidgets.icon_button` entries keep it in child
  Labels).

## Variants

- Non-campaign sector: `@abandon` and `@objective` hide, `@back`/`@settings`/
  `@hostserver`/`@quit` stay.
- Mobile preview (`MindUi.set_mobile_preview(true)`): `@planetmap` becomes
  visible after the deferred gate resolves (one frame).
