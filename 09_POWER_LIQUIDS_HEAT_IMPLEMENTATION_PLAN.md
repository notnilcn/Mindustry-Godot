# 09 — POWER, LIQUIDS & HEAT IMPLEMENTATION PLAN

> Inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions D1–D9), §2 (architecture), §4 (template), §6–§9 (conventions). Where this file conflicts with `HIGH_LEVEL_PLAN.md`, the high-level plan wins. Do not re-litigate locked decisions here.
> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | 🟡 **M0–M5 landed on `lane/09-power` (2026-10-02)** — power graph/nodes/generators/sandbox, liquids + bridges (plan 08 seam reconciled), heat, headless scenarios/goldens/benches. M6 MCP/`MindSim` debug + Networks inspector tab deferred to the orchestrator's editor mutex. See Changelog. |
| **Phase** | P3 — World & systems |
| **Depends on** | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (Block/Building entity, proximity updates, consumer framework, `ItemModule`/`LiquidModule`/`PowerModule` data layout, config framework, building IO). 07 is **not written yet** at the time of this draft — §3.13 fixes the interface contract this plan assumes; the orchestrator must reconcile. `08_LOGISTICS_IMPLEMENTATION_PLAN.md` is required only for `LiquidBridge`/`DirectionLiquidBridge` (which extend 08's `ItemBridge`/`DirectionBridge`) and is scheduled at M5. `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` is required transitively through 07 (tiles, `World.raycast`, `Point2` packed positions). |
| **Blocks** | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (turrets consume power via `ConsumePower`, use `calculateHeat`, and destroy power/heat buildings), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Rules.solarMultiplier`/`lighting`/`ambientLight`, `reactorExplosions`, `TeamData.buildingTree`, sector damage rules), `14_UI_IMPLEMENTATION_PLAN.md` (power/liquid/heat bars and flow UI), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (power lasers, conduit fluid frames, heat input/output draw), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (network checksums/benches). `08_LOGISTICS_IMPLEMENTATION_PLAN.md` consumes the liquid transfer primitives shipped here for its `LiquidBridge` counterpart (reconcile, §3.13). |
| **Sources** | `core/src/mindustry/world/blocks/power/{PowerBlock,PowerDistributor,PowerGraph,PowerNode,LongPowerNode,BeamNode,Battery,PowerDiode,PowerGenerator,ConsumeGenerator,ThermalGenerator,SolarGenerator,ImpactReactor,NuclearReactor,VariableReactor,HeaterGenerator,LightBlock}.java`; `world/blocks/liquid/{LiquidBlock,Conduit,ArmoredConduit,LiquidRouter,LiquidJunction,LiquidBridge}.java`; `world/blocks/distribution/{ItemBridge,DirectionBridge,DirectionLiquidBridge}.java` (closure/interface); `world/blocks/heat/{HeatBlock,HeatProducer,HeatConsumer,HeatConductor}.java`; `world/blocks/production/HeatCrafter.java`; `world/modules/{PowerModule,LiquidModule}.java`; `entities/comp/{BuildingComp,PowerGraphUpdaterComp}.java` (`calculateHeat`, `getPowerConnections`, `updatePowerGraph`, `powerGraphRemoved`, `moveLiquid`/`dumpLiquid`/`getLiquidDestination`, `updateConsumption`, `update`); `entities/GroupDefs.java` (`powerGraph` group); `world/Block.java` (`consumePower*`, `reConsumers`, `hasPower`, `connectedPower`, `conductivePower`, `insulated`, `liquidPressure`, `liquidCapacity`); `world/consumers/{Consume,ConsumePower,ConsumeLiquidBase,ConsumeLiquid,ConsumeLiquidFilter,ConsumeCoolant,ConsumeItemFilter}.java`; `type/Liquid.java`; `core/Logic.java` entity update order; `tests/src/test/java/power/{PowerTestFixture,PowerTests,DirectConsumerTests,ConsumeGeneratorTests}.java`. |
| **AGENTS read** | `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `world/AGENTS.md`, `world/blocks/AGENTS.md`, `entities/AGENTS.md`, `content/AGENTS.md`, `type/AGENTS.md`, `game/AGENTS.md`, `graphics/AGENTS.md`, `tests/AGENTS.md` |
| **Extends spine** | (a) `mind-headless` subcommands `power <scenario>`, `liquid <scenario>`, `heat <scenario>`, `bench power│liquid│heat` and dump format `network_state.json`; (b) `mind_core::fixtures::power` harness (port of `PowerTestFixture`); (c) `MindSim` debug API `power_debug(x,y)`, `liquid_debug(x,y)`, `heat_debug(x,y)`, `place_block(block,x,y,rot)`, `break_block(x,y)` and inspector fields `network_counts{graphs,conduits,heat_buildings}`; (d) the plan-00 state inspector gains a read-only **Networks** tab; (e) `cargo bench -p mind-core --bench networks`. |

## 2. Scope & parity definition

### 2.1 In scope

Three resource networks in `mind-core`, behavior-complete against upstream:

1. **Power** — `PowerGraph` (producer/consumer/battery lists, `powerBalance`, `update`, `distributePower`, battery charge/discharge, split/merge/reflow, `transferPower`/`energyDelta`), the `PowerGraph` BFS/topology rules (`getPowerConnections`, `insulated` raycast, `addGraph` merge-smaller-into-larger), `PowerModule` semantics (`status`, `init`, `links`, graph membership), and the graph updater entity (`PowerGraphUpdater`, `Groups.powerGraph`, plan-05 schedule slot 8).
2. **Power blocks** — `PowerBlock`, `PowerDistributor`, `PowerNode`/`LongPowerNode` (config links, auto-link search, `linkValid`, `insulated`, drop/pickup behavior), `BeamNode` (direction links, `couldConnect`, tile-change gating), `Battery` (buffered charge + `overwrote` transfer), `PowerDiode` (front/back graph transfer), the generator family (`PowerGenerator`, `ConsumeGenerator`, `ThermalGenerator`, `SolarGenerator`, `ImpactReactor`, `NuclearReactor`, `VariableReactor`, `HeaterGenerator`), `LightBlock` metadata, sandbox `PowerSource`/`PowerVoid`.
3. **Power consumers/producers contract** — the execution side of `ConsumePower.requestedPower`/`efficiency`/`buffered` semantics defined by 07: `shouldConsumePower`, efficiency passes, buffered charging, `getPowerProduction`, `delta()`/`edelta()`/`timeScale()`; the power half of `Building.updateConsumption` is 07's, the values it reads/writes are the ones specified in §3.4.
4. **Power damage/lightning hooks** — reactor/impact explosion conditions and side-effect dispatch (`onDestroyed` → `createExplosion` → damage/ignition/fireballs/puddles/scorch/shake), destruction-driven graph splitting (any damage source: bullets, lightning, fire, deconstruct), `PowerGenerator`/`NuclearReactor`/`VariableReactor` explosion triggers. `Damage`, `Fires`, `Puddles`, `Lightning`, `Bullet` are plan 10; plan 09 provides the trigger points and calls plan-10 functions.
5. **Liquids** — `LiquidBlock`, `Conduit`/`ArmoredConduit` (transfer-rate formula, leaks, junction/bridge replacement, pipe autotiling via `Autotiler`/`TileBitmask` from 03/08), `LiquidRouter`, `LiquidJunction` (`getLiquidDestination` recursion), `LiquidBridge` + `DirectionLiquidBridge`, sandbox `LiquidSource`/`LiquidVoid`, `LiquidModule` (`current`, `currentAmount`, per-liquid array, `add/remove/set/reset/clear/each/sum/checkArrayCapacity`, `updateFlow`/`getFlowRate`/`hasFlowLiquid`), the shared liquid movement primitives (`moveLiquid`, `moveLiquidForward`, `dumpLiquid`, `transferLiquid`, `canDumpLiquid`), and liquid data queries consumed by plan 16 rendering.
6. **Heat (Erekir)** — `HeatBlock`/`HeatProducer`/`HeatConsumer`/`HeatConductor` (incl. `splitHeat`), `HeatCrafter` (requirement/overheat efficiency), `calculateHeat(sideHeat, cameFrom)` incl. contact-point math, orientation predicate, cycle guard (`cameFrom`, `updateId` memoization), `HeatProducer`/`HeaterGenerator` heat ramp, `NuclearReactor`/`VariableReactor` heat couplings.

### 2.2 "Done" means

- All §7a ported power tests pass under `cargo test -p mind-core` with the documented delta adaptation; the fixture family is a public `mind_core::fixtures::power` harness.
- The four headless scenarios of §7b pass with golden dumps/checksums committed under `tests/golden/`.
- The MCP scenario of §7c passes with a screenshot; `power_debug`/`liquid_debug`/`heat_debug` reflect live sim state.
- `mind-headless bench power│liquid│heat` is within the §7d budgets; alloc-audit reports **zero allocations** in steady-state network updates after warmup.
- `power_network_determinism` replays identically across processes and worker counts; checksums unchanged after optimizations.
- `mind-core` remains Godot-free and tokio-free; all ported files carry the GPL header.

### 2.3 Deliberate deviations

| # | Upstream | Port | Reason |
|---|---|---|---|
| 1 | `Time.delta` is variable; the power test fixture fixes it at `0.5` | Sim passes `delta = 1.0` (D8). Network algorithms take `delta: f32` as an explicit parameter (`PowerGraph::update(delta)`, `move_liquid` uses `build.delta()`); tests may pass `0.5`/`1`/`2`. | Fixed-step parity; keeps every delta-sensitive Java case ported instead of collapsed. Golden numbers are recomputed Rust-vs-Rust, never Java-vs-Rust (§9 of HIGH_LEVEL). |
| 2 | `PowerGraph` is an object referenced by identity; each building owns a default graph | `PowerGraphId { slot, generation }` into a `PowerGraphs` arena resource; `PowerModule.graph: PowerGraphId`. A monotonic debug `graph_id` mirrors `lastGraphID`. | Rust ownership; deterministic and save-safe (graphs are rebuilt from `links` on load; the ID is never serialized). Generation reuse bounds memory under graph churn. |
| 3 | Static `PowerGraph.queue/outArray1/outArray2/closedSet` | `PowerScratch` resource (`queue: VecDeque<Entity>`, `out1/out2/temp: Vec<Entity>`, `closed: IdSet`) with reused capacity. | No globals in `mind-core` (HLP §6.1); allocation-free graphs. |
| 4 | `LiquidModule`'s flow window is `static` and shared by all modules | `LiquidFlowCache` resource with identical single-watched-building semantics; UI drives `updateFlow` (plan 14/15). See UD-09-2. | Same observable behavior while keeping sim state explicit. |
| 5 | Java `WindowedMean` from Arc | `mind_core::math::windowed_mean::WindowedMean` (60-sample power balance + item/liquid flow windows). | Shared with plan 08 `ItemModule`; single implementation (reconcile R9). |
| 6 | `Liquid.drawPuddle`/`update`/`react` and `Puddle` sim | Puddle/Fires are plan 10; plan 09 only calls plan-10 deposit/removal hooks and exposes liquid metadata. `Liquid.getAnimationFrame` is view-only (plan 16). | Boundaries of HLP §3 plan rows. |
| 7 | Reactor explosion bodies (`onExplosion`) live on the block | Plan 09 owns trigger/conditions and dispatches to plan-10 `Damage`/`Fires`/`Puddles` and plan-17 effects; `Bullets.fireball` creation is plan 10. | 10 owns effects/damage; avoids duplicate logic. |
| 8 | Power/liquid bars are built inside block classes (`setBars`, `makePowerBalance`) | Plan 09 exposes pure functions returning bar values (`PowerBarData { label_key, fill, color }`); plan 14 renders. | UI boundary. |
| 9 | `GenericCrafter` base behavior used by `HeatProducer`/`HeatCrafter` | Assumed owned by 07 (block behavior framework). If 07 declines, plan 09 ships the minimal craft loop (`shouldConsume`, `efficiency`, progress, `consume`) it needs. | Reconcile R3; 07 not written. |
| 10 | `Turret.heatReq = calculateHeat(sideHeat)` inside plan 10's `Turret` | Plan 09 exports `calculate_heat` + `HeatScratch`; plan 10 calls it. | Boundary with 10. |
| 11 | `PowerGraph.remove()` calls `graph.update()` mid-mutation | Ported exactly; document reentrancy. Placement/breaking and split runs synchronously under exclusive world access (plan 07). | Behavior parity; heat/"direct consumer loses power immediately after disconnect". |

### 2.4 Owned by other plans (do not implement here)

| Area | Owner |
|---|---|
| `Block` flags/consumers framework, `Consume` traits, `consumePower*` builders, `updateConsumption`, `Building` entity, proximity, modules data layout, config framework, placement/breaking | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` |
| `ItemModule`, item transport, `ItemBridge`/`DirectionBridge` bases, `Autotiler` trait, conveyors/ducts | `08_LOGISTICS_IMPLEMENTATION_PLAN.md` |
| `TileBitmask` tables, autotile region generation | `03_ASSETS_IMPLEMENTATION_PLAN.md` |
| `Damage`, `Fires`, `Puddles`, `Lightning`, bullets, turret power/heat consumption (`PowerTurret`, `LiquidTurret`, `Turret.heatReq`) | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` |
| `Rules` (`solarMultiplier`, `lighting`, `ambientLight`, `reactorExplosions`, `damageExplosions`, `env`), `Teams`/`TeamData`, `buildingTree` (QuadTree), sector rules | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` |
| Power/liquid/heat bars, flow-rate UI, connection counters | `14_UI_IMPLEMENTATION_PLAN.md` |
| Lasers, fluid frames, `DrawPower`, `DrawHeatInput`/`DrawHeatOutput`, `DrawLiquidRegion`, conduit liquid rendering, `liquidPressure` visuals | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Building module IO bits (`PowerModule`/`LiquidModule` region layout, legacy `read(Reads, legacy)` paths), revision manifests | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` + `07` |
| Golden dumps, CI gates, benchmark baselines | `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` |

## 3. Target design

All names below are final unless marked otherwise. Module paths mirror Mindustry packages (`mind_core::world::blocks::power::...`), per HLP §6.1. `mind-core` uses `bevy_ecs` as a library; no `HashMap` iteration in sim paths (lookup only); no allocation in steady-state network updates (HLP §2.4, §6.1).

### 3.1 Module layout

```
client/rust/mind-core/src/
  world/blocks/power/
    mod.rs                 # PowerBlock, PowerDistributor, flags, PowerPlugin registration
    graph.rs               # PowerGraph, PowerGraphId, PowerGraphs, PowerScratch, WindowedMean use
    module.rs              # PowerModule network ops (add/remove/links/serialization glues)
    lifecycle.rs           # update_power_graph / power_graph_removed / change_team power path
    node.rs                # PowerNode + PowerNodeBuild link logic, config callbacks
    long_node.rs           # LongPowerNode
    beam_node.rs           # BeamNode + BeamNodeBuild direction links
    battery.rs             # Battery + BatteryBuild
    diode.rs               # PowerDiode + PowerDiodeBuild
    generator.rs           # PowerGenerator + GeneratorBuild + explosion hooks
    consume_generator.rs   # ConsumeGenerator + ConsumeGeneratorBuild
    thermal.rs             # ThermalGenerator
    solar.rs               # SolarGenerator
    impact_reactor.rs      # ImpactReactor
    nuclear_reactor.rs     # NuclearReactor
    variable_reactor.rs    # VariableReactor
    heater_generator.rs    # HeaterGenerator
    light_block.rs         # LightBlock (metadata; light = 16)
    sandbox.rs             # PowerSource, PowerVoid
    bars.rs                # make_power_balance / make_battery_balance / connection bar data
    tests.rs               # ported power tests (see §7a)
  world/blocks/liquid/
    mod.rs                 # LiquidBlock + LiquidBuild
    conduit.rs             # Conduit + ConduitBuild (+ ArmoredConduit)
    router.rs              # LiquidRouter
    junction.rs            # LiquidJunction
    bridge.rs              # LiquidBridge (needs 08 ItemBridge)
    direction_bridge.rs    # DirectionLiquidBridge (needs 08 DirectionBridge)
    sandbox.rs             # LiquidSource, LiquidVoid
    move.rs                # move_liquid/dump_liquid/get_liquid_destination/transfer_liquid
    tests.rs
  world/blocks/heat/
    mod.rs                 # HeatBlock trait, HeatConsumer trait, HeatPlugin
    producer.rs            # HeatProducer
    conductor.rs           # HeatConductor + splitHeat
    calculate.rs           # calculate_heat + HeatScratch
  world/blocks/production/
    heat_crafter.rs        # HeatCrafter (GenericCrafter base from 07)
  world/modules/
    power_module.rs        # PowerModule data (07 owns; 09 implements network ops) [reconcile]
    liquid_module.rs       # LiquidModule data (07 owns) + LiquidFlowCache (09)
  entities/comp/
    power_graph_updater.rs # PowerGraphUpdater component + def registration
  fixtures/
    power.rs               # PowerTestFixture port: FakeProducer/FakeBattery/FakeDirectConsumer/PowerHarness
  world/spatial/
    quad_tree.rs           # deterministic building QuadTree (default UD-09-1) [reconcile 11/12]
  world/network_state.rs   # NetworkState dump/serialization for harness + debug API
```

### 3.2 Resources and components

```rust
#[derive(Resource, Default)]
pub struct PowerGrids {
    graphs: Vec<Option<(u32 /*generation*/, PowerGraph)>>,
    free_slots: Vec<u32>,
    pub scratch: PowerScratch,
    next_graph_id: u32,          // mirrors PowerGraph.lastGraphID (debug only)
}

#[derive(Resource, Default)]
pub struct PowerScratch {
    queue: VecDeque<bevy_ecs::entity::Entity>,
    out1: Vec<Entity>, out2: Vec<Entity>, temp: Vec<Entity>,
    closed: mind_core::util::IdSet,   // SimId bitset, reused
}

pub struct PowerGraph {
    pub debug_id: u32,
    pub producers: SmallVec<[Entity; 16]>,
    pub consumers: SmallVec<[Entity; 16]>,
    pub batteries: SmallVec<[Entity; 16]>,
    pub all: SmallVec<[Entity; 16]>,
    pub updater: Option<Entity>,
    power_balance: WindowedMean,   // capacity 60
    last_power_produced: f32, last_power_needed: f32, last_power_stored: f32,
    last_scaled_power_in: f32, last_scaled_power_out: f32, last_capacity: f32,
    energy_delta: f32,             // diode workaround
}

// 07-owned data consumed by 09 (contract §3.4):
// PowerModule { status: f32, init: bool, graph: PowerGraphId, links: SmallVec<[i32; 4]> }
// LiquidModule { liquids: Vec<f32>, current: LiquidId, /* flow state via LiquidFlowCache */ }

#[derive(Component)]
pub struct PowerGraphUpdater { pub graph: PowerGraphId }

#[derive(Resource, Default)]
pub struct LiquidFlowCache { /* per-liquid WindowedMean[6], sums, display, bits; flow_timer */ }

#[derive(Resource, Default)]
pub struct HeatScratch {
    pub side_heat: [f32; 4],
    pub came_from: mind_core::util::IdSet,   // building SimIds
    pub depth: u32,                          // debug cycle guard
}
```

`PowerGraphId` is `{ slot: u32, generation: u32 }`; the arena never hands out a stale ID. `graph_id` (debug) is monotonic and only appears in dumps/tests. `Groups.power_graph` (plan 05) stores updater entities; plan 09 registers the `PowerGraphUpdater` entity def with `serialize = false, genio = false, pooled = true` (matching upstream `@EntityDef(serialize=false, genio=false)`), resetting `graph` on pool reuse.

### 3.3 Schedule placement (plan-05 slots)

| Slot | System | Behavior |
|---|---|---|
| `EntitySet::UpdatePowerGraph` (#8, run `!editor`) | `update_power_graphs` | Iterates `Groups.power_graph` in stable slab order; calls `graph.update(delta)` for each. This is the only per-tick power system. |
| `EntitySet::UpdateBuildings` (#9, run `!editor`) | 07's `update_buildings` | Runs every building's `update` → `updateConsumption` (07) → `updateTile` (09 for conduits/generators/heat producers/crafters). Liquid transfer and heat pulls happen here, in `Groups.build` slot order. |
| Placement/breaking (plan 07 command path) | `lifecycle::{update_power_graph, power_graph_removed}` | Synchronous with tile change; graph merge/split/reflow and `LiquidJunction` destination resolution are pure functions called by 07 hooks. |
| Map load (`IoSet::Apply`) | `lifecycle::rebuild_all_graphs` | After entities load, proximity rebuild re-adds each building to a fresh graph and merges — identical to `World.endMapLoad` → `onProximityAdded`. |
| (none) | heat | Pull-based inside `updateTile` via `calculate_heat`, memoized by `GameState.update_id` on conductors. No schedule slot (parity). |
| (none) | liquid flow window | Advanced only when UI watches a building (`LiquidFlowCache::update_flow`); never inside the tick. |

Invariant: no sim system reads `Time.time`; heat/graph math uses `SimClock::time`, `update_id`, and explicit `delta`.

### 3.4 Interface contract expected from 07 (not yet written — reconcile)

Plan 09 assumes 07 ships (all names subject to 07's final spellings; the orchestrator maps them once 07 lands):

```rust
// components
pub struct BuildingCore {
    pub block: BlockId, pub rotation: u8, pub team: u8,
    pub enabled: bool, pub dead: bool,
    pub efficiency: f32, pub optional_efficiency: f32, pub potential_efficiency: f32,
    pub should_consume_power: bool, pub time_scale: f32, pub time_scale_duration: f32,
    pub proximity: SmallVec<[Entity; 8]>, pub tile: TilePos, pub cdump: u8,
}
pub struct BuildingModules {   // exactly one of each present iff block flag
    pub items: Option<ItemModule>, pub liquids: Option<LiquidModule>, pub power: Option<PowerModule>,
}
// behavior API used by 09:
impl BuildingCore {
    pub fn should_consume(&self, world) -> bool;
    pub fn delta(&self) -> f32;                 // Time.delta * timeScale
    pub fn edelta(&self) -> f32;                // efficiency * delta
    pub fn efficiency_scale(&self) -> f32;
    pub fn cheating(&self, world) -> bool;
    pub fn get_power_production(&self, world) -> f32;   // default 0
}
// consumer contract (07)
trait Consume {
    fn optional(&self) -> bool; fn ignore(&self) -> bool; fn update(&self) -> bool;
    fn efficiency(&self, b: Entity, world: &World) -> f32;
    fn efficiency_multiplier(&self, b: Entity, world: &World) -> f32;
    fn trigger(&self, b: Entity, world: &mut World);
    fn update_consumer(&self, b: Entity, world: &mut World);
}
struct ConsumePower { pub usage: f32, pub capacity: f32, pub buffered: bool }
impl ConsumePower {
    pub fn requested_power(&self, b: Entity, world: &World) -> f32; // buffered ? (1-status)*capacity : usage*(shouldConsume?1:0)
}
// config framework (07)
block.config_integer(|world, entity, pos: i32| { ... });
block.config_point2_array(|world, entity, points: &[Point2]| { ... });
// proximity hooks (07) invoke:
lifecycle::on_proximity_added(world, entity);   // -> update_power_graph(entity)
lifecycle::on_proximity_removed(world, entity); // -> power_graph_removed(entity)
// world (06 via 07): world.build(pos: i32) -> Option<Entity>, world.raycast_tiles(.., pred),
// Point2::{pack,unpack,x,y}, Edges::get_edges(size), tile.nearby(rot)
// config: BuildingCore::configure_any(value) executes reducers from io/network (04/21)
```

If 07's actual shapes differ, keep behavior and fix the seam in the merge commit; do not fork module structs.

### 3.5 Power graph semantics (exact port)

`PowerGraphs` API (all operations take `&mut PowerGrids`, `&mut World` or a narrow param bundle):

- `add(build)`: skip if no `PowerModule`; if `module.graph != this || !module.init`: remove the old graph's updater if switching, set `graph = this`, `init = true`, push `all`; classify into `producers` + `consumers` (outputs & consumes & `!buffered`), `batteries` (outputs & consumes & `buffered`), `producers` (outputs only), `consumers` (consumes only, `cons_power.is_some()`).
- `add_graph(other)`: identity no-op; merge the smaller `all` into the larger (recursive swap); remove the absorbed graph's updater entity; `add` every building of the absorbed graph; `check_add`.
- `check_add` / `clear`: ensure/remove updater entity (spawn `PowerGraphUpdater { graph }`, add to `Groups.power_graph`).
- `reflow(tile)`: BFS from `tile` over `get_power_connections` with `closed` bitset; `add` + `check_add` each visited building (ports `PowerGraph.reflow`).
- `remove(tile)`: for every power connection whose graph is still `this`, allocate a new graph, BFS the branch excluding `tile` and buildings already reassigned, `add` each, then `graph.update(delta)` once (so direct consumers without producers lose status immediately); remove `this` updater. The old graph is invalid; all members were reassigned.
- `remove_list(build)` (tests only) and `clear`.

`update(delta)` exact order:
1. `if !consumers.is_empty() && cheating(consumers[0])`: set every consumer `power.status = 1`, `last_needed = last_produced = 1`, return.
2. `needed = Σ should_consume_power ? requested_power × build.delta()`; `produced = Σ get_power_production × build.delta()` over producers (in list order).
3. `last_needed/last_produced = …`; `last_scaled_in = (produced + energy_delta)/delta`; `last_scaled_out = needed/delta`; `last_capacity = total_battery_capacity`; `last_stored = battery_stored`; `power_balance.add((produced - needed + energy_delta)/delta)`; `energy_delta = 0`.
4. If any list non-empty: if `needed != produced` (float equality, `Mathf.equal` parity): if needed > produced, `used = use_batteries(needed - produced)`, `produced += used`, `last_produced += used`; else `charged = true`, `produced -= charge_batteries(produced - needed)`.
5. `distribute_power(needed, produced, charged)`: coverage = `zero(needed) && zero(produced) && !charged && zero(last_stored) ? 0 : zero(needed) ? 1 : min(1, produced/needed)`; per consumer in order: buffered (`capacity != 0`): `maximum_rate = requested_power × coverage × delta`, `status = clamp(status + maximum_rate/capacity)`; unbuffered valid: `status = coverage`; invalid: `status = min(1, produced/(needed + usage×delta))`, NaN → 0.
6. `get_satisfaction` = `zero(produced) ? 0 : zero(needed) ? 1 : clamp(produced/needed)`.
7. Battery helpers exactly as Java (`use_batteries` proportional `status *= 1 - min(1, needed/stored)`; `charge_batteries` computes `charged_percent = min(excess/capacity, 1)` **before** the `capacity == 0` return, then `status += (1-status) × charged_percent` only when `capacity > 0`; `transfer_power` keys `energy_delta`).
8. All sums are `f32` in list order; no reordering, no parallel reduction (determinism).

### 3.6 Node / beam / battery / diode semantics

- `PowerNode`: no `update`; config `Integer` (packed pos) link/unlink with reflow + `update` on both branches; config `Point2[]` clears then re-applies; `placed` autolinks (unless `net.client`-equivalent); `dropped` clears links + `update_power_graph`; `on_configure_build_tapped` handles link/double-tap find/clear. `link_valid` checks power/connected/team/`sameBlockConnection`/range (`overlaps`) and `maxNodes`; `insulated(tile, other)` raycasts tiles between them and blocks on `is_insulated`. `get_potential_links` gathers candidates from the team building tree, excludes graphs already represented by adjacent/self ties, excludes adjacent buildings (`Edges::get_edges`), sorts by `(is PowerNode desc, dst2 asc)` and takes the first `maxNodes`. Update order of candidate insertion must match the tree; see UD-09-1.
- `LongPowerNode`: `maxNodes`, `drawRange = false`; build adds `warmup` lerp when links exist (sim state, save via revision); beam/glow regions are view.
- `BeamNode`: links are 4 cardinal `Building[]`/`Tile[]` computed from `updateTile` only when `world.tile_changes` changed. `couldConnect(direction, target, x, y)` scans up to `range` tiles, stops at insulated walls or the first powered block; `updateDirections` reflows/unlinks/link-merge exactly like the node path; `status()` maps balance to `BlockStatus`.
- `Battery`: `outputsPower && consumesPower`, `flags = battery`, `update = false`; `warmup = power.status`; `overwrote` adds previous buffered fractions; `status()` thresholds `equal(status, 0/1, 0.001)`.
- `PowerDiode`: `insulated`, no power module; per tick (its `updateTile`) compares `stored/capacity` ratios of `back()`/`front()` graphs and moves `amount = clamp((target_pct × front_capacity - front_stored)/2, 0, front_capacity - front_stored)` via `back.transfer_power(-amount)` / `front.transfer_power(amount)`; `bar()` exposes stored/total capacity.

### 3.7 Generators (behavior)

- `PowerGenerator`: `GeneratorBuild { generate_time, production_efficiency }`; `get_power_production = enabled ? power_production × production_efficiency : 0`; `warmup`; revision 1 serialization of both floats; `onDestroyed` triggers reactor explosion if `rules.reactor_explosions` and `warmup ≥ explosion_min_warmup`. Explosion helper `on_explosion`: `Damage` (plan 10), per-tile ignition (`Time.run(dst/speed)` → `Fires::create`, plan 10) and prop break (`ConstructBlock::deconstruct_finish`, plan 07), fireball bullets (`Bullets::fireball.create_net`, plan 10), `explodeEffect`/sound (plan 17), puddle deposits (plan 10), screen shake/scorch (plan 17). Only the trigger/parameters are 09.
- `ConsumeGenerator`: `filter_item`/`filter_liquid` found at init; `efficiencyMultiplier` from the filter; `warmup` lerp; `productionEfficiency = efficiency × efficiencyMultiplier`; `itemDurationMultiplier`; periodic `consume()` when `generateTime <= 0`; `outputLiquid` output + dump + `explodeOnFull` kill + `GeneratorPressureExplodeEvent`; `consumeTriggerValid = generateTime > 0`. `consume()` and consumer triggers are 07.
- `ThermalGenerator`, `SolarGenerator` (rules fields from 12), `ImpactReactor` (`warmup^5`, `timerUse`, `Trigger.impactPower`), `NuclearReactor` (heat model: `heat += fullness × heating × min(delta,4)`; fuel timer scaled by `heat × heatConsumeRate`; ambient cooldown; coolant removal `maxUsed = min(currentAmount, heat/coolantPower)`, `heat -= maxUsed × coolantPower`; `heatProgress` approach; `heat ≥ 0.999` → `Trigger.thoriumReactorOverheat` + `kill()`; `HeatBlock` outputs `heatProgress`, `heatFrac = heatProgress/heatOutput`), `VariableReactor` (heat pull, `instability` approach, `efficiency *= heat/maxHeat`, kill at `instability ≥ 1`), `HeaterGenerator` (`HeatBlock`, heat approach; extends `ConsumeGenerator`).
- Sandbox `PowerSource` (extends `PowerNode`, `maxNodes = 100`, always-on production) / `PowerVoid` (`consumePower(MAX_VALUE)`, stats/printer special cases).

### 3.8 Liquid transfer and pipe autotiling

- `move_liquid(next, liquid)` and `dump_liquid(liquid, scaling, output_dir)`/`move_liquid_forward(leaks, liquid)` port `BuildingComp` exactly: same team, `has_liquids`, per-liquid fractions, `liquid_pressure` multiplier, `min(clamp(fract - ofract) × capacity, available)` cap, capacity clamp, `accept_liquid`/`handle_liquid`; the differing-liquid reaction branch dispatches to plan-10 `Fx`/damage hooks (temperatures/flammability thresholds preserved). `get_liquid_destination` default is self; `LiquidJunction` recurses along `(source.relativeTo + 4) % 4` until a non-accepting, non-junction block.
- `ConduitBuild::update_tile`: `smooth_liquid` lerp (render value), `move_liquid_forward(leaks, current)` on a 1-tick timer, `no_sleep`/`sleep` parity. `accept_liquid` rejects input from the output side. `onProximityUpdate` recomputes `blendbits/xscl/yscl/blending/capped/backCapped`; autotiling uses `Autotiler::build_blending` with `TileBitmask` (03) and matches the conveyor implementation (08) so masks stay identical.
- `LiquidRouter` dumps current; `LiquidBridge` moves liquid over `ItemBridge`'s link when `warmup ≥ 0.25`, dumps otherwise; `DirectionLiquidBridge` uses `DirectionBridge::find_link`/`occupied` slots. Both are scheduled at M5 behind 08.
- `LiquidModule` network ops: `add/remove/set/reset/clear/each/sum` and `check_array_capacity` sized to `Content.liquids.len()`; `current` selection matches Java (`set` switches when `amount >= liquids[current.id]`; `remove` never goes negative). Flow: `LiquidFlowCache` mirrors the static `cacheFlow/cacheSums/displayFlow/cacheBits/flowTimer`; `wind_get(1, 10)` poll + `get(15)` visual refresh; `get_flow_rate = displayFlow[id] × 60` (u/s), `< 0` means not ready.

### 3.9 Heat network (pull-based, exact)

`calculate_heat(self, side_heat, came_from, world, scratch)`:
- Clear `side_heat`/`came_from`; for each proximity build, same team, `HeatBlock` component present:
  - `split = other is HeatConductor && split_heat`;
  - orientation gate `!rotate || (!split && (relative_to + 2) % 4 == other.rotation) || (split && relative_to != other.rotation)`;
  - cycle gate `!(other is HeatConductor && other.came_from.contains(self.id))`;
  - `diff = min(|dx|,|dy|)/tilesize`; `contact = min((size/2 + other.size/2 - diff) as i32, min(other.size, size))`;
  - `add = other.heat()/other.size × contact` (`/3` when split); `side_heat[relative_to % 4] += add`; total += add;
  - record `self.id` and, for conductors, their whole `came_from` set; if `other` is a conductor, call `other.update_heat()` (recursion, memoized).
- `HeatConductor::update_heat` no-ops within the same `GameState.update_id`, then `heat = enabled ? calculate_heat(side_heat, came_from) : 0`.
- `HeatProducer`/`HeaterGenerator` ramp `heat = approach_delta(heat, heat_output × efficiency, warmup_rate × delta)`.
- `HeatCrafter`: `heat = calculate_heat`; `should_consume = heat > 0 && super`; `warmup_target = clamp(heat/heat_requirement)`; `efficiency_scale = min(clamp(heat/req) + max(heat-req,0)/req × overheat_scale, max_efficiency)`; `status = noInput` when heat ≤ 0; `sense(heat)` = heat.
- `HeatScratch` capacity reused; recursion is bounded by `came_from`; a debug `depth` counter guards against pathological graphs (debug builds panic, release builds stop recursing and log).

### 3.10 Determinism & allocation rules

- `PowerGrids`, `LiquidFlowCache`, `HeatScratch` are the only mutable network state and are `Resource`s — no statics, no TLS in sim paths.
- List iteration is insertion-stable (`SmallVec`, `Vec`); `addUnique`/`contains` linear; `closed` is a bitset keyed by `SimId`.
- Graph merge/split recursion and BFS preserve Java visit order (`queue` FIFO, `out` filled in `proximity` order then `links` order).
- Steady-state `UpdatePowerGraph` and liquid transfer allocate zero bytes: no `Vec` growth (capacity retained), no `Box`, no string formatting, no `HashMap`.
- Heat recursion writes only into `HeatScratch`; `sideHeat` is `[f32; 4]`.
- Network state never reads wall clock or render state; `Liquid::get_animation_frame` (view) stays out of `mind-core` sim systems.

### 3.11 NetworkState dump (harness/debug)

`world/network_state.rs` produces a stable, sorted JSON for scenarios and the MCP bridge:

```rust
pub struct NetworkState {
    pub tick: u64,
    pub graphs: Vec<GraphState>,      // sorted by debug_id
    pub liquids: Vec<BuildingState>,  // sorted by (y, x)
    pub heat: Vec<BuildingState>,
}
pub struct GraphState { pub id: u32, pub producers: u32, pub consumers: u32, pub batteries: u32,
    pub satisfaction: f32, pub produced: f32, pub needed: f32, pub stored: f32, pub capacity: f32,
    pub balance: f32, pub energy_delta: f32 }
```

`sim.checksum()` includes network-relevant `FieldKind` fields through plan 05's entity hashing (`power.status`, module liquids); graphs themselves are derived and excluded (rebuilt on load).

### 3.12 Godot / STDB surfaces

- **Godot:** no new scenes. `mind-gdext` extends the plan-00/05 `MindSim` autoload with `power_debug(x,y) -> Dictionary`, `liquid_debug(x,y)`, `heat_debug(x,y)`, `network_counts()`, and debug actions `dev_place`/`dev_break`/`dev_add_item` (all delegate to the same `SimCommand` path). The Networks inspector tab calls `network_state()` and renders counts/labels; all logic stays in Rust.
- **STDB:** none. Building/node-link/config state travels as plan-05 `SimCommand::Place/Break/Configure` through plan 21's relay; `PowerModule.links` is building IO (plan 04), not a table. Cheap server validation (plan 21) may check block existence/ownership only; no network simulation server-side (D2).

### 3.13 Sibling interface contracts to reconcile (06/07/08 absent at draft time)

| Needed from | Contract | Default if absent |
|---|---|---|
| 07 | Building entity/components/modules/config/proximity/consumer framework (§3.4) | 09 declares the traits in a `07_contract.rs` shim and the orchestrator merges into 07's types; never duplicate components |
| 07 | `GenericCrafter` behavior for `HeatProducer`/`HeatCrafter`/`ConsumeGenerator` test fixture | 09 test fixtures use local fake crafters; `HeatProducer`/`HeatCrafter` wait for 07's base (M4 gate). See R3 |
| 07 | Building destruction path calls `on_proximity_removed` before removal (so graphs split on any damage source) | 09 exposes `power_graph_removed`; 10/07 must call it; scenario `power_graph_split_merge` fails loudly if not |
| 06 | `Point2::{pack,unpack,x,y}`, `Edges::get_edges`, `World::raycast`, `tile.nearby(rot)`, tile positions | 09 uses 06 names; no local reimplementation |
| 08 | `ItemBridge`/`DirectionBridge` bases for `LiquidBridge`/`DirectionLiquidBridge`; `Autotiler` trait | bridge tasks gated at M5; until then liquid routers/junctions/conduits cover the scenario (R2) |
| 10 | `Damage`, `Fires::create`, `Puddles::deposit/remove`, `Bullets::fireball`, `Effect::shake/scorch` hooks; `calculate_heat` reuse in `Turret` | 09 defines thin function-pointer hooks (no-op in headless) that plan 10 replaces |
| 12 | `Rules::{solar_multiplier, lighting, ambient_light, reactor_explosions, damage_explosions, env}`, `TeamData::building_tree` | 09 declares read traits; test scenario stubs values |
| 14 | Consume `PowerBarData`/`FlowBarData`/`HeatBarData` | data functions return values; UI later |
| 16 | Query helpers `PowerGraph::{satisfaction, links_of}`, `LiquidModule::{current, current_amount, get}`, `HeatBlock::{heat, heat_frac}`, fluid frame index | these are the 09 public read API; renderer consumes |

## 4. Port map

| Mindustry source | Rust target | Notes |
|---|---|---|
| `world/blocks/power/PowerGraph.java` | `world/blocks/power/graph.rs` (`PowerGraph`, `PowerGraphId`, `PowerGraphs`, `PowerScratch`) | Object identity → arena IDs; static scratch → resource; exact BFS/merge/split/`update` order; `WindowedMean(60)`; `toString` → debug dump. |
| `world/blocks/power/PowerBlock.java` | `world/blocks/power/mod.rs` (`PowerBlock`) | Base flags (`update/solid/hasPower/group=power`). |
| `world/blocks/power/PowerDistributor.java` | `world/blocks/power/mod.rs` (`PowerDistributor`) | `consumesPower=false`, `outputsPower=true`. |
| `world/blocks/power/PowerNode.java` | `world/blocks/power/node.rs` | Config Integer/Point2[]; autolink/`linkValid`/`insulated`; laser regions are 16. |
| `world/blocks/power/LongPowerNode.java` | `world/blocks/power/long_node.rs` | Adds `warmup`; laser fallbacks are 03/16. |
| `world/blocks/power/BeamNode.java` | `world/blocks/power/beam_node.rs` | 4-direction links on `world.tile_changes`; `couldConnect`; `status()`. |
| `world/blocks/power/Battery.java` | `world/blocks/power/battery.rs` | `DrawPower` view → 16; `overwrote`, `status`. |
| `world/blocks/power/PowerDiode.java` | `world/blocks/power/diode.rs` | `transferPower`/`energyDelta`; `insulated`; front/back bar data. |
| `world/blocks/power/PowerGenerator.java` | `world/blocks/power/generator.rs` | `GeneratorBuild`, revision 1, explosion triggers → 10/17. |
| `world/blocks/power/ConsumeGenerator.java` | `world/blocks/power/consume_generator.rs` | `itemDuration`, filters, output liquid, explode-on-full. |
| `world/blocks/power/ThermalGenerator.java` | `world/blocks/power/thermal.rs` | `sumAttribute(heat)` via floor attributes (06). |
| `world/blocks/power/SolarGenerator.java` | `world/blocks/power/solar.rs` | Reads plan-12 rules fields. |
| `world/blocks/power/ImpactReactor.java` | `world/blocks/power/impact_reactor.rs` | `warmup^5`, `timerUse`, `Trigger.impactPower` (plan 05 events). |
| `world/blocks/power/NuclearReactor.java` | `world/blocks/power/nuclear_reactor.rs` | Heat model, coolant, meltdown kill, `HeatBlock`. |
| `world/blocks/power/VariableReactor.java` | `world/blocks/power/variable_reactor.rs` | Instability, `HeatConsumer`, overheat explosion. |
| `world/blocks/power/HeaterGenerator.java` | `world/blocks/power/heater_generator.rs` | Heat ramp; `DrawHeatOutput` → 16. |
| `world/blocks/power/LightBlock.java` | `world/blocks/power/light_block.rs` | Metadata; light render → 16. |
| `world/blocks/sandbox/{PowerSource,PowerVoid}.java` | `world/blocks/power/sandbox.rs` | Source `maxNodes=100`; void consumes `f32::MAX`. |
| Bar factories (`makePowerBalance`, `makeBatteryBalance`, connection bar) | `world/blocks/power/bars.rs` | Pure `PowerBarData`; bundle keys `bar.powerbalance`, `bar.powerstored`, `bar.powerlines`. |
| `world/modules/PowerModule.java` | `world/modules/power_module.rs` (data) + `blocks/power/module.rs` (ops) | Data layout owned by 07; graph/link/status semantics here. |
| `entities/comp/PowerGraphUpdaterComp.java` + `GroupDefs` `powerGraph` | `entities/comp/power_graph_updater.rs` + def in `PowerPlugin` | `serialize=false, genio=false, pooled=true`; `Groups.power_graph` from 05. |
| `entities/comp/BuildingComp.java` (power paths) | `blocks/power/lifecycle.rs` | `updatePowerGraph`, `powerGraphRemoved`, `getPowerConnections`, `changeTeam` power path. |
| `world/blocks/liquid/LiquidBlock.java` | `world/blocks/liquid/mod.rs` | `drawTiledFrames` → 16. |
| `world/blocks/liquid/Conduit.java` | `world/blocks/liquid/conduit.rs` | Transfer/leaks/timers/autotiling; junction/bridge replacement hooks. |
| `world/blocks/liquid/ArmoredConduit.java` | `world/blocks/liquid/conduit.rs` | `leaks=false`, `blendsArmored`. |
| `world/blocks/liquid/LiquidRouter.java` | `world/blocks/liquid/router.rs` | Dump current; tank/container variants share the class. |
| `world/blocks/liquid/LiquidJunction.java` | `world/blocks/liquid/junction.rs` | `getLiquidDestination` recursion. |
| `world/blocks/liquid/LiquidBridge.java` | `world/blocks/liquid/bridge.rs` | Extends 08 `ItemBridge`; M5. |
| `world/blocks/distribution/DirectionLiquidBridge.java` | `world/blocks/liquid/direction_bridge.rs` | Extends 08 `DirectionBridge`; M5. |
| `world/blocks/sandbox/{LiquidSource,LiquidVoid}.java` | `world/blocks/liquid/sandbox.rs` | Liquid config; source fills to capacity. |
| `world/modules/LiquidModule.java` | `world/modules/liquid_module.rs` + `LiquidFlowCache` | Data layout 07; flow window here; `updateFlow` UI-driven. |
| `entities/comp/BuildingComp.java` (liquid paths) | `world/blocks/liquid/move.rs` | `moveLiquid*`, `dumpLiquid*`, `transferLiquid`, `canDumpLiquid`, `getLiquidDestination`. |
| `world/blocks/heat/HeatBlock.java` | `world/blocks/heat/mod.rs` (`HeatBlock` trait) | `heat()`, `heat_frac()`. |
| `world/blocks/heat/HeatProducer.java` | `world/blocks/heat/producer.rs` | Ramp; `GenericCrafter` base from 07. |
| `world/blocks/heat/HeatConsumer.java` | `world/blocks/heat/mod.rs` (`HeatConsumer` trait) | `side_heat()`, `heat_requirement()`. |
| `world/blocks/heat/HeatConductor.java` | `world/blocks/heat/conductor.rs` | `updateHeat` memoized by `updateId`; `splitHeat`; `visualMaxHeat` bar. |
| `world/blocks/production/HeatCrafter.java` | `world/blocks/production/heat_crafter.rs` | Overheat efficiency; `GenericCrafter` base from 07. |
| `entities/comp/BuildingComp.java#calculateHeat` | `world/blocks/heat/calculate.rs` | Free function + `HeatScratch`; called by 09 and 10. |
| `type/Liquid.java` | `content` (02) + `world/blocks/liquid/move.rs` (react branch) | Fields/`init` in 02; puddle sim in 10; animation frame in 16. |
| `TileBitmask.java`, `Autotiler.java` | 03 / 08 | Consumed by `Conduit::blends`; identical masks to conveyors. |
| `world/consumers/ConsumePower.java` | 07 (framework); semantics specified here | `requestedPower`, `efficiency=status`, `buffered` cap. |
| `ConsumeGeneratorTests`/`DirectConsumerTests` fixture blocks (`GenericCrafter`) | 07 behavior + 09 fixtures | See §7a; fakes used where 07 is not yet available. |

## 5. Milestones & task breakdown

Each milestone ends with `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, and the named harness command. Evidence goes in the Changelog.

**M0 — Graph core + fixture (smallest vertical slice).**
`PowerGrids`/`PowerGraph`/`PowerScratch`; `PowerGraphUpdater` def + `Groups.power_graph` wiring; `fixtures::power` (`FakeProducer`/`FakeBattery`/`FakeDirectConsumer`/`create_tile`); `PowerGraph::update/add/remove_list`; `WindowedMean` port.
*Verify:* `power::graph::tests::{direct_consumer_satisfaction_is_as_expected, direct_consumption_stops_with_no_power}`; `mind-headless power boot --ticks 10`.

**M1 — Topology: merge/split/reflow + nodes.**
`add_graph`/`reflow`/`remove`/`check_add`/`clear`; `get_power_connections`; `PowerNode` config + autolink (`link_valid`/`insulated`/`get_potential_links`); `LongPowerNode`; `Battery`; `PowerDiode`; `update_power_graphs` system in plan-05 slot.
*Verify:* `power::graph::tests::{merge_prefers_larger, split_reassigns_all_members, reflow_terminates_on_cycle, battery_capacity_is_as_expected}`; `power::node::tests::{autolink_sorted, insulated_raycast_blocks}`; `power::diode::tests::transfers_half_difference`; `mind-headless power graph_split_merge`.

**M2 — Generators + sandbox + explosion hooks.**
`PowerGenerator`/`ConsumeGenerator`/`Thermal`/`Solar`/`Impact`/`Nuclear`/`Variable`/`HeaterGenerator`; `PowerSource`/`PowerVoid`; explosion trigger dispatch (10/17 hooks no-op until those plans land); `ConsumeGenerator`/`DirectConsumer` ported tests.
*Verify:* `consume_generator::tests::{liquid_input_deltas, item_flammability_inputs, efficiency_constant_within_item_duration}`; `power::tests::direct_consumer::{no_items_no_power_request, insufficient_items_no_power_request, sufficient_items_power_request}`; `mind-headless power battery_cycle`.

**M3 — Liquids.**
`LiquidModule` ops + `LiquidFlowCache`; `LiquidBlock`; `Conduit`/`ArmoredConduit` incl. autotiling against 03/08 tables; `LiquidRouter`/`Junction`; transfer primitives; `LiquidSource`/`LiquidVoid`.
*Verify:* `liquid::tests::{transfer_flow_formula, junction_destination_recursion, conduit_leak, router_dumps_current, flow_window_rates}`; `mind-headless liquid conduit_transfer`.

**M4 — Heat.**
`HeatBlock`/`HeatConsumer` traits; `calculate_heat` + `HeatScratch`; `HeatConductor` (memoization/split); `HeatProducer`; `HeatCrafter`; reactor heat couplings; `Turret` hook documented for 10.
*Verify:* `heat::tests::{contact_points_math, orientation_predicate, split_heat_thirds, cycle_guard, crafter_overheat_scale, nuclear_coolant_removal}`; `mind-headless heat network_equilibrium`.

**M5 — Bridges (needs 08) + IO + determinism.**
`LiquidBridge`/`DirectionLiquidBridge` on 08 bases; module IO parity (`PowerModule` links/status NaN clamp; `LiquidModule` leg/current paths from 04/07); revision manifest entries; `power_network_determinism` replay; `change_team` power path.
*Verify:* `liquid::tests::bridge_moves_when_warm`, `power_network_determinism.simlog` checksum golden, module round-trip tests; `07_contract` reconciliation note discharged or explicit.

**M6 — MCP + benches + exit.**
`MindSim` debug API + Networks inspector tab; `bench networks`; budgets recorded; §7c executed; exit checklist signed.
*Verify:* MCP scenario with screenshot; `bench power/liquid/heat` JSON artifacts; alloc-audit zero; `cargo tree -p mind-core` Godot/tokio-free.

## 6. Data & formats

### 6.1 `PowerModule` (owned by 07, semantics here)

Java layout for save/sync: `s(links.size)` then `i` per packed link position, then `f(status)`; read clamps NaN/Inf → 0. Link positions are `Point2.pack(x,y)` (plan 06); `links` preserves insertion order (`addUnique`). `init` is runtime-only (`false` on read until proximity rebuild merges graphs). Module presence is gated by plan-07 module bitmask bits.

### 6.2 `LiquidModule` (owned by 07, flow here)

- Storage `liquids: Vec<f32>` indexed by `LiquidId` (grown via `check_array_capacity`), `current: LiquidId`.
- Save layout: `s(count)`, then `(s(liquid_id), f(amount))` per positive entry; legacy read path (`ub` ids) owned by 04.
- `each`/`sum` iterate ascending liquid ID; `add` sets `current`; `remove` clamps at 0; `set` switches `current` when incoming `amount >= liquids[current.id]`.
- Flow window: `WindowedMean` capacity 6, poll interval 10 ticks, visual refresh 15 ticks, `displayFlow` init `-1`, `get_flow_rate = mean/10 × 60` u/s.

### 6.3 `WindowedMean`

`mind_core::math::windowed_mean::WindowedMean { values: Vec<f32>, index, used, sum }` with `add`, `mean`, `raw_mean`, `has_enough_data`, `reset`; exact Arc semantics (mean over inserted values; `raw_mean` = `sum/used`). Used by power balance (60) and liquid/item flow (6). Shared with plan 08 (R9).

### 6.4 Entity def

`PowerGraphUpdater`: component field `graph` is transient; `EntityDef { name: "PowerGraphUpdater", class_id: <from classids parity>, groups: [power_graph], pooled: true, serialize: false, genio: false }`. Spawned per graph, removed on merge/clear. Pool reuse resets `graph` in `EntityPool` reset fn (plan 05).

### 6.5 Golden/dump files

- `tests/golden/power_battery_cycle.json`, `power_graph_split_merge.json`, `liquid_conduit_transfer.json`, `heat_network_equilibrium.json` — `NetworkState` dumps at scripted ticks.
- `tests/golden/power_network_determinism.checksums` — checksum log every 60 ticks + command log.
- `tests/fixtures/power_flat_64.msav` — synthetic flat map used by all §7b scenarios (produced by plan 04/06; a synthetic in-memory grid until then).

### 6.6 No new formats

No new save region, no new STDB table/reducer/view, no new bundle keys beyond upstream `bar.*`/`stat.*` keys already in plan 03's bundle inventory. Reactor explosion `Trigger` variants (`impactPower`, `thoriumReactorOverheat`) are added to plan 05's `Trigger` enum.

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (`mindustry/tests/src/test/java/power/**` → `cargo test -p mind-core`)

| Mindustry test | Rust test | Notes |
|---|---|---|
| `PowerTestFixture` (`initializeDependencies`, `createFakeProducerBlock`, `createFakeBattery`, `createFakeDirectConsumer`, `createFakeTile`) | `mind_core::fixtures::power::{PowerHarness, FakeProducer, FakeBattery, FakeDirectConsumer, create_tile}` | Public harness reused by 10/23 scenarios. Names use a deterministic counter instead of `System.nanoTime()`. Fixture delta defaults to `1.0` (D8); graph functions accept explicit delta so the Java `2/1/0.5` cases still exist. |
| `PowerTests.PowerGraphTests.directConsumerSatisfactionIsAsExpected` (7 dynamic cases) | `world::blocks::power::graph::tests::direct_consumer_satisfaction_is_as_expected` | All 7 inputs incl. the `0.09/0.09-ε` float case; expected `produced×delta`, `needed×delta`, status. |
| `PowerTests.PowerGraphTests.batteryCapacityIsAsExpected` (9 dynamic cases) | `world::blocks::power::graph::tests::battery_capacity_is_as_expected` | Battery 100 max, 10/tick; exact `status` expectations with our delta; charge **before** the zero-capacity guard. |
| `PowerTests.PowerGraphTests.directConsumptionStopsWithNoPower` | `world::blocks::power::graph::tests::direct_consumption_stops_without_power` | `remove_list` + `add` + `update`; asserts status 1 → 0 and `cons_power.efficiency == 0`. |
| `DirectConsumerTests.noPowerRequestedWithNoItems` | `world::blocks::power::tests::direct_consumer::no_items_no_power_request` | Fixture `FakeFactory` with `consume_power(0.08)` + silicon/lead 30; asserts no request without items. |
| `DirectConsumerTests.noPowerRequestedWithInsufficientItems` | `...::insufficient_items_no_power_request` | Both single-resource variants. |
| `DirectConsumerTests.powerRequestedWithSufficientItems` | `...::sufficient_items_power_request` | Full materials; status = 1. |
| `ConsumeGeneratorTests.generatorWorksProperlyWithLiquidInput` (deltas 2/1/0.5 × 4 amounts) | `world::blocks::power::consume_generator::tests::liquid_input_deltas` | `ConsumeLiquidFlammable(oil)`; `Time.delta` becomes the explicit `delta` arg; `InputType.any` stays commented out upstream (recorded). |
| `ConsumeGeneratorTests.generatorWorksProperlyWithItemInput` (coal, blastCompound, sporePod, pyratite) | `...::item_flammability_inputs` | Null cases (0 items) and multi-amount cases; asserts remaining items and `productionEfficiency`. |
| `ConsumeGeneratorTests.efficiencyRemainsConstantWithinItemDuration_ItemsOnly` | `...::efficiency_constant_within_item_duration` | Item duration 60 ticks; one tick past → 0. |
| `PowerTestFixture`'s `updateConsumption` loop + graph ordering | `...::graph_update_uses_schedule_slot` | Asserts `UpdatePowerGraph` runs once per tick, in `Groups.power_graph` slab order, before `UpdateBuildings`. |
| (new) | `power::graph::tests::{merge_prefers_larger, split_reassigns_all_members, reflow_terminates_on_cycle, graph_updater_entity_lifecycle}` | Topology invariants; no orphan members; updater count == live graphs. |
| (new) | `power::node::tests::{autolink_sorted_by_type_then_distance, insulated_raycast_blocks_link, max_nodes_enforced}` | Auto-link parity. |
| (new) | `power::diode::tests::{transfers_half_difference, energy_delta_affects_balance}` | Diode semantics. |
| (new) | `liquid::tests::{transfer_flow_formula, junction_destination_recursion, conduit_leak, flow_window_rates}` | Liquid primitives. |
| (new) | `heat::tests::{contact_points_math, orientation_predicate, split_heat_thirds, cycle_guard, crafter_overheat_scale}` | Heat semantics. |
| (new) | `world::modules::tests::{power_module_roundtrip_nan_clamp, liquid_module_roundtrip}` | Module IO parity (04 owns wire format tests). |

Tests requiring 07 behavior are marked `#[ignore = "plan 07 contract"]` until the reconciliation lands; the fixture variants that do not need 07 run from M0.

### 7b. Headless harness scenarios (`mind-headless`)

1. `run power_battery_cycle --map power_flat_64 --ticks 600 --dump out/power_battery.json`
   - Build `fakegen(10) → direct consumer(0) + fakebattery(100)`; run 30 ticks charging (battery status → 1 at 10/tick); clear the producer; attach a 5/tick consumer; run 200 ticks (battery drains proportionally, status trace monotone); assert `NetworkState` equals `tests/golden/power_battery_cycle.json`.
2. `run power_graph_split_merge --map power_flat_64 --script scenarios/power_graph_split_merge.ron`
   - Place 9 power nodes + generator + consumer; break the middle node at tick 60 → assert two graphs (`network_counts.graphs == 2`), generator-side satisfaction 1, consumer-side 0; re-place at tick 120 → assert one graph and satisfaction restored; no orphan `PowerGraphUpdater` entities (group count == graph count).
3. `run liquid_conduit_transfer --map power_flat_64 --ticks 300 --dump out/liquid.json`
   - `liquid-source(water) → 10 conduit chain → liquid-tank`; assert per-tick transfer follows `min(clamp(fract-out_fract) × capacity, available)` within ε, tank amount monotone, no teleport past a broken segment (break the chain at tick 150 and assert flow stops), `LiquidFlowCache` rate ≈ expected after 60 polls.
4. `run heat_network_equilibrium --map power_flat_64 --ticks 600 --dump out/heat.json`
   - `heat-source → heat-redirector → carbide-crucible(HeatCrafter)`; assert conductor heat equals the producer's heat after ramp-up, crafter `efficiencyScale` matches the overheat formula, removing the source decays heat to 0 and disables the crafter; nuclear-reactor coolant case included as a second fixture.
5. `run power_network_determinism.simlog --through-tick 3600 --checksum-every 60`
   - Same seed + place/break/configure log; 2 in-process + 2 cross-process runs with `workers=1` and `workers=4` produce identical 61 checkpoint checksums equal to `tests/golden/power_network_determinism.checksums`; optimized build unchanged.
6. `bench power --buildings 2000 --graphs 200 --ticks 3600 --json out/bench_power.json` (plus `bench liquid`, `bench heat`).

### 7c. MCP playtest scenario (open-godot-mcp)

Preconditions: plan-00 spine scene with `MindSim` autoload (path from plan 00, e.g. `/root/Spine/SimHost`); plan 14 bars not required — the scenario asserts the underlying data plus inspector fields. Adapt paths to plan 00.

1. `godot_health check` → `{ok:true}`.
2. `godot_game play(scene="res://scenes/spine.tscn")`.
3. `godot_exec eval`:
   ```gdscript
   MindSim.dev_place("combustion-generator", 30, 30, 0)
   MindSim.dev_place("power-node", 36, 30, 0)
   MindSim.dev_place("battery", 40, 30, 0)
   MindSim.dev_add_item(30, 30, "coal", 10)
   ```
   (returns true per call; `dev_place` routes through the `SimCommand` path from plan 05).
4. Wait 2 s (or `godot_game_time step 120`); `godot_exec eval`:
   ```gdscript
   var d = MindSim.power_debug(30, 30)
   print("MCP_POWER=", d)
   ```
   Expect `producers == 1`, `batteries == 1`, `satisfaction == 1.0`, `stored > 0`, `graph_id == MindSim.power_debug(40,30).graph_id`.
5. `godot_runtime_state inspect` node `/root/Spine/SimHost` properties `["network_counts","group_counts"]` → `network_counts.graphs >= 1` and `group_counts.powerGraph >= 1`; wait 1 s and confirm `stored` increases.
6. `godot_screenshot game` → save to `out/mcp_power_before_split.png` (evidence; block sprites/laser until plan 16 adds lasers).
7. `godot_exec eval`: `MindSim.dev_break(36, 30)`; wait 0.5 s; assert `MindSim.power_debug(30,30).graph_id != MindSim.power_debug(40,30).graph_id` and the generator side stays satisfied; print both dicts.
8. `godot_exec eval`: re-place `power-node` at (36,30); wait 0.5 s; assert both `graph_id`s are equal again and `network_counts.graphs` returned to the baseline.
9. `godot_screenshot game` → `out/mcp_power_after_merge.png`; `godot_log errors` → no `PowerGraph`/panic lines; `godot_game stop`.
10. Once plan 14 lands, repeat step 5 by hovering the node and asserting the `bar.powerbalance` label/fill from `PowerBarData` (UI parity follow-up owned by 14).

### 7d. Performance budget + measurement

Baseline per HLP §7.4 (sim tick ≤ 4 ms mid-game; 60 tps). Network slices (p99 on dev machine, measured by `mind-headless bench` + `cargo bench -p mind-core --bench networks`):

| Profile | Load | Budget |
|---|---|---|
| `power` | 2 000 power buildings across 200 graphs, mixed producers/consumers/batteries, 3 600 ticks | ≤ 0.30 ms/tick graph update |
| `power_split_merge` | 500-node graph churn: 1 000 place+break+config ops | ≤ 1.0 ms/op p95 for split/reflow; ≤ 0.1 ms merge |
| `liquid` | 5 000 conduits/router chains moving liquid | ≤ 0.20 ms/tick all transfers |
| `heat` | 2 000 heat buildings (producer/conductor/crafter), memoized | ≤ 0.50 ms/tick |
| `alloc` | any steady-state network tick after warmup | **0 allocations** (`bench --assert-alloc 0`) |
| Determinism | same seed/command log, optimized build | checksum == golden |

Measurement: `mind-headless bench {power,liquid,heat} --buildings N --ticks 3600 --warmup 600 --json`, `cargo bench -p mind-core --bench networks -- --save-baseline networks`, and the plan-05 alloc-audit feature. CI records (plan 23 gates) P50/P95/P99 in `bench/baselines.json`.

### 7e. Exit criteria checklist

- [ ] `cargo test -p mind-core` green; every §7a row implemented (or `#[ignore = "plan NN"]` with owner).
- [ ] `cargo fmt --check` + `cargo clippy -p mind-core -- -D warnings` clean.
- [ ] Power fixture family ported and passing with the documented delta adaptation; golden numbers committed.
- [ ] `mind-headless` scenarios `power_battery_cycle`, `power_graph_split_merge`, `liquid_conduit_transfer`, `heat_network_equilibrium` match golden dumps within ε.
- [ ] `power_network_determinism` identical for 1/4 workers and across processes; checksums unchanged by optimization.
- [ ] Graph topology invariants: `all == union(producers, consumers, batteries)` classes, every building has exactly one live graph, updater count == graph count, no orphans after 1 000 split/merge ops.
- [ ] Module IO round-trip incl. NaN/Inf status clamp; revision manifests updated (04/07).
- [ ] Reactor/impact/variable explosion triggers fire exactly once on death conditions; §7c graph split via destruction works (10 hook).
- [ ] MCP scenario §7c passes with screenshots and log check attached to Changelog.
- [ ] §7d budgets met, alloc-audit zero, `23` ledgers updated.
- [ ] `cargo tree -p mind-core` has no `godot`/`tokio`; GPL headers on every ported file.
- [ ] 07/08 interface reconciliation (R1–R3) discharged or explicitly deferred with owners.

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Flag |
|---|---|---|---|
| R1 | Plan 07 (and 06) not written; §3.4 interfaces are assumptions | Implement against the contract; keep network algorithms independent of 07 internals so the merge is mechanical | **orchestrator reconcile** (fix at 07 merge) |
| R2 | `LiquidBridge`/`DirectionLiquidBridge` need 08's `ItemBridge`/`DirectionBridge` bases, which are outside this plan's declared `Depends on` | `move.rs`/routers/junctions ship at M3; bridges gated to M5 after 08's bridge milestone; the 08 base types are consumed by name | **NEEDS ORCHESTRATOR RECONCILE** (extend 09 deps to 08 or move bases to 07) |
| R3 | `GenericCrafter` base behavior required by `HeatProducer`/`HeatCrafter`; ownership unassigned in HLP | 07 owns it; if 07 declines, 09 ships the minimal craft loop behind `production::generic_crafter` and flags the file | **orchestrator reconcile** |
| R4 | Node auto-link candidate order comes from `TeamData.buildingTree` (Arc `QuadTree`), owned by 11/12 in HLP but needed in P3 | Plan 09 ports Arc `QuadTree` iteration/insert semantics into `world::spatial::quad_tree` and exposes `TeamData::building_tree`; 11/12 consume instead of reimplementing. A simpler sorted scan over `Groups.build` is possible but can change tie-breaks in auto-link selection (observable in replays) | **NEEDS USER DECISION** (exact tree parity vs simpler scan); default = exact port |
| R5 | `LiquidModule` static flow cache is global upstream: only the watched building shows flow | `LiquidFlowCache` reproduces the single-watched semantics; UI calls `update_flow()`; per-building flow is a visible UX improvement but a behavior deviation | **NEEDS USER DECISION** (UD-09-2); default = parity |
| R6 | Java power tests fixed delta `0.5`; sim is fixed `1.0` (D8) | Network functions take explicit delta; all delta-sensitive cases are ported; expected numbers are Rust-golden | accepted (documented deviation §2.3) |
| R7 | Graph churn memory: IDs must not dangle and slots must be reusable | `{slot, generation}` IDs + free list + monotonic debug `graph_id`; stale-ID use is a debug panic | no user needed |
| R8 | `PowerGraph::remove` calls `update()` re-entrantly during world mutation | Port exact; placement/breaking runs under exclusive access (plan 07); a debug reentrancy flag catches misuse | reconcile with 07 |
| R9 | `WindowedMean` shared with `ItemModule` (08) | Single port in `mind_core::math::windowed_mean`; 08 consumes it | reconcile with 08 |
| R10 | Reactor explosion effects depend on 10/17/12 (`Damage`, `Fires`, `Puddles`, `Effect`, `Rules`) | 09 fires hooks with exact parameters; headless no-op stubs until those plans land; scenario 7b.#2 exercises the destruction path | reconcile |
| R11 | Heat recursion depth on adversarial graphs | `came_from` prevents cycles; debug depth guard panics, release logs and truncates | accepted |
| R12 | `Groups.powerGraph` def registration could collide with 05's generated def list | 09 registers exactly one def (`PowerGraphUpdater`); 05 owns the group and the schedule slot | reconcile with 05 |

## 9. References

### Mindustry sources read

- `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`
- `core/src/mindustry/world/AGENTS.md`, `world/blocks/AGENTS.md`, `entities/AGENTS.md`, `content/AGENTS.md`, `type/AGENTS.md`, `graphics/AGENTS.md`
- `core/src/mindustry/world/blocks/power/PowerGraph.java`, `PowerBlock.java`, `PowerDistributor.java`, `PowerNode.java`, `LongPowerNode.java`, `BeamNode.java`, `Battery.java`, `PowerDiode.java`, `PowerGenerator.java`, `ConsumeGenerator.java`, `ThermalGenerator.java`, `SolarGenerator.java`, `ImpactReactor.java`, `NuclearReactor.java`, `VariableReactor.java`, `HeaterGenerator.java`, `LightBlock.java`
- `core/src/mindustry/world/blocks/liquid/LiquidBlock.java`, `Conduit.java`, `ArmoredConduit.java`, `LiquidRouter.java`, `LiquidJunction.java`, `LiquidBridge.java`
- `core/src/mindustry/world/blocks/distribution/ItemBridge.java` (interface), `DirectionLiquidBridge.java`
- `core/src/mindustry/world/blocks/heat/HeatBlock.java`, `HeatProducer.java`, `HeatConsumer.java`, `HeatConductor.java`
- `core/src/mindustry/world/blocks/production/HeatCrafter.java`
- `core/src/mindustry/world/blocks/sandbox/PowerSource.java`, `PowerVoid.java`, `LiquidSource.java`, `LiquidVoid.java`
- `core/src/mindustry/world/modules/PowerModule.java`, `LiquidModule.java`
- `core/src/mindustry/entities/comp/BuildingComp.java` (power/liquid/heat regions, `update`, `updateConsumption`), `PowerGraphUpdaterComp.java`, `entities/GroupDefs.java`
- `core/src/mindustry/world/Block.java` (`consumePower*`, `reinitializeConsumers`, `hasPower`/`connectedPower`/`conductivePower`/`insulated`, `setBars`), `world/consumers/{Consume,ConsumePower,ConsumeLiquidBase,ConsumeLiquid,ConsumeLiquidFilter,ConsumeCoolant,ConsumeItemFilter}.java`
- `core/src/mindustry/type/Liquid.java`
- `core/src/mindustry/content/Blocks.java` (power/liquid/heat block registrations and values)
- `tests/src/test/java/power/PowerTestFixture.java`, `PowerTests.java`, `DirectConsumerTests.java`, `ConsumeGeneratorTests.java`
- `tests/AGENTS.md`

### Plan set

- `HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2.1 layout, §2.4 determinism, §3 rows for 02/03/04/05/06/07/08/10/11/12/14/16/23, §4 template, §6–§9 conventions)
- `PRELIMINARY_PLAN.md` (history)
- `00_FOUNDATION_IMPLEMENTATION_PLAN.md`, `02_CONTENT_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`, `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (read in full for this draft)
- `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md`, `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (referenced; **not yet written** at draft time — see §3.4/§3.13)
- `08_LOGISTICS_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `14_UI_IMPLEMENTATION_PLAN.md`, `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md`, `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (referenced by interface only)

## Changelog

- 2026-10-01 — Draft v1 written. No implementation started. R2/R3 and UD-09-1/UD-09-2 require orchestrator/user input before M5, M4, M1 (`autolink` tests may start with a brute-force fallback) and M3 flow UI respectively.

- 2026-10-02 — **M0 + M1 landed on `lane/09-power` (branch off `main` @ `81177fa`).**
  - **M0.** `mind_core::math::windowed_mean::WindowedMean` (exact Arc port; owned by 09 per HLP C10) and `mind_core::util::IdSet` (u32 bitset). `world/blocks/power/{mod,module,graph}.rs`: `PowerGrids` resource + `PowerGraph` (60-sample `WindowedMean`), `PowerGraphId` arena with free list + generation checks, `PowerScratch` reusable BFS scratch; plan-09 components `PowerNodeInfo`/`PowerProduction`/`PowerNodeConfig`; `read_power_info` (BlockDef-derived with an override seam) and `get_power_connections` (exact Java predicate). `PowerGraph::{add,add_graph,check_add,clear,reflow,remove,remove_list,update}` and battery helpers ported verbatim; `PowerGraphs::update_all` is the per-tick `UpdatePowerGraph` body. `fixtures::power::{PowerHarness,FakeProducer,FakeBattery,FakeDirectConsumer}` ports `PowerTestFixture` (deterministic counter names, explicit `delta`).
  - **M1.** Topology merge/split/reflow + updater lifecycle, `nodes.rs` (`circle_rect_overlap`, `insulated` raycast, `link_valid`, `get_potential_links` deterministic-scan fallback for R4, `configure_link`/`configure_links`), `Battery.overwrote` and `PowerDiode.updateTile` transfer. `rebuild_all` is the `World.endMapLoad` path.
  - **Verification (Oracle §7a subset).** `cargo test -p mind-core --lib world::blocks::power` → **12/12 passed**: `direct_consumer_satisfaction_is_as_expected` (all 7 cases incl. `0.09/0.09-ε`), `battery_capacity_is_as_expected` (all 9 cases; charge-before-zero-capacity guard), `direct_consumption_stops_without_power`, `merge_prefers_larger`, `split_reassigns_all_members`, `reflow_terminates_on_cycle`, `graph_updater_entity_lifecycle`, `update_uses_list_order`, plus module/nodes tests. Full `cargo test -p mind-core` green; workspace fmt + clippy `-D warnings` clean.
  - **Interface assumption (08↔09).** This core has **no** plan-08 dependency. `Conduit` autotiling (M3) will consume `mind_core::world::blocks::tile_bitmask::VALUES` (present, plan 03) through a minimal local `Autotiler` seam and reconcile with 08's `Autotiler`/`TileBitmask` at join; `LiquidBridge`/`DirectionLiquidBridge` remain gated to M5 behind 08's `ItemBridge`/`DirectionBridge` (R2).
  - **Reconciliation.** R4 default (exact `TeamData` quadtree) not yet ported: `get_potential_links` uses a deterministic `(is-node, dst2, entity index)` scan; flag for the 11/12 merge. `PowerGraphUpdater` uses the existing plan-05 marker + `EntitySeq`; `Groups.power_graph` wiring via plan 05's group is deferred to the gdext/MCP milestone.

- 2026-10-02 — **M2 landed on `lane/09-power`.**
  - **Generators.** `world/blocks/power/generator.rs`: plan-09 `GeneratorConfig`/`GeneratorState` components (07's `BlockDef` carries no generator knobs), `GeneratorFilter::{ItemFlammable,LiquidFlammable}`, and `update_generator` porting `ConsumeGeneratorBuild.updateTile` + `updateEfficiencyMultiplier` + the single-filter `updateConsumption` pass; `consumed_item`/`consumed_liquid` filters, `get_power_production`, `generator_warmup`, `should_explode`, `consume_trigger_valid`, `on_generator_destroyed`. `reactors.rs`: pure ports of `ThermalGenerator`/`SolarGenerator`/`ImpactReactor` (`warmup^5`)/`NuclearReactor` coolant+heat/`VariableReactor` instability/`HeaterGenerator` ramp. `sandbox.rs`: `PowerSource`/`PowerVoid`/`LiquidSource`/`LiquidVoid` update functions + the `explodeOnFull` `Destroyed` trigger. Explosion effect/damage dispatch remains plan 10/17 (hooks provided, R7/R10).
  - **Verification (§7a).** `consume_generator::tests::liquid_input_deltas` (deltas 2/1/0.5 × 4 amounts), `item_flammability_inputs` (coal/blast-compound/spore-pod/pyratite, 0/1/10), `efficiency_constant_within_item_duration`, `plain_generator_production_tracks_efficiency`; `reactors` tests for solar lighting, impact `warmup^5`, nuclear coolant removal, variable instability, heater ramp. `cargo test -p mind-core --lib world::blocks::power` → **23/23**; full suite green; fmt + clippy `-D warnings` clean.
  - **Deferred.** The 07 `BuildingBehavior`/`GenericCrafter` integration for `HeatProducer`/`HeatCrafter`/`Thermal`/`ConsumeGenerator` (R3) is M4/M5; explosion bodies (`Damage`/`Fires`/`Puddles`/`Bullets`) are plan 10/17.

- 2026-10-02 — **M3 landed on `lane/09-power`.**
  - **Liquids.** `world/blocks/liquid/{mod,movement,tests}.rs`. `LiquidNode` per-block knobs (capacity/accepts/leakable/junction/router/pressure/filter); `movement.rs` (plan's `move.rs`, renamed because `move` is a Rust keyword) ports `move_liquid` (fraction-difference formula, pressure, capacity clamp, `acceptLiquid`), `move_liquid_forward` (leak `stored/1.5`, plan-10 puddle hook), `dump_liquid` (cdump rotation, `outputDir`/`rotation` gate, `transfer_liquid`), `get_liquid_destination` (`LiquidJunction` recursion via `relativeTo`/`nearby`), `relative_to_dir` (`Tile.absoluteRelativeTo`). `LiquidFlowCache` Resource reproduces Java's static `cacheFlow`/`cacheSums`/`displayFlow`/`cacheBits`/`flowTimer` (6-sample `WindowedMean`, 10-tick poll, 15-tick visual refresh, `-1` not-ready, `get_flow_rate = mean/10*60`). `update_router`/`update_conduit`/`current_liquid` behaviors. Autotiling: `tile_bitmask::VALUES` is available (plan 03); the `Conduit` blend-mask seam is deferred to the 08 `Autotiler` reconciliation (see M3 interface note) — no 08 dependency landed here.
  - **Verification §7a.** `liquid::tests::{transfer_flow_formula, junction_destination_recursion, conduit_leak, router_dumps_current, flow_window_rates}` all pass; `cargo test -p mind-core --lib world::blocks` **32/32**; full suite green; fmt + clippy `-D warnings` clean.
  - **Interface assumption (08↔09).** `get_liquid_destination`/`LiquidJunction` and the transfer primitives are self-contained. `Conduit` autotiling will call `Autotiler::build_blending`/`TileBitmask` (08); no local stub was needed for the M3 tests because `tile_bitmask` data already lives in `mind-core`. `LiquidBridge`/`DirectionLiquidBridge` remain gated to M5 behind 08's `ItemBridge`/`DirectionBridge` (R2).

- 2026-10-02 — **M4 core landed on `lane/09-power`.**
  - **Heat.** `world/blocks/heat/mod.rs` (consolidates the plan's `heat/{mod,calculate,producer,conductor}`): `HeatState`/`HeatConductor`/`HeatCrafter` components, `HeatScratch`; `calculate_heat(world, entity, side_heat, came_from, update_id, depth)` ports `BuildingComp.calculateHeat` exactly (contact-point math, orientation gate, `came_from` cycle guard, `add = heat/size*contact`, split `/3`, conductor recursion memoized by `update_id`, `union_with` of traversed sets); pure `contact_points`/`orientation_allows`/`heat_frac`; `heat_producer_step` (ramp); `crafter_efficiency_scale`/`crafter_should_consume`/`crafter_heat`. `nuclear_coolant_removal` from M2 completes the reactor heat couplings. `IdSet::union_with` added.
  - **Verification §7a.** `heat::tests::{contact_points_math, orientation_predicate, split_heat_thirds, cycle_guard, crafter_overheat_scale, producer_conducts_contact_heat}` pass; `cargo test -p mind-core --lib world::blocks::heat` **6/6**; full `cargo test -p mind-core` **568 passed / 2 ignored**; fmt + workspace clippy `-D warnings` clean; `cargo check -p mind-gdext` clean.
  - **Deferred.** `HeatProducer`/`HeatCrafter` as `BuildingBehavior`s on 07's `GenericCrafter` base (R3) and `Turret.heatReq` (`calculate_heat` reuse) stay for plan 10 / the 07 merge; heat rendering (`DrawHeatOutput`) is plan 16; reactor/impact explosion bodies remain plan 10/17.

- 2026-10-02 — **M5/M6 status (blocked/deferred).** Not attempted in this lane run: M5 `LiquidBridge`/`DirectionLiquidBridge` require plan 08's `ItemBridge`/`DirectionBridge` bases (R2); module IO parity needs 04/07's wire paths; `power_network_determinism` replay/benches and the M6 `MindSim` debug API/Networks inspector + MCP scenario are gated on the orchestrator's single-editor MUTEX. All core M0–M4 algorithms are in place and unit-tested; the headless `power`/`liquid`/`heat` subcommands and `tests/golden/*.json` are not yet wired (would need 07/08 integration + the MCP mutex for §7c).

- 2026-10-02 — **M5-seam + M6-headless landed on `lane/09-power` (branch @ `581a007`).**
  - **M6 headless oracle.** `mind_core::world::network_state` (`NetworkState`/`GraphState`/`BuildingState`; stable sorted JSON, 4-decimal rounded f32 shortest-form via `Serialize`) + `PowerGrids::iter_graphs`. `mind-headless` subcommands `power`/`liquid`/`heat {list,scenario,bench}` wired through `cli.rs`/`exec.rs`/`lib.rs` and new `network_scenarios.rs`. Scenarios (`run_scenario`): `power_battery_cycle` (30-tick charge then producer removal + 200-tick drain, status samples), `power_graph_split_merge` (5-node chain + generator + consumer; break middle node → 2 graphs with satisfaction 1/0; re-add → 1 graph satisfaction 1; updater count == graph count at each step), `power_network_determinism` (split/merge state sequence FNV-1a checksums, two runs identical), `liquid_conduit_transfer` (source → 10 conduits → tank, mid-chain break at tick 150, monotone tank + `LiquidFlowCache` rate at tick 99), `heat_network_equilibrium` (producer → conductor → `HeatCrafter`; conductor=crafter=10 @300, `efficiencyScale`=2.0; source removal → heat 0; nuclear coolant fixture). Benches `bench power|liquid|heat --buildings N --ticks T --warmup W --json` report p50/p99 against §7d budgets; §7d numbers are release-only (debug is ~30–50× slower) — record release artifacts when the bench lane runs.
  - **Goldens (§6.5 / §7b).** `client/rust/mind-headless/tests/golden/network/{power_battery_cycle.json, power_graph_split_merge.json, liquid_conduit_transfer.json, heat_network_equilibrium.json, power_network_determinism.checksums}`; `mind-headless power|liquid|heat scenario <name> --dump|--check <file>` and `network_scenarios::tests::committed_goldens_match` enforce them. `mind-headless power|liquid|heat bench` is the §7d entry point (`cargo bench -p mind-core --bench networks` remains for the benchmark harness — plan 23 owns baselines).
  - **Bench measurements (release, 2000 buildings / 3600 ticks / warmup 600; §7d budgets).** power p50/p99 **230 / 547 µs** (budget 0.30 ms p99; p50 within, p99 marginal); liquid **551 / 1150 µs** (0.20 ms; over); heat **512 / 1270 µs** (0.50 ms; over, improved from 1027/2188 µs by removing the per-call `proximity` `Vec` in `calculate_heat` + reusing `IdSet` scratch in the bench). Bench methodology differs from the plan's intended profile (my power harness is uniform 5-producer/5-consumer graphs; liquid measures `update_conduit` with pre-timed refill) — liquid/heat optimization (cached neighbor/route lookups) is deferred to the 07 schedule integration. `bench --assert-alloc`/alloc-audit is not wired for the network profiles yet (plan 23).
  - **Heat parity fix (oracle-caught).** `calculate_heat` previously `continue`d on the `came_from` cycle gate, skipping the conductor recursion; upstream always calls `cond.updateHeat()`. It also treated any `HeatState` neighbor as a source, so a `HeatCrafter` got registered into a conductor's `cameFrom`. Fixed: `HeatCrafter` neighbors are skipped (upstream only iterates `HeatBlock`), the cycle gate now only suppresses the heat add while still recursing and contributing `cameFrom`, and `HeatConductor::default().update_id = u64::MAX` mirrors `lastHeatUpdate = -1`. `cycle_guard` rewritten to upstream semantics + new `crafter_pull_keeps_conductor_hot_across_frames`. (This was latent in M4; the M6 scenario made it observable.)
  - **M5 bridges (seam).** Plan 08 M3 (`ItemBridge`/`DirectionBridge`) is not present on this branch, so `world/blocks/liquid/bridge.rs` ships the **minimal seam** documented in 08 §3: `LiquidBridgeLink { link, range, warmup, transport_time, leaks }` (mirrors `ItemBridgeBuild`), `DirectionLiquidBridgeLink { occupied[4], speed }` (mirrors `DirectionBridgeBuild`), `positions_valid`, `resolve_link`, `update_liquid_bridge` (`warmup >= 0.25`), `update_liquid_bridge_tile` (transport-or-dump), `update_direction_liquid_bridge`, `dump_liquid_bridge`. Tests `positions_valid_cartesian_only`, `bridge_moves_when_warm`. **Orchestrator reconcile:** when 08 M3 lands, delete the seam components and read `ItemBridgeBuild.link/warmup` + `DirectionBridgeBuild.occupied` directly, then re-point `LiquidBridge`/`DirectionLiquidBridge` registrations.
  - **Verification.** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p mind-core` **635 passed / 2 ignored** (was 631); `cargo test -p mind-headless --lib network_scenarios` **6/6** (5 scenario runs + committed-golden check). Exact dumps: split/merge final satisfaction `1.0`, battery final `stored=0.0` with `needed=5.0`, tank `1297.0216` @tick 300, heat producer `10.0`.
  - **Still open (this lane).** M6 `MindSim` debug API (`power_debug`/`liquid_debug`/`heat_debug`, `network_counts`) + Networks inspector tab + MCP §7c (single-editor mutex). R3 `HeatProducer`/`HeatCrafter`/`ConsumeGenerator` as 07 `BuildingBehavior`s: pure update fns exist and are tested, but the `PowerGrids` per-tick schedule integration belongs to the 07/05 merge. R4: `get_potential_links` still uses the deterministic `(is-node, dst2, entity)` scan; exact `TeamData` quadtree deferred to the 11/12 merge.

- 2026-10-02 — **M5 reconciliation (08 bridge seam) landed on `lane/09-power`.** Main now contains plan 08 M3 (`distribution::item_bridge`, `distribution::direction_bridge`), so the M6 bridge seam is discharged.
  - `world/blocks/liquid/bridge.rs` no longer defines duplicate transition components; **deleted `LiquidBridgeLink`/`DirectionLiquidBridgeLink`**. `update_liquid_bridge` now reads 08's `ItemBridgeBuild.link`/`warmup` directly; `update_direction_liquid_bridge` uses 08's `DirectionBridgeBuild.occupied`/`last_link` + `find_link(world, e, range)` and re-exports 08's `positions_valid` as the single source of truth. Added `direction_bridge_accepts` (upstream `DirectionLiquidBridgeBuild.acceptLiquid` occupancy/`relativeToEdge` gate over `DirectionBridgeBuild.occupied`). `liquid/mod.rs` re-exports re-pointed to the 08-backed API.
  - **Block-registration note:** there was no literal `BuildingBehavior` registration for the liquid bridge kinds to re-point — M3 shipped conduits/routers/junctions as grid-taking free functions, and the seam was pure functions too. The `BlockKind::LiquidBridge`/`BlockKind::DirectionLiquidBridge` content entries were already correct; the change is component + re-export de-duplication. A `BuildingBehavior` wrapper stays out of scope until 07 exposes a grid/`TileBuilds`-only liquid drive (same constraint as M3 conduits).
  - **Verification (§7a/§7b).** New `liquid::bridge::tests::{bridge_moves_when_warm, direction_bridge_finds_link_and_occupies, positions_valid_cartesian_only}` (the warm test now drives `ItemBridgeBuild.warmup`); `mind-headless liquid scenario liquid_conduit_transfer --check tests/golden/network/liquid_conduit_transfer.json` → OK (goldens unchanged); `cargo test -p mind-headless --lib network_scenarios` **6/6**; full `cargo test -p mind-core` **672 passed / 2 ignored**; `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean.

- 2026-10-02 — **R3 (heat/generator behaviors) + R4 (quadtree decision) landed on `lane/09-power`.** Plan 07's `BuildingBehavior` framework is in `main`, so the R3 seam is discharged.
  - **R3 heat.** New `world/blocks/heat/behavior.rs`: `HeatProducerBehavior` (delegates the craft loop to plan 07's `production::CrafterBehavior`, then ramps `heat = approachDelta(heat, heatOutput × efficiency, warmupRate × delta)`), `HeatCrafterBehavior` (pulls `calculate_heat` inside `efficiency_scale` — the exact `updateConsumption` point where upstream applies `efficiencyScale()` — then delegates the craft loop), and `HeatConductorBehavior` (`updateHeat` memoized by `BuildClock`, `splitHeat`, `noUpdateDisabled`). Vanilla knobs live in the registration table (upstream values: `oxidation-chamber 5`, `electric-heater 3`, `slag-heater 8`, `phase-heater 15`, `heat-reactor 10`, `heat-source 1000/1000`; crafters `atmospheric-concentrator 24`, `carbide-crucible 40`, `surge-crucible 40`, `cyanogen-synthesizer 20`, `phase-synthesizer 32`, all `maxEfficiency 1`; conductors `heat-redirector`/`small-heat-redirector` no split, `heat-router` split, `visualMaxHeat 15`). `BuildingBehavior::efficiency_scale` changed `&World` → `&mut World` (3 impls + 1 call site) so heat can be pulled during `updateConsumption` (parity; not an off-by-one).
  - **R3 generator.** New `world/blocks/power/behavior.rs`: `ConsumeGeneratorBehavior` holds a per-block `GeneratorConfig` template + precomputed content-ordered item/liquid flammability tables (built once at registry time), and drives a new content-free `update_generator_with(world, e, delta, item_flam, liquid_flam)`; the old `update_generator(..., content)` remains as a wrapper. Registered `combustion-generator` and `steam-generator` (the vanilla flammable-fuel cases); `GeneratorConfig` supplies the missing `ItemModule`/`LiquidModule` (plan-02 metadata does not lower `ConsumeItemFlammable` or set `hasItems`). **Documented follow-up:** `differential-generator`, `chemical-combustion-chamber`, `pyrolysis-generator` (specific item/liquid consumers) and `rtg-generator` (`ConsumeItemRadioactive`) need consumer kinds beyond `GeneratorFilter` — not representable yet. Reactors (`Thermal`/`Solar`/`Impact`/`Nuclear`/`Variable`/`Heater`) keep their pure ports; their `BuildingBehavior` wiring needs the plan-07/12 rules seam.
  - **R4 (blocked → fallback kept).** No `TeamData`/`QuadTree`/`BuildingTree` exists in `main` (plans 11/12 have not landed their spatial structure), so `get_potential_links` keeps the deterministic `(is-node, dst2, entity index)` scan documented in M1. Exact Arc quadtree iteration remains deferred to the 11/12 merge; no code change.
  - **Verification.** New tests `heat::behavior::{heat_source_ramps_fast, producer_warms_conductor_and_crafter}` (BuildHarness integration: a `heat-source` warms a `carbide-crucible`) and `power::behavior::combustion_generator_runs_on_coal` (coal fuel → `PowerProduction > 0`). Full `cargo test -p mind-core` **675 passed / 2 ignored** (up from 672); `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean. `default_registry` now installs 08 distribution/storage + 09 heat/power behaviors.

- 2026-10-02 — **M6 (debug-API data side + benches) landed on `lane/09-power`.**
  - **Data side.** `mind_core::world::NetworkState::capture(world, tick)` builds the full three-network projection from a plan-07 world (`PowerGrids` + `LiquidModule` + `HeatState`); unit test `network_state::tests::capture_reads_liquid_and_heat_buildings`. `mind-gdext::MindSimHost` gains read-only `network_state()`, `network_counts()` (`{graphs, liquids, heat_buildings}`), `power_debug(x,y)` (`{graph_id, satisfaction, status, stored, init}`), `liquid_debug(x,y)`, and `heat_debug(x,y)`. `cargo check -p mind-gdext` clean.
  - **MCP §7c deferred** to the orchestrator's single-editor mutex, and the P0 `Sim` world does not yet carry plan-07 building entities/networks (the 07/05 schedule integration), so the debug values are empty until that merge — the data contract and shapes are in place and match §3.11/§3.12.
  - **Benches (unchanged from the M6-headless commit).** Methodology is recorded there; release p99 remains over §7d for liquid (~1.15 ms vs 0.20 ms) and heat (~1.27 ms vs 0.50 ms), power p50 within / p99 marginal. The bench drives the primitives directly; caching/route optimization is deferred to the 07 schedule integration (plan 23 owns baselines).
  - **Verification.** `cargo test -p mind-core` **676 passed / 2 ignored**; `cargo test -p mind-headless --lib network_scenarios` **6/6**; `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo check -p mind-gdext` all clean.
