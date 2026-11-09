<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S17 — Split the blocks registry `mod.rs` (4798), turrets `mod.rs` (4697) and `io/sim_io.rs` (3662)

**Wave:** W1 (pure mechanical split, RULES §3). **Determinism-sensitive (turrets, sim_io): RULES §5.**
**Owns:**
- `client/rust/mind-core/src/content/registries/blocks/mod.rs` plus new siblings
  `{kind,spec,types,consume,def,traits,loader,tests}.rs` (the 19 existing siblings are **not** yours);
- `client/rust/mind-core/src/world/blocks/defense/turrets/mod.rs` plus new siblings (existing
  `advanced.rs`, `behavior.rs`, `save.rs` are not yours);
- `client/rust/mind-core/src/io/sim_io.rs` → `io/sim_io/**`.

**Effort:** large. These are three independent parts; commit them separately.

## Part A — `content/registries/blocks/mod.rs` (port of `content/Blocks.java` types + `world/Block.java` fields)

| File | Lines at `291e30a8` | Contents |
|---|---|---|
| `mod.rs` | 1–66 (decls 21–39) + re-exports | |
| `kind.rs` | 67–680 | `BlockKind` enum 67–357 + impl 358–680 |
| `spec.rs` | 681–1222, 1223–2169 | `BuildVisibility`, `BlockGroup`, `BlockFlag`, `EnvMask`; spec helper types (`StackSpec`, plans, `ConsumeDef`, `Consume`, `StatSpec`, `BarSpec`); `BlockSpec` struct + `Default` + impl. **About 1490 lines. If it is over 1500, put 681–1222 into `types.rs`** |
| `consume.rs` | 2170–2386 | consts, light defaults, `consume_*` builders |
| `def.rs` | 2387–3525 | `BlockDef` struct 2387–2813, kind predicates 2814–2868, `impl BlockDef` (`from_spec` 2878–3341, **whole**), `resolve_*` 3432–3525 |
| `traits.rs` | 3526–3841 | `Content`/`Mappable`/`Unlockable` impls |
| `loader.rs` | 3842–4117 | `BlockSink`, `Blocks`, `load`, `post_init_link`, `kind_rotates` |
| `tests.rs` | 4118–4798 | tests |

`mod.rs` must `pub use kind::*; pub use spec::*; …`. The 19 sibling files use `super::X`, and
`content/mod.rs:47` re-exports `BarSpec`, `BlockDef`, `BlockFlag`, `BlockGroup`, `BlockKind`,
`BlockSpec`, `Blocks`, `BuildVisibility`, `Consume`, `ConsumeSpec`, `EnvMask`, `StatSpec`,
`TILE_SIZE`, `is_floor_kind`, `is_overlay_floor_kind`. 55 files import from `registries::blocks::`.
Glob re-exports of the moved items keep all of them working.

## Part B — `world/blocks/defense/turrets/mod.rs` (port of `world/blocks/defense/turrets/Turret.java`, `ItemTurret`, `LiquidTurret`, `PowerTurret`, `BuildTurret`)

| File | Lines | Contents |
|---|---|---|
| `mod.rs` | header, decls, re-exports | |
| `types.rs` | 58–565 | ammo/config/state types (`TurretConfig` 185–345, `TurretState` 362–565) |
| `config.rs` | 566–1318 | `register_bullets`, rebalance, `install_turret_ammo`, `config_for_inner` 970–1318 (**whole**) |
| `math.rs` | 1319–1375 | math helpers |
| `ammo.rs` | 1376–1872 | ammo accept/handle/use (`handle_item`, `has_ammo`) |
| `update.rs` | 1873–2273 | update engine (`update_turret` 1909–2153, `update_turrets`) |
| `build_turret.rs` | 2274–2804 | BuildTurret (`update_build` 2549–2804) |
| `targeting.rs` | 2805–3291 | shoot/bullet/targeting (`turret_range`) |
| `tests.rs` | 3292–3954 | `mod tests` (keep the name) |
| `find_target_tests.rs` | 3955–4697 | it uses `super::tests::set_unit_elevation`. After the move that is `super::tests::set_unit_elevation` from a sibling, so make it `pub(super)` in `tests.rs` and import it as `use super::tests::set_unit_elevation;` |

Re-export `TurretState` (26 imports), `install_turret_ammo`, `handle_item`, `turret_range`,
`update_turrets`, `has_ammo`, `TurretKind`, `TurretAmmo`, and anything else the grep finds.

## Part C — `io/sim_io.rs` → `io/sim_io/` (save/load glue between `Sim` and IO; port of `io/SaveVersion.java` entity chunks + `io/TypeIO.java` unit fields)

| File | Lines | Contents |
|---|---|---|
| `mod.rs` | 1–174 | consts, playtime 51–106, decoded records 107–174 |
| `entities.rs` | 175–418 | `LiveEntitySink`/`Source` |
| `unit_codec.rs` | 419–766 | `write_unit_fields`/`read_unit_fields`, AI/plan wire helpers |
| `decode.rs` | 767–932 | `DecodedSave`, `decode_*` |
| `apply.rs` | 933–1243 | `impl Sim` (`apply_decoded_entities`), `apply_unit_record` |
| `handler.rs` | 1244–1594 | meta tags, `SimMapSource`, `SimIoHandler` |
| `tests.rs` | 1595–3429 | tests |
| `map_filter_tests.rs` | 3430–3662 | |

Re-export:
- for `sim/mod.rs`, `io/mod.rs`, `io/java_entities.rs`;
- for `world/blocks/payloads/{mod,payload_block}.rs`: `write_unit_fields`, `read_unit_fields`,
  `SIM_UNIT_REVISION`, `apply_unit_record`;
- for mind-gdext `campaign.rs`/`sim_host.rs`: `SimIoHandler`, `SectorResumeMeta`, `meta_tick`;
- for mind-headless `server/host.rs` and `campaign_scenarios.rs`.

Grep `sim_io::` for the full list.

## Verification (RULES §6, §5)

```bash
B=$(git merge-base main HEAD); C=client/rust/mind-core/src
python3 tools/refactor/move_check.py --base $B --old $C/content/registries/blocks/mod.rs --new $C/content/registries/blocks/{mod,kind,spec,types,consume,def,traits,loader,tests}.rs
python3 tools/refactor/move_check.py --base $B --old $C/world/blocks/defense/turrets/mod.rs --new $C/world/blocks/defense/turrets/{mod,types,config,math,ammo,update,build_turret,targeting,tests,find_target_tests}.rs
python3 tools/refactor/move_check.py --base $B --old $C/io/sim_io.rs --new $C/io/sim_io
run --cargo-check --workspace --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
run --cargo-test -p mind-core --lib
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- parity goldens
run --cargo-run -q -p mind-headless -- run-all --tier T0
run --goldens --check
```

(Drop from the `--new` lists any file you did not create; `move_check` reads only paths that exist.)
