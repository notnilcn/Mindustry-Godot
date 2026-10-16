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
