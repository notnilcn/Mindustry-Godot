# AGENTS.md — mind-core/src/world/blocks (block families)

The block-family subtree of `mind-core`'s `world` module: a Rust mirror of upstream Mindustry
`core/src/mindustry/world/blocks/`. Each family owns the per-tick behavior, item/liquid/power
transport, IO and save shape for one class of placed building, plus the shared autotiling and join
machinery every family consumes. Registration goes through `world::behavior::BehaviorRegistry`;
`mod.rs::default_registry` is the composition root. Grid/tile mutation and the behavior trait belong
to [`world`](../AGENTS.md); crate-wide rules are in [`mind-core/AGENTS.md`](../../../AGENTS.md), and
the repo map is the root [`AGENTS.md`](../../../../../../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Family module list and `default_registry`, which calls each family's `register`. |
| `autotiler.rs` | Blend/join state: `d4`/`d4x`/`d4y`, `BlendWorld`, `BlendNeighbor`, `build_blending`, `blends_world`, `blends_armored`, `facing`, `looking_at*`, `SliceMode`. |
| `tile_bitmask.rs` | `VALUES: [u8; 256]` 8-neighbor → 47-slice map, `SLICE_COUNT`, `load`, `load_variants`. |
| `distribution/` | Item logistics: belts, ducts, junctions, routers, sorters, bridges, mass driver, gates, stack/overflow variants, chained buildings, directional unloaders, `transfer`. |
| `storage/` | `StorageBehavior`/`StorageBuild`, `CoreBehavior`/`CoreBuild`, `UnloaderBehavior` with its `ContainerStat` cache. |
| `power/` | Power graph arena, nodes/diodes/batteries, generators/reactors, modules, explosions, sandbox sources. |
| `liquid/` | Conduit/router/junction behaviors, `LiquidNode`, `movement` primitives, `LiquidFlowCache`, bridges. |
| `heat/` | Erekir heat network: `HeatState`, `HeatConductor`, `HeatCrafter`, `calculate_heat`, producer ramp. |
| `payloads/` | Payload core (`PayloadRef`, `PayloadHolder`, `PayloadCarried`) plus conveyors, loaders, unloaders, routers, constructors/deconstructors, block producers, sources and voids. |
| `defense/` | Projector/shield family (`behaviors.rs`, `projectors.rs`, `shields.rs`) and `turrets/` targeting/shooting; turret save lives in `turrets/save.rs`. |
| `units/` | Unit-producing blocks: factories, reconstructors, assemblers/modules, repair tower/turret, cargo loader/unload point. |
| `production.rs` | Content-dependent production registrations: drills (`DrillBehavior`) and wall miners/plasma bores (`BeamDrillBehavior`) that read the item-hardness/wall-ore tables. |

## Key types

- `world::behavior::BuildingBehavior` + `BehaviorRegistry` are the registration surface (see
  [`world`](../AGENTS.md)); families implement only the hooks they need and are keyed by `BlockId`.
- `world::block::{BlockTable, BlockInstance}` supplies `BlockDef` metadata (`size`, `item_capacity`,
  `requirements`, `outputs_power`, `insulated`, `attributes`, `kind`); `TILE_SIZE` and `TilePos` are
  the shared geometry.
- ECS state components live beside their family: `ConveyorBuild`, `StorageBuild`, `UnloaderBuild`,
  `PowerProduction`, `PowerNodeConfig`, `LiquidNode`, `HeatState`, `PayloadHolder`, `TurretState`,
  `UnitFactoryBuild`. Modules are `world::modules::{ItemModule, LiquidModule, PowerModule,
  PowerGraphId}`; the placed entity carries `entities::comp::Building`.

## Shared autotiling and join machinery

`autotiler.rs` ports the `Autotiler` interface's default methods into free functions plus the
`BlendWorld` host trait. `build_blending(&dyn BlendWorld, tile, rotation, directional, check_world)`
returns `[case, scale_x, scale_y, bits, non_square_bits]`; `BlendWorld` supplies `blends_block`,
`near_build`, `square_sprite`, `rotated_output` and `block_size`, so a family plugs in its own join
predicate. Belt families implement `BlendWorld` (see `distribution::conveyor::ConveyorBlendWorld`)
and cache the result in `on_proximity_update`. `tile_bitmask::VALUES` maps an 8-neighbor mask to a
slice index used by `<name>-<i>` atlas regions; `load`/`load_variants` resolve those regions.
`distribution::transfer` is the shared item transfer surface (`default_accept_item`,
`default_handle_item`, `offload`, `dump`, `dump_accumulate`, `move_forward`, `front`/`back`,
`relative_dir`, and the `dispatch_*` behavior forwarders).

## Rules

- Register every block by name or `BlockId` from the family's `register`; `default_registry`
  aggregates `distribution`, `storage`, `payloads`, `liquid`, `heat`, `power`, `units::behavior`,
  `production` and `defense`.
- Implement per-tick work in `update_tile`; hot logistics behaviors override `update_batch` to use
  `world::update::building_update_no_consumers`.
- Route item movement through `distribution::transfer`; route liquid movement through
  `liquid::movement`; call `power::module`/`power::graph` for graph links.
- Persist family state with `write`/`read` on the behavior using `BuildingWriter`/`BuildingReader`
  and a `version`; keep item/liquid/power storage in the shared modules.
- Grid/tile reads and writes use `WorldGrid`/`WorldCtx` as described in [`world`](../AGENTS.md); do
  not duplicate footprint, neighbor or placement logic here.

## Invariants

- Every registered behavior returns the same deterministic simulation; no wall-clock or unordered
  iteration in `update_tile`.
- Family state is inserted by `create_state` and is serialized in field order by the behavior's IO
  arms.
- `autotiler::d4` order is `(1,0),(0,1),(-1,0),(0,-1)`; `tile_bitmask::VALUES` is 256 entries and
  every value is `< SLICE_COUNT`.
- Power uses `power::graph::PowerGrids` arena handles (`PowerGraphId`) with `PowerScratch`; graphs
  are read through `power::module::read_power_info`.
- Payloads are real ECS entities marked with `PayloadCarried`; carried builds carry
  `CarriedBuild` and a `TilePos::EMPTY` tile.

## Verification

`power/`, `liquid/` and `heat/` carry in-module tests (`power/tests.rs`, `power/generator_tests.rs`,
`liquid/tests.rs`, `heat/tests.rs`) alongside per-family `#[cfg(test)]` modules. From `client/rust/`:

```bash
cargo test -p mind-core world
cargo test -p mind-core power
cargo test -p mind-core liquid
cargo test -p mind-core heat
```
