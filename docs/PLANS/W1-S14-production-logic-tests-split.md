<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S14 — Split `sim/production_tests.rs` (6588, 130 tests) and `sim/logic_campaign_tests.rs` (2857, 71 tests)

**Wave:** W1 (pure mechanical split, RULES §3). Test-only.
**Owns:**
- `client/rust/mind-core/src/sim/production_tests.rs` → `sim/production_tests/**`;
- `client/rust/mind-core/src/sim/logic_campaign_tests.rs` → `sim/logic_campaign_tests/**`.

**Effort:** medium.

## `production_tests` → `production_tests/`

`git mv` to `production_tests/mod.rs`. `mod.rs` keeps the imports and **every fixture**, all
`pub(super)`:
- 16–36 (`place`, `item`);
- 424–450 (`liquid`, `set_floor`, `stored_liquid`, `runtime_sim`);
- 507–525 (`configure`, …);
- 1206–1262, 1393, 2250;
- 3025–3086 (`install_campaign_session`, `install_logic_world`, `place_world_processor`, `test_rules`);
- 4586–4650.

Fixtures that sit between tests stay in `mod.rs` too, if more than one topic uses them.

| File | Lines at `291e30a8` |
|---|---|
| `generators.rs` | 37–423 |
| `liquids_sources.rs` | 451–903 |
| `factories.rs` | 904–1112 |
| `wall_crafters.rs` | 1113–1513 |
| `crafting_power.rs` | 1514–2036 |
| `extractors_drills.rs` | 2037–2614 |
| `nodes_menders_sounds.rs` | 2615–2985 |
| `world_processor_a.rs` | 2986–3558 (G06 T4, first half) |
| `world_processor_b.rs` | 3559–4215 |
| `sensor_control.rs` | 4216–4336 |
| `core_placement_rules.rs` | 4337–4820 |
| `kinds_env.rs` | 4821–5068 |
| `fill_items_stats.rs` | 5069–5471 |
| `configure.rs` | 5472–5674 |
| `t14_blocks.rs` | 5675–5974 |
| `campaign_accelerator.rs` | 5975–6320 |
| `heat_attribute_crafters.rs` | 6321–6588 |

Check the 3558/3559 cut at HEAD: put it at a test boundary near there.

**Paths other files depend on:**
- `sim/tests.rs:550` imports `production_tests::runtime_sim`;
- `logic_campaign_tests.rs:24–27` imports 8 fixtures from `production_tests`.

Both keep working because the fixtures stay in `production_tests/mod.rs` as `pub(super)`.

## `logic_campaign_tests` → `logic_campaign_tests/`

Despite the name, this is mostly the mlog / world-processor conformance suite run on a live campaign
sim. Keep the module name, because `TWIN-VERIFICATION.json` cites
`sim::logic_campaign_tests::sensor_reads_team_properties`. If that test moves into a child module,
`pub use` it back is **not** possible for a `#[test]` fn path, so record the new path for C01.

| File | Lines |
|---|---|
| `mod.rs` | 1–245 (imports, program constants 29–52, helpers 54–245) |
| `maps.rs` | 246–470 (Ground Zero GZ1–5; Frozen Forest/Frontier/fungal/extraction/overgrowth) |
| `world_effects.rs` | 471–790 (H06 T8 world side effects) |
| `units.rs` | 791–1264 (I03 T1 ubind/ucontrol/ulocate; I03 T2 objective completion script) |
| `sensors.rs` | 1265–1446 and 2322–2857 (I03 T3 sensor/fetch/printchar; family `logic_sense` overrides; PAR-0218 audit). If the combined file exceeds 1500 lines, split it into `sensors.rs` (1265–1446, 2322–2451) and `sense_audit.rs` (2452–2857) |
| `turrets_radar.rs` | 1447–1702 (I03 T4, T5) |
| `links.rs` | 1703–1950 (I03 T6) |
| `displays.rs` | 1951–2321 (I03 T12 displays, T14 canvas) |

## Verification

```bash
B=$(git merge-base main HEAD); S=client/rust/mind-core/src/sim
python3 tools/refactor/move_check.py --base $B --old $S/production_tests.rs --new $S/production_tests
python3 tools/refactor/move_check.py --base $B --old $S/logic_campaign_tests.rs --new $S/logic_campaign_tests
run --cargo-test -p mind-core --lib sim::production_tests
run --cargo-test -p mind-core --lib sim::logic_campaign_tests
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
```

The test count must be unchanged. List every test path cited in `TWIN-VERIFICATION.json` / `parity/`
that changed, for C01.
