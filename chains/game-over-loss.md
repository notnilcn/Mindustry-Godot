---
id: game-over-loss
title: Force the campaign loss (zero-core sector 170) and assert the game-over dialog opens without the MindUi bind_mut panic
status: verified
applies_when: Verifying the GameOverEvent -> GameOverDialog path (EV-0061 family), the deferred HUD toggle, or any dialog opened from a live HUD watcher; also the fastest way to reach a real game-over in-engine without UI navigation.
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - "`/root/MindHud` and `/root/MindUi` are autoloads at the tree root, NOT under /root/Spine: addressing /root/Spine/MindHud errors and an eval error trips break-on-error (frame loop pins, later evals time out)."
  - "serpulo sector 15 is the groundZero preset (has a core); sector 170 is the preset-less generated sector, so it has hasCore=false and loses on the first GameStateCheck."
tools: [godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_log]
last_verified: 2026-10-08 1d8f75b twin (runs/20261008-215055-ev0061-game-over-twin, pids 78253/80270); 2026-10-08 afd380f writer (runs/20261008-185511-ev0061-game-over-godot, pid 74313)
---

# game-over-loss

## Steps

1. Attach and boot (boot-and-identity): health check, editor identity, open
   `res://scenes/game.tscn`, `play` with the scene passed explicitly, wait for
   `runtime_connected: true`.

2. `godot_log {"action":"clear"}` then pid-stamp:

   ```
   godot_exec {"action":"eval","params":{"code":"return {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"frames\": Engine.get_process_frames(), \"window\": str(DisplayServer.window_get_size())}"}}
   ```

3. Load the preset-less sector on the sim host (tick resets to 0):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nreturn {\"pid\": OS.get_process_id(), \"loaded\": h.load_sector(\"serpulo\", 170), \"tick\": h.get_tick()}"}}
   ```

4. Start the campaign sector (installs the live `CampaignRuntime`):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"started\": c.start_sector(\"serpulo\", 170), \"sector_state\": c.get_sector_state(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick()}"}}
   ```

   Expect `started: true`, `campaign: true`, `hasCore: false`.

5. Let the pump run (or pause + `step(600)` as a deterministic variant), then
   poll the game-over edge and the dialog. Note the autoload paths:

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/MindHud\")\nvar u = get_node(\"/root/MindUi\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"frames\": Engine.get_process_frames(), \"hud\": c.get_hud_state(), \"game_over\": bool(h.get(\"game_over\")), \"has_dialog\": u.call(\"has_dialog\"), \"stack\": str(u.call(\"dialog_stack\")), \"hud_visible\": u.call(\"hud_visible\")}"}}
   ```

   Expect `game_over: true`, `hasCore: false`, `has_dialog: true`,
   `stack == "[\"restart\"]"` (the manifest name of `game_over_dialog`),
   `hud_visible: false`.

6. Prove liveness and no panic: repeat the frames/tick read after a few seconds
   (both must increase), then `godot_log {"action":"errors","params":{"include_warnings":true}}`
   (must be `[]`) and check for a new
   `$PARITY_LOOP_DIR/xdg-data/Mindustry-Godot/crashes/crash-report-*.txt`
   (there must be none newer than the run start).

7. Optional render-state read (structured, no OCR needed):

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/restart\")\nreturn {\"pid\": OS.get_process_id(), \"dialog_node\": d != null, \"visible\": (d != null and d.visible), \"size\": (str(d.size) if d != null else \"\")}"}}
   ```

8. Teardown: `godot_game {"action":"stop"}`.

## Success signals

- `game_over=true` with `hasCore=false`; the HUD watcher opened the dialog by
  itself (stack `["restart"]`, `is_dialog_shown("restart")=true`, node visible).
- `hud_visible=false` while the dialog is open: the deferred `hud_set_visible`
  ran without re-entering the mutably bound `MindUi`.
- `Engine.get_process_frames()` and `get_tick()` keep advancing and
  `godot_log errors` is empty; no crash report written.

## Failure modes

- Wrong node path (`/root/Spine/MindHud`): `Node not found` error + 15 s eval
  timeout, and the errored eval trips break-on-error so the frame loop pins
  (`godot_debugger sessions` -> `paused: true`). Recover with
  `godot_game stop` + `play` (fresh pid); `resume` re-breaks.
- `hud_visible` still true right after the dialog opens: the deferred call has
  not flushed yet — re-read one frame later. If it stays true, the
  `shown()`/`hidden()` defer seam regressed and `MindUi::hud_set_visible` will
  panic with `Gd<T>::bind_mut() failed, already bound` (plus a crash report).
- `Engine.get_frames_drawn()` can stay pinned at a small number while
  `Engine.get_process_frames()` advances (PIE presentation quirk): judge
  liveness on process frames/tick, not `frames_drawn`.
- `godot_game stop` leaves `launchid.dat`, so the next launch logs
  `[W] previous launch may have crashed (leftover launchid.dat)`; confirm
  against the crashes dir mtimes before calling it a crash.

## Variants

- Deterministic advance: after step 4, `godot_exec {"action":"call","params":
  {"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}`,
  then `godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").step(600)"}}`
  before the step-5 poll (the EV-0046 run took this path).
- Dialog action read (campaign `@continue`), twin-verified: the dialog node is
  visible at 1152x648 and its single Button carries labels `["", "Continue"]`
  with one `pressed` connection:

  ```
  godot_exec {"action":"eval","params":{"code":"var d = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/restart\")\nvar b = d.find_children(\"*\", \"Button\", true, false)[0]\nvar labels = b.find_children(\"*\", \"Label\", true, false)\nreturn {\"pid\": OS.get_process_id(), \"labels\": labels.map(func(l): return str(l.text)), \"pressed_connections\": b.pressed.get_connections().size()}"}}
  ```

  The `Button.text` property is empty (the label is an HBox child), so read the
  Labels. Screenshots may be a stale boot frame while `frames_drawn` is pinned
  (2-4) even though `Engine.get_process_frames()` advances — judge from this
  state, not the image.
