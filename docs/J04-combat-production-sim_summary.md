<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J04 — Combat targeting and production/power simulation residuals (summary)

Branch `parity/session-2` (worktree `Mindustry-Godot-session2`), session 2 (display :11,
bridge 6980). This document accumulates the J04 tasks as they land; wave B covers **T1**
only (T2–T6 are later waves).

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — Turret target selection = `Turret.findTarget` | done (in-engine verified) | `e5b94549` |

## T1 — turret target selection

**Java.** `TurretBuild.findTarget` (`Turret.java:631-677`) picks units with the turret's
`unitSort` (default `UnitSorts.closest`, `Turret.java:113`; overridden by ripple
`grouped` (`Blocks.java:4107`), foreshadow/lustre/scathe/malign `strongest`
(`Blocks.java:4261,5239,5667,6278`)) or the ammo's `unitSort` when it is not `closest`
(`Turret.java:643`), then falls back to a damaged ally when `targetHealing` applies.
`Units.bestEnemy` (`Units.java:322-341`) keeps the highest `UnitType.targetPriority`
band and the lowest `UnitSorts` cost within it; the formulas are
`UnitSorts.java:9-40` (`strongest = -maxHealth + dst2/6400`,
`grouped(0.003) = -getClustered + dst*0.003`). `minRange` clamps the `scaleLife` bullet
lifetime (`Turret.java:797`, `minRange()` at `:340-344`); `trackingRange` gets a second
search pass (`:654-658`, smite/malign `range*1.4` at `Blocks.java:5947,6290`);
`newTargetInterval` is the retarget cadence while a target is held (`:44,216,571`;
sublimate 30, titan/afflict/scathe 40). `drawMinRange` (`:76,258,491`) draws a dashed
min-range ring.

**Port changes.**

- `mind-core::combat::targeting`: `UnitSort` (closest/farthest/strongest/weakest/
  `Grouped(w)`) with the exact `UnitSorts` cost formulas; `ClusterCounts`
  (`TeamData.getClustered`, ground units only, rebuilt per targeting pass);
  `TargetQueries::best_enemy`/`best_target` with Java's
  `(cost < cdist || priority > cpriority) && priority >= cpriority` selection and the
  dead/derelict/fog/`targetable`/`range + hitSize/2` gates.
- turret config gained `unit_sort`, `min_range`, `tracking_range`,
  `new_target_interval`, `target_healing`, `target_under_blocks` (with the per-turret
  overrides); `BulletDef` gained `unit_sort`/`min_range_change` (plumbed through the ammo
  configs); `config_for` applies `tracking_range = max(range, tracking_range)`
  (`Turret.java:223`).
- `find_target` now runs the two-pass search, the ammo sort override, the
  missile (`TimedKillComp`, `targetMissiles`)/air/ground/`targetUnderBlocks` gates and
  the ally-healing fallback (`canHeal`, `Turret.java:634-636`); the generic/continuous/
  laser retarget timers use `newTargetInterval`; the shoot path clamps `scaleLife`
  lifetimes to `[minRange()/range, range()/range]`; `find_ally_tile` takes an exclude.
- `register_bullets` derives `BulletType.range = calculateRange()` for the anonymous
  turret ammo (`BulletType.java:410`); without it `def.range` stayed 0 and the
  `scaleLife` clamp never ran.
- gdext debug probes: `turret_debug(x,y)` (sort/ranges/ammo/target) and
  `unit_debug_positions` now carries `hp_fract` (J04 verification; no rules).

**Tests (exact commands).**

```bash
cargo test -p mind-core --lib find_target_tests        # 5 passed
```

- `find_target_strongest_and_closest_sorts`: foreshadow picks the far 1000-hp unit over
  the near 100-hp one; duo the near one.
- `find_target_grouped_prefers_cluster`: ripple picks the 3-unit cluster over a lone
  nearer unit.
- `ripple_min_range_clamps_bullet_lifetime`: a 40-px shot gets the
  `minRange(50)/type.range(240)` lifetime floor, not the raw distance ratio.
- `find_target_uses_tracking_range`: smite acquires a unit at 350 px (outside range
  300, inside tracking 420) and not at 460 px.
- `new_target_interval_applies_with_a_target`: scathe holds 40 while a target is held,
  the plain interval otherwise.

Full crate suites after the change: `cargo test -p mind-core` -> 2731 passed (only the
pre-existing `blocks_golden` failure); `cargo test -p mind-headless --no-fail-fast` ->
the same 4 pre-existing goldens; `cargo clippy -p mind-core -p mind-gdext --all-targets
-- -D warnings` and `cargo fmt --all -- --check` clean.

**In-engine (`runs/20261017-s2-J04-T1/`).** Session 2, pid 552862, Ancient Caldera
custom game reached through the UI (Play > Custom Game > Ancient Caldera > Survival >
Play). Staging through the sim command seams: `place_block`, an `item-source` configured
through `submit_player_command_json`, and the pair spawned through
`apply_sim_command_json({op:"spawn_unit"})` — near `dagger` (~50 px, 150 hp), far `mace`
(~110 px, 600 hp); the sim is advanced with `step()`.

- Foreshadow: `turret_debug(140,32)` -> `{sort: "strongest", tracking_range: 500,
  total_ammo: 40, target: "mace"}` — the farther, high-max-health unit
  (`godot/foreshadow-targets-mace.png`).
- Duo: `{sort: "closest", tracking_range: 160, total_ammo: 29, target: "dagger"}` — the
  near unit, ammo dropping as it fires (`godot/duo-targets-dagger.png`).

**Deviations (with Java locations).**

1. `drawMinRange` (`Turret.java:76`) is ported as a config field, but no vanilla turret
   sets it (no `Blocks.java` assignment), so the dashed min-range ring can never draw;
   ripple's minimum range is observable through the bullet-lifetime clamp instead.
2. `TeamData.getClustered`'s 10-tick cache (`Teams.java:477-490`) is recomputed per
   targeting pass (the same value a fresh Java cache would hold).

**Blockers.** None for T1.

**Observations (candidate follow-ups, not T1).**

- `update_proximity` scans only the orthogonal `Edges` ring for size-1 blocks, while
  Java's `World.updateProximity` adds the 4 diagonals for `checkDiagonal` blocks: an
  item source diagonally adjacent to a turret does not feed it (directly adjacent
  does). Found while staging the duo.
- Deliberately not ported: the erekir unit-shell `BulletType.unitSort` overrides
  (`Blocks.java:4702,4741,4820`) are only read by `Turret` (`Turret.java:643`), so they
  have no turret effect; ripple's silicon shell sets `closest`, which Java treats as a
  no-op against the turret's `grouped` (`Turret.java:643`) and the port mirrors that.

**Ledger changes to apply.**

- **New item (T1)**: "Turrets always target the closest unit (no UnitSorts/minRange/
  trackingRange)" — status `verified` (`mode: reference`), fix `e5b94549`, evidence
  `runs/20261017-s2-J04-T1/godot/foreshadow-targets-mace.png` +
  `godot/duo-targets-dagger.png`, surface `world`.
- **Candidate item**: diagonal proximity transfer (`update_proximity` orthogonal-only).

**Done criteria (wave-B scope).** T1 landed with tests and an in-engine walk showing
the sort change end-to-end; T2–T6 remain for later waves.

---

# Wave C (T2–T6) — 2026-10-17, session 2

Branch `parity/session-2`; base `3bd1abe8` (main after the J02/J06/J03-B/J05/J10-B
merges). Wave B (T1) is above; this section covers **T2–T6**, each as its own
commit.

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T2 — `ConsumeItemExplode` + `itemDurationMultipliers` | done (in-engine verified) | `fefc574` |
| T3 — `GenericCrafter.liquidOutputDirections` | done (in-engine verified) | `d236e4a7` |
| T4 — `PowerGenerator.createExplosion` | done (sim half; unit-tested) | `501f2727` |
| T5 — per-block player deposit cooldown | done | `60e34db5` |
| T6 — round-I residuals | partial: PAR-0349 + power `rangeChange` landed; rest recorded | `8986959a` |

## T2 — `ConsumeItemExplode` + `itemDurationMultipliers`

`GeneratorConfig` gained `explode_items` (`ItemExplodeConfig` with the Java
threshold/damage/baseChance) and `item_duration_multipliers`; a new `Sim` pass
`update_item_explode_consumers` applies the damage
(`delta * 0.06 * clamp(explosiveness - 0.5)`) and fires
`Trigger::BlastGenerator` while `rules.reactorExplosions`; combustion/steam
register the consumer + pyratite x3, the RTG phase fabric x15
(`Blocks.java:2537-2538,2571-2572,2614`). `update_generator_with` follows the
consumed item's duration multiplier each tick (`ConsumeGenerator.java:126-131`).
Tests: `explosive_fuel_damages_generator_and_fires_trigger`,
`item_duration_multipliers_scale_burn_time`. In-engine (pid 628685): a
combustion generator fed blast-compound lost 24/40 health in 120 paused ticks
and died after 120 more; the coal control stayed at 40/40
(`runs/20261017-s2-J04-T2/godot/generator-explosive-fuel.png`).

## T3 — `liquidOutputDirections`

`CrafterDef.liquid_output_directions` + the per-output `dir` in the
`dumpOutputs` liquid half (`GenericCrafter.java:345-351`; the dump's direction
check already matched `Building.dumpLiquid`); electrolyzer `{1, 3}`
(`Blocks.java:1397`). Test:
`electrolyzer_dumps_liquids_in_configured_directions`. In-engine (pid 664020): a
rotated electrolyzer with conduits on all four sides dumps ozone only left and
hydrogen only right (`tile_debug(...)["liquids"]` after 300 paused ticks;
`runs/20261017-s2-J04-T3/`).

## T4 — `PowerGenerator.createExplosion` (sim half)

The 11 missing `PowerGenerator` explosion fields are ported to
`GeneratorConfig`/`ReactorExplosion`; `fire_explosion` queues
`PendingGeneratorExplosions` and the new `Sim::apply_generator_explosions` pass
applies `Damage.damage`, ignition fires (delayed by `dst / explosionSpeed`),
`unitMoveBreakable` prop breaking, fireballs and puddles
(`PowerGenerator.java:149-201`). Vanilla knobs for combustion/steam
(`Blocks.java:2532-2534,2566-2568`), thorium/impact reactors
(`NuclearReactor.java:66-67`, `ImpactReactor.java:36-37`), flux
(`VariableReactor.java:47-50`) and neoplasia (`Blocks.java:2834-2848`).
Test: `generator_explosion_applies_sim_side_effects` (damage, prop gone,
puddles, fires, fireballs). No live `ExplosionSink` host yet, so the view half
(effect/sound/shake/scorch) is not visible in-engine.

## T5 — per-block deposit cooldown

`BlockSpec`/`BlockDef.deposit_cooldown` + the 11 turret values
(`Blocks.java:3346-4337`) and the `InputHandler.canDepositItem` gate
(`InputHandler.java:2310-2316`). Tests: `deposit_cooldown_gate` (extended) and
`player_deposit_cooldowns_match_java`.

## T6 — round-I residuals

- **Landed**: `PAR-0349` door close guard (`Door.java:49-53`; test
  `door_refuses_to_close_with_a_unit_on_it`) and the power-turret
  `PowerAmmo { range_change, min_range_change }` snapshot
  (`Turret.java:340-353`; test `power_turret_reads_bullet_range_change`).
- **Already in main (verified)**: the `AIController.faceTarget` intercept lead
  (`sim/unit_runtime.rs:6138`) and the sublimate activation window
  (`PAR-0089`, `advanced.rs:2029-2047`).
- **Recorded, not ported**: unit-assembler output gate/hand-off
  (`UnitAssembler.java:466-472,516-532,560-562`), the `router` `ControlBlock`,
  bullet suppression + `findPierceLength`/`findLaserLength`
  (`BulletType.java:562-580`), `UnitTetherComp.spawnerUnitId` +
  `EmptyDataAbility` round-trip. Details in
  `runs/20261017-s2-J04-T6/report.md`.

## Deviations (wave C, with Java locations)

1. T2/T4: generator effects (`Fx.generatespark`/`explodeEffect`/shake/scorch)
   have no live host sink yet; the sim halves are complete.
2. T4: ignition delays are whole ticks (`PendingExplosionFires`) instead of the
   `Time.run` unit clock (≤1-tick boundary lag); the puddle/fireball draws use
   the sim stream rather than Java's global `Mathf.rand`.
3. T3: `DrawLiquidOutputs`/`drawOverlay` side icons remain region-only
   (`render/draw_desc.rs:678`); the side data now exists in `CrafterDef`.
4. T2: the `ConsumeItemEfficiency.display`/`StatValues.itemEffMultiplier` info
   row and Java's sandbox `cheating()` consumer bypass are not ported.

## Pre-existing failures at the wave-C base

`blocks_golden::family_counts_match_golden` (other 158 vs 164) plus the
mind-headless goldens (`audio`, `input`, `logistics`, `maps_editor`) and
**`units_scenarios::committed_goldens_match`** (`units_legs_ik`). The legs
golden was verified pre-existing by running the test in a worktree at
`99ef19cf`: it was introduced by the merged J03 T4 (`9f381d49` "leg draws
(UnitType.drawLegs) + Java leg geometry in LegsComp" — the golden still holds
the old segment lengths). Wave C touches nothing under `entities/`.

## Ledger changes to apply

- **PAR-0349** (door close guard): status -> `fix`/verified, fix `8986959a`,
  evidence `runs/20261017-s2-J04-T6/report.md`.
- **PAR-0089** (sublimate activation window): verified present in main
  (`advanced.rs:2029-2047`, tests `:2545`); close if its remaining criteria are
  met.
- **New item (T2)**: \"Explosive items never damage generators and never fire
  Trigger.blastGenerator; itemDurationMultipliers ignored\" — fix `fefc574`,
  evidence `runs/20261017-s2-J04-T2/godot/generator-explosive-fuel.png`.
- **New item (T3)**: \"Crafters dump output liquids on every side
  (liquidOutputDirections ignored)\" — fix `d236e4a7`, evidence
  `runs/20261017-s2-J04-T3/godot/electrolyzer-directions.png`.
- **New item (T4)**: \"Generator/reactor explosions apply no side effects\"
  — fix `501f2727`, evidence
  `runs/20261017-s2-J04-T4/` (unit test) + the residual view half.
- **New item (T5)**: \"Block.depositCooldown ignored (global gate only)\" —
  fix `60e34db5`, evidence `runs/20261017-s2-J04-T5/report.md`.
- **New item (T6)**: \"Power turrets report rangeChange/minRangeChange = 0\" —
  fix `8986959a`.
- **Candidate follow-ups**: live `ExplosionSink` (view half of T4), generator
  spark/explode effects, `DrawLiquidOutputs` side regions, the filter
  `itemEffMultiplier` stats row, assembler output gate, `ControlBlock`,
  bullet suppression, tether `spawnerUnitId`.
