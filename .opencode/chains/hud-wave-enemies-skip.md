---
id: hud-wave-enemies-skip
title: Fresh groundZero → live MindHud wave/enemy/status text → fragment mirror + skip button + capture toast (EV-0053)
status: verified
applies_when: Verifying the HudFragment status/skip producers (wave counter, enemies remaining, wavetime, canSkipWave skip button, capture/lose toast) in-engine; any HUD read-model probe on a campaign sector.
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - Fresh serpulo:15 launch (no leftover sector save) for deterministic values; low map ids differ from upstream preset ids (groundZero is sector 15 here, 170 upstream).
  - MindHud/MindUi are autoloads at /root/MindHud and /root/MindUi, NOT under /root/Spine.
tools: [godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_log, godot_screenshot]
last_verified: 2026-10-08 1d8f75b (runs/20261008-192317-ev0053-hud-wave-enemies; twin runs/20261008-225009-ev0053-hud-twin, pid 131412)
---

# hud-wave-enemies-skip

## Steps

1. Attach (boot-and-identity), then clear the log and pid-stamp:

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"return {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"window\": str(DisplayServer.window_get_size())}"}}
   ```

2. Fresh sector launch + campaign start in one eval (the pause is inside the eval):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nh.set_paused(false)\nvar cleared = c.clear_planet_campaign_saves(\"serpulo\")\nvar loaded = h.load_sector(\"serpulo\", 15)\nvar loadout = JSON.stringify([{\"item\": \"copper\", \"amount\": 500}, {\"item\": \"lead\", \"amount\": 500}])\nvar started = c.start_sector_with_loadout(\"serpulo\", 15, loadout)\nh.set_paused(true)\nreturn {\"pid\": OS.get_process_id(), \"cleared\": cleared, \"loaded\": loaded, \"started\": started, \"hud\": c.get_hud_state()}"}}
   ```

   Expect `loaded: true`, `started: true`, `hasCore: true`, `waves: true`,
   `winWave: 10`, `wave: 0`, `enemies: 0`, `wavetime: 14400`; log
   `materialized 61 map building(s)`.

3. Reveal the in-game HUD (the planet dialog does this on launch; direct facade
   launches leave the menu up):

   ```
   godot_exec {"action":"eval","params":{"code":"var root = get_node(\"/root/Spine/Ui/UiRoot\")\nroot.call(\"set_menu_visible\", false)\nreturn {\"pid\": OS.get_process_id(), \"hud_group_visible\": get_node(\"/root/Spine/Ui/UiRoot/HudGroup\").visible}"}}
   ```

4. Baseline read (wait ≥0.2 s after step 2/3 for the MindHud campaign poll):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/MindHud\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar frag = get_node(\"/root/Spine/Ui/UiRoot/HudGroup/hud\")\nvar status = frag.get_node(\"WavesStack/WavesMain/Status\")\nvar skip = frag.get_node_or_null(\"WavesStack/WavesMain/WaveButtons/skip\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"hud_state\": c.get_hud_state(), \"hud_wave\": h.get(\"wave\"), \"hud_enemies\": h.get(\"enemies\"), \"hud_wavetime\": h.get(\"wavetime\"), \"hud_status_text\": str(h.get(\"status_text\")), \"status_label_text\": status.text, \"skip_button_visible\": (skip != null and skip.visible)}"}}
   ```

   Expect status `Wave 0/10\n4:00` (wave.cap + countdown) and
   `skip_button_visible: true` (`waves && enemies <= 0 && has_core`).

5. Force a wave and let the enemy count register: `run_wave()` + `step(6)`,
   then `step(5)`, then wait ≥0.2 s:

   ```
   godot_exec {"action":"eval","params":{"code":"var sim = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar ok = c.run_wave()\nvar t = sim.step(6)\nreturn {\"pid\": OS.get_process_id(), \"run_wave\": ok, \"tick\": t, \"groups\": sim.call(\"get_group_counts\"), \"hud\": c.get_hud_state()}"}}
   ```

   Expect `unit: 1` after the first step and `enemies: 1` in `get_hud_state`
   after the second; then the same read as step 4 shows
   `Wave 1/10\n1 Enemy Remaining\n1:59` and `skip_button_visible: false`.

6. Capture-toast edge (only when the wasCaptured/capture branch is under test):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar ok = c.capture_sector()\nreturn {\"pid\": OS.get_process_id(), \"captured\": ok, \"wasCaptured\": c.get_hud_state().get(\"wasCaptured\")}"}}
   ```

   Then read the MindHud toast signal, the MindUi toast signal and the
   OverlayLayer toast label in the **same eval, immediately after** the edge:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/MindHud\")\nvar ui = get_node(\"/root/MindUi\")\nvar overlay = get_node(\"/root/Spine/Ui/UiRoot/OverlayLayer\")\nvar texts = []\nfor n in overlay.get_children():\n\tif n.get(\"text\") != null:\n\t\ttexts.append(str(n.get(\"text\")))\nreturn {\"pid\": OS.get_process_id(), \"hud_toasts\": h.get_meta(\"ev0053_hud_toasts\"), \"ui_toasts\": ui.get_meta(\"ev0053_ui_toasts\"), \"overlay_texts\": texts}"}}
   ```

   The recorders are installed before the edge with (MindUi has a `toast`
   **method** as well as a signal, so connect by name, never `ui.toast.connect`):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/MindHud\")\nvar ui = get_node(\"/root/MindUi\")\nh.set_meta(\"ev0053_hud_toasts\", [])\nvar hcb = func(t, i):\n\tvar l = h.get_meta(\"ev0053_hud_toasts\")\n\tl.append(str(t))\nh.toast.connect(hcb)\nui.set_meta(\"ev0053_ui_toasts\", [])\nvar ucb = func(t, i):\n\tvar l = ui.get_meta(\"ev0053_ui_toasts\")\n\tl.append(str(t))\nui.connect(\"toast\", ucb)\nreturn {\"pid\": OS.get_process_id()}"}}
   ```

   Expect all three carry `Sector [accent]groundZero[white] Captured!` and the
   status text flips to `Sector Captured` (`sector.curcapture`).

7. `godot_log {"action":"errors","params":{"include_warnings":true}}` must be
   `[]`, then teardown (`godot_game stop`).

## Success signals

- `MindHud.wave/enemies/status_text` track `MindCampaign.get_hud_state()`
  after `run_wave`; the fragment `WavesStack/WavesMain/Status` text equals
  `"[center]" + status_text`.
- Skip button (`WavesStack/WavesMain/WaveButtons/skip`) visible iff
  `waves && enemies <= 0 && has_core`.
- Capture edge reaches `MindHud.toast` -> `MindUi.toast` -> an OverlayLayer
  label; status switches to `Sector Captured`.

## Failure modes

- Reading `MindHud` sooner than its 0.2 s campaign poll returns the previous
  values; wait one poll after an edge.
- `enemies` stays 0 for the first tick or two after `run_wave`; step a few
  ticks.
- `ui.toast` resolves to the MindUi **method** Callable, so
  `ui.toast.connect(...)` / `.is_connected(...)` errors ("Nonexistent function
  … in base 'Callable'") and trips break-on-error (frame loop pins; recover
  with `godot_game stop` + `play`). Use `ui.connect("toast", cb)`.
- An eval with `await` needs `params.await: true`; otherwise
  "Trying to call an async function without \"await\"" plus a 15 s timeout
  and the same pinned loop.
- Breaking the player core via `SimHost.break_block` removes the building but
  `get_hud_state().hasCore` stayed true over 400+ ticks in the EV-0053 run, so
  the `sector.lost` toast edge did not fire; that bookkeeping is a game-state
  seam, out of scope for the HUD producer.
- After `start_sector_with_loadout` the save queues one IO job and the
  `LoadingLayer/loading` overlay (`io_pending` 1, "Loading... 0% Cancel") sits
  over the screen and **swallows HUD clicks**: a skip click right after the
  launch does nothing and `gui_get_hovered_control()` reports
  `/root/Spine/Ui/UiRoot/LoadingLayer/loading/Background`. Step the sim
  (`step(30)`) until `io_pending == 0` and the fragment's 0.2 s refresh hides
  the overlay, then the skip control hovers. The writer run never clicked, so
  its reads were unaffected.
- The capture toast label is transient: reading `OverlayLayer` ~0.6 s after
  `capture_sector()` came back with no text child. The signal recorders still
  prove the chain; to verify the render hop, call
  `MindHud.push_toast("probe", "ok")` and read `OverlayLayer` in the same eval
  (label present synchronously).

## Variants

- `MindHud.push_toast("text","ok")` from an eval proves the toast render chain
  standalone (a MindLabel appears under OverlayLayer with that text).
- **Real skip-button control path** (validated in the twin run, pid 131412):
  after the IO overlay is gone, resolve
  `skip.get_global_rect().get_center()` (viewport (575,63) at 1152x648) and
  send discrete `godot_input` `mouse_button` press then release with
  `coords:"viewport"` (hover it first and check `gui_get_hovered_control()`
  names `skip`), then run an eval with `Input.flush_buffered_events()` reading
  `MindCampaign.get_hud_state()`: wave 0→1 and wavetime 14400→7200 without
  calling `run_wave` directly. `step(11)` then registers `unit: 1` and
  `enemies: 1`, and the skip button hides.
- The wave pane renders top-center at 1152x648 (Java's sits top-left); the
  composed text is read from `WavesStack/WavesMain/Status`, not from the
  top-left corner.
