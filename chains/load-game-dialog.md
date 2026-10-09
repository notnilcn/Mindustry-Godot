---
id: load-game-dialog
title: Main menu → Load Game → live slot list → card click queues + applies a load
status: verified
applies_when: Exercising the Load Game dialog listing (EV-0054 family) or click-to-load from a save card.
preconditions:
  - "boot-and-identity completed (`res://scenes/game.tscn` playing, runtime connected, pid stamped)."
  - "At least one non-sector `.msav` exists under the loop's Godot user dir; create one through the game's own path with `MindCampaign.save_slot` (step 1)."
  - "Game rendering is live: `Engine.get_frames_drawn()` advances between two evals; a frozen value means the viewport is not drawing (see open-godot-mcp-learnings.md) — `godot_game stop` + play first."
  - "Clicks resolve control centers via eval; never hardcode coordinates."
tools: [godot_exec, godot_input, godot_log, godot_screenshot]
last_verified: 2026-10-08 9efd068 (.opencode/evals/runs/l2-20261008-012632-ev0054-load-game-godot; twin re-run runs/l2-20261008-034014-ev0054-load-game-twin, pid 267163)
---

# load-game-dialog

## Steps

1. Create a visible (non-sector) slot through the game's own save path — a
   `spine_place_break` baseline gives a known world to compare against after
   the load:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar loaded = h.load_scenario(\"res://scenarios/spine_place_break.json\")\nvar tick = h.step(60)\nvar save = c.save_slot(\"l2ev54\")\nvar pending = h.io_pending()\nh.step(120)\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"tick\": tick, \"save\": save, \"pending_before\": pending, \"pending_after\": h.io_pending(), \"exists\": FileAccess.file_exists(\"user://saves/l2ev54.msav\")}"}}
   ```

   Expect `save: true`, `pending 1 -> 0`, `exists: true`; the scenario checksum
   at tick 60 is `a1a7b96167c9718d` and the world is 32x32 with one `stone-wall`
   at (5,4). A loop user dir that starts empty is fine: this one slot alone
   supplies the live listing (twin re-run pid 267163 saw exactly 1 card when the
   pre-existing `sector-serpulo-15.msav` fixture had been cleared), and the
   empty state is 0 grid Buttons plus one or more Labels (`@save.none`).

2. Optional but recommended — mutate the live sim so the later load is
   observable, then pause:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar placed = h.place_block(10, 10, \"stone-wall\")\nvar tick = h.step(1)\nh.set_paused(true)\nreturn {\"placed\": placed, \"tick\": tick, \"paused\": h.is_paused()}"}}
   ```

   Do **not** use a second `load_scenario` with a still-running command
   generator (e.g. `spine_determinism`, 600 steps) as the mutation: its player
   keeps emitting commands after the save is applied and logs
   `[E] sim tick failed: tile ... outside a ... world`.

3. Resolve and click the menu `Play` button (index 0 under `Sidebar/Buttons`),
   press+release as two discrete `godot_input` calls, then flush and confirm
   the submenu:

   ```
   godot_exec {"action":"eval","params":{"code":"var menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar b = menu.get_node(\"Sidebar/Buttons\").get_child(0) as Button\nvar r = b.get_global_rect()\nreturn {\"x\": int(r.get_center().x), \"y\": int(r.get_center().y)}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar sm = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu/Submenu\")\nreturn {\"visible\": sm.visible, \"count\": sm.get_node(\"Buttons\").get_children().filter(func(n): return n is Button).size()}"}}
   ```

4. Resolve and click the `Load Game` submenu button — filter `is Button` and
   take index 3 (child 0 of `Submenu/Buttons` is a non-Button spacer, so raw
   indices are off by one), then verify the dialog:

   ```
   godot_exec {"action":"eval","params":{"code":"var menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar lg = menu.get_node(\"Submenu/Buttons\").get_children().filter(func(n): return n is Button)[3] as Button\nvar r = lg.get_global_rect()\nreturn {\"x\": int(r.get_center().x), \"y\": int(r.get_center().y)}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/load\")\nvar slots = dlg.get(\"_slots\")\nvar cards = dlg.get(\"_grid\").get_children().filter(func(n): return n is Button)\nvar empty = dlg.get(\"_grid\").get_children().filter(func(n): return n is Label).size()\nreturn {\"visible\": dlg.visible, \"slots\": slots.map(func(s): return str(s.get(\"file\", \"\")).get_file()), \"cards\": cards.size(), \"empty_labels\": empty}"}}
   ```

   `_slots` is the live `MindCampaign.list_save_slots` listing refreshed on
   `shown()`; the grid's card order matches `_slots` order for the slots that
   pass the sector hidden filter. An empty listing renders exactly one Label
   (`@save.none`), not a Button.

5. Resolve the target card's center (grid Button children, index of the slot in
   `_slots`), click it, then flush and read the queue:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/load\")\nreturn {\"pending_before\": h.io_pending(), \"dialog_visible\": dlg.visible}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar h = get_node(\"/root/Spine/SimHost\")\nvar dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/load\")\nreturn {\"pending_after_click\": h.io_pending(), \"dialog_visible\": dlg.visible}"}}
   ```

   Expect `pending_after_click: 1` (the card called `MindCampaign.load_slot`)
   and `dialog_visible: false`.

6. Drain the load and compare the world against the step-1 snapshot:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar before = h.io_pending()\nh.step(120)\nvar state = JSON.parse_string(h.get_state_json())\nreturn {\"pending_before\": before, \"pending_after\": h.io_pending(), \"tick\": h.get_tick(), \"world_json\": JSON.stringify(state.get(\"world\", {}))}"}}
   ```

   Expect `pending 1 -> 0` and the step-1 world (32x32, single wall at (5,4));
   the step-2 mutation tile at (10,10) is gone.

7. `godot_log {"action":"errors"}` must be empty; clean up the arranged slot
   (`DirAccess.remove_absolute` on `user://saves/<name>.msav` and its
   `-backup.msav`) to leave the user dir as found.

## Success signals

- The dialog lists the live slots as cards (no `@save.none` Label) and the
  click queues a load (`io_pending` 0 -> 1) and closes the dialog.
- After the drain the world matches the saved slot, and the error log is empty.

## Failure modes

- `godot_input {"action":"sequence", ...}` times out on this host (llvmpipe);
  use discrete `mouse_button` calls + `Input.flush_buffered_events()`.
- The submenu re-sorts after `_show_submenu`; re-read the button rect
  immediately before clicking — a stale center from a previous frame can miss.
- A sector save (`is_sector()`, meta rules carry a sector) is hidden and never
  renders a card; `list_save_slots` still returns it.
- The dialog's `_slots`/`_grid`/`cont` are script members reached with
  `Object.get()`; this is an MCP-eval-only observation, not a game API.
- If `Engine.get_frames_drawn()` is frozen, the game viewport is not drawing:
  MCP screenshots and X captures both show stale frames. `godot_game stop` +
  play recovers; the screenshot evidence can also come from the game X window
  (`xwd -id <game window>`, decoded locally).
