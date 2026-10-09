---
id: pause-save-dialog
title: Non-campaign pause menu -> @savegame -> @save.new prompt -> MindCampaign.save_slot -> io_pending drain + user://saves/<name>.msav
status: verified
applies_when: Verifying the manual save path (EV-0066 family): the desktop pause-save/load entries, the Save dialog live slot listing, the new-save name prompt and the @save.overwrite confirm.
preconditions:
  - boot-and-identity completed (res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - A non-campaign, non-editor session: `SimHost.load_scenario` for a live world plus `MindCampaign.start_map(<builtin>)` (or a real Play > Custom Game launch). `get_sector_state()` must report campaign=false and editor=false.
  - The game menu hidden (`UiRoot.set_menu_visible(false)`) or Escape is swallowed by the menu.
tools: [godot_log, godot_exec, godot_input, godot_screenshot]
last_verified: 2026-10-09 2027fbe (runs/l2-20261009-000205-ev0066-save-menu-godot)
---

# pause-save-dialog

## Steps

1. Clear the log and arrange the non-campaign session with a known world
   (loop-free eval; `load_scenario` + `step(60)` keeps the committed golden):

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar loaded = h.load_scenario(\"res://scenarios/spine_place_break.json\")\nvar tick = h.step(60)\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"tick\": tick, \"checksum\": str(h.get_checksum())}"}}
   ```

   Expect `loaded: true`, `tick: 60`, `checksum: a1a7b96167c9718d`.

2. Start a non-campaign map from the live registry, hide the menu, park the sim:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar names: Array = get_node(\"/root/Spine/MindPreview\").maps_list().map(func(m): return str(m.get(\"name\", \"\")))\nvar started = c.start_map(names[0])\nvar state = c.get_sector_state()\nroot.call(\"set_menu_visible\", false)\nh.set_paused(true)\nreturn {\"pid\": OS.get_process_id(), \"map\": names[0], \"started\": started, \"campaign\": state.get(\"campaign\"), \"editor\": state.get(\"editor\"), \"paused\": h.is_paused(), \"tick\": h.get_tick()}"}}
   ```

   Expect `started: true`, `campaign: false`, `editor: false`, `paused: true`.

3. One real Escape opens the pause dialog; flush and dump the entry set:

   ```
   godot_input {"action":"key","params":{"key":"Escape"}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar paused = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/paused\")\nvar entries = paused.find_children(\"*\", \"Button\", true, false).map(func(b): return {\"name\": str(b.name), \"visible\": b.visible, \"center\": {\"x\": int((b as Control).get_global_rect().get_center().x), \"y\": int((b as Control).get_global_rect().get_center().y)}})\nreturn {\"pid\": OS.get_process_id(), \"stack\": get_node(\"/root/MindUi\").call(\"dialog_stack\"), \"entries\": entries}"}}
   ```

   Expect `savegame` and `loadgame` visible (with `back`/`settings`/`hostserver`/`quit`);
   `objective`/`abandon`/`planetmap` hidden in a plain non-campaign game.

4. Click `savegame` (resolved center, one press + one release), then verify the
   dialog and its live listing:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar save = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/save\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": get_node(\"/root/MindUi\").call(\"dialog_stack\"), \"save_visible\": save != null and save.visible, \"rows\": save.get(\"_grid\").get_children().filter(func(n): return n is Button).map(func(b): return str(b.text))}"}}
   ```

   Expect `stack: ["paused","save"]` and `rows[0] == "New Save"`; a fresh user dir has
   no slot rows (`_slots` is rebuilt from `MindCampaign.list_save_slots()` on every
   `shown()`).

5. Click the `New Save` row (grid Button 0), then fill the prompt. **Do not use
   `godot_input action=text`**: this addon's `_input_text` parses the same
   `InputEventKey` twice in one frame and the engine drops it with an error. Set the
   field in an eval and emit its `text_changed` so the prompt's OK enables:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar overlay = get_node(\"/root/Spine/Ui/UiRoot/OverlayLayer\")\nvar f = overlay.find_children(\"*\", \"LineEdit\", true, false)[0]\nf.text = \"evalslot\"\nf.text_changed.emit(str(f.text))\nvar ok = overlay.find_children(\"*\", \"Button\", true, false).filter(func(b): return str(b.text) == \"OK\")[0] as Control\nvar c = ok.get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"ok_disabled\": ok.disabled, \"x\": int(c.x), \"y\": int(c.y)}"}}
   ```

6. Click OK; flush and read the queue:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar h = get_node(\"/root/Spine/SimHost\")\nvar save = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/save\")\nreturn {\"pid\": OS.get_process_id(), \"pending\": h.io_pending(), \"save_visible\": save != null and save.visible, \"stack\": get_node(\"/root/MindUi\").call(\"dialog_stack\")}"}}
   ```

   Expect `pending: 1`, `save_visible: false`, `stack: ["paused"]`.

7. Drain and assert the file + listing (`step` runs while paused; the IO queue
   applies at tick boundaries):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar before = h.io_pending()\nh.step(120)\nvar c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"pending_before\": before, \"pending_after\": h.io_pending(), \"tick\": h.get_tick(), \"exists\": FileAccess.file_exists(\"user://saves/evalslot.msav\"), \"slots\": c.list_save_slots().map(func(s): return {\"file\": str(s.get(\"file\", \"\")).get_file(), \"sector\": s.get(\"sector\")})}"}}
   ```

   Expect `pending 1 -> 0`, `exists: true`, one non-sector `evalslot.msav` row.

8. Optional overwrite path: re-open `savegame`, click the listed slot row; the
   `@save.overwrite` prompt appears (`Yes`/`No`); `Yes` queues another save, `No`
   cancels with `io_pending` still 0 and the save dialog open.

9. Screenshot (`godot_screenshot {"action":"game"}`) at the pause menu, the save
   dialog and the overwrite prompt; finish with `godot_log {"action":"errors"}`
   (empty). Clean up the arranged slot before `godot_game stop`.

## Success signals

- The pause dialog shows `@savegame`/`@loadgame` for a non-campaign, non-editor
  session and the save dialog lists `@save.new` plus the live slots.
- The new-save prompt name reaches `MindCampaign.save_slot`: `io_pending` 0 -> 1 -> 0,
  `user://saves/<name>.msav` exists and `list_save_slots()` returns it.
- The existing-slot click shows `@save.overwrite`; No cancels without queueing.
- Error log empty; a fresh user dir had no slot before the run.

## Failure modes

- `Escape` does nothing while `MenuGroup` is visible; call
  `UiRoot.set_menu_visible(false)` first.
- `godot_input action=text` does not land (engine logs "input event object is
  being parsed more than once"); use the step-5 eval fallback.
- If `save_slot` is unreachable (the pre-fix state), `pending` stays 0 after OK and
  no file appears — that is the finding, not the harness.
- The save dialog can only be opened from this pause entry (manifest
  `menu_openable: false`); `UiRoot` must not be in menu state.
