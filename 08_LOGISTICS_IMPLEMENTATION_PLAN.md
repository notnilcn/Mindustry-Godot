# 08 — LOGISTICS (items, storage, payloads) implementation plan

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | 🟡 **In progress — 2026-10-02 (`lane/08-logistics`): M0 complete; M1 Autotiler/`TileBitmask` + `Conveyor`/`ArmoredConveyor` landed; `Duct`/`StackConveyor` + M2–M8 open.** Executable after `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` M0–M3 (green). R2 (core inventory sharing) and R3 (carried payload entity model) remain `NEEDS USER DECISION` defaults; neither blocks M0. See `## Changelog`. |
| **Phase** | P3 — World & systems |
| **Depends on** | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (Block/Building framework, `BuildingBehavior` dispatch, `Building` base component, `ItemModule`, proximity/`Edges`, placement/config, revisioned building IO, `RotBlock`/`ControlBlock` hooks), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (tile grid/`Edges`), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (schedule slots, groups, pooling, `Time.run`, RNG, checksum), `02_CONTENT_IMPLEMENTATION_PLAN.md` (`BlockDef`/`BlockKind`, `Item`, `PayloadStack`/`PayloadSeq`, `UnitType` payload fields), `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (building chunk reader/writer, `TypeIO` configs), `03_ASSETS_IMPLEMENTATION_PLAN.md` (autotile/bridge/payload region names, resolved by name at M1). |
| **Blocks** | `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (reuses `Autotiler`, `TileBitmask`, `DirectionBridgeBuild.occupied`), `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (`MassDriverBolt`, `PayloadAmmoTurret` consumes `Payload` API, `Puddles` for deconstructor), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (`CargoAI`, `UnitCargoLoader`/`UnitCargoUnloadPoint`, block units/`ControlBlock`, payload carrying by units), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (core inventory sharing, `handleCoreItem`, core capture/unlock), `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (conveyor/bridge placement, config dialogs), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (belt/duct/bridge/payload draw), `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (payload/driver effects). |
| **Sources** | AGENTS: `world/blocks/AGENTS.md`, `world/AGENTS.md`, `content/AGENTS.md`, `type/AGENTS.md`, `entities/AGENTS.md`, `tests/AGENTS.md`. Java read in full or in the item/payload regions: `world/blocks/Autotiler.java`, `world/blocks/TileBitmask.java`; `world/blocks/distribution/{Conveyor,ArmoredConveyor,Duct,StackConveyor,ChainedBuilding,Junction,DuctJunction,Router,DuctRouter,StackRouter,Sorter,OverflowGate,OverflowDuct,ItemBridge,BufferedItemBridge,DirectionBridge,DuctBridge,DirectionLiquidBridge,MassDriver,DirectionalUnloader}.java`; `world/blocks/storage/{StorageBlock,CoreBlock,Unloader}.java`; `world/blocks/payloads/{Payload,PayloadBlock,BuildPayload,UnitPayload,PayloadConveyor,PayloadRouter,PayloadLoader,PayloadUnloader,PayloadMassDriver,PayloadDeconstructor,BlockProducer,Constructor,PayloadSource,PayloadVoid}.java`; `world/ItemBuffer.java`, `world/DirectionalItemBuffer.java`, `world/modules/ItemModule.java`; `entities/comp/BuildingComp.java` (transfer/dump/payload regions), `entities/comp/UnitComp.java` (`remove/add`); `entities/bullet/MassDriverBolt.java`; `content/Blocks.java` (distribution/storage/payload sections); `annotations/.../StructProcess.java` (buffer packing); `tests/src/test/java/ApplicationTests.java` (conveyor tests). Plans: `HIGH_LEVEL_PLAN.md`, `PRELIMINARY_PLAN.md`, `02`, `03`, `04`, `05`. |
| **Extends spine** | (a) `mind-headless` scenarios `logistics_*` + `bench logistics`; (b) `MindSim` inspector APIs: `dev_place_belt`, `dev_configure_building`, `dev_inspect_building(x,y)` (items/rotation/config/linked), `dev_inspect_conveyor(x,y)` (`len`, `ys[]`, `frame`), `dev_inspect_payload(x,y)` (`kind`, content name, pay_vector), `dev_set_floor`, `dev_step_ticks(n)`, `logistics_counts()`; (c) plan-00 state inspector gains a `Logistics` tab (per-block count, belt-lane items, bridge latency, core inventory); (d) fixture builders in `mind-core/tests/fixtures/logistics.rs` (flat map, item source/sink, fake core) reused by 09/11. |

**License**: GPL-3.0 (D6); new files carry the header above; content names, bundle keys and sprite region names are parity ABI.

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Autotiling infrastructure** — `Autotiler` (blend/tiling logic used by belts, ducts, conduits, stack conveyors) and `TileBitmask` (47-slice `[u8; 256]` table + `load`/`load_variants` name resolution). Sim-side blending state only; drawing is 16.
2. **Item transfer lifecycle** — the exact `acceptItem`/`handleItem`/`acceptStack`/`handleStack`/`removeStack`/`getMaximumAccepted`/`canDump`/`canUnload`/`offload`/`dump`/`dumpAccumulate`/`moveForward`/`itemTaken` semantics, implemented per logistics block, allocation-free.
3. **Item transport blocks** — `Conveyor` (conveyor, titanium-conveyor), `ArmoredConveyor` (armored-conveyor), `Duct` (duct, armored-duct), `StackConveyor` (plastanium-conveyor, surge-conveyor; note: there is **no** `PlastaniumConveyor` Java class — plastanium is a `StackConveyor` instance), `Junction`, `DuctJunction`, `Router` (router, distributor), `DuctRouter`, `StackRouter` (surge-router), `Sorter` (sorter, inverted-sorter), `OverflowGate` (overflow-gate, underflow-gate), `OverflowDuct` (overflow-duct, underflow-duct), `ItemBridge` (phase-conveyor), `BufferedItemBridge` (bridge-conveyor), `DirectionBridge`, `DuctBridge` (duct-bridge), `MassDriver` (mass-driver), `Unloader`, `DirectionalUnloader` (duct-unloader).
4. **Time-delayed item queues** — `ItemBuffer` and `DirectionalItemBuffer` (packed `u64` item+time entries, insert/poll/remove semantics, write/read incl. legacy revision-1 shapes). Buffer throughput and latency are tick-deterministic.
5. **Storage** — `StorageBlock` (container, vault, reinforced-container, reinforced-vault) with `linkedCore` forwarding; `CoreBlock` (6 cores) inventory/capacity semantics: `storageCapacity = itemCapacity + Σ adjacent owned storage`, team-wide shared item pool, incineration at capacity, `coreUnloaders`/`coreIncinerates` rules, proximity re-linking; `Unloader` (full comparator/accumulator algorithm).
6. **Payload system** — `Payload` (interface → Rust enum `PayloadRef` + behavior trait), `BuildPayload`, `UnitPayload`, `PayloadBlock` framework (move-in/out, `pushOutput`, carried flag, pick-up/dump), `PayloadConveyor` (payload-conveyor, reinforced-payload-conveyor), `PayloadRouter` (payload-router, reinforced-payload-router), `PayloadLoader` (payload-loader), `PayloadUnloader` (payload-unloader), `PayloadMassDriver` (payload-mass-driver, large-payload-mass-driver), `PayloadDeconstructor` (small-deconstructor, deconstructor), `BlockProducer`, `Constructor` (constructor, large-constructor), `PayloadSource` (payload-source), `PayloadVoid` (payload-void).
7. **Config values for logistics** — sorter/duct-router/stack-router `sortItem` (`Option<ItemId>`), overflow/underflow `invert`, bridge/driver link (`Point2` relative + packed pos + `-1`), payload-router `sorted` content (`Block` or `UnitType`), constructor `recipe`, unloader `sortItem`, core team config (campaign). Encoded through plan 04's `TypeIO`/plan 15's config path.
8. **Save/sync metadata** — per-building revisioned `write`/`read` for every stateful logistics build (§6.3), and `@SyncField` descriptors consumed by plans 04/21 (cardinality: builds are not entity-defs; 07 owns the building IO hook, 08 supplies fields).
9. **View hand-off data** — sim components exposing everything 16/17 draw (belt item positions `xs/ys`, blendbits/scales, duct `progress/recDir/current`, stack `link/cooldown/lastItem`, payload `payVector/payRotation`, driver turret angle/reload, deconstructor progress). Sim never draws; `mind-gdext` reads components after `tick()`.

### 2.2 "Done" means

- Every item-transport, storage and payload vanilla block is placeable, updates, transfers items/payloads and serializes under `mind-core` headless with **no allocations in steady state**.
- The ported upstream fixtures (§7.1) and headless scenarios (§7.2) pass with exact expected distributions; item motion is exactly `speed` tiles/tick for a free lane (§7.2a).
- `cargo test -p mind-core logistics` and `mind-headless bench logistics` meet the §7.4 budget on the dev machine.
- The MCP scenario §7.3 builds a drill→conveyor→core chain in-engine, watches items move and core inventory grow, with screenshot evidence.
- Save round-trip (in-memory + `.msav` via plan 04) preserves belt item positions, bridge buffers, driver state, payloads and core inventories bit-for-bit across a load.
- `mind-core` stays Godot-free/tokio-free; sim paths stay `HashMap`-iteration-free.

### 2.3 Deliberate deviations (each with reason)

| # | Deviation | Reason |
|---|---|---|
| L1 | **No reflection/inner-`Build` classes.** Each Java `XBuild` becomes a Bevy component (`ConveyorBuild`, `DuctBuild`, …) plus a `BuildingBehavior` impl registered per `BlockId` with plan 07. | D1 (no Java), Bevy ECS composition (plan 05). Behavior hooks and update order stay Java-equivalent (§3.3). |
| L2 | **Carried payloads are real ECS entities** holding `PayloadCarried { holder }`, removed from `Groups.num`/`Groups.build` and excluded from tile occupancy (`TilePos::EMPTY`), instead of Java's free-floating heap `Payload` objects. | Single entity lifecycle for save/sync (`04`/`21`); identical observable behavior (`Payload.set/update/dump`). R3. |
| L3 | **Variable-capacity buffers preallocate once** at building creation (`ItemBuffer`/`DirectionalItemBuffer` hold `Vec<u64>` sized from block metadata) and reuse capacity across pooling; no per-tick allocation. | Mirrors Java's constructor `new long[capacity]`; keeps hot path alloc-free (95 §6.1). |
| L4 | **Packed buffer entries keep the Java `@Struct` bit layout** (little-offset-first, declaration order): `BufferItem` = `item:u16` bits 0..16, `time:f32` bits 16..48; `TimeItem` = `data:u16` 0..16, `item:u16` 16..32, `time:f32` 32..64; legacy `BufferItemLegacy` = `item:u8` 0..8, `time:f32` 8..40. | Free upstream-save mapping if OD2 import is ever enabled (plan 04 §2.3.1); zero behavioral difference otherwise. |
| L5 | **Buffer time uses the sim's `f32` `SimClock.time`** (accumulated `+1.0/tick`), not `f64`. | Java compares `Time.time` floats (`>= time + speed || < time`); f32 accumulation preserves the overflow/wraparound branch behavior. |
| L6 | **Update order = plan 05's insertion-stable `Groups.build` slab** (OD-05-A), not Java swap-removal. | HLP §2.4 stable iteration; distributions differ only by tie-break order, which is covered by golden checksums. R5. |
| L7 | **`MassDriverBolt`/payload-carrying bullet behavior is deferred to plan 10.** 08 owns `DriverBulletData`, firing/receiving hooks and calls a `MassDriverPayloadCarrier` trait; 10 implements it on `MassDriverBolt`. Tests use a `TestBolt` double until 10 lands. | HLP row 10 owns bullet behavior; avoids a circular dependency (08 blocks 10). |
| L8 | **`ControlBlock` (player-controlled router branch)** delegates to plan 11's block-unit API; 08 implements the uncontrolled path and the `canControl`/`shouldAutoTarget` metadata. | HLP row 11 owns block units; no logic duplication. |
| L9 | **`Router`/`Duct`/`Junction` "do not update when disabled" exceptions** (`noUpdateDisabled=true`) are honored per block via `BuildingBehavior::always_update_when_disabled()`. | Java sets `noUpdateDisabled` on those classes; dropping it changes belt behavior when logic disables a block. |
| L10 | **`DirectionLiquidBridge` is plan 09**, but it extends `DirectionBridgeBuild`; 08 ships `DirectionBridgeBuild` with `occupied`/`last_link`/`find_link` as the shared base and exposes it to 09. | One source of truth for the direction-bridge occupancy protocol (Java inheritance). |
| L11 | **Effects/audio/minimap colors are data or hooks only** (e.g. `SorterBuild.sort_item` + `minimap_color()` query). Actual playback/draw is 16/17/18. | Sim/view split (D1). |
| L12 | **`PayloadDeconstructor` liquid puddles call a trait hook** implemented by plan 10's `Puddles`; no-op until 10 lands. | `Puddles` is owned by 10 (HLP row 10). |

### 2.4 Owned by other plans (do not implement here)

| Area | Owner |
|---|---|
| `Conduit`, `LiquidRouter`, `LiquidJunction`, `LiquidBridge`, `DirectionLiquidBridge`, `LiquidBlock`, liquid `liquidCapacity` semantics | `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` |
| `MassDriverBolt` body, `PayloadAmmoTurret`, `Damage`, `Puddles`, `Fires`, `Lightning` | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` |
| `CargoAI` (unit logistics), `UnitCargoLoader`, `UnitCargoUnloadPoint`, unit carrying (`UnitType.payloadCapacity` runtime, `allowedInPayloads` gate), block units/`ControlBlock` | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| Core capture/freezing, campaign `sector.info.handleCoreItem`, `Saves`/autosave, `allowCoreUnloaders`/`coreIncinerates` rule ownership, `Universe` imports | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` |
| `Placement.calculateBridges`/`calculateNodes`/conveyor A* line, config dialogs/selection UI, last-config persistence | `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` |
| `BlockRenderer` belt/bridge/payload layers, `DrawBlock`, item sprites on belts, payload rendering, `Drawf.shadow`/`Drawf.construct` | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Payload/driver/deconstruct effects (`Fx`), payload dust/shake | `17_FX_PARTS_IMPLEMENTATION_PLAN.md` |
| `BlockDef` datapatch parser bindings, modded transport blocks, item-count growth for `ItemModule` arrays | `20_MODS_IMPLEMENTATION_PLAN.md` |
| Config/state command relay, desync checksums for logistics state | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Cross-cutting goldens/bench gates | `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` |

---

## 3. Target design

All names below are final for this plan unless marked *reconcile*. `mind-core` stays Godot-free/tokio-free. Sim code uses `Entity` handles, never Java-style heap references; `BlockId`/`ItemId` are plan 02 newtypes.

### 3.1 Module layout

```
client/rust/mind-core/src/
  world/
    item_buffer.rs                 # ItemBuffer, DirectionalItemBuffer, BufferItem/TimeItem/legacy pack/unpack, BufferStore factory
    blocks/
      autotiler.rs                 # Autotiler trait + free fns: build_blending, blends*, facing, looking_at*, sliced
      tile_bitmask.rs              # TILE_BITMASK_VALUES [u8;256], load()/load_variants() -> [RegionName;47] (name-only)
      distribution/
        mod.rs                     # LogisticsPlugin, register(), LogisticsKind enum
        conveyor.rs                # ConveyorBlock, ConveyorBuild, ConveyorBehavior, O(1) add/remove item rings
        armored_conveyor.rs        # ArmoredConveyorBlock (blends/accept overrides)
        duct.rs                    # DuctBlock, DuctBuild, DuctBehavior (progress model)
        stack_conveyor.rs          # StackConveyorBlock, StackConveyorBuild, state machine (Move/Load/Unload)
        chained_building.rs        # ChainedBuilding trait (next())
        junction.rs                # JunctionBlock, JunctionBuild (DirectionalItemBuffer)
        duct_junction.rs           # DuctJunctionBlock, DuctJunctionBuild (4-slot directional progress)
        router.rs                  # RouterBlock, RouterBuild (per-item cycles), ControlBlock metadata
        duct_router.rs             # DuctRouterBlock, DuctRouterBuild (sort target + progress)
        stack_router.rs            # StackRouterBlock, StackRouterBuild (batch unload)
        sorter.rs                  # SorterBlock, SorterBuild (rotation bitfield flip)
        overflow_gate.rs           # OverflowGateBlock, OverflowGateBuild (instantTransfer side choice)
        overflow_duct.rs           # OverflowDuctBlock, OverflowDuctBuild (cdump side choice)
        item_bridge.rs             # ItemBridgeBlock, ItemBridgeBuild (link, incoming, transport counter)
        buffered_item_bridge.rs    # BufferedItemBridgeBuild (ItemBuffer + timers)
        direction_bridge.rs        # DirectionBridgeBlock, DirectionBridgeBuild (occupied[4], last_link) incl. find_link
        duct_bridge.rs             # DuctBridgeBlock, DuctBridgeBuild (speed-paced pulled transfer)
        mass_driver.rs             # MassDriverBlock, MassDriverBuild (DriverState, waiting_shooters), DriverBulletData
        directional_unloader.rs    # DirectionalUnloaderBlock, DirectionalUnloaderBuild
      storage/
        mod.rs
        storage_block.rs           # StorageBlockBlock, StorageBuild (linked_core forwarding)
        core_block.rs              # CoreBlockBlock, CoreBuild (storageCapacity, shared inventory, launch visuals data)
        unloader.rs                # UnloaderBlock, UnloaderBuild (ContainerStat, comparator, unloadAccumulate)
      payloads/
        mod.rs                     # PayloadPlugin, PayloadCarried, PayloadRef, PayloadKind, PayloadOps trait
        payload_block.rs           # PayloadBlockBlock, PayloadBlockBuild<T> generic core (move_in/out, push_output)
        build_payload.rs           # BuildPayload ops (place, destroyed, requirements, io)
        unit_payload.rs            # UnitPayload ops (dump, solidity checks, io)
        payload_conveyor.rs        # PayloadConveyorBlock/Build (moveTime, step, blocked, curStep/fract)
        payload_router.rs          # PayloadRouterBlock/Build (sorted/recDir/match, pickNext)
        payload_loader.rs          # PayloadLoaderBlock/Build (items/liquids/power in; exporting)
        payload_unloader.rs        # PayloadUnloaderBlock/Build (items/liquids/power out)
        payload_mass_driver.rs     # PayloadMassDriverBlock/Build (charge machine, queue)
        payload_deconstructor.rs   # PayloadDeconstructorBlock/Build (accum buffers, progress)
        block_producer.rs          # BlockProducerBlock/Build (dynamic consume, progress, output)
        constructor.rs             # ConstructorBlock/Build (recipe config, filter, size bounds)
        payload_source.rs          # PayloadSourceBlock/Build (config block/unit, command pos)
        payload_void.rs            # PayloadVoidBlock/Build
  content/blocks/                  # plan 02 registry: 08 adds the BlockKind data variants in registries/blocks/distribution|storage|payloads.rs
  fixtures/logistics.rs            # flat map + source/sink/core fixture helpers (test + harness)
```

`LogisticsPlugin` (plan 05 plugin rule) registers: buffers, behaviors, systems, event listeners, dev APIs. One file per Java class mirrors `world/blocks/<family>/` path parity per HLP §6.1.

### 3.2 Contract with plan 07 (freeze before M1)

Plan 07 does not exist yet; 08 needs the following exact surface. The orchestrator must merge these into 07 (or 08 must adapt names at M0 review if 07 lands differently).

Required from 07 (names assumed by this plan):

```rust
// Building base component (plan 05/07)
#[derive(Component)] pub struct Building {
    pub block: BlockId,
    pub team: TeamId,
    pub tile: TilePos,                 // TilePos::EMPTY for carried BuildPayloads
    pub rotation: u8,
    pub enabled: bool,
    pub efficiency: f32,
    pub time_scale: f32,
    pub health: f32,
    pub dead: bool,
    pub items: Option<ItemModule>,     // present when block.has_items
    pub proximity: Proximity,          // ordered by Edges::get_edges(block.size) (Java updateProximity)
    pub cdump: u8,                     // dump rotation cursor
    pub dump_accum: f32,               // dump accumulator
}
impl Building {
    pub fn front(&self) -> Option<Entity>; pub fn back(&self) -> Option<Entity>;
    pub fn left(&self) -> Option<Entity>;  pub fn right(&self) -> Option<Entity>;
    pub fn nearby(&self, dir: u8) -> Option<Entity>;
    pub fn relative_to(&self, other: Entity) -> i8;        // -1 if not adjacent
    pub fn relative_to_edge(&self, other: Entity) -> i8;
    pub fn edge_facing(&self, source: Entity) -> Option<Entity>;
    pub fn rotdeg(&self) -> f32;
    pub fn delta(&self) -> f32; pub fn edelta(&self) -> f32;
    pub fn no_sleep(&mut self); pub fn sleep(&mut self);
    pub fn update_proximity(&mut self, world: &mut World);
    pub fn take_payload(&mut self) -> Option<PayloadRef>;  // default None
    pub fn dump(&mut self, item: Option<ItemId>) -> bool;  // Java dump()/dump(Item)
    pub fn dump_accumulate(&mut self, item: Option<ItemId>) -> bool;
    pub fn offload(&mut self, item: ItemId);
    pub fn move_forward(&mut self, item: ItemId) -> bool;
    pub fn can_dump(&self, to: Entity, item: ItemId) -> bool; // default true
}
// Behavior registration (07 owns dispatch in EntitySet::UpdateBuildings, in Groups.build slot order)
pub trait BuildingBehavior: Send + Sync {
    fn update_tile(&self, world: &mut World, e: Entity);
    fn always_update_when_disabled(&self) -> bool { false }
    fn on_proximity_update(&self, world: &mut World, e: Entity) {}
    fn created(&self, world: &mut World, e: Entity) {}
    fn placed(&self, world: &mut World, e: Entity) {}
    fn dropped(&self, world: &mut World, e: Entity) {}
    fn on_removed(&self, world: &mut World, e: Entity) {}
    fn overwrote(&self, world: &mut World, e: Entity, previous: &[Entity]) {}
    fn config(&self, world: &World, e: Entity) -> ConfigValue;
    fn configured(&self, world: &mut World, e: Entity, player: Option<Entity>, value: ConfigValue);
    fn sense(&self, world: &World, e: Entity, sensor: LAccess) -> f64 { /* default */ 0.0 }
    fn version(&self, world: &World, e: Entity) -> u8 { 0 }
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter);
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8);
    fn get_tiling(&self, world: &World, e: Entity) -> Option<[i32; 5]> { None } // plan-16 hint only
}
pub fn register_behavior(block: BlockId, behavior: Arc<dyn BuildingBehavior>);
```

Required from 02/07 content metadata (registered with `BlockDef`): `has_items`, `item_capacity`, `outputs_items`, `accepts_items`, `accepts_payload`, `outputs_payload`, `payload_capacity(size)`, `rotate`, `size`, `under_bullets`, `unloadable`, `separate_item_capacity`, `consumes_item`, `instant_transfer`, `no_side_blend`, `is_duct`, `solid`, `no_update_disabled`, `conveyor_placement`, plus 08's typed kind data (`ConveyorDef { speed, displayed_speed, push_units, junction_replacement, bridge_replacement }`, `DuctDef { speed, armored }`, `StackConveyorDef { speed, base_efficiency, output_router, recharge, item_capacity }`, `RouterDef { speed }`, `JunctionDef { speed, capacity }`, `ItemBridgeDef { range, transport_time, link_same_type, ... }`, `DirectionBridgeDef { range }`, `MassDriverDef { range, rotate_speed, translation, min_distribute, reload, bullet_speed, bullet_lifetime, ... }`, `UnloaderDef { speed, allow_core_unload }`, `PayloadBlockDef { payload_speed, payload_rotate_speed, max_payload_size, ... }`, etc.). Registered by plan 02's loader through a `BlockKindData` enum hook.

What 08 provides to others:

- `Autotiler` free functions + `TileBitmask` (03 loads names; 16 draws; 09 reuses for conduits).
- `ChainedBuilding` trait (`fn next(&self) -> Option<Entity>`).
- `DirectionBridgeBuild` (`occupied: [Option<Entity>; 4]`, `last_link: Option<Entity>`, `find_link`) shared with 09.
- `PayloadRef`/`PayloadOps`/`PayloadCarried` API for 10 (`PayloadAmmoTurret`) and 11 (`CargoAI`, carrying units).
- `CoreBuild::storage_capacity()`/inventory access for 12/14.
- `ItemBuffer`/`DirectionalItemBuffer` for any mod block (20) and buffered variants.

### 3.3 Item transfer lifecycle (exact semantics)

08 implements these hooks per behavior; names map 1:1 to `BuildingComp`/block overrides:

| Hook | Java source | Rust behavior impl notes |
|---|---|---|
| `accept_item(src, item) -> bool` | `BuildingComp.java:869`, per-block overrides | Read-only; must not mutate (Java contract). Checks capacity + source direction + `!source.rotate && next == source` exclusions. |
| `handle_item(src, item)` | `BuildingComp.java:865` | Moves exactly 1 item into `items` and side-state (belt array insert at `0`/`mid`, duct `current`, etc.). |
| `accept_stack(item, amount, src) -> i32` | `BuildingComp.java:779` | Default: `accept_item(self, item)` + `min(max - items.get(item), amount)`; conveyors override with the spacing-limited `(minitem / itemSpace)`. |
| `handle_stack(item, amount, src)` | `BuildingComp.java:801` | Belt override inserts `amount` items front-to-back with `ys = i*itemSpace`. |
| `remove_stack(item, amount) -> i32` | `BuildingComp.java:792` | Belt override scans `ids[]`, `remove(i)` shifts down; unloaders use it. |
| `get_maximum_accepted(item) -> i32` | `BuildingComp.java:787` | `itemCapacity` default; `BlockProducer` recipe×2; core `storageCapacity`; `StorageBuild` forwards to linked core. |
| `can_unload() -> bool` | `BuildingComp.java:727` | `block.unloadable`; `StackConveyorBuild` state != Load; `CoreBuild` rule-gated; `StorageBuild` forwards. |
| `can_dump(to, item) -> bool` | `BuildingComp.java:1129` | Bridge `checkDump` directional checks. |
| `offload(item)` | `BuildingComp.java:1006` | `produced()` then dump-first-nearby else self `handle_item`. |
| `dump(item)` / `dump_accumulate(item)` | `BuildingComp.java:1053-1119` | Rotation-cursor scan of `proximity` (`cdump`), increments per candidate; `dump_accum` accumulator. |
| `move_forward(item)` | `BuildingComp.java:1134` | Front-only transfer used by ducts/stack conveyors/unloaders. |
| `item_taken(item)` | `BuildingComp.java:740` | Core/StorageBuild campaign/hook notification. |
| `produced(item, amount)` | `BuildingComp.java:1040` | Campaign production stat hook (12). |
| `handle_payload(src, payload)` / `accept_payload` | `BuildingComp.java:811-815` | Payload variants only (§3.8). |

Invariants:

1. **Hot path allocation-free**: belt arrays are fixed-size inside the component; no `Vec` growth in `update_tile`/`handle_*`.
2. **Exactly-once move**: a successful `handle_item` implies the source removed its copy in the same tick (Java call sites do removal immediately after). Ports keep the call site order identical (`accept → handle → remove`).
3. **No `unwrap` on runtime data**: missing `items` module is a debug assert tied to `block.has_items`.
4. **Transfer order within a building** is front-to-back for belts (`i = len-1..0`), matching Java; external order is `Groups.build` slot order (L6).
5. **`itemCapacity == 0` blocks** (junction, unloaders, sorter/gate) never allocate `items` but still participate; `accept_stack` returns 0 where Java does.
6. **`instant_transfer`** blocks (sorter, gates) may transfer in `accept_item`/`handle_item` via nearby (`SorterBuild.getTileTarget`), never via update.

### 3.4 Item movement: components & systems

**Conveyor** (`ConveyorBuild`, capacity 3, `itemSpace = 0.4`):

```rust
#[derive(Component, Default)]
pub struct ConveyorBuild {
    pub ids: [ItemId; CAPACITY],       // CAPACITY = 3
    pub xs:  [f32; CAPACITY],
    pub ys:  [f32; CAPACITY],
    pub len: u8,
    pub next: Option<Entity>,          // cached from proximity
    pub nextc: Option<Entity>,         // next if ConveyorBuild + same team
    pub aligned: bool,
    pub last_inserted: u8, pub mid: u8, pub minitem: f32,
    pub clog_heat: f32,
    pub blend_bits: u8, pub blending: u8, pub blend_sclx: i8, pub blend_scly: i8,
    pub frame_cache: u8,               // view-only hint (16 computes/paints)
}
```

Update (`updateTile` port, `Conveyor.java:254`): `minitem = 1; mid = 0`; early-sleep if `len == 0 && timeScale == 1`; `nextMax = aligned ? 1 - max(itemSpace - nextc.minitem, 0) : 1`; iterate `i = len-1..0`: `nextpos = (i == len-1 ? 100 : ys[i+1]) - itemSpace`; clamp move by `speed * edelta()`; clamp `ys <= nextMax`; update `mid`; `xs = approach(xs, 0, moved*2)`; on `ys >= 1 && pass(ids[i])`: if aligned copy `xs[i]` into `nextc.xs[nextc.last_inserted]`, then `items.remove(ids[i], len-i); len = min(i, len)`; else track `minitem`. Clog heat `approach_delta` toward 1 when `minitem < itemSpace + (blendbits == 1 ? 0.3 : 0)`. `pass` = `next.accept_item(this, item) → next.handle_item(this, item)`.

Movement numbers (perf/oracle): a free lane advances exactly `speed` tiles/tick — base `0.035`, titanium `0.0801`, armored `0.08` — so 1 tile per `ceil(1/speed)` ticks (29 ticks base, 13 ticks titanium/armored). Full-load throughput = `speed / itemSpace` items/tick (base 0.0875 = 5.25/s; titanium 0.20025 = 12.0/s; matches `displayedSpeed` 5/10 within game convention).

**Duct** (`DuctBuild`, `Duct.java:131`): `progress += edelta/speed*2`; with `current != null && next != null`, at `progress >= 1 - 1/speed` call `move_forward(current)` → `items.remove(current,1)`, `progress %= 1-1/speed`; else `progress = 0`; pick `current = items.first()` when empty; acceptance rejects front and requires the facing edge direction `!= 2` relative; `handle_item` sets `progress = -1` and `recDir`. `armored` variants use `blendsArmored` acceptance (side entry only). Duct `speed = 4` for Erekir ducts (progress rate `2/speed` per tick at full efficiency; stat `60/speed` items/s).

**StackConveyor** (`StackConveyorBuild`, plastanium `speed = 4/60`, capacity 10, `recharge = 2`): state machine `state ∈ {Move, Load, Unload}` recomputed in `on_proximity_update` from `build_blending` + neighbor states (exact conditions at `StackConveyor.java:226-248`, including the mutual `proxUpdating` recursion guard); `update_tile` reels `cooldown` down (`clamp(cooldown - speed*eff*delta, 0, recharge)`), transfers `items` + `last_item` + `link = tile.pos()` to a front `StackConveyorBuild` with `link == -1`, and batch-unloads (`dump` or `move_forward`) while enabled. `link` is a packed tile pos or `-1`; `dropped()` re-links backward. `surge-conveyor` adds power efficiency (`baseEfficiency = 1`).

**Sleep/pooling**: conveyor/duct sleep when idle and wake on `handle_*` (`no_sleep`); routers/junctions/bridges/mass drivers are `noUpdateDisabled` and never sleep (Java `noUpdateDisabled = true`), so their systems run whenever `state.is_game() && !paused`. `sleeping` membership is owned by 05/07; 08 only calls `sleep`/`no_sleep`.

### 3.5 Time-delayed queues

```rust
// world/item_buffer.rs — exact Java semantics
pub struct ItemBuffer { buf: Vec<u64>, index: usize }                 // Java ItemBuffer(capacity)
pub struct DirectionalItemBuffer { bufs: [Vec<u64>; 4], indexes: [u8; 4] } // Java DirectionalItemBuffer(capacity)

pub mod packed {
    pub fn buffer_item(item: ItemId, time: f32) -> u64;               // item | time<<16 (L4)
    pub fn buffer_item_item(v: u64) -> ItemId; pub fn buffer_item_time(v: u64) -> f32;
    pub fn time_item(data: i16, item: ItemId, time: f32) -> u64;      // data | item<<16 | time<<32
    pub fn time_item_data(v: u64) -> i16; pub fn time_item_item(v: u64) -> ItemId; pub fn time_item_time(v: u64) -> f32;
    pub fn legacy_buffer_item(v: u64) -> u64;                          // 8-bit item → 16-bit, keep time
}
impl ItemBuffer {
    pub fn accepts(&self) -> bool; pub fn accept(&mut self, item: ItemId, data: i16, now: f32);
    pub fn poll(&self, speed: f32, now: f32) -> Option<ItemId>; pub fn remove(&mut self);
    pub fn write(&self, w: &mut BuildingWriter); pub fn read(&mut self, r: &mut BuildingReader);
}
impl DirectionalItemBuffer {
    pub fn accepts(&self, dir: u8) -> bool; pub fn accept(&mut self, dir: u8, item: ItemId, now: f32);
    pub fn poll(&self, dir: u8, speed: f32, now: f32) -> Option<ItemId>; pub fn remove(&mut self, dir: u8);
    pub fn write(&self, w: &mut BuildingWriter); pub fn read(&mut self, r: &mut BuildingReader, legacy: bool);
}
```

- `poll` uses `now >= time + speed || now < time` (wraparound branch, L5).
- Consumers: `JunctionBuild.buffer` (capacity 6, `speed = capacity`... junction uses `float speed = 26` frames), `BufferedItemBridgeBuild.buffer` (`bufferCapacity`, `speed 74`), `StackRouter`/legacy-read-only paths for `Sorter` rev 1 and `OverflowGate` rev 1 (data patches must keep reading them). `TimeItem` is used by `ItemBuffer` for `OverflowGate` pre-v4 compatibility only (legacy read); runtime uses `BufferItem`.
- Buffers are created in `created()` from block metadata (L3); pooled buildings keep their `Vec` capacity (component `reset` clears `index`).

`ItemModule` (07) must expose the Java ordering helpers exactly: `first()` scans index 0..; `take()` uses a serialized-not `take_rotation` (runtime-only) so `BufferedItemBridge`/`DuctBridge` item selection order matches upstream; `add/remove` do not clamp except `remove` (clamps to available).

### 3.6 Routers, sorters, junctions, gates, bridges, drivers

- **`Router`** (`Router.java:57`): `last_item: Option<ItemId>`, `last_input: Option<TilePos>`, `time: f32`, `cycles: [u8; item_count]`; update: `time += 1/speed*delta`; target = `get_tile_target(last_item, last_input, false)`; fire when `time >= 1 || target.block.instantTransfer`; on fire call with `set=true` (increments the per-item cycle) then `target.handle_item`, `items.remove`, clear. `get_tile_target` skips `from` when `from.block == overflow-gate`; skips the `ControlBlock` controlled branch until 11 (L8). `distributor` uses the same class with `size = 2`.
- **`DuctRouter`** (`DuctRouter.java:101`): `current` + `progress += edelta/speed*2`; `target()` scans `proximity` from `cdump`, excluding the back relative (`(rotation+2)%4`), respecting `sortItem` match against `rel == rotation`, then `increment_dump`; `accept_item` requires facing edge == rotation.
- **`StackRouter`**: batch-unloads all of `current` when full (`items.total() >= itemCapacity`), using the same target scan.
- **`Sorter`** (`Sorter.java:111`): `sort_item: Option<ItemId>`, `invert: bool`, rotation bitfield is the persistent `rotation` byte (4 direction bits + XOR flip on branch choice, `rotation ^= 1 << dir`); `get_tile_target` returns `nearby(dir)` for matches and side/`null` for the 3-chain guard; `handle_item` may find `None` (Java would NPE only on corrupted state; port returns early + debug log).
- **`OverflowGate`/`OverflowDuct`**: side-choice with `cdump` alternation (`OverflowGate.java:51`, `OverflowDuct.java:98`); gate is `instant_transfer` (`update = false`) and only acts through `accept_item`/`handle_item`; duct has progress/`current` like `Duct`.
- **`Junction`** (`Junction.java:45`): `DirectionalItemBuffer(6)`; per-direction `Time.time >= time + speed/timeScale || <` gate, then `dest = nearby(i)`, require `dest.team == team && dest.accept_item`, shift the direction queue down. Does **not** check `dest.can_dump` (upstream). `DuctJunction` uses 4 progress slots instead of the buffer.
- **`ItemBridge`** (`ItemBridge.java:210`): `link: i32` packed pos or `-1`; `incoming: SmallVec<[i32; 4]>`; `warmup`, `transportCounter`, `wasMoved/moved` (30-tick move pulse), `hadValidLink`. Link validity is the exact `positionsValid` cartesian-axis + range + `linkSameType` + team + `other.build.link != tile.pos()` double-check; `checkIncoming` prunes stale suppliers. `updateTransport` loops `while transportCounter >= transportTime` taking `items.take()` and requeueing on refusal (`items.undo_flow`). `BufferedItemBridgeBuild` overrides transport with `ItemBuffer(bufferCapacity)` + `timer(timerAccept, 4/timeScale)`.
- **`DirectionBridge`** (`DirectionBridge.java:208`): `occupied: [Option<Entity>; 4]`, `last_link`, `find_link()` scans exactly `range` tiles ahead; consumers mark `link.occupied[rotation] = self` each tick and clear stale entries; 09's liquid bridge reuses this component (L10).
- **`DuctBridge`** (`DuctBridge.java:26`): pulled transfer paced by `progress += edelta`, `while progress > speed { take(); if target has room handle_item; progress -= speed }`; fallback `move_forward` when unlinked.
- **`MassDriver`** (`MassDriver.java:121`): `DriverState {Idle, Accepting, Shooting}`; `waitingShooters: SmallVec<[Entity; 4]>` ordered (insertion); `reloadCounter`; `rotation` (RotBlock); accept only when `items.total() < itemCapacity && linkValid`; shooting requires `items.total() >= minDistribute` and target space; on fire builds `DriverBulletData { from, to, items: Vec<i32> (alloc from pool) }`, removes items, creates the bolt via the `MassDriverPayloadCarrier` trait (L7), and schedules `SimClock::run(timeToArrive, || { other.waiting_shooters.remove(this); other.state = Idle; })`. Receiving is `handle_payload(bullet, data)` → add up to `itemCapacity*2` total, effects/sound hooks (17/18), `reloadCounter = 1`. `DriverBulletData` is pooled in plan 05's `VecPool`/`Pools` to avoid per-shot allocation.
- **Legacy reads**: `Sorter` revision 1 read consumes a `DirectionalItemBuffer(20)` blob; `OverflowGate` revisions 1/3 consume legacy buffers/ints — keep readers for plan 04's revision dispatch (native `MGRS` writes current revisions only).

### 3.7 Storage & core inventory

- **`StorageBuild`** (`StorageBlock.java:50`): `linked_core: Option<Entity>`; if linked, `accept_item`/`handle_item`/`can_unload`/`get_maximum_accepted`/`item_taken`/`draw_select`/`sense(itemCapacity)` forward to the core; over-capacity deposits trigger `incinerate_effect` (view hook, `wasVisible` guarded); `remove_stack` fires `sector.info.handle_core_item(item, -result)` when `team == default_team && is_campaign` (12 trait).
- **`CoreBuild`** (`CoreBlock.java:258`): `storage_capacity: i32`, `iframes`, `thruster_time`, `command_pos`. `on_proximity_update` (exact algorithm `CoreBlock.java:754`): team core inventory unification, `storageCapacity = itemCapacity + Σ owned proximity itemCapacity`, owned `StorageBuild`s get `items`/`linked_core` assigned, other team cores' capacities updated, item clamp when not generating. `accept_item` = `rules.coreIncinerates || items.get(item) < storageCapacity`; `get_maximum_accepted` = `i32::MAX/2` when incinerating else `storageCapacity`; `handle_item` increments campaign stats + `handleCoreItem`; `handle_stack` clamps to capacity and incinerates overflow.
- **Inventory sharing model (R2, NEEDS USER DECISION)**: default implementation is a `TeamInventory` resource (`IndexMap<TeamId, ItemModule>` inside plan 12's `Teams`, accessor `&mut ItemModule` per team) that every `CoreBuild` on a team aliases logically; `on_proximity_update` makes each core read/write that module, and `StorageBuild.linked_core` routes through the owning core's accessor. This replaces Java's `this.items = other.items` pointer alias in an ECS-safe way. Campaign ownership stays 12.
- **`CoreBuild` launch/capture visuals** are data + hooks (`LaunchAnimator` from 07): `thruster_time`, `land_duration`, `thrusters`; `playerSpawn`/`onControlSelect`/`requestSpawn`/`unitType` spawning are 11/21 remotes; `changeTeam`/`canBreak`/`canPlaceOn`/`placeBegan`/`beforePlaceBegan` (core upgrade item transfer with `nextItems`) are implemented here because they mutate inventory state; `onDestroyed`/`afterDestroyed` capture behavior is 12.
- **`Unloader`** (`Unloader.java:92`): full port — `possible_blocks: Vec<ContainerStat>` rebuilt in `on_proximity_update` (alloc only there, pooled), `isPossibleItem`, `rotations` cursor, comparator (core-priority, canUnload-only, canLoad-only, loadFactor, lastUsed), `unload_accumulate` loop honoring `unloadTimer >= speed`, `canLoad`/`canUnload` caching with `interactable(team)` checks. `sort_item` config. `speed = 60/11` for vanilla unloader.
- **`DirectionalUnloader`** (`DirectionalUnloader.java:87`): `unload_timer += edelta`, front/back checks, `allow_core_unload` gate against `CoreBuild`/linked `StorageBuild`, item scan from `(i + offset) % item_count` when no `unload_item`, else single-item; `offset = item.id + 1` after each pick.

### 3.8 Payload system

```rust
#[derive(Clone, Copy, PartialEq)] pub enum PayloadKind { Build = 0, Unit = 1 }
#[derive(Clone, Copy)] pub struct PayloadRef { pub kind: PayloadKind, pub entity: Entity }
#[derive(Component)] pub struct PayloadCarried { pub holder: Entity }   // on the carried building/unit
#[derive(Component)] pub struct PayloadHolder {
    pub payload: Option<PayloadRef>,
    pub pay_vector: Vec2, pub pay_rotation: f32,
    pub carried: bool,                  // pickedUp()/drawTeamTop() hint (16)
}
pub trait PayloadOps {                  // implemented for carried Building/Unit entities (L2)
    fn payload_set(&mut self, world: &mut World, e: Entity, x: f32, y: f32, rotation: f32);
    fn payload_size(world: &World, e: Entity) -> f32;                 // block.size*tilesize | unit.hit_size
    fn payload_requirements(world: &World, e: Entity) -> &[ItemStack];
    fn payload_build_time(world: &World, e: Entity) -> f32;
    fn payload_update(world: &mut World, e: Entity, unit_holder: Option<Entity>, building_holder: Option<Entity>);
    fn payload_dump(world: &mut World, e: Entity) -> bool;            // BuildPayload::place / UnitPayload::dump
    fn payload_destroyed(world: &mut World, e: Entity);
    fn payload_write(world: &World, e: Entity, w: &mut BuildingWriter);
}
```

Semantics:

- `handle_unit_payload(unit, grab)`: fire `Fx.spawn` (17), clear player unit if player, `Groups.unit.remove(unit)`, allocate a fresh `EntityId` (`EntityGroup.nextId` / plan 05 `EntityIds::next_id`) via `post` semantics, add `PayloadCarried { holder }`, `grab(UnitPayload)`. BuildPayload creation is `block.newBuilding().create(block, team)` with `tile = EMPTY_TILE`.
- `take_payload`, `accept_payload(source, payload)`, `handle_payload` are per-block behavior; `PayloadBlockBuild` stores holder payload + vector + rotation and calls `update_payload` (Java exact clamp `±size*tilesize/2`).
- `move_in_payload(rotate)` / `move_out_payload()`: `pay_vector.approach`, rotation `move_toward`; on arrival `move_payload` (front accept) else dump (`payload.dump()`, unit solidity/overlap checks) else `push_output` (unit pushing via `Groups.unit.intersect` — plan 05 spatial query).
- **`PayloadConveyor`** (`PayloadConveyor.java:76`): `moveTime = 45`, `progress = time % moveTime`, `cur_step = time / moveTime`, `step`/`stepAccepted` rollover skip, `blocked`/`next` computed in `on_proximity_update` with same-size/different-size alignment rules; `accept_payload` accepts self (`source == self`) or `progress <= 5`; payload moves with the `fract()` interpolation (view only; sim uses `progress`/`animation`).
- **`PayloadRouter`**: `sorted: Option<UnlockableContentRef>`, `rec_dir`, `matches`, `control_time`; `pick_next` rotates through directions, forcing one `next.update_tile()` to resolve the round-robin deterministically; logic `control(config)` sets `control_time = 360` and snap-back; `configured` sets `sorted` and re-checks.
- **`PayloadLoader`/`PayloadUnloader`**: item/liquid/power transfer between the block modules and the carried payload's modules (`payload.build.items/liquids/power`); power paths use plan 09's `PowerModule` (guarded by `has_battery` = payload block `cons_power.buffered`); `exporting` flag + `should_export`; timers (`timerLoad`, `loadTime/efficiency`); 09 owns power capacity fields; the power transfer math is implemented here per `PayloadLoader.java:199-215` against 09's module API.
- **`PayloadMassDriver`**: charge machine (`charge`, `charging`, `chargeTime`), queue `waiting_shooters` (VecDeque), `loaded/pay_length/pay_vector` hand-off to the linked driver, `transfer_effect` data (17); receiving handled by `handle_payload` + `effect_delay_timer`.
- **`PayloadDeconstructor`**: `accum: Vec<f32>` sized to requirements (allocated only on accept), `progress`, `deconstruct_speed`, dump rate; liquid dump calls the `PuddlesHook` trait (10, L12); item output via `items.add` clamped, then dump.
- **`BlockProducer`/`Constructor`**: dynamic item consumption (`ConsumeItemDynamic` equivalent, 07), `progress += buildSpeed*edelta`, on complete `consume()`, create `BuildPayload(recipe, team)`, `progress %= 1`; `Constructor` config `recipe` (size/filter/banned checks), `getMaximumAccepted = requirements × 2`; `PayloadSource` config block/unit and `commandPos` hand-off to 11.
- **Units as payloads** (11 cross-ref): carried units are excluded from `Groups.unit`/physics/AI; `payload_update` skips update when `!unit_type.update_in_units` etc. (exact Java gate `BuildPayload.update`); dumping re-adds to `Groups.unit` + fires `UnitUnloadEvent` + `Units.notify_unit_spawn`.

### 3.9 Schedule, ordering & boundaries

- `LogisticsPlugin` registers `BuildingBehavior`s; plan 07's `EntitySet::UpdateBuildings` dispatches in `Groups.build` slot order, honoring `always_update_when_disabled`. No new top-level `TickSet`.
- Proximity changes (`update_proximity`) happen on placement/removal via 07/06; 08 hooks `on_proximity_update` to recompute cached `next`/`nextc`/`blendbits`/bridge links (`recache` in Java is view-only, skipped in headless).
- `SimClock::run` is used only by `MassDriver` delayed arrival (L7); the callback list is deterministic (05 §3.8).
- Events fired by 08: `UnitUnloadEvent` (payload dump, 11 consumes), `UnitCreateEvent` (PayloadSource, 11), `BlockBuildEndEvent` (core upgrade path; 07/12). No new event types.
- View: all drawing data lives in components; `mind-gdext` reads after `Sim::tick()` (16/17). No `Vars.ui`-style calls in `mind-core`.
- STDB: **no new tables/reducers/views**. Logistics config changes travel as plan 05 `SimCommand::Configure` (relayed by 21) and as `TypeIO`-encoded configs in saves (04). Building state sync (21) uses 04's field metadata + 07's per-building IO.

---

## 4. Port map

Source paths are relative to `Mindustry/core/src/mindustry/` unless stated.

| Mindustry source | Rust target | Notes |
|---|---|---|
| `world/blocks/Autotiler.java` | `world/blocks/autotiler.rs` | `build_blending`, `transform_case`, `blends` (world + plan variants), `blends_armored`, `not_looking_at`, `looking_at_either`, `looking_at`, `facing`, `sliced/top_half/bot_half` (name/slice description only). Returns `[i32; 5]`; no drawing. |
| `world/blocks/TileBitmask.java` | `world/blocks/tile_bitmask.rs` | `const TILE_BITMASK_VALUES: [u8; 256]` literal; `load(name)`/`load_variants(name, n)` → `[RegionName; 47]` / `[[RegionName; 47]; n]` (plan 03 resolves). Consumed by 16. |
| `world/ItemBuffer.java` | `world/item_buffer.rs` (`ItemBuffer`, packed `TimeItem`+`BufferItem`) | Capacity preallocated (L3); `time` f32 (L5); exact `accepts/accept/poll/remove/write/read`. |
| `world/DirectionalItemBuffer.java` | `world/item_buffer.rs` (`DirectionalItemBuffer`, `legacy` read) | 4 queues, `indexes: [u8;4]`; legacy 1-byte item unpack (L4). |
| `world/modules/ItemModule.java` | plan 07 module; 08 defines the ordering contract | 08 requires Java-ordering `first`/`take`; `write` nonzero `(id:i16, amount:i32)` list, `read` legacy `u8` vs `i16`. |
| `world/blocks/distribution/Conveyor.java` | `distribution/conveyor.rs` | Fixed `[3]` arrays; `add/remove` shifts; `accept_item`/`handle_item` exact direction rules; `overwrote` copies belt state; `sense(progress/firstItem)`; rev 0/1 read. |
| `world/blocks/distribution/ArmoredConveyor.java` | `distribution/armored_conveyor.rs` | `no_side_blend`; armored blend + armored `accept_item` (source is `Conveyor` or edge == rotation). |
| `world/blocks/distribution/Duct.java` | `distribution/duct.rs` | `progress/current/rec_dir/next/prev/capped/back_capped`; rev 1 writes `rec_dir`; `handle_stack` sets `current`. |
| `world/blocks/distribution/StackConveyor.java` | `distribution/stack_conveyor.rs` | State machine + `link` packed pos + `cooldown` + `last_item`; `surge-conveyor` power status view data; `blends` depends on state. |
| `world/blocks/distribution/ChainedBuilding.java` | `distribution/chained_building.rs` | `trait ChainedBuilding { fn next(&self) -> Option<Entity>; }` consumed by 16. |
| `world/blocks/distribution/Junction.java` | `distribution/junction.rs` | `DirectionalItemBuffer(6)`, `speed = 26`; write/read buffer; rev 1. |
| `world/blocks/distribution/DuctJunction.java` | `distribution/duct_junction.rs` | `itemdata[4]`/`times[4]`, capacity 4; write/read `(f32, Option<ItemId>)` ×4. |
| `world/blocks/distribution/Router.java` | `distribution/router.rs` | `cycles[items]`, `last_item`, `last_input`, `time`; sized to item count (20 grows via `check_array_capacity`); controlled branch deferred (L8); `canControl` size==1. |
| `world/blocks/distribution/DuctRouter.java` | `distribution/duct_router.rs` | `sort_item`, `current`, `progress`, `target()` scan, rev 1. |
| `world/blocks/distribution/StackRouter.java` | `distribution/stack_router.rs` | Batch-unload while full; `base_efficiency`; glow view data only. |
| `world/blocks/distribution/Sorter.java` | `distribution/sorter.rs` | `sort_item`, `invert`, rotation bitfield flip; `minimap_color()` query; rev 2 write (`-1`/id `i16`), rev 1 legacy `DirectionalItemBuffer(20)` read. |
| `world/blocks/distribution/OverflowGate.java` | `distribution/overflow_gate.rs` | `instant_transfer`, no `update_tile`; side choice via `rotation`/`cdump`? (gate uses `rotation` bitmask); rev 4 with legacy reads. |
| `world/blocks/distribution/OverflowDuct.java` | `distribution/overflow_duct.rs` | Progress/current + side choice via `cdump` alternation; invert variant. |
| `world/blocks/distribution/ItemBridge.java` | `distribution/item_bridge.rs` | Link/config, `incoming`, `warmup`, transport timer, power (`phase-conveyor` 0.3), `check_accept`/`check_dump` incl. upstream bug kept; rev 1. |
| `world/blocks/distribution/BufferedItemBridge.java` | `distribution/buffered_item_bridge.rs` | `ItemBuffer(bufferCapacity)`, `timerAccept = 4/timeScale`; `do_dump` = `dump()`. |
| `world/blocks/distribution/DirectionBridge.java` | `distribution/direction_bridge.rs` | Base for `DuctBridge` (08) and `DirectionLiquidBridge` (09, L10); `occupied[4]`, `last_link`, `find_link`; no vanilla direct instance. |
| `world/blocks/distribution/DuctBridge.java` | `distribution/duct_bridge.rs` | Pulled transfer paced by speed; `occupied` protocol; config-less `DirectionBridge` link. |
| `world/blocks/distribution/DirectionLiquidBridge.java` | plan 09, implemented on 08's `DirectionBridgeBuild` | 09 owns `acceptLiquid`/`updateTile`; 08 owns the component + `find_link` (L10). |
| `world/blocks/distribution/MassDriver.java` | `distribution/mass_driver.rs` + `DriverBulletData` | State machine, waiting queue, `SimClock::run`; bolt via `MassDriverPayloadCarrier` (L7); `RotBlock` from 07; rev 1. |
| `world/blocks/distribution/DirectionalUnloader.java` | `distribution/directional_unloader.rs` | Timer/speed, offset cursor, core-unload gate; config `Item`; rev 0 write `(i16, i16)`. |
| `world/blocks/storage/StorageBlock.java` | `storage/storage_block.rs` | `linked_core` forwarding set of overrides; `core_merge` metadata; incinerate effect hook; overwrote merges items and clamps. |
| `world/blocks/storage/CoreBlock.java` | `storage/core_block.rs` | `storage_capacity` unification algorithm; team inventory access (R2); upgrade `place_began`/`before_place_began`; campaign/remote pieces delegated (12/11/15); rev 1 (`command_pos` nullable vec). |
| `world/blocks/storage/Unloader.java` | `storage/unloader.rs` | `ContainerStat`, comparator, `isPossibleItem`, `rotations`, `unloadAccumulate`; config `Item`; write `i16`; rev 1 read `i16`/rev 0 `i8`. |
| `world/blocks/payloads/Payload.java` | `payloads/mod.rs` (`PayloadRef`, `PayloadKind`, static write/read) | Type tags `Unit = 0`/`Block = 1` preserved; read reconstructs carried entities (`EMPTY_TILE`, `PayloadCarried`). |
| `world/blocks/payloads/PayloadBlock.java` | `payloads/payload_block.rs` (+`build_payload.rs`, `unit_payload.rs`) | `PayloadHolder` + move-in/out + `push_output` + pickup/carry flags; factory region suffix data. |
| `world/blocks/payloads/{BuildPayload,UnitPayload}.java` | `payloads/{build_payload,unit_payload}.rs` | `PayloadOps` impls: place/dump/update/destroyed/requirements/io; unit dump solidity/overlap gates (11 cross-ref). |
| `world/blocks/payloads/PayloadConveyor.java` | `payloads/payload_conveyor.rs` | `moveTime = 45`, step rollover, `blocked`, different-size alignment; unit push via `unitOn`. |
| `world/blocks/payloads/PayloadRouter.java` | `payloads/payload_router.rs` | `sorted`/`recDir`/`matches`/`control_time`, deterministic `pick_next`; config block/unit; rev 1 write `(ctype:i8, id:i16, recDir:i8)`. |
| `world/blocks/payloads/PayloadLoader.java` | `payloads/payload_loader.rs` | Item/liquid/power-in, `exporting`, `shouldExport`, `fraction`; power math uses 09 module (09 stub until landed). |
| `world/blocks/payloads/PayloadUnloader.java` | `payloads/payload_unloader.rs` | Extraction + `maxPowerUnload`, `offloadSpeed`, `full()`, `shouldExport`. |
| `world/blocks/payloads/PayloadMassDriver.java` | `payloads/payload_mass_driver.rs` | Charge machine, queue, payload hand-off, `transfer_effect` data; `maxPayloadSize`. |
| `world/blocks/payloads/PayloadDeconstructor.java` | `payloads/payload_deconstructor.rs` | `accum`, `progress`, liquid dump hook (10), item buffer acceptance rules. |
| `world/blocks/payloads/BlockProducer.java` | `payloads/block_producer.rs` | Dynamic consume closure equivalent (07 consumers), build progress, `moveOutPayload`. |
| `world/blocks/payloads/Constructor.java` | `payloads/constructor.rs` | `recipe` config + `canProduce` (size/filter/banned/env), `write.s(recipe)`; `filter`. |
| `world/blocks/payloads/PayloadSource.java` | `payloads/payload_source.rs` | Config block/unit, `UnitPayload` creation + `commandPos` hand-off (11), no accept. |
| `world/blocks/payloads/PayloadVoid.java` | `payloads/payload_void.rs` | Ingest + effects. |
| `entities/comp/BuildingComp.java` (transfer/payload regions) | plan 07 base; 08 behavior overrides | `offload/dump/dumpAccumulate/moveForward/canDump/getMaximumAccepted/itemTaken` semantics table §3.3. |
| `entities/comp/UnitComp.java` (`add/remove` region) | plan 11; 08 calls | Carried units removed/re-added to `Groups.unit` (L2). |
| `entities/bullet/MassDriverBolt.java` | plan 10; 08 defines `MassDriverPayloadCarrier` | `hit` → `data.to.handlePayload(b, data)`; `remove()` → despawn. |
| `content/Blocks.java` (distribution/storage/payload sections) | `content/blocks/registries/...` (02) + `BlockKindData` | Field-for-field metadata port; no behavior in content. |
| `annotations/.../impl/StructProcess.java` (packing) | reference for `packed::*` | Field order → ascending bit offsets (L4). |
| `tests/src/test/java/ApplicationTests.java` | `mind-core/tests` + harness scenarios | §7.1 mapping. |
| `PowerTestFixture.java` | not used here; 09 fixture | Cross-ref only. |

---

## 5. Milestones & task breakdown

Order is strict; each milestone ends with evidence appended to `## Changelog` when execution starts. Smallest vertical slice first.

**M0 — Buffers + transfer API + one-tile belt (vertical slice).**
- `world/item_buffer.rs` (packed structs, `ItemBuffer`, `DirectionalItemBuffer`, legacy read), `fixtures/logistics.rs` (flat map, source/sink behaviors), `distribution/conveyor.rs` minimal (capacity 3, motion loop), `LogisticsPlugin` skeleton, behavior registration into 07's dispatch.
- Port `ConveyorBuild` fixed arrays + `pass`; `items` integration via 07 `ItemModule`; `EMPTY_TILE` sentinel.
- Verify: `cargo test -p mind-core logistics::item_buffer` (pack/unpack, poll latency, wraparound, legacy read); `mind-headless run logistics_smoke --ticks 120` (1 source → 1 conveyor → 1 vault: item count 1, exact position after 28/29 ticks); alloc-audit test green (`logistics_alloc_steady`).

**M1 — Autotiler + full Conveyor/Duct/StackConveyor family.**
- `autotiler.rs`, `tile_bitmask.rs`; `conveyor.rs` full (clog, `overwrote`, sense, rev 0/1), `armored_conveyor.rs`, `duct.rs` (both variants), `stack_conveyor.rs` (plastanium/surge + state machine).
- Placement hooks for junction/bridge replacement are data; actual line placement is 15 (`handle_placement_line` delegates).
- Verify: ported `conveyorCrash`, `conveyorBench` (bench form); `mind-headless run logistics_conveyor_lane` (exact `speed` advance + throughput formula §7.2a); duct/stack timing tests; autotiler golden blend matrix (synthetic 5×5 tile layout → 47 indices).

**M2 — Routers, sorters, junctions, gates.**
- `router.rs`, `duct_router.rs`, `stack_router.rs`, `sorter.rs`, `junction.rs`, `duct_junction.rs`, `overflow_gate.rs`, `overflow_duct.rs`; configs; legacy reads.
- Verify: ported `routerOutputAll`, `sorterOutputCorrect`, `junctionOutputCorrect`; `mind-headless run logistics_router_fairness`; distribution checksum golden.

**M3 — Bridges + mass driver.**
- `item_bridge.rs`, `buffered_item_bridge.rs`, `direction_bridge.rs`, `duct_bridge.rs`, `mass_driver.rs` + `DriverBulletData` + `MassDriverPayloadCarrier` + `TestBolt` double; power consumption uses 09 stub (`efficiency = 1` in harness until 09).
- Verify: link-validity matrix tests; `mind-headless run logistics_bridge_latency`; `logistics_mass_driver_roundtrip`; `SimClock::run` callback determinism test.

**M4 — Storage & unloaders (non-core).**
- `storage_block.rs`, `unloader.rs`, `directional_unloader.rs`; `StorageBuild` incinerate effect hook; unloader comparator tests.
- Verify: `mind-headless run logistics_unloader_drain` (unloader empties containers to vaults); ported `blockInventories` item-total semantics (with 07 test shim); `inventory_deposit` subset.

**M5 — CoreBlock inventory & capacity.**
- `core_block.rs`: `storage_capacity` unification, team inventory access (R2 default), clamp on load, upgrade item transfer (`next_items`), `sense(itemCapacity/maxUnits)`, rev 1 `command_pos`; campaign hooks behind trait (`CoreCampaignHooks { handle_core_item, is_campaign, default_team, core_incinerates, allow_core_unloaders }`) implemented by 12; `playerSpawn`/capture delegated.
- Verify: `mind-headless run logistics_core_inventory` (multi-core capacity + shared items + vault linking); save/load round-trip of core inventories + linked storage (04 API); campaign-hook mock test.

**M6 — Payload core + conveyors/routers.**
- `payloads/mod.rs`, `payload_block.rs`, `build_payload.rs`, `unit_payload.rs`, `payload_conveyor.rs`, `payload_router.rs`; `PayloadCarried`, EMPTY_TILE, `handle_unit_payload`, `push_output` via 05 spatial query.
- Verify: `mind-headless run logistics_payload_move` (conveyor→conveyor hand-off, exact step timing); payload router sort config; ported `allPayloadBlockTest` phase 1 (all buildable blocks as payload update cleanly) with a harness-only subset until 11 lands; save/load payload round-trip.

**M7 — Payload loader/unloader/driver/deconstructor/constructor/producers.**
- `payload_loader.rs`, `payload_unloader.rs`, `payload_mass_driver.rs`, `payload_deconstructor.rs`, `block_producer.rs`, `constructor.rs`, `payload_source.rs`, `payload_void.rs`; power transfer math against 09 module; liquid stub until 09.
- Verify: `mind-headless run logistics_payload_load_unload`; `logistics_payload_driver_throw`; ported `allPayloadBlockTest` full phase 2 (save → `logic.reset` → load → checkPayloads) once 04/06 are green; deconstructor accumulation test (requirements → items with `buildCostMultiplier`).

**M8 — Integration, MCP, budgets, exit.**
- Register `MindSim` logistics APIs; inspector tab; MCP scenario §7.3; run full benchmark; finalize §7.4/§7.5; plan changelog.
- Verify: §7.3 MCP scenario with screenshots; budget table filled; exit checklist signed.

---

## 6. Data & formats

### 6.1 Component structs (canonical, per file)

```rust
// distribution/conveyor.rs
#[derive(Component, Default)] pub struct ConveyorBuild { ids:[ItemId;3], xs:[f32;3], ys:[f32;3], len:u8,
    next:Option<Entity>, nextc:Option<Entity>, aligned:bool, last_inserted:u8, mid:u8, minitem:f32,
    clog_heat:f32, blend_bits:u8, blending:u8, blend_sclx:i8, blend_scly:i8 }
// distribution/duct.rs
#[derive(Component, Default)] pub struct DuctBuild { progress:f32, current:Option<ItemId>, rec_dir:u8,
    blend_bits:u8, xscl:i8, yscl:i8, blending:u8, next:Option<Entity>, prev:Option<Entity>,
    nextc:Option<Entity>, capped:bool, back_capped:bool }
// distribution/stack_conveyor.rs
#[derive(Component, Default)] pub struct StackConveyorBuild { state:u8 /*0 Move,1 Load,2 Unload*/,
    blendprox:u8, link:i32 /*PackedTilePos or -1*/, cooldown:f32, last_item:Option<ItemId>, prox_updating:bool }
// distribution/router.rs
#[derive(Component, Default)] pub struct RouterBuild { cycles:ItemCycleArray /*[u8; items] growth-safe*/,
    last_item:Option<ItemId>, last_input:Option<PackedTilePos>, time:f32 }
// distribution/item_bridge.rs
#[derive(Component, Default)] pub struct ItemBridgeBuild { link:i32, incoming:SmallVec<[i32;4]>,
    warmup:f32, time:f32, time_speed:f32, was_moved:bool, moved:bool, had_valid_link:bool,
    transport_counter:f32, buffer:Option<ItemBuffer> }
// distribution/mass_driver.rs
#[derive(Component, Default)] pub struct MassDriverBuild { link:i32, rotation:f32, reload_counter:f32,
    state:DriverState, waiting_shooters:SmallVec<[Entity;4]> }
#[derive(Component)] pub struct DriverBulletData { from:Entity, to:Entity, items:Vec<i32> /*pooled*/ }
// storage/core_block.rs
#[derive(Component, Default)] pub struct CoreBuild { storage_capacity:i32, no_effect:bool, last_damage:TeamId,
    iframes:f32, thruster_time:f32, command_pos:Option<Vec2> }
// storage/unloader.rs
#[derive(Component, Default)] pub struct UnloaderBuild { unload_timer:f32, rotations:u16,
    sort_item:Option<ItemId>, possible:Vec<ContainerStat>, dumping_from:Option<usize>, dumping_to:Option<usize> }
#[derive(Clone, Default)] pub struct ContainerStat { building:Option<Entity>, load_factor:f32,
    can_load:bool, can_unload:bool, not_storage:bool, last_used:u32 }
// payloads/* (uniform)
#[derive(Component, Default)] pub struct PayloadConveyorBuild { item:Option<PayloadRef>, progress:f32,
    item_rotation:f32, animation:f32, cur_interp:f32, last_interp:f32, next:Option<Entity>,
    blocked:bool, step:i32, step_accepted:i32 }
```

### 6.2 Packed buffer layout (parity ABI with Java `@Struct`)

| Struct | Bits 0.. | Bits 16.. | Bits 32.. | Field order source |
|---|---|---|---|---|
| `BufferItem { short item; float time; }` | `item:u16` | `time:f32` | — | `DirectionalItemBuffer.BufferItemStruct` |
| `BufferItemLegacy { byte item; float time; }` | `item:u8` | `time:f32` (8..40) | — | `DirectionalItemBuffer.BufferItemLegacyStruct` |
| `TimeItem { short data; short item; float time; }` | `data:i16` | `item:u16` | `time:f32` (32..64) | `ItemBuffer.TimeItemStruct` |

`packed::*` functions cover construct + field get/set; unit tests assert exact `u64` literals derived from the Java formulas.

### 6.3 Building save revisions (08 blocks; plan 07/04 IO hook)

Write order follows Java `write(Writes)` after `super.write`; revisions are append-only and dispatch per plan 04's nested tile-entity chunk (`building.version() u8`).

| Block | Version | Fields after base |
|---|---|---|
| Conveyor | 1 | `len:i32`, per item `(id:i16, x:i8, y:i8)`; rev 0 packed `i32` per item (legacy read preserved) |
| Duct | 1 | `rec_dir:u8`; rev 0 no `rec_dir`; `current = items.first()` on read |
| StackConveyor | 0 | `link:i32`, `cooldown:f32`; `last_item = items.first()` |
| Junction | 1 | `DirectionalItemBuffer` (4 × `(index:u8, len:u8, longs)`) |
| DuctJunction | 0 | 4 × (`times:f32`, `item:TypeIO item`) |
| Sorter | 2 | `sort_item:i16`; rev 1 additionally reads `DirectionalItemBuffer(20)` legacy |
| DuctRouter/StackRouter | 1 | `sort_item:i16`; StackRouter inherits |
| OverflowGate | 4 | no payload; revisions 1/3 legacy reads (buffer/int), then `items.clear()` |
| OverflowDuct | 0 | none |
| ItemBridge/Buffered | 1 | `link:i32`, `warmup:f32`, `incoming` count+ints, `was_moved\|moved:bool` (rev ≥ 1); Buffered adds `ItemBuffer` after super |
| DirectionBridge/DuctBridge | 0 | none (`occupied` is transient) |
| MassDriver | 0 | `link:i32`, `rotation:f32`, `state:u8` |
| DirectionalUnloader | 0 | `unload_item:i16`, `offset:i16` |
| StorageBlock | 0 | none (`linked_core` transient, rebuilt in proximity) |
| CoreBlock | 1 | `command_pos:TypeIO vec nullable` (rev ≥ 1) |
| Unloader | 1 | `sort_item:i16` (rev 0 `i8`) |
| PayloadBlock/PayloadConveyor | 0 | `payVector(f32,f32)`, `payRotation:f32`, `Payload` blob; payload conveyor instead `progress:f32`, `itemRotation:f32`, `Payload` blob |
| PayloadRouter | 1 | + `ctype:i8`, `sorted:i16`, `rec_dir:i8` |
| PayloadLoader | 1 | + `exporting:bool` |
| PayloadDeconstructor | 0 | `progress:f32`, `accum` count `i16` + floats, `Payload` blob |
| BlockProducer | 0 | `progress:f32` |
| Constructor | 0 | + `recipe:i16` |
| PayloadSource | 1 | `unit:i16`, `configBlock:i16`, rev ≥ 1 `commandPos` nullable vec |
| MassDriver (payload) | 1 | + `turretRotation:f32`, `state:u8`, rev ≥ 1 `reloadCounter/charge/loaded/charging` |

`Payload` blob = `bool` + `kind:u8` + (`block id:i16` + `building version:u8` + payload-build chunk) | (`unit class id:u8` + unit write). Plan 04 provides `BuildingWriter/Reader` scratch; 08 registers per-block codecs via 07.

### 6.4 Config values

`ConfigValue` variants used by 08 (encoded by 04's `TypeIO`, surfaced by 15):

| Variant | Java config | Blocks |
|---|---|---|
| `ConfigValue::Content(ContentRef)` | `Item.class`/`Block.class`/`UnitType.class` | Sorter, DuctRouter, StackRouter, Unloader, DirectionalUnloader, Constructor, PayloadRouter, PayloadSource |
| `ConfigValue::Point2Relative(i16, i16)` | `Point2.class` (relative) | ItemBridge, MassDriver, PayloadMassDriver |
| `ConfigValue::Int(i32)` | `Integer.class` (packed pos / `-1`) | ItemBridge, MassDriver, PayloadMassDriver |
| `ConfigValue::Clear` | `configClear(...)` | all configurable logistics blocks |

`configured()` re-validates (bridge link validity, constructor `canProduce`, payload router `canSort`) and never panics on bad values (debug log + ignore).

### 6.5 Region names used (owned by 03; listed for the 47-slice/asset audit)

`<name>-<0..46>` and variants `<name>-<v+1>-<0..46>` (autotiles); `duct-top/bottom/cap`, `duct-junction-bottom/top`; `conveyor` array `[7][4]` from `@-#1-#2` (7 blend cases × 4 frames); `stack-conveyor-<0..2>`, `-edge`, `-stack`, `-glow`, `-edge-glow`; bridge `-end/-bridge/-arrow/-bridge-bottom/-bridge-liquid/-dir`; mass driver `-base`, payload `-top/-out/-in/-over`, factory fallbacks `factory<suf>-<size>`; payload deconstructor `topRegion`; `center`/`cross-full`, `block-<name>-full` icons.

### 6.6 Scenario/golden files

- `scenarios/logistics_conveyor_lane.json`, `logistics_router_fairness.json`, `logistics_sorter_config.json`, `logistics_bridge_latency.json`, `logistics_mass_driver_roundtrip.json`, `logistics_payload_load_unload.json`, `logistics_core_inventory.json`, `logistics_unloader_drain.json`, `logistics_payload_move.json` (plan 00 scenario format).
- `tests/golden/logistics/*.checksum` (plan 23 format), `tests/golden/logistics/tile_bitmask.txt`, `tests/golden/logistics/buffer_pack.txt` (u64 literals).
- `bench/baselines.json` gains `logistics_conveyor`, `logistics_midgame`.

---

## 7. Oracle & verification (REQUIRED)

### 7.1 Ported tests (Mindustry `tests/src/test/java` → `cargo test -p mind-core`)

| Java test / behavior | Rust test | Oracle |
|---|---|---|
| `ApplicationTests.sorterOutputCorrect` (`:440`) | `world::blocks::distribution::sorter::tests::sorter_output_correct` | coal→right vault first, copper→top vault first, coal→left vault first after 200 ticks; same tile layout. |
| `ApplicationTests.routerOutputAll` (`:473`) | `distribution::router::tests::router_output_all` | all three vaults receive coal; rotation-cursor cycle advances deterministically. |
| `ApplicationTests.junctionOutputCorrect` (`:501`) | `distribution::junction::tests::junction_output_correct` | coal reaches vault1, copper reaches vault2; directional buffers preserve times. |
| `ApplicationTests.conveyorCrash` (`:541`) | `distribution::conveyor::tests::conveyor_crash` | `accept_stack(copper, 1000, None)` does not panic; `len <= 3`. |
| `ApplicationTests.conveyorBench` (`:550`) | `distribution::conveyor::tests::conveyor_bench` (`#[ignore]`, bench) + `mind-headless bench logistics` | nonzero items delivered; §7.4 numbers. |
| `ApplicationTests.blockInventories` (`:273`) | `storage::tests::item_module_totals` | 5 coal + 50 titanium − 10 phase − 10 titanium = 45 total after 07 shim. |
| `ApplicationTests.inventoryDeposit` (`:689`) | `storage::tests::deposit_all_blocks` (subset; full matrix once 07/10 land) | each depositable block routes items to the right inventory/linked core. |
| `ApplicationTests.checkPayloads`/`allPayloadBlockTest` (`:824`, `:842`) | `payloads::tests::all_payload_blocks_update_and_roundtrip` | every buildable block survives `handle_payload → update → save → reset → load`; health equal. |
| `ApplicationTests.liquidJunctionOutput`/`liquidRouterOutputAll` | plan 09 tests (cross-read) | owned by 09; 08 only shares `Autotiler`. |
| `ApplicationTests.edges`/`blockOverlapRemoved` | 06/07 tests | `Edges::get_edges` order is the foundation of `proximity`; 08 asserts it in `contract_edges_order`. |
| `ApplicationTests.initBuilding` core assertions (`:970`) | `storage::core_block::tests::team_core_registration` | core registered in `Teams`, `core.items` reachable, deposit of 3000×items bounded by capacity. |
| (new, from Java behavior) | `world::item_buffer::tests::{pack_literals, poll_latency_and_wraparound, legacy_read}` | exact u64 literals; `poll` releases only after `speed`; `time > now` branch releases. |
| (new) | `world::blocks::autotiler::tests::blend_matrix_golden` | synthetic layout → `blending`/`blend_bits`/scales golden. |
| (new) | `storage::unloader::tests::comparator_order` | priority: core/notStorage, canUnload-only, canLoad-only, loadFactor, lastUsed inverted. |
| (new) | `distribution::mass_driver::tests::state_machine_and_waiting_queue` | idle→accepting→shooting transitions, queue FIFO, `SimClock::run` completion. |
| (new) | `payloads::tests::carried_entity_group_exclusion` | carried units/buildings absent from `Groups.unit`/`Groups.build`, `tile == EMPTY_TILE`, re-added on dump. |

### 7.2 Headless harness scenarios (`mind-headless`)

Registered in `mind-headless/src/scenarios/logistics_*.rs`; every scenario boots the fixture map from `fixtures/logistics.rs` with `workers = 1`, `seed` fixed, and dumps a plan-23 canonical checksum.

| Command | Asserts |
|---|---|
| `run logistics_smoke --ticks 120 --dump out/log_smoke.json` | 1 source → 1 conveyor → 1 vault; exactly 1 item exists; after `ceil(1/speed)` ticks the belt item's `y` advanced by `speed` ± 1e-6 (exact tiles/tick); checksum golden. |
| `run logistics_conveyor_lane --ticks 600 --seed 1234` | 32-tile base conveyor lane at full load: delivered ≈ `floor((600 − warmup) · speed/itemSpace)` items (tolerance ±2); item positions strictly monotonic; zero clog heat when draining; `logistics_conveyor_lane.checksum` golden. Repeat with `titanium-conveyor` (`speed = 0.0801`) and assert `2.29×` ± 1% throughput. |
| `run logistics_router_fairness --ticks 600` | source→router→3 vaults; every vault count > 0 and `max−min <= 1`; per-item cycle order golden. |
| `run logistics_sorter_config --ticks 200` | exact port of `sorterOutputCorrect` layout; first items per vault coal/copper/coal. |
| `run logistics_bridge_latency --ticks 300` | `bridge-conveyor` (buffer 14, speed 74): first item arrival tick recorded and equal to golden; `phase-conveyor` (`transportTime = 2`) arrival tick = expected; link-config round-trip via `SimCommand::Configure`. |
| `run logistics_mass_driver_roundtrip --ticks 1200 --assume-power` | two linked drivers, 120 copper injected; receiver total == sent − in-flight bolt payload; states return to idle; `DriverBulletData` pool has zero live entries after arrival. |
| `run logistics_payload_move --ticks 300` | payload-conveyor chain moves a `container` payload exactly `moveTime = 45` ticks per tile; router rotations; payload transforms match step boundaries. |
| `run logistics_payload_load_unload --ticks 900` | loader ingests payload, moves 100 copper into it, exports it; unloader reverses; block item totals conserved (loader+payload+unloader sum unchanged). |
| `run logistics_core_inventory --ticks 1 --dump out/core.json` | core + 2 adjacent vaults: `storage_capacity = core.itemCapacity + Σ vault capacities`; deposits above capacity rejected/incinerated per rules; two cores share totals; save/load round-trip preserves totals. |
| `run logistics_unloader_drain --ticks 600` | unloader moves a 4-container mixed inventory into vaults; deterministic extraction order via comparator; no item loss. |
| `bench logistics --map fixtures/logistics_lane --iters 200000` | §7.4 budget; prints items/tick and P50/P95 per-tick time. |

### 7.3 MCP playtest scenario (open-godot-mcp)

Preconditions: plan 00 spine running with `MindSim` autoload at `/root/Spine/SimHost` (adapt the path if plan 00 names it differently), plan 07 placement APIs, plan 03 assets for belt/core sprites.

1. `godot_health check` → `{ok: true}`.
2. `godot_game play(scene="res://scenes/spine.tscn", frozen=false)`; poll `godot_log get source=game count=50` for `[sim] playing`.
3. `godot_exec eval`:
   ```gdscript
   MindSim.dev_load_flat(48, 48)
   MindSim.dev_set_floor(14, 24, "ore-copper")
   print(MindSim.dev_place("mechanical-drill", 14, 24, 0))
   for x in range(15, 23):
       print(MindSim.dev_place("conveyor", x, 24, 0))
   print(MindSim.dev_place("core-shard", 24, 24, 0))
   print(MindSim.find_building(14, 24) != null, MindSim.find_building(24, 24) != null)
   ```
   Expect `true true true` (drill, belt chain, core placed).
4. `godot_exec eval`: `MindSim.dev_step_ticks(400)` (fast-forwards the fixed 60 Hz sim; also callable via `godot_game_time step`).
5. Drill output check: `godot_exec eval`:
   ```gdscript
   var d = MindSim.dev_inspect_building(14, 24)
   var c = MindSim.dev_inspect_building(24, 24)
   print(d.block, d.items_total, "|", c.block, c.items_total)
   ```
   Expect the core's item count > 0, drill item count >= 0 (drill may hold or have passed items).
6. Belt item motion check (deterministic, not just visual): `godot_exec eval`:
   ```gdscript
   var a = MindSim.dev_inspect_conveyor(18, 24)
   print(a.len, a.ys)
   ```
   Assert `len > 0` for at least one belt on the lane over a 120-tick window; then `MindSim.dev_step_ticks(30)` and assert `ys` changed by ≈ `0.035 × 30` tiles (or the titanium equivalent).
7. Bridge/payload spot check (optional, requires plan 09/10 placeholders absent → use only payload-conveyor): `MindSim.dev_place("payload-conveyor", 30, 30, 0)` + `MindSim.dev_spawn_payload("container", 30, 30)`; `MindSim.dev_inspect_payload(30, 30)` returns `{kind:"block", content:"container"}`.
8. `godot_runtime_state watch` node `/root/Spine/SimHost` property `checksum` 500 ms → changes between steps, stable when paused.
9. `godot_screenshot game` before/after a 60-tick step; attach both to the Changelog (belt item visibly advances; core visible with items).
10. `godot_log errors` → empty (no `unwrap`/panic lines); `godot_game stop`.

Fallback if plan 07's `dev_place` API differs: use the plan-00 editor place tool through `godot_input` clicks at tile screen coordinates computed from the camera, and assert via `godot_runtime_state inspect`.

### 7.4 Performance budget

Method: `cargo bench -p mind-core --bench logistics` (criterion; port of `conveyorBench`) and `mind-headless bench logistics`; P50/P95 over 200k iterations on the dev machine; allocation counter (plan 05 §7.4 hook) attached to a 600-tick run. Baseline map: 128-tile belt lane at full load + a 32×32 mid-game logistics cluster (2,000 transport buildings, 500 buffers, 200 payload blocks).

| Metric | Budget |
|---|---|
| `conveyorBench` port (128 conveyors × 200,000 updates = 25.6M + same again in the timed double-run) | ≥ 20M building-updates/s (≤ 2.6 s wall for the doubled loop); items delivered non-zero |
| Full-load items/tick per lane | matches `speed/itemSpace` within 1% (base 0.0875 = 5.25/s; titanium 0.20025 = 12.0/s; plastanium stack 0.667 = 40/s) |
| Mid-game logistics slice (`UpdateBuildings` subset: 2,000 belts/routers/junctions + 500 bridges + 200 payload blocks) | ≤ 0.35 ms P95/tick single-threaded |
| Steady-state allocations in `update_tile`/`handle_*`/`poll`/`dump` | 0 net allocations over 600 ticks (alloc-audit gate) |
| Buffer poll/accept (junction/bridge) | ≥ 50M ops/s; `poll` is O(1) per direction (no memmove on the hot path: queue head index) |
| Save/load of a 128-belt loaded lane + 50 bridges + payloads | ≤ 10 ms P95 each way (plan 04 bench) |
| Payload deconstructor/constructor update | ≤ 5 µs P95 per building per tick |

### 7.5 Exit criteria checklist

- [ ] `cargo test -p mind-core` green for all §7.1 tests (ported + new); `logistics_*` scenarios green with committed goldens.
- [ ] Exact lane speed verified for base/titanium/plastanium/surge; throughput formula verified at full load.
- [ ] Router fairness, sorter config, junction directionality match upstream fixtures exactly.
- [ ] Bridge buffers observe correct latency; mass-driver round-trip conserves items and leaves no pooled leak.
- [ ] Payload move/load/unload/round-trip pass; carried entities excluded from groups and restored on dump.
- [ ] Core capacity unification, team sharing (or chosen R2 model), incineration and linked storage verified; campaign hooks wired through mock then 12.
- [ ] All §6.3 revisions written/read; legacy revisions (conveyor v0, sorter v1, overflow gate v1/v3, unloader v0) load without panic.
- [ ] §7.3 MCP scenario passes with two screenshots attached.
- [ ] §7.4 budgets measured and recorded; alloc-audit shows zero steady-state allocations.
- [ ] `cargo tree -p mind-core` has no `godot`/`tokio`; no `HashMap` iteration in logistics sim paths.
- [ ] GPL headers on every ported file; §8 decisions resolved or formally deferred.

---

## 8. Risks & open decisions

| # | Item | Default taken | Flag |
|---|---|---|---|
| R1 | **Plan 07 missing** — `BuildingBehavior`, `ItemModule`, proximity/`Edges`, IO hooks not yet written. This plan's §3.2 is the freeze request. | 08 proceeds against §3.2 names; the orchestrator merges with 07 before M1. | Reconcile with `07` author (blocking for M1+, not M0). |
| R2 | **Core inventory sharing model.** Java aliases `items` pointers across cores (`CoreBlock.java:754`) and into linked `StorageBuild`s. ECS cannot alias components. | `TeamInventory` resource in plan 12's `Teams` (`IndexMap<TeamId, ItemModule>`); cores/storage route through accessors. Alternative: `CoreLink(primary_core)` component chain per team. | **NEEDS USER DECISION** (affects save shape 04, campaign 12, core UI 14, sync 21; default continues). |
| R3 | **Carried payload entity model.** Java keeps heap `Payload` objects detached from groups/tiles. | Real ECS entities + `PayloadCarried` marker + group removal + `TilePos::EMPTY`, re-added on dump (L2). Alternative: value-snapshot payloads (serialize a `BuildingSnapshot`/`UnitSnapshot`). | **NEEDS USER DECISION** (affects entity IDs in saves/replays 04/21; default continues). |
| R4 | `proximity` order must equal Java `Edges.getEdges(size)` order or router/unloader/dump distributions drift. | 07/06 must implement `Edges::get_edges` byte-order-identical; 08 adds `contract_edges_order` test. | Reconcile with `06`/`07`. |
| R5 | Java swap-removal vs plan 05 insertion-stable update order changes tie-breaks. | Accept 05 OD-05-A; golden checksums freeze our order. | Track in `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`. |
| R6 | Variable-capacity buffer storage (`Vec` vs fixed max). | `Vec` allocated at `created()` from block metadata, capacity retained across pooling (L3); buffers with `capacity <= 8` may use inline `SmallVec`. | No user needed. |
| R7 | `MassDriverBolt` (10) and payload carriers drive cross-plan tests. | `MassDriverPayloadCarrier` trait + `TestBolt` double; swap to 10's implementation when it lands. | Reconcile with `10`. |
| R8 | Campaign/core hooks (`handleCoreItem`, capture, `rules.coreIncinerates`/`allowCoreUnloaders`, core stats) owned by 12. | `CoreCampaignHooks` trait with a no-op default + mock in tests; 12 implements. | Reconcile with `12`. |
| R9 | `Time.run` delayed mass-driver completion vs pause/editor boundary. | Use 05 `SimClock::run`; scenario asserts completion only under `is_game && !paused`; queued callbacks cleared on reset (05 owns). | Reconcile with `05`. |
| R10 | Legacy revision reads (Sorter rev 1 buffer, OverflowGate rev 1/3, Conveyor rev 0, Unloader rev 0/1) are dead weight under native `MGRS` unless OD2 import is enabled. | Implement readers now (cheap, revision-dispatch already required); no writers. | Follows plan 04 OD2. |
| R11 | `Payload.fits` units: `UnitPayload.size() = unit.hit_size` while `BuildPayload.size() = block.size*tilesize`; `payloadLimit = 3` means "3 blocks" for builds but 3 tiles of hit size for units. | Preserve exactly (`fits(s) = size()/tilesize <= s`). | No user needed. |
| R12 | Power transfer inside `PayloadLoader/Unloader` touches 09's `PowerModule` capacity API before 09 lands. | Shim via `PowerCapacity` trait with a test implementation; replace at 09. | Reconcile with `09`. |
| R13 | `@SyncField` metadata for logistics state (belt positions are recomputed from sim; core items synced elsewhere in Java). | Persist/checksum belt state; do **not** sync belt item arrays as fields (Java recomputes on join); core inventory sync policy decided by 21/12. | Reconcile with `21`. |
| R14 | Modded item count growth invalidates `RouterBuild.cycles`/`ItemModule` array sizing. | Sized from `content.items().len()` with `check_array_capacity`-equivalent growth on mod load (20). | Reconcile with `20`. |
| R15 | `Unloader` comparator uses `Integer.MAX_VALUE` modulo increments and `Pools` for `ContainerStat`; ECS port must preserve ordering and avoid per-tick allocation. | `Vec<ContainerStat>` rebuilt only in `on_proximity_update`; `last_used` uses `u32` wrapping `% i32::MAX` semantics. | No user needed. |

---

## 9. References

### Mindustry sources read

- `Mindustry/AGENTS.md`; `core/AGENTS.md`; `core/src/mindustry/AGENTS.md`; `core/src/mindustry/world/AGENTS.md`; `core/src/mindustry/world/blocks/AGENTS.md`; `core/src/mindustry/content/AGENTS.md`; `core/src/mindustry/type/AGENTS.md`; `core/src/mindustry/entities/AGENTS.md`; `core/src/mindustry/tests/AGENTS.md`
- `core/src/mindustry/world/blocks/Autotiler.java`, `TileBitmask.java`
- `core/src/mindustry/world/blocks/distribution/` (all 20 files listed in §2.1)
- `core/src/mindustry/world/blocks/storage/StorageBlock.java`, `CoreBlock.java`, `Unloader.java`
- `core/src/mindustry/world/blocks/payloads/` (all 14 files)
- `core/src/mindustry/world/ItemBuffer.java`, `DirectionalItemBuffer.java`, `world/modules/ItemModule.java`
- `core/src/mindustry/entities/comp/BuildingComp.java` (transfer/dump/payload regions), `entities/comp/UnitComp.java` (`add`/`remove`), `entities/bullet/MassDriverBolt.java`
- `core/src/mindustry/content/Blocks.java` distribution/storage/payload sections
- `annotations/src/main/java/mindustry/annotations/impl/StructProcess.java` (bit packing reference)
- `tests/src/test/java/ApplicationTests.java` (`sorterOutputCorrect`, `routerOutputAll`, `junctionOutputCorrect`, `conveyorCrash`, `conveyorBench`, `blockInventories`, `inventoryDeposit`, `checkPayloads`, `allPayloadBlockTest`, `initBuilding`)

### Project plans read

- `mindustry-godot/HIGH_LEVEL_PLAN.md`, `PRELIMINARY_PLAN.md`, `00_FOUNDATION_IMPLEMENTATION_PLAN.md`, `02_CONTENT_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`, `05_SIM_CORE_IMPLEMENTATION_PLAN.md`
- Sibling plans that do not exist at authoring time and must reconcile by filename: `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (tile/`Edges` order), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (§3.2 freeze), `09`, `10`, `11`, `12`, `15`, `16`, `17`, `20`, `21`, `23`.

---

## Changelog

> Newest last. Evidence is the actual test/scenario output (plan 08 §7); never tick on intent.

**2026-10-02 — M0 complete (`lane/08-logistics`, base `81177fa`).**

- **Buffers.** `world/item_buffer.rs`: `ItemBuffer` + `DirectionalItemBuffer` with the exact Java `@Struct` bit layouts (`BufferItem`, `BufferItemLegacy`, `TimeItem`; §6.2 / L4), f32 `Time.time` comparison incl. the `now < time` wraparound branch (L5), and legacy 1-byte-item reads (`read_legacy`). Capacity preallocated once (L3). Tests: `world::item_buffer::tests::{pack_literals_match_java_structs, poll_latency_and_wraparound, buffer_write_read_roundtrip, directional_legacy_read_upgrades_items, directional_accept_poll_remove}` — **5 passed**.
- **Transfer API.** `world/blocks/distribution/transfer.rs` ports the `BuildingComp` transfer region: `accept_item`/`handle_item`/`accept_stack`/`handle_stack`/`remove_stack` defaults, `get_maximum_accepted`, `dump`/`dump_accumulate`/`offload`/`move_forward`, `increment_dump`, `front`, and `dispatch_accept_item`/`dispatch_handle_item`/`dispatch_can_dump` for behavior overrides. `BuildingBehavior` gained the matching default hooks (append-only). Tests: `transfer::tests::{dump_moves_item_to_accepting_neighbor, move_forward_uses_facing_neighbor}`.
- **Conveyor vertical slice.** `world/blocks/distribution/conveyor.rs`: fixed `[3]` parallel arrays, the Java `updateTile` motion loop / `minitem` / `mid` / `clogHeat`, `acceptItem`/`handleItem`/`acceptStack`/`handleStack`/`removeStack`/`pass`, `overwrote`, rev 1 write + rev 0/1 read. `ConveyorBehavior::{BASE, TITANIUM, ARMORED}` carry the exact speeds (`0.035`/`0.0801`/`0.08`). Test `belt_moves_item_at_exact_speed` asserts exactly `speed` tiles/tick; `titanium_belt_speed`; `adjacent_belts_pass_isolated`; `terminal_passes_to_container_isolated`; `harness_chain_delivers_to_container`.
- **Storage (deposit half).** `world/blocks/storage/storage_block.rs` `StorageBehavior` for `container`/`vault`/`reinforced-container`/`reinforced-vault` (linked-core forwarding is M4/M5).
- **Registration + fixtures.** `world/blocks/mod.rs::default_registry` is now used by `BlockTable::build_default`; `BuildHarness::register_behavior` lets fixtures override a name; `world/fixtures/logistics.rs` provides `SourceBehavior` + a logistics state checksum.
- **Oracle.** `mind-headless blocks scenario logistics_smoke` (1 source → 5 belts → 2×2 `container`; 400 ticks) → **pass**, `sink_items=22`, `belt_items=12`, checksum `f5c005f78e993ac1` (identical across two runs). `cargo test -p mind-core` lib **540 passed / 2 ignored** (baseline 526), workspace clippy `-D warnings` + `cargo fmt --check` clean, `cargo check -p mind-gdext` clean, `cargo tree -p mind-core` Godot/tokio-free.

**2026-10-02 — M1 partial: Autotiler + TileBitmask + full Conveyor/ArmoredConveyor (08↔09 freeze).**

- **`world/blocks/autotiler.rs`.** Ported `Autotiler.java`: `build_blending`, `transform_case`, `blends` (world + plan-directional), `blends_armored`, `facing`, `not_looking_at`, `looking_at_either`, `looking_at`, `d4/d4x/d4y`, `BlendNeighbor`, `SliceMode`/`slice_span`. The draw-time slice logic is a data descriptor (plan 16 draws). Tests: `d4_order_matches_geometry`, `transform_case_matrix`, `blend_matrix_all_sides`, `blend_matrix_one_side`, `facing_matches_geometry`.
- **`world/blocks/tile_bitmask.rs`.** The 47-slice `[u8; 256]` `VALUES` table + `load`/`load_variants` name resolution is retained as the single shared owner for plans 09/16 (no fork; plan 03 §7.1a test kept green).
- **Conveyor family.** `on_proximity_update` computes `blend_bits`/`blending`/`blend_sclx`/`blend_scly`, `next`/`nextc`/`aligned`; `ArmoredConveyor` uses `blends_armored` + the armored `accept_item` (source-is-conveyor or edge == rotation) in `ConveyorBehavior::ARMORED`.
- **Oracle.** `mind-headless blocks scenario logistics_conveyor_lane` (32 belts + a 3/tick source, 1600 ticks) → **pass**, `delivered=59` (matches `speed/itemSpace = 0.0875` items/tick × ~670 delivery ticks), checksum `5d627616f619347c`.

**08↔09 frozen API (also recorded in `HIGH_LEVEL_PLAN.md` §13):**

- `mind_core::world::blocks::autotiler`:
  - `pub trait BlendWorld { fn blends_block(&self, source: TilePos, rotation: u8, other_x: i32, other_y: i32, other_rot: u8, other_block: BlockId) -> bool; fn near_build(&self, direction: u8, source: TilePos) -> Option<BlendNeighbor>; fn square_sprite(&self, block: BlockId) -> bool; fn rotated_output(&self, block: BlockId) -> bool; fn block_size(&self, block: BlockId) -> i32; }`
  - `pub fn build_blending(world: &dyn BlendWorld, tile: TilePos, rotation: u8, directional: &[Option<BlendNeighbor>; 4], check_world: bool) -> [i32; 5]` (`[case, scale_x, scale_y, bits, non_square_bits]`).
  - `pub struct BlendNeighbor { pub x: i32, pub y: i32, pub rotation: u8, pub block: BlockId }`
  - `pub fn blends_directional(...)`, `blends_world(...)`, `blends_armored(...)`, `looking_at(...)`, `looking_at_either(...)`, `not_looking_at(...)`, `facing(...)`, `d4/d4x/d4y`, `mod_i`, `relative_to`, `SliceMode`/`slice_span`.
- `mind_core::world::blocks::tile_bitmask`: `pub const VALUES: [u8; 256]`, `SLICE_COUNT`, `load(&AtlasIndex, &str) -> [Option<&Region>; 47]`, `load_variants(&AtlasIndex, &str, usize)`.
- `mind_core::world::blocks::distribution::transfer`: `dispatch_accept_item`/`dispatch_handle_item`/`dispatch_can_dump`, `dump`/`offload`/`move_forward`, `get_maximum_accepted`, `item_capacity`, `proximity`/`front`/`increment_dump`.
- `ConveyorBuild` component fields for plan 16: `ids: [ItemId;3]`, `xs/ys: [f32;3]`, `len: u8`, `next/nextc`, `aligned`, `mid`, `minitem`, `clog_heat`, `blend_bits`, `blending`, `blend_sclx`, `blend_scly`.

**Open (next milestones).** M1 `Duct`/`DuctJunction` + `StackConveyor` state machine; M2 `Router`/`Sorter`/`Junction`/`OverflowGate`/`OverflowDuct` + configs; M3 `ItemBridge`/`BufferedItemBridge`/`DirectionBridge`/`DuctBridge`/`MassDriver`; M4 `StorageBlock` core-link + `Unloader`/`DirectionalUnloader`; M5 `CoreBlock`; M6–M7 payloads; M8 integration/MCP/budgets. In-engine MCP run is deferred to the orchestrator's single-editor mutex.
