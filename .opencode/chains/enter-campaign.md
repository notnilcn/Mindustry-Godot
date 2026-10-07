---
id: enter-campaign
title: Main menu → Campaign → planet → sector → launch
status: verified
applies_when: Entering or observing the campaign flow, from menu to launched sector HUD.
preconditions:
  - boot-and-identity completed (game in the main menu, runtime connected).
  - UI clicks resolve control centers via eval; never hardcode coordinates.
  - Java reference leg (optional comparison) needs computer-mcp with DISPLAY set at opencode process start.
tools: [godot_exec, godot_input, godot_screenshot, godot_log]
last_verified: 2026-10-08 9efd068 facade launch + loadout (runs/l2-20261008-012103-ev0052-research-purchase-godot); UI Play→Campaign→serpulo→OK→sector-panel action (runs/l2-20261008-014155-ev0037-core-launch-godot)
---

# enter-campaign

## Steps (UI variant)

1. Resolve the Play button center from the live scene tree (paths are
   illustrative; list the actual `Control` nodes first):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"<menu path>/Play\")\nvar p = (c as Control).get_global_rect().get_center()\nreturn {\"x\": int(p.x), \"y\": int(p.y)}"}}
   ```

2. Click it and flush:

   ```
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nreturn get_tree().root.gui_get_hovered_control().name if get_tree().root.gui_get_hovered_control() else null"}}
   ```

3. Repeat for Campaign and the sector row; the planet dialog is only reachable
   from the main menu (there is no in-game pause-menu route while EV-0035-family
   campaign gaps are open).

4. Launch and confirm the HUD:

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": host.get_tick(), \"checksum\": str(host.get_checksum())}"}}
   godot_screenshot {"action":"game"}
   ```

## Steps (campaign facade variant)

Use when the UI flow is blocked but the campaign model is the subject. Verified
2026-10-08 at `9efd068`: a fresh launch has a real player core and the loadout
stocks its live module.

1. Load the preset map on the sim host. `groundZero` is sector **15**; 170 is a
   plain generated sector (terrain only, no preset map, no core):

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").load_sector(\"serpulo\", 15)"}}
   ```

   Expect `true` and log lines `load_sector: loaded preset map `serpulo/groundZero` (256x256)`
   and `load_sector: materialized 61 map building(s)`.

2. Start the campaign sector on **`MindCampaign`** (not `SimHost`) with an
   explicit `[{item,amount}]` loadout:

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar loadout = JSON.stringify([{\"item\": \"copper\", \"amount\": 500}, {\"item\": \"lead\", \"amount\": 500}])\nvar ok = c.start_sector_with_loadout(\"serpulo\", 15, loadout)\nreturn {\"started\": ok, \"sector_state\": c.get_sector_state(), \"core_items\": str(h.core_items_json())}"}}
   ```

3. Read back: `started: true`, `sector_state.hasCore: true`, and `core_items`
   equal to the loadout; then read rules / group counts / screenshot as needed.

## Success signals

- Planet dialog visible with sector rows; after a successful launch the world
  renders and the HUD shows a core, wave counter, and toolbar.
- No error log entries during the sequence.

## Failure modes

- A click that is never flushed leaves the menu unchanged; the flush in step 2
  is mandatory.
- Hover state is read on the NEXT frame after `godot_input`; use
  `gui_get_hovered_control()` in a follow-up eval, not the same call.
- Verified 2026-10-08 at `9efd068` (EV-0037 run): the UI steps land as written —
  Play (230,149) → Campaign (460,149) → serpulo card (412,194) → OK (499,631),
  each as discrete `mouse_button` press/release + `Input.flush_buffered_events()`
  (a `sequence` call timed out on this host once; discrete calls are the safe form).
- The sector panel's action is read from `CampaignViews::vanilla_fixture()` in this
  build: `MindUi.campaign_views()` falls back to the fixture when
  `MindCampaign.campaign_views_json` is absent, and the dialog caches it at `_ready()`,
  so groundZero always renders `@sectors.go` (resume) even with no saves. Clearing
  campaign saves mid-process does not refresh the dialog — restart the game so the
  cache is rebuilt. The `go` click still launches fresh when the sector has no save
  (`play_sector` → `play_new_sector`); verify freshness from the
  `MindCampaign ready (0 save slot(s))` log and `wasCaptured=false`, not the label.
- Campaign bootstrap/launch gaps (EV-0035 family) can make step 3 fail before any
  click is wrong. Record the facade result and stop; do not hunt random coordinates.
- `hasCore: false` after the facade launch means the running binary predates
  `e85732c` (map-building materialization) or the sector has no preset map; use
  groundZero (sector 15), not 170. A stocked core also requires the loadout to
  pass `start_sector_with_loadout`, not the bare `start_sector`.
- Java comparison: computer-mcp crashes at import without `DISPLAY` (pynput),
  and opencode silently drops its tools; the Godot leg is unaffected.
