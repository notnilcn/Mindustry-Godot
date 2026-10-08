---
id: placement-picker-audit
title: Campaign launch -> in-game placement picker audit (panel rect, category rail, block grid, catalog filtering)
status: verified
applies_when: Verifying the PlacementFragment palette after a campaign launch — icon sizes, unlock filtering (locked/empty categories hidden), panel clipping, catalog endpoint counts (EV-0013 family).
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - Fresh campaign profile if the check depends on the starting unlock set; clear leftover sector saves (`clear_planet_campaign_saves("serpulo")` + restart) and assert `list_save_slots() == []` and `MindCampaign.get_tech_state().unlocked == 9` before launching. Persistent `user://config/settings.bin` unlocks survive restarts (see learnings 2026-10-08).
  - The editor process must postdate the built `libmind_gdext.so`; a loop editor started before `tools/build.sh` may hold the previous library. Relaunch it after a rebuild (killed 14:03 editor, relaunched 14:28 over the 14:24 build in the verified run) and confirm in-engine with `MindCampaign.has_method("block_catalog_json")` (false = stale/pre-fix binary).
  - Icon checks need the generated `assets/sprites/sprites.atlas.json` in the worktree (gitignored, made by `tools/pack.sh`); probe `MindAssets.region_count() > 0` first or the run only proves the text fallback.
tools: [godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_input, godot_screenshot, godot_log]
last_verified: 2026-10-08 986cda3 (runs/l2-20261008-145144-ev0013-block-picker-godot)
---

# placement-picker-audit

## Steps

1. Launch the loop editor, then boot the entry scene and pid-stamp; `godot_log clear`:

   ```
   godot_editor_edit {"action":"open_scene","params":{"path":"res://scenes/game.tscn"}}
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"project\": ProjectSettings.globalize_path(\"res://\"), \"tick\": h.get_tick(), \"slots\": str(c.list_save_slots()), \"unlocked\": c.get_tech_state().get(\"unlocked\"), \"campaign_has_block_catalog\": c.has_method(\"block_catalog_json\")}"}}
   ```

   Expect `runtime_connected: true` at 1152x648. `campaign_has_block_catalog` records whether
   the live filter path exists (it is `false` at a1ca816 — b390354's `block_catalog_unlocked`
   has no `MindCampaign` caller).

2. Drive the launch flow, resolving each center live with
   `(node as Control).get_global_rect().get_center()` and sending one discrete
   press/release + `Input.flush_buffered_events()` per click (never hardcode; centers held
   at 1152x648 in the verified run):

   Play `menu/Sidebar/Buttons` index 0 (230,149) -> Campaign submenu index 0 (460,149) ->
   Serpulo card (412,194) -> OK (499,631) -> sector-page action button (576,613).

   The `planet` dialog keeps its node name but swaps content after OK; re-resolve the action
   button each time. One `godot_input sequence` call per click (press+release,
   `frame_delay: 1`) lands every step; only the `Submenu/Buttons` first real button is
   child index 1 (child 0 is the layout spacer). The sequence payload is
   `{"steps":[{"type":"mouse_button","params":{...}}, ...], "frame_delay":1}` — the flat
   `events[]` form returns `INVALID_ARGUMENT: steps[] required`.

3. Wait for the HUD (1 frame is enough; no `await` in eval):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"tick\": h.get_tick(), \"menu\": get_node(\"/root/Spine/Ui/UiRoot/MenuGroup\").is_visible_in_tree(), \"hud\": get_node(\"/root/Spine/Ui/UiRoot/HudGroup\").is_visible_in_tree(), \"sector_state\": str(c.get_sector_state())}"}}
   ```

   Expect `menu=false`, `hud=true`, `sector_state.hasCore=true`, `sector=15`,
   `wasCaptured=false` (Ground Zero). The fragment builds its catalog on first reveal
   (`visibility_changed` -> `_ensure_loaded`), so read the panel only after this.
   `_build_categories`/`_rebuild` free old children with `queue_free()`, so a panel read in
   the *same eval* as a rebuild still lists the queued children — read the panel in a
   follow-up eval (or one frame later) before judging counts (hit while re-reading after
   `clear_planet_research`).

4. Audit the panel at `/root/Spine/Ui/UiRoot/HudGroup/placement`:

   ```
   godot_exec {"action":"eval","params":{"code":"var p = get_node(\"/root/Spine/Ui/UiRoot/HudGroup/placement\")\nvar r = (p as Control).get_global_rect()\nvar cats = []\nfor b in p.get_node(\"Panel/Layout/Categories\").get_children():\n\tvar br = (b as Control).get_global_rect()\n\tcats.append({\"name\": str(b.name), \"w\": int(br.size.x), \"h\": int(br.size.y), \"icon\": b.icon != null, \"glyph_child\": b.get_child_count() > 0})\nvar blocks = []\nfor b in p.get_node(\"Panel/Layout/Scroll/Blocks\").get_children():\n\tvar br = (b as Control).get_global_rect()\n\tblocks.append({\"name\": str(b.name), \"w\": int(br.size.x), \"has_icon\": b.icon != null, \"text\": str(b.text)})\nreturn {\"pid\": OS.get_process_id(), \"panel_rect\": {\"x\": int(r.position.x), \"y\": int(r.position.y), \"w\": int(r.size.x), \"h\": int(r.size.y)}, \"visible\": p.is_visible_in_tree(), \"selected\": p.get(\"_selected_category\"), \"categories\": cats, \"blocks\": blocks}"}}
   ```

5. Cross-check the catalog endpoint and the live unlock state (the assertion for filtering):

   ```
   godot_exec {"action":"eval","params":{"code":"var parsed = JSON.parse_string(str(get_node(\"/root/MindUi\").call(\"block_catalog_json\")))\nvar per = []\nfor cat in parsed.get(\"categories\", []):\n\tper.append({\"cat\": str(cat.get(\"name\", \"\")), \"count\": cat.get(\"blocks\", []).size()})\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar a = get_node(\"/root/MindAssets\")\nreturn {\"pid\": OS.get_process_id(), \"per_category\": per, \"total\": per.size(), \"unlocked\": c.get_tech_state().get(\"unlocked\"), \"campaign_has_block_catalog\": c.has_method(\"block_catalog_json\"), \"assets_region_count\": a.call(\"region_count\")}"}}
   ```

   Expected (Java `PlacementFragment.getUnlockedByCategory`): only visible+unlocked+placeable+
   environmentBuildable blocks per category, empty categories omitted, no picker content when
   nothing is unlocked; a fresh Ground Zero has `unlocked=9` (auto-unlock roots), so late-game
   blocks (foreshadow/spectre/meltdown/malign) must be absent. At 986cda3 the verified fresh-GZ
   result is **both endpoints 0 categories and the panel 0 category/block buttons** (the old
   7352edb result was `distribution:186` + `effect:1` of non-buildable world/editor rows —
   `air`, `spawn`, `build1..16`, floors, ores, legacy factories, `command-center`,
   `core-bastion` — none of which pass Java `unlocked(block)`), so any such row at fresh GZ is
   a regression. `assets_region_count=0` means icon observations are environment-limited, not
   a code verdict.

5b. Positive half (one unlock). Ground Zero starts with core `{copper:100}`
   (add_starting_items), enough for the `duo` node. Run the real tech API and re-read the
   endpoints + panel (refresh with a `visible=false; visible=true` toggle, then read in a
   later eval):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"ok\": c.research(\"duo\"), \"tech\": c.get_tech_state()}"}}
   ```

   At 986cda3 `duo` appears in the `turret` category (panel: 8 category buttons 50x50, block
   buttons 46px min; icon `null` without an atlas) — but the same call also runs
   `tech_tree::check_auto_unlocks` and grants ~60 further nodes (`unlocked 9->70`, log
   `research auto-unlocked 60 content`), because the pass tests the generated tree's empty
   `node.requirements` instead of `effective_requirements()`. That cascade is not the catalog
   filter (the catalog mirrors the store exactly) and is already recorded in EV-0052's notes;
   judge the exact-set property at the store level (`ui/campaign.rs`
   `unlocked_catalog_filters_visibility_and_tech_gates`) or after a real fix to the auto-unlock
   pass. Restore with `c.clear_planet_research(\"serpulo\")` when done probing.

6. Capture the frame and logs:

   ```
   godot_exec {"action":"eval","params":{"code":"get_window().grab_focus()\nreturn OS.get_process_id()"}}
   godot_screenshot {"action":"game"}
   godot_log {"action":"errors","params":{"include_warnings":true}}
   ```

7. Teardown: no pause/camera was toggled by the UI launch; `godot_game {"action":"stop"}`.

## Success signals

- HUD visible with `hasCore=true` at sector 15; panel rect bottom-right inside the viewport
  (640x260 at (512,388) at 1152x648).
- Catalog counts match the live unlock state; categories/blocks listed by
  `block_catalog_json` equal the rendered rail/grid.

## Failure modes

- The sector action label is always `@sectors.go` (fixture read model); a fresh launch still
  happens when no save exists — verify freshness from `list_save_slots()==[]` /
  `wasCaptured=false` / `hasSave`, not the label.
- An eval body with `await` or a `for`/`while` loop times out and can freeze the frame loop;
  keep evals loop-free and await-free (see `open-godot-mcp-learnings.md`).
- If `MindAssets.region_count()==0`, block buttons silently fall back to text (67-120px) and
  the world may render blank; do not attribute that to the picker code.
- The catalog is cached (`MindUi.block_catalog_cache`); at 7352edb the fragment reloads it on
  every HUD reveal and launch/research call `invalidate_block_catalog`, so a launch-order read
  is live. On older commits the fragment loads once (`_loaded`) and there is no invalidation —
  restart the game process for a clean read unless the code under test invalidates it.
- `get_window().move_to_foreground()` (the screenshot-staleness workaround) emits a
  deprecation `[W]` in `godot_log errors`; it is evaluator-induced, not a game error. Filter it
  when checking "no new errors" (986cda3 run).
- A fresh-GZ `research(name)` probe moves the profile's unlock state (and the
  `check_auto_unlocks` cascade); the next evaluator's "fresh" baseline is only fresh if they
  clear saves/research or restart from a clean profile.

## Variants

- Facade launch (`enter-campaign.md` campaign-facade variant) when the UI flow is blocked;
  the panel audit steps are unchanged.
