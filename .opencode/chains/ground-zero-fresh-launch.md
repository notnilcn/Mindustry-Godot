---
id: ground-zero-fresh-launch
title: Fresh Ground Zero launch (clear leftover sector saves → restart → UI launch) and core assertions
status: verified
applies_when: Verifying a fresh campaign-sector launch registers the preset map's core (EV-0037 family) — hasCore, building entities, no instant game-over.
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - Ground Zero is serpulo sector 15 (170 is a plain generated sector with no preset map).
  - "No sector save may exist for serpulo-15: a leftover autosave forces the resume branch and hides the fresh-launch behavior."
tools: [godot_health, godot_game, godot_exec, godot_input, godot_screenshot, godot_log]
last_verified: 2026-10-08 9efd068 (runs/l2-20261008-014155-ev0037-core-launch-godot)
---

# ground-zero-fresh-launch

## Steps

1. Boot the entry scene explicitly and pid-stamp; `godot_log clear`:

   ```
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   godot_exec {"action":"eval","params":{"code":"return {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"slots\": str(get_node(\"/root/Spine/MindCampaign\").list_save_slots())}"}}
   ```

2. If `user://saves/sector-serpulo-15.msav` exists (from an earlier evaluator's
   launch), copy it to the run evidence and clear the campaign saves (planet-scoped;
   the Settings Data pane's `@settings.clearcampaignsaves` is the UI equivalent):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"cleared\": c.clear_planet_campaign_saves(\"serpulo\"), \"slots_after\": str(c.list_save_slots())}"}}
   ```

3. **Restart the game.** This build's `MindCampaign` has no `campaign_views_json`, so
   the dialogs render the `CampaignViews::vanilla_fixture()` snapshot cached at
   `_ready()`; a mid-process clear leaves the planet view showing stale `has_base`.
   A fresh process rebuilds the cache from the cleared state:

   ```
   godot_game {"action":"stop"}
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   ```

   Expect log `MindCampaign ready (0 save slot(s))` and
   `list_save_slots() == []` (re-stamp the new pid).

4. Enter the campaign through the UI: Play (sidebar index 0) → Campaign → serpulo
   card → OK, resolving every center via `get_global_rect().get_center()` first and
   sending each click as discrete `mouse_button` press + release followed by an eval
   with `Input.flush_buffered_events()`. The planet view selects sector 15
   (`_selected_sector` on the planet dialog) with the action labelled
   `@sectors.go` from the fixture; `has_base`/`has_save` in that readout are fixture
   values, not live state.

5. Resolve the sector-panel action button (the Button inside
   `Center/Panel/Layout/Buttons` whose label is the sector action) and click it. With
   no save present the launch takes the fresh branch
   (`play_sector` → `play_new_sector`).

6. Assert, all pid-stamped:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": h.get_tick(), \"group_counts\": str(h.call(\"get_group_counts\")), \"sector_state\": c.get_sector_state(), \"hud\": str(c.get_hud_state())}"}}
   ```

   Expect `sector_state.hasCore=true`, `sector=15`, `wasCaptured=false`,
   `group_counts.build=61`, and `hud.gameOver=false`/`hasCore=true`/`winWave=10`.
   `load_sector: loaded preset map serpulo/groundZero (256x256)` and
   `load_sector: materialized 61 map building(s)` in the log are the fix's evidence.

7. Core ownership: parse `SimHost.get_state_json()` and filter `world.tiles` for
   blocks containing `core` — a 3x3 `core-shard` on team 0 (the default team that
   `PlaySession::player_core_count()` reads). Then `h.step(60)` and re-read
   `get_hud_state()` to prove the first `GameStateCheck` ticks keep `gameOver=false`.

8. Teardown: `godot_game stop`; no pause/camera state was changed beyond the dialogs'
   own pause governor.

## Success signals

- `hasCore=true` with `build=61` right after launch and `gameOver=false` before and
  after `step(60)`; the log names groundZero and the 61 materialized buildings.

## Failure modes

- `build=0`/`hasCore=false` means the running binary predates `e85732c` (map-building
  materialization) or `load_sector` ran against a sector without a preset map (170).
- A `@sectors.go` label is expected on this build (fixture read model); do not treat it
  as a reason to skip the launch — check the save-slot listing and the `0 save slot(s)`
  boot log for freshness instead.
- `Engine.get_frames_drawn()` pinned and byte-identical screenshots after heavy evals:
  the viewport stalled while evals still answer; `godot_game stop` + `play` recovers
  (see open-godot-mcp-learnings.md).
