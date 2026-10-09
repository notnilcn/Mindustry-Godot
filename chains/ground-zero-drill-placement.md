---
id: ground-zero-drill-placement
title: Ground Zero — research the Mechanical Drill, select it in the picker, place it on copper (R04 Godot leg)
status: verified
applies_when: >-
  Driving PAR-0046-family items: the tutorial R04 action (research the
  Mechanical Drill -> select it in the placement pane -> place it on the
  copper patch) against a launched Ground Zero sector in the Godot client.
preconditions:
  - boot-and-identity + ground-zero-click-mining path replayed (launched sector, player unit at the core, mining works).
  - The fork (objective 1 active) needs the core to hold >= 15 copper: click the tutorial ore tile (137,53) with no block selected and let the player offload run (MinerComp offload branch, PAR-0046 round 1).
  - The session worktree is built (tools/build.sh) and the editor relaunched.
tools: [godot_exec, godot_input, godot_game, godot_screenshot, godot_log]
last_verified: 2026-10-10 commit 6fb36fe (76cce64 lib), twin run runs/20261009-162304-PAR-0046-twin/ (pid 280832; writer leg runs/20261009-153006-PAR-0046/, pid 263804)
---

# ground-zero-drill-placement

The R04 action: `tech tree -> research Mechanical Drill (10 copper) -> select
the drill in the placement pane -> place it on the gold marker (the copper
patch)`. Java side: `chains/java-ref-ground-zero-tutorial.md` R04.

## Steps

1. **Fork** — launch the sector: `SimHost.load_sector("serpulo",15)` (node
   `/root/Spine/SimHost` — NOT MindCampaign, that method does not exist),
   `MindCampaign.start_sector("serpulo",15)`, `SimHost.step(3)`; wait ~6 s,
   `UiRoot.set_menu_visible(false)` (`/root/Spine/Ui/UiRoot`), press/release `q`
   (clear the block selection), flush. Resolve the tutorial ore and click it:

   ```
   godot_exec {"action":"eval","params":{"code":"return str(get_node(\"/root/Spine/World/Camera2D\").call(\"tile_to_screen\", 137, 53))"}}
   # -> (832, 388) at the spawn camera (1032,440); motion + press + release + flush
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nreturn str(get_node(\"/root/Spine/SimHost\").call(\"player_mine_tile\"))"}}
   # -> [137, 53]
   ```

   Wait ~6 s. Check the fork: `MindCampaign.get_objective_state()` ->
   objective 0 `completed: true`, objective 1 `buildcount` active; the HUD
   status shows `Obtain: N/15`; `core_items_json` holds the copper.

2. **Research** — open the dialog and select the `<null>` root rail (second
   rail button, the campaign tree's unpinned branch), scroll the node table,
   and click the Mechanical Drill row:

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/MindUi\").open_dialog(\"research\", JSON.stringify({\"planet\": \"serpulo\"}))"}}
   # rail button 2 at (98, 95): motion + press + release + flush
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nvar cur = d._nodes\nwhile cur != null and not (cur is ScrollContainer):\n\tcur = cur.get_parent()\ncur.scroll_vertical = 732\nreturn {\"v\": cur.scroll_vertical}"}}
   # scroll takes a frame: re-resolve the button (rows start below the viewport)
   ```

   The drill row resolves to (660, 237) after the scroll; hover-verify the
   hovered control is `Mechanical Drill`, click it, then assert
   `campaign_views_json("serpulo")` -> `mechanical-drill` `unlocked: true`.
   `Escape` closes the tree.

3. **Picker** — the pane catalogue is the unlock-filtered campaign catalogue:
   *before* any research it reads empty, so after the unlock collapse/expand
   the pane to reload (`close_palette` then `open_palette` in **separate**
   evals — same-frame they leave auto-renamed duplicate buttons). Resolve the
   live category buttons (skip `is_queued_for_deletion()`), click
   `Production` (1123, 405), then the `block-mechanical-drill` button
   (866, 403); assert `SimHost.selected_block() == "mechanical-drill"`.

4. **Place** — the objective-1 ShapeText marker sits at world (1084, 412)
   (`MindCampaign.objective_markers_json()`); at the spawn camera that is viewport
   (784, 436). Hover (placement preview) and click:

   ```
   godot_input {"action":"mouse_motion","params":{"coords":"viewport","position":{"x":784,y":436}}}
   # press + release + flush
   ```

5. **Assert** — the drill is on the map and the objective advanced:

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar a = c.call(\"get_objective_state\")\nreturn {\"obj0\": str(a[0]), \"obj1\": str(a[1]), \"obj2\": str(a[2])}"}}
   # obj0 completed, obj1 completed (BuildCount mechanical-drill), obj2 active
   ```

   `get_state_json()` lists `mechanical-drill` at (136, 52) (the 2x2 anchor
   under the marker; footprint covers the ore tile (137,53)). Capture with
   `godot_screenshot game`; diff against the Java action frame
   `runs/20261009-065416-ref-ground-zero/java/r04-05-placed.png`.

## Success signals

- Objective 0 completes as the core crosses 15 copper (offload branch:
  `core_items_json` grows while mining, the unit stack stays empty).
- Research debits the core and flips `mechanical-drill` to unlocked.
- The placement carries team `sharded` (1): the editor log shows the
  `placed-debug` team line in instrumented builds; the stat increment feeds
  `BuildCount` and objective 1 completes on the next objective tick.
- Objective 2 (`Move into Core: 0/5 Copper`) becomes the current objective.

## Failure modes

- Mining with no core transfer (pre-round-1 builds) fills the unit stack (30)
  and clears the mine order; the objective counter stays 0/15.
- A too-long movement hold pans the camera far and drags the unit out of mine
  range; the tutorial ore is within range of the spawn — mine stationary.
- The same-frame `close_palette`+`open_palette` leaves auto-renamed duplicate
  buttons (`@Button@NNNN`) whose stale names pin `get_node` lookups; keep the
  calls one frame apart and filter `is_queued_for_deletion()`.
- `get_rules_json` + `JSON.parse_string` on the full rules payload times out;
  the objective state comes from `get_objective_state`/`campaign_views_json`.
- Eval errors (unverified facade methods) pin the loop: recover with
  `godot_game stop` + `play` — the fork state is not saved, so the chain above
  must be replayed. `/root/MindInput` does not exist (the input node is
  `/root/Spine/Input`); an eval calling `get_input_state_json` on the bad path
  errored and pinned the frame loop (twin run 2026-10-10). Also: `stop`
  autosaves the campaign, so re-run `clear_planet_campaign_saves` after the
  restart.
- After the research unlock + palette collapse/expand the category buttons can
  still be auto-renamed (`@Button@NNNN`) even with the calls one eval apart;
  resolve them by iterating the container, skipping `is_queued_for_deletion()`,
  and matching `tooltip_text` ("Production").
- The placement hover ghost may not render from an injected `mouse_motion`
  (twin run 2026-10-10: empty marker in the hover frame); the press+release at
  the same point still places — judge the placement from `get_state_json`, not
  the preview.
