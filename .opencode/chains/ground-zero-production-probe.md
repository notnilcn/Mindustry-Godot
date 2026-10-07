---
id: ground-zero-production-probe
title: Fresh Ground Zero → placed drill on overlay ore + adjacent core → deterministic first-delivery bracket
status: verified
applies_when: Verifying placed-building production/offload (mining into a core) on the Ground Zero sector with the live block runtime — EV-0047 family, or any probe that needs a placed producer to be observable.
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - No sector save may exist for serpulo-15 (`MindCampaign.clear_planet_campaign_saves("serpulo")`), or `load_sector` takes the resume branch and the map differs.
  - "`SimHost.core_items_json()` reads the first core in entity order: the materialized map core (build_id 5) must be broken before a placed probe core becomes observable."
tools: [godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_log, godot_screenshot]
last_verified: 2026-10-08 9efd068 (runs/l2-20261008-021834-ev0047-production-godot; twin re-run runs/l2-20261008-034337-ev0047-production-twin, pid 267163)
---

# ground-zero-production-probe

## Steps

1. Preflight/boot per `boot-and-identity`; `godot_log clear`.

2. Clear any leftover sector save (planet-scoped):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"cleared\": c.clear_planet_campaign_saves(\"serpulo\"), \"slots\": str(c.list_save_slots())}"}}
   ```

3. Load the preset fresh and pause in one eval (the pump runs while Playing, so
   pausing inside the same eval pins tick 0):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nh.set_paused(false)\nvar loaded = h.load_sector(\"serpulo\", 15)\nh.set_paused(true)\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"tick\": h.get_tick(), \"checksum\": str(h.get_checksum()), \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

   Expect `loaded=true`, tick 0, `build=61`, and the log lines
   `load_sector: loaded preset map serpulo/groundZero (256x256)` /
   `load_sector: materialized 61 map building(s)`.

4. Place the probe (anchor convention: odd-size blocks center on the given tile,
   even-size blocks use it as the top-left):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"break_block","args":[129, 55]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[129, 48, "mechanical-drill"]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[130, 46, "core-shard"]}}
   ```

   The mechanical drill gets the 2x2 `ore-copper` overlay footprint
   (129-130,48-49); the core gets (129-131,45-47) and is edge-adjacent to the
   drill. `place_block` returns true even when the runtime rejects the
   placement, so verify with a filtered state dump:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar s = JSON.parse_string(h.get_state_json())\nvar win = s[\"world\"][\"tiles\"].filter(func(t): return t[\"x\"] >= 126 and t[\"x\"] <= 134 and t[\"y\"] >= 43 and t[\"y\"] <= 52)\nreturn {\"pid\": OS.get_process_id(), \"tick\": s[\"tick\"], \"window\": win, \"core_items\": h.core_items_json()}"}}
   ```

5. Deterministic production bracket. `step(120)` is the original repro window;
   it is legitimately empty for a mechanical drill (`drillTime 600`, 4 ore
   tiles). Fresh replays of steps 3-4 + `step(N)` bracket the first delivery:
   `{}` at 160/176/180/182, `{"copper":1}` at 183/184/192/213.

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar t = h.step(120)\nreturn {\"pid\": OS.get_process_id(), \"tick\": t, \"checksum\": str(h.get_checksum()), \"core_items\": h.core_items_json()}"}}
   ```

6. Error check: `godot_log errors` (include_warnings) must be empty; a `[D]`
   place-rejection is expected if a placement overlapped.

7. Teardown: `godot_game stop`.

## Success signals

- `core_items_json()` `{}` at tick 0, then `{"copper":1}` first at **tick 183**
  with checksum `058849b733bc6720` (9efd068), and the same setup replays to the
  same checksum/inventory.
- No `[E]/[W]` entries.

## Failure modes

- `place_block` returns true but the tile window shows no block: the placement
  was rejected (overlap / wrong anchor). Even-size blocks use the given tile as
  top-left; odd-size as center.
- `core_items_json()` stays `{}` while a placed core should hold items: the
  materialized map core is still first in entity order — break it first.
- Empty at `step(120)` is expected (rate + warmup), not evidence of inertness;
  pre-fix the same probe was `{}` through tick 4962
  (`runs/20261007-092013-ev0047-verify`).
- `load_sector` log says `resuming`: a leftover save was loaded; clear saves
  (step 2) and reload.
- **Don't `JSON.parse_string(get_state_json())` on a loaded 256x256 sector**:
  the dump is ~6.6 MB and the parse exceeds the 30 s eval budget; the oversized
  reply also wedges the editor bridge (`ERR_OUT_OF_MEMORY` from the websocket
  outbound buffer, `godot_log errors` shows it under source `editor`). Stash the
  string once with `SimHost.set_meta("dump", s)` and read it with
  `String.find/count/substr`; `get_meta` on a missing key logs a game-side error,
  so guard with `has_meta`.

## Variants

- Power-tagged production: laser-drill 3x3 (131-133,47-49) + `solar-panel`
  (134,47) + `solar-panel` (134,48) + core (128-130,46-48) forms one graph
  (`graph_id 1`) but only `satisfaction 0.218` at tick 120 (map-start day ramp),
  so it cannot demonstrate within-120 delivery either.
- Map-inventory probe without parsing the dump (twin 2nd sweep): after the fresh
  `load_sector`, `get_state_json()` once, `set_meta("dump", s)`, then
  `count("mechanical-drill")` / `find` + `substr(i-700, 1400)` for context. Each
  tile of a multiblock carries its own entry with the same `build_id`. Ground
  Zero's only machinery is the derelict outpost — drill build_id 39 at
  (172-173,163) + conveyor build_id 38 at (171,163) — nothing feeds the player
  core (128-130,54-56).
