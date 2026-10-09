---
id: ground-zero-conveyor-line
title: Ground Zero — research/place a conveyor line so the drill feeds the core (R05 Godot leg)
status: verified
applies_when: >-
  Driving PAR-0049-family items: the tutorial R05 action (place a conveyor line
  from the placed Mechanical Drill into the core; the core intakes advance
  objective 2).
preconditions:
  - ground-zero-drill-placement replayed (drill at (136,52), objective 2 active,
    core stocked by the mining phase).
  - The session worktree is built (tools/build.sh) and the editor relaunched.
  - Port research persists across launches; call `MindCampaign.clear_research()`
    before a fresh launch (see open-godot-mcp-learnings.md).
tools: [godot_exec, godot_input, godot_game, godot_screenshot, godot_log]
last_verified: 2026-10-10 commit pending (runs/20261009-164959-PAR-0049/, pid 320985)
---

# ground-zero-conveyor-line

The R05 action: `select the Conveyor in the placement pane -> lay a line from
the drill to the core -> deliveries raise the core; objective 2 (Move into
Core: 0/5 Copper) completes; objective 3 becomes current`. Java side:
`chains/java-ref-ground-zero-tutorial.md` R05.

## Coordinate conventions (the important part)

- Screen mapping at the spawn camera (1032,440): `tile_to_screen(135,51) =
  (768,452)`, `(137,53) = (832,388)`; one tile = 32 px, **y is inverted on
  screen** (higher tile y = higher on screen).
- **Building rotation (y-up world, matches Java)**: `0 = east (+x)`,
  **`1 = north (+y)`**, `2 = west (-x)`, `3 = south (-y)`. The tutorial core
  sits NORTH-WEST of the drill: its footprint is (128..130, 54..56) while the
  drill anchors at (136,52), so the line runs **west then north**.
- Belts feed their **back** and push to their front: a west-facing belt accepts
  from the east (its back), and the final west-facing belt at (131,54) pushes
  into the core tile (130,54).

## Steps

1. **Fork** — replay `ground-zero-drill-placement.md` to the drill placement
   (objective 1 completes; objective 2 `coreitem` active). Capture the fork.

2. **Conveyor selection** — the pane's catalogue is the unlock-filtered
   campaign catalogue; the Conveyor is boot-unlocked: press/release `q` off,
   then collapse/expand the pane (`close_palette` then `open_palette` in
   SEPARATE evals) and click the `Distribution` category (near (1069,459)),
   then the `block-conveyor` button (near (866,403)).

3. **Rotation** — the `rotate` bind is a scroll axis: one
   `MOUSE_BUTTON_WHEEL_UP` press event = **+1** rotation (press+release = +2);
   `MOUSE_BUTTON_WHEEL_DOWN` press = -1. Verify each change via
   `MindInput.get_input_state_json() -> rotation`.

4. **Line** — with rotation 2 (west):
   - click tiles (135,53) `(768,388)`, (134,53) `(736,388)`, (133,53)
     `(704,388)`, (132,53) `(672,388)` — west-facing row;
   - wheel-down to rotation **1 (north)**, click (131,53) `(640,388)` — the
     corner (a south-facing corner is WRONG: `next=None`, the line stalls);
   - wheel-up back to 2 (west), click (131,54) `(640,356)` — the drop into the
     core.

   Verify the tiles + rotations in `get_state_json()["world"]["tiles"]`.

5. **Deliveries** — the drill offloads each produced item to the adjacent belt
   (`transfer::offload` proximity loop; the multiblock facing-edge fix makes
   the 2x2 drill's anchor diagonal case work); the belts carry west, turn
   north, and the last belt `pass`es into the core (`handle_item` ->
   `stats.coreItemCount`). Core copper rises; objective 2 completes; the
   objective list advances to objective 3 (the 100-copper objective).

6. **Assert** — `get_objective_state()`: objective 1 true, objective 2 true,
   objective 3 active; `core_items_json` climbing. Capture with
   `godot_screenshot game`; diff against the Java action frame
   `runs/20261009-065416-ref-ground-zero/java/r05-22-delivered2.png`.

## Success signals

- The drill's module cycles (full -> draining) once the line's first belt is
  adjacent; core copper rises at the drill's rate.
- Objective 2 (`CoreItem 5`) completes only from belt intakes: the player's
  mining offload uses `handleStack` and does not count, and the map's
  pre-placed stock does not count either.
- The reference after-state shows objective 3 current with the "Expand the
  mining operation..." prompt.

## Failure modes

- **South-facing corner**: with y-up rotations, `3` is south; a corner placed
  at rotation 3 points at the empty tile below the row, `next=None`, and every
  belt backs up (diagnose with the belt `len`/`next` debug). Use rotation 1.
- Same-frame `close_palette`+`open_palette` leaves auto-renamed duplicate
  buttons; keep the calls one frame apart and filter
  `is_queued_for_deletion()`, or resolve by tooltip text.
- A 2x2 source diagonal to the belt (a drill's anchor vs its footprint edge)
  is rejected without the `Edges.getFacingEdge` clamp; the fix is in
  `conveyor.rs::facing_edge`.
- The research dialog is opening E/W vs N/S: the node scroll needs one eval to
  set and one to settle; resolve the target row in a later eval.
- Eval errors pin the frame loop: recover with `godot_game stop` + `play` and
  replay from step 1 (the fork is not saved).
