# AGENTS.md — mind-core/src/ai (unit AI & pathfinding)

This folder owns unit AI, pathfinding and the headless unit harness: the controller framework and
`AiKind` selection, the `AiCtx` helper set, the `ai/types/*` controllers (`GroundAI`, `FlyingAI`,
`CommandAI`, …), the flowfield `Pathfinder` and cluster/portal `ControlPathfinder` alongside the
synchronous `Astar`, RTS squad assignment, unit commands/stances and `UnitGroup` formation, the
`WaveSpawner`, and the deterministic `UnitHarness`. All of it runs inside the fixed 60 Hz simulation
and participates in the checksum. Read the root [`AGENTS.md`](../../../../../AGENTS.md) first, then
the crate notes [`mind-core/AGENTS.md`](../../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Module root; declares the submodules and re-exports the public AI surface. |
| `controller.rs` | `UnitController` trait, `AiKind` (append-only controller kinds), `ControllerSlot` component, `select_ai`. |
| `controller_registry.rs` | `select_controller` (player/RTS/AI matrix), `keep_state`, `is_logic_controllable`. |
| `ai_controller.rs` | `AiCtx` per-tick context and the `AIController` free-function helper set (targeting, movement, steering, `pathfind`). |
| `types/` | The 18 controllers: `ground`, `flying`, `hug`, `flying_follow`, `command`, `logic`, `missile`, `builder`, `miner`, `cargo`, `defender`, `suicide`, `boost`, `assembler`, `prebuild`, `repair`, `player_bridge`, `no_ai`. |
| `pathfinder/` | `Pathfinder` flowfield cache, `Flowfield`, `PathTile`, `Cost`, `PathfindQueue`, deterministic budget constants (`worker`). |
| `astar.rs` | Synchronous 4-connected grid A* (`AstarScratch`, `TileHeuristic`, `manhattan`/`euclidean`/`octile`). |
| `control_pathfinder.rs` | `ControlPathfinder` cluster/portal build, per-goal `FieldCache` fields, `raycast`/`raycast_fast_avoid`, `PathfindResult`. |
| `control_structs.rs` | Frozen bit-packed `FieldIndex`, `IntraEdge` and `NodeIndex` (`@Struct` layouts). |
| `block_indexer.rs` | `BlockIndexer` per-team building buckets by `BlockFlag`, damaged list, `BlockPriority` target scans. |
| `base_registry.rs` | `BaseRegistry`/`BasePart`/`BaseResource`: classic base schematics, ore maps, tier classification. |
| `base_builder_ai.rs` | `BaseBuilderAi` timer machine, enemy-core path trace, `try_place`, `BlockPlan` queueing. |
| `rts_ai.rs` | `RtsAi` squad grouping, `Squad`/`SquadStats`/`RtsUnit`, target selection and `estimate_stats`. |
| `unit_command_runtime.rs` | Command metadata helpers: `allows_command`, `default_command`, `command_controller`, `get_unit_stances`. |
| `unit_stance_runtime.rs` | `StanceBits` 64-bit mask and `set_stance`/`disable_stance`/`apply_stance`. |
| `unit_group.rs` | `UnitGroup` formation offsets, relaxation and `clamp_offsets_to_pathfinder`. |
| `wave_spawner.rs` | `WaveSpawner` spawn-tile list, 121-tick window and per-`SpawnGroup` emission. |
| `harness.rs` | `UnitHarness`: headless spawn/tick/checksum integration point for `mind-headless units …`. |

## Key types

- Framework: `UnitController`, `AiKind`, `ControllerSlot`, `AiCtx`; selection via `select_ai`,
  `select_controller`, `keep_state`.
- Controllers: `GroundAi`, `FlyingAi`, `CommandAiState` (`CommandQueueEntry`, `AttackTarget`), plus
  `update_ground`/`update_flying`/`update_command` entry functions.
- Pathfinding: `Pathfinder`, `Flowfield`, `PathTile`, `Cost`, `PathfindQueue`; `AstarScratch`,
  `DistanceHeuristic`, `TileHeuristic`, `UniformCost`; `ControlPathfinder`, `FieldCache`, `Portal`,
  `PathfindResult`; `FieldIndex`, `IntraEdge`, `NodeIndex`.
- RTS/AI services: `BlockIndexer`, `BlockPriority`; `RtsAi`, `Squad`, `SquadMember`, `SquadStats`,
  `RtsUnit`, `RtsBuilding`, `EnemyStat`; `BaseBuilderAi`, `BaseBuildInput`, `BaseBuildActions`,
  `AiInterval`, `BaseRegistry`, `BasePart`, `BaseResource`; `WaveSpawner`, `UnitGroup`, `StanceBits`.
- Harness: `UnitHarness`.

## Pathfinding

- `Pathfinder` returns a cached `Flowfield` per `(team, cost)` via `get_field`; `build_field` solves
  it with an exact Dijkstra pass over `PathfindQueue` (ties break by node index). `PathTile` encodes
  health/team/solid/deep/damage bits and its layout is frozen.
- `ControlPathfinder::build` derives clusters (`CLUSTER_SIZE = 12`), shared-boundary `Portal`s and
  inner edges; `get_path_position` serves a per-goal `FieldCache` built by a deterministic bounded
  BFS, and `get_path_position_uncached`/`get_path_position_astar` are result-neutral parity entries.
- `astar::pathfind` is the shared 4-connected A* used for in-cluster edges; it excludes the start tile
  and returns an empty vec when unreachable.
- The incremental budgets (`FLOWFIELD_NODES_PER_TICK`, `CONTROL_NODES_PER_TICK`, refresh/timeout
  constants in `pathfinder::worker`) replace wall-clock budgeting so checksums do not depend on
  worker count.

## Determinism and invariants

- **Part of the simulation.** Controllers, pathfinding and the harness execute inside the fixed step;
  `UnitHarness::checksum_value` folds unit fields, `ControllerSlot` and `pathfinder.updates` into the
  canonical checksum. Keep AI state on serialized components/`Resource`s.
- **Randomness only through shared streams.** Harness ticks draw from `determinism::SimRng`;
  `BaseBuilderAi` takes a seeded `math::ArcRand`. No wall clock or OS entropy in this subsystem.
- **Stable ordering.** Iteration is by `EntitySeq`/entity index; ties in targeting, squad grouping and
  path reconstruction break by entity or node index. Serialized containers are ordered (`BTreeMap`,
  `BTreeSet`).
- **Frozen ABI.** `PathTile`, `FieldIndex`, `IntraEdge` and `NodeIndex` bit layouts are save/A* ABI;
  `AiKind` and `Cost` are append-only.
- `UnitHarness::tick` iterates a clone of the live unit list, so units killed during a tick are skipped
  without panicking; `remove`/`kill` keep `units_removed` consistent.

## Rules

- Port upstream Mindustry and cite the source in the file header; files are UTF-8/LF and
  `SPDX-License-Identifier: GPL-3.0-only`.
- Keep the subsystem Godot-free and tokio-free, runnable under a plain `cargo test -p mind-core`.
- Rebuild path state after terrain changes: `Pathfinder::rebuild`/`update_tile` and
  `UnitHarness::refresh_path_tiles`; `BlockIndexer::rebuild` after building changes.
- Add new controllers as an `ai/types/*` module, an `AiKind` variant and a `controller_registry`
  arm — never reorder existing `AiKind`/`Cost` variants.

## Verification

Run from `client/rust/`, or add `--manifest-path client/rust/Cargo.toml` from the repo root.

```bash
cargo test -p mind-core
cargo clippy -p mind-core --all-targets -- -D warnings
cargo fmt --all -- --check
```

Focused runs: `cargo test -p mind-core ai::harness` (harness determinism/fuzz),
`cargo test -p mind-core ai::control_pathfinder` (cached-vs-A* parity) and
`cargo test -p mind-core ai::unit_group` (formation).
