# 05 — SIM CORE IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft v1 — 2026-10-01, not started |
| **Phase** | P2 — Sim core |
| **Depends on** | `02_CONTENT_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |
| **Blocks** | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md`, `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md`, `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md`, `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md`, `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` |
| **Sources** | `Mindustry/core/src/mindustry/Vars.java`, `ClientLauncher.java`, `core/Logic.java` (update `:503`, `updateEntities` `:464`, `play` `:269`, `reset` `:299`), `core/GameState.java`, `entities/EntityGroup.java`, `entities/EntityIndexer.java`, `entities/GroupDefs.java`, `entities/comp/{EntityComp,UnitComp,BuildingComp,PowerGraphUpdaterComp}.java`, `game/EventType.java`, `async/{AsyncCore,AsyncProcess,PhysicsProcess,AvoidanceProcess}.java`, `annotations/src/main/java/mindustry/annotations/{Annotations,BaseProcessor}.java`, `annotations/.../entity/{EntityProcess,EntityIO}.java`, `annotations/src/main/resources/classids.properties` + `revisions/`; `tests/src/test/java/ApplicationTests.java`, `tests/src/test/java/power/PowerTestFixture.java` |
| **AGENTS read** | `Mindustry/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/entities/AGENTS.md`, `core/src/mindustry/game/AGENTS.md`, `core/src/mindustry/ai/AGENTS.md`, `annotations/AGENTS.md`, `tests/AGENTS.md` |
| **Extends spine** | Replaces plan 00's placeholder fixed-step pump with `mind_core::sim::Sim`; the same accumulator (`60 Hz`, clamp `MAX_TICKS_PER_FRAME = 4`) now drives the real schedule. State inspector gains `tick`, `update_id`, `state`, `group_counts[]`, `checksum`. `mind-headless` gains `sim`, `replay`, `meta entities`, `trace order`, `bench` subcommands used by every plan after this one. |

## 2. Scope & parity definition

### 2.1 In scope

The authoritative simulation runtime and its entity framework, Godot-free, in the `mind-core` crate:

- The `Sim` context that replaces `Vars` static singletons with owned resources inside a Bevy `World`.
- The exact ordered 60 Hz schedule of `Logic.update` + `Logic.updateEntities`, including editor/pause/network run conditions.
- `GameState`, the `State` enum, `StateChangeEvent`, and the `play`/`reset`/pause/resume flows.
- The entity framework replacing the Java annotation pipeline: composition (`@Component`, `@Import`, `@Replace`, `@MethodPriority`), `@EntityDef` archetypes + `EntityMapping`, the base component set, all ten `Groups`, O(1) group removal and indexing, pooled creation and queue-free, sync/save field metadata (`@SyncField`, `@SyncLocal`, `@NoSync`, `@NoSerialize`, `transient`, `@ReadOnly`).
- `Events.fire/on` typed synchronous dispatch + `Trigger` per-frame registry, allocation-free on hot paths.
- `Time` (`delta`, `time`, `update()`, `run()`, `clear()`), `Tmp`/scratch discipline, and the `AsyncCore` worker design for `PhysicsProcess`/`AvoidanceProcess`.
- Determinism hooks: seeded PRNG stream rules, stable iteration rules, `SimCommand`/`CommandLog` replay interface, `Sim::checksum`, and the harness contracts consumed by `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` and `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`.
- Capability traits separating headless sim from client-only modules (no `Vars.ui` equivalent ever exists in `mind-core`).

### 2.2 "Done" means

- `cargo test -p mind-core` runs the full sim runtime headless with no Godot and no network; the ported tests of §7.1 pass.
- `mind-headless run sim_core_boot`, `replay sim_core_determinism`, `reset_play_cycle`, `trace order` and `bench sim_core` pass with the numbers in §7.4.
- Every later plan can register systems into named sets, spawn typed entities through one lifecycle path, subscribe to events, and trust the frame order in §3.4 without editing this module.
- `mind-gdext` steps the identical schedule inside the Godot fixed-step loop and reports `tick`/`checksum`/group counts through the inspector (plan 00 rig).

### 2.3 Deliberate deviations (allowed by `HIGH_LEVEL_PLAN.md` §9)

| Deviation | Reason |
|---|---|
| No Java float/PRNG bit-parity | Only Rust↔Rust determinism is required (D8 appendix in §9 of HLP); tests compare Rust outputs to golden Rust outputs, never to Java binary output. |
| `Time.delta == 1.0` inside sim ticks | Fixed 60 Hz accumulator (D8). Variable frame time exists only in view interpolation, outside `mind-core`. |
| Render `Trigger::draw*`/`universeDraw*` are never fired by sim | They are client-only; `mind-gdext` fires them during Godot draw. Headless triggers (`update`, `beforeGameUpdate`, `afterGameUpdate`, command triggers) are sim-side. |
| Composition is data + systems, not merged method bodies | Rust has no build-time merge. `@MethodPriority(n)` becomes an explicit ordered system list per entity def; `@Replace` becomes a run condition. Behavior is preserved; JVM call semantics are not. |
| Async workers are deterministic by construction | Java's worker races do not exist here: each process writes disjoint, slot-indexed buffers and the join is order-normalized. Default worker count `min(cores, 4)`; harness runs with `workers = 1`. |
| `@InternalImpl`/`@CallSuper`/`@Final` have no direct Rust analog | They only constrained Java codegen; their semantic effect is captured by system ordering tables and sealed helper APIs. |
| `EntityGroup.fixedUpdate(int)` is not ported | No call site in `core/`; kept as a documented stub if a mod layer ever needs it. |

### 2.4 Owned by other plans (do not implement here)

- Content classes, `ContentLoader`, content IDs, `Item`/`Liquid`/`UnitType`/`Block`/`Weather` bodies → `02_CONTENT_IMPLEMENTATION_PLAN.md`.
- `Rules` struct, `Gamemode`, `Team`, `Teams`/`TeamData`, `Universe`, `Saves`, `SectorInfo`, objectives, fog, campaign stats → `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (this plan defines the boundary in §3.12).
- `SaveIO`/`TypeIO`/`JsonIO`/revision files/`.msav` → `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`.
- Tile grid `World`/`Tiles`/`Tile` → `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md`.
- Building behavior, multiblocks, placement rules → `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`; item transport → `08_LOGISTICS_IMPLEMENTATION_PLAN.md`; power/heat/liquid graphs → `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md`.
- Bullet types and collision math → `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`; unit kinds/controllers/pathfinding/waves → `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md`.
- mlog VM and `GlobalVars.update()` → `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md`.
- Net relays, ordered command application, desync detection, world streaming → `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md`.
- Cross-cutting golden/perf suite → `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`.

## 3. Target design

All names below are final unless marked otherwise. `mind-core` is Godot-free and tokio-free; `bevy_ecs` is used as a library (D1), pinned by plan 00's workspace `Cargo.toml`. No `HashMap`/`HashSet` iteration in sim paths (lookup only via `hashbrown::HashMap` is allowed); ordered maps use `IndexMap`, sorted maps use `BTreeMap`.

### 3.1 Module layout

```
client/rust/mind-core/src/
  sim/
    mod.rs            # Sim, SimBuilder, SimConfig, TickReport, post queue
    schedule.rs       # SimSchedule plugin, TickSet/EntitySet labels, run conditions
    logic.rs          # Logic.update port: pre-game state phase systems (state-free module)
    state.rs          # GameState, State, StateChangeEvent
    events.rs         # EventBus, SimEvent, Trigger, listener table, drain systems
    time.rs           # SimClock, RunQueue, TimeUpdate system
    reset.rs          # reset/play/pause flows
  entities/
    mod.rs            # re-exports
    meta.rs           # EntityDef, EntityDefId, GroupMask, FieldMeta, FieldKind, EntityRegistry
    mapping.rs        # EntityMapping (class_id <-> def), classids parity loader
    comp/             # ported components: base.rs, unit.rs, building.rs, bullet.rs, player.rs, ...
    groups.rs         # Groups resource, GroupKind, membership rules
    group.rs          # EntityGroup<T>, EntityIndexer, spatial index hook
    lifecycle.rs      # SimEntity trait, spawn/despawn wrappers, EntityPool, ID allocator, queue_free
    sync_meta.rs      # SyncField/SyncLocal/NoSync/NoSerialize descriptors (plans 04/21 consume)
  async_work/
    mod.rs            # AsyncCore, AsyncProcess trait, WorkerPool
    physics.rs        # PhysicsProcess
    avoidance.rs      # AvoidanceProcess
  determinism/
    rng.rs            # SimRng, RngStream
    command.rs        # SimCommand, CommandLog, CommandError
    checksum.rs       # Checksummer, Checksum
  platform/
    mod.rs            # Platform trait, HeadlessPlatform
    hooks.rs          # ClientHooks trait (no-op default), render-only
  util/
    tmp.rs            # Tmp scratch, TempVec, TempSlice
    pools.rs          # Vec/Box reuse pools (non-entity)
  math/               # Mathf/Angles/Geometry ports used by all sim code
  fixtures/           # test fixtures (power fixture, flat world, entity stress)
  tests/              # integration tests (sim_core)
client/rust/mind-macros/src/    # proc-macro crate: #[derive(SimComponent)], entity_def! metadata emission
```

### 3.2 `Sim` context (the `Vars` replacement)

`Vars`'s 60+ `public static` fields collapse into one owned `Sim` and Bevy resources. Nothing in `mind-core` is a global static except `const`.

```rust
pub struct Sim {
    pub world: bevy_ecs::world::World,   // authoritative ECS: entities + all sim resources
    pub schedule: bevy_ecs::schedule::Schedule,
    pub clock: SimClock,                 // Time
    pub events: EventBus,                // separate field: enables split-borrow synchronous fire
    pub cmdlog: CommandLog,              // replay/network interface (plan 21)
    pub async_core: AsyncCore,           // physics/avoidance workers
    pub platform: Box<dyn Platform>,     // headless by default
    pub hooks: ClientHooksHandle,        // render-only; never read by sim systems (type-safe no-op)
    pub rng: SimRng,                     // sim stream only
    pub config: SimConfig,
    pub post_queue: PostQueue,           // Core.app.post equivalent
}

impl Sim {
    pub fn boot(config: SimConfig, content: &ContentDb, platform: Box<dyn Platform>) -> Result<Self, BootError>;
    pub fn tick(&mut self) -> TickReport;                  // exactly one fixed step (§3.4)
    pub fn command(&mut self, cmd: SimCommand) -> Result<(), CommandError>;
    pub fn fire_now<E: Into<SimEvent>>(&mut self, e: E);   // synchronous, exclusive contexts
    pub fn post(&mut self, f: impl FnOnce(&mut Sim) + 'static); // drained at end of tick
    pub fn checksum(&self) -> u64;
    pub fn snapshot(&self) -> SimSnapshot;                 // plan 21 serialization hooks
}
```

Resources inside `sim.world` (created by `SimBuilder`; typenames are the parity anchors):

| Resource | Ported from | Owner |
|---|---|---|
| `GameState` | `core/GameState.java` | this plan |
| `Rules` | `game/Rules.java` | `12_...` (stub here, see §3.12) |
| `Teams` | `game/Teams.java` | `12_...` (stub here) |
| `WorldGrid` (`World` alias) | `core/World.java`, `world/Tiles.java` | `06_...` |
| `Groups` | generated `mindustry.gen.Groups` + `GroupDefs.java` | this plan |
| `EntityRegistry` / `EntityMapping` | `annotations` `EntityProcess`, `classids.properties` | this plan |
| `EntityIds` | `EntityGroup.nextId()/checkNextId` | this plan |
| `EventBus` (mirror) | `game/EventType` + `arc.util.Events` | this plan |
| `SimClock` (mirror of `Sim.clock`) | `arc.util.Time` | this plan |
| `ContentDb` | `core/ContentLoader.java` | `02_...` |
| `EntityCollisions` | `entities/EntityCollisions.java` | this plan (algorithms), `10_...` (damage) |
| `FogControl`, `Universe`, `MapObjectives` | `game/*` | `12_...` |
| `BlockIndexer`, `Pathfinder`, `ControlPathfinder` | `ai/*` | `11_...` |
| `GlobalVars` (logic) | `logic/GlobalVars.java` | `13_...` |

Registered plugins: `SimSchedulePlugin`, `TimePlugin`, `EventPlugin`, `EntityPlugin`, `LogicPlugin`, `AsyncPlugin`, `ResetPlugin`. Each later plan adds exactly one plugin and never edits this crate's core files (HLP §2.2 boundary).

### 3.3 Platform & capability traits (headless vs client)

```rust
pub trait Platform: Send {
    fn headless(&self) -> bool;                       // Vars.headless
    fn data_dir(&self) -> &std::path::Path;           // Vars.dataDirectory
    fn log(&self, level: LogLevel, msg: &str);        // arc Log
    fn load_settings(&self) -> SettingsHandle;        // plan 04 persistence
    fn exit(&self);                                   // app.exit
    fn millis(&self) -> u64;                          // never used by sim scheduling
}
pub struct HeadlessPlatform { /* data dir, stdout logger */ }

/// Render/UI/audio/input capability. `mind-gdext` implements it; `mind-core` only
/// calls these from `ClientHooks` call sites that are compiled out of the tick.
pub trait ClientHooks: Send {
    fn on_state_change(&mut self, _from: State, _to: State) {}
    fn on_world_loaded(&mut self) {}
    fn on_reset(&mut self) {}
    fn view_interpolate(&mut self, _alpha: f32) {}      // render-only
    fn fire_draw_trigger(&mut self, _t: Trigger) {}     // render-only
}
pub struct ClientHooksHandle(Option<Box<dyn ClientHooks>>); // NoopClientHooks default
```

Invariants:
1. Sim systems never call `ClientHooks` methods that read or write sim state; hooks are notified, never consulted.
2. `mind-core` never names a Godot type, never imports `gdext`, and has no `cfg` branch on Godot.
3. `Platform::headless` is only read by content/asset loading and settings, not by tick logic.

### 3.4 Fixed 60 Hz loop and exact schedule

`SimConfig { fixed_hz: 60, max_ticks_per_frame: 4, seed: u64, workers: usize, trace_order: bool }`.
`mind-gdext` accumulates `delta` and calls `Sim::tick()` up to `max_ticks_per_frame` (mirrors `Vars.maxDeltaClient/maxDeltaServer = 4`, `ClientLauncher.java:84-87`). Headless calls `tick()` directly. `tick()` is the only mutation entry point for sim data.

Java `Logic.update()` order (`Logic.java:503-621`) and its Rust mapping:

| # | Java step | Rust set (`TickSet`) | Run condition |
|---|---|---|---|
| 0 | `PerfCounter.frame` | `Frame` (tracing span) | always |
| 1 | `Events.fire(Trigger.update)` | `TriggerUpdate` | always (even menu/paused) |
| 2 | `universe.updateGlobal()` | `UniverseGlobal` | stub → plan 12 |
| 3 | modified-settings save | `SettingsFlush` | `platform` |
| 4 | `state.enemies = Groups.unit.count(...)` | `EnemyCount` | `!client` |
| 5 | `Events.fire(Trigger.beforeGameUpdate)` | `BeforeGameUpdate` | `is_game && !paused` |
| 6 | `state.tick += delta*60`, `state.updateId++` | `StateClock` | `is_game && !paused` |
| 7 | `state.teams.updateTeamStats()` | `TeamStats` | same → plan 12 |
| 8 | `MapPreviewLoader.checkPreviews()` | `PreviewCheck` | same → plan 16 |
| 9 | `fogControl.update()` | `Fog` | same → plan 12 |
| 10 | sector info / `universe.update()` | `Campaign` | same → plan 12 |
| 11 | `Time.update()` | `TimeRuns` | same |
| 12 | `logicVars.update()` | `LogicVars` | same → plan 13 |
| 13 | weather + per-team `fillItems`/`BuildAI`/`RtsAI`/`prebuildAI` | `WeatherAndAi` | `!client && !editor` (AI sub-sets → 11/12) |
| 14 | `objectives.update()` | `Objectives` | `!editor` → plan 12 |
| 15 | wave timer / `isWaitingWave` / `runWave()` | `WaveTimer`, `RunWave` | `rules.waves` |
| 16 | `state.envAttrs` rebuild | `EnvAttrs` | always in game |
| 17 | `updateEntities()` | `EntityUpdate` (nested chain below) | `is_game && !paused` |
| 18 | `Events.fire(Trigger.afterGameUpdate)` | `AfterGameUpdate` | same |
| 19 | `checkGameState()` | `GameStateCheck` | `run_state_check` |
| 20 | — | `DrainEvents` (event bus listener drain) | after each firing set |
| 21 | — | `PostQueue` (`Core.app.post` flush) | end of tick |

Rust clamps `state.tick` as `f64` (`+ 1.0` per tick) and `update_id` as `u64` for exact parity with `GameState.tick/updateId` semantics; `Time.delta = 1.0` inside the tick and is never mutated by frame time.

Java `Logic.updateEntities()` (`Logic.java:464-500`) → Rust `EntitySet`, chained in this exact order:

| # | Java call | Rust set | Run condition | Notes |
|---|---|---|---|---|
| 1 | `Groups.updatePooling()` | `PoolCleanup` | always | drains `queue_free` into `EntityPool`s (mirrors `Groups.queueFree`/`Pools.free`), then resets pooled components |
| 2 | `Groups.bullet.updatePhysics()` | `PhysicsBullets` | always | `EntityCollisions::update_physics(bullet)` |
| 3 | `Groups.unit.updatePhysics()` | `PhysicsUnits` | always | `EntityCollisions::update_physics(unit)` |
| 4 | `Groups.player.update()` | `UpdatePlayers` | always | |
| 5 | `Groups.effect.update()` | `UpdateEffects` | always | EffectState lifetime; render data only |
| 6 | `Groups.all.update()` | `UpdateAll` | `!editor` | `all` excludes units, players, bullets, effects, power-graph updaters (GroupDefs `exclude`) and buildings (`BuildingComp.java:52` `excludeGroups={"all"}`); includes fires/puddles/decals/world labels/weather states |
| 7 | `Groups.unit.update()` or editor-filtered update | `UpdateUnits` / `UpdateUnitsEditor` | `!editor` / `editor` | editor filter: `is_player() || spawned_by_core` |
| 8 | `Groups.powerGraph.update()` | `UpdatePowerGraph` | `!editor` | one `PowerGraphUpdater` entity per graph; plan 09 owns `PowerGraph::update` |
| 9 | `Groups.build.update()` | `UpdateBuildings` | `!editor` | plan 07 |
| 10 | `Groups.bullet.update()` | `UpdateBullets` | `!editor` | plan 10 |
| 11 | `Groups.bullet.collide()` | `CollideBullets` | `!editor` | `EntityCollisions::collide(bullet)` |

Bevy shape: one `#[derive(SystemSet)] enum TickSet` with the 20 variants above chained by `.chain()` in `SimSchedulePlugin`; `EntitySet` nested under `TickSet::EntityUpdate`; `run_if` closures read `Res<GameState>`/`Res<Rules>` only. Systems are `fn(/* SystemParams */)`; no system stores state between ticks (`Logic` is state-free — see §3.12). The alternative (nested `Schedule`) is rejected: one schedule gives a single, assertable order trace.

### 3.5 Entity framework replacing annotation codegen

#### 3.5.1 Component model

Each Java `@Component abstract class XComp` becomes a Bevy component struct plus associated behavior functions/systems:

```rust
// Ported from entities/comp/HealthComp.java
#[derive(Component, SimComponent)]
#[sim(component, base, methods(update_priority = 0))]
pub struct Health {
    pub health: f32,
    pub max_health: f32,
    #[sim(sync_local)] pub elevation: f32,     // example of @SyncLocal
}
```

Composition semantics mapping (`annotations/AGENTS.md` "How the component system works"):

| Java | Rust |
|---|---|
| `@Component` | `#[derive(SimComponent)]` registers a `ComponentMeta` in `EntityRegistry`; the struct is a Bevy component. |
| `@Component(base = true)` | Component may be the base kind of a hierarchy (`Unit`, `Building`, `Bullet`, `Player`); concretes are `UnitType`/`Block` instances, not new Rust types. |
| `@Import` | Field ownership is explicit. Behavior functions take the union via a `SystemParam`; the `entity_def!` macro verifies at compile time that every imported owner component is in the def list. No field is duplicated. |
| `@Replace` | The replaced component's system is skipped via a run condition keyed on the def's `SystemOrder` table entry; the replacing system runs at the same slot. Exactly one winner is enforced by the macro. |
| `@MethodPriority(n)` | `SystemOrder { method: MethodId, priority: i32 }` in def metadata; the generated per-def update chain sorts by `(priority asc, component name, method name)` — mirrors codegen's stable ordering and `TimedComp.update` priority 100 running last. |
| `@InternalImpl` | Not needed; codegen-only marker. |
| `@Final`/`@CallSuper` | Not needed; system tables are sealed by ownership. |
| `@EntityDef({...})` on a component | `entity_def!(Building = [BaseEntity, BuildingCore, Health, ...])` in `entities/defs.rs`; emitted metadata: `EntityDef { id, name, class_id, groups: GroupMask, pooled, serialize, genio, legacy, fields }`. |
| `@EntityDef` on `UnitTypes.java` fields | `EntityMapping` entry created when plan 02's `UnitType::init` runs: `UnitType.entity_def = EntityMapping.by_name("mace")`; `UnitType.create()` allocates the mapped def. |
| `@SyncField(float, clamped)` | `#[sim(sync_float(clamped))]` → `FieldMeta::SyncFloat { clamped, interp: true }`; plan 21 generates the `_target_`/`_last_` interpolation companion storage from the metadata. |
| `@SyncLocal` | `FieldMeta::SyncLocal`; plan 21 must treat as client-authoritative (never read from server). |
| `@NoSync`, `@NoSerialize`, `transient`, `@ReadOnly` | `FieldMeta::{NoSync, NoSerialize, Transient, ReadOnly}`; plans 04/21 filter on these. |
| `self()` / `as()` | Bevy `Entity` handle + `world.get::<T>(entity)`; no casts. |

`EntityMapping` and IDs:
- `class_id: u16` mirrors `annotations/src/main/resources/classids.properties` 1:1 (same names, same numbers, append-only). A committed copy lives at `mind-core/assets/classids.properties`; `EntityRegistry::load_classids()` parses it; the build/revision test in §7.1 fails if a Rust def's name/id diverges. Plan 04 owns I/O; plan 20 registers mod defs via `EntityRegistry::register(name, ctor)` (the `EntityMapping.register` analog).
- `EntityDefId(u16)` is internal; `class_id` is the save/network ABI.
- Content IDs (blocks/units/etc.) remain plan 02's append-only `ContentId` spaces.

`FieldMeta` is the single metadata surface for plans 04 and 21 (see §6.2). It is emitted by the `SimComponent` derive; `mind_macros` never parses Java.

#### 3.5.2 Base components (`@BaseComponent` equivalent)

Ported from `entities/comp/EntityComp.java` and `PosTeamDef.java`:

```rust
#[derive(Component)] pub struct BaseEntity { pub added: bool }
#[derive(Component)] pub struct SimId(pub i32);            // id field; allocated by EntityIds
#[derive(Component)] pub struct DefId(pub EntityDefId);    // classId()
#[derive(Component)] pub struct Local;                     // isLocal()
#[derive(Component)] pub struct Remote;                    // isRemote() (derived: unit controlled by remote player)
#[derive(Component)] pub struct Pos { pub x: f32, pub y: f32 }
#[derive(Component)] pub struct Vel { pub x: f32, pub y: f32 }
#[derive(Component)] pub struct TeamComp { pub team: u8 }
```

`is_local`/`is_remote` are computed functions (`fn is_local(world, entity) -> bool`) mapping `EntityComp.isLocal()` (`EntityComp.java:30-36`), with `Local`/`Remote` marker components maintained by the player/unit plugins (plan 11/21) for O(1) queries.

#### 3.5.3 Lifecycle: spawn, add, remove, pooling

- `SimEntity` trait (generated by `entity_def!`): `fn def() -> &'static EntityDef; fn bundle(&self) -> impl Bundle;`.
- `SimEntityCommands::sim_spawn(def_id, bundle) -> Entity` allocates `SimId`, inserts all base components + group membership rows, fires `UnitCreateEvent`-class events (def-specific), and marks `added = true`.
- `SimEntityCommands::sim_remove(entity)` mirrors generated `remove()`: `added = false`, remove from every group via O(1) slot removal, fire removal/despawn hooks, then push to `queue_free` if `def.pooled`.
- `PoolCleanup` (first entity set) drains `queue_free` into per-def `EntityPool { free: Vec<Entity>, reset: fn(&mut World, Entity) }`; spawn reuses a free entity when available, else `world.spawn`. Entity numeric IDs are **never** reused (`EntityIds.next_id()` monotonic, `check_next_id(id)` after load — `EntityGroup.java:36-44`), so saves/replays are stable.
- No Java-style static `create()`; `EntityPool` + Bevy's generational `Entity` gives the same "no per-tick allocator churn" property with reset-on-reuse.

### 3.6 `Groups` — typed sets with O(1) removal and indexing

`Groups` resource (port of `GroupDefs.java` + generated `mindustry.gen.Groups`):

```rust
pub struct Groups {
    pub all: EntityGroup, pub unit: EntityGroup, pub build: EntityGroup, pub bullet: EntityGroup,
    pub player: EntityGroup, pub effect: EntityGroup, pub weather: EntityGroup,
    pub power_graph: EntityGroup, pub sync: EntityGroup, pub draw: EntityGroup,
}
```

Membership is computed once per `EntityDef` at registry init, using the exact `GroupDefinition` rule from `EntityProcess.java:273`:

```
group applies ⇔ group.components ⊆ def.components
                ∧ def.components ∩ group.exclude = ∅
                ∧ group.name ∉ def.exclude_groups
```

with `GroupDefs.java:7-16` as the canonical table and `BuildingComp`'s `excludeGroups={"all"}` recorded on the Building def. The computed `GroupMask` is part of `EntityDef`, so membership never depends on runtime component-set inspection (deterministic and cheap).

`EntityGroup` (port of `EntityGroup.java`):
- Storage: slot slab `Vec<Option<Entity>>` + `free_slots: Vec<u32>` + `index: HashMap<Entity, u32>` (lookup-only). Iteration is insertion-stable; removed slots are skipped.
- `add_index(e) -> GroupIndex`, `remove_index(e, idx)` are O(1); `indexer.change(swapped, idx)` (`EntityIndexer.java`) becomes `EntityGroup::set_slot` fixups for the spatial index.
- Optional spatial index (`spatial = true` for `bullet`/`unit`): `mind_core::entities::spatial::QuadTree`, rect-based, rebuilt lazily on `Group::intersect`; floats only from sim data.
- `mapping = true` for `unit`/`player`/`sync`: `id_map: HashMap<i32, Entity>` maintained on add/remove (`get_by_id`, `remove_by_id`).
- `clear()` sets `clearing = true` so nested removals are no-ops, then removes all (mirrors `EntityGroup.java:329-339`).
- Iteration helpers `each`, `update`, `count`, `find`, `contains`, `sort`; hot-path iteration uses index loops (no Rust iterator allocation; port of `ApplicationTests.arrayIterators`).
- **Open decision OD-05-A**: Java uses swap-removal (last element moves into the hole), which changes relative update order. This plan defaults to insertion-stable slab removal because HLP §2.4 mandates stable iteration and swap order is not observable to players except through float tie-breaking. Flagged in §8.

### 3.7 Events and Triggers

Port of `game/EventType.java` + Arc `Events`.

```rust
pub enum SimEvent { /* one variant per Java event class that sim fires */ }
#[derive(Clone, Copy)] pub enum Trigger { /* all 41 Java triggers */ }

pub struct EventBus {
    queues: [EventQueue; SimEvent::COUNT],
    listeners: [SmallVec<[ListenerFn; 4]>; SimEvent::COUNT],
    triggers: [SmallVec<[TriggerFn; 8]>; Trigger::COUNT],
}
type ListenerFn = fn(&mut World, &SimEvent);
type TriggerFn  = fn(&mut World);
```

- Production sites push into the typed queue (payload by value; reusable fixed-size buffers, zero allocation after warmup).
- `drain_events` is an exclusive system (`fn(&mut World)`) using `World::resource_scope` so listeners can read/write the whole ECS synchronously — preserving Java's "listener runs immediately" semantics. Drain systems are chained after every set that can fire (`StateClock`, `EntityUpdate` per sub-set, `RunWave`, `GameStateCheck`).
- `Sim::fire_now` provides the same dispatch for exclusive contexts (boot/play/reset/harness).
- Reused Java payloads (`BulletCreateEvent`, `TileChangeEvent`, `TileFloorChangeEvent`, `TileOverlayChangeEvent`, `BuildTeamChangeEvent`, `UnitDamageEvent`, `BuildDamageEvent`) become by-value structs; the "never nest/store" contract remains a documented rule and is enforced with a debug reentrancy flag.
- `Trigger` registry: `Events.run(Trigger, fn)` → `EventBus::on_trigger`; fired by named systems at exact positions (`TriggerUpdate` set, `AfterGameUpdate` set, command-change triggers fired by their fns). Draw triggers are fired only by `mind-gdext`; `Sim` never fires them.
- Listeners registered via `Plugin::init` / `EventBus::on::<E>`; plan 12/13/21 register their own. No listener registration on hot paths.
- Deferred world mutation (`Core.app.post`) → `Sim::post(f)`; `PostQueue` drains after `AfterGameUpdate`, mirroring Arc's runnable queue flush point.

### 3.8 Time

`arc.util.Time` port (there is no `mindustry.core.Time`; all uses go through Arc's static `Time`):

```rust
pub struct SimClock {
    pub delta: f32,          // 1.0 inside sim (fixed step); Arc parity name
    pub time: f64,           // accumulated delta
    pub global_time: f64,    // accumulates even when paused (mirrors Arc globalTime)
    pub update_id: u64,      // mirrors GameState.updateId (kept here too for Time users)
    runs: RunQueue,          // Time.run(delay, task)
    seq: u64,
}
impl SimClock {
    pub fn update(&mut self, world: &mut World);                    // Time.update()
    pub fn run(&mut self, delay_ticks: f32, f: impl FnOnce(&mut World) + 'static); // Time.run
    pub fn clear(&mut self);                                        // Time.clear()
}
```

- `RunQueue` is a `BinaryHeap<Reverse<(at_time, seq)>>` + lazy cancellation; deterministic ordering by `(at_time, seq)`; `Time.update()` pops and runs every due task (port of `timers`, `manyTimers`, `longTimers` tests).
- `Time.delta` is clamped/real-time-driven in Java; here it is fixed at 1.0 for sim, while `mind-gdext` computes its own view interpolation alpha. Documented deviation §2.3.
- `SimClock::update` runs in `TickSet::TimeRuns` (Java calls `Time.update()` at `Logic.java:544` before entity updates).

### 3.9 `Tmp` / scratch / pooling discipline

- `Tmp`: thread-local scratch (`Tmp::v1::<f32x2>()`, `Tmp::r1::<Rect>()`, `Tmp::seq::<T>()`), reset-on-borrow, never held across systems. Java's static `Tmp` is single-threaded; worker threads get their own TLS instances.
- `TempVec<T>`: reusable `Vec<T>` from a `VecPool`; `with_temp_vec(|v| ...)` borrow guard. Hot loops use `SmallVec<[T; N]>` with inline capacity first.
- Non-entity pools: `VecPool`, `BoxPool` for per-tick scratch collections.
- Entity pooling: §3.5.3.
- The alloc-audit hook (§7.4) proves steady-state ticks allocate nothing.

### 3.10 `AsyncCore` (physics / avoidance workers)

Port of `async/AsyncCore.java` + `PhysicsProcess`/`AvoidanceProcess`. No tokio; `std::thread` + `crossbeam-channel`.

```rust
pub trait AsyncProcess: Send {
    fn init(&mut self, world: &mut World);
    fn reset(&mut self);
    fn begin(&mut self, snapshot: &PhysicsSnapshot);  // build disjoint work items
    fn process(&self, shard: ShardId);                 // runs on a worker
    fn end(&mut self, world: &mut World);              // join + apply
    fn should_process(&self) -> bool;
}
pub struct AsyncCore { processes: Vec<Box<dyn AsyncProcess>>, pool: WorkerPool, futures: Vec<JoinHandle<()>> }
```

- Tick integration: `Sim::tick` → `async_core.complete()` (join previous), run schedule (workers for the *next* tick start after the state phase), `async_core.begin()` (snapshot + dispatch), end of tick marks results ready. Results consumed by `PhysicsBullets`/`PhysicsUnits` next tick, matching Java's pipeline where `AsyncCore.end()` joins at frame end and `updatePhysics` reads ready buffers.
- Determinism: each process partitions entities by group slot index (`slot % shards`), writes into preallocated `Vec`s indexed by slot, and `end()` merges in ascending slot order. No atomics accumulation, no lock-order effects. `workers = 1` is exercised in the determinism harness; results are identical to `workers > 1`.
- `begin` on `WorldLoadEvent`, `reset` on `ResetEvent` (ports `AsyncCore.java:25-38`).

### 3.11 Determinism, RNG, replay

- `SimRng`: deterministic (SplitMix64-based) with an Arc-`Random`-shaped API (`random(high)`, `random(low, high)`, `chance`, `next_float`, `range`) implemented in `math/rng.rs`. Streams: `RngStream::{Sim, MapGen, Waves, Fx}`. Rule: only `RngStream::Sim` is part of `Sim::checksum`; `Fx`/view code must never touch `Sim` (else desync). Content that needs randomness at load time uses `MapGen`.
- Stable iteration: all sim iteration over `Vec`/slab/`IndexMap`/`BTreeMap`; `HashMap` is lookup-only. CI lint (plan 00 clippy config) denies `HashMap`/`HashSet` `iter()`/`for` in `mind-core/src/sim/**` and `mind-core/src/entities/**`; plan 23 owns the enforcement test.
- `SimCommand` / `CommandLog` / `Checksum` are specified in §6.4–6.5. The interface is deliberately transport-free: plan 21 turns STDB `command_event` rows into `SimCommand`s in relay order; plan 23 replays `.simlog` files.
- Simulation scope: all peers run the same build (HLP §2.4); checksums are only compared Rust↔Rust.

### 3.12 Module communication, `Logic` state-freeness, and the `Rules`/`Teams` boundary

- `sim::logic` contains only systems and free functions; it owns no long-lived state (the Java `Logic` class holds only event wiring). All mutable match state is in `GameState`/`Rules`/`Teams`/`WorldGrid`.
- Cross-module writes happen through `EventBus` fires (and the `post` queue for "next frame" semantics), not by reaching into another module's internals. Example: `run_wave` calls `spawner.spawn_enemies()` (plan 11) through a function pointer registered by its plugin, then fires `WaveEvent`; stats/unlock listeners react.
- **`Rules` boundary** (owned by `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`): plan 05 defines `GameState { rules: Rules }` and the minimal `Rules` surface that `Logic` reads/writes today: `waves, wave_timer, win_wave, initial_wave_spacing, wave_spacing, can_game_over, editor, default_team, wave_team, attack_mode, pvp, cleanup_dead_teams, fog, attributes, weather, spawns, loadout, sector, planet, limit_map_area, allow_edit_rules`. Plan 12 extends the same struct/module with every remaining field, `Gamemode`, team rules and JSON I/O. The orchestrator must merge-own `mind-core/src/game/rules.rs` to plan 12 after plan 05 lands (see §8 reconcile note).
- **`Teams` boundary**: plan 05 declares `Teams`/`TeamData` skeleton (`get_active()`, `player_cores()`, `cores(team)`, `update_team_stats()`, `register_core`, `get(team)`, `is_alive`, `destroy_to_derelict` signatures) and calls them from the schedule; plan 12 implements the bodies and caches. `Groups.unit` remains the source of truth for unit enumeration; `TeamData` caches are derived (rebuild in `TeamStats`).
- `Logic` stubs that depend on later plans are registered as no-op systems with a `// plan NN` marker and a debug `unimplemented_stub` counter exposed in `TickReport`, so the schedule order is complete from M6 onward.

### 3.13 Godot / STDB surfaces touched

- **Godot (plan 00 rig, extended):** `mind-gdext::sim_bridge::SimBridge` — an autoload `Node` (assumed `/root/Spine/SimHost` from the plan-00 spine; adapt path if plan 00 differs) with `_physics_process(delta)` accumulating and calling `Sim::tick()` ≤ 4×; exported methods/properties for the inspector: `tick: i64`, `update_id: i64`, `state: String`, `group_counts: Dictionary`, `checksum: i64`, `paused: bool`, and `get_group_count(name)`. No sim logic in GDScript. `ClientHooks` implementation lives in `mind-gdext` and syncs view nodes after `tick()`, never during.
- **STDB:** `mind-core` touches no table/reducer/view. The only interface is `SimCommand`/`CommandLog` and `Sim::checksum()` consumed by plan 21's relay (`command_events` rows → `SimCommand` in relay order) and plan 23's harness. Plans 01/21 own `server/spacetimedb` shapes; this plan only guarantees the Rust-facing command/checksum contract. Per D2 there is no authoritative server sim yet; nothing here blocks adding one.

## 4. Port map

| Mindustry source | Rust target | Notes |
|---|---|---|
| `Vars.java` (statics, `init()` `:311`) | `sim::Sim` + `platform::{Platform, HeadlessPlatform}` + plugin-created resources | No statics; ~60 fields become resources or plugin config. Constants (`tilesize=8`, `maxBlockSize=16`, `finalWorldBounds=250`) land in `mind_core::constants` (shared with 02/06). |
| `ClientLauncher.java` (`setup` `:41`, `add` `:177-182`, `update` `:217`) | `mind-gdext::client::ClientRuntime` (client); `mind_core::sim::SimBuilder` (shared boot) | Headless boot mirrors `server ServerLauncher` subset: content → Logic → sim. `asyncCore.begin/end` (`:257-261`) become explicit phases inside `Sim::tick`. |
| `core/Logic.java` | `sim::logic` (systems), `sim::schedule` (sets), `sim::reset` (`play`/`reset`) | `Logic` event wiring in constructor → `LogicPlugin` listeners. `@Remote sectorCapture/updateGameOver/gameOver/researched` → sim fns + events; remotes are plan 21. |
| `core/GameState.java` | `sim::state::{GameState, State}` | `set()` fires `StateChangeEvent` before assignment (`GameState.java:60-66`); `is_*` helpers ported 1:1. |
| `core/Time.java` (does not exist) / Arc `arc.util.Time` | `sim::time::{SimClock, RunQueue}` | See §3.8. |
| `entities/EntityGroup.java` | `entities::group::{EntityGroup, EntityIndexer}` | Slab + id map + optional quadtree; `nextId`/`checkNextId` → `entities::lifecycle::EntityIds`. |
| `entities/EntityIndexer.java` | `entities::group::EntityIndexer` | One-method trait (`change(t, index)`); used for stable slot fixups. |
| `entities/GroupDefs.java` | `entities::groups::{Groups, GroupKind, MEMBERSHIP_TABLE}` | Canonical 10 groups; membership per def computed like `EntityProcess.java:273`; `all` + Building `excludeGroups` behavior preserved. |
| generated `mindustry.gen.Groups` | `entities::groups::Groups` resource | Query-free explicit sets for stable order. |
| `entities/comp/*Comp.java` | `entities::comp::*` | One struct + component behavior systems per source component; Unit/Building/Bullet/Player bases stay data. |
| `entities/comp/EntityComp.java` | `entities::comp::base::{BaseEntity, SimId, DefId, Local, Remote}` | `@BaseComponent`; `self()/as()` disappear (Bevy handles). |
| `annotations/Annotations.java` `@Component/@Import/@Replace/@MethodPriority/@SyncField/@SyncLocal/@NoSync/@NoSerialize/@ReadOnly` | `mind-macros` + `entities::meta::{FieldMeta, FieldKind, SystemOrder}` | Metadata-only annotations; no codegen merging. |
| `annotations/entity/EntityProcess.java` | `entities::meta::EntityRegistry` + `mind-macros::entity_def!` | Group membership, def naming, ordered method table. |
| `annotations/entity/EntityIO.java` + `classids.properties` + `revisions/` | `entities::mapping::EntityMapping`, `assets/classids.properties`, revision files consumed by plan 04 | Plan 05 emits field/sync metadata; plan 04 owns byte I/O and revision dispatch. |
| `game/EventType.java` | `sim::events::{SimEvent, Trigger, EventBus}` | All event classes + 41 triggers; synchronous typed dispatch. |
| `async/AsyncCore.java` | `async_work::AsyncCore` | begin/process/end; deterministic join. |
| `async/PhysicsProcess.java`, `async/AvoidanceProcess.java` | `async_work::{physics, avoidance}` | Algorithms ported; plan 11 supplies pathfinder avoidance sampling. |
| Arc `Events`, `arc.util.Time`, `Tmp`, `Pools` | `sim::events`, `sim::time`, `util::tmp`, `util::pools`, `entities::lifecycle::EntityPool` | Discipline preserved, storage made explicit/thread-local. |
| `game/Rules.java`, `game/Teams.java` | `game::rules`, `game::teams` | Defined by plan 12; plan 05 ships the minimal boundary structs (§3.12). |
| `core/World.java`, `world/Tiles.java` | `world::grid` | Plan 06; plan 05 calls only `world.tiles`, `world.invalid_map()`, `world.load_*`, `world.checksum_part()`. |
| `core/PerfCounter.java` | `sim::TickReport` + `tracing` spans (feature `perf`) | Profiler UI is plan 14; counter enum is not ported. |
| `logic/GlobalVars.java` | `logic::GlobalVars` (plan 13) | `LogicVars` set calls `update()`. |
| `ai/{Pathfinder,BlockIndexer,...}` | `ai::*` (plan 11) | `WeatherAndAi` hosts their update hooks; `Init`/`Reset` events already exist. |

## 5. Milestones & task breakdown

Each milestone is verifiable through the plan-00 spine and `mind-headless`; evidence goes in the Changelog. Order is strict.

**M0 — Crate skeleton + boot (smallest vertical slice).**
- Create `mind-core` crate, `mind-macros` crate (empty macro), module tree §3.1.
- `Sim`, `SimBuilder`, `SimConfig`, `GameState`, `State`, empty `SimSchedule` with `TickSet` chain and a `tick()` that runs it.
- `Platform`/`HeadlessPlatform`, logging, `BootError`.
- Verify: `cargo test -p mind-core tests::initialization` (port of `ApplicationTests.initialization`); `mind-headless sim --ticks 10`; `cargo clippy` clean.

**M1 — `Time` + `Tmp` + RNG + pools.**
- `SimClock`, `RunQueue`, `Tmp`, `TempVec`, `EntityIds`, `SimRng`, `util::pools`.
- Verify: ported `timers`, `manyTimers`, `longTimers`; `rng::tests::stream_isolation`.

**M2 — Events + Triggers.**
- `EventBus`, `SimEvent` for every event `Logic` fires, `Trigger` enum, drain systems + order trace.
- Verify: `events::tests::{sync_listener, trigger_order, reused_payload_reentrancy_guard}`.

**M3 — `GameState` transitions + `play`/`reset`/pause.**
- `StateChangeEvent`, `Logic::play` port (wavetime, stats reset, loadout, core heal), `Logic::reset` port (Groups.clear, Time.clear, ResetEvent, world reset hook, new GameState + StateChangeEvent).
- Verify: `tests::reset_play_cycle`; state change event sequence assertions.

**M4 — Entity framework core.**
- Base components, `EntityIds` wiring, `EntityDef`, `EntityRegistry`, `GroupMask` computation, `EntityMapping` + `classids.properties` parity loader, `EntityGroup`, `Groups`, `EntityPool`, `sim_spawn`/`sim_remove`.
- Verify: `entities::tests::{add_remove_index_consistency, id_collisions, classids_parity, pool_reset, group_membership_rules}`; 100k spawn/remove fuzz.

**M5 — Composition macro + metadata.**
- `#[derive(SimComponent)]`, `entity_def!`, `SystemOrder`, `FieldMeta`; `mind-headless meta entities --out` JSON; docs for plans 04/21.
- Verify: `meta::tests::field_meta_golden`, `meta::tests::replace_and_priority_order`; JSON snapshot under `tests/golden/`.

**M6 — Full schedule with stubs.**
- Implement every `TickSet`/`EntitySet`, run conditions, editor filter, `checkGameState` skeleton, `EnemyCount`, env attrs, wave timer + `runWave` dispatch hook, `post` queue, `TickReport` (per-set durations + `unimplemented_stub` count).
- Verify: `trace order` scenario equals golden file; run conditions asserted for menu/paused/editor/playing.

**M7 — Async workers.**
- `AsyncCore`, `PhysicsProcess`, `AvoidanceProcess`, deterministic sharding; hook `PhysicsBullets`/`PhysicsUnits`.
- Verify: `workers=1` vs `workers=4` checksums identical over 10k ticks; `async_work::tests::shard_join_order`.

**M8 — Determinism + command log + bench.**
- `SimCommand`, `CommandLog`, `Checksummer`, `snapshot()`.
- Verify: §7.2 scenarios `sim_core_boot`, `sim_core_determinism`, `sim_core_schedule_order`; `cd`/`bench` budgets in §7.4.

**M9 — `mind-gdext` integration + MCP.**
- `SimBridge` autoload + inspector fields + pause/resume; hooks registration; MCP scenario §7.3; alloc-audit CI flag.
- Verify: MCP steps recorded; screenshot attached to Changelog.

## 6. Data & formats

### 6.1 IDs

| ID | Type | Rules |
|---|---|---|
| `EntityId` (Java entity `id`) | `i32` | Monotonic `EntityIds::next_id`; `check_next_id` after load; never reused across resets (`lastId` persists in Java too); collisions detected by `EntityGroup::check_id_collisions` test helper. |
| `EntityDefId` | `u16` | Internal registry index. |
| `ClassId` | `u16` | Exact mirror of `annotations/src/main/resources/classids.properties`; append-only; mod defs use `max+1` sorted-name assignment via `EntityRegistry::register`. |
| `ContentId` | `u16` | Plan 02. |
| `TeamId` | `u8` (0-255) | Plan 12; `Team.get(id) & 0xff` semantics. |
| `TimerId` | `u64` sequence | Deterministic `Time.run` ordering key. |

### 6.2 Field metadata (`FieldMeta`)

```rust
pub struct FieldMeta {
    pub name: &'static str,
    pub kind: FieldKind,
    pub ty: FieldType,          // F32, I32, Bool, Entity, Content, Struct(...)
    pub revision_added: u16,    // plan 04 revision bookkeeping
}
pub enum FieldKind {
    Plain,                      // serialized + synced
    SyncFloat { clamped: bool, interp: bool },
    SyncLocal,
    NoSync,
    NoSerialize,
    Transient,
    ReadOnly,
}
```

`mind-headless meta entities --out entitymeta.json` emits a stable, sorted JSON (`defs[] { name, class_id, groups[], pooled, serialize, fields[] }`). Plan 04 uses `FieldKind::{Plain, SyncFloat, NoSerialize, Transient}` for `.msav`/`TypeIO`; plan 21 uses `SyncFloat`/`SyncLocal`/`NoSync` for snapshots and interpolation companion slots. Golden file committed so field additions are visible in review.

### 6.3 Entity def registration format

- `entity_def!` macro emits `static ENTITY_DEFS: &[EntityDef]` in `entities::defs`; each `EntityDef` includes `groups: GroupMask` computed by the same rule as §3.6.
- `assets/classids.properties` is a committed copy of the upstream file (same content, GPL header). `classids_parity` test parses both if the upstream checkout is present, else asserts against embedded expected values.
- Revision JSONs: plan 04 owns `mind-core/src/io/revisions/`; plan 05's metadata test asserts the latest revision's field set matches `FieldMeta` for defs whose names are shared with upstream.

### 6.4 `SimCommand` / `CommandLog`

```rust
pub enum SimCommand {
    Place { x: i16, y: i16, block: u16, rotation: i8, team: u8, player: Option<i32> },
    Break { x: i16, y: i16, player: Option<i32> },
    Configure { x: i16, y: i16, value: ConfigValue },
    UnitCommand { units: SmallVec<[i32; 32]>, command: u16, x: f32, y: f32 },
    SetRules(Box<rules::Rules>),               // plan 12
    SpawnUnit { unit: u16, x: f32, y: f32, team: u8 }, // server/harness only
    Custom { kind: u16, data: SmallVec<[u8; 32]> },    // mod/data extension
}
pub struct CommandLog { pub header: LogHeader, pub entries: Vec<(u64 /*tick*/, SimCommand)> }
pub struct LogHeader { pub format: u32, pub seed: u64, pub map: String, pub map_hash: u64, pub build: String, pub content_hash: u64 }
```

- Text form (harness/dev): `.simlog` JSON Lines, one header line then one command per line. Binary form is plan 04's codec.
- `Sim::command(SimCommand)` applies the command through the same code path as the relay would; invalid commands return `CommandError` and are logged, never panic.
- Replay: `mind-headless replay file.simlog --through-tick N --checksum-every 60` calls `tick()` then applies commands whose tick matches before the next tick, exactly like plan 21's ordered application.

### 6.5 Checksum

```rust
pub struct Checksum(u64);                 // FNV-1a-64, locally implemented (no dep)
pub trait ChecksumPart { fn checksum_into(&self, h: &mut Hasher); }
```

Hasher order is fixed and versioned (`CHECKSUM_VERSION: u32 = 1`): `GameState` scalar fields → `SimClock.time`/runs count → `RngStream::Sim` state → each `Groups` member in slot order: `SimId`, `DefId`, then every `FieldKind::{Plain, SyncFloat}` field in `FieldMeta` order → `WorldGrid::checksum_part()` (plan 06). `Checksum` also included in `TickReport`. Any change to hashing bumps the version (plan 21 compares versions before desync judgment).

### 6.6 Constants and files

- `mind_core::constants::{TICKS_PER_SECOND=60, MAX_TICKS_PER_FRAME=4, TILESIZE=8, MAX_BLOCK_SIZE=16, FINAL_WORLD_BOUNDS=250.0}`.
- `assets/classids.properties` (upstream copy), `tests/golden/entitymeta.json`, `tests/golden/sim_core_boot.checksum`, `tests/golden/sim_core_schedule_order.txt`, `tests/fixtures/maps/sim_core_flat_128.msav` (plan 04/06 produce the fixture; plan 05 can start with a synthetic empty grid).
- `bench/baselines.json` owned by plan 23; this plan contributes the `sim_core` profile entries.

## 7. Oracle & verification (REQUIRED)

### 7.1 Ported tests (`mindustry/tests/src/test/java` → `cargo test`)

| Java test | Rust test | Notes |
|---|---|---|
| `ApplicationTests.initialization` | `mind_core::tests::initialization` | boot + registry + content map non-empty. |
| `ApplicationTests.timers` | `mind_core::sim::time::tests::timers` | 1.9999 tick delay runs on second `clock.update()`. |
| `ApplicationTests.manyTimers` | `mind_core::sim::time::tests::many_timers` | 100k timers, one update, all run. |
| `ApplicationTests.longTimers` | `mind_core::sim::time::tests::long_timers` | long delay + 100 steps. |
| `ApplicationTests.arrayIterators` | `entities::groups::tests::iteration_no_alloc` | index-loop iteration, zero iterator allocations. |
| `ApplicationTests.resetWorld` fixture semantics | `tests::reset_play_cycle` | reset returns to menu, groups/time cleared. |
| `ApplicationTests.playMap` (schedule half) | `tests::play_sets_playing_and_wavetime` | world loading itself is plan 06. |
| `ApplicationTests.spawnWaves` (schedule half) | `tests::run_wave_fires_event` | full spawning is plan 11; assert event + wave increment with stub spawner. |
| `ApplicationTests.buildingOverlap`/`buildingDestruction` (ordering half) | `sim_core::tests::entity_order_build_before_bullets` | full behavior plans 07/11. |
| `PowerTestFixture` (`fakegen`, `createFakeTile`) | `mind_core::fixtures::power::{fake_producer_block, fake_battery, fake_direct_consumer, create_fake_tile}` | Reused by `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` tests; fixed `delta = 1.0` (was 0.5 in Java) documented in fixture. |
| `EntityGroup.checkIDCollisions` behavior | `entities::group::tests::id_collisions` | |
| `EntityGroup.removeIndex` wrong-index fallback | `entities::group::tests::remove_index_fallback` | |
| `Revision.equal`-style field-change detection (annotations) | `entities::meta::tests::field_meta_golden` | Rust-only: snapshot diff catches field changes. |

Tests gated on other plans are marked `#[ignore = "plan NN"]` and un-ignored by that plan's milestone; the table above records ownership so `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` can track them.

### 7.2 Headless harness scenarios (`mind-headless`)

1. `run sim_core_boot --ticks 600 --seed 12345 --dump out/boot.json`
   - Assertions: state `playing` after `boot -> play`, `tick == 600`, group counts stable, zero `unimplemented_stub` warnings beyond the allow-list, `checksum == tests/golden/sim_core_boot.checksum`.
2. `replay sim_core_determinism.simlog --through-tick 3600 --checksum-every 60 --workers 1`
   - Same seed + command log (50 place/break/config ops + 10 unit commands injected at fixed ticks); run twice in-process and twice in fresh processes; all 61 checksums equal `tests/golden/sim_core_determinism.checksums`. Also run with `--workers 4`; identical output.
3. `trace order --ticks 3 --out out/order.txt`
   - Exact `TickSet`/`EntitySet` execution trace equals `tests/golden/sim_core_schedule_order.txt`, including run-condition skips for menu, paused, editor, and `!client` branches. Fails loudly if any set is reordered or added without updating the golden file.
4. `run sim_core_reset_play_cycle --cycles 20 --ticks 120`
   - After each `reset`: groups empty, `state == menu`, `clock.time == 0`, entity count returns to baseline; after each `play` + 120 ticks no panic; alloc-audit delta over cycles is zero after warmup.
5. `meta entities --out out/entitymeta.json`
   - Stable JSON equals `tests/golden/entitymeta.json`; `classids.properties` parsed without missing/extra names.
6. `bench sim_core --profile {empty,mid,stress} --ticks 3600 --json out/bench.json`
   - Numbers checked by plan 23 CI gate.

### 7.3 MCP playtest scenario (open-godot-mcp)

Assumes the plan-00 spine scene `res://scenes/spine.tscn` with autoload node `/root/Spine/SimHost`; adapt paths to what plan 00 actually ships.

1. `godot_health` → `ok: true`.
2. `godot_game play` (main scene) → game process starts.
3. `godot_runtime_state inspect /root/Spine/SimHost properties ["tick","state","group_counts","checksum"]` → `tick` > 0, `state == "playing"`, `group_counts` contains `unit`/`build`/`bullet`/`all` keys.
4. Wait 1 s; inspect again → `tick` increased by ≈60.
5. `godot_game pause`; wait 0.5 s; inspect twice → `tick` constant, `state == "paused"`.
6. `godot_game resume`; wait 0.5 s; inspect → `tick` increasing again.
7. `godot_screenshot` → save path recorded in Changelog; inspector overlay shows tick + group counts.
8. `godot_log errors` → empty (warnings allowed only for content stubs).
9. `godot_game stop`.

### 7.4 Performance budget + measurement

Baseline from HLP §7.4: sim tick ≤ 4 ms at mid-game load; 16.6 ms frame budget at 60 tps.

| Profile | Load | Budget (p99, dev machine) |
|---|---|---|
| `empty` | 128×128 grid, no entities | ≤ 0.5 ms/tick |
| `mid` | 256×256 grid, 600 buildings, 300 units, 3 000 bullets, 4 000 conveyor items, 8 teams | ≤ 4.0 ms/tick |
| `stress` | 512×512 grid, 2 000 buildings, 1 000 units, 10 000 bullets | ≤ 10 ms/tick |

- Measurement: `mind-headless bench sim_core --profile mid --ticks 3600 --warmup 600 --json` (p50/p95/p99 from `TickReport` spans) and `cargo bench -p mind-core --bench sim_core_tick` (criterion) for per-set attribution.
- Allocation guarantee: `mind-core` builds with feature `alloc-audit` (custom `GlobalAlloc` counting allocs). `bench --assert-alloc 0` fails if any allocation occurs during steady-state ticks after warmup (Bevy query/archetype first-use allocations are excluded by the warmup contract). `Time.run` closures and explicitly documented lazy caches are the only allowed allocations, and never inside `TickSet::EntityUpdate`.
- Determinism guard: `bench --checksum` must equal the golden checksum for the same seed, proving the optimizations did not change behavior.

### 7.5 Exit criteria checklist

- [ ] `cargo test -p mind-core` green without Godot/network; all §7.1 rows implemented or explicitly `#[ignore = "plan NN"]` with an owner.
- [ ] `cargo fmt --check` + `cargo clippy -p mind-core -- -D warnings` clean; no `HashMap` iteration in `sim/`/`entities/`.
- [ ] `mind-headless trace order` equals golden; order assertions cover menu/paused/editor/playing.
- [ ] Determinism scenario: 2 in-process + 2 cross-process runs with 1 and 4 workers produce identical 61-checkpoint checksum logs.
- [ ] Reset/play cycle ×20: no entity/group/time leak; alloc-audit zero after warmup.
- [ ] Entity group fuzz: 100k adds/removes with index/id-map consistency; O(1) removal verified by bench assertion (no O(n) scans).
- [ ] `meta entities` golden matches; `classids.properties` parity test passes.
- [ ] MCP scenario §7.3 executed with logs/screenshot attached; pause/resume proven.
- [ ] `bench sim_core` within all §7.4 budgets, p99 included, checksum unchanged.
- [ ] Plugin API documented for plans 06/07/10/11/12/13/21: set names, spawn/remove, event registration, metadata access — each consumes without editing core files.

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Status |
|---|---|---|---|
| R1 | Bevy ECS version/API churn (system sets, component hooks, schedule API) | Wrap all Bevy touch-points in `sim::{schedule, lifecycle}` helpers; version pinned by plan 00; no Bevy types in public plugin API beyond `World`, `Entity`, `SystemSet` | watch; reconcile with plan 00 Cargo pin |
| R2 | Java swap-removal order vs stable iteration | **Insertion-stable slab** removal (§3.6) | **NEEDS USER DECISION** (OD-05-A): if exact Java relative update order matters to plans 10/11 float tie-breaks, switch `EntityGroup` to swap-removal; deterministic either way |
| R3 | Multithreaded physics determinism | Deterministic slot-sharded writers + ordered join; `workers=1` exercised in replay | **NEEDS USER DECISION** (OD-05-B) only if the user wants worker threads disabled entirely in authoritative replay; default keeps them and proves equality |
| R4 | `Rules`/`Teams` split ownership with plan 12 | Plan 05 ships minimal boundary structs + `// plan 12` markers; plan 12 takes merge ownership of `game/rules.rs`, `game/teams.rs` | orchestrator must reconcile (see §3.12) |
| R5 | Proc-macro `mind-macros` vs handwritten metadata | `mind-macros` crate, proc-macro only, no runtime deps; `entity_def!` usable without derive for bootstrapping | accepted; note in plan 00 workspace lints |
| R6 | Content load ordering vs `Groups` init (`Vars.init` calls `Groups.init()` first) | `EntityRegistry` + `Groups` are built before content init; unit defs bound during `UnitType::init` (plan 02) | reconcile with plan 02 |
| R7 | `Time.delta` fixed 1.0 breaks code that assumed variable delta (e.g. `BuildingComp.update` timeScale) | 60 tps parity intended; any system needing real elapsed time uses `SimClock.time` ticks | documented deviation §2.3 |
| R8 | Event drain granularity vs Java immediate listeners | Drain after every firing set + `fire_now` for exclusive code; §7.2 scenario covers event-order-sensitive flows | accepted |
| R9 | `classids.properties` drift if plan 04 chooses its own save format (OD2) | Keep upstream IDs regardless; class IDs are also the mod ABI | accepted |
| R10 | Godot node path assumptions in §7.3 differ from plan 00 | Plan 00's actual autoload path is authoritative; update §7.3 when plan 00 lands | watch |
| R11 | `WeatherAndAi`/`TeamStats` stubs could hide schedule holes until plan 12 | `TickReport.unimplemented_stub` counts + trace golden make stubs visible; exit checklist requires them to be no-ops, not omissions | accepted |

## 9. References

Read in full for this plan:

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 locked decisions, §2 architecture, §4 template, §6–§9 conventions; §3 plan rows for 02/04/05/06/07/10/11/12/13/21/23).
- `mindustry-godot/PRELIMINARY_PLAN.md`.
- `Mindustry/AGENTS.md`, `Mindustry/core/AGENTS.md`, `Mindustry/core/src/mindustry/AGENTS.md`.
- `Mindustry/core/src/mindustry/core/AGENTS.md`, `entities/AGENTS.md`, `game/AGENTS.md`, `ai/AGENTS.md`.
- `Mindustry/annotations/AGENTS.md`, `Mindustry/tests/AGENTS.md`.
- `Mindustry/core/src/mindustry/Vars.java`, `ClientLauncher.java`.
- `Mindustry/core/src/mindustry/core/Logic.java`, `core/GameState.java`.
- `Mindustry/core/src/mindustry/entities/EntityGroup.java`, `EntityIndexer.java`, `GroupDefs.java`.
- `Mindustry/core/src/mindustry/entities/comp/EntityComp.java`, `UnitComp.java`, `BuildingComp.java`, `PowerGraphUpdaterComp.java`.
- `Mindustry/core/src/mindustry/game/EventType.java`.
- `Mindustry/core/src/mindustry/async/AsyncCore.java`.
- `Mindustry/annotations/src/main/java/mindustry/annotations/Annotations.java`, `BaseProcessor.java`, `entity/EntityProcess.java`.
- `Mindustry/annotations/src/main/resources/classids.properties`, `revisions/` (sampled `alpha/5.json`, `BuildingComp/1.json`, `BulletComp/2.json`).
- `Mindustry/tests/src/test/java/ApplicationTests.java`, `tests/src/test/java/power/PowerTestFixture.java`.

## Changelog

- 2026-10-01 — Draft v1 written. No implementation started. Open decisions OD-05-A (group removal semantics) and OD-05-B (worker threads in authoritative replay) require user input before M4/M7 respectively; R4 requires orchestrator reconciliation with plan 12.
- 2026-10-02 — **CROSS-LANE HANDOFF: `mind-macros` crate landed (plan 03 M6 dependency).** New proc-macro crate at `client/rust/mind-macros` (registered in `client/rust/Cargo.toml`; `mind-core` depends on it). Public API:
  - `#[derive(SimComponent)]` (helper attribute `#[sim(...)]`) emits an impl of `mind_core::entities::meta::SimComponentMeta` (`component_meta() -> &'static ComponentMeta`). Struct options: `component`, `base`, `name = "..."`, `methods(update_priority = 0, ...)`. Field options: `sync_float[(clamped[, interp])]`, `sync_local`, `no_sync`, `no_serialize`, `transient`, `read_only`, `since = N`, `name = "..."`.
  - `#[entity_def]` function-like macro emits `pub static ENTITY_DEF_SPECS: &[EntityDefSpec]` from `Name = [Path, ...];` entries (component names are stringified from the last path segment; no resolution needed).
- 2026-10-02 — **M0–M3 landed.** M0: `constants` (`TICKS_PER_SECOND`/`FIXED_HZ`/`MAX_TICKS_PER_FRAME`/`CHECKSUM_VERSION`/`FINAL_WORLD_BOUNDS`), `platform::{Platform, HeadlessPlatform, LogLevel}`, `platform/hooks::{ClientHooks, ClientHooksHandle, NoopClientHooks}`, `sim::config::SimConfig`, `sim::boot::{SimBuilder, BootError, TickReport}` + `Sim::tick_report()`; `tests::initialization` (`ApplicationTests.initialization` port: boot + registry + content + 10 ticks + seed stability). M1: `sim::clock::{SimClock, RunQueue}` (micro-tick `BinaryHeap` + lazy cancel; ported `timers`/`manyTimers`/`longTimers`), `determinism::rng::{SimRng, RngStream}` (isolated `Sim`/`MapGen`/`Waves`/`Fx` streams over `JavaRandom`; `stream_isolation`), `util::{tmp::{Tmp, TempVec}, pools::VecPool}`, `EntityIds` (M4 module). M2: `sim::events::{Trigger (all 41 EventType.Trigger values), TriggerRegistry, ALL_TRIGGERS}` + new bus events `ResetEvent`/`PlayEvent`/`WaveEvent`/`MusicRegisterEvent` (**HLP §12 C7 closed**). M3: `GameState` gains `update_id` + `is_game/is_playing/is_paused/is_menu`; `sim::reset` (`play`/`reset`/`pause`/`resume`, clearing entities/grid/clock, firing `StateChangeEvent`/`PlayEvent`/`ResetEvent`/`Trigger.newGame`); `sim::logic` run-condition helpers + `check_game_state` stub; `Sim::tick` now fires `Trigger.update`→set chain→`Time.update`→`SimClock::update`→`advance`→`Trigger.afterGameUpdate`. Evidence: `cargo test -p mind-core --lib` → **233 passed / 1 ignored**; fmt + clippy `-D warnings` clean; `mind-headless sim 10 --json` emits per-tick checksums; all three P0 goldens still pass unchanged (`2033eb5b4ec1206d` / `791592fab6a00469` / `37cc80ef11cfec1e`). Still open: M6 full `TickSet` schedule + trace golden, M7 async workers, M8 canonical FNV-1a checksum/command log/bench + golden re-record.
  - **Plan 03 M6 steps to add `#[derive(LoadRegions)]` (purely additive):** (1) in `mind-macros/src/lib.rs` add `#[proc_macro_derive(LoadRegions, attributes(load))] pub fn derive_load_regions(...)` parsing `#[load(...)]` and emitting an impl of a trait owned by the consuming crate (follow `expand_sim_component` as the template); (2) the consuming crate adds `mind-macros = { path = "../mind-macros" }` (already a `mind-core` dep) and defines the trait the derive targets; (3) do **not** add runtime deps to `mind-macros` (proc-macro only: `syn`/`quote`/`proc-macro2`). Verified: `cargo check -p mind-macros` + `cargo clippy -p mind-macros --all-targets -- -D warnings` clean.
- 2026-10-02 — **M4 + M5 core landed (partial).** `mind-core::entities`: `meta` (`FieldMeta`/`FieldKind`/`FieldType`/`ComponentMeta`/`SystemOrder`/`EntityDef`/`EntityDefSpec`/`EntityRegistry`), `groups` (`GroupKind`/`GroupMask`/`GROUP_DEFS`/`Groups` with the exact `EntityProcess.java:273` membership rule), `group` (`EntityGroup` slab + O(1) remove + id map + wrong-index fallback + `EntityIndexer`), `lifecycle` (`EntityIds` monotonic/`check_next_id`, `EntityPool(s)`, `sim_spawn`/`sim_remove`, `SimEntity`), `mapping` (`EntityMapping` + committed `assets/classids.properties` copy, 51 rows), `comp::{base,markers}`, `defs` (`entity_def!` vanilla set + `vanilla_registry()`). `mind-headless meta entities --out` emits the stable JSON; golden committed at `mind-core/tests/golden/entitymeta.json` and asserted by `tests/sim_core_meta.rs`. Evidence: `cargo test -p mind-core` → **209 passed + 2 integration passed / 1 ignored** (baseline was 192); fmt + clippy (`-D warnings`) clean. Reconciliation: plan 04's `io::entity::{EntityDefMeta, FieldDesc}` stays the IO surface; `FieldMeta::to_field_desc()` is the bridge (no duplicate codec).
