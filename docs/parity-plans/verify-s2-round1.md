<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Plan verification, session 2, round 1 (2026-10-14): H00, H01, H09

Worktree `Mindustry-Godot-session2` @ `ffbe84dd` (fast-forwarded from main), bridge 6980, display `:11`.
Evidence: `runs/20261014-s2-verify/` (`verify-s2-evidence.json` holds the quoted eval results).
The H00 T1 (Change 1) and T2 (Changes 1-3) experiment was applied in the worktree only, then
reverted (`git checkout -- client/`), rebuilt and replayed; the worktree is clean.

Status legend: CONFIRMED (symptom reproduced), PARTLY, NOT CHECKABLE (headless-only or needs a
later tree), FAILING (acceptance smoke fails as the plan anticipated).

## H00

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 | CONFIRMED; fix works | `H00-T1-before-gz.png` (env 31, no unit), `H00-T1-after-gz-alpha.png` (env 233, alpha, log "core unit team 1 spawned") | `player_unit_pos` is (1036,444) until H01 T1; Change 7 = already applied on resume; `core/game.rs` path; `unit_id`/`planet_id` instead of non-existent `unit_by_name`/`planet_by_name` |
| T2 | CONFIRMED; fix works | evidence JSON; `H00-T2-plan-queued.png` | gmcp `place` defaults `team` 0 (pass `"team":1`); verify readback was racy -> pause, click, `step(10)`; camera follow overrides `center_on_tile` |
| T3 | FAILING (as anticipated) | `H00-T3-gameover.png`, evidence JSON | wave composition note (port off-by-one), causes listed with owners |
| T4 | NOT CHECKABLE | — | none |

H00 experiment result: Change 1 alone gives `rules.env == 233`, planet serpulo, the alpha spawns
on the "Go" resume at (1036,444), 0 panics. T2 Changes 1-3 compile as written; with no unit both
`player:Some` place and break are dropped (no building, no plan, copper unchanged); with the unit
the click queues a plan (progress 0.31 after 10 ticks) with the build beam. Respawn after death
works live with H00 (~60 ticks).

## H01

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 | CONFIRMED | `best_core_position` (1032,440) vs alpha (1036,444) | verify text: `building_debug` has no position; `best_core_position` is px |
| T2 | CONFIRMED (c, d) | `H01-T2-horizon.png` | dagger range 150 not 60 (162 vs 154 px; test distance 161) |
| T3 | PARTLY | evidence JSON (respawn 60 ticks; possession round trip OK) | dock types = evoke/incite/emanate only; kill stand-in `debug_apply_status(id,"melting",2000)` |
| T4 | CONFIRMED | evidence JSON | acceptance numbers: Java dagger drift ≈ 1 px/600 ticks (not > 4); use a flare (≈ 5.9 px); test value fixed |
| T5 | NOT CHECKABLE | code | none |
| T6 | CONFIRMED | evidence JSON (flare gone in ≤ 5 ticks) | kill stand-in noted |
| T7 | NOT CHECKABLE | code | none |
| T8 | CONFIRMED | evidence JSON | verify tiles: panel (146,62), generator (150,40) |
| T9 | CONFIRMED (NoAI) | evidence JSON (dummy moved 60 px on Onset) | none |
| T10 | PARTLY | Onset rules dump | Onset has malis cores but no `buildAi`; 225 unverified |
| T11 | NOT CHECKABLE | code | none |
| T12 | PARTLY | `content_list("unitCommand")` | assist = command index 3 (plan said no list exists) |
| T13 | NOT CHECKABLE | code | none |
| T14 | FAILING | same run as H00 T3 | duo range 160 not 110; causes outside this plan noted |

## H09

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 | CONFIRMED | Onset rules fog/staticFog true; `fog_query` placeholder | Onset core tile (276,107); `best_core_position` px; limit-area note |
| T2 | CONFIRMED (code) | eval: (129,120) rejected without fog | probe tile -> (112,35), `"team":1`, literal block 216 |
| T3 | CONFIRMED | radar powered, no progress readout | none needed (fallback "add radarProgress" required) |
| T4 | CONFIRMED | derelict drill mined 3 copper, enabled true | pre-T4 recipe (team-0 command place) noted |
| T5 | CONFIRMED (code) | coal container death: 0 bullets | no fire seam / no fire group noted |
| T6 | CONFIRMED (code) | — | no liquid seam / puddle group noted |
| T7 | CONFIRMED | battery + coal container deaths: 0 bullets | battery capacity 4000 -> 6 bolts (not 2); tiles (140,60)/(140,66)/(143,66) |
| T8 | CONFIRMED | Onset malis core `{}` with fillItems true | Onset recipe replaces frontier/50 |
| T9 | CONFIRMED | Onset bastion stored 23 sand | none (recipe works) |
| T10 | CONFIRMED | 2 walls built, `built` 0 | none (note: compare deltas) |
| T11 | PARTLY | Java source | GZ resume resets fog in Java too -> use Onset |

## Java leg

Ground Zero wave-1 composition: Java NOT observed (Java GZ in the seeded profile opens paused with
the "Obtain 0/15 Copper" objective; wave timer off, no skip panel; `space` did not unpause within the
budget). Composition derived from source instead (`WaveSpawner.java:74`, `Logic.java:319-321`).
Frames `java-0*.png`. Session 3's JVM (DISPLAY :12) matches the same `pgrep` pattern — left alone.

## Hiccups (logged in the learnings files)

* `center_on_tile` overridden by camera follow; click readback race on fast player builds;
  `best_core_position` is px; `block_size` wrong-type eval timed out; one transient
  `No Godot instance connected`; gmcp `place` default team 0 (open-godot-mcp-learnings.md).
* Java GZ tutorial save cannot spawn a wave; pid ownership check for parallel sessions
  (java-leg-learnings.md).

## Coordinator decisions needed

1. **Wave index off-by-one (not in any plan).** `MindCampaign.run_wave` and the timer path
   (`core/game/runtime.rs:402`) hand `run_wave_system` the pre-increment wave and it calls
   `get_spawned(wave)`; Java spawns `getSpawned(state.wave - 1)` before `state.wave++`. Ground Zero
   wave 1 is 1 dagger in Java, 1 dagger + 2 flares in the port. Owner? (H04 owns run_wave.)
2. **Ground path cost (not in any plan).** The live flowfield (`core/ai/pathfinder/flowfield.rs`)
   uses uniform costs and `!solid` passability; Java `costGround` (`Pathfinder.java:56-62`) adds
   health*5, near-solid +2, near-liquid +6, deep +6000, damages +30, makes all-deep tiles impassable
   and other-team buildings passable. Wave daggers on Ground Zero drown in deep water. H09 §1.1 lists
   "Pathfinder cost types -> H01", but H01 has no task.
3. **Turret target lead (not in any plan).** Port turrets aim at the target's current position
   (`turrets/mod.rs:1829`); Java `Turret.targetPosition` uses `Predict.intercept` (`predictTarget`
   default true, `Turret.java:459-471`). The duo hits flares ~18%; likely the main reason the
   Ground Zero core dies. H02?
4. H00 T3 / H01 T14 acceptance: with (1) fixed, wave 1 is a single dagger; decide whether the smoke
   should run wave 2 (1 dagger + 2 flares) to keep testing air defence.
5. `H-environment.md` (not edited here): §4.2 says `best_core_position()` returns a core tile — it
   returns world pixels; §4.1 "player alpha spawns at world (1032,440)" is (1036,444) until H01 T1;
   the 4.1 "copper ore at (130..131,48..49)" recipe works; the Ground Zero playable area is the
   limit area x 90-159, y 20-79 (many tiles near the core are ice/snow walls).
6. Port HUD objective differs on Ground Zero: Java shows "Obtain 0/15 Copper", the port
   "Build: mechanical-drill x1" (`java-05-gz-in.png` vs `H00-T1-after-gz-alpha.png`) — H05?
