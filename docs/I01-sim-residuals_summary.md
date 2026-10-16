# I01 — Simulation residuals: executor summary

Plan: `docs/parity-plans/I01-sim-residuals.md`; environment: `docs/parity-plans/I-environment.md`.
Worktree: `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session2`, branch `parity/session-2`,
bridge 6980, display `:11`. Baseline: `c2ba1f69` (I00 marker; mind-core 2551 / headless 133 / gdext 47 / goldens 97).

# Pass 1 (session 2, 2026-10-15)

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T1 CoreItem objective text | done | `Gameplay I01 T1` (15ac41d0; merged to main); headless test + in-engine panel/`coreItems` |
| T2 closed doors solid | done | `Gameplay I01 T2` (8a5fe999; merged to main); 3 headless tests + in-engine pinned dagger |
| T3 collideLine rounding | done | `Gameplay I01 T3` (d64a8911; merged to main); `collide_line_rounds_like_world_to_tile` |
| T4 pathfinder health + control costs | done | `Gameplay I01 T4` (0080fc57; merged to main); `damaged_wall_is_cheaper_to_path_through`, `control_path_prefers_shallow_over_deep` |
| T5 `BulletType.rangeChange` | done | `Gameplay I01 T5` (56c79542; merged to main); `duo_graphite_range_is_176` |
| T6 air/ground area damage | done | `Gameplay I01 T6` (fd5af539 + fixup 9769335e; merged to main); `ground_only_splash_spares_flyers`, `logic_explosion_air_only` |
| T7 Erekir `requiresCoreZone` | done | `Gameplay I01 T7` (2987a714); `bastion_requires_core_zone` |
| T8 darkness flag, dead code, markers | done | `Gameplay I01 T8` (79396fbf); `env_blocks_ignore_build_darkness`, `serpulo_preset_markers_regions_are_empty` |
| T9 coolant uses the current liquid | done | `Gameplay I01 T9` (8df5a940 + fixup 357bb2c2); `coolant_uses_current_liquid` |
| T10 unit ids across saves + cargo loader | not reached | |
| T11 unit assembler save body + drones | not reached | |
| T12 turret block units + core control select | not reached | |
| T13 middle-click command queue | not reached | |
| T14 power graph re-placement relink | not reached | |
| T15 cheap fog debug readbacks | done | `Gameplay I01 T15` (6efa56c3); no-arg probe + rect probe verified in-engine |
| T16 verification sweep | partial (check 4 done; checks 1-3 not run) | `units_base_build` 27 queued plans (PAR-0168 symptom not reproduced) |

`git log --oneline main..HEAD` at stop (main already contains T1–T6 through
`a40cea26`):

```
b9533490 Merge branch 'main' into parity/session-2
6efa56c3 Gameplay I01 T15: fog debug readbacks stay cheap on large maps
357bb2c2 Gameplay I01 T9: fixup (restore the PAR-0087 test attribute)
8df5a940 Gameplay I01 T9: liquid-filter consumers use the current liquid
79396fbf Gameplay I01 T8: ignoreBuildDarkness, drop dead BeamDrillState, markers region decision
2987a714 Gameplay I01 T7: Erekir cores require the core zone
```

Earlier commits from this pass (now on `main`): `15ac41d0` T1, `8a5fe999` T2,
`d64a8911` T3, `0080fc57` T4, `56c79542` T5, `fd5af539` T6, `9769335e` T6
clippy fixup.

## Tests (exact commands + results, at the stop commit)

All from `WT/client/rust`, `export CARGO_BUILD_JOBS=6`, `L="flock /tmp/opencode/mg-build.lock"`:

* `$L cargo test -p mind-core --lib --features msav-import`
  -> `test result: ok. 2568 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out`
  (baseline 2551; +17 new tests: T1 1, T2 3, T3 1, T4 2, T5 1, T6 2, T7 1, T8 2, T9 1,
  plus 3 tests landed on `main` by I05)
* `$L cargo test -p mind-headless --lib` -> `133 passed; 0 failed` (run after T4)
* `$L cargo test -p mind-gdext --lib` -> `48 passed; 0 failed` (after T15)
* `$L cargo run -q -p mind-headless -- parity goldens` -> **97 PASS / 0 non-PASS** (run after T4 and T7)
* `$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` -> clean (after T8, T9 fixup, T15)
* `cargo fmt -p mind-core -p mind-gdext` after every task
* GDScript parse gate: not needed (no `.gd`/`.tscn` edits in this pass)
* Build: `CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh` ->
  `Finished dev profile [unoptimized + debuginfo] target(s) in 2m 04s` + `sync_scenarios: synced 11 file(s)`

## In-engine evidence (session 2 bridge 6980, display :11)

Run dir `runs/20261015-s2-I01-inengine/` (`notes.md`, `t1-objective-panel.png`).
Editor restarted with `tools/session-play.sh 2 --play` after the build; Ground Zero
entered through `load_sector("serpulo",170)` / `start_sector` /
`set_menu_visible(false)` (`get_hud_state` -> `hasCore: true`, `sectorName: "groundZero"`,
`has_player_unit`: true). Newest session log panic count: **0**; no new crash reports.

* **T1**: `get_node("/root/MindHud").status_text` ->
  `[color=#ffd37fff]Obtain: [/color][color=#bfbfbfff]0[/color]/15\n\uf838 Copper` and
  `$MC.game_stats_json()` -> `{"wave":0,...,"coreItems":{}}` (the new
  `stats.coreItemCount` readback the HUD locale consumes). Screenshot
  `t1-objective-panel.png`. No shipped map carries a `CoreItem` objective
  (`grep -c CoreItem assets/maps/**/*.msav` = 0), so the `objective.coreitem` text
  itself stays headless-only.
* **T2**: walls/door staged with `apply_sim_command_json` (team 1) and a team-2
  dagger spawned at (1112,480) on Ground Zero; after 480 stepped ticks
  `$H.unit_debug_json()` reports the dagger at `pos [1092.0, 457.21]`, `vel [0.0,
  0.0]`, `aim [1032.0, 440.0]` — pinned at x = 1092.0, the east face of the
  door/wall column at tile 136, with the player core west of it (two staged walls
  were shot away by the daggers, which is why the pin is at the surviving
  wall/door faces). The open-door half stays headless
  (`open_door_lets_unit_pass`).
* **T5/T6/T7/T8/T9**: no in-engine half captured in this pass (internal rules;
  covered by the headless tests).
* **T15** (second editor restart after the T15 build):
  `$H.fog_debug_json()` (no arguments - the call that stalled the writer) ->
  `{"fog":false,"staticFog":false,"teams":[],"discoveredTiles":0,"sources":1}`
  instantly; `$H.fog_debug_json_rect(1, 128, 54, 4, 3)` ->
  `{"x":128,...,"rows":[[1,1,1,1],[1,1,1,1],[1,1,1,1]]}`;
  `$H.debug_set_fog(true, false)` -> true, then `fog_debug_json()` ->
  `{"fog":true,...,"teams":[1],...}` and `fog_debug_tile_json(1,129,55)` (core
  tile) -> `visible: true, discovered: true`; fog restored to off; tick 83220
  advancing, panic count 0. Screenshot `t15-fog-debug-rect.png`.

Staging note (useful for later passes): 15 `apply_sim_command_json` placements in
one eval exceed the 15 s budget (each placement re-packs the pathfinder) and can
stall the frame loop; recover with `tools/session-play.sh 2 --play` (done twice).

## Deviations from Java (each with the Java location)

1. **T4**: Java does NOT re-pack a damaged building's pathfinder tile
   (`grep updateTile|BuildDamageEvent|Events.on J/ai/Pathfinder.java` — only
   `TileChangeEvent` at Pathfinder.java:150-154, team changes, door toggles,
   construct progress). The port therefore keeps the tile-change trigger and
   added no damage-driven re-pack; `packTile` already reads the live
   `build.health` (Pathfinder.java:255-270).
2. **T2**: Java's door refuses to CLOSE while `Units.anyEntities(tile)` holds a
   unit (`Door.java:49-51`); the port closes regardless (the chained-door path
   `set_open` has no unit query). Documented in the T2 commit body.
3. **T8**: the legacy `markers` region is still skipped (every shipped preset
   carries an empty UBJson map, 71/78 import + measured <= 2 bytes; the guard test
   is `serpulo_preset_markers_regions_are_empty`). Java's `MapMarkers.write`
   IntMap decode (`MapMarkers.java:57-59`) stays unported for now;
   `io::json::ubjson::read_ubjson` exists when a map ships markers.
4. **T9**: `TurretAmmo::Power` holds only the bullet id, so `peekAmmo().rangeChange`
   for power turrets reads 0 (no vanilla power turret sets `rangeChange`);
   `Turret.java:348-353`.
5. **T15**: the rect probe is `fog_debug_json_rect(team, x, y, w, h)` (the plan's
   `(x, y, w, h)` plus a leading team id, clamped to 64x64).
6. **T3** (not a deviation, a test fix): two line-damage tests fired from
   `tile_center` (`(x+0.5)*8`) which no longer rounds back into the source tile
   under `World.toTile`; they now fire from `tile_origin` (`tile*8`, Java
   `Tile.worldx()`).

## T16 partial (verification sweep check 4 only)

`CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock cargo run -q -p mind-headless
-- units scenario units_base_build --json` ->

```
{"scenario":"units_base_build","pass":true,"seed":11,"ticks":3600,"registered_parts":1,
 "queued_plans":27,"invalid_plans":0,"core_units_spawned":2,"path_found_at":2,
 "total_calcs":1,"checksum":"1aedac089f2e9e05"}
```
Matches the committed golden `mind-headless/tests/golden/units/units_base_build.json`
(27 queued plans, same checksum): **PAR-0168's "queues no plans (0 of 28)" does not
reproduce** - `BaseBuilderAI` queues 27 plans over 3600 ticks on the scenario. No
code change needed. Checks 1-3 (GZ wave 2 survival, sandstorm wind drift,
BuilderAI boost tail) were not run.

## Findings that deserve ledger items (not fixed in this pass)

1. **7 of 78 Serpulo presets fail to import** through the legacy reader with
   strict JSON errors (found while measuring markers, I01 T8):
   `hidden/176.msav` (invalid type: null, expected a string @27316),
   `hidden/247.msav` (invalid number @50265), `hidden/254.msav` (@20258),
   `hidden/263.msav` (@1784), `hidden/92.msav` (@9699),
   `infestedCanyons.msav` (@5851), `navalFortress.msav` (@4211). Reproduce with
   `cargo test -p mind-core --lib --features msav-import
   serpulo_preset_markers_regions_are_empty -- --nocapture` (prints the list).
2. **Eval-budget hazard**: `apply_sim_command_json` placement re-packs the whole
   pathfinder per call, so >10 placements in one 15 s eval stall the frame loop
   (recovered twice this pass). Worth documenting in I-environment §4.

## Ledger changes to apply (the coordinator owns `chains.json`)

| Item | After |
|---|---|
| new: "CoreItem objective shows stored items" | verified (T1 15ac41d0) |
| new: "Closed doors are not solid" | verified (T2 8a5fe999) |
| new: "Ammo rangeChange ignored" | verified (T5 56c79542) |
| new: "Area damage ignores air/ground" | verified (T6 fd5af539) |
| new: "Erekir cores ignore core zone" | verified (T7 2987a714) |
| new: "Legacy collideLine floored tile coordinates" | verified (T3 d64a8911) |
| new: "Pathfinder packs stale/def health; control paths use uniform cost" | verified (T4 0080fc57) |
| new: "env blocks blocked by build darkness; dead BeamDrillState; markers region" | verified (T8 79396fbf) |
| new: "Coolant consumers ignore liquids.current()" | verified (T9 8df5a940) |
| new: "fog_debug_json stalls on no arguments" | verified (T15 6efa56c3, in-engine) |
| new: "7 Serpulo presets fail the legacy import (rules JSON)" | seeded (finding above, no fix) |
| PAR-0138 | still fixing (T12 not reached; the payload half was verified by H08) |
| PAR-0286 (power relink) | still fixing (T14 not reached) |
| H02 T4 note ("requiresCoreZone treated false") | superseded by T7 |
| H06 T8 deviation (logic explosion air/ground) | superseded by T6 |
| H01 T15 deviation 4 (packed health) | superseded by T4 (Java-faithful trigger documented) |
| H02 range_change deviation (fog/runtime comments) | superseded by T5 |
| H02 T10 note (coolant first-slot pick) | superseded by T9 |

## Blockers

* None for the work that landed. T10–T14/T16 were not reached in this pass (budget);
  they are unblocked for the next pass: T10/T11 need the unit-id plumbing, T12 the
  possession path, T13 the command-log field, T14 the power-graph reflow, T16 is a
  verification sweep.

## Done criteria

`All tasks committed with their tests; suites green; summary in
REPO/docs/I01-sim-residuals_summary.md` — **not met**: T10-T14 remain and T16 is
partial (check 4 only). Everything committed is green (2551 -> 2568 mind-core,
133 headless, 48 gdext, 97 goldens, clippy clean) and T1-T9 + T15 have their
in-engine or headless acceptance checks.

# Pass 2 (session 2, 2026-10-15)

Worktree `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session2`, branch
`parity/session-2`, bridge 6980, display `:11`. Start state: main @
`fb513f47`+ (fast-forwarded at session start; later re-merged during the pass);
pass-1 commits T7-T9/T15 already on main. Main advanced to `498c4a12` (I07 pass
2) after this pass's last merge — the next pass must `git merge --no-edit main`
first.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T10 unit ids across saves + cargo loader body | done | `Gameplay I01 T10` (475fde36); 3 new tests; full suite green |
| T11 unit assembler save body + drone arrangement | done | `Gameplay I01 T11` (1dfa93c0); 4 new tests + rewritten assembler test |
| T12 turret block units + core control select | **not reached** (budget) | PAR-0138 stays `still fixing` |
| T13 middle-click command queue | done | `Gameplay I01 T13` (1eaf4bed); `queued_orders_execute_in_sequence` + simlog v2 |
| T14 power graph relink on replacement | done | `Gameplay I01 T14` (4a2bc539); failing-then-green test + two fixes |
| T16 checks 1-3 | done | check 4 pass 1; checks 1-2 in-engine (PASS), check 3 fixed + headless test; commit cd2d751f |
| T16 extra: live content boot regression | fixed | commit cd2d751f (see finding below) |

`git log --oneline main..HEAD`:

```
cd2d751f Gameplay I01 T16: live content boot installs turret ammo; BuilderAI boost tail
4a2bc539 Gameplay I01 T14: power graphs relink when a building is replaced
1eaf4bed Gameplay I01 T13: middle-click queued unit orders execute in sequence
1dfa93c0 Gameplay I01 T11: unit assembler save body and drone arrangement
47fe25b7 Merge branch 'main' into parity/session-2
475fde36 Gameplay I01 T10: unit ids persist across saves; cargo loader save body
```

## Tests (exact commands + results)

All from `WT/client/rust`, `export CARGO_BUILD_JOBS=6`, `L="flock /tmp/opencode/mg-build.lock"`:

* `$L cargo test -p mind-core --lib --features msav-import`
  -> final `test result: ok. 2610 passed; 0 failed; 4 ignored` (T10 +3 tests,
  T11 +4, T13 +1, T14 +1, T16 +1; 2551 baseline -> +59 overall vs I00)
* `$L cargo test -p mind-headless --lib` -> `133 passed / 0 failed`
* `$L cargo test -p mind-gdext --lib` -> `48 passed / 0 failed`
* `$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` -> clean
  (mind-headless clippy has two pre-existing `expect_used` errors in
  `campaign_scenarios.rs:663,1196`, byte-identical on main, not this pass)
* `$L cargo run -q -p mind-headless -- parity goldens` -> **97 PASS / 0 non-PASS**
* `CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh` ->
  `Finished dev profile ... in 26.88s` + `sync_scenarios: synced 11 file(s)`

## In-engine evidence

`runs/20261015-s2-I01-T16/` (`notes.md`, `t16-wave.json`,
`t16-wave2-survived.png`, `t16-wind.json`). Final build of this pass
(`cd2d751f`) + clean `tools/session-play.sh 2 --import`
(`{"nodes": 375.0, "unlocked": 282.0}`), Ground Zero via
`load_sector`/`start_sector`/`set_menu_visible(false)`.

* **T16 check 1 (PASS)**: drill (130,48) + duo (132,48), both fed; duo (131,50)
  player-stocked 30 copper (the 2x2 ore patch is fully covered by the first
  drill); `run_wave()` -> single dagger cleared in 3300 ticks; `run_wave()` ->
  1 dagger + 2 flares cleared in 3300 ticks (<=6000); after: `gameOver:false`,
  `hasCore:true`, `enemies:0`, core `core-shard` at (129,55) with 255 copper,
  both duos 25 ammo. Screenshot `t16-wave2-survived.png`.
* **T16 check 2 (PASS)**: `debug_create_weather("sandstorm",1.0,3600)` + team-1
  flare at (1150,420); 600 paused ticks -> wind (-0.9803,0.1974) (168.62deg),
  flare velocity magnitude **0.009431 px/tick** = the Java steady state
  **0.00943** quoted in `runs/20261014-s5-H01-T4/probe.json`; drift angle
  168.57deg = wind aligned; 600-tick displacement 5.88 px (the H01 probe's
  recorded 8.84 px includes extra movement; the velocity datum matches exactly).
* **T16 check 3**: no in-engine half; headless
  `builder_boost_tail_follows_moving`.
* **T10-T14**: no in-engine half required by their acceptance (headless tests);
  the live-build turret check below was the only in-engine probe needed.

## Finding + fix outside the task list (recorded for the coordinator)

The I07 merge's shared live boot (`content::boot::build_registry_with_mods`, used
by MindSimHost/MindCampaign/MindEditor) never called `install_turret_ammo`. The
live registry therefore held no turret ammo bullets, `defense::register` skipped
every turret, and **in-engine turrets had no `TurretState`** (a freshly placed
team-1 duo reported no `turret_total_ammo`; the drill kept its `DrillState`).
The headless harness uses `sim::runtime::load_vanilla_content`, which does
install them, so no test caught it. Fixed in `cd2d751f` (+ test
`live_boot_installs_turret_behaviors`). This touches `content/boot.rs`, which
session 4's I07 work also edits — expect a merge conflict.

## Deviations from Java (each with the Java location)

1. **T10**: id restore bumps the port's split allocators (`UnitRuntime.seq`,
   `UnitBlockSeq`, `Sim.entity_seq`) instead of Java's single static
   `EntityGroup.lastId` (`EntityGroup.java:36-44`); duplicates still reassign.
2. **T11**: `commandPos` is saved/restored but not handed to the produced unit —
   the port's `CommandAiState` lives in the Sim-owned `UnitCommandRuntime`, not
   an ECS component (`UnitAssembler.java:560-562`); the hand-off stays with the
   I08/T13 command wiring.
3. **T11**: the assembler's `checkSolid`/`wasOccupied` gate and the
   payload-to-`targetBuild` hand-off are not ported; the product spawns directly
   at `getUnitSpawn` (`UnitAssembler.java:466-472,516-532`).
4. **T16 check 3**: the builder boost tail's floor terms are limited to deep
   floors (no duct/`damageTaken` floor data yet) — same documented
   simplification as the MinerAI port (`BuilderAI.java:231-233`).
5. **T16 check 2**: measured 600-tick displacement 5.88 px vs the H01 probe's
   recorded 8.84 px; the steady-state velocity (the Java-derived parity datum)
   matches to 4 significant figures.

## Ledger changes to apply (the coordinator owns `chains.json`)

| Item | After |
|---|---|
| new: "Unit ids lost on save; cargo loader save body" | verified (T10 475fde36) |
| new: "Unit assembler save body + drone arrangement missing" | verified (T11 1dfa93c0) |
| new: "Queued unit orders apply as direct orders" | verified (T13 1eaf4bed) |
| new: "Power graph loses links on re-placement" (PAR-0286) | verified (T14 4a2bc539) |
| new: "Live content boot drops turret ammo (no turret behaviors in-engine)" | verified (T16 cd2d751f) |
| new: "BuilderAI boost tail unported" | verified (T16 cd2d751f) |
| PAR-0138 | still fixing (T12 not reached; payload half verified by H08) |
| PAR-0168 | verified already (pass 1, check 4) |

## Blockers

* None for the landed work. **T12 was not reached** (budget): it is unblocked
  (unit-id plumbing landed in T10; the possession path and
  `control_block_unit_id` are untouched), and it keeps its PAR-0138 entry.
* Main moved to `498c4a12` after the last merge in this pass; the next pass must
  merge `main` first (the T16 `content/boot.rs` change may conflict with I07).

## Done criteria

`All tasks committed with their tests; suites green; summary in
REPO/docs/I01-sim-residuals_summary.md` — **not met**: T12 remains. Everything
committed is green (2610 mind-core, 133 headless, 48 gdext, 97 goldens, clippy
clean) and T10, T11, T13, T14 plus T16 checks 1-3 have their acceptance checks;
check 1 and 2 were run in-engine on the final build.

# Pass 3 (session 2, 2026-10-16)

Merged `main` @ `3d029285` (pass-2 merge `40c57e3d` + I03 pass 2 + I06 pass 3)
before starting; the T16 `content/boot.rs` change merged cleanly.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T12 turret block units + core control select | **done** | `Fixes PAR-0138` (346da687); 3 new tests; in-engine screenshots |

`git log --oneline main..HEAD`:

```
346da687 Fixes PAR-0138: turrets carry block units the player can control; core control select respawns
```

## Tests (exact commands + results)

All from `WT/client/rust`, `export CARGO_BUILD_JOBS=6`, `L="flock /tmp/opencode/mg-build.lock"`:

* `$L cargo test -p mind-core --lib --features msav-import` ->
  `test result: ok. 2642 passed; 0 failed; 4 ignored`
* `$L cargo test -p mind-headless --lib` -> `133 passed / 0 failed`
* `$L cargo test -p mind-gdext --lib` -> `50 passed / 0 failed`
* `$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` -> clean
* `$L cargo run -q -p mind-headless -- parity goldens` -> **97 PASS / 0 non-PASS**

## In-engine evidence (T12, `runs/20261016-s2-I01-T12/`)

Build `tools/build.sh` + `tools/session-play.sh 2 --play`, Ground Zero (170),
bridge 6980:

* Ctrl-click the own duo (132,48) -> `player_unit_status` `alpha` ->
  **`{"type":"block"}` at [1056,384]** (the duo's centre); screenshot
  `t12-possessed-duo.png`.
* Aim + shoot: peak live bullets with LMB up = **0**; with LMB down = **3**
  (east and north aim); west = 0 because the adjacent drill blocks the bore.
  Screenshots `t12-controlled-fire-east.png`, `t12-controlled-fire-north.png`,
  `t12-controlled-turret-firing.png`.
* Ctrl-click the core (129,55) -> **`{"type":"alpha"}` at [1032,440.01]** =
  `best_core_position()` (1032,440); screenshot `t12-core-respawn.png`.
* Input recipe notes (for later sessions): the bridge key name for Ctrl is
  `"Ctrl"` (`"Control"` -> keycode 0), and the cursor only moves with
  `mouse_motion`; inject motion before Ctrl+click.

## Deviations from Java (each with the Java location)

1. The size-1 `router` ControlBlock is not ported (turrets and `build-tower`
   are); `Turret.canControl()` `playerControllable` is treated as always true
   (the port has no such block field; Java default true),
   `Turret.java:382-384`.
2. The `block` proxy is a detached ECS entity; every entity-wide unit scan that
   Java drives through `Groups.unit` now filters `BlockUnitComp` (fog, area
   damage, line bullets, targeting). `spawned()`'s
   `unit.controller() instanceof Player` check is approximated by the `Player`
   marker on the proxy.
3. `CoreBlock.onControlSelect`'s `net.client()` client half (input controlledType
   reset) is single-player only in the port, `CoreBlock.java:624-637`.

## Ledger changes to apply (the coordinator owns `chains.json`)

| Item | After |
|---|---|
| PAR-0138 | **verified** (T12 346da687; the payload half was verified by H08) |
| new (pass 3): "Turret block-unit proxies are fog/target/damage sources" | fixed in the same commit (scan filters) |

## Blockers

* None.

## Done criteria

`All tasks committed with their tests; suites green; summary in
REPO/docs/I01-sim-residuals_summary.md` — **MET**: T1-T16 are all committed with
their acceptance checks (T16 checks 1-2 in-engine on pass 2, check 3 headless,
check 4 pass 1; T12 in-engine on pass 3). Final gate: 2642 mind-core, 133
headless, 50 gdext, 97 goldens, clippy clean.
