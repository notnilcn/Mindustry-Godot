---
id: keybind-rebind-camera
title: Rebind Pan/Boost in Settings > Controls and verify the camera consumes the new keys
status: verified
applies_when: Verifying that a keybind-dialog rebind (move_x/move_y/boost and other camera binds) reaches gameplay, or any Settings > Controls / KeybindDialog UI drive.
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - The standalone menu is visible at boot (or use MindUi.open_dialog("controls","") to skip navigation).
  - Use a target key that is NOT bound elsewhere (J is the default `research` bind and opens the research dialog, which gates the camera through `ui_dialog`).
tools: [godot_exec, godot_input, godot_game, godot_log]
last_verified: 2026-10-08 996f288 (runs/l2-20261008-190017-ev0057-keybind-rebind-godot, pids 83855/96741)
---

# keybind-rebind-camera

EV-0057: `BindingState` must be consumed by the camera (`DesktopInput.java:239-295`
reads `Binding.boost`/`moveX`/`moveY`). This chain drives the real KeybindDialog
capture path, then reads `MindInput.pan_axis()`/`boost_pressed()` and the
`MindCamera2D` position while keys are held.

## Steps

1. Clear the shared log buffer and boot the spine game (`godot_game play` with
   `{"scene":"res://scenes/game.tscn"}`; wait for `runtime_connected: true`).
   Pid-stamp and read the baseline binding table (`move_x` default `a`/`d`,
   `boost` default `shiftLeft`) plus `dialog_stack`:

   ```
   godot_exec {"action":"eval","params":{"code":"var input = get_node(\"/root/Spine/Input\")\nvar kb = JSON.parse_string(str(input.call(\"keybinds_json\")))\nvar mx = kb.filter(func(e): return e[\"name\"] == \"move_x\")[0]\nvar bo = kb.filter(func(e): return e[\"name\"] == \"boost\")[0]\nreturn {\"pid\": OS.get_process_id(), \"move_x\": str(mx), \"boost\": str(bo), \"rebind_count\": input.call(\"rebind_count\")}"}}
   ```

2. Navigate the real UI. Settings is sidebar button index 4 under
   `MenuGroup/menu/Sidebar/Buttons`; the Settings category rail (runtime-built
   buttons at x≈355) has Controls at index 4 (y≈299). Click each with
   motion → press → release in **separate** `godot_input` calls, then flush and
   assert the stack:

   ```
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nreturn str(get_node(\"/root/MindUi\").call(\"dialog_stack\"))"}}
   ```

   Expect `["settings"]` then `["settings","controls"]`.

3. Resolve the KeybindDialog row's Rebind button from the live tree — never a
   hardcoded index. Rows are built in `BINDS` order (move_x is entry 0, boost
   entry 4); the Rebind buttons are the first `Button` children with text
   `settings.rebind`:

   ```
   godot_exec {"action":"eval","params":{"code":"var dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/controls\")\nvar buttons = dlg.find_children(\"*\", \"Button\", true, false)\nvar reb = buttons.filter(func(b): return str(b.text) == \"settings.rebind\" and b.is_visible_in_tree())\nvar c = (reb[0] as Control).get_global_rect().get_center()\nreturn {\"x\": int(c.x), \"y\": int(c.y), \"entries\": str(dlg.get(\"_entries\")[0])}"}}
   ```

   Click it; the capture prompt opens (`dlg.get("_capture_name")` = the bind).
   Then press the new key with discrete down+up `godot_input key` calls.

4. Assert the rebind landed in both the live state and the persisted file:

   ```
   godot_exec {"action":"eval","params":{"code":"var dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/controls\")\nvar input = get_node(\"/root/Spine/Input\")\nvar kb = JSON.parse_string(str(input.call(\"keybinds_json\")))\nvar mx = kb.filter(func(e): return e[\"name\"] == \"move_x\")[0]\nvar file = FileAccess.open(\"user://keybinds.json\", FileAccess.READ)\nreturn {\"capture_open\": dlg.get(\"_capture\") != null, \"move_x\": mx[\"value\"], \"file\": file.get_as_text(), \"rebind_count\": input.call(\"rebind_count\")}"}}
   ```

5. Close the dialogs (`MindUi.close_dialog("controls")` then `("settings")`),
   hide the menu (`UiRoot.set_menu_visible(false)`), and give the bridge one
   frame to refresh `ui_dialog` before trusting `gameplay_input_active`.

6. Center the camera and record a stable baseline, then hold the new key and
   sample position + `Engine.get_process_frames()` twice around a `sleep`:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/World/Camera2D","method":"center_on_tile","args":[10,10]}}
   godot_input {"action":"key","params":{"key":"U","pressed":true}}
   godot_exec {"action":"eval","params":{"code":"var input = get_node(\"/root/Spine/Input\")\nvar cam = get_node(\"/root/Spine/World/Camera2D\")\nvar st = JSON.parse_string(str(cam.call(\"camera_state_json\")))\nvar pa = input.call(\"pan_axis\")\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"pos\": [st[\"position\"][0], st[\"position\"][1]], \"pan\": [pa.x, pa.y], \"boost\": input.call(\"boost_pressed\"), \"active\": input.call(\"gameplay_input_active\")}"}}
   ```

   Repeat after `sleep 1`; per-frame px = Δpos / Δframes. Normal pan is
   `PAN_SPEED*60/fps` (4.5*60/145 ≈ 1.86 px/frame here), boost is 15/4.5 ≈
   3.33×. Release the key with a second `godot_input key` call.

7. Repeat the held-key samples with the old keys (A/D for `move_x`, Shift for
   `boost`): `pan_axis` must stay `[0,0]`, `boost_pressed` false, and the camera
   must not move. Release each key.

8. Optional persistence leg: `godot_game stop` + `play`, then check
   `keybinds_json` still shows the rebound values and the boot log line
   `[I] MindInput ready (88 keybinds, 2 rebinds)`.

## Success signals

- New key: `pan_axis` = `[1,0]` (or `[0,1]` for move_y) while held; camera moves
  at the expected per-frame rate; `boost_pressed` true with the new boost key.
- Old key: `pan_axis` = `[0,0]`, `boost_pressed` false, camera unmoved.
- `user://keybinds.json` carries the new value; restart logs the rebind count.

## Failure modes

- **Key collision.** `J` is the default `research` bind: holding it as the new
  pan key opens the research dialog, sets `ui_dialog`, and pins `pan_axis` to
  `[0,0]` through the `DesktopBridge` gate — looks like the bind is dead. Pick
  an unbound key (U/K/O/L) or unbind the colliding action first.
- **Edge pan drift.** On the loop's Wayland compositor the OS cursor can report
  viewport `(0,0)` and `Input.warp_mouse` is a no-op, so `MouseInput.auto_pan`
  drifts the camera every frame; a held-key delta then mixes pan with drift.
  Judge the reload leg on `pan_axis`/`boost_pressed` and the drift reversal, or
  take the baseline drift first.
- `MindUi.close_dialog` + the next eval may still read the previous frame's
  `ui_dialog`/`text_focus` (bridge flags refresh in `MindInput._process`);
  re-read `gameplay_input_active` after a frame before interpreting.
- Setting `LineEdit.text` directly does not filter the list — the `_search` var
  only updates through `_on_search_changed`; don't rely on programmatic search.
- `move_to_foreground()` (used for screenshot freshness) logs a deprecation
  warning — note it as a known warning, not a run error.
