# AGENTS.md — mind-core/src/world (world & terrain)

The world/terrain subsystem: the tile grid, every tile-mutation path, building construction and the
building codec, the boot resources the generation/map paths install, and the geometry/query helpers
the rest of the simulation builds on. Terrain generation itself belongs to `maps` (see Boundaries).
Crate-wide rules are in
[`mind-core/AGENTS.md`](../../AGENTS.md); the repo map is the root
[`AGENTS.md`](../../../../../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | `WorldGrid` resource (owns `Tiles`, `tile_changes`/`floor_changes`, `generating`, `limit_area`), `WorldError`, `WorldPlugin`, `has_building_kind`/`block_has_building`. |
| `ops.rs` | The tile-mutation surface. `WorldCtx` (`grid` + `content` + `&mut World` ECS + `WorldHooks`/`RenderHooks` + `WorldEventLog`) with `set_block`/`set_floor`/`set_overlay`/`set_air`/`clear_building`/footprint assignment. |
| `tile.rs` | One `Tile`: x/y, block/floor/overlay ids, packed `data` fields, ECS `build` handle, `changing` guard, derived predicates. |
| `tiles.rs` | Flat `Vec<Tile>` grid indexed `x + y*width`, puddle/fire entity slots, generation scratch buffers; `each` (x-outer) and `iter` (row-major) orders. |
| `tile_index.rs` | `TileBuilds`, a dense tile→building-entity mirror kept in sync by `ops`. |
| `build.rs` | `Build.validPlace`/`validBreak`, `canReplace`, `contactsGround`/`contactsShallows`, `getEnemyOverlap`. |
| `construct.rs` | `ConstructState` component: construction/deconstruction progress and cost math. |
| `plan.rs` | `BuildPlan` pending placement/deconstruction orders. |
| `building_io.rs` | `BuildingCodec`/`write_base`/`read_base`, `DecodedBase` and module bitmask for revisioned save/sync IO. |
| `consumers.rs` | `Consumers`/`ConsumeInstance`: lowered item/liquid/power/coolant consumers and the partition arrays. |
| `checksum.rs` | `impl ChecksumPart for WorldGrid` — the world's simulation checksum stream. |
| `generation.rs` | Installs `WorldGrid` plus the logic resources map-generation scripts read (boot resources only). |
| `map_source.rs` | `EcsMapSource`: `io::save::MapSource` over a live grid, resolving core teams/centers from ECS. |
| `raycast.rs` | Bresenham grid raycasts (`raycast_each`, `raycast`, `raycast_first`, world-space variants). |
| `proximity.rs` | `update_proximity`/`remove_from_proximity` with `Edges` neighbor order and `ProximityUpdateEvent`. |
| `status.rs` | `BlockStatus`/`block_status` for the active/no-input pulse. |
| `stats.rs` | `Stats`/`StatValue`/`BarDisplay` runtime rows and `evaluate_bar`. |
| `update.rs` | `BlockInstance::spawn`, `update_buildings`/`update_consumption`, timer/sleep helpers and `delta`/`edelta`. |
| `block.rs`, `block_kind_data.rs` | `BlockTable`/`BlockInstance`/`BlockView` runtime side table and typed per-family knobs. |
| `behavior/` | `BuildingBehavior` trait + `BehaviorRegistry` and the campaign/defense/environment/production/sandbox families. |
| `blocks/` | Block-family subtree (`autotiler`, `defense`, `distribution`, `heat`, `liquid`, `payloads`, `power`, `production`, `storage`, `units`, `tile_bitmask`); it has its own `AGENTS.md`. |
| `fixtures/` | Deterministic test fixtures (`fixtures/logistics.rs`). |
| `modules/` | Item/liquid/power building modules and `FlowWindow`. |
| support | `events.rs`, `hooks.rs`, `limits.rs`, `config.rs`, `allow_update.rs`, `config_tap.rs`, `draw.rs`, `cached.rs`, `context.rs`, `params.rs`, `pos.rs`, `attributes.rs`, `color_mapper.rs`, `darkness.rs`, `edges.rs`, `item_buffer.rs`, `network_state.rs`, `harness.rs`. |

## Boundaries

- The save container, `SaveVersion`, `TypeIO` and the `MapSource` trait belong to
  [`io`](../io/AGENTS.md); this tree owns only the building codec (`building_io.rs`, dispatched by
  `io::entity::EntityCodec`) and the live-grid adapter in `map_source.rs`.
- Terrain generation belongs to [`maps::generators`](../maps/generators/mod.rs) (`WorldGenerator`,
  `BasicGenerator`, `PlanetGenerator`); `generation.rs` only installs the boot resources those
  scripts and the privileged logic VM read.

## Key types

- `WorldGrid` (`Resource`) owns a real `tiles: Tiles` grid and the `tile_changes`/`floor_changes` counters; counters start at `1` and reset to `-1` when `end_map_load` completes.
- `WorldCtx<'_>` is the explicit bundle passed to every tile op (`grid` + `content` + ECS `World` + hooks + event log); there is no global `WorldGrid` borrow.
- `Tile.build: Option<Entity>` is the building link; multiblock proxies point at the center entity, and team/rotation live on the building entity.
- `WorldHooks`/`RenderHooks` (with `Noop*` defaults) are the lifecycle/render seams; `world::blocks::default_registry` supplies the behavior overrides.
- `BuildHarness` drives place/configure/destroy for tests and headless scenarios.

## Invariants

- All tile mutation goes through `ops::WorldCtx`; direct `Tiles` writes are limited to setup/fixtures. `set_block` removes the old building, assigns the whole footprint, fires counters/proximity/recache, then syncs `TileBuilds`.
- While `WorldGrid::generating` is set, tile ops skip events, counters and proximity.
- Iteration order is a determinism contract: `Tiles::each` is x-outer/y-inner while `Tiles::iter` and `WorldGrid::iter_row_major` are row-major.
- `Tile.x`/`Tile.y` stay `i16`; do not widen them.
- Building field order is serialization order (`BuildingCodec`).

## Rules

- Add tile behavior by extending `ops` + `BuildingBehavior`, not by mutating `Tile` fields from unrelated modules.
- Register new block families through `BehaviorRegistry`/`default_registry`; keep `BlockInstance` data-driven.
- Do not change the checksum stream casually: `WorldGrid` participates in the simulation checksum, so goldens depend on it.
- Keep generation policy out of this tree: `world` provides the grid/resources, `maps` decides terrain.

## Verification

Run from `client/rust/` (or add `--manifest-path client/rust/Cargo.toml`).

```bash
cargo test -p mind-core world
cargo test -p mind-core building_io
```
