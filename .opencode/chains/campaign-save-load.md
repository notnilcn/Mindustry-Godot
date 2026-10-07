---
id: campaign-save-load
title: Campaign facade → launch a sector → save / list / load a slot
status: seeded
applies_when: Exercising MindCampaign save_slot/load_slot/list_save_slots after a sector launch (EV-0036 family).
preconditions:
  - boot-and-identity completed (`res://scenes/game.tscn` playing, runtime connected).
  - Node paths fixed in the scene: `/root/Spine/SimHost`, `/root/Spine/MindCampaign`.
  - The GDExtension is rebuilt (`tools/build.sh`) and the editor reloaded it (EXTENSION_RELOADED re-runs bootstrap).
tools: [godot_health, godot_game, godot_exec]
last_verified: not yet (loop-3 pre-commit smoke on 2026-10-07 confirmed every step; no run dir captured)
---

# campaign-save-load

## Steps

1. Play the main scene explicitly and wait for `runtime_ready`:

   ```
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   ```

2. Load the sector world into the sim, then start the campaign session:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\"); return h.load_sector(\"serpulo\", 170)"}}
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\"); return c.start_sector(\"serpulo\", 170)"}}
   ```

   Both must return `true`; `start_sector` only succeeds when content booted and
   the play flow produced events.

3. Queue a save and confirm it is pending (the fresh `Sim` installed by
   `load_sector` must carry the `SimIoHandler` again):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\"); var h = get_node(\"/root/Spine/SimHost\"); return {\"save\": c.save_slot(\"evalslot\"), \"pending\": h.io_pending()}"}}
   ```

   Expect `{"save": true, "pending": 1}`; a `pending` of `0` means the request was
   never queued.

4. Drain the queue with sim steps and check it emptied:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\"); h.step(120); return {\"tick\": h.get_tick(), \"pending\": h.io_pending()}"}}
   ```

   Expect `pending: 0`; a stuck `1` is the lost-handler regression.

5. Confirm the file and the slot listing (`user://` → Godot's app_userdata dir):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\"); var slots = c.list_save_slots(); return {\"exists\": FileAccess.file_exists(\"user://saves/evalslot.msav\"), \"slots\": slots.size()}"}}
   ```

   Expect `exists: true`, `slots >= 1`; `list_save_slots()` returns `[]` (never
   panics) on a fresh install.

6. Queue and drain a load:

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\"); var h = get_node(\"/root/Spine/SimHost\"); var ok = c.load_slot(\"evalslot\"); var before = h.io_pending(); h.step(30); return {\"load\": ok, \"before\": before, \"after\": h.io_pending()}"}}
   ```

   Expect `load: true`, `before: 1`, `after: 0`.

7. Clean up the smoke slot (leave the user dir as found):

   ```
   godot_exec {"action":"eval","params":{"code":"var p = ProjectSettings.globalize_path(\"user://saves/evalslot.msav\"); if FileAccess.file_exists(p): DirAccess.remove_absolute(p); return not FileAccess.file_exists(p)"}}
   ```

## Success signals

- Every step returns the expected value with no `[E]` log entries.
- The `.msav` exists under `user://saves/` and `list_save_slots()` sees it.

## Failure modes

- A queue that never drains (`pending` stays `> 0`) means a fresh `Sim` replaced
  `self.sim` without re-installing the IO executor (`MindSimHost::install_sim_seams`).
- `list_save_slots()` on a fresh data dir must not call `chunks(0)`; an empty
  saves dir returns `[]` (see `io::save::slot::tests::empty_dir_lists_no_slots`).
- `save_slot`/`load_slot` need the sim node at `/root/Spine/SimHost`; without it
  they log a warning and return `false`.
