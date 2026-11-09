<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H08 — Sim non-test code: determinism-safe helpers only

**Wave:** W2 (helper package, RULES §4 + §5). **Needs:** W1 (S11, S12) merged.
**Owns:**
- the **non-test** files of `client/rust/mind-core/src/sim/{unit_runtime,runtime,builder,spine}/**`;
- `client/rust/mind-core/src/sim/mod.rs`;
- a new `client/rust/mind-core/src/sim/query.rs`, if you need it (declare it in `sim/mod.rs`).

**Effort:** medium. **Risk:** highest of W2. Every commit must pass the golden and determinism
gates before you start the next one.

## Allowed helpers (and only these)

| # | Helper | Replaces | Safe because |
|---|---|---|---|
| 1 | `pub(crate) fn sorted_by_seq(it: impl Iterator<Item = (EntitySeq-or-u64, Entity)>) -> Vec<Entity>`. Copy the key exactly: `sort_by_key(\|(seq, e)\| (*seq, e.index()))` | `unit_runtime/` (was ur:586, 619, 1003, 2566, 3977), `runtime/` (was 1202, 2091) | the keys are unique, so stable and unstable sorts agree. Use the same sort fn (`sort_by_key`) anyway |
| 2 | `pub(crate) fn team_of(world: &World, e: Entity) -> Option<u8>` = `world.get::<TeamComp>(e).map(\|t\| t.team)` | about 34 sites in `unit_runtime/` | pure read |
| 3 | `pub(crate) fn unit_def<'a>(world: &World, content: &'a ContentRegistry, e: Entity) -> Option<&'a UnitDef>` = `get::<UnitTypeComp>` then `content.unit(id)` | about 45 sites | pure read; check that each site does nothing between the two lookups |
| 4 | one `tile_world` | `unit_runtime/targeting.rs` (was ur:6571) and `builder/checks.rs` (was builder:324). They are numerically identical (`as f32` vs `f32::from` on an `i16`) | keep the `unit_runtime` copy and delete the other; or keep both and note why if visibility makes that awkward |
| 5 | `fn with_unit_runtime_removed<R>(world: &mut World, f: impl FnOnce(&mut World, &mut UnitRuntime) -> R) -> Option<R>` | `remove_non_send::<UnitRuntime>()` … `insert_non_send(runtime)` pairs (19 sites in `unit_runtime/`) | Switch only pairs where the remove and insert bracket the block, and the "missing" path matches `None`. **Never** convert a `get_non_send(_mut)` site to remove-insert or back: while the resource is removed, nested code sees no runtime |
| 6 | `fn dist2(dx: f32, dy: f32) -> f32 { dx * dx + dy * dy }` | `let dist2 = dx*dx + dy*dy` with index tie-break (ur:4717, 5937, 6165, 6294, 6325, 6350) | the exact expression. No `mul_add`, `hypot`, `f64` or reordering. Rust does not contract to FMA, so a fn call is bit-identical |

## Explicitly forbidden in this package

- **Unifying the 9 core lookups**: `has_core_of_team`, `closest_enemy_core`, `closest_core_of_team`,
  `best_core_for_player`, `enemy_core_targets`, `builder::team_core`, `builder::team_has_core`,
  `runtime::team_core_positions`, `mod::is_own_core`. They use different predicates
  (`BlockKind::CoreBlock` vs `CoreBuild` vs `TeamComp`) and tie-breaks (distance-then-index, lowest
  `EntitySeq`, raw archetype order). Write a short table of all nine into `sim/AGENTS.md` notes in
  your report (C01 adds it), so the next person does not try.
- Touching `unit_update_system`'s body (that is X02), `resolve_deaths`, or `sim/schedule.rs`.
- Duplicates outside `sim/` (`building_center` in `logic/executor/unit_control.rs:104` and
  `ai/types/cargo.rs:419`). List them for C01.

## Verification after **every** commit (RULES §5)

```bash
run --cargo-test -p mind-core --lib sim::
run --cargo-run -q -p mind-headless -- parity goldens
run --cargo-run -q -p mind-headless -- run spine_determinism --json
```

Before the report, also run:
```bash
run --cargo-test -p mind-core
run --cargo-test -p mind-core --features msav-import sim::unit_runtime
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- run-all --tier T0
for s in spine_place_break spine_many_commands; do run --cargo-run -q -p mind-headless -- run $s --json; done
run --goldens --check
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
```

Any checksum drift means you revert that commit. Do not bless it.
