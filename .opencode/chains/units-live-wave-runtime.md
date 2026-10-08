---
id: units-live-wave-runtime
title: Fresh sector → campaign launch → run_wave → live unit/bullet assertions (EV-0048)
status: verified
applies_when: Verifying the live unit/wave/combat runtime in-engine (WaveSpawner spawns via MindCampaign.run_wave, Groups.unit/Groups.bullet liveness, unit movement/weapons) on a campaign sector.
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - No leftover sector save for serpulo-15, or clear it first (`clear_planet_campaign_saves("serpulo")`).
  - Sim paused is fine for `step()`; the host pause only stops the wall-clock pump.
tools: [godot_log, godot_exec, godot_screenshot, godot_game]
last_verified: 2026-10-08 a1ca816 (runs/20261008-134638-ev0048-live-unit-runtime-godot; re-verified twin runs/20261008-040830-ev0048-live-unit-runtime-twin)
---

# units-live-wave-runtime

## Steps

1. Clear the log and the planet's campaign saves so `load_sector` takes the fresh
   preset branch:

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"cleared\": c.clear_planet_campaign_saves(\"serpulo\"), \"slots\": str(c.list_save_slots())}"}}
   ```

   Expect `cleared: true`. Leftover saves make `load_sector` resume and change
   the checksum/world.

2. Load groundZero (`serpulo:15`) fresh and pause inside the same eval:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nh.set_paused(false)\nvar loaded = h.load_sector(\"serpulo\", 15)\nh.set_paused(true)\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"tick\": h.get_tick(), \"checksum\": str(h.get_checksum()), \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

   Expect `loaded: true`, tick 0, checksum `4353bdfd835ec038`, `build: 61`,
   `unit: 0`; log `loaded preset map `serpulo/groundZero` (256x256)` and
   `materialized 61 map building(s)`.

3. Start the campaign sector with an explicit loadout (third arg is a JSON
   string, not an Array):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar loadout = JSON.stringify([{\"item\": \"copper\", \"amount\": 500}, {\"item\": \"lead\", \"amount\": 500}])\nvar ok = c.start_sector_with_loadout(\"serpulo\", 15, loadout)\nreturn {\"pid\": OS.get_process_id(), \"started\": ok, \"sector_state\": str(c.get_sector_state()), \"hud\": str(c.get_hud_state())}"}}
   ```

   Expect `started: true`, `hasCore: true`, `gameOver: false`, `wave: 0`.

4. (Optional movement/weapons probe) place an enemy building at a known-free
   tile ~20-45 tiles from the spawn; `place_block` returns `true` even when the
   live runtime rejects it, so verify `build` increments:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar placed = h.place_block(143, 187, \"core-shard\")\nreturn {\"pid\": OS.get_process_id(), \"placed\": placed, \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

   On groundZero the spawn overlay is at (135,230); free ground stretches north.
   A rejected placement logs `[D] place command rejected at (x, y)`.

5. Force a wave; the spawn lands on the next tick:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar ok = c.run_wave()\nvar t = h.step(1)\nreturn {\"pid\": OS.get_process_id(), \"run_wave\": ok, \"tick\": t, \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

   Expect `unit` 1 at tick 1 (was 0 at tick 0). A second `run_wave` + `step(1)`
   spawns the next wave groups (wave index 1 -> 3 units on groundZero).

6. Sample in small loop-free chunks; `bullet` becomes non-zero once a unit is in
   range of an enemy building. The HUD mirror follows on the next frame:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar t = h.step(120)\nreturn {\"pid\": OS.get_process_id(), \"tick\": t, \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

## Success signals

- `get_group_counts().unit` 0 -> 1 on the tick after `MindCampaign.run_wave`.
- With a core-shard 43.7 tiles (349.9 px) from the spawn, first `bullet > 0`
  between tick 300 and 420: the unit covers the gap at 0.5 px/tick until the
  162 px mount radius (`range 146 + 2*TILESIZE`).
- `get_hud_state().enemies` mirrors the live wave-team count; `godot_log errors`
  stays empty.
- Re-verified on the 2026-10-08 twin run: tick 1 checksum `9f527206f0c78da9`
  again, first bullets between tick 300 and 420, second `run_wave` -> 4 units,
  `bullet` 5 after another `step(60)`.

## Failure modes

- **Do not run one huge `step()`** (e.g. 50000): the eval times out at ~15 s,
  the game finishes the step in the background, and later evals stall until it
  completes. Chunk steps and poll `get_tick`.
- `MindCampaign.get_hud_state()` can go stale after such a timeout (wavetime /
  enemies frozen); trust `SimHost.get_group_counts()` for liveness instead.
- `place_block` returning `true` does not prove placement; check the `build`
  count or the `[D]` rejection log.
- Units are not rendered yet, so screenshots cannot show them; use group counts
  and a placed enemy core as a proximity/target oracle.
- The stock-sector map buildings (nearest ~69 tiles from the spawn) were not
  engaged in the sampled windows; `unit > 0` is still the pass assertion.
