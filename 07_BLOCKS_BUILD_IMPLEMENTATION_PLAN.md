# 07 — BLOCKS & BUILD IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | In progress (2026-10-02, `lane/07-blocks`): M0–M3 complete; M4/M7 partial (draw/stats + sandbox behavior). Remaining families, codec registration, revision manifests and M8 are additive on the frozen `BuildingBehavior`/`BlockTable` surface — see Changelog. |
| **Phase** | P3 — World & systems |
| **Depends on** | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (written 2026-10-01; `Tile`/`Tiles`/`WorldGrid`/`Edges`/`WorldHooks`/tile ops — see §2.5 for the reconciled contract). Transitively 00, 02, 03, 04, 05. |
| **Blocks** | `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (written; its §3.2 freezes the `BuildingBehavior`/`Building` surface this plan provides), `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (written; its §3.4 freezes the consumer/module/graph-id surface), `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (written; consumes `Build.validPlace`, `ConstructBlock`, building entities, `TeamData` building sets); also consumed by `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md`, `15_INPUT_RTS_IMPLEMENTATION_PLAN.md`, `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md`, `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md`, `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md`, `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`. |
| **Sources** | `Mindustry/core/src/mindustry/world/AGENTS.md`, `world/blocks/AGENTS.md`, `content/AGENTS.md` (Blocks registry + requirements), `entities/AGENTS.md` (BuildingComp, modules), `graphics/AGENTS.md` (DrawBlock/cached buildings contract); `world/Block.java` (1709 lines, read in full), `world/Build.java`, `world/blocks/ConstructBlock.java`, `world/consumers/*` (23 files), `world/modules/{BlockModule,ItemModule,LiquidModule,PowerModule}.java`, `world/meta/{Attribute,BlockFlag,BlockGroup,BlockStatus,BuildVisibility,Env,Stat,StatCat,StatUnit,StatValue,StatValues,Stats}.java` (StatValues sampled), `world/draw/*` (DrawBlock, DrawMulti, DrawRegion, DrawFlame, DrawLiquidRegion, DrawHeatInput/Output, DrawPistons, DrawFade, DrawWeave, DrawDefault read; the rest enumerated), `entities/comp/BuildingComp.java` (2317 lines, read in full), `entities/comp/{TeamComp,HealthComp,TimerComp,TimedComp}.java`, `entities/units/BuildPlan.java`; representative blocks: `environment/Floor.java`, `production/GenericCrafter.java`, `production/Drill.java`, `defense/Wall.java`, `sandbox/*`, `storage/StorageBlock.java`, `campaign/{Accelerator,LandingPad,LaunchPad}.java` (sampled), `legacy/*`, root helpers; `world/{Tile,Tiles,Edges,WorldContext,WorldParams,CachedTile,TileGen,ItemBuffer,DirectionalItemBuffer,ColorMapper}.java` (skimmed, owned by 06/08/19); `tests/src/test/java/ApplicationTests.java` (§multiblock, blockInventories, blockOverlapRemoved, inventoryDeposit, buildingOverlap, buildingDestruction, allBlockTest), `tests/src/test/java/power/PowerTestFixture.java` (handshake only); `game/EventType.java` (build events). |
| **Extends spine** | Replaces the plan-00 placeholder place/break handling with real `Build`/`Block` behavior. `mind-headless` gains `blocks place|construct|destroy|config|proximity|bench` scenario drivers. State inspector gains a `Buildings` tab (per-tile block/health/rot/team/modules/config). `MindSimHost` (plan 00 API) gains `place_block(x,y,block,rot?) -> bool`, `break_block(x,y) -> bool`, `configure_block(x,y,value) -> bool`, `get_tile_json(x,y) -> String`, `get_buildings() -> i64` (append-only test API, plan 00 §3.10 rule 4). |
| **License** | GPL-3.0 (D6). Behavioral/name parity with upstream classes is the goal; no code is copied verbatim beyond what the license permits and attributes. |

## 2. Scope & parity definition

### 2.1 In scope

The block/building framework and the non-specialized block families, in `mind-core` (Godot-free):

1. **`Block` base runtime.** The behavior half of every block: behavior-kind dispatch, building factory, placement hooks, config handlers, consumer runtime arrays, bar list, draw descriptors, icon descriptors, and the `BlockView` read API over plan 02's `BlockDef`. Derived-value finalization (`health`, `buildTime`, `offset`/`sizeOffset`, consumer partitions, `consPower`, `liquidCapacity`, sound defaults) is implemented/asserted in a single `BlockInit` pass mirroring `Block.init()`/`afterPatch()`; see the 02/07 ownership split in §3.2.
2. **`Building` entity.** ECS representation on plan 05's framework: `Building` component, `ItemModule`/`LiquidModule`/`PowerModule`, family state components, `create()` module allocation, the final `update()` (`timeScale` decay → `updateConsumption()` → `updateTile()` gate), `efficiency`/`optionalEfficiency`/`potentialEfficiency`, `edelta()`/`delta()`/`getProgressIncrease()`, per-building timers, team changes, sleep/wake, health-damage entry points, and lifecycle hooks (`created`, `placed`, `onRemoved`, `overwrote`, `onDeconstructed`, `onDestroyed`, `afterDestroyed`, `onProximityAdded/Update/Removed`).
3. **`Build` static placement/breaking.** `validPlace*`, `validBreak`, `checkNoUnitOverlap`, `contactsGround`/`contactsShallows`, `getEnemyOverlap`, bans/limits/darkness/core-radius checks, line/rotation entry points (`canPlaceOn`, `canReplace`, `canBreak`, `getReplacement`, `planRotation`, `flipRotation`), `beginPlace`/`beginBreak` mutation paths, quick-rotate and derelict-repair paths, `ConstructBlock`/`ConstructBuild` construction/deconstruction accumulation, `instantBuild`/`instantDeconstruct` fast paths, `BlockBuildBeginEvent`/`BlockBuildEndEvent`/`BuildRotateEvent`, `overwrote`/`onDestroyed`/`afterDestroyed` ordering, `BuildPlan`, and the `beginPlace`/`beginBreak` command hooks (relayed by 21, driven by player input in 15, by builders in 11).
4. **Proximity.** `updateProximity`/`removeFromProximity` from `Edges`, `onProximityAdded/Update/Removed` dispatch, teammate-only linking semantics, multiblock assemble/disassemble performed by 06's `Tile` on top of 07's `onProximity*` calls, and a Rust-only `ProximityUpdateEvent` for the block indexer (11) and renderer invalidation (16).
5. **Config.** `config(...)`/`configured(...)`/`configClear(...)`, `config()` readback, `buildConfiguration` data (widget in 14), `configurable`/`configSenseable`/`logicConfigurable`, `lastConfig`/`saveConfig`/`copyConfig`/`clearOnDoubleTap`, and config serialization through 04's `TypeIO`/`TypeValue` with the upstream network whitelist (`Number|Boolean|Content`).
6. **Revisioned building IO.** `writeBase`/`readBase` port (module bitmask, visibility version, timeScale, disabler, efficiency bytes), per-building-kind `version()`/`write()`/`read()` via 04's `EntityIo`/`EntityWriter`/`EntityReader`; building revision manifests.
7. **`DrawBlock` framework base.** `DrawBlock` trait, `DrawDefault`, `DrawMulti`, `DrawRegion`, `DrawFlame`, `DrawLiquidRegion`, `DrawHeatInput`/`DrawHeatOutput`, `DrawPistons`, `DrawFade`, `DrawWeave`, `icons()`/`createIcons()` descriptors, region loading through 03's `#[derive(LoadRegions)]`, and the render-command contract consumed by 16.
8. **Block families owned here** (behavior + `BlockDef` wiring + tests): environment (`Floor`, `OverlayFloor`, `OreBlock`, `StaticWall`, `Prop`, `Cliff`, `ShallowLiquid`, `SteamVent`, `SpawnBlock`), basic defense (`Wall`, `Door`, `AutoDoor`, `Radar`, `Thruster`, `TargetDummy`), production (`GenericCrafter`, `AttributeCrafter`, `Drill`, `BurstDrill`, `BeamDrill`, `Pump`, `SolidPump`, `Fracker`, `WallCrafter`, `Separator`, `Incinerator`), sandbox (`ItemSource`, `ItemVoid`, `LiquidSource`, `LiquidVoid`, `PowerSource`, `PowerVoid`), campaign generic behavior (`Accelerator`, `LandingPad`, `LaunchPad` — campaign wiring in 12), legacy (`LegacyBlock` + subclasses, removed at `World.endMapLoad()`), and helpers (`ControlBlock`, `RotBlock`, `UnitTetherBlock`, `LaunchAnimator`, `ExplosionShield`, `ItemSelection`).

### 2.2 Definition of done

`cargo test -p mind-core` runs every §7a test with no Godot/network; `mind-headless run blocks_*` scenarios pass with committed goldens; the MCP place→configure→destroy scenario passes against the plan-00 spine; §7d budgets are met with an allocation-free steady-state check; every block family above has an executable behavior and at least one scenario assertion. “Done” means a player can place, construct, configure, damage, destroy, save and reload every block in these families with 1:1 semantics — not that their sprites render (16), their items move (08), their power flows (09), or their turrets shoot (10).

### 2.3 Explicit boundaries (who owns what)

| Area | This plan (07) owns | Deferred to |
|---|---|---|
| `Tile`/`Tiles`/`World`/`Edges`/multiblock tile spans/`WorldContext` | only calls into them (`set_block`, `remove`, linked tiles, darkness) | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` |
| `BlockDef` metadata record, content registration, name/ID ABI, tech nodes, bundles, JSON family data | consumes read-only | `02_CONTENT_IMPLEMENTATION_PLAN.md` |
| Atlas/region handles, `LoadRegions`, `loadIcon` chain, block icons in the pack | consumes `AtlasIndex`/`LoadCtx`, provides draw/icon descriptors | `03_ASSETS_IMPLEMENTATION_PLAN.md` |
| Entity/component revision derive, `TypeIO`, save container, settings, FS | uses `EntityIo`/`TypeValue`/`Writes`/`Reads`; building codec registered as a def-specific codec | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |
| Schedule labels, `Groups`, `EntityGroup`, spawn/remove/pool, events, `SimCommand`, components Pos/Team/Health | registers `UpdateBuildings`, defines building components + def entry | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` |
| Item transport (`Conveyor`/`Duct`/`Router`/`Bridge`/`MassDriver`/`Unloader`), payload system, `StorageBlock`/`CoreBlock`, `ItemBuffer`/`DirectionalItemBuffer` | base item-transfer API only (`acceptItem`/`handleItem`/`dump`/`offload`/`removeStack`) | `08_LOGISTICS_IMPLEMENTATION_PLAN.md` |
| `PowerGraph` semantics, power/liquid/heat blocks, `HeatCrafter`, reactors, generators | `ConsumePower`/`PowerModule`/consumer framework; heat hooks as traits | `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` |
| Turrets/projectors/shockmine, `Damage` math, `Lightning`, bullet collision resolution | base `collision`/`damage` entry points + hook interfaces; `Wall`'s lightning/deflect via 10 hooks | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` |
| Unit component framework, builders (`BuilderComp` plan execution), `BlockIndexer`, targeting priorities | `BuildPlan` data + `construct` progress API invoked by builders; `TargetPriority` constant is 02 metadata | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| `Rules`/`Teams`/`TeamData`, block limits, bans, campaign wiring of `Accelerator`/`LandingPad`/`LaunchPad`, derelict rules | uses `Rules` boundary fields; ships provider traits + fallbacks | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` |
| Logic `LAccess`/`Senseable`/`Settable`/`Controllable`, `Building.sense/setProp/control` | exposes building fields/read APIs | `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` |
| Block config/save UI (`ItemSelection` widget, config dialogs), HUD, DB | config *data* + hooks (`buildConfiguration(Table)` → `ConfigUiSpec`) | `14_UI_IMPLEMENTATION_PLAN.md` |
| Input handlers, placement lines/rects/A*, previews, build queue | base placement/rotation/data API; `BuildPlan` queue execution helper | `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` |
| `BlockRenderer`, `FloorRenderer`, cached buildings, cracks, shadows, lights | draw descriptors + `recache()` request hooks | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Editor-only blocks/views (`EditorTile`, `RemoveOre`/`RemoveWall` edit paths, editor config) | behavior flags (`editorConfigurable`, `inEditor`, `buildEditorConfig` data) | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| JSON/patch mod mutation (`DataPatcher`), `@NoPatch` enforcement | `PATCH_DENIED` field list + `afterPatch`/`reinitializeConsumers` APIs | `20_MODS_IMPLEMENTATION_PLAN.md` |
| Command relay ordering, snapshots, building sync | `SimCommand` handlers + deterministic `place/break/configure` APIs | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |

### 2.4 Deliberate deviations (reason stated)

1. **No inner `Building` classes / reflection.** Java `Block.initBuilding()` reflects the first inner class assignable to `Building`. Rust registers one `BuildingBehavior` per `BlockId` (the shape 08 §3.2 froze) plus a `BuildingKind` tag for grouping/IO, and uses family state components for per-tile state (§3.4). Behavior is preserved; runtime reflection does not exist.
2. **One `BlockInstance` per block; behavior + state split.** Java per-block `Block` subclasses hold config in the outer class and per-tile state in the inner class. Rust: per-block singletons hold behavior (`Arc<dyn BuildingBehavior>`), config, regions and draw data; per-tile state is ECS components. No `Rc`/`RefCell`.
3. **Consumers are data + `fn` pointers, not anonymous classes.** `ConsumeSpec` (02) is lowered to `ConsumeInstance`; dynamic consumers (`ConsumePowerDynamic`, `ConsumePowerCondition`, `ConsumeItemDynamic`, `ConsumeLiquidsDynamic`) carry registered C-style function indices so they stay data-driven for mods (20). Closures with captured state are not allowed in sim content.
4. **`ItemSelection` widget is plan 14.** 07 ships the selection *data* (`getPlanConfigs`, `selectionRows/Columns`, `selectScroll`) and `ConfigUiSpec`; the Arc `Table` UI port is 14/20.
5. **`Stats`/`StatValues` split.** 02 owns stat data (`StatSpec`); 07 owns the runtime `Stats` container + bar computation (`BarSpec` evaluation). Display formatting is 14.
6. **`Building.sense/senseObject/setProp/control` are owned by 13** (logic) even though the switch lives in `BuildingComp.java`; 13 implements the trait for the 07 components. 07 keeps the data fields they read/write.
7. **Module flow windows are not global statics.** Java `ItemModule.cacheFlow/cacheSums/displayFlow` are static; Rust keeps the flow window per module and only updates the currently selected building's flow (the only Java caller is `PlacementFragment`), so per-tick cost is unchanged and there are no cross-building statics.
8. **`Block` mutable derived fields (`offset`, `sizeOffset`, `buildTime`, filters) are derived once at content load by 02 and validated by 07**, not re-derived per instance. `@NoPatch` fields become `const PATCH_DENIED` name lists enforced by 20.
9. **No `Vars.headless` checks in `mind-core`.** Sound/effect/visibility calls go through `Platform`/`ClientHooks` (05 §3.3); headless is a no-op sink.
10. **Draw execution is data.** `DrawBlock` produces a Godot-free `DrawCommand` list consumed by 16; 07 never links `gdext` (D1, HLP §2.2).

### 2.5 Plan 06 contract (written 2026-10-01 — reconciled)

`06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` exists and is authoritative for the tile grid. 07 implements 06's `WorldHooks`/`RenderHooks` seams and consumes:

- `WorldGrid` (Resource): `tiles: Tiles`, `generating`, `invalid_map`, `tile_changes`/`floor_changes`, `generator_seed` (06 §3.2).
- `Tile { x, y: i16, data: i8, floor_data: i8, overlay_data: i8, extra_data: i32, floor/overlay/block: BlockId, build: Option<Entity>, changing: bool }` (06 §3.3); `TilePos(i16, i16)` + `TilePos::EMPTY` (carried payloads, 08); no `team`/`rotation` on `Tile` — they live on the building entity (`TeamComp`/`Building.rotation`), exactly like Java.
- Tile mutation via exclusive free functions taking `&mut bevy_ecs::World` (06 §3.4), e.g. `world::set_block(w, x, y, block, team, rot)`, `set_floor/set_overlay/remove/set_air`, `set_tile_blocks`/`fill_tile_*`; the order inside `set_block` (pre-change → `on_removed`+`remove_from_proximity` → swap → multiblock two-pass → `changed` → `update_proximity` → `TileChangeEvent`) is 06's and 07 supplies the hooks. Events suppressed while `generating`.
- `Edges::{edges(size), inside_edges(size), facing_edge(...), pixel_polygon(radius)}` with Java sort order (06 asserts `ApplicationTests.edges`; 08 adds `contract_edges_order`).
- `end_map_load` ordering (06 §3.6): `WorldLoadEndEvent` → per-tile `hooks.legacy_remove_self(block, x, y)` → `build.update_proximity()` → `add_darkness` → `Groups::resize` → `generating=false` → `WorldLoadEvent`, whose listener calls `hooks.check_allow_update()` for every building and resets counters.
- Seam traits 07 implements/registers: `WorldHooks { new_building, check_allow_update, block_changed, floor_changed, legacy_remove_self, ... }`; `RenderHooks` is 16's (no-op defaults in core). 06 M3 ships a bare-`BuildingComp` fallback for `new_building`, so 06 and 07 can land in either order.
- `tile.data` is overloaded (block `saveData` vs static-wall darkness): 07 routes `saveData` writes only through `Tile::should_save_data()` and never on `is_darkened()` tiles (06 §8 R9).
- `WorldGrid::checksum_part()` (06) and `Groups::resize` (05) are used as-is; adding building entities to `Groups.build` changes the 05/23 checksum stream, so the golden swap is a joint commit (06 §8 R10).

## 3. Target design

All names are final unless marked. `mind-core` is Godot-free/tokio-free; `bevy_ecs` pinned by plan 00. No `HashMap` iteration in sim paths (lookup only allowed); ordered maps use `IndexMap`, sorted maps `BTreeMap`.

### 3.1 Module layout

```
client/rust/mind-core/src/world/
  mod.rs                    # WorldPlugin (schedule/systems/event registration), re-exports
  block.rs                  # Blocks (Resource), BlockInstance, BlockView, helper queries
  block_kind_data.rs        # BlockKindData enum + per-kind defs (CrafterDef, DrillDef, WallDef, ...)
  behavior/
    mod.rs                  # BuildingBehavior trait, BuildingKind, registry (register_behavior), dispatch
    environment.rs          # Floor/OverlayFloor/OreBlock/StaticWall/Prop/Cliff/ShallowLiquid/SteamVent/SpawnBlock
    defense.rs              # Wall/Door/AutoDoor/Radar/Thruster/TargetDummy
    production.rs           # GenericCrafter/AttributeCrafter/Drill/BurstDrill/BeamDrill/Pump/SolidPump/Fracker/WallCrafter/Separator/Incinerator
    sandbox.rs              # ItemSource/ItemVoid/LiquidSource/LiquidVoid/PowerSource/PowerVoid
    campaign.rs             # Accelerator/LandingPad/LaunchPad (building half)
    legacy.rs               # LegacyBlock family + endMapLoad sweep
    helpers.rs              # ControlBlock/RotBlock/UnitTetherBlock/LaunchAnimator/ExplosionShield traits
    init.rs                 # BlockInit pass: health/buildTime/offset/consumers/bars/sounds derivation + asserts
  consumers/                # Consume trait + items/liquids/power/coolant/condition/dynamic; payload kinds = 08
  modules/                  # BlockModule trait, ItemModule, LiquidModule, PowerModule, flow windows
  build.rs                  # static validity API: valid_place*, valid_break, contacts_*, enemy overlap
  place.rs                  # begin_place/begin_break, quick-rotate, derelict repair, command application
  construct.rs              # ConstructBlock sizes + ConstructState + construct/deconstruct/finish + instant paths
  plan.rs                   # BuildPlan
  proximity.rs              # update/remove proximity, ProximityUpdateEvent
  config.rs                 # ConfigValue, ConfigKind, config handler registry, plan configs
  building_io.rs            # write_base/read_base, MODULE_* bitmask, BuildingCodec registration
  draw.rs                   # DrawBlock trait, DrawCommand sink, DrawSpec, region slots, icons()
  stats.rs                  # Stats container + BarSpec evaluation (display in 14)
  status.rs                 # BlockStatus computation
  limits.rs                 # BlockCounter resource + trait providers (core radius, bans, placement limits)
  events.rs                 # BlockBuildBegin/EndEvent, BuildRotateEvent, BuildTeamChangeEvent, BuildDamageEvent, BuildingBulletDestroyEvent, ProximityUpdateEvent
  attributes.rs             # Attributes container (Floor affinity data)
client/rust/mind-core/src/entities/comp/building.rs   # Building + module + family state components (05 owns the tree)
client/rust/mind-core/src/entities/defs.rs            # 07 appends the Building def entry (05 owns file)
client/rust/mind-core/revisions/buildings/<kind>.json # building revision manifests (04 owns dir conventions)
```

`WorldPlugin` registers: `update_buildings` into `EntitySet::UpdateBuildings` (05), the placement command handler into 05's command registry, `blocks_apply_commands` (tick start, pre-`TickSet::Frame`), `proximity_drain`, and `world::events` listeners. No other module edits 05's core files (HLP §2.2).

### 3.2 Block data model and the 02/07 ownership split

```rust
// content-side (02) — one per Block content, already constructed in Blocks.load()
pub struct BlockDef { /* as in 02 §6.1 */ pub kind: BlockKind, pub kind_data: BlockKindData, /* ... */ }

// behavior-side (07) — one per BlockId, built at ContentInitEvent
pub struct BlockInstance {
    pub id: BlockId,
    pub building: BuildingKind,                  // inner-Build-class equivalent (grouping/IO/revision)
    pub behavior: Arc<dyn BuildingBehavior>,     // registered by 07/08/09/10/11/20 (08 §3.2 freeze)
    pub kind_data: BlockKindData,                // copied out of BlockDef for fast access
    pub consumers: Consumers,                    // all/optional/non_optional/update + filters + cons_power
    pub configs: ConfigHandlers,                 // ConfigKind -> handler fn
    pub bars: SmallVec<[BarSpec; 4]>,            // static bar descriptors; dynamic ones are per-kind fns
    pub draw: DrawSpec,                          // DrawBlock tree (data)
    pub regions: RegionSlots,                    // resolved via 03 LoadRegions at load()
    pub sounds: BlockSounds,                     // place/break/destroy/ambient handles (18)
    pub icon_descriptors: SmallVec<[RegionName; 4]>,
}
#[derive(Resource)]
pub struct Blocks(pub IndexMap<BlockId, BlockInstance>);

impl Blocks {
    pub fn view<'a>(&'a self, defs: &'a ContentRegistry, id: BlockId) -> BlockView<'a>; // combines BlockDef + BlockInstance
}
pub struct BlockView<'a> { pub def: &'a BlockDef, pub inst: &'a BlockInstance }
```

`BlockView` exposes every read a sim system needs (`has_items`, `has_liquids`, `has_power`, `size`, `offset`, `size_offset`, `health`, `env_*`, `solid`, `placeable_*`, `rotate`, `configurable`, `group`, `flags`, `item_capacity`, `liquid_capacity`, `build_time`, `consumers()`, …) and is the only way behavior code reads blocks. `BlockDef` is never re-registered by 07.

**Ownership reconciliation (plan 02 R3).** The split this plan implements:

| Data | Owner | Notes |
|---|---|---|
| name/id/kind/family tag, `size`, `health`/`scaled_health`, `armor`, `requirements`, `category`, `build_visibility`, `group`, `flags`, `env_*`, `item_capacity`, `liquid_capacity`, `placeable_*`, `solid`/`solidifes`, `rotate`, `update`, `destructible`, `consumers: Vec<ConsumeSpec>`, stats data, research fields, source region names | 02 (`BlockDef`) | source of truth; JSON/patched by 20 under `PATCH_DENIED` rules |
| per-family definition knobs (`craft_time`, `output_items`, `drill_time`, `tier`, `power_production`, `launch_*`, door timings, …) | 02 registration waves construct `BlockKindData` (enum defined in 07, §6.1); stored inside `BlockDef.kind_data` | this is an **additive field on `BlockDef`**; orchestrator action |
| derived `offset`/`size_offset`, final `health`, `build_time` | 02 derives in `post_init`; 07's `BlockInit` **asserts** the equations and exposes them | avoids double derivation; test `blocks_derivation_matches` |
| consumer runtime arrays, `cons_power`, item/liquid filters, `has_consumers`, `liquid_capacity` inference, default sounds, `accepts_items` promotion | 07 `BlockInit` | consumes `BlockDef.consumers` + family data |
| behavior fns (`update_tile`, `accept_item`, `configure`, `on_proximity_*`, `write`/`read`, `draw` handling), building factory, family state components | 07 | one `Arc<dyn BuildingBehavior>` per `BlockId`; mods register via `register_behavior` (20) |

`BlockInit` runs inside 02's `ContentLoader.init()` sweep for `ContentType::Block`, via the lifecycle hook interface 02 provides; `afterPatch` re-runs only the consumer/bars parts (Java `afterPatch` semantics) after 20's patches.

### 3.3 Content lifecycle and tech-tree visibility

- **`init()`** (`BlockInit`): default `destroy_sound`/`place_sound`/`break_sound` by size; `customShadow → has_shadow=false`; `underBullets → priority=TargetPriority.under`; `fogRadius>0 → flags |= hasFogRadius`; `sync → flags |= synced`; health from `scaled_health`; `clipSize`/`lightClipSize`; `hasLiquids && drawLiquidLight → emitLight`; `liquid_capacity` inference from `ConsumeLiquidBase` amounts (`round(10 * max(1, amount*60))`); `acceptsItems` promotion for `BlockGroup::transportation`/`Category::distribution`; build-time from requirements × `build_cost_multiplier` (default 20); consumer partition arrays; `item_filter`/`liquid_filter` sized to current content counts; `setBars()`; `logicConfigurable` from `UnlockableContent`-keyed configs; buffered-power-without-output warning; `sandboxOnly → hideDetails=false`.
- **`load()`** (client stage, skipped headless): `region = atlas.find(name)`; `LoadRegions` on the family `BlockRegions` struct (03 `#[derive(LoadRegions)]`); team variants `<name>-team-<team>` with `team.hasPalette`, else `teamRegion`; `variants>0` → `<name>1..N` (+ shadow variants); draw tree `load()`; floor tiling/autotile/edge region resolution (`TileBitmask::load` from 03); `icons()` cached for 03's pack + 16.
- **`afterPatch()`**: clear/rebuild bars, re-derive `offset`/`size_offset`, resize filters, re-apply all consumers (`Consume::apply`), re-run family `after_patch` (e.g. `GenericCrafter::afterPatch` output-array promotion).
- **`reinitializeConsumers()`** exposed for 20/09; `checkContentArrayCapacity(items, liquids)` grows filters after mod content (Java parity).
- **Visibility/unlock (with 02/12):** `is_hidden` = `!build_visibility.visible(world) && !rules.revealed_blocks.contains(this)`; `is_visible` = `!is_hidden && (editor || !rules.hide_banned_blocks || !is_banned)`; `is_placeable` = `is_visible && (!is_banned || editor) && supports_env(rules.env)`; `environmentBuildable` = `is_on_planet(state.planet)`; `canBeBuilt` = visibility not `Hidden`/`DebugOnly`; `logic_visible`. `build_visibility` predicates are `BuildVisibility` variants evaluated by 07 against `GameState`/`Rules`/`Indexer` providers. Tech-node ownership/`alwaysUnlocked`/`shownPlanets`/`databaseTag` stay in 02; 12 supplies the runtime tech-tree/research gate (`Block::is_researched` hook) used by `isVisible` in campaign contexts.

### 3.4 Building entity on plan 05's ECS

**Def.** 07 appends to 05's `entities/defs.rs`:

```rust
entity_def!(Building = [BaseEntity, SimId, DefId, Pos, TeamComp, Health, Building, Timers],
            groups = BUILD, pooled = true, serialize = false /* custom codec, §3.10 */);
```

Per 08's frozen contract (08 §3.2), behavior is a trait registered per `BlockId`; components are inserted at spawn by `BlockInstance::spawn` based on the block flags and `BuildingKind`:

```rust
pub trait BuildingBehavior: Send + Sync {
    fn update_tile(&self, world: &mut World, e: Entity);
    fn always_update_when_disabled(&self) -> bool { false }     // == !BlockDef.no_update_disabled
    fn create_state(&self, world: &mut World, e: Entity) {}     // family state components
    fn created(&self, world: &mut World, e: Entity) {}
    fn placed(&self, world: &mut World, e: Entity) {}
    fn dropped(&self, world: &mut World, e: Entity) {}
    fn on_removed(&self, world: &mut World, e: Entity) {}
    fn on_deconstructed(&self, world: &mut World, e: Entity, builder: Option<Entity>) {}
    fn on_destroyed(&self, world: &mut World, e: Entity) {}
    fn after_destroyed(&self, world: &mut World, e: Entity) {}
    fn overwrote(&self, world: &mut World, e: Entity, previous: &[Entity]) {}
    fn on_proximity_added(&self, world: &mut World, e: Entity) {}
    fn on_proximity_update(&self, world: &mut World, e: Entity) {}
    fn on_proximity_removed(&self, world: &mut World, e: Entity) {}
    fn config(&self, world: &World, e: Entity) -> ConfigValue { ConfigValue::None }
    fn configured(&self, world: &mut World, e: Entity, player: Option<Entity>, value: ConfigValue) {}
    fn sense(&self, world: &World, e: Entity, sensor: u16) -> f64 { 0.0 }   // LAccess mapping owned by 13
    fn version(&self, world: &World, e: Entity) -> u8 { 0 }
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter);
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8);
    fn get_tiling(&self, world: &World, e: Entity) -> Option<[i32; 5]> { None }  // 16 hint only
    // item/liquid/payload transfer defaults (Java BuildingComp bases); 08 overrides:
    fn accept_item(&self, world: &World, e: Entity, src: Entity, item: ItemId) -> bool { false }
    fn handle_item(&self, world: &mut World, e: Entity, src: Entity, item: ItemId) {}
    fn accept_liquid(&self, world: &World, e: Entity, src: Entity, liquid: LiquidId) -> bool { false }
    fn handle_liquid(&self, world: &mut World, e: Entity, src: Entity, liquid: LiquidId, amount: f32) {}
    fn get_liquid_destination(&self, world: &World, e: Entity, from: Entity, liquid: LiquidId) -> Option<Entity> { None }
    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> { None }
    fn dump(&self, world: &mut World, e: Entity, item: Option<ItemId>) -> bool { false }
    fn dump_accumulate(&self, world: &mut World, e: Entity, item: Option<ItemId>) -> bool { false }
    fn offload(&self, world: &mut World, e: Entity, item: ItemId) {}
    fn move_forward(&self, world: &mut World, e: Entity, item: ItemId) -> bool { false }
    fn can_dump(&self, world: &World, e: Entity, to: Entity, item: ItemId) -> bool { true }
    fn efficiency_scale(&self, world: &World, e: Entity) -> f32 { 1.0 }
}
pub fn register_behavior(block: BlockId, behavior: Arc<dyn BuildingBehavior>);   // 08/09/10/11/20
```

`BuildingWriter`/`BuildingReader` are 07 re-export aliases of 04's `EntityWriter`/`EntityReader` (04 §3.5), so 08/09 per-block codecs compile against either name. `team` and `health`/`dead` stay on 05's `TeamComp`/`Health` (not duplicated here); behavior implementations read them through 05 helpers (`building::team(world, e)`, `health::hp(world, e)`).

Components (07-owned, inserted by `BlockInstance::spawn`):

```rust
#[derive(Component)] pub struct Building {
    pub tile: TilePos, pub block: BlockId, pub rotation: u8,
    pub enabled: bool, pub last_disabler: Option<Entity>,
    pub efficiency: f32, pub optional_efficiency: f32, pub potential_efficiency: f32,
    pub should_consume_power: bool,
    pub time_scale: f32, pub time_scale_duration: f32,
    pub proximity: Proximity,                     // SmallVec<[Entity; 6]>, Edges::edges(size) order
    pub cdump: u8, pub dump_accum: f32,           // Java dump cursor/accumulator
    pub last_accessed: Option<String>,
    pub visible_flags: u64, pub was_visible: bool, pub was_damaged: bool,
    pub sleeping: bool, pub sleep_time: f32, pub initialized: bool,
    pub heal_suppression_time: f32, pub last_heal_time: f32, pub last_damage_time: f32,
}
#[derive(Component)] pub struct Timers(pub SmallVec<[f32; 4]>);       // Interval(block.timers)
#[derive(Component)] pub struct ItemModule { /* ints, total, take_rotation, flow: Option<FlowWindow> */ }
#[derive(Component)] pub struct LiquidModule { /* floats ring + current, flow */ }
#[derive(Component)] pub struct PowerModule { pub status: f32, pub init: bool, pub graph: PowerGraphId, pub links: SmallVec<[i32; 4]> }
// PowerGraphId is defined in 07 (plain { slot: u32, generation: u32 } newtype); 09's PowerGraphs arena consumes it (09 §3.2).
// family state components live in entities/comp/building.rs:
#[derive(Component)] pub struct CrafterState { pub progress: f32, pub warmup: f32, pub total_progress: f32, pub output_accumulator: SmallVec<[f32; 2]> }
#[derive(Component)] pub struct DrillState { pub progress: f32, pub warmup: f32, pub time_drilled: f32, pub last_drill_speed: f32, pub dominant_item: Option<ItemId>, pub dominant_items: u16 }
#[derive(Component)] pub struct WallState { pub autotile_bits: u8, pub hit: f32 }
#[derive(Component)] pub struct DoorState { pub open: bool }
#[derive(Component)] pub struct RadarState { pub progress: f32 }
// ...one per family, named after the Java Build-class state fields
```

**Spawn.** `BlockInstance::spawn(world, tile, team, rot, should_add) -> Entity`:
1. `create()` module allocation exactly like `BuildingComp.create` (`has_items → ItemModule`, `has_liquids → LiquidModule`, `has_power → PowerModule` + graph join hook (09)), `health = block.health`, `maxHealth`, `Timers(block.timers)`; then `behavior.create_state()` inserts family state components; `initialized = true`.
2. `init(tile, team, should_add, rot)`: set `tile`/`rotation`; position `tile.drawx/drawy`; if `!initialized` run step 1; else re-init power (`power.init=false`, new graph — 09); clear proximity; `add()` if `should_add`; `check_allow_update()`; `behavior.created()`.
3. 06's `WorldHooks::new_building` (registered by 07 at plugin init) calls this from `world::set_block`; 06 links covered tiles/proxies to the returned entity (06 §3.4).

**Per-tick update.** 07 registers `update_buildings(world: &mut World)` in `EntitySet::UpdateBuildings` (after `UpdatePowerGraph`, before `UpdateBullets`; run condition `!editor`). It iterates `Groups.build` in insertion-stable slot order (05) and calls `building_update`:

```rust
fn building_update(world: &mut World, e: Entity) {
    { let mut b = world.get_mut::<Building>(e); if (b.time_scale_duration -= 1.0) <= 0.0 { b.time_scale = 1.0; } }
    update_consumption(world, e);                    // §3.5, exact port
    let (enabled, block) = { let b = world.get::<Building>(e); (b.enabled, b.block) };
    let beh = blocks.behavior(block);                // Arc<dyn BuildingBehavior>, one per BlockId
    if enabled || beh.always_update_when_disabled() { beh.update_tile(world, e); }
}
```

Dispatch is one vtable call per building per tick (08 §3.2 freeze). 08/09/10/11/20 register their families with `register_behavior`. Optionally (M8 perf), `update_buildings` pre-groups entities by `BlockId` into reusable per-kind scratch `SmallVec`s purely for cache locality; ordering inside a group stays `Groups.build` slot order.

**Sleep/wake.** `sleep()`/`no_sleep()` port: sleeping removes the entity from `Groups.build` (05 supports O(1)); `sleeping_entities` becomes a [`TickReport`](05) counter. `update_proximity`/hooks call `no_sleep()` like Java. Determinism: membership changes are command/update-driven and deterministic.

**Damage/health.** `Health` (05) owns `health`/`max_health`/`dead`; 07 implements `damage`/`damage(Bullet, team, amount)`, `handle_damage`, `collision`, `check_solid`, `absorb_lasers`, `health_changed` (fires `BuildDamageEvent`, calls the 11 indexer hook, batches net health in 21) and `killed()` ordering: `dead=true` → `BlockDestroyEvent` → `onDestroyed()` → `tile.remove()` → `remove()` → `afterDestroyed()`. `damaged()` = `health < max_health - 0.001`; cracks/shake are 16; armor math in `collision` is the 10 `Damage` API (07 calls `Damage::apply_armor`).
**`changeTeam`** ports the safe path (indexer remove/add hooks, power reflow via 09, `BuildTeamChangeEvent`, pathfinder tile update hook, proximity refresh, `check_allow_update`). `forceTeam` honored.

### 3.5 Consumers (07) and power boundary (09)

Lowering: `ConsumeSpec` (02) → `ConsumeInstance` enum with exact Java fields and flags (`optional`, `booster`, `update`, `ignore`). Runtime arrays follow the Java partition exactly:

- `consumers` = `consume_builder` order.
- `optional_consumers` = `optional && !ignore()`.
- `non_optional_consumers` = `!optional && !ignore()`.
- `update_consumers` = `update && !ignore()`.
- `cons_power` = the single `ConsumePower` (registering a second removes the first); `ConsumePower::ignore() == buffered`.
- `apply(block)` effects: items/liquids/power flags + filters; `ConsumeLiquidBase::apply` sets `has_liquids`; coolant filters by `coolant/gas/temperature/flammability` (exact `ConsumeCoolant` predicate).

`update_consumption` is a line-by-line port of `BuildingComp.java:1950-2011` (quoted default in §3.4/§6): cheat shortcut (`team.rules().cheat`), disabled shortcut, two-pass min efficiency, optional min, `potential_efficiency`, zero on `!update`, `update_efficiency_multiplier`, then `update_consumers` when `efficiency > 0`. `edelta()` inside consumer `efficiency` sees `efficiency == 1` during the pass (Java sets it before the pass) — replicate exactly.

`ConsumePower::efficiency(build) = power.status`; `requested_power(entity) = buffered ? (1-status)*capacity : usage * should_consume ? 1 : 0`; condition/dynamic variants override via registered `fn` indices. `PowerModule` fields (`status`, `init`, `graph: PowerGraphId`, `links`) are 07-owned and serialized by 07; `PowerGraphId` is defined in 07 (09 §3.4 expects it), while the arena, graph algorithms, link resolution, diode/battery behavior, and status updates are 09. `update_power_graph`/`power_graph_removed`/`get_power_connections` call the 09 `PowerGraphs` API; 09 registers `power_graph_removed` through 07's `on_proximity_removed` path. Heat: 07 defines the `heat()`/`heat_frac()`/`side_heat()` hooks as a `HeatApi` trait; 09 implements the network and `DrawHeatInput/Output` values.

### 3.6 Block/Build static placement (`world/build.rs`, `world/place.rs`)

Port `Build.java` directly:

- `valid_place(type, team, x, y, rot, check_visible=true, check_core_radius=true) = valid_place_ignore_units(...) && check_no_unit_overlap(...)`.
- `valid_place_ignore_units`: editor/visibility/placeable gate; core-radius block (polygon protection via `CoreRadiusProvider` default fallback, else `any_enemy_cores_within_build_radius`); tile null; `can_place_on`; floor/overlay special path; darkness ≥ 3; `requires_water`/`contacts_shallows`/`placeable_liquid`; `is_over_placement_limit`; the per-tile multiblock scan (deep water, static fog discovered, derelict, same-block-same-rotation, `interactable`, floor `placeable_on`, payload replacement hack `!check_visible && check_core_radius && !always_replace`, `can_replace`/`canBeReplaced`/construct-same-type, bounds containment, requires-water drop); `place_range_check` enemy-overlap via 11 indexer hook.
- `valid_break(team,x,y)`: `tile.block != air && can_break(tile) && (tile.breakable() || rules.allow_environment_deconstruct) && tile.interactable(team)`.
- `contacts_ground`/`contacts_shallows` with `Edges` for multiblocks.
- `begin_place(unit, result, team, x, y, rot, config)`: quick-rotate path (`BuildRotateEvent`, rotate Fx/sound hook); derelict repair path (`allowDerelictRepair && rules.derelict_repair`); clear always-replace props in linked tiles; `instantBuild` path (`placeBegan` → `construct_finish`); else record `previous`, `prevBuild` (dedup by build id), `beforePlaceBegan`, swap to `ConstructBlock.get(size)`, `set_construct`, `BlockBuildBeginEvent`, `placeBegan`.
- `begin_break(unit, team, x, y)`: `valid_break`; capture health fraction/rotation/previous; `instantDeconstruct` → `deconstruct_finish`; else `onDeconstructed` (liquid spill), `dead=true`, swap to construct block, `set_deconstruct`, health carry-over, `BlockBuildBeginEvent(breaking=true)`.
- Rotation entry points: `plan_rotation`, `flipRotation`, `canReplace` (exact `Block.canReplace` predicate incl. `group.anyReplace`, `subclass` equality proxy = `BlockKind` equality, `alwaysReplace`, `privileged`, `quickRotate`), `getReplacement`, `changePlacementPath`, `handlePlacementLine`, `onNewPlan`, `drawArrow`/`allowDiagonal`/`conveyorPlacement`/`swapDiagonalPlacement`/`allowRectanglePlacement`/`schematicPriority` data.
- `BuildPlan` (`world/plan.rs`): exact fields + `placeable`/`is_rotation`/`is_derelict_repair`/`same_pos`/`point_config`/`copy`/`bounds`/`is_done`/`tile`/`build`/`hitbox`; `screen_to_world` input helpers stay in 15.
- Command hooks: `CommandHandler` registry in 05's command path. 07 registers `Place`, `Break`, `Configure` handlers that call the same functions 15 (player) and 11 (builder units) call. Remote/relay concerns are 21; no direct `Call.*` here.
- Limits/darkness/core radius: `limits.rs` defines `BlockCounter` (`IndexMap<(TeamId, BlockId), u32>` maintained on place/remove/finish) as the default `is_over_placement_limit` provider reading `rules.block_limits` (12); `CoreRadiusProvider` trait with a 07 fallback that scans `Groups.build` for `BlockFlag::core` and checks `TeamRules::protect_cores`/`check_placement` (12 overrides).
- Derelict rules: `derelict_repair` flag/rule, `allow_derelict_repair`, repair effects/sound hooks.

### 3.7 `ConstructBlock`/`ConstructBuild` (`world/construct.rs`)

- One `BlockInstance` per size 1..=16 named `build<size>` (registered as real content by 02's `Blocks.load`, kind `BlockKind::Construct`, hidden, `update=true`, `health=10`, `consumesTap=true`, `solidifes=true`, `inEditor=false`, `sync=true`, no icons). 07 registers their `BuildingKind` and special-case placement/removal so they are never placeable by the player.
- `ConstructState` component: `current: BlockId`, `previous: BlockId`, `prev_build: SmallVec<[Entity; 9]>`, `progress: f32`, `build_cost: f32`, `last_config: ConfigValue`, `last_builder: Option<Entity>`, `was_constructing: bool`, `active_deconstruct: bool`, `construct_color: f32`, `accumulator: SmallVec<[f32; 4]>`, `total_accumulator: SmallVec<[f32; 4]>`, `items_left: SmallVec<[i32; 4]>`.
- `set_construct(previous, block)` / `set_deconstruct(previous)`: allocate accumulators, seed `items_left = round(amount * rules.build_cost_multiplier)`, `build_cost = block.build_time * multiplier`, pathfinder tile hook (11).
- `construct(builder, core, amount, config)`: exact port of `ConstructBuild.construct` including `check_required` item pull/removal, `infinite_resources` shortcut, `progress >= 1` finish gate with leftover-item consumption and `is_over_placement_limit` re-check, `constructed()` (which must be server-only in Java; here the command path already guarantees ordering).
- `deconstruct(builder, core, amount)`: reset accumulators when switching from construct; apply `deconstruct_refund_multiplier`; transfer refund directly to core (`storage_capacity`, `unlockedNowHost`), `items_left` bookkeeping, `progress <= deconstruct_threshold` finish, rounding-error top-up.
- `construct_finish(tile, block, builder, rot, team, config)`: overlay/floor vs block set; health-fraction carry; `configured(builder, config)`; `overwrote(prev)`; `lastAccessed`; indexer health hook; `player_placed` hook (15/14); place effect/sound hooks; `placeEnded`; `BlockBuildEndEvent`.
- `deconstruct_finish(tile, block, builder)`: break effect/sound, `BlockBuildEndEvent(breaking=true)`, `tile.remove()`.
- Shared pitch logic (`shouldPlay`, `calcPitch`) becomes deterministic: Java uses `Time.millis`; **deviation** — port to `Time.time`-based counters seeded from sim time (recorded in §2.4); sounds are view-only so this does not affect checksums.
- `ConstructBlock.constructed()` is the only finish path used by builders; instant build calls it directly.

### 3.8 Proximity (`world/proximity.rs`)

Exact port of `removeFromProximity`/`updateProximity`:

- `remove_from_proximity`: `onProximityRemoved()`, collect `Edges::edges(size)` neighbors, for each: remove self from their `proximity`, call `onProximityUpdate()`, then clear own.
- `update_proximity`: clear; for each edge neighbor with same team: `add_unique` self to other's proximity; collect dedup set; assign own proximity; `onProximityAdded()` → `onProximityUpdate()` → each neighbor `onProximityUpdate()`; `drawCached → recache()` view hook.
- Fired by 06's `Tile` at the same points Java fires them: `Tile.preChanged` (removal) and `Tile.changed` (update), suppressed while `world.is_generating()`.
- `ProximityUpdateEvent { entity }` (Rust-only, documented): fired once per building after `onProximityUpdate` completes, consumed by 11's `BlockIndexer` and 16's invalidation; Java has no equivalent public event (the hook *is* the interface). This satisfies the assignment's “`ProximityUpdateEvent` semantics” by making the hook observable without changing hook order.
- Multiblock linking/rebuild: 06's `world::set_block` links every covered tile/proxy to the center entity; breaking from any covered tile routes through `begin_break` at the center; `overwrote(prev)` receives deduped previous builds; damaged-index/repair behavior for multiblocks is 11 (indexer) + 12 (objectives) + 16 (cracks); 07 states `was_damaged` and calls `indexer.notify_health_changed` (11 hook). Destruction must call the behavior's `on_proximity_removed` before entity removal so 09's `power_graph_removed` runs on every removal path (09 §3.13).

### 3.9 Config system (`world/config.rs`)

```rust
pub enum ConfigValue { None, Item(ItemId), Liquid(LiquidId), Block(BlockId), Unit(UnitTypeId),
                       Content(ContentType, u16), Point2(i32, i32), Point2Array(SmallVec<[i32; 2]>),
                       Number(f64), Bool(bool), Building(Entity), Bytes(SmallVec<[u8; 32]>), String(SmolStr) }
pub type ConfigureFn = fn(&mut World, Entity, Option<Entity> /*builder*/, &ConfigValue);
pub struct ConfigHandlers { by_kind: SmallVec<[(ConfigKind, ConfigureFn); 2]>, clear: Option<ConfigureFn> }
```

- `configure(e, value)` → `block.last_config = value` (per `BlockInstance`) + `tile_config` command (21 relay / direct in harness); `configure_any` (server-style, no player).
- `configured(builder, value)` dispatch: normalize value kind (anonymous subclass → superclass), player last-access update, handler lookup, fallback “copy config from Building value” behavior, `configClear` on `None`.
- `config()` readback per family (e.g. `ItemSource → Option<ItemId>`); `next_config()` (`saveConfig`); `plan_configs` (`getPlanConfigs`: items if `configurations.containsKey(Item)`, liquids likewise); `point_config` transform hook (ties/rotors); `selection_rows/columns`, `select_scroll`.
- `configurable`, `logic_configurable` (13 calls `configured` under privilege rules), `config_senseable`, `ignore_resize_config`.
- Serialization: 04 `TypeIO` `Object` codec; only `Number|Boolean|Content` allowed in network plan queues (04 `write_plans`); save configs unrestricted (whitelist per kind in §6.3).
- `buildConfiguration` returns a `ConfigUiSpec` (item/liquid/block/unit selection lists, sliders, clear button) consumed by 14; `ItemSelection` data (`items/liquids` filtered by unlocked/on-planet/hidden, rows/cols) is built here.

### 3.10 Revisioned building IO (`world/building_io.rs`)

`write_base` is a byte-exact port of `BuildingComp.writeBase`:

```
f32 health
u8  rotation | 0x80 (marker)
u8  team.id
u8  version(4 if fog-visibility else 3)
u8  enabled
u8  moduleBitmask()
ItemModule  if bit0   (sparse count: i16 count + (i16 id, i32 amount)* ; legacy read u8)
PowerModule if bit1   (i16 links + i32 pos*, f32 status)
LiquidModule if bit2  (sparse …)
u8  consume bit (version<=2 only on read; bit3 always set)
f32 timeScale + f32 timeScaleDuration if bit4
i32 lastDisabler.pos() if bit5
u8  efficiency*255, u8 optionalEfficiency*255  (version>=3)
u64 visibleFlags (version==4)
```

`read_base` mirrors it including legacy `moduleBits` from `moduleBitmask()` and the `version<=2` consume bool, NaN/inf status sanitization, health cap by `block.health`.

Building codec registration with 04: because upstream `@EntityDef(genio=false, serialize=false)` makes Building a custom-IO def, 07 registers a `BuildingCodec` (`fn write(world, entity, w)`, `fn read(world, block, entity/None, r, revision)`) into 04's entity registry under `class_id(BuildingComp)`. `write` = `write_base` then `BuildingBehavior::write`; `read` = `read_base` then `BuildingBehavior::read(revision)`. Per-kind revisions: `BuildingKind::revision()` (e.g. `ConstructBuild=1`, `DrillBuild=1`, `SeparatorBuild=1`, `LiquidSourceBuild=1`, `AcceleratorBuild=1`, `LaunchPadBuild=1`; default 0). Manifests under `mind-core/revisions/buildings/<kind>.json` with field name/type/order; 04's `io check-revisions` checks these too (reconciliation item R3).

Module classes are 07-owned (`BlockModule` trait + `write(w, legacy)`/`read(r, legacy)`), and the module bitmask is exported as constants (`MODULE_ITEM=1`, `MODULE_POWER=2`, `MODULE_LIQUID=4`, `MODULE_CONSUME=8`, `MODULE_TIMESCALE=16`, `MODULE_DISABLER=32`).

### 3.11 Draw, icons, stats and bars (contract with 03/16)

`DrawBlock` is a Godot-free descriptor that emits draw commands:

```rust
pub trait DrawBlock: Send + Sync {
    fn load(&mut self, block: BlockId, load: &BlockLoadCtx);                     // resolves regions via 03
    fn draw(&self, out: &mut DrawCommands, b: &BuildDrawData);                   // pushes sprites/shapes
    fn draw_light(&self, out: &mut DrawCommands, b: &BuildDrawData) {}
    fn draw_plan(&self, out: &mut DrawCommands, p: &BuildPlanDrawData) {}
    fn icons(&self) -> SmallVec<[RegionName; 4]> { smallvec![] }
    fn regions_to_outline(&self) -> SmallVec<[RegionName; 2]> { smallvec![] }
}
```

`DrawCommands` is an allocation-free per-frame command buffer (`Sprite { region, x, y, rot, color, blend, z }`, `Shape`, `Lines`, `Fill`, `SetZ`, `SetBlend`) consumed by 16's block renderer inside the same frame; commands never feed back into the sim. `DrawSpec` composes: `Default`, `Multi(SmallVec<[DrawSpec; 4]>)`, `Region { suffix/name, rotate_speed, spin, color, layer }`, `Flame { … }`, `LiquidRegion`, `HeatInput`, `HeatOutput`, `Pistons`, `Fade`, `Weave`. `iconOverride`/`finalIcons` and the `<name>-full` icon packing rules are exposed to 03 (`icons()`, `getRegionsToOutline`, `makeIconRegions`, `outline_color/radius/outlined_icon`, `has_color`/`map_color` from the center pixel for non-synthetic blocks). `icons()`/`createIcons()` are thus split: descriptors here, offline pixel work in 03, runtime icon caching in 03/16.

`Stats` (07) is a typed accumulator (`Vec<(StatCat, Stat, StatValue)>`, `use_categories`, `time_period`); `set_stats` for these families is mostly built from 02's `StatSpec` data plus family additions (e.g. `GenericCrafter` production time/output, `Drill` tier/speed, `Wall` deflect/lightning, `ItemSource` output rate). `StatValues` formatting stays in 14; 07 only supplies values. Bars: `BarSpec { name, label: BarLabel, color: BarColor, fraction: BarFn }` with `BarFn` either a simple accessor (health, power, items, liquid-by-id, dynamic-current-liquid) or a per-kind `fn(&World, Entity) -> f32`; `add_liquid_bar`, `remove_bar`, `list_bars` ported; `DrillBuild` drill-speed bar, `GenericCrafter` output-liquid bars, `ConstructBuild` progress/breaking bars included.

### 3.12 Family behavior specs (what each module implements)

| Family | Ported behavior highlights |
|---|---|
| environment | `Floor`: movement/status/damage/drown fields, `liquidDrop`/`liquidMultiplier`, `wall`/`decoration` resolution in `init`, `edge` fallback, blend-group ids, tiling/autotile region setup, `drawBase`/`drawMain`/`drawOverlay`/edges as draw commands (16 executes), `floorChanged`, `shouldIndex`, `updateRender`/`renderUpdate` (Fx via 17), `isDeep`/`hasSurface`. `OverlayFloor` place rule (`!wallOre \|\| solid`), `OreBlock` item/name/color + ore shadow icon data, `StaticWall` autotile + 2×2 large variant rules + `canReplace`, `Prop` layer/break defaults, `Cliff` data-mask rendering + minimap color, `ShallowLiquid` derivation, `SteamVent` 9-tile adjacency + center indexing + vent effect cadence, `SpawnBlock` editor-only draw + `needsSurface=false`. |
| defense | `Wall` (group walls, `buildCostMultiplier=6`, `crushDamageMultiplier=5`, `envEnabled=any`, autotile `WallState`, cached draw contract, hit flash; lightning/deflect hook via 10), `Door`/`AutoDoor` (timers, open/close effects+sounds, solidity when closed/open, chain effect), `Radar` (discovery progress via 11 fog), `Thruster`, `TargetDummy` (tether `UnitType`, pull, dps timer, `UnitTetherBlock` impl). |
| production | `GenericCrafter` (progress/warmup/craft/dumpOutputs, output items/liquids/directions, `shouldConsume` fullness rules, `getProgressIncrease` liquid scaling, `ignoreLiquidFullness`, `scaleOutput`, `dumpExtraLiquid`), `AttributeCrafter` (attribute sum efficiency boost), `Drill` (ore counting/sorting, dominant item, tier/blocked items, boost, `drillMultipliers`, hardness), `BurstDrill` (burst timing), `BeamDrill` (beams, glow, item output), `Pump` (pump amount from floor liquid, boost), `SolidPump` (attribute-scaled, `warmup`, liquid), `Fracker` (item + liquid, attribute), `WallCrafter` (facing wall item, efficiency), `Separator` (liquid in → weighted random results with `Mathf.chance` RNG stream, accumulators), `Incinerator` (power-gated item/liquid burn). |
| sandbox | All `envEnabled=any`, `noUpdateDisabled`; `ItemSource` config + `itemsPerSecond` emission; `ItemVoid` fake flow module; `LiquidSource`/`LiquidVoid`; `PowerSource` (`outputsPower`, `powerProduction`); `PowerVoid` (`consumePower(f32::MAX)`); all four sandbox power paths honor 09's graph API. |
| campaign (generic) | `Accelerator` (state machine, power buffer requirement, launch animation via `LaunchAnimator`, draw rings), `LandingPad` (item config, landing sequence, liquid consumption, inventory, cooldown), `LaunchPad` (launch wait/time, item capacity checks, config, animation); launch targets/resource transfer/`Universe` effects are 12 (`LaunchTargetProvider` trait). |
| legacy | `LegacyBlock` marker + subclasses registered as hidden content (02), removed during `World.end_map_load()` via a `WorldLoadEnd` listener (06 fires), replacements applied where defined. |
| helpers | `ControlBlock` (`unit()`, `is_controlled`, `can_control`, `should_auto_target`), `RotBlock` (`build_rotation`), `UnitTetherBlock` (`spawned(id)`), `LaunchAnimator` (draw/begin/end/update + `launch_duration`/`zoom`), `ExplosionShield` (`absorb_explosion`) as Rust traits implemented by family modules and later plans. `ItemSelection` = data only (14). |

### 3.13 Godot / STDB surfaces touched

- **Godot:** no new scenes. `mind-gdext` adds the `MindSimHost` methods listed in §1 driving the 07 command API; the inspector reads the plan-00 dump (extended with `buildings[]`: `id`, `block`, `x`, `y`, `team`, `rot`, `health`, `enabled`, `efficiency`, `config`, module summaries). Draw output flows to 16.
- **STDB:** no new tables/reducers here. 07's `SimCommand::{Place,Break,Configure}` handling is the client-side application path plan 21 relays; building IO feeds 21's snapshot/streaming through 04.

### 3.14 Reconciliation with the written sibling plans (06/08/09/11)

| Sibling | Interface frozen there | 07 resolution |
|---|---|---|
| 06 | `WorldGrid`/`Tile`/tile ops/`WorldHooks`/`RenderHooks`; `Tile.build: Option<Entity>`; legacy-removal and `check_allow_update` hooks; `tile.data` overload (06 §3.2–3.6, §8 R2/R9) | §2.5 adopts it verbatim; 07 registers the `WorldHooks` implementation; `ProximityUpdateEvent` is a Rust-only observable on top of the exact Java hook order. |
| 08 | `BuildingBehavior` trait + `Building` component + `register_behavior` + `ItemModule` ordering + `BuildingWriter/Reader` + `BlockKindData` hook (08 §3.2) | Adopted in §3.4; `team`/`health` stay on 05's components with helpers (R16); `ItemModule::first/take` ordering honored; payload/tiling hooks default to no-op until 08 overrides. |
| 09 | `PowerModule { status, init, graph: PowerGraphId, links }`, consumer semantics, `on_proximity_removed` before removal, `GenericCrafter` base ownership (09 §3.4/§3.13, R3) | `PowerGraphId` defined in 07 (`world/modules/power.rs`); graph/heat network stays 09; removal ordering asserted (§3.8); 07 owns `GenericCrafter` for 09 to extend. |
| 11 | `Build.valid_place`, `ConstructBlock`, building entities, `TeamData` building sets, `BuildPlan` driven by `BuilderComp` (11 §3) | 07 exposes the placement/construct API + `BuildPlan`; 11 calls it from `BuilderComp::update_build_logic`; `TeamData.get_buildings` provider is 12 with 07's `BlockCounter` fallback. |

## 4. Port map

Legend: **owner** column references this plan unless another file is named.

| Mindustry source | Target Rust | Notes on adaptation |
|---|---|---|
| `world/Block.java` (fields, `init`, `afterPatch`, `load`, `setStats`/`setBars`, `createIcons`, placement hooks, `sense` excluded) | `world/block.rs`, `world/behavior/init.rs`, `world/stats.rs`, `world/draw.rs`; `BlockDef` stays 02 | Metadata fields live in `BlockDef` (02); behavior side table keyed by `BlockId` (07); `sense`/`control` → 13. |
| `world/Build.java` | `world/build.rs` (validity) + `world/place.rs` (mutation) | `@Remote` statics become plain API + `SimCommand` handlers (21 relays). |
| `world/blocks/ConstructBlock.java` | `world/construct.rs` | 16 size singletons; byte-versioned `ConstructState`; no reflection. |
| `world/consumers/Consume.java` + 22 subclasses | `world/consumers/*.rs` | Enum + flags; dynamic variants use registered `fn` indices; payload consumers registered by 08. |
| `world/modules/{BlockModule,ItemModule,LiquidModule}.java` | `world/modules/{mod,items,liquids}.rs` + `entities/comp/building.rs` components | Static flow caches removed (deviation 7). |
| `world/modules/PowerModule.java` | `world/modules/power.rs` + component | Graph algorithms 09; links/status persisted here. |
| `world/meta/{BlockFlag,BlockGroup,Env}` | `content` enums in 02; 07 consumes bitflags | Ordinals/names ABI. |
| `world/meta/BuildVisibility.java` | `world/block.rs` (`BuildVisibility` variant evaluation) | Closures → `fn` predicates over `GameState`/`Rules`/indexer. |
| `world/meta/Attribute.java`, `world/blocks/Attributes.java` | `content` `Attribute` (02) + `world/attributes.rs` | Float array grows with `Attribute.all`. |
| `world/meta/{Stat,StatCat,StatUnit,StatValue,StatValues,Stats}.java` | `world/stats.rs` (container); stat data in 02; formatting 14 | 07 supplies values only. |
| `world/meta/BlockStatus.java` | `world/status.rs` | `active` pulse uses `state.tick`. |
| `world/draw/*.java` (DrawBlock, DrawDefault, DrawMulti, DrawRegion, DrawFlame, DrawLiquidRegion, DrawHeatInput/Output, DrawPistons, DrawFade, DrawWeave) | `world/draw.rs` (+ `behavior/` for per-kind drawing) | `DrawTurret` and turret drawers → 10; remaining drawers ported as `DrawSpec` variants (16 executes). |
| `entities/comp/BuildingComp.java` | `entities/comp/building.rs` + `world/{proximity,config,building_io,status,limits}.rs` + `behavior/` | `sense`/`setProp`/`control` → 13; payload methods → 08; power graph calls → 09; render/UI calls → 16/14. |
| `entities/units/BuildPlan.java` | `world/plan.rs` | Data type shared by 11/15; no Arc collections. |
| `world/blocks/environment/{Floor,OverlayFloor,OreBlock,StaticWall,Prop,Cliff,ShallowLiquid,SteamVent,SpawnBlock}.java` | `behavior/environment.rs` | Draw parts → `DrawSpec`/16; ore shadow icons → 03. |
| `world/blocks/defense/{Wall,Door,AutoDoor,Radar,Thruster,TargetDummy}.java` | `behavior/defense.rs` | Lightning/deflect hooks → 10; fog discovery → 11. |
| `world/blocks/production/{GenericCrafter,AttributeCrafter,Drill,BurstDrill,BeamDrill,Pump,SolidPump,Fracker,WallCrafter,Separator,Incinerator}.java` | `behavior/production.rs` | `HeatCrafter` → 09; `SingleBlockProducer`/`BlockProducer` → 08. |
| `world/blocks/sandbox/*.java` | `behavior/sandbox.rs` | `PowerSource`/`PowerVoid` graphs → 09 at runtime. |
| `world/blocks/campaign/{Accelerator,LandingPad,LaunchPad}.java` | `behavior/campaign.rs` | Launch targets/resources/Universe → 12 via provider trait. |
| `world/blocks/legacy/*.java` | `behavior/legacy.rs` | Removal sweep on `WorldLoadEnd`. |
| `world/blocks/{ControlBlock,RotBlock,UnitTetherBlock,LaunchAnimator,ExplosionShield}.java` | `behavior/helpers.rs` traits | Consumers: 10/11/12. |
| `world/blocks/ItemSelection.java` | `world/config.rs` (data); widget → 14 | Deviation 4. |
| `world/blocks/storage/{StorageBlock,CoreBlock,Unloader}.java` | **08** | Boundary; `overwrote`/`itemTaken` base hooks here. |
| `world/blocks/distribution/*`, `payloads/*` | **08** | Boundary. |
| `world/blocks/{power,liquid,heat}/*` | **09** | Boundary; consumers/module hooks here. |
| `world/blocks/defense/turrets/*`, `ForceProjector`, `MendProjector`, `ShieldWall`, `ShockMine`, `BuildTurret` | **10** | Boundary; `Wall` lightning/deflect via 10 hooks. |
| `world/{Tile,Tiles,Edges,WorldContext,WorldParams,CachedTile,TileGen,ColorMapper}.java` | **06** (19 for editor/color mapper) | Consumed via §2.5 contract. |
| `world/{ItemBuffer,DirectionalItemBuffer}.java` | **08** | Boundary. |
| `world/blocks/Autotiler.java`, `TileBitmask.java` | **03/08** (`TileBitmask` in 03, `Autotiler` trait consumed by 08/16) | 07 uses autotile bitmasks in environment/wall behaviors. |
| `game/EventType.java` build events | `world/events.rs` | `BlockBuildBegin/EndEvent`, `BuildRotateEvent`, `BuildTeamChangeEvent`, `BuildDamageEvent`, `BuildingBulletDestroyEvent` + `ProximityUpdateEvent` (Rust-only). |
| `annotations/entity/EntityIO` building path | `world/building_io.rs` + 04 `EntityCodec` registration | Custom def codec (R3); `BuildingWriter`/`BuildingReader` aliases for 08. |
| `ContentRegions.loadRegions` for blocks | 03 `#[derive(LoadRegions)]` on `BlockRegions` | Region grammar per `world/blocks/AGENTS.md`. |

## 5. Milestones & task breakdown

Each milestone ends with `cargo fmt --check`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, and the named harness command; evidence goes in the Changelog. Order is strict; the smallest vertical slice is M1.

**M0 — Block side table + view (no placement).**
Deliver `Blocks`/`BlockInstance`/`BlockView`, `BlockKindData` enum + all family defs, `BlockInit` consumer/bar/derived-value pass with `blocks_derivation_matches` (02 values asserted, not recomputed), `PATCH_DENIED` list, `BlockInstance` lookup by `BlockId`/name.
*Verify:* `cargo test -p mind-core world::block` (view flags/filters/derivations); `mind-headless blocks audit --json` dumps per-block `kind/building/family/consumers`; counts equal 02's ledger.

**M1 — Building entity + base update (smallest vertical slice).**
`Building`/`Timers`/module components, Building def entry, spawn/remove via 05, `building_update` in `EntitySet::UpdateBuildings`, `update_consumption` port, `edelta`/`delta`/`getProgressIncrease`, `status()`, `sleep/no_sleep`, resource-free `Blocks` lookups. Register a trivial test block (kind `Crafter`) plus `Wall`.
*Verify:* scenario `blocks_spawn_update` (spawn N test buildings headless, 600 ticks, checksum golden); `world::block::tests::building_update_final_order`; `world::consumers::tests::consumption_efficiency_math` (§7a) with hand-computed efficiencies.

**M2 — Placement, construction, destruction.**
`build.rs` validity, `place.rs` begin/quick-rotate/derelict, `construct.rs` + 16 construct singletons, instant paths, `plan.rs`, command handlers, events, limits/darkness/`BlockCounter`, `World.end_map_load` legacy sweep. Family `Wall`, `Floor`, `StaticWall`, `Prop`, `Cliff` behaviors (simplest).
*Verify:* scenarios `blocks_place_construct_destroy`, `blocks_multiblock_cover_clear`; ported `multiblock`, `blockInventories`, `blockOverlapRemoved`, `buildingOverlap`, `buildingDestruction` (§7a); `blocks` MCP step 1–2 preview.

**M3 — Proximity, config, IO.**
`proximity.rs` + `ProximityUpdateEvent`; `config.rs` + `ConfigValue` ↔ 04 `TypeValue`; `building_io.rs` + `BuildingCodec` + revision manifests; module save/load; `ConstructState` write/read; `afterPatch`/`reinitializeConsumers`.
*Verify:* scenarios `blocks_proximity_multiblock`, `blocks_config_roundtrip` (uses 04 `io roundtrip`); `world::building_io::tests::{module_bitmask_roundtrip, revision_per_kind}`, `world::config::tests::config_value_roundtrip`.

**M4 — Draw framework, icons, stats/bars.**
`draw.rs` trait + 9 base drawers, `DrawCommands`, 03 `LoadRegions` wiring per family, `icons()` descriptors, `Stats` container + `BarSpec` evaluation.
*Verify:* `cargo test world::draw` + `world::stats`; `assets regions --assert-complete` for the 07 families (03 command); MCP screenshot shows block + flame + liquid overlays.

**M5 — Family wave A: environment + basic defense.**
Complete environment behaviors and `Wall`/`Door`/`AutoDoor`/`Radar`/`Thruster`/`TargetDummy` (with 10/11 hooks stubbed behind traits).
*Verify:* ported `allBlockTest` restricted to these families; scenarios `blocks_environment_*`, `blocks_wall_door`; per-family golden dumps.

**M6 — Family wave B: production.**
All 11 production classes, RNG consumption for `Separator`, warmup/progress math, output dumping (uses 08 base APIs once present; stub `dump` until then — `dump` base is in 07).
*Verify:* scenario `blocks_consumer_efficiency` with real blocks (`silicon-smelter`, `mechanical-drill`, `water-extractor`/`Pump`, `spore-press`); `ApplicationTests.inventoryDeposit` (07 share: `surge-smelter` path); `Drill` `countOre` parity test.

**M7 — Family wave C: sandbox, campaign generic, legacy, helpers.**
Sandbox blocks full config + IO; `Accelerator`/`LandingPad`/`LaunchPad` building halves with `LaunchTargetProvider` stub; legacy sweep; helper traits.
*Verify:* scenarios `blocks_sandbox_config`, `blocks_campaign_pad`, `blocks_legacy_sweep`; MCP scenario §7c full.

**M8 — Perf, parity, exit.**
`bench blocks`, alloc-audit, `blocks` MCP run recorded, revision/class-id checks, ledger rows complete for all owned families, §7e checklist signed.
*Verify:* §7d numbers recorded; `tools/ci.sh` additions (`blocks` scenarios + benches) green.

## 6. Data & formats

### 6.1 `BlockKindData` (07-defined, embedded in 02's `BlockDef`)

```rust
pub enum BlockKindData {
    Construct { size: u8 },
    Floor(FloorDef), OverlayFloor(OverlayDef), Ore(OreDef), StaticWall(StaticWallDef), Prop(PropDef),
    Cliff(CliffDef), ShallowLiquid(ShallowLiquidDef), SteamVent(SteamVentDef), Spawn(SpawnDef),
    Wall(WallDef), Door(DoorDef), AutoDoor(AutoDoorDef), Radar(RadarDef), Thruster(ThrusterDef), TargetDummy(TargetDummyDef),
    Crafter(CrafterDef), AttributeCrafter(AttributeCrafterDef), Drill(DrillDef), BurstDrill(BurstDrillDef),
    BeamDrill(BeamDrillDef), Pump(PumpDef), SolidPump(SolidPumpDef), Fracker(FrackerDef),
    WallCrafter(WallCrafterDef), Separator(SeparatorDef), Incinerator(IncineratorDef),
    ItemSource(SandboxItemDef), ItemVoid, LiquidSource(SandboxLiquidDef), LiquidVoid,
    PowerSource(SandboxPowerDef), PowerVoid,
    Accelerator(AcceleratorDef), LandingPad(LandingPadDef), LaunchPad(LaunchPadDef),
    Legacy(LegacyDef), /* 08/09/10/11 families are declared here too, owned by their plans */
}
```

Fields mirror Java field names (snake_case) and are constructed by 02's `registries/blocks/*.rs` waves exactly in source order. This enum is the single additive interface change to `BlockDef` (flagged §8 R2).

### 6.2 Module bitmask and building wire order

Constants and byte layout are in §3.10. Revision constants (per `BuildingKind`): `ConstructBuild=1`, `DrillBuild=1`, `SeparatorBuild=1`, `LiquidSourceBuild=1`, `AcceleratorBuild=1`, `LaunchPadBuild=1`; all others `0`. Revision manifests at `client/rust/mind-core/revisions/buildings/<kind>.json`:

```json
{ "kind": "GenericCrafterBuild", "revision": 0,
  "base": "building-base-v4",
  "fields": [ {"name":"progress","type":"f32"}, {"name":"warmup","type":"f32"} ] }
```

### 6.3 `ConfigValue` ↔ TypeIO tags (04)

| Config value | TypeIO tag | Notes |
|---|---|---|
| `None` | `null(0)` | `configClear` |
| `Item/Liquid/Block/Unit/Content` | `content(5)` | network-plan whitelist |
| `Number` | `int(1)`/`long(2)`/`float(3)`/`double(11)` | whitelist |
| `Bool` | `bool(10)` | whitelist |
| `Point2`/`Point2Array` | `point2(7)`/`point2_array(8)` | ties/rotors |
| `Bytes` | `byte_array(14)` | logic config buffers (13) |
| `Building` | `building(12)` | config-copy |

`read_object_safe` is always used for configs (04 caps: `MAX_ARRAY=1000`, `MAX_BYTE_ARRAY=40_000`, safe string 1200).

### 6.4 State dump additions (plan 00 §6.3, append-only)

```json
"buildings": [ { "id": 7, "block": "copper-wall", "x": 5, "y": 4, "team": 0, "rot": 0,
                 "health": 360.0, "enabled": true, "efficiency": 1.0,
                 "config": null,
                 "modules": { "items": {"copper": 3}, "liquids": {"water": 10.5}, "power": {"status": 0.75, "links": 2} } } ]
```

Buildings sorted by `SimId`; `config` uses the type-name form (`"copper"`, `{"x":1,"y":2}`); module sections omitted when the module is absent. `SimHost.get_state_json()` returns this schema (inspector protocol).

### 6.5 Files

- `mind-core/src/world/**` (§3.1); building revision manifests (§6.2).
- Scenarios: `scenarios/blocks_*.json` (canonical) mirrored to `client/scenarios/`; goldens for checksums/dumps.
- `tests/golden/blocks_families.json` (per-family metadata + behavior smoke expectations), `tests/golden/blocks_schedule.txt` (building update order trace).

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (`Mindustry/tests/src/test/java/**` → `cargo test -p mind-core`)

| Mindustry test | Rust test | Notes |
|---|---|---|
| `ApplicationTests.multiblock` (entity half; 06 owns the tile-linkage half) | `world::block::tests::multiblock_building_identities` | 3×3 core: one entity, center tile owns it, proxies alias it; team/rotation on the entity. |
| `ApplicationTests.blockInventories` | `world::modules::tests::item_module_arithmetic` | `add/remove/total` exact; item counts clamped at 0; uses `ItemModule`. |
| `ApplicationTests.blockOverlapRemoved` (building half; 06 owns tile half) | `world::place::tests::multiblock_overlap_replaces_buildings` | placing an overlapping multiblock removes the old buildings and routes `overwrote(prev)`. |
| `ApplicationTests.inventoryDeposit` | `world::place::tests::crafter_accepts_deposit` (07 share) | `surge-smelter` deposit path; `vault` → 08; `thorium-reactor` → 09 (`#[ignore = "plan 08/09"]` until then). |
| `ApplicationTests.buildingOverlap` | `world::construct::tests::builder_overlap_deterministic` | two overlapping plans: first wins, second is dropped; center/tile equality. |
| `ApplicationTests.buildingDestruction` | `world::construct::tests::builder_construct_then_deconstruct` | construct at t→ `build2`; finish → real block; deconstruct from edge → all 9 tiles air. |
| `ApplicationTests.allBlockTest` (update half; metadata half is 02) | `world::behavior::tests::all_blocks_update_without_panic` | every buildable block placed on `stone`, one `update()`, `tile.block == build.block`, `health == block.health`. Excludes families owned by 08/09/10/11 via an explicit allow-list in the test. |
| `PowerTestFixture.{createFakeTile,fakeProducerBlock,...}` | consumed by `09` tests; 07 provides `fixtures::building::{create_fake_tile, fake_block}` | 07 fixture creates a building entity through the real spawn path (06 `WorldHooks`); 09 builds on it. |
| — (new) | `world::consumers::tests::consumption_efficiency_math` | Non-optional min, optional min, disabled/cheat shortcuts, `update_consumers` gating, `edelta` scaling — hand-computed via fake consumers. |
| — (new) | `world::construct::tests::construction_progress_math` | `construct`/`deconstruct` accumulators, refund multiplier, rounding top-up, `items_left`, instant paths. |
| — (new) | `world::place::tests::env_flags_placeability` | `supports_env` truth table; `envDisabled`/`envRequired` semantics; `placeableLiquid`/`requiresWater`. |
| — (new) | `world::proximity::tests::proximity_update_order` | `updateProximity`/`removeFromProximity` equal Java ordering over `Edges`; duplicate neighbor handling; team filter. |
| — (new) | `world::config::tests::config_dispatch_and_clear` | by-kind handler, clear, copy-from-building fallback, absent handler ignored. |
| — (new) | `world::building_io::tests::{module_bitmask_roundtrip, revision_per_kind, visibility_version4, legacy_consume_bool}` | `writeBase`/`readBase` byte parity with the Java layout table. |
| — (new) | `world::block::tests::derivation_matches_def` | `offset == ((size+1)%2)*4`, `size_offset == -((size-1)/2)`, health/build time formulas; 02/07 drift guard. |

### 7b. Headless harness scenarios (`mind-headless`)

| Scenario | World / script | Assertions |
|---|---|---|
| `blocks_place_construct_destroy` | flat 32×32, seed 7; `begin_place(4,4,copper-wall)`; harness advances construct amount until finish; break; 60 ticks | state transitions: construct block (`build2`) → `copper-wall` → air; event order `BlockBuildBegin(begin=true) → Begin(false) → End(false) → Begin(true) → End(true)`; final checksum golden; dump tiles |
| `blocks_multiblock_cover_clear` | flat 32×32; place `copper-wall-large` at (5,5); break from (4,4) | all 9 tiles reference the center entity; breaking any covered tile clears all 9 and fires one End event; overlap with `copper-wall` removes it |
| `blocks_consumer_efficiency` | flat 32×32; place `silicon-smelter`; inject 10 coal/10 sand; run 600 ticks; assert progress/output at fixed ticks | exact `progress` at tick 1/2/…; item counts; efficiency = 1; after fuel removal efficiency = 0 and no further consumption |
| `blocks_config_roundtrip` | place `item-source`, configure `copper`; `io roundtrip` save/reload (04 command), 0 ticks | config and entity survive; checksum equal before/after; config dump shows `"copper"` |
| `blocks_proximity_multiblock` | place 3×3 wall + adjacent door; remove center wall | proximity lengths before/after match `Edges` neighbor counts; `onProximityRemoved` effects (door solidity recompute) asserted |
| `blocks_limits_darkness` | edge-of-map placement, `limitMapArea` on, darkness ≥ 3; banned block via rules | `valid_place` false in each case; `valid_break` false for indestructible blocks |
| `bench blocks_update` | mid profile: 2000 buildings of mixed families, 3600 ticks | p50/p99 JSON per §7d |

Scenario code is Rust-registered (HLP §7.1); it calls the same `Build`/`construct` APIs builders use (11 later drives them through `BuildPlan`). No new JSON command variants are required; `place`/`break`/`configure` command ops (00 §6.2 append-only) are used where applicable.

### 7c. MCP playtest scenario (concrete, open-godot-mcp)

Preconditions: `tools/build.sh`; `godot_health check`; if BRIDGE_NOT_CONNECTED launch the editor per the repo playtest skill; always pid-stamp evals. Node paths are the plan-00 spine (`res://scenes/spine.tscn`); adapt if plan 00 renames them.

1. `godot_health check` → `{ok:true}`.
2. `godot_game play(scene="res://scenes/spine.tscn")`; wait for `[I] content loaded` in `godot_log get`.
3. Pid + baseline: `godot_exec eval` → `{"pid": OS.get_process_id(), "tick": SimHost.get_tick(), "checksum": str(SimHost.get_checksum())}` using `get_node("/root/Spine/SimHost")`; record pid.
4. Place: `godot_exec call /root/Spine/SimHost place_block [4, 4, "copper-wall"]` → `true`; eval `get_tile_json(4,4)` contains `"block":"build2"` (normal placement goes through construction) and `get_buildings()` increased by 1.
5. Construct: `godot_exec eval` → `SimHost.dev_construct(4,4,1.0)` (harness hook exposed for MCP; plan-00-style debug API) then eval `get_tile_json(4,4).block == "copper-wall"`, `"health":360`.
6. Configure a sandbox block: `place_block [6, 4, "item-source"]` → construct → `godot_exec call /root/Spine/SimHost configure_block [6, 4, "copper"]` → `true`; eval `get_tile_json(6,4).config == "copper"`; then `configure_block [6,4, null]` → config null (configClear).
7. Rotation + multiblock: `place_block [8, 6, "copper-wall-large"]`; eval tile (7,5) and (9,7) both report the same `build_id`; `break_block [7,5]` → all 9 tiles become `air`.
8. Destroy path: place `copper-wall` at (4,4) again, construct, then `break_block [4,4]` → `true`; eval tile air and `get_state_json()` has no building with that id.
9. Determinism + persistence: eval `checksum`; `godot_exec call /root/Spine/SimHost dev_save ["mcp_blocks"]`, `dev_reset`, `dev_load ["mcp_blocks"]`; eval checksum equal.
10. Inspector: `godot_runtime_state inspect /root/Spine/Ui/StateInspector` → `Buildings` tab shows the placed block; `godot_screenshot game` saved as evidence (read PNG, non-blank).
11. `godot_log errors` → empty; `godot_game stop`.

`tools/mcp-smoke.sh` gains steps 4–8 as the recurring gate.

### 7d. Performance budget + measurement

| Metric | Budget (dev machine, release) | Measurement |
|---|---|---|
| `UpdateBuildings` incl. `update_consumption` for 2 000 mixed buildings, idle | p50 ≤ 0.6 ms, p99 ≤ 1.5 ms | `mind-headless bench blocks --profile buildings --ticks 3600 --warmup 600 --json` |
| Same with 500 active crafters/drills producing | p99 ≤ 2.0 ms | same, `--profile active` |
| `valid_place` + `begin_place` + proximity for one multiblock, 2 000-building world | p99 ≤ 250 µs | `bench blocks --profile place` |
| `construct`/`deconstruct` advance for 1 000 builds, one tick | ≤ 300 µs | `bench blocks --profile construct` |
| Steady-state allocations during `EntitySet::UpdateBuildings` | 0 after warmup | `mind-core` feature `alloc-audit`; `bench --assert-alloc 0`; scratch `SmallVec`s only |
| Building base IO (`writeBase`+`readBase`, no modules) | ≤ 200 ns/building | criterion `bench/building_io` |
| MCP frame CPU with one configured sandbox block + wall | ≤ 2 ms | `godot_profiler series` in step 10 |
| Scenario `blocks_consumer_efficiency` (600 ticks) | ≤ 400 ms release | wall time in CI |

Budgets are recorded in `bench/baselines.json`; plan 23 owns the CI gate. Determinism guard: `bench blocks --checksum` must equal the scenario golden for the same seed.

### 7e. Exit criteria checklist

- [x] `cargo test -p mind-core` green; no Godot/tokio imports in `world/**` (plan-00 boundary grep).
- [ ] All §7a rows implemented (08/09/10/11 rows explicitly `#[ignore = "..."]` with owners).
- [ ] `blocks_place_construct_destroy`, `blocks_multiblock_cover_clear`, `blocks_consumer_efficiency`, `blocks_config_roundtrip`, `blocks_proximity_multiblock`, `blocks_limits_darkness` pass with committed goldens.
- [ ] `all_blocks_update_without_panic` covers every owned family; allow-list for other plans is explicit and empty for 07 rows.
- [ ] Building codec: module bitmask, visibility version 4, per-kind revisions, legacy consume-bool read all round-trip; `io check-revisions` includes building manifests.
- [ ] Config round-trip (save + `TypeIO`) for item/liquid/block/unit/number/bool/point configs; network whitelist enforced.
- [ ] `DrawBlock` command output verified for all 9 base drawers (unit test) and composited by 16 in the MCP screenshot.
- [ ] `ProximityUpdateEvent` consumed by a test listener; indexer/renderer hooks documented.
- [ ] MCP scenario §7c passes; `godot_log errors` empty; screenshots attached.
- [ ] §7d budgets met; alloc-audit zero in `UpdateBuildings`; `bench/baselines.json` updated.
- [ ] `PATCH_DENIED` fields (`size`, consumers, `buildType`, `itemFilter`/`liquidFilter` lengths) rejected by 20's patch test handshake.
- [x] Every produced file has the GPL header and source-cite comments.
- [ ] 02/04/05/06 reconciliation notes closed or carried in §8 with an owner; plan Changelog updated with evidence paths.

## 8. Risks & open decisions

Each item has the default this plan proceeds with. `NEEDS USER DECISION` items are load-bearing and not covered by `HIGH_LEVEL_PLAN.md`; execution continues on the default unless the user overrides.

| # | Decision / risk | Default being planned against | Needs user? |
|---|---|---|---|
| R1 | **06 seams after reconciliation.** 06 M3 ships a bare-`BuildingComp` fallback for `WorldHooks::new_building` and owns tile-op ordering; 07 must satisfy `WorldHooks` exactly and keep `tile.data` write rules. | §2.5 is the agreed contract; 07 registers `WorldHooks` before 06's M3 tests lose the fallback (either order works). `tile.data` overload covered by a cross-plan test (06 §8 R9). | Orchestrator reconcile; no user decision |
| R2 | **`BlockDef` kind-data ownership (02 §8 R3; 08 §3.2 freeze).** 07 needs `craft_time`, `output_items`, `drill_time`, … somewhere consumable by typed behavior and by 08's `ConveyorDef`/`PayloadBlockDef` etc. | Add `BlockDef.kind_data: BlockKindData` (enum defined in 07, §6.1); 02's registration waves construct it; no re-registration by 07. This is exactly the hook 08 §3.2 asks for. | **NEEDS USER DECISION** (cross-plan API; orchestrator must accept the additive field) |
| R3 | **`Building` is a custom-IO def upstream (`genio=false`)**; 04's `#[derive(EntityIo)]` is per component. | 04 exposes a manual `EntityCodec` registration; 07 registers `BuildingCodec` (base + per-kind extra), surfaced to 08 as `BuildingWriter`/`BuildingReader`. | **NEEDS USER DECISION** only if the user insists on derive-only entity IO (would force components to own `writeBase`, breaking byte layout parity) |
| R4 | Building update dispatch model (08 §3.2 freeze). | `BuildingBehavior` trait per `BlockId`, dispatched per entity in `Groups.build` slot order from `UpdateBuildings`; optional per-`BlockId` scratch grouping for locality. No per-kind Bevy systems (would break Java order). | No (frozen by 08; documented choice) |
| R5 | Dynamic consumer closures (`ConsumePowerDynamic`/`Condition`, `ConsumeItemDynamic`, `ConsumeLiquidsDynamic`) cannot be Rust closures with captures. | Registered `fn` indices (`ConsumeDynFn` table in `Blocks`); payload consumers registered by 08. | No (mod API documented for 20) |
| R6 | 05's minimal `Rules` stub lacks `banned_blocks`, `block_limits`, `place_range_check`, `deconstruct_refund_multiplier`, `allow_environment_deconstruct`, `revealed_blocks`, `derelict_repair`, `cheat`. | 07 appends these fields to the 05 boundary stub; plan 12 takes merge ownership (same pattern as 05 R4). | Orchestrator reconcile |
| R7 | Core-radius checks and placement-limit queries need `Teams`/`TeamData` (12). | `CoreRadiusProvider`/`PlacementLimitProvider` traits with 07 fallbacks (`BlockCounter` resource; `Groups.build` core scan); 12 overrides. | Orchestrator reconcile |
| R8 | Java `Time.millis`-based pitch/rate-limit (`ConstructBlock.shouldPlay/calcPitch`, `sounds`) has no sim-time equivalent. | Port to sim-time counters, seeded deterministically; sounds are view-only and excluded from checksums. | No (documented deviation §2.4) |
| R9 | `ItemModule`/`LiquidModule` flow windows use Java statics and are UI-only (`PlacementFragment`). | Per-module `FlowWindow`; `update_flow()` called only for the selected building by 14/15, exactly like Java. | No |
| R10 | `DrawBlock` output format constrains 16. | Allocation-free `DrawCommands` buffer per frame; 16 consumes and owns z/layers/caches; `recache()` is a request flag polled by 16. | Orchestrator reconcile with 16 |
| R11 | `@NoPatch` enforcement is split across 02/07/20. | 07 exports `PATCH_DENIED` field-name list (size, consumers, filters, kinds, region bindings); 20's `DataPatcher` rejects with a logged error. | Orchestrator reconcile with 20 |
| R12 | Building `sense`/`setProp`/`control` live in 13 but read 07 fields; risk of duplicated switches. | 13 implements the trait over 07 components only; 07 ships no logic accessors. Read/write field list frozen in §3.4 and handed to 13. | Orchestrator reconcile with 13 |
| R13 | Multiblock damaged/index behavior spans 11 (indexer) and 16 (cracks). | 07 fires `BuildDamageEvent`/`health_changed` and exposes `was_damaged`; 11 indexes, 16 draws; no damage logic here beyond `Damage` calls (10). | Orchestrator reconcile |
| R14 | Legacy block removal hook needs a `WorldLoadEnd` event from 06. | 07 registers the sweep listener; if 06 takes a different load-end event name, adapt one line. | Orchestrator reconcile |
| R15 | Mod-registered building kinds (`20`) must not require recompiling `BuildingKind`. | `BuildingKind` is an `IndexMap`-backed `u16` id; 20 registers `Arc<dyn BuildingBehavior>` via `register_behavior`; core kinds get compile-time constants. | No |
| R16 | 08's frozen `Building` shape lists `team`/`health` on the component; 05 owns `TeamComp`/`Health`. | No duplication: 07 exposes `building::team(world, e)` / `health::hp(world, e)` helpers and 08 reads those; documented in §3.4. | Orchestrator reconcile with 08/05 |
| R17 | 09 expects `PowerModule.graph` to be a `PowerGraphId`. | 07 defines `PowerGraphId { slot: u32, generation: u32 }` in `world/modules/power.rs`; 09's `PowerGraphs` arena consumes it (09 §3.2). | No |
| R18 | 11 expects `Build.validPlace`/`ConstructBlock`/`TeamData` building sets and drives construction through `BuilderComp`/`BuildPlan`. | 07 exposes the placement API + `BuildPlan` + `construct` progress; 11's builders call it. `TeamData.get_buildings` is the 12 provider; 07's `BlockCounter` is the fallback until 12 lands. | Orchestrator reconcile with 11/12 |

## 9. References

Read in full for this plan:

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2 architecture, §3 plan rows, §4 template, §6–§9 conventions) and `PRELIMINARY_PLAN.md`.
- Sibling plans (read/reconciled): `00_FOUNDATION_IMPLEMENTATION_PLAN.md`, `02_CONTENT_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`, `05_SIM_CORE_IMPLEMENTATION_PLAN.md`, `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (§2.5, §3.14), `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (§3.2 freeze), `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (§3.4/§3.13), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (§3 unit/blocks boundaries); forward references: `10`, `12`, `13`, `15`, `16`, `17`, `19`, `20`, `21`, `23`.
- Mindustry docs: `world/AGENTS.md`, `world/blocks/AGENTS.md`, `content/AGENTS.md`, `entities/AGENTS.md`, `graphics/AGENTS.md`, plus the repo/core/source-hub `AGENTS.md` files loaded transitively.
- Sources: `world/Block.java`, `world/Build.java`, `world/blocks/ConstructBlock.java`, `world/consumers/**`, `world/modules/**`, `world/meta/**`, `world/draw/**`, `entities/comp/BuildingComp.java`, `entities/comp/{TeamComp,HealthComp,TimerComp,TimedComp}.java`, `entities/units/BuildPlan.java`, `world/blocks/environment/{Floor,OverlayFloor,OreBlock,StaticWall,Prop,Cliff,ShallowLiquid,SteamVent,SpawnBlock}.java`, `world/blocks/defense/{Wall,Door,AutoDoor,Radar,Thruster,TargetDummy}.java`, `world/blocks/production/{GenericCrafter,AttributeCrafter,Drill,BurstDrill,BeamDrill,Pump,SolidPump,Fracker,WallCrafter,Separator,Incinerator}.java`, `world/blocks/sandbox/*`, `world/blocks/campaign/{Accelerator,LandingPad,LaunchPad}.java`, `world/blocks/legacy/*`, `world/blocks/{ControlBlock,RotBlock,UnitTetherBlock,LaunchAnimator,ExplosionShield,ItemSelection,Attributes}.java`, `world/{Tile,Tiles,Edges,WorldContext,WorldParams}.java` (interface skim), `game/EventType.java` (build events).
- Tests: `tests/src/test/java/ApplicationTests.java` (block tests), `tests/src/test/java/power/PowerTestFixture.java` (handshake), `tests/AGENTS.md`.

## Changelog

- 2026-10-01 — Draft v1 written. No implementation started.
- 2026-10-01 — Draft v2: reconciled with the sibling plans that landed after v1 — 06 (tile ops/`WorldHooks`; §2.5 rewritten, R1 demoted), 08 (`BuildingBehavior`/`Building`/`BlockWriter`/`BlockKindData` freeze adopted; §3.4/§3.14), 09 (`PowerGraphId` defined here; consumer/removal ordering), 11 (placement/construct API hand-off). Test paths moved into 07-owned modules to avoid collisions with 06. Open `NEEDS USER DECISION` items are now R2 (`BlockDef.kind_data` additive field) and R3 (manual `BuildingCodec` registration in 04); R4 is frozen by 08.

- 2026-10-02 — **M0 COMPLETE (`lane/07-blocks`, commit `67fef07`).** `world/block.rs` (`BlockInstance`/`BlockView`/`BlockTable`=`Blocks`; `BlockInstance::from_def` runs the `BlockInit` derivation-assert pass), `world/block_kind_data.rs` (`BlockKindData` + `BlockFamily`, all owned variants), `world/behavior/mod.rs` (`BuildingBehavior` trait per 08 §3.2, `BuildingKind` + revisions, `BehaviorRegistry`, `NoopBehavior`), `world/consumers.rs` (`ConsumeInstance` lowering + Java partition arrays + `consPower` last-wins), `world/config.rs` (`ConfigValue`/`ConfigKind`/`ConfigHandlers` + network whitelist), `world/draw.rs` (`DrawBlock`/`DrawCommands`/`DrawSpec`/`DrawDefault`/`SpecDraw`), `world/stats.rs` (`Stats`/`BarDisplay` evaluation), `world/status.rs`. `Block.rotate` is a real field on `BlockInstance` (`kind_rotates`) exposed via `BlockTable::rotate` — this is the predicate seam plan 04's `write_client_plans` takes. Deviation: `BlockKindData` is derived from plan-02 metadata (R2 remains open) and stored on `BlockInstance` rather than `BlockDef`. Oracle: `world::block::tests::{table_covers_registry_and_view_reads,derivation_matches_def,rotate_predicate_wires_plan04_seam,consumers_are_lowered_in_declaration_order}`, `world::consumers`, `world::config`, `world::draw`, `world::stats`, `world::block_kind_data`; `cargo test -p mind-core` 462 lib passed. `mind-headless blocks audit --json` → 447 blocks / 7 families.

- 2026-10-02 — **M1 COMPLETE (`lane/07-blocks`, commit `6d791ae`).** `entities/comp/building.rs` (`Building` state — replaced the plan-05 marker so the archetype key *is* the runtime state — plus `Timers`/`CrafterState`/`DrillState`/`BeamDrillState`/`WallState`/`DoorState`/`RadarState`/`SandboxState`/`PumpState`), `entities/comp/health.rs` (`Health`), `world/modules/mod.rs` (`ItemModule`/`LiquidModule`/`PowerModule`/`FlowWindow`/`PowerGraphId`), `world/update.rs` (`BlockInstance::spawn`, `update_buildings` in `EntitySet::UpdateBuildings`, `update_consumption` line-for-line, `edelta`/`delta`/`get_progress_increase`, sleep/wake), `world/limits.rs` (`BuildRules`/`BlockCounter`). `Sim`'s schedule now registers `update_buildings` guarded on the `BlockTable` resource, so the P0 `spine_place_break` golden is unchanged. Evidence: `world::update::tests::*`, `world::modules::tests::*`, `entities::comp::tests::*`; 468 lib passed.

- 2026-10-02 — **M2 COMPLETE (`lane/07-blocks`, commit `9f8e67d`).** `world/build.rs` (`valid_place`/`valid_place_at`/`valid_break`/`can_replace`/`supports_env`/`contacts_ground`/`contacts_shallows`/`get_enemy_overlap`), `world/construct.rs` (`ConstructState` + construct/deconstruct progress math), `world/plan.rs` (`BuildPlan`), `world/proximity.rs` (`update_proximity`/`remove_from_proximity` + Rust-only `ProximityUpdateEvent`), `world/harness.rs` (`BuildHarness`, the deterministic place/construct/destroy/configure/tick/checksum API the scenarios drive; `HarnessHooks` bridges plan 06's `WorldCtx` to `BlockInstance::spawn`). Ported tests: `harness::tests::{place_construct_destroy_cycle,multiblock_cover_clear,checksum_is_reproducible}`, `proximity::tests::{two_adjacent_walls_link,remove_from_proximity_unlinks_both}`, `build::tests`. 478 lib passed.

- 2026-10-02 — **M3 COMPLETE (`lane/07-blocks`, commit `982c3eb`), M4/M7 PARTIAL (`fd6bef2`).** M3: `world/building_io.rs` byte-layout port of `BuildingComp.writeBase`/`readBase` (`DecodedBase`, module bitmask constants, module wire formats, legacy consume-bool read); `world/config.rs::configure`/`read_config` dispatch; `world/behavior/sandbox.rs` (`ItemSource` config + emission, `ItemSource`/`LiquidSource`/`Power*` state); plan 06's `world::context::Context::read_building` now decodes into `pending_buildings` (the ECS-owning host drains via `take_pending_buildings`) instead of erroring. M4/M7 partial: `DrawBlock` framework + `Stats`/bars landed in M0; `SandboxBehavior` is the first family behavior. Headless: `mind-headless blocks audit|scenario|bench`; scenarios `place_construct_destroy` (`4beb827f802c2f91` after construct, `13d4da57d5151364` after break), `multiblock_cover_clear` (`13d4da57d5151364`), `spawn_update` 600 ticks (`493f1d399875295e`), `config_roundtrip` (`622d670c3478e063`), `proximity_multiblock` (`bd7e29aa34a7e916`) all `pass:true`. **Deferred (with owners):** M4/M5/M6/M7 family behaviors beyond sandbox (environment/defense/production/campaign/legacy are `NoopBehavior`), `LoadRegions`/icon descriptors (03), full `BuildingCodec` registration into plan 04's entity registry, `TypeIO` config whitelist round-trip, revision manifests, and M8 perf/baselines. All are additive on top of the frozen `BuildingBehavior`/`BlockTable` surface. Evidence: `cargo test -p mind-core` **482 lib + 2 + 2 + 1 = 487 passed / 2 ignored**; `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check -p mind-gdext` all clean; `spine_place_break` → `a1a7b96167c9718d` (unchanged).

  **§7c MCP evals (deferred to the orchestrator's single-editor mutex; pid-stamp every eval).** After `godot_health check` → `godot_game play scene:"res://scenes/spine.tscn"` → pid-stamp `godot_exec eval {code: 'return {"pid": OS.get_process_id(), "tick": get_node("/root/Spine/SimHost").get_tick()}'}`, the block/building runtime is exercised headlessly today via `mind-headless blocks scenario <name> --json`; once plan 16's block renderer exists the in-engine equivalents are: (1) `godot_exec call /root/Spine/SimHost place_block [4,4,"copper-wall"]` (MCP hooks live in plan 00's `MindSimHost` and await the building host wiring — the harness is the current oracle); (2) `godot_exec eval {code: 'return get_node("/root/Spine/SimHost").get_tile_json(4,4)'}` expecting `"block":"build1"` then finish-construct; (3) configure `item-source` → `"copper"`, then clear; (4) `copper-wall-large` footprint identity + break-from-edge; (5) `godot_screenshot game` + `godot_log errors`. These are recorded for the merge-time mutex run; no engine code is required from this lane beyond the headless API.
