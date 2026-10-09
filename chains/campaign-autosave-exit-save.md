---
id: campaign-autosave-exit-save
title: Fresh/resumed serpulo:15 -> launch save -> stepped autosave interval -> pause Quit / campaign_complete @menu / clean-quit dispose exit saves
status: verified
applies_when: Verifying the Saves policy path (EV-0064 family): the launch slot write, playtime accrual (session + planet stats), autosave after `saveinterval`, paused-with-dialog suspension, and `runExitSave` on pause Quit, campaign_complete @menu and application dispose.
preconditions:
  - "boot-and-identity completed (`res://scenes/game.tscn` played with the explicit scene param, runtime_connected true, pid stamped)."
  - "Save Interval minimum: the Settings slider is `sliderPref(\"saveinterval\", 60, 10, 5*120, 10, seconds)` and `Saves.update` keeps upstream's `time > saveinterval * 60`, so the shortest testable period is 10 -> 600 s of playing ticks (>= 36000 ticks). `MindUi.settings_set(\"saveinterval\", 10)` is the real settings path."
  - "A leftover `user://saves/sector-serpulo-15.msav` forces the resume branch; clear it and restart for a fresh launch."
tools: [godot_health, godot_game, godot_exec, godot_input, godot_screenshot, godot_log]
last_verified: 2026-10-09 7f57318 (runs/20261009-003000-ev0064-autosave-playtime-godot)
---

# campaign-autosave-exit-save

## Steps

1. Arrange a clean campaign and the interval, then restart so `MindCampaign`
   boots with no sector save (do not skip the restart: the facade reads the
   settings at boot and the slot listing at boot):

   ```
   godot_exec {"action":"eval","params":{"code":"var ui = get_node(\"/root/MindUi\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar set_ok = ui.call(\"settings_set\", \"saveinterval\", 10)\nvar cleared = c.call(\"clear_planet_campaign_saves\", \"serpulo\")\nreturn {\"pid\": OS.get_process_id(), \"set_ok\": set_ok, \"cleared\": cleared, \"saveinterval\": ui.call(\"settings_get_i32\", \"saveinterval\"), \"exists\": FileAccess.file_exists(\"user://saves/sector-serpulo-15.msav\"), \"slots\": str(c.list_save_slots())}"}}
   godot_game {"action":"stop"}
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   godot_log {"action":"clear"}
   ```

   Expect `saveinterval: 10`, `exists: false`, `slots: []`; re-stamp the pid
   after the restart.

2. Launch the sector and drain the launch save (fresh branch:
   `Control.playNewSector` -> `Saves.saveSector`):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar loaded = h.load_sector(\"serpulo\", 15)\nvar started = c.start_sector(\"serpulo\", 15)\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"started\": started, \"tick\": h.get_tick(), \"sector\": str(c.get_sector_state())}"}}
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar t = h.step(3)\nreturn {\"pid\": OS.get_process_id(), \"tick\": t, \"exists\": FileAccess.file_exists(\"user://saves/sector-serpulo-15.msav\"), \"slots\": str(c.list_save_slots())}"}}
   ```

   Expect `exists: true` and the slot named `sector-serpulo-15`
   (`autosave: true`). Record the host-side sha256/mtime of
   `$PARITY_LOOP_DIR/xdg-data/godot/app_userdata/Mindustry-Godot/saves/sector-serpulo-15.msav`
   (the Godot user dir under the loop's `XDG_DATA_HOME`).

3. Park the sim, change the world, and step across the interval in
   paused-without-dialog chunks (each chunk's delta is consumed by the pump on
   the next frame; `step` applies while paused):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nh.set_paused(true)\nvar placed = h.place_block(128, 100, \"stone-wall\")\nreturn {\"pid\": OS.get_process_id(), \"paused\": h.is_paused(), \"tick\": h.get_tick(), \"placed\": placed}"}}
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").step(10000)"}}   # repeat to >= 36001 ticks
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": h.get_tick(), \"io_pending\": h.io_pending(), \"total_playtime\": c.call(\"get_total_playtime\")}"}}
   ```

   After the interval crosses, the pump queues the write: `io_pending: 1` on
   the first read after the crossing chunk (`godot_log get` shows
   `autosave queued`). Drain with `step(4)` and re-check `io_pending: 0`; the
   file sha must change. `total_playtime` and
   `campaign_views_json("serpulo")` `planets[].playtime_ms` must be > 0 and
   equal.

   ```
   godot_exec {"action":"eval","params":{"code":"var v = JSON.parse_string(str(get_node(\"/root/Spine/MindCampaign\").campaign_views_json(\"serpulo\")))\nvar p = v.get(\"planets\", []).filter(func(x): return str(x.get(\"name\", \"\")) == \"serpulo\")\nreturn {\"pid\": OS.get_process_id(), \"planet_playtime_ms\": (p[0].get(\"playtime_ms\", -1) if p.size() > 0 else -2), \"complete_playtime_ms\": v.get(\"complete\", {}).get(\"playtime_ms\", -1)}"}}
   ```

4. Paused-with-dialog suspension: hide the menu, open the real pause dialog,
   step a sub-interval amount; playtime must not move and the file must not be
   rewritten:

   ```
   godot_exec {"action":"eval","params":{"code":"get_node(\"/root/Spine/Ui/UiRoot\").call(\"set_menu_visible\", false)\nreturn \"ok\""}}
   godot_input {"action":"key","params":{"key":"Escape"}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar paused = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/paused\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"has_dialog\": ui.call(\"has_dialog\"), \"is_paused\": h.is_paused(), \"paused_visible\": paused != null and paused.visible}"}}
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").step(600)"}}
   ```

   Expect `stack == ["paused"]`, `has_dialog: true`, `is_paused: true`; sha and
   playtime unchanged after the step.

5. Pause Quit exit save (`PausedDialog.showQuitConfirm` -> `runExitSave`):
   resolve the button centers (never hardcode), then click Quit and Yes:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar q = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/paused\").find_child(\"quit\", true, false) as Control\nvar c = q.get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"x\": int(c.x), \"y\": int(c.y)}"}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":X,"y":Y},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar overlay = get_node(\"/root/Spine/Ui/UiRoot/OverlayLayer\")\nvar yes = overlay.find_children(\"*\", \"Button\", true, false).filter(func(b): return str(b.get(\"text\")) == \"Yes\")\nvar c = (yes[0] as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"x\": int(c.x), \"y\": int(c.y)}"}}
   godot_input {"action":"mouse_motion","params":{"position":{"x":X2,"y":Y2},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X2,"y":Y2},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X2,"y":Y2},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"menu_visible\": root.get_node(\"MenuGroup\").visible, \"total_playtime\": c.call(\"get_total_playtime\"), \"sector_state\": str(c.get_sector_state())}"}}
   ```

   Expect `stack: []`, `menu_visible: true`, `total_playtime: 0`,
   `sector_state.campaign: false`, the file sha changed and the game log shows
   `saved on exit`.

6. campaign_complete `@menu` repeats it: resume (`load_sector` +
   `start_sector`; `hasSave: true`), `set_paused(true)`, `step(120)` twice
   (the second delta proves the resumed slot is `current` via
   `get_total_playtime() > 0`), then `MindUi.open_dialog("campaign_complete",
   "{}")` and click the button whose text is `Menu`. Expect the same menu/0
   playtime state and another `saved on exit` (mtime advances even when the
   re-serialized bytes match).

7. Application dispose (`Control.dispose` -> `MindCampaign.exit_tree`):
   while in-game, record the file sha, then quit cleanly. `godot_game stop`
   kills the process **without** a scene teardown, so it does not exercise
   this path:

   ```
   godot_exec {"action":"eval","params":{"code":"get_tree().quit()\nreturn \"quitting\""}}
   ```

   The eval may not return; check the file sha/mtime and the third
   `saved on exit` line in `$PARITY_LOOP_DIR/logs/editor.log`.

## Success signals

- Launch write lands with the sector slot name/autosave persisted; playtime
  (session, planet stats and CampaignViews) is > 0 and equal.
- `io_pending` 0 -> 1 at the interval, 0 after the drain, file sha changes
  exactly at the autosave; paused-with-dialog leaves playtime and the file
  untouched.
- Pause Quit / campaign_complete `@menu` / clean quit each write once more
  (`saved on exit`), land on the menu, and reset `current`/playtime.
- `godot_log errors` empty.

## Failure modes

- The first `update_policy` call only seeds its timestamp: a single chunked
  `step(N)` after launch shows playtime 0; step a second delta (or play
  normally) before asserting > 0.
- `saveinterval=1` is not reachable from the UI (clamped to 10); a 3600-tick
  run cannot cross the interval even with the fix. Use >= 36000 ticks.
- Calling a Rust-only method (`tile_region`) from an eval errors
  (`Nonexistent function`) and pins the frame loop; recover with
  `godot_game stop` + `play` and probe tiles through
  `MindCampaign.write_schematic_selection`.
- `godot_game stop` does not run `exit_tree`; only a clean
  `get_tree().quit()` exercises the dispose save.
