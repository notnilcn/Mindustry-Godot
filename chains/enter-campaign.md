---
id: enter-campaign
title: Main menu → Campaign → planet → sector → launch
status: verified
applies_when: Entering or observing the campaign flow, from menu to launched sector HUD.
preconditions:
  - boot-and-identity completed (game in the main menu, runtime connected).
  - UI clicks resolve control centers via eval; never hardcode coordinates.
  - "Java-leg shell helpers (optional comparison): scripts in `.opencode/skills/parity-eval/scripts/`, run with `$PARITY_PY` (bootstrap.sh) and `DISPLAY=:10`."
tools: [godot_exec, godot_input, godot_screenshot, godot_log]
last_verified: 2026-10-09 ad25ae5 UI variant through the launch-loadout confirm (pid 129492: sector 15, hasCore=true, build=61, step(60) tick 70, gameOver=false, winWave=10; single opencode `execute` call, 40 chained MCP calls); 2026-10-08 9efd068 facade launch + loadout (runs/l2-20261008-012103-ev0052-research-purchase-godot); UI Play→Campaign→serpulo→OK→sector-panel action (runs/l2-20261008-014155-ev0037-core-launch-godot, re-verified a1ca816 in runs/l2-20261008-133536-ev0013-block-picker-godot)
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

3. Repeat for Campaign → serpulo card → OK; the planet dialog is only reachable
   from the main menu (there is no in-game pause-menu route while EV-0035-family
   campaign gaps are open). After OK, the selected sector's panel carries the
   action button — the only Button under `Center/Panel/Layout/Buttons` whose
   parent is not the row itself:

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/planet\")\nvar row = d.get_node_or_null(\"Center/Panel/Layout/Buttons\") if d != null else null\nvar inner = row.find_children(\"*\", \"Button\", true, false).filter(func(b): return b.is_visible_in_tree() and b.get_parent() != row) if row != null else []\nvar b = inner[0] as Control if not inner.is_empty() else null\nvar p = b.get_global_rect().get_center() if b != null else Vector2(-1, -1)\nreturn {\"x\": int(p.x), \"y\": int(p.y), \"n\": inner.size()}"}}
   ```

4. On current HEAD the action opens the launch-loadout picker
   (`_open_launch_loadout`, client/ui/dialogs/planet_dialog.gd:683) instead of
   launching directly; wait for `DialogLayer/launch_loadout.visible`, resolve its
   first Button row entry and click it (same press/release + flush as step 2) to
   fire `loadout_chosen` and launch:

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/launch_loadout\")\nvar row = d.get_node_or_null(\"Center/Panel/Layout/Buttons\") if d != null else null\nvar btns = row.get_children().filter(func(c): return c is Button and c.is_visible_in_tree()) if row != null else []\nvar b = btns[0] as Control if not btns.is_empty() else null\nvar p = b.get_global_rect().get_center() if b != null else Vector2(-1, -1)\nreturn {\"x\": int(p.x), \"y\": int(p.y), \"n\": btns.size(), \"text\": str((b as BaseButton).text) if b != null else \"\"}"}}
   ```

5. Launch and confirm the HUD:

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": host.get_tick(), \"sector_state\": c.get_sector_state(), \"group_counts\": str(host.call(\"get_group_counts\"))}"}}
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
- No error log entries during the sequence except the known loadout-arg
  `start_sector` fallback (see failure modes).

## Failure modes

- A click that is never flushed leaves the menu unchanged; the flush in step 2
  is mandatory.
- Hover state is read on the NEXT frame after `godot_input`; use
  `gui_get_hovered_control()` in a follow-up eval, not the same call.
- Verified 2026-10-08 at `9efd068` (EV-0037 run): the UI steps land as written —
  Play (230,149) → Campaign (460,149) → serpulo card (412,194) → OK (499,631),
  each as discrete `mouse_button` press/release + `Input.flush_buffered_events()`
  (a `sequence` call timed out on this host once; discrete calls are the safe form).
- Verified 2026-10-09 at `ad25ae5` including the launch-loadout confirm: Play
  (230,149) → Campaign (460,149) → serpulo card (412,194) → OK (499,634) →
  sector action (576,613) → launch-loadout confirm (367,503), all discrete, each
  resolved live via `get_global_rect().get_center()`. Result pid 129492:
  `sector=15`, `hasCore=true`, `group_counts.build=61`, `step(60)` → tick 70,
  `gameOver=false`, `winWave=10`. A `godot-rust function call failed:
  MindCampaign::start_sector() ... received 3 arguments` log line is the expected
  loadout-arg fallback (`client/ui/dialogs/planet_dialog.gd:747`), not a launch
  failure.
- No HUD after the sector action: the launch-loadout picker is probably open
  (`DialogLayer/launch_loadout.visible`) and unconfirmed — click its first
  Button row entry (step 4) before hunting coordinates.
- The sector panel derives state live: `MindCampaign.campaign_views_json(planet)`
  projects `CampaignViews::from_campaign()` and the dialogs prefer it over the
  fixture (fix `1d8f75b`; sequence in `campaign-live-views-capture.md`). Flags
  refresh on every dialog open (`shown()` → `refresh_campaign_views`), so a
  capture is visible on reopen without restarting; the fixture only serves when
  the facade/endpoint is absent. Verify freshness from the dialog's own
  `campaign_views()` row plus the `MindCampaign ready (N save slot(s))` log, not
  the label alone.
- Campaign bootstrap/launch gaps (EV-0035 family) can make step 3 fail before any
  click is wrong. Record the facade result and stop; do not hunt random coordinates.
- `hasCore: false` after the facade launch means the running binary predates
  `e85732c` (map-building materialization) or the sector has no preset map; use
  groundZero (sector 15), not 170. A stocked core also requires the loadout to
  pass `start_sector_with_loadout`, not the bare `start_sector`.
- Java comparison (historical): `computer-mcp` crashed at import without
  `DISPLAY` (pynput) and opencode silently dropped its tools; the Godot leg is
  unaffected. The Java leg now runs `java_input.py` with `DISPLAY=:10`.
