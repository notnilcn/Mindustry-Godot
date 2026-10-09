---
id: campaign-save-restore
title: Campaign save -> restart client -> load_slot -> meta/entity/rules restore assertions
status: verified
applies_when: Verifying live-sim save contents and the `SaveSlot.load` path (EV-0065 family): real meta tags, materialized buildings that execute, saved units respawned, resumed rules/wave, and a continued production delta after the load.
preconditions:
  - "boot-and-identity completed (`res://scenes/game.tscn` playing, runtime connected, pid stamped)."
  - "No leftover `user://saves/sector-serpulo-15.msav` (or call `clear_planet_campaign_saves(\"serpulo\")`) so `load_sector` takes the fresh preset branch."
  - "Rebuild + relaunch the editor before the run (`tools/build.sh` writes the cdylib; a running editor keeps the old one mmap'd)."
  - "Pause (`SimHost.set_paused(true)`) before the save and again immediately after the restart: the unpaused fixed-step pump advances during MCP round-trips and confounds the post-load production delta."
  - "`mind-headless io dump-meta` takes the file **positionally** (`io dump-meta <file> --json`); there is no `--file` flag."
tools: [godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_log, godot_screenshot]
last_verified: 2026-10-09 af9dac2 (.opencode/evals/runs/20261009-005513-ev0065-save-restore-godot-2)
---

# campaign-save-restore

## Steps

1. Boot + identity per `boot-and-identity`; `godot_log clear`.

2. Fresh Ground Zero (serpulo:15), paused inside the eval (deterministic tick 0):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nh.set_paused(false)\nvar cleared = c.clear_planet_campaign_saves(\"serpulo\")\nvar loaded = h.load_sector(\"serpulo\", 15)\nh.set_paused(true)\nreturn {\"pid\": OS.get_process_id(), \"cleared\": cleared, \"loaded\": loaded, \"tick\": h.get_tick(), \"groups\": str(h.call(\"get_group_counts\")), \"checksum\": str(h.get_checksum())}"}}
   ```

   Expect `loaded: true`, tick 0, checksum `4353bdfd835ec038`, `build: 61`.

3. Start the sector, drain the launch save, break the map core and place the
   probe chain (fresh placements run the full runtime; the maps core must go
   first so `core_items_json()` reads the placed core):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar started = c.start_sector(\"serpulo\", 15)\nvar t = h.step(4)\nvar broke = h.break_block(129, 55)\nvar drill = h.place_block(129, 48, \"mechanical-drill\")\nvar core = h.place_block(130, 46, \"core-shard\")\nreturn {\"pid\": OS.get_process_id(), \"started\": started, \"tick\": t, \"pending\": h.io_pending(), \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

   Expect `started: true`, `pending: 0`, `build: 62` (61 - broken core + drill + core).

4. Pre-save production baseline (drill on 4 copper tiles → core):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar t = h.step(600)\nreturn {\"pid\": OS.get_process_id(), \"tick\": t, \"core_items\": h.core_items_json(), \"checksum\": str(h.get_checksum())}"}}
   ```

   Expect tick 604, `core_items {"copper":3}`, checksum `d1993244ed46feda`.

5. Spawn a saved unit (`run_wave` needs the installed campaign runtime):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar ok = c.run_wave()\nvar t = h.step(1)\nreturn {\"pid\": OS.get_process_id(), \"run_wave\": ok, \"tick\": t, \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

   Expect `unit: 1` at tick 605.

6. Save + drain:

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar saved = c.save_slot(\"ev0065r2\")\nvar pending = h.io_pending()\nvar t = h.step(4)\nreturn {\"pid\": OS.get_process_id(), \"saved\": saved, \"pending_before\": pending, \"pending_after\": h.io_pending(), \"tick\": t, \"exists\": FileAccess.file_exists(\"user://saves/ev0065r2.msav\"), \"groups\": str(h.call(\"get_group_counts\")), \"core_items\": h.core_items_json()}"}}
   ```

   Expect `saved: true`, pending 1→0, `exists: true`, `build: 62`, `unit: 1`.

7. Dump the real meta host-side and keep the sha (also compare `rules` to the
   post-load `get_rules_json()` later):

   ```
   $PARITY_WORKTREE/client/bin/rust/debug/mind-headless io dump-meta \
     "$PARITY_LOOP_DIR/xdg-data/godot/app_userdata/Mindustry-Godot/saves/ev0065r2.msav" --json
   ```

   Expect `saved`/`time_played`/`wave`/`tick` non-zero, `stats`/`rules` non-`{}`,
   `rules` containing the sector key (`serpulo-15`), `nocores` false.

8. Restart and load **paused** (new pid; pause before the load so only `step`
   advances the sim):

   ```
   godot_game {"action":"stop"}
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nh.set_paused(true)\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar fresh = str(h.call(\"get_group_counts\"))\nvar queued = c.load_slot(\"ev0065r2\")\nvar before = h.io_pending()\nvar t = h.step(4)\nreturn {\"pid\": OS.get_process_id(), \"fresh_groups\": fresh, \"queued\": queued, \"pending_after\": h.io_pending(), \"tick\": t, \"groups\": str(h.call(\"get_group_counts\")), \"core_items\": h.core_items_json(), \"playtime\": c.get_total_playtime()}"}}
   ```

   Fresh world 0/0/0 → after the load `build: 62`, `unit: 1`,
   `core_items {"copper":3}`, `playtime` == the meta value; the game log shows
   `sim_io load: materialized 62 building(s)`.

9. Post-load production must continue (the saved drill mines again):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar t600 = h.step(600)\nvar mid = h.core_items_json()\nvar t1200 = h.step(600)\nreturn {\"pid\": OS.get_process_id(), \"tick_600\": t600, \"items_600\": mid, \"tick_1200\": t1200, \"items_1200\": h.core_items_json()}"}}
   ```

   Expect `{"copper":3}` → `{"copper":6}` → `{"copper":10}`.

10. Rules equality + clean log: write `c.get_rules_json()` to a `user://` file,
    compare byte-for-byte to the step-7 `rules` tag, `godot_log errors` empty,
    `godot_screenshot game` non-blank, then `godot_game stop`.

## Success signals

- Meta dump: `saved`/`time_played`/`wave`/`tick` non-zero, `stats`/`rules` real.
- Load: `build` count restored, units respawned, core inventory restored,
  playtime restored, rules byte-equal.
- Crafting resumes after the load (`core_items_json` rises across stepped
  windows) and the error log is empty.

## Failure modes

- **Production flat after load**: materialized drills need
  `refresh_drill_ore` (grid-derived `DrillState.dominantItem`); without it the
  tile entities exist but all mine nothing (`af9dac2`).
- Unpaused sim across round-trips: the pump advances hundreds/thousands of
  ticks between evals, so a "no production" reading can be an artifact and a
  delta can be wall-clock, not step-driven. Pause first, `step` only.
- `MindCampaign.get_sector_state().campaign` is `false` after a direct
  `load_slot` (no `CampaignRuntime` installed on that path); rules/wave still
  load (`rules_json` carries the sector). Use `load_sector` for sector resume.
- Stale editor library: a rebuilt `libmind_gdext.so` is not picked up until the
  editor is relaunched; check `pgrep -f "godot --editor --path <worktree>/client"`
  against the build time before starting the run.

## Variants

- Frozen Forest (serpulo:86) factory chain for coal-fed crafter checks; the
  same save/restart/load assertions apply.
