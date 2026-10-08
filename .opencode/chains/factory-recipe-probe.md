---
id: factory-recipe-probe
title: Fresh Frozen Forest → drill on coal + graphite-press + core chain → graphite delivery bracket
status: verified
applies_when: Verifying placed factory crafting in-engine (input acceptance + output emission), EV-0058 family, or any probe that needs a factory fed by a real transport path on a map with the right ore.
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - No sector save may exist for serpulo-86 (`MindCampaign.clear_planet_campaign_saves("serpulo")`), or `load_sector` takes the resume branch and the map differs.
  - The coal coordinates come from the committed `assets/maps/serpulo/frozenForest.msav`; ore lives in `tile.overlay` and is invisible in the state dump, so probe coordinates offline (load the MSAV with `SaveIo::load_bytes` behind the `msav-import` feature in a throwaway crate).
  - "`SimHost.core_items_json()` reads the first core in entity order: the materialized map core at (40,70) must be broken before the placed probe core becomes observable."
tools: [godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_log]
last_verified: 2026-10-08 fac6ae5 (runs/l2-20261008-192815-ev0058-factory-recipes-godot)
---

# factory-recipe-probe

## Steps

1. Preflight/boot per `boot-and-identity`; `godot_log clear`.

2. Clear any leftover sector save, load the preset fresh and pause in one eval (the
   pump runs while Playing, so pausing inside the same eval pins tick 0):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nh.set_paused(false)\nvar cleared = c.clear_planet_campaign_saves(\"serpulo\")\nvar loaded = h.load_sector(\"serpulo\", 86)\nh.set_paused(true)\nreturn {\"pid\": OS.get_process_id(), \"cleared\": cleared, \"loaded\": loaded, \"tick\": h.get_tick(), \"checksum\": str(h.get_checksum()), \"groups\": str(h.call(\"get_group_counts\"))}"}}
   ```

   Expect `loaded=true`, tick 0, pre-placement checksum `7ddea6e7e3c0ba22`, `build=23`,
   and the log lines `load_sector: loaded preset map \`serpulo/frozenForest\` (200x200)`
   / `load_sector: materialized 23 map building(s)`.

3. Break the map core and place the chain (anchor convention: odd-size blocks center
   on the given tile, even-size blocks use it as the top-left):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"break_block","args":[40, 70]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[106, 55, "mechanical-drill"]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[108, 55, "graphite-press"]}}
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[111, 55, "core-shard"]}}
   ```

   The drill gets (106-107,55-56) on 4x `ore-coal`; the press gets (108-109,55-56)
   and is edge-adjacent to the drill; the core gets (110-112,54-56) and is
   edge-adjacent to the press. `place_block` returns true even when the runtime
   rejects the placement, so verify with a stashed state dump (never parse a loaded
   sector dump in-eval — 3 MB+ here; stash it with `set_meta` and count substring
   hits):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar s = str(h.get_state_json())\nh.set_meta(\"dump\", s)\nreturn {\"pid\": OS.get_process_id(), \"drill\": s.count(\"\\\"mechanical-drill\\\"\"), \"press\": s.count(\"\\\"graphite-press\\\"\"), \"core\": s.count(\"\\\"core-shard\\\"\"), \"items\": h.core_items_json()}"}}
   ```

   Expect drill 5 / press 5 / core 10 (= 4/4/9 tiles + 1 entity each) and `items: "{}"`.

4. Deterministic production bracket (paused; `step` is exact):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar t1 = h.step(600)\nvar a = h.core_items_json()\nvar t2 = h.step(600)\nreturn {\"pid\": OS.get_process_id(), \"tick_600\": t1, \"items_600\": a, \"tick_1200\": t2, \"items_1200\": h.core_items_json(), \"checksum\": str(h.get_checksum())}"}}
   ```

   Expect `items_600={"graphite":1}`, `items_1200={"graphite":3}`, checksum
   `88421ba91eb63a6d` at tick 1200 (fac6ae5; the drill's first coal lands ~183, the
   press crafts 90 ticks after its 2nd coal).

5. Error check: `godot_log errors` (include_warnings) must be empty. Do **not** call
   `get_window().move_to_foreground()` first — Godot 4.7 logs it as an error-level
   deprecation, which breaks the empty-log requirement; `grab_focus()` is the 4.7 name.

6. Teardown: restore the camera, `godot_game stop`.

## Success signals

- `core_items_json()` `{}` at tick 0, `{"graphite":1}` at tick 600, `{"graphite":3}` at
  tick 1200, checksum `88421ba91eb63a6d`; the same setup replays byte-identically.
- No `[E]/[W]` entries.

## Failure modes

- `core_items_json()` stays `{}` with the chain placed: the factory rejects neighbor
  deliveries (`CrafterBehavior` missing `accept_item`/`handle_item` — EV-0058), or the
  map core at (40,70) was not broken (core order).
- Drill tiles < 4: the anchor missed the coal; re-probe the MSAV (ore is in
  `tile.overlay`, absent from the state dump).
- `load_sector` log says `resuming`: a leftover save was loaded; clear saves (step 2)
  and reload.
- **Frozen viewport on the loop's weston session**: `Engine.get_frames_drawn()` pins
  (e.g. 94) while `get_process_frames()` advances, and every screenshot is
  byte-identical (even across stop/play and `RenderingServer.force_draw()`). Do not
  block the verdict on frames — `core_items_json`/checksum is the evidence class that
  answers this probe; record the frozen-viewport note and the PNG sha256s.

## Variants

- Other coal-fed serpulo factories: `pyratite-mixer` (coal+lead+sand) needs a second
  drill on lead; `kiln`/`silicon-smelter` need a sand source (pulverizer on scrap, or
  the sand ore patch). Keep the core adjacent to the factory as the observation sink.
- `serpulo/15` (groundZero) has only copper/lead/scrap ore — use it for drill/offload
  probes (`ground-zero-production-probe`), not for coal-fed factories.
