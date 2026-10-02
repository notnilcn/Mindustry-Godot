# 11 — UNITS, UNIT AI, PATHFINDING & WAVES IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft v1 — 2026-10-01, not started |
| **Phase** | P4 — Combat, units, campaign (HIGH_LEVEL_PLAN §5) |
| **Depends on** | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (ECS framework, `Groups`, schedule, events, async slots, `SimCommand`), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (`Tile`/`Tiles`/`World`, tile-change events, `World.raycast`), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (building entities, `Build.validPlace`, `ConstructBlock`, block flags, `TeamData` building sets), `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (`Weapon` runtime mounts, `BulletType`/damage, `Predict`, status application). Content metadata (`UnitTypeDef`/`WeaponDef`/`EntityDefSpec`/`UnitCommandDef`/`UnitStanceDef`) is owned by `02_CONTENT_IMPLEMENTATION_PLAN.md`; entity revisions by `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`; payload carrier semantics by `08_LOGISTICS_IMPLEMENTATION_PLAN.md`. |
| **Blocks** | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Rules.spawns`, `TeamData.buildAi`/`rtsAi`, `CampaignRules` difficulty, `Objectives` unit hooks), `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` (`LogicAI` + `LUnitControl`), `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (drag-select, command queue UI, formations, control groups), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (unit render data), `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (unit/leg/segment/weapon parts), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (unit command relay, spawn/death sync, `@SyncField` interpolation for units). |
| **Sources (read in full)** | Mindustry AGENTS: `core/src/mindustry/entities/AGENTS.md`, `ai/AGENTS.md`, `type/AGENTS.md`, `game/AGENTS.md`, `world/blocks/AGENTS.md` (units section), `annotations/AGENTS.md`, `io/AGENTS.md`. Code: `entities/comp/{UnitComp,MechComp,LegsComp,TankComp,WaterMoveComp,WaterCrawlComp,UnderwaterMoveComp,CrawlComp,SegmentComp,ElevationMoveComp,PayloadComp,BuilderComp,MinerComp,BlockUnitComp,UnitTetherComp,BuildingTetherComp,ChildComp,OwnerComp,DamageComp,ItemsComp,ShieldComp,ShielderComp,StatusComp,WeaponsComp,SyncComp,TimerComp,TimedComp,TimedKillComp,PhysicsComp,HitboxComp,HealthComp,PosComp,RotComp,VelComp,TeamComp,TargetDummyComp,EntityComp}.java`, `entities/Units.java`, `entities/units/{UnitController,AIController,WeaponMount,BuildPlan,StatusEntry}.java`, `ai/*.java` (`Pathfinder`, `ControlPathfinder`, `Astar`, `PathfindQueue`, `UnitGroup`, `BlockIndexer`, `UnitCommand`, `UnitStance`, `ItemUnitStance`, `RtsAI`, `BaseBuilderAI`, `BaseRegistry`, `WaveSpawner`), `ai/types/*.java` (all 17), `game/{Waves,SpawnGroup}.java`, `world/blocks/units/*.java` (all 9), `type/UnitType.java` (`create`/`spawn`/`init`/`getUnitStances`), `io/TypeIO.java` (`writeController`/`readController`), `annotations/src/main/resources/{classids.properties,revisions/*}` (unit defs), `tests/src/test/java/ApplicationTests.java` (`spawnWaves`, `checkPayloads`, `allPayloadBlockTest`, `testSectorValidity` wave/sector assertions). |
| **Extends spine** | Adds a `Units` inspector tab (`unit_count`, `wave`, `ai_stub_count`, per-kind counts), GDExtension autoload `MindUnits` (`spawn_unit`, `command_move`, `command_stance`, `unit_state`, `wave_info`, `set_wave`), `godot_exec` eval API via `MindUnits`, and `mind-headless` subcommands `unit spawn|path|command|wave|factory|cargo|bench-air|bench-path`. |

**Locked inputs treated as constants:** GPL-3.0 (D6) with the ported-file header; pure Rust/GDExtension (D1); fixed 60 Hz sim (D8); full parity (D3); `mind-core` is Godot-free and tokio-free; no `HashMap` iteration in sim paths (HLP §2.4); content IDs and entity class IDs append-only.

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Unit-side component set.** Every `entities/comp/*Comp.java` reachable from a unit `@EntityDef` is ported to a Bevy component (or a field grouping inside one) with upstream behavior, sync/save annotations, and `@Replace`/`@MethodPriority` semantics expressed through plan 05's component/system tables.
2. **`EntityDefSpec` vocabulary (owned here).** The concrete `ComponentKind` enum, the 12 unit defs (plus legacy flags), their component closures, class IDs, group masks, and the registry bridge consumed by plans 02 (registration) and 05 (`EntityDefs!`/`FieldMeta`).
3. **Unit lifecycle.** `UnitType.create(team)`, `spawn(...)` incl. segmented multi-unit spawn, `add()`/`remove()` through plan 05 pools, `unloaded()`, `killed()`, `kill()`, `destroy()`, `notifyUnitSpawn` equivalent (local add + sync hook), team unit counts and caps, `afterRead`/`afterReadAll` controller restoration.
4. **`UnitType` runtime.** `init()` derived fields (already specified in plan 02 metadata; the runtime re-derivation and live `sample`/`constructor` binding lives here), controller factory dispatch, command/stance auto-population re-check, per-unit ability instances, weapon mount setup (`setupWeapons`), elevation/boost state.
5. **Unit kinds / movement physics.** Mech, legs, tank, water-move, water-crawl, underwater, crawl, segmented, elevation-move (hover), block-unit, building/unit-tether, timed-kill (missile), target dummy; leg IK (`InverseKinematics.solve`), tread animation state, crawl/segment rotation, payload coupling hooks into plan 08.
6. **Controllers.** `UnitController` + `AIController` base behavior and all `ai/types/*`: `GroundAI`, `HugAI`, `FlyingAI`, `FlyingFollowAI`, `CommandAI`, `BuilderAI`, `RepairAI`, `MinerAI`, `CargoAI`, `DefenderAI`, `SuicideAI`, `MissileAI`, `LogicAI`, `NoAI`, `BoostAI`, `AssemblerAI`, `PrebuildAI`, plus `Player` controller bridge (plan 15 owns input, not the controller body).
7. **Pathfinding.** `Pathfinder` flowfields (`PathTile` packing, 6 cost types, field types, deterministic incremental update model), `ControlPathfinder` 12×12 cluster HPA* (clusters, portals, inner A*, cluster A*, per-unit `PathRequest`s, cached flow fields, raycast/`raycastFastAvoid`), `Astar`, `PathfindQueue`, `BlockIndexer` (flagged buildings, ores, damaged, turret quadtree, tile-change maintenance).
8. **RTS.** `UnitGroup` formation computation, `CommandAI` command queue/stances/formation/attack-target logic, `UnitCommand`/`UnitStance`/`ItemUnitStance` runtime side (metadata in 02, registry lookup here), team `RtsAI`.
9. **Team AI.** `BaseBuilderAI`, `BaseRegistry` (`baseparts/*.msch` classification, `basepartnames` equivalent), core-unit spawning, team unit caps.
10. **Waves.** `WaveSpawner` runtime (spawn tile tracking, ground/flyer spawn resolution, attack-mode core spawns, spawn effects/invulnerability, shockwave, `UnitSpawnEvent`), `SpawnGroup` struct + `getSpawned`/`getShield`/`createUnit` + JSON keys, built-in `Waves` table, `Waves.generate(difficulty, rand, attack, airOnly, naval)`.
11. **Unit factories/blocks.** `UnitBlock`/`UnitBuild`, `UnitFactory`, `Reconstructor`, `UnitAssembler`/`UnitAssemblerModule`, `RepairTower`, `RepairTurret`, `UnitCargoLoader`, `UnitCargoUnloadPoint`.

### 2.2 “Done” means

- `cargo test -p mind-core` runs the unit/AI/wave tests of §7a headless with no Godot and no network.
- `mind-headless` scenarios `units_spawn_path_arrive`, `units_flowfield_costs`, `units_rts_command_queue`, `units_waves_difficulty`, `units_cargo_pickup_deliver`, `units_factory_output`, `units_legs_ik`, `units_segment_chain` pass against committed golden dumps/checksums.
- Wave counts, wave compositions and `SpawnGroup` JSON round-trips are byte-parity against golden generated from upstream (`parity/java/DumpWaves.java`, §7a), and the campaign sector validity assertions that involve spawns/indexer pass under plan 12's sector tests.
- The MCP scenario in §7c runs in the plan-00 rig: units spawn via the relay/headless API, a move command is issued in-engine, unit positions change and arrive; screenshot + logs attached.
- Perf budgets of §7d are met and recorded; `bench units` checksum unchanged with 1 vs 4 workers.

### 2.3 Explicit boundaries (who owns what)

| Area | Owner | Plan 11 provides |
|---|---|---|
| Bullet/weapon math, `BulletType`, `Damage`, mount firing (reload/velocity), `Predict` | **10** | Controller code that sets `mount.target`/`mount.shoot`; `WeaponMount`/`Weapon` runtime struct shapes reconciled with 10 |
| Blueprints/`Schematic`, `.msch` read, `Schematics.place`, UI schematics picker | **12** (data), editor **19** | `BaseBuilderAI` consumes decoded schematics; `BaseRegistry` classification |
| `Rules`/`TeamRule`/`Gamemode`/`Difficulty`/`CampaignRules`/`TeamData` container | **12** | `Rules.spawns: Vec<SpawnGroup>` type, `TeamData` AI handles (`build_ai`, `rts_ai`) and cap fields, difficulty hook interface |
| Campaign wave-table generation call sites (planet generators), attack-mode setup | **12** | `Waves::get()` + `Waves::generate()` algorithms |
| Logic statement parsing, `LUnitControl` variants, `LUnitControl` -> `LogicAI` setters, processors | **13** | `LogicAI` controller, timeout/reset behavior, `checkTargetTimer` radar-cache hook |
| UI command panel, stances bar, control groups, drag-select, placement queue | **15** | `CommandAI` public API (`command`, `command_position`, `command_target`, `command_queue`, `set_stance`), selection helpers, `UnitCommand`/`UnitStance` icons/localized keys |
| Unit/leg/segment/weapon draws, outlines, shadows, minimap icons | **16/17** | Component data (`legs`, `base_rotation`, `walk_time`, `segment_rot`, `payloads`, `elevation`, `trail` metadata) exposed read-only for the view |
| Payload carrier entities (`Payload`/`UnitPayload`/`BuildPayload`, conveyor/loader/unloader, power graph of payloads) | **08** | `PayloadComp` behavior (pickup/drop/accept/canPickup), payload-coupled `UnitType` fields |
| Turret blocks/interception | **10** | `RepairTurret`/`RepairTower` unit-side repair behavior (uses 10's `BaseTurret` consumers where applicable) |
| Command relay, spawn/death snapshots, ordered command application | **21** | `SimCommand` unit variants, deterministic command handlers, `EntityMapping`/controller codec |
| AI draw debug (`Trigger.draw` markers), marker rendering | **16** | Data + `!headless` guards |
| Global parity/bench suite | **23** | Scenario names, golden artifacts, budget numbers |

### 2.4 Deliberate deviations (reason stated)

1. **Deterministic bounded pathfinding instead of wall-clock daemon budgets.** Upstream `Pathfinder` spends `maxUpdate = 8 ms` per thread pass and refreshes every 100 ms wall time; `ControlPathfinder` spends 12 ms at 30 Hz. Wall-clock work budgets are not reproducible and would break lockstep (HLP §2.4). The port replaces them with **fixed work units per sim tick** (flowfield nodes, cluster/field nodes, request steps) executed inside the plan-05 schedule (or on a worker joined before AI reads, with a fixed node budget), so checksums are stable when `workers` changes. Numbers in §6.4; flagged **NEEDS USER DECISION** OD-11-A (budget sizes are tunable; the model is locked).
2. **One Bevy archetype per unit entity-def, not per unit type.** Java generates 12 unit classes; every unit instance has every field of its def, unused fields defaulted (e.g. `TimedKill.time` on a mech is absent — fields only exist where the component exists). Rust mirrors this exactly: 12 `EntityDefSpec`s → 12 bundles; a `dagger` and a `mace` share the `MechUnit` archetype. This keeps archetype churn at zero and revision IO per-def, matching `revisions/<def>/*.json`.
3. **No `Prov<UnitController>`/`Func<Unit,UnitController>` closures.** Controller selection is a data `ControllerKind` + a function-pointer registry (`ControllerRegistry::create(kind) -> ControllerState`); `UnitTypeDef.controller_kind`/`ai_controller_kind` come from plan 02. Mod JSON uses the same kind names (`ClassMap` short names).
4. **`Bits` (Java) → `StanceBits([u64; 1])`.** Vanilla has 30 stances (8 + 22 item stances); a fixed 64-bit mask preserves `andNot`/`get`/`set` semantics and stays copyable without allocation. If mods exceed 64, plan 20 appends a `SmallVec<u64>` variant (append-only, invisible here).
5. **`UnitGroup` formation is deterministic and join-free.** Upstream submits to `mainExecutor` and raycasts after the fact; the port computes formation synchronously inside the tick that triggers it (squads are ≤ 50 units; budget §7d). No partial `valid` flag semantics are observable.
6. **`Astar` static scratch becomes per-call scratch from `Tmp`.** Same algorithm, no cross-call global state (thread-safety by construction, matching HLP §6.1).
7. **Client-only branches (`if(!headless)` draws, sounds) move to view hooks.** The sim keeps the branch points as `ClientHooks` calls; no behavior difference in headless.
8. **`Core.app.post`-deferred pathfinder registrations become explicit tick-order publish steps** (main-list updates at `TickSet::AfterGameUpdate`); no wall-clock timing.
9. **Controller state is serialized by a dedicated codec, not `TypeIO` generic object reflection** (deviation 4 of plan 04 applies): `ControllerCodec` with the upstream type bytes (0, 3, 4, 5, 6, 7, 8, 9) and fields exactly as `TypeIO.writeController`/`readController` (`io/TypeIO.java:776-950`). Handed to plan 04/21.

---

## 3. Target design

All names below are final unless marked otherwise. Modules live in `client/rust/mind-core/src/` and compile into plan 05's `Sim`/schedule; nothing here reads Godot types.

### 3.1 Module layout

```
mind-core/src/
  entities/comp/unit/              # unit-side components (plan 05 base comps stay in entities/comp/base.rs)
    mod.rs                         # UnitCore (UnitComp fields+systems), closure definition
    pos_rot_vel.rs                 # PosComp/RotComp/VelComp/TeamComp unit additions (aliases of plan-05 base comps)
    health_physics_hitbox.rs       # HealthComp, PhysicsComp, HitboxComp
    items_shield_status.rs         # ItemsComp, ShieldComp, ShielderComp, StatusComp (StatusEntry)
    weapons_draw_sync.rs           # WeaponsComp, DrawComp, SyncComp
    miner_builder.rs               # MinerComp, BuilderComp (+ BuildPlan)
    payload.rs                     # PayloadComp
    mech.rs legs.rs tank.rs crawl.rs water_move.rs water_crawl.rs underwater_move.rs
    elevation_move.rs segment.rs block_unit.rs tether.rs child_owner.rs
    timed.rs timed_kill.rs target_dummy.rs damage.rs
    defs.rs                        # ComponentKind, EntityDefSpec, UNIT_DEFS (12), EntityMapping bridge
    lifecycle.rs                   # UnitSpawner, notify_unit_spawn, kill/destroy, reset controller, after_read
    queries.rs                     # Units.java port (closest/best/nearby/count/canCreate/getCap)
    weapon_mount.rs                # WeaponMount + setupWeapons (mount layout; firing math in 10)
    status_entry.rs                # StatusEntry (duration/effect), apply/extend/cancel
    inverse_kinematics.rs          # Mathf/Angles + IK solver used by LegsComp
  ai/
    mod.rs
    controller.rs                  # UnitController trait-object replacement, AIController base helpers
    controller_registry.rs         # ControllerKind -> ctor fn, Player/NoAI/LogicAI keep-state
    types/                         # one file per ai/types/*.java
      ground.rs hug.rs flying.rs flying_follow.rs command.rs builder.rs repair.rs
      miner.rs cargo.rs defender.rs suicide.rs missile.rs logic.rs no_ai.rs boost.rs
      assembler.rs prebuild.rs player_bridge.rs
    pathfinder/
      mod.rs                       # Pathfinder resource + schedule systems + events
      path_tile.rs                 # PathTile bit packing (frozen layout)
      cost.rs                      # 6 ground-AI cost types + 4 control cost types
      flowfield.rs                 # Flowfield, EnemyCoreField, PositionTarget, frontier
      worker.rs                    # deterministic incremental budget engine
    control_pathfinder.rs          # ControlPathfinder (clusters/portals/fields/requests)
    control_structs.rs             # FieldIndex/IntraEdge/NodeIndex bit packing (frozen)
    astar.rs pathfind_queue.rs
    unit_group.rs block_indexer.rs
    unit_command_runtime.rs unit_stance_runtime.rs item_unit_stance.rs
    rts_ai.rs base_builder_ai.rs base_registry.rs wave_spawner.rs
  game/
    spawn_group.rs                 # SpawnGroup + JSON
    waves.rs                       # Waves table + generate()
  world/blocks/units/
    mod.rs unit_block.rs unit_factory.rs reconstructor.rs
    unit_assembler.rs unit_assembler_module.rs
    repair_tower.rs repair_turret.rs unit_cargo_loader.rs unit_cargo_unload_point.rs
  math/rand_arc.rs                 # Arc Rand port (needed by Waves.generate + randomWaveAI)
```

### 3.2 `EntityDefSpec` and the `ComponentKind` vocabulary (owned by this plan)

`ComponentKind` is a `#[repr(u8)]` enum whose variant names are the upstream component class names minus `Comp`; string names are the ABI (mod JSON + plan 02 registries).

```rust
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ComponentKind {
    // unit base closure (UnitComp's implemented interfaces; always present)
    Pos = 0, Rot = 1, Vel = 2, Team = 3, Health = 4, Physics = 5, Hitbox = 6,
    Status = 7, Items = 8, Weapons = 9, Draw = 10, Sync = 11, Shield = 12,
    Miner = 13, Builder = 14, Owner = 15, Damage = 16, Timer = 17,
    // optional per-def kinds
    Mech = 18, Legs = 19, Tank = 20, WaterMove = 21, WaterCrawl = 22, UnderwaterMove = 23,
    Crawl = 24, Segment = 25, ElevationMove = 26, Payload = 27, BlockUnit = 28,
    BuildingTether = 29, UnitTether = 30, Child = 31, Timed = 32, TimedKill = 33,
    TargetDummy = 34, Shielder = 35, PlayerBridge = 36,
}
pub const KIND_COUNT: usize = 37;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDefSpec {
    /// Entity-def element name used by revisions/classids (first `@EntityDef` field).
    pub def_name: &'static str,          // "alpha", "mace", "corvus", "stell", ...
    /// Display/group name of the merged class (Java `<...>Unit`).
    pub class_name: &'static str,        // "Unit", "MechUnit", "LegsUnit", ...
    /// Component closure, base kinds first, then def kinds in canonical order.
    pub components: &'static [ComponentKind],
    /// Upstream `@EntityDef(legacy = true)`: affects revision read fallbacks only, never composition.
    pub legacy: bool,
}
```

Canonical unit defs (`defs.rs::UNIT_DEFS`, confirmed against `content/UnitTypes.java:34-93` and `classids.properties`):

| # | Java def (element) | class / Bevy bundle | components (beyond base closure) | class_id anchor | latest upstream revision |
|---|---|---|---|---|---|
| 1 | `alpha` (`Unit`) | `UnitBundle` | — | `alpha=0` | `alpha/5` |
| 2 | `mace` (`MechUnit`) | `MechBundle` | `Mech`, `ElevationMove` | `mace=4` | `mace/9` |
| 3 | `corvus` (`LegsUnit`) | `LegsBundle` | `Legs` | `corvus=24` | `corvus/9` |
| 4 | `stell` (`TankUnit`) | `TankBundle` | `Tank`, `ElevationMove` | `stell=43` | `stell/2` |
| 5 | `risso` (`WaterMoveUnit`) | `WaterMoveBundle` | `WaterMove` | `risso=20` | `risso/9` |
| 6 | `latum` (`CrawlUnit`) | `CrawlBundle` | `Crawl` | `latum=46` | `latum/2` |
| 7 | `elude` (`ElevationMoveUnit`) | `ElevationMoveBundle` | `ElevationMove` | `elude=45` | `elude/2` |
| 8 | `mega` (`PayloadUnit`) | `PayloadBundle` | `Payload` | `mega=5` | `mega/7` |
| 9 | `manifold` (`BuildingTetherUnit`) | `BuildingTetherBundle` | `BuildingTether`, `Payload` | `manifold=36` | `manifold/3` |
| 10 | `missile` (`TimedKillUnit`) | `TimedKillBundle` | `TimedKill`, `Timed` | `missile=39` | `missile/3` |
| 11 | `block` (`BlockUnit`) | `BlockUnitBundle` | `BlockUnit` | `block=2` | `block/9` |
| 12 | `dummy` (`TargetDummyUnit`) | `TargetDummyBundle` | `TargetDummy` | `dummy=49` | `dummy/0` |

Legacy declarations (`nova/pulsar/quasar` mech, `spiroct/arkyid/toxopid` legs, `mono/poly` unit, `quad/oct` payload) set `legacy = true` on the matching spec (they share the composition table; the flag is only consulted by plan 04's revision reader and by `after_read` controller reset rules). `classids.properties` is plan 05's committed copy; this plan must not re-derive IDs.

**Closure rule (must match plan 02's `EntityDefSpec` note and plan 05's `entity_def!`):** `UnitComp implements Healthc, Physicsc, Hitboxc, Statusc, Teamc, Itemsc, Rotc, Unitc, Weaponsc, Drawc, Syncc, Shieldc, Miner c, Builderc` → the base closure `[Pos, Rot, Vel, Team, Health, Physics, Hitbox, Status, Items, Weapons, Draw, Sync, Shield, Miner, Builder, Owner, Damage, Timer]` is implicit for every unit def; `MechComp implements ElevationMovec` (already in the mech spec), `SegmentComp`/`TimedKill`/`Timed` are explicit in their defs. Plan 02 stores the spec on `UnitTypeDef`; plan 05's `entity_def!` registers the 12 defs and their `FieldMeta`; this plan's `defs.rs` owns the constant tables and the `UnitBundle` constructors.

### 3.3 Composition semantics (mapping `@Import`/`@Replace`/`@MethodPriority`)

- Each component struct is a Bevy `Component` with `#[derive(SimComponent)]` metadata (plan 05). `@Import` fields are phantoms: component structs never duplicate `x`/`y`/`team`/`health`; behavior systems take the union through `SystemParam` and the `entity_def!` macro verifies owners are in the def closure.
- `update()` merge order is a per-def `SystemOrder` table built from upstream priorities and alphabetical class order. Unit-specific entries (all reused from plan 05's model, listed here because they are my port's responsibilities):
  - `UnitComp.update` is the canonical unit update body (bounds, drowning, boost falloff, floor damage, abilities, trail, knockback, AI controller call, despawn). It runs after `HitboxComp.update`, `HealthComp.update`, `StatusComp.update`, `PayloadComp.update`, `BuilderComp.update`, `MinerComp.update`, `WeaponsComp.update`.
  - `TimedComp.update` priority 100 (last, matching upstream `TimedComp`), `TimedKillComp` before it.
  - `MechComp.update` (`walked` handling) runs after `ElevationMoveComp`; `LegsComp.update` after `Rot`; `TankComp.update` after `ElevationMove`; `SegmentComp.update` head-first (recursion order == segment index order).
- `@Replace` becomes a `SystemOrder` entry with a `replaces(ComponentKind)` field: e.g. `MechComp.rotateMove` replaces `UnitComp.rotate_move`; `TankComp.floorSpeedMultiplier` replaces `UnitComp.floor_speed_multiplier`; `LegsComp.solidity`/`drownFloor` replace base implementations. Exactly one winner is enforced by the macro.
- `self()` becomes `Entity`; `controller instanceof X` becomes `ControllerKind` matching on `UnitControllerKind`.

### 3.4 Unit lifecycle

- **Spawn:** `UnitSpawner::spawn(spec: EntityDefId, team, x, y, rot, on_each)`:
  1. take an entity from plan 05's `EntityPool` for the def (or `world.spawn` the def bundle),
  2. `set_type(unit_type)` — health/drag/armor/hit_size from def, `setup_weapons` (mounts sized to `type.weapons`), abilities instantiated with `data` carry-over, `elevation = flying ? 1 : 0`, `heal()`, `TimedKill.lifetime(type.lifetime)`,
  3. `set_pos`, `rotation`, `add()` — team count `+1`, cap check (`Units::can_create`), physics entry, `BaseEntity.added = true`, groups populated,
  4. `on_each` callback (used by segmented spawn and `SpawnGroup.createUnit`),
  5. segmented units (`segmentUnits > 1`): spawn `segmentUnits-1` more from `segmentUnit`/`segmentEndUnit`, offset by `segmentSpacing`, chain `add_child`.
- **Remove:** plan 05 `sim_remove` calls unit `remove` hooks: team count `-1`, `controller.removed(unit)`, trail fade effect (`Fx.trailFade`), payload removal (`PayloadComp`), then pool return.
- **Death:** `kill()` → (host) `Units::unit_death(id)` → `killed()` (sets `was_player`, `dead = true`; flying+createWreck → wreck sound, else `destroy()`) → `destroy()` (item-based explosion, shake, sound, `UnitDestroyEvent`, shoot-on-death mounts, crash damage + shield absorb via 10, wreck decals, ability `death()`, `type.killed()`, `remove()`). `unitCapDeath`/`unitEnvDeath`/`unitDespawn`/`unitSafeDeath` map to explicit host functions fired by the same event path. All four are `SimCommand`-safe host functions; plan 21 relays them for clients (`notifyUnitSpawn` equivalent = `Groups.unit` add + `sync` group membership; clients receive via snapshot/relay, no local duplicate).
- **Timers:** `TimerComp::timer(index, time)` bitfield per unit (32 slots); `TimedComp::time()`; `TimedKillComp` removes/kills on lifetime; `StatusComp` holds `statuses: SmallVec<StatusEntry>` and dirty flags.
- **Save/load:** `afterRead` re-derives type fields and resets the controller unless `keepState()` (only `LogicAI`/`CommandAI` in vanilla); `afterReadAll` resolves `attackTarget`/segment parents/tethers by ID; `afterSync` re-binds controller to unit. `ControllerCodec` (§6.3) serializes the controller type byte + CommandAI state exactly as upstream.
- **Caps:** `Units::get_cap(team)` (wave team infinite unless PvP/campaign, `rules.disableUnitCap`, `team.ignore_unit_cap`, `unitCapVariable` + `TeamData.unit_cap`) via interfaces supplied by plan 12; `can_create(team, type)` = `!useUnitCap || count < cap && !type.is_banned()`.

### 3.5 Controller framework

```rust
pub trait UnitController: Send {
    fn unit(&self) -> Option<Entity>;
    fn set_unit(&mut self, u: Entity);
    fn init(&mut self, world: &mut World) {}
    fn update_unit(&mut self, world: &mut World) {}          // fallback chain handled by base helper
    fn hit(&mut self, world: &mut World, bullet: Entity) {}
    fn removed(&mut self, world: &mut World, unit: Entity) {}
    fn after_read(&mut self, world: &mut World, unit: Entity) {}
    fn is_valid_controller(&self) -> bool { true }
    fn is_logic_controllable(&self) -> bool { false }
    fn keep_state(&self) -> bool { false }
}
```

- Controllers are stored in a **transient** `UnitControllerSlot { kind: ControllerKind, state: ControllerState }` component; `ControllerState` is an enum (`Ground`, `Legsless`, `Command(CommandAiState)`, `Builder(BuilderAiState)`, `Miner(MinerAiState)`, `Logic(LogicAiState)`, `Missile`, `Assembler`, `Player { player_id }`, `None`) so Bevy storage stays archetype-stable.
- `AIController` base helpers are free functions over a `SystemParam` union: `update_unit` (fallback → visuals/targeting/movement), `use_fallback`, `stop_shooting`, `update_visuals`, `update_weapons` (target selection and mount shoot flags; bullet firing in 10), `face_target`, `face_movement`, `pathfind(field_target, stop_at_target, avoidance)`, `move_to(..6 overloads)`, `circle`, `circle_attack`, `pref_speed`, `find_target`, `target`, `target_flag`, `target_flag_active`, `get_closest_spawner`, `unload_payloads`, `has_stance`, `stance_changed`.
- Controller selection (`UnitTypeDef.controller_kind`, `ai_controller_kind`) mirrors `UnitType.controller = !playerControllable || (team.isAI() && !team.rules().rtsAi) ? aiController : CommandAI`. RTS-AI teams get `CommandAI`; `Player` controller is created by plan 15 on possession; `LogicAI` is installed by plan 13's `LUnitControl.unitControl` and times out after 10 s without a valid controller.
- `resetController()` re-evaluates selection; called on load unless `keep_state`, on invalid controllers, and when RTS-AI toggles (plan 12's `CampaignRules.apply` equivalent fires a team event that this plan listens to).

### 3.6 Unit kinds / movement details

| Kind | Component | Core behavior ported |
|---|---|---|
| Mech | `MechComp` | `base_rotation` sync-local, `walk_time`/`walk_extension`, step shake/particles/sound threshold, `drown_floor` (hitSize ≥ 12 needs all 8 neighbors deep), `rotate_move` on `base_rotation`, `walked` marking |
| Legs | `LegsComp` | `Leg[]` (base/joint, transient), `reset_legs`, `leg_angle`/`leg_offset` (+ straightness), stage/group stepping with `leg_pair_offset`, `InverseKinematics::solve`, deep-floor drowning rules, leg splash damage + `unitMoveBreakable` deconstruct, walk effects, `lock_leg_base`, `leg_continuous_move`, `destroy` leg explosion/FX data |
| Tank | `TankComp` | tread dust timing/`tread_rects`, `crush_fragile`, `crush_damage`, solids slow (`crawl_slowdown`), `last_deep_floor`, `floor_speed_multiplier` replacement |
| WaterMove | `WaterMoveComp` | two wave trails, water-solid predicate, `floor_speed_multiplier` shallow rules, `on_liquid` |
| WaterCrawl | `WaterCrawlComp` | crawl movement with water-specific solidity/slowdown (Erekir neoplasm crawlers) |
| Underwater | `UnderwaterMoveComp` | underwater solidity/elevation behavior |
| Crawl | `CrawlComp` | `segment_rot`, area tile scan for solids/deeps/damage/`crawl_dust`, slowdown, deep faction |
| Segment | `SegmentComp` | `parent_id` serialized, `add_child`, head update recursion, `update_segment` spacing/rotation clamp (`segment_rotation_range`, `segment_max_rot`), `collision_layer = -1` for non-head, `moving()` semantics, `before_write` parent id, `check_parent` after read |
| ElevationMove (hover) | `ElevationMoveComp` | flying/solid predicate split |
| Payload | `PayloadComp` | `payloads: SmallVec<PayloadRef>`, payload power graph update, `set/update` per payload, `can_pickup(unit/building/payload)`, `pickup`, `drop_last_payload`, `try_drop_payload` (acceptor → block place → unit drop), `drop_unit` (ID reallocation + count fixup + `PayloadDropEvent`), `drop_block` (`Build.valid_place`), drop sounds, `payload_used`, `destroy` explosion rule; **payload entities are plan 08's** |
| BlockUnit | `BlockUnitComp` | proxy unit over a building tile, `tile(tile)` stats copy, `killed → tile.kill`, damage/team delegation, internal-only add guard |
| Tether | `UnitTetherComp`/`BuildingTetherComp` | spawner/building ID serialization (`spawner_unit_id`), despawn when invalid |
| TimedKill (missile) | `TimedKillComp`/`TimedComp` | lifetime countdown, homing target resolution in `MissileAI` |
| TargetDummy | `TargetDummyComp` | target-dummy proxy to building, shoot-practice damage forwarding |

### 3.7 Pathfinding

**`PathTile` (frozen 32-bit layout, matches `@Struct PathTileStruct`):**

```
bits 0..7   health (0..80, block hp/40 capped)
bits 8..15  team (0..255; 255 = derelict under coreCapture)
bit 16 solid      bit 17 liquid     bit 18 legSolid   bit 19 nearLiquid
bit 20 nearGround bit 21 nearSolid  bit 22 nearLegSolid bit 23 deep
bit 24 damages    bit 25 allDeep    bit 26 nearDeep   bit 27 teamPassable
```

`Pathfinder` resource: `tiles: Vec<u32>` (world-packed index `x + y*width`), `cache[team][cost][field]`, `main_list`, `thread_list`, `needs_refresh`, `last_refresh_tick`. Events: `WorldLoadEvent` (rebuild tiles, preload wave-team ground + naval fields when `waveTeam.needsFlowField() && !client`), `ResetEvent` (stop/clear), `TileChangeEvent` (`update_tile` main-thread, `control_path.update_tile`, mark refresh), `TilePreChangeEvent` (nearSolid bit removal), `AfterGameUpdate` (`refresh_interval_ticks` throttle for `refreshRate == 0` fields).

**Deterministic budget model (`worker.rs`):** a single work queue of `FlowfieldOp { Register, RefreshTargets, Step(field) }` consumed in FIFO order; each tick the worker processes up to `FLOWFIELD_NODES_PER_TICK` frontier pops (default 6 000) and up to `CONTROL_NODES_PER_TICK` (default 8 000) for fields/clusters/A*. No wall clock is read; `Time.millis` uses in the upstream throttle become `SimClock.time` tick counters (`refresh_interval_ticks = 6`, control invalidate `= 60 ticks`). Work is executed on one dedicated `std::thread` owned by `Pathfinder`, with an explicit join barrier at the end of `TickSet::EntityUpdate` (before `UpdateUnits` reads fields); the output is identical whether the thread does 1 or N nodes per burst because the algorithm is state-machine incremental. `workers = 1` harness mode runs the same queue inline. A field's `completeWeights` swap happens only on the tick boundary.

**Flowfield API:** `get_field(team, cost, field_type)`, `get_target_tile(tile, flowfield, diagonals, avoidance_id)`, `Flowfield::{setup, get_cost, passable, update_target_positions, has_targets, has_complete_weights}`, `EnemyCoreField` (core targets + random `BlockFlag` targets under `rules.randomWaveAI`, seeded by `Rand::set_seed(wave or tick/5400 + hash)`), `PositionTarget` (`refresh_rate = 900` ms → 900 ticks). `avoidance` array is provided by plan 05's `AvoidanceProcess` (its contract from plan 05: `avoidance.getAvoidance()`), read-only here.

**`ControlPathfinder`:** `cluster_size = 12`; `cluster[team][cost]` with portals shared between neighbors (`Point2` packed inclusive ranges), `update_inner_edges` via bounded `inner_astar`; `FieldCache` keyed by packed `(goal_pos, cost_id, team)` with per-cluster `12×12` weight arrays and frontier; `PathRequest` per unit (`result_path`, costs/cameFrom/frontier scratch, `last_update_id`, `last_recompute_time`, raycast cache `last_raycast_tile/result/world_update`); request lifecycle invalidated when idle ≥ 10 update IDs or unit removed; fields dropped after 30 update IDs; `clusterChanged` invalidates affected requests; `raycast`, `raycastFastAvoid`, `raycastRect`, `nearPassable`, `solid`, `passable` ports. Public API: `get_path_position(unit, dest, main_dest) -> PathfindResult { unreachable, move, next, dest }`; deprecated out-param overloads reimplemented as thin wrappers for `CommandAI`/`LogicAI` callsites.

**`Astar`:** synchronous grid A* with `DistanceHeuristic::{manhattan, euclidean, octile}`, `TileHeuristic`, passable predicate, `out` result; scratch from `Tmp`/`VecPool`; used by `BaseBuilderAI`/`ControlPathfinder`/editor? (editor owns its own copy in 19 if needed).

**`PathfindQueue`:** exact int-node/float-weight binary heap (`add`/`peek`/`poll`/`clear`, growth ratios) in `pathfind_queue.rs`. `ControlStructs` packing: `FieldIndex = pos | cost_id << ... | team << ...` (long), `IntraEdge = dir | portal | cost`, `NodeIndex = cluster(22) | dir(2) | portal(8)` — bit widths frozen, tested.

**`BlockIndexer`:** per-team `Seq<Building>` per `BlockFlag` (`core, storage, generator, turret, factory, repair, battery, reactor, extinguisher, drill, shield, launchPad, unitCargoUnloadPoint, unitAssembler, hasFogRadius, steamVent, blockRepair`), plus damaged list, ore/floor-flag lists, all-buildings list, `turret_tree` quadtree, `getAllPresentOres`, `findClosestOre/WallOre`, `findEnemyTile/findTile` with `BuildingPriority`, `eachBlock`, `isBlockPresent`, `notifyHealthChanged`. Maintained by `TilePreChange`/`TileChange`/`TileFloorChange`/`WorldLoadEvent`; uses plan 06/07's tile/build accessors.

### 3.8 RTS runtime

- `CommandAI` state (serialized subset per `TypeIO.writeController` type 9): `target_pos: Option<Vec2>`, `attack_target: Option<TeamcRef>` (building pos or unit ID), `command: Option<UnitCommandId>`, `command_queue: SmallVec<PositionRef>` (building pos / unit ID / Vec2), stances bitset; transient: `group`, `group_index`, `unreachable_buildings`, `stop_at_target`, `stop_when_in_range`, `last_target_pos`, `blocking_unit`, `time_spent_blocked`, `payload_pickup_cooldown`, `transfer_state`, `command_controller`, `last_command`.
- `command(cmd)` accepts only `unit.type.commands.contains(cmd)`; clears `mine_tile`/build plans. `set_stance` applies `and_not(incompatible_stance_bits)` then sets; `disable_stance` clears; both call `stance_changed` into the active command controller.
- `update_unit` order: mineAuto default, pursue/patrol stances, prune invalid queue entries, default command, swap `command_controller` when `last_command` changes, delegate to command controller or `default_behavior()`; boost handling.
- `default_behavior`: payload load/unload/enter calls (host guard; events/commands to 08/21), `update_visuals`/`update_targeting`, attack target validation, `finishPath` advancement, formation offset via `UnitGroup`, pathing via `ControlPathfinder::get_path_position`, blocked-unit yield check (`ControlPathfinder::is_near_obstacle`), `alwaysArrive`/`isFinalPoint` arrival logic, `circle_attack` for `circleTarget` units, flyer look-at, `exactArrival`, `stopWhenInRange`, `finishPath` (loop-payload transfer state machine, queue advance + patrol/loop re-append, formation raycast refresh).
- `UnitGroup`: `calculate_formation(dest, collision_layer)` from unit positions (center-relative), compression + physics iterations (40 total, `maxPhysicsIterations = min(1 + size^0.65/10, 6)`), `original_positions` clone, raycast per index with `raycastFastAvoid`, `update_raycast`. Deterministic quadtree ordering (`IntQuadTree` port from plan 05's spatial module with insertion-order iteration).
- `UnitCommand`/`UnitStance`/`ItemUnitStance`: lookup by ID from plan 02 registries; runtime helpers `localized`, `get_icon`, `is_compatible`, `get_unit_stances(unit, out)` (mineAuto + present-ore item stances), `allow_stance`, `allow_command`. UI/keybind hooks are plan 15 data (`Binding` ids in the defs).

### 3.9 Team AI

- **`RtsAI`** (per `TeamData`, created lazily when `TeamRule.rtsAi`): 2 s timer; `checkBuilding` core-unit spawn (`aiCoreSpawn`, every 7 s, `coreUnits < cores.size`); `assignSquads` BFS within `squad_radius = 60 + hitSize*1.5` among idle `CommandAI` units with matching `flag`; `handleSquad` computes naval/targetAir/targetGround, health/dps, defends damaged buildings (core rush, `rtsMinSquad`, proximity), else `findTarget` over `[generator, factory, core, battery, drill]` filtered by assigned/invalid targets, shuffled (deterministic `Rand` seeded by `state.update_id`), capped to 15, and weighted by `estimate_stats`; commands position/target with `unit.flag` mobilization; `BuildDamageEvent` listener adds damaged buildings.
- **`BaseBuilderAI`** (per `TeamData` when `TeamRule.buildAi`): fills core items, core-unit spawn (`aiCoreSpawn`, `coreUnitMultiplier = 2`), enemy-path trace over `pathfinder.get_field(team, costGround, fieldCore).complete_weights` (50 steps/frame, `IntSet` path), then places a random `BasePart` near cores/spawns (`randomPosition`, 150 px scatter, 6 attempts, `placeInterval` lerp by `buildAiTier`), validity via `Build.valid_place`, payload-block proximity, AI-path avoidance (`path` IntSet + `get_linked_tiles`), drill resource fit checks (`correct/incorrect`), then queues `BlockPlan`s into `TeamData.plans`. `emptyChance = 0.01`; placement skews to `tile.drop()` resources.
- **`BaseRegistry`** (`Vars.bases`): load at content load; builds `ores`/`oreFloors` maps from `OreBlock`/floor `itemDrop`; reads `basepartnames` generated by plan 03's asset pipeline (upstream `build.gradle` generates it; plan 03 owns the generator and the file ships in `core/assets/`), then `Schematics.read("baseparts/<name>")` and classifies `BasePart { schematic, center_x/y (drill/pump average), required (item/liquid source config), core, tier = Σ (buildTime/buildCostMultiplier)^1.4 }`; cores sorted by tier, parts/resources sorted by tier.
- **Unit caps:** `Units::get_cap` / `can_create` implemented here against plan 12's `Rules`/`Team`; `TeamData::count_type`/`update_count` provided by plan 12 (called on add/remove).

### 3.10 Waves

- **`SpawnGroup`** (struct + JSON keys exactly as upstream `write/read`): `type` (name; legacy unit-name map applied), `begin`, `end`, `spacing`, `max`, `unitScaling`, `shields`, `shieldScaling`, `unitAmount`, `spawn`, `payloads`, `effect`, `items`, `team`; `get_spawned(wave)` (`min(unitAmount + (int)(((wave-begin)/spacing)/unitScaling), max)` with spacing guard), `get_shield(wave)`, `create_unit(team, x, y, rot, wave, cons)` (spawn → effect 999999 s, items, shield, payload pickup), legacy boss effect numeric `8` → `StatusEffects.boss` rule preserved.
- **`WaveSpawner`** resource: spawn tiles = tiles with `Blocks.spawn` overlay (tracked on `TileOverlayChangeEvent`), `count_spawns`, `get_spawns`, `get_first_spawn`, `player_near`; `spawn_enemies()`: shockwaves on ground spawns, per group `get_spawned(state.wave - 1)` × difficulty multiplier (boss rounded down), flying → `each_flyer_spawn` (map-edge projection unless `airUseSpawns`; core positions in attack mode), ground → `each_ground_spawn` (spawn tiles; attack-mode core-proximity search with `maxSteps=30`), spread jitter, `spawn_unit` (`group.create_unit(waveTeam or group.team, ..., facing map center, wave-1, spawn_effect)`), `spawning` window 121 ticks; `spawn_effect` (30 ticks `unmoving`, 60 ticks `invincible`, `unloaded()`, `UnitSpawnEvent`, Fx + 30-tick delayed `Fx.spawn` via `Time.run`); `do_shockwave` (Fx + lethal damage to wave team).
- **`Waves::get()`**: the full built-in 445-line table ported verbatim (Serpulo survival prescription, start/end/scaling/shields/spacing per group, boss groups, `waveVersion = 7`).
- **`Waves::generate(difficulty, rand, attack, airOnly, naval)`**: exact algorithm port (species tiers, shields per wave by difficulty, progression stepping `5 + rand(5)` then `rand(15,30)*lerp(1,0.5,difficulty)`, boss wave `rand(50,70)*lerp`, alt/final boss, attack megas, `shift = max(int(difficulty*14 - 5), 0)`), driven by `math::rand_arc::Rand` (bit-exact Arc `Rand` port; golden vectors captured per §7a). Campaign call sites and `rules.spawns` assignment are plan 12's.
- **Difficulty hook:** `trait WaveDifficulty` with `enemy_spawn_multiplier(planet) -> f32` and `unit_health_multiplier(planet)`; plan 12 supplies `CampaignRules.difficulty` values; default identity shim until 12 lands.

### 3.11 Unit factories / blocks

- `UnitBlock` (`PayloadBlock` subclass in 08): `UnitBuild` progress/time/speed_scl, `spawned()`, `dump_payload()` completion call.
- `UnitFactory`: `plans: Vec<UnitPlan { unit, requirements, time }>`, `capacities: Vec<i32>` (indexed by item id, two× max requirement, min 10), dynamic item consumer (`ConsumeItemDynamic` from 07/08), `UnitFactoryBuild { command_pos, command, current_plan, progress }` with config handlers (plan index / UnitType / UnitCommand / clear), `update_tile` progress + unit creation + command assignment + payload output + `UnitCreateEvent`, `get_maximum_accepted`, `accept_item`, `should_consume` (`team.activate_unit_factories()`), `status`, sensing (`LAccess.config/progress/itemCapacity`), revision write/read v3 fields.
- `Reconstructor`: `upgrades: Vec<(UnitType, UnitType)>`, `construct_time`, capacities; `ReconstructorBuild` accepts payload only for valid upgrades, `update_tile` consumes items + progress + swap unit type, command carry, `overwrote` behavior, senses.
- `UnitAssembler` + `UnitAssemblerModule`: `area_size = 11`, `drones_created`, `drone_construct_time`, `AssemblerUnitPlan`, `get_rect`, module tier/same-type logic, drone spawn/update, `yeet_payload`, payload accept rules, `assemblerUnitSpawned`/`assemblerDroneSpawned` host functions, config UI data (14).
- `RepairTower`: `range`, `heal_amount`, 6-tick refresh target list, suppression, warmup, `should_consume`, draw data hooks.
- `RepairTurret`: block fields (`repair_radius`, `repair_speed`, `powerUse`, beam drawing data) + `RepairPointBuild` target acquisition at 60-tick intervals, continuous heal + `laser` state, `Ranged`/`RotBlock` behavior. (This block is `Block`-derived upstream, not a turret-base consumer; 10's `BaseTurret` is not reused.)
- `UnitCargoLoader`: tether unit spawn (`unit_type = manifold`, `unit_build_time`), `UnitTransportSourceBuild` progress/tether/`unitTetherBlockSpawned` ID handshake, item accept, `outputs_items = false`.
- `UnitCargoUnloadPoint`: `stale_time_duration = 360`, item config, `accept_stack`, stale flag + revision IO.
- All builds implement plan 07's `Building` hooks; `BlockDef.kind` tags come from plan 02; texture/part expectations feed 16/17.

### 3.12 Schedule slots & boundaries

Plan 05 owns the schedule; this plan registers systems at these anchors (exact names locked):

| Slot | Systems added here |
|---|---|
| `TickSet::WeatherAndAi` (`!client && !editor`) | `pathfinder::after_update` throttle, `control_pathfinder::invalidate_tick`, `rts_ai::update`, `base_builder_ai::update` (plan 12 calls per active `TeamData`), `unit_group::join_if_dirty` |
| `TickSet::RunWave` | `wave_spawner::spawn_enemies` (called by plan 05's wave timer hook), wave counter increment owned by 05/12 |
| `EntitySet::PoolCleanup` | unit pool reset hooks (`reset_type_defaults`, leg clear) |
| `EntitySet::PhysicsUnits` | unit physics via plan 05 `EntityCollisions` (unit predicates supplied here) |
| `EntitySet::UpdateUnits` | per-unit `update` chain (component systems), `UnitControllerSlot` update via `UnitComp.update`, `TimedComp`/`TimedKill` |
| `EntitySet::UpdateBuildings` | unit blocks' `update_tile` (through plan 07's building update chain) |
| end of `EntitySet` (after collisions) | pathfinder worker **join barrier** (fields consistent before next tick) |
| `TickSet::AfterGameUpdate` | `pathfinder::publish` (main-list refresh), `Time::run` spawn-effect hooks |
| `Events` | `UnitCreateEvent`, `UnitSpawnEvent`, `UnitDestroyEvent`, `UnitDrownEvent`, `PayloadDropEvent`, `PickupEvent`, `BuildSelectEvent` (builder), `UnitDamageEvent` (10 fires, controller counterattack consumes) |

### 3.13 Godot / STDB surfaces touched

- **Godot (plan 00 rig):** autoload `Node` `/root/Spine/MindUnits` (adapt to plan 00's actual autoload root) with methods: `spawn_unit(unit_name: String, team: int, x: float, y: float) -> int`, `unit_count(unit_name := "") -> int`, `unit_state(id: int) -> Dictionary {x,y,rotation,health,type,controller}`, `command_move(ids: PackedInt32Array, x: float, y: float, queue := false)`, `command_stance(ids, stance_name, enabled)`, `set_wave(wave: int)`, `wave_info() -> Dictionary {wave, wavetime, spawns, unitCap}`, `despawn_all()`. Inspector tab `/root/Spine/StateInspector/Units`. No sim logic in GDScript; these call plan-05 `Sim` systems through the GDExtension.
- **STDB:** no tables/reducers added here. Contract consumed: `SimCommand::{UnitCommand, UnitStance, SpawnUnit}` (§6.5 reconciliation) produced by plan 21's `command_event` relay in relay order; `UnitSpawnEvent`/destroy events emitted for the relay to package. Cheap validation stays in plan 21 (ID existence, team ownership, command allowed by type, distance sanity).

---

## 4. Port map

### 4.1 `entities/comp/*` (unit-reachable) and unit helpers

| Mindustry source | Target Rust | Notes |
|---|---|---|
| `comp/UnitComp.java` | `entities/comp/unit/mod.rs` (`UnitCore` component + behavior fns) | All 1007 lines' behavior: bounds/warp, drown, floor effects, abilities, trail, knockback, floor damage, boosting, `kill/destroy/killed`, senses/settables, `setType`, `afterSync/afterRead/afterReadAll`, cap add/remove, `collisionLayer`, `shouldUpdateController` |
| `comp/MechComp.java` | `entities/comp/unit/mech.rs` | `baseRotation` sync-local, walk animation, step FX hooks, `drownFloor`, `rotateMove`/`moveAt`/`approach` replacement |
| `comp/LegsComp.java` | `entities/comp/unit/legs.rs` + `inverse_kinematics.rs` | Leg struct (`Leg.java`), staging, IK, deep-floor rules, splash damage, `destroy` FX data (`LegDestroyData`) |
| `comp/TankComp.java` | `entities/comp/unit/tank.rs` | treads, crush, slow, deep floor |
| `comp/WaterMoveComp.java` | `entities/comp/unit/water_move.rs` | twin trails, water-solid, speed |
| `comp/WaterCrawlComp.java` | `entities/comp/unit/water_crawl.rs` | neoplasm crawler movement |
| `comp/UnderwaterMoveComp.java` | `entities/comp/unit/underwater_move.rs` | underwater solidity |
| `comp/CrawlComp.java` | `entities/comp/unit/crawl.rs` | segment rotation, area scan, dust |
| `comp/SegmentComp.java` | `entities/comp/unit/segment.rs` | parent/child chain, serialized `parentId` |
| `comp/ElevationMoveComp.java` | `entities/comp/unit/elevation_move.rs` | solidity split |
| `comp/PayloadComp.java` | `entities/comp/unit/payload.rs` | pickup/drop state machine; payload entities in **08** |
| `comp/BuilderComp.java` | `entities/comp/unit/miner_builder.rs` (builder half) | plans queue, `validatePlans`, `updateBuildLogic`, config/plan draw hooks deferred to 16 |
| `comp/MinerComp.java` | `entities/comp/unit/miner_builder.rs` (miner half) | mining tick, transfer-to-core, `mineTile` sync-local |
| `comp/BlockUnitComp.java` | `entities/comp/unit/block_unit.rs` | proxy semantics |
| `comp/UnitTetherComp.java`, `comp/BuildingTetherComp.java` | `entities/comp/unit/tether.rs` | ID serialization/dangling despawn |
| `comp/ChildComp.java`, `comp/OwnerComp.java` | `entities/comp/unit/child_owner.rs` | ownership/child links |
| `comp/DamageComp.java` | `entities/comp/unit/damage.rs` | `dead`/`damaged()` |
| `comp/ItemsComp.java` | `entities/comp/unit/items_shield_status.rs` | `stack`, `item`, `addItem`, `clearItem`, `acceptsItem` |
| `comp/ShieldComp.java`, `comp/ShielderComp.java` | `entities/comp/unit/items_shield_status.rs` | shield pool/absorb rules; damage application from 10 |
| `comp/StatusComp.java` | `entities/comp/unit/items_shield_status.rs` (`StatusEntry` in `status_entry.rs`) | apply/extend/cancel, multipliers, `disarmed`, immunities |
| `comp/WeaponsComp.java` | `entities/comp/unit/weapons_draw_sync.rs` + `weapon_mount.rs` | mount array, `setupWeapons`, `aimX/aimY`, `controlWeapons`, `isShooting`; bullet firing math in 10 |
| `comp/DrawComp.java`, `comp/SyncComp.java` | `entities/comp/unit/weapons_draw_sync.rs` | draw-data exposure; `isSyncHidden`/`handleSyncHidden`, `snapInterpolation` |
| `comp/HealthComp.java`, `comp/PhysicsComp.java`, `comp/HitboxComp.java` | `entities/comp/unit/health_physics_hitbox.rs` | health/damage/heal, drag/vel, hitbox/hit rects, `hittable` |
| `comp/PosComp.java`, `comp/RotComp.java`, `comp/VelComp.java`, `comp/TeamComp.java`, `comp/TimerComp.java`, `comp/TimedComp.java`, `comp/TimedKillComp.java`, `comp/TargetDummyComp.java` | `entities/comp/unit/{pos_rot_vel,timed,timed_kill,target_dummy}.rs` | thin/aliasing + timer bitfield |
| `comp/EntityComp.java` | plan 05 `entities/comp/base.rs` | consumed, not re-ported |
| `comp/PlayerComp.java` | **21/15** (player entity); this plan only matches `Player` controller in unit code | — |
| `entities/Units.java` | `entities/comp/unit/queries.rs` | closest/best/nearby/count/canCreate/getCap/invalidateTarget; `UnitSyncContainer` net shape → 21 |
| `entities/units/{UnitController,AIController,WeaponMount,BuildPlan,StatusEntry}.java` | `ai/controller.rs`, `entities/comp/unit/weapon_mount.rs`, `entities/comp/unit/miner_builder.rs` (`BuildPlan`), `status_entry.rs` | `WeaponMount` target/rotate/shoot fields reconciled with 10 |
| `entities/{Leg,LegDestroyData,Mover,Sized}.java` | `entities/comp/unit/legs.rs`, `math`/`entities` shared | small value types |

### 4.2 `ai/*`

| Mindustry source | Target Rust | Notes |
|---|---|---|
| `ai/Pathfinder.java` | `ai/pathfinder/*` | `PathTile` layout, costs, flowfields, deterministic budget worker, tile events |
| `ai/ControlPathfinder.java` | `ai/control_pathfinder.rs` + `ai/control_structs.rs` | clusters/portals/inner edges/fields/requests/raycasts |
| `ai/PathfindQueue.java` | `ai/pathfind_queue.rs` | exact binary heap |
| `ai/Astar.java` | `ai/astar.rs` | synchronous A*, per-call scratch |
| `ai/UnitGroup.java` | `ai/unit_group.rs` | formation packing + raycast avoidance |
| `ai/BlockIndexer.java` | `ai/block_indexer.rs` | flags/ores/damaged/turret quadtree |
| `ai/UnitCommand.java` | `ai/unit_command_runtime.rs` (+ metadata in 02) | registry lookup, `control` mapping, `getUnitStances` |
| `ai/UnitStance.java`, `ai/ItemUnitStance.java` | `ai/unit_stance_runtime.rs`, `ai/item_unit_stance.rs` | incompatible bits (link pass in 02), item stance map |
| `ai/RtsAI.java` | `ai/rts_ai.rs` | squad assignment/target weighting |
| `ai/BaseBuilderAI.java` | `ai/base_builder_ai.rs` | path trace + schematic placement |
| `ai/BaseRegistry.java` | `ai/base_registry.rs` | baseparts classification, tier sort |
| `ai/WaveSpawner.java` | `ai/wave_spawner.rs` | spawn windows/effects/attack mode |
| `ai/types/GroundAI.java` | `ai/types/ground.rs` | stuck detection + core pathing |
| `ai/types/HugAI.java` | `ai/types/hug.rs` | raycast approach |
| `ai/types/FlyingAI.java` | `ai/types/flying.rs` | flag targeting/random wave AI |
| `ai/types/FlyingFollowAI.java` | `ai/types/flying_follow.rs` | follow / fallback to `FlyingAI` |
| `ai/types/CommandAI.java` | `ai/types/command.rs` | full command/stance/queue/formation state machine |
| `ai/types/BuilderAI.java` | `ai/types/builder.rs` | repair/rebuild/assist plans |
| `ai/types/RepairAI.java` | `ai/types/repair.rs` | damaged-tile healing + retreat |
| `ai/types/MinerAI.java` | `ai/types/miner.rs` | ore selection/mining/delivery |
| `ai/types/CargoAI.java` | `ai/types/cargo.rs` | tether loader shuttle |
| `ai/types/DefenderAI.java` | `ai/types/defender.rs` | player-controllable targeting |
| `ai/types/SuicideAI.java` | `ai/types/suicide.rs` | ram pathing/ignore walls |
| `ai/types/MissileAI.java` | `ai/types/missile.rs` | homing forward, impact kill |
| `ai/types/LogicAI.java` | `ai/types/logic.rs` | `LUnitControl` bridge (13), keep-state |
| `ai/types/NoAI.java` | `ai/types/no_ai.rs` | inert controller |
| `ai/types/BoostAI.java` | `ai/types/boost.rs` | RTS boost stance controller |
| `ai/types/AssemblerAI.java` | `ai/types/assembler.rs` | fixed spot/angle |
| `ai/types/PrebuildAI.java` | `ai/types/prebuild.rs` | experimental wave pre-build (full parity, upstream-labeled experimental) |
| — (Player possession) | `ai/types/player_bridge.rs` | bridges plan 15 input into the controller slot |

### 4.3 `game/` waves and unit blocks

| Mindustry source | Target Rust | Notes |
|---|---|---|
| `game/Waves.java` | `game/waves.rs` | table + `generate`; `waveVersion=7`; `Rand` port needed |
| `game/SpawnGroup.java` | `game/spawn_group.rs` | struct + JSON + clone/eq; `LegacyIO.unitMap` applied on read |
| `world/blocks/units/UnitBlock.java` | `world/blocks/units/unit_block.rs` | base build + spawn handshake |
| `world/blocks/units/UnitFactory.java` | `world/blocks/units/unit_factory.rs` | plans/capacities/config/revision v3 |
| `world/blocks/units/Reconstructor.java` | `world/blocks/units/reconstructor.rs` | upgrades/payload/commands/revision |
| `world/blocks/units/UnitAssembler.java` | `world/blocks/units/unit_assembler.rs` | modules/drones/tiers/yeet |
| `world/blocks/units/UnitAssemblerModule.java` | `world/blocks/units/unit_assembler_module.rs` | module update/registration |
| `world/blocks/units/RepairTower.java` | `world/blocks/units/repair_tower.rs` | aura heal |
| `world/blocks/units/RepairTurret.java` | `world/blocks/units/repair_turret.rs` | beam repair |
| `world/blocks/units/UnitCargoLoader.java` | `world/blocks/units/unit_cargo_loader.rs` | tether spawn/progress |
| `world/blocks/units/UnitCargoUnloadPoint.java` | `world/blocks/units/unit_cargo_unload_point.rs` | item config + stale |
| `type/UnitType.java` (runtime half) | `entities/comp/unit/lifecycle.rs` + `ai/controller_registry.rs` | create/spawn/init runtime bindings; metadata stays in 02 |
| `io/TypeIO.writeController/readController` | `io/entity/controller_codec.rs` (**plan 04** owns file, this plan specifies) | type bytes 0/3/4/5/6/7/8/9 and field layout |
| `content/UnitTypes.java` `@EntityDef` groups | `entities/comp/unit/defs.rs` | 12 specs + legacy flags |
| `core/assets/basepartnames` + `baseparts/*.msch` | plan 03 asset pipeline (generation), `ai/base_registry.rs` (consumption) | `.msch` decode is 12's `Schematic` |
| `annotations/.../revisions/<unit def>/*.json` | plan 04 revision manifests under `crates/mind-core/revisions/<def>/` | element names `alpha`, `mace`, … must match; latest numbers per §3.2 table |

---

## 5. Milestones & task breakdown

Every milestone ends with `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, and the named harness command; evidence goes in the Changelog. Order is strict. Each milestone assumes plan 05 M4–M8 green.

**M0 — Smallest vertical slice: one unit spawns, paths, arrives.**
- `entities/comp/unit/defs.rs` (`ComponentKind`, 12 specs), `UnitBundle` for `alpha` (UnitEntity) + `mace` (MechUnit), `lifecycle.rs` (`spawn`, `add`, `remove`, `kill`/`destroy` minimal), `queries.rs` target helpers, `math/rand_arc.rs`.
- `ai/pathfinder/{path_tile,cost,flowfield,worker}.rs` with `costGround` only and `EnemyCoreField`; `ai/controller.rs` + `ai/types/ground.rs` minimal (`pathfind(Pathfinder::field_core)` + `face_target`).
- Verify: `mind-headless run units_spawn_path_arrive --unit dagger --ticks 1200` — unit spawned at (x0,y0) with a core target reaches within `hitSize` of the target tile; deterministic checksum committed. `cargo test units::spawn_path_arrive`.

**M1 — Full component set + revisions + lifecycle parity.**
- All unit comps of §3.6, base-closure ordering tables, `FieldMeta` for plan 04 with latest upstream revision numbers (§3.2), `UnitBundle` for all 12 defs, segmented spawn, `afterRead`/`afterReadAll`, caps, `Units` queries.
- Verify: `mind-headless meta entities --out` includes 12 unit defs with expected component lists; `cargo test units::{defs_match_upstream_revisions, seg_chain_spawns, pool_reset_defaults, cap_death}`.
- Hand-off: plan 04 revision manifests (`revisions/alpha/…`) committed; plan 02 `EntityDefSpec` re-export added.

**M2 — All 17 controllers + targeting + weapons mounting.**
- `ai/types/*` complete (movement/targeting/queue behavior), `controller_registry.rs` selection, `AIController` helper set, `weapon_mount.rs` (`setupWeapons`, aim/rotate), `Units` target sorting; `WeaponsComp.update` integration with plan 10's mount firing.
- Verify: `cargo test ai::types::{selection_matrix, command_stance_transitions, flying_flag_priority, miner_targets_ore, cargo_shuttle}`; `units_controller_matrix` harness scenario dumps controller kind per unit type after `UnitType.init`.

**M3 — Pathfinding complete (both systems) + BlockIndexer.**
- `cost.rs` all 6 ground costs + 4 control costs, `PositionTarget`, refresh budgets, `control_pathfinder.rs` (portals, inner edges, fields, requests, raycasts), `control_structs.rs`, `astar.rs`, `pathfind_queue.rs`, `block_indexer.rs` (all flags/ores/turret tree).
- Verify: `mind-headless run units_flowfield_costs --map flat_256 --ticks 600` asserts per-cost passability/cost tables vs golden; `cargo test pathfinder::{path_tile_bits, cost_types, field_cache_eviction, request_invalidate, cluster_portals, node_index_packing, indexer_flags}`; `mind-headless bench-path`.

**M4 — RTS: CommandAI, stances, formations, RtsAI.**
- `ai/types/command.rs`, `unit_group.rs`, `unit_command_runtime.rs`, `unit_stance_runtime.rs`, `item_unit_stance.rs`, `rts_ai.rs`.
- Verify: `units_rts_command_queue` scenario (queue 3 waypoints with stance changes; assert reached order + stance bits + formation offsets); `cargo test rts::{queue_advance, patrol_loop, stance_incompatibility, formation_deterministic, rts_assigns_squads}`. Plan 15 unblocks its UI against the public API.

**M5 — Team AI: BaseBuilderAI + BaseRegistry.**
- `base_registry.rs`, `base_builder_ai.rs`; plan 03 ships `basepartnames` + `baseparts/` decode via 12's `Schematics`.
- Verify: `cargo test base::{registry_tiers, path_avoids_blocks, places_on_ore}`; harness `units_base_build --map serpulo/groundZero --ticks 3600` asserts plan count > 0 and no invalid placement.

**M6 — Waves + SpawnGroup + WaveSpawner.**
- `game/spawn_group.rs`, `game/waves.rs`, `ai/wave_spawner.rs`, difficulty hook; `DumpWaves.java` golden + `Rand` vectors.
- Verify: `units_waves_difficulty` scenario (run waves 1..50 with seeded difficulty 0/0.5/1.0 and dump per-wave counts, air/naval split, boss waves; assert golden equality); `cargo test waves::{generate_matches_golden, spawn_group_json, get_spawned_bounds, spawn_effect_statuses}`.

**M7 — Unit factories / repair / cargo blocks.**
- `world/blocks/units/*` all 9; plan 07 building registration + plan 08 payload integration.
- Verify: `units_factory_output` (dagger factory with items → payload unit appears with command), `units_cargo_pickup_deliver` (loader/unload point loop with `CargoAI`), `cargo test blocks::{factory_progress, reconstructor_upgrade, assembler_tiers, unload_point_stale}`.

**M8 — MCP integration + perf + hardening.**
- `MindUnits` autoload and inspector tab; `bench-air`/`bench-path`; alloc-audit on unit-heavy ticks; determinism with `workers=1/4`; fuzz spawn/despawn/kill.
- Verify: §7c MCP scenario recorded; §7d budgets measured; `cargo test units::fuzz_{add_remove, kill_during_update}`.

Dependency-safe notes: M0–M1 can start once plan 05 M4–M5 lands (components/registry) with plan 06 tiles stubbed through its trait; M3 requires plan 06's tile events; M5/M7 require plan 12 `Schematic`/plan 08 payloads and may run against trait stubs earlier. Plans 06/07/08/10 were not present on disk when this plan was authored (see §8 R1) — start M0 against the interfaces named in §3 and re-verify at their kickoff.

---

## 6. Data & formats

### 6.1 `ComponentKind` and `EntityDefSpec`

- `ComponentKind` serialized as its uppercase variant name (`"Pos"`, `"Mech"`, …) in JSON/audit dumps; the value order above is frozen for ABI dumps but only names are load-bearing (registry JSON from plan 20 uses names).
- `EntityDefSpec` appears in plan 02's `UnitTypeDef.entity_def` and in the entity-meta dump (`mind-headless meta entities`): `{ "def_name": "mace", "class_name": "MechUnit", "components": ["Pos", ..., "Mech", "ElevationMove"], "legacy": false }`.
- Any change to a def's component list is a save-format change and requires a new revision manifest (plan 04 rule); the `components` set for the 12 defs is append-only by the same rule (mods may add new defs, never reorder base kinds).

### 6.2 Unit field metadata (revision parity)

- Each unit component field carries its upstream marker: `#[sim(plain)]`, `#[sim(sync_float(clamped, interp = linear|angle))]`, `#[sim(sync_local)]`, `#[sim(no_sync)]`, `#[sim(no_serialize)]`, `#[sim(transient)]`, `#[sim(read_only)]`. The source of truth is the upstream component field list and the committed revision JSONs; `alpha/5` is the reference for the UnitEntity closure and lists serialized fields alphabetically (`abilities, aimX, aimY, controller, elevation, flag, health, isShooting, mineTile, mounts, plans, rotation, shield, spawnedByCore, stack, statuses, team, type, updateBuilding, vel, x, y`).
- Latest upstream revision numbers to transcribe into plan 04 manifests: `alpha/5, mace/9, corvus/9, stell/2, risso/9, latum/2, elude/2, mega/7, manifold/3, missile/3, block/9, dummy/0` (legacy dirs `mono/8`, `poly/7`, `nova/7`, `pulsar/5`, `quad/8`, `oct/7` are read-only mappings to the same defs).
- `FieldMeta` additions for units must also produce plan 21's `_TARGET_`/`_LAST_` companion metadata for `sync_float` fields (`x`, `y`, `rotation`, `health`, `shield`, `elevation` via SyncLocal rules, `mineTile` position, `baseRotation`, `segmentRot` etc. per field).

### 6.3 Controller codec (hand-off to plan 04/21)

Type bytes (exact upstream): `0` Player (+ `i32 player.id`), `2` generic/reset, `3` LogicAI (+ `i32 building.pos()`), `4/6/7/8/9` CommandAI revisions; writer emits `9`:

```
u8 9
bool hasAttack, bool hasPos
[f32 x, f32 y]?                     // targetPos
[b8 entityType (1=building), i32 packedPos | unitId]?
b8 command id (-1 = none)
b8 queue length, then per entry: b8 kind (0 building+i32 pos, 1 unit+i32 id, 2 f32 x/f32 y, 3 garbage)
b8 stance count, then per stance: content UnitStance id (i16 per plan 02's `writeStance`)
```

Readers must accept 4/6/7/8 layouts (ignored trailing fields exactly as upstream). `AssemblerAI` writes `5` (no payload); `NoAI`/`BoostAI`/`PrebuildAI` write `2` and reset on read. `Player` read failure returns `prev`.

### 6.4 Pathfinding budgets and bit layouts (frozen)

| Constant | Value | Upstream source |
|---|---|---|
| `FLOWFIELD_NODES_PER_TICK` | 6 000 | replaces `maxUpdate = 8 ms` |
| `REFRESH_INTERVAL_TICKS` | 6 (=100 ms) | `refreshIntervalMs = 100` |
| `CONTROL_NODES_PER_TICK` | 8 000 | replaces `maxUpdate = 12 ms` |
| `CONTROL_UPDATE_INTERVAL_TICKS` | 2 (=30 fps) | `updateFPS = 30` |
| `CONTROL_INVALIDATE_INTERVAL_TICKS` | 60 | `invalidateCheckInterval = 1000 ms` |
| `REQUEST_IDLE_TIMEOUT_TICKS` | 10 update IDs | `lastUpdateId <= state.updateId - 10` |
| `FIELD_IDLE_TIMEOUT_TICKS` | 30 update IDs | `<= state.updateId - 30` |
| `CLUSTER_SIZE` | 12 | `ControlPathfinder.clusterSize` |
| `PATH_TILE_BITS` / `FIELD_INDEX_BITS` / `INTRA_EDGE_BITS` / `NODE_INDEX_BITS` | per §3.7 / §3.7 | `@Struct` widths |
| `MAX_COMMAND_QUEUE` | 50 | `CommandAI.maxCommandQueueSize` |
| `AVOID_INTERVAL` | 10 ticks | `avoidInterval` |

Field-type indices: `fieldCore = 0`, `maxFields = 10`; cost indices: ground `0, legs 1, naval 2, neoplasm 3, none 4, hover 5, maxCosts 8` (plan 02 stores `flowfieldPathType`); control cost ids `ground 0, hover 1, legs 2, naval 3`. New costs/fields are appended only (registry type indices are ABI).

### 6.5 `SimCommand` unit variants (reconciliation with plan 05 §6.4)

Plan 05 already ships `SimCommand::UnitCommand { units, command, x, y }` and `SpawnUnit`. This plan **requires two additions** executed in plan 05's `determinism/command.rs` at M4:

```rust
UnitCommandQueue { units: SmallVec<[i32; 32]>, command: u16, x: f32, y: f32 }, // append to queue
UnitStance { units: SmallVec<[i32; 32]>, stance: u16, enabled: bool },
```

Both are deterministic, replay-safe, and validated cheaply by plan 21 (unit exists, command ∈ type.commands, stance allowed). If plan 05's enum is closed, the alternative is an `Custom { kind, data }` encoding — rejected: explicit variants keep the relay/checksum contract readable.

### 6.6 Assets / files

- `basepartnames`: UTF-8 LF list generated by plan 03 from `baseparts/*.msch` (alphabetical); `BaseRegistry` refuses an empty list (headless dev asset fallback ships the vanilla names list committed).
- `baseparts/*.msch`: decoded by plan 12's `Schematic::read`; blueprint cache (`Vec<BasePart>`) rebuilt on content load and data patch.
- Golden files: `tests/golden/units/{wave_generate.json, controller_matrix.json, path_costs.json, entitymeta_units.json}`; `bench/baselines.json` entries contributed: `units_mid`, `units_stress`, `pathfinder_flat256`, `rts_command_200`.

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (Mindustry `tests/src/test/java/**` → `cargo test -p mind-core`)

Upstream has no dedicated AI/pathfinding test class; the table marks direct ports (P) and new tests derived from upstream assertions/goldens (N).

| Upstream test / behavior | Rust test | Notes |
|---|---|---|
| `ApplicationTests.spawnWaves` (P) | `waves::tests::spawn_waves_headless` | load fixture map, `spawner.countSpawns() > 0`, `run_wave()`, force timers, `Groups.unit` non-empty |
| `ApplicationTests.testSectorValidity` spawn/indexer portion (P) | `waves::tests::sector_spawns_nonempty_to_boss` + `ai::tests::sector_indexer_no_sandbox_sources` | per-sector: every wave ≤ boss wave spawns ≥ 1 unit; no `powerSource/powerVoid/itemSource/liquidSource` indexed |
| `ApplicationTests.checkPayloads` / `allPayloadBlockTest` (P, partial) | `units::tests::payload_blocks_update_and_save` (with 08/04) | payload carrier update + save/load round-trip |
| `ApplicationTests.initialization` unit anchors (P) | `units::tests::unit_content_registered` | 65 unit types + 12 defs present |
| `Pathfinder` cost semantics (N) | `pathfinder::tests::cost_types_match_golden` | hand-built 32×32 tile grids per cost type; expected passability/costs dumped from golden |
| `PathTile` packing (N) | `pathfinder::tests::path_tile_bit_layout` | set/pack/get round-trip, all bits + hostile-team variants |
| `Flowfield` decrement (N) | `pathfinder::tests::flowfield_frontier_converges` | wall maze; `get_target_tile` strictly decreasing complete weights until target |
| `ControlPathfinder` (N) | `control::tests::{portals_shared_between_clusters, inner_edges_symmetric, request_uses_cache, unreachable_reported}` | 24×24 cluster fixture |
| `Astar` (N) | `astar::tests::{manhattan_optimal, no_path, heuristic_consistency}` | wall/obstacle fixtures |
| `CommandAI` queue (N) | `ai::tests::{command_queue_advances, patrol_loops, hold_fire_stance, mine_auto_default}` | simulated `update_unit` over fixed ticks |
| Formation (N) | `unit_group::tests::formation_collision_free_and_raycast_clamped` | deterministic output twice |
| `SpawnGroup` math (N) | `waves::tests::{get_spawned_bounds, shield_scaling, create_unit_payloads}` | direct port of formulas/JSON |
| `Waves.generate` (N) | `waves::tests::generate_matches_java_golden` | `parity/java/DumpWaves.java` emits the golden table for seeds `{1,2,3}` with `difficulty {0, 0.35, 0.7, 1.0}`, flags `(attack, airOnly, naval)`; Rust must reproduce exactly (`Rand` port) |
| `TypeIOController` round-trip (N) | `io::entity::tests::controller_codec_types_{4,6,7,8,9}` | owned by plan 04, listed here; fixtures captured from upstream `writeController` |
| `UnitType.init` commands/stances (N) | `units::tests::{derived_commands_by_capability, derived_stances_by_capability, mirrored_weapons_double_reload}` | metadata half in 02, runtime binding asserted here |
| `Reconstructor`/`UnitFactory` (N) | `blocks::tests::{factory_plan_requirements_scaled, reconstructor_upgrade_matrix}` | uses `Blocks.java` plans as golden |
| — (N) | `units::tests::{fuzz_add_remove_kill, controller_reset_on_read, segment_parent_repair}` | lifecycle stress |

`cargo test -p mind-core` must run with no Godot, no network, no JVM (goldens committed).

### 7b. Headless harness scenarios (`mind-headless`)

| Command | Behavior | Assertions |
|---|---|---|
| `run units_spawn_path_arrive --map fixtures/flat_128 --unit dagger --ticks 1200` | spawn one dagger at a corner, enemy core at opposite corner | unit arrives ≤ `hitSize` from target tile; tick count ≤ budget; final position checksum equals golden |
| `run units_flowfield_costs --map fixtures/cost_lab_64 --ticks 600` | lab map with deep water, walls, team blocks, damaging floor; one wave-team unit per kind (ground/legs/naval/hover/neoplasm) | each unit crosses only passable tiles; path length/route dumps equal golden |
| `run units_rts_command_queue --ticks 1800` | 10 daggers, queue 3 move points with `patrol`+`pursueTarget` stance changes | all units reach queue tail; queue empties; stance bits and formation offsets match dump; command type in a stubbed reactor crosses relay (21 stub) |
| `run units_waves_difficulty --map fixtures/flat_128 --difficulty 0|0.5|1.0 --waves 1..50 --seed 42` | run `Waves.generate` + `WaveSpawner` (no AI ticks beyond spawn) | per-wave ground/air counts, boss waves, shields match golden `wave_generate.json` |
| `run units_cargo_pickup_deliver --map fixtures/cargo_128 --ticks 3600` | `UnitCargoLoader` + 2 `UnitCargoUnloadPoint`s + `manifold` with `CargoAI` | items move loader→unit→point; units return; unload point stale flips at 360 ticks; no item loss |
| `run units_factory_output --map fixtures/factory_128 --ticks 2400` | dagger factory fed items, command set to move | unit appears with requested command after `plan.time`; count/team correct; save/load mid-build round-trips |
| `run units_legs_ik --ticks 600` | legs unit crosses stepped terrain | leg joints clamp within `legMinLength..legMaxLength`; no NaN; deterministic leg positions dump |
| `run units_segment_chain --ticks 600` | `latum`-style segmented spawn | `segmentUnits` chain length, spacing, rotation clamps; head-only controller updates; kill head → chain despawns cleanly |
| `bench-path --map fixtures/flat_256 --ticks 3600 --json` | pathfinder budget profiles | §7d numbers |
| `bench-air --units 300|1000 --json` | unit AI+physics attribution | §7d numbers |

### 7c. MCP playtest scenario (concrete, open-godot-mcp)

Preconditions: plan-00 spine with autoload `/root/Spine/MindUnits` (adapt to plan 00's actual node root); plan 05 `SimBridge` ticking; plan 06 world loaded; plan 21 relay stub connected or local mode.

1. `godot_health check` → `{ok:true}`.
2. `godot_game play` (spine scene); wait for `godot_log` to show `world loaded`.
3. `godot_exec eval` `MindUnits.spawn_unit("dagger", 0, 200.0, 200.0)` (few times, plus one `mace` and one `risso`) → returns entity IDs; `print("MCP_UNITS=", MindUnits.unit_count("dagger"))`.
4. `godot_exec eval` `MindUnits.unit_state(<id>)` for each → positions around (200,200).
5. `godot_exec eval` `MindUnits.command_move([<ids...>], 800.0, 800.0)` (relay path: when plan 21 is live, issue via `MindCli.send_unit_command(...)` in the same eval and assert the command row exists via `spacetime sql`); `print("MCP_CMD=ok")`.
6. `godot_wait 3` (or repeat `godot_exec` with `godot_game_time step`/`freeze+step` under plan 05's clock control); `godot_exec eval` positions again → each unit moved ≥ 100 units toward (800,800), positions strictly increasing in the move direction.
7. `godot_runtime_state watch /root/Spine/MindUnits property "unit_count" 500` → stable count (no unintended despawn).
8. `godot_exec eval` `print(MindUnits.unit_state(<id0>).x, ",", MindUnits.unit_state(<id0>).y)` after another 3 s → closer to destination (arrival eventually within `max(hitSize, 6)` of (800,800) on flat ground).
9. `godot_screenshot` → attach path; overlay shows wave/time and unit markers if 16 has landed (else raw units).
10. `godot_log errors` → no `UnknownRevision`/`MissingComponent`/panic lines; `godot_game stop`.

Negative checks in the same session: command a stance the unit type does not allow → `command_stance` returns `false` and no stance bit changes; spawn with a banned unit type in a rules-banned test → spawn rejected with a log line.

### 7d. Performance budget + measurement

Baseline HLP §7.4: sim tick ≤ 4 ms mid-game; 16.6 ms frame. All numbers p95 on the dev machine, release, from `mind-headless bench` (`TickReport` spans) and `cargo bench -p mind-core --bench units`.

| Profile | Load | Budget |
|---|---|---|
| `units_mid` | flat 256², 300 units (mixed kinds, all AI active), 200 buildings | unit systems (physics+AI+weapons-mount targeting, excluding bullets) ≤ 1.5 ms/tick |
| `units_stress` | 512², 1 000 units, 1 000 buildings | ≤ 5.0 ms/tick; no O(n²) unit scans (nearby queries use plan-05 quadtree) |
| `pathfinder_flat256` | 256² flat, 2 000 tile changes + 8 active fields + 40 control requests | attributable pathfinder work ≤ 0.6 ms/tick average, ≤ 1.5 ms worst tick, rebuild complete within 4 ticks |
| `rts_command_200` | 200 commanded units, 12 squads, 5 request storms | ≤ 1.2 ms/tick; `UnitGroup` formation per 50-unit squad ≤ 1.0 ms once |
| `waves_200` | spawn burst of 200 units in one wave tick | spawn tick spike ≤ 4 ms; no allocation growth after first spawn |
| Memory | path tiles 512² + active fields | ≤ 1 MiB `PathTile` + ≤ 8 MiB field/cluster caches with eviction; no growth over 10 000 ticks |
| Determinism guard | `bench-air --workers 1` vs `--workers 4`, same seed | identical checksums (plan 05 contract) |

Allocation: `bench --assert-alloc 0` for steady-state unit ticks after warmup; allowed lazy allocs: new flowfield arrays on first field creation, command-queue `SmallVec` growth (documented outliers).

### 7e. Exit criteria checklist

- [ ] `cargo test -p mind-core` green for all §7a rows; goldens committed.
- [ ] 12 unit defs registered with exact component closures and upstream revision numbers; `meta entities` diff empty.
- [ ] All 17 controllers + Player/NoAI bridges implemented; controller selection matrix matches `UnitType.controller` semantics.
- [ ] `units_spawn_path_arrive`, `units_flowfield_costs`, `units_rts_command_queue` scenarios pass with golden checksums.
- [ ] `units_waves_difficulty` matches `DumpWaves.java` golden for all seeds/difficulty/flags tested.
- [ ] `units_cargo_pickup_deliver`, `units_factory_output`, `units_legs_ik`, `units_segment_chain` pass.
- [ ] Pathfinder budget model deterministic: same seed/tile-change log → identical field dumps in 2 in-process + 2 cross-process runs, `workers 1` vs `4`.
- [ ] Controller codec round-trips type 4/6/7/8/9; save/load preserves CommandAI queue/stances/targets (via plan 04 integration test).
- [ ] Unit factory/Reconstructor/Assembler save revisions match upstream field sets (`version()` bytes 3/…); payload factory round-trip.
- [ ] MCP scenario §7c executed with logs, position deltas and screenshot attached; relay command path exercised.
- [ ] §7d budgets measured and recorded; p95s within budget; alloc-audit clean.
- [ ] `cargo clippy -p mind-core -- -D warnings` clean; no `HashMap` iteration in `ai/`/`entities/comp/unit/`.
- [ ] Public API handed to 12/13/15/16/17/21 documented in `mind-core` rustdoc (command/stance entry points, controller hooks, unit view data).
- [ ] `basepartnames` consumption verified with plan 03's generated file; `BaseRegistry` tier sort golden.

---

## 8. Risks & open decisions

| # | Item | Default taken | Status |
|---|---|---|---|
| OD-11-A | Pathfinder/`UnitGroup` work model: wall-clock daemon budgets (upstream) vs deterministic per-tick node budgets | Deterministic fixed node budgets on the sim clock, single worker joined at tick boundary (§3.7/§6.4); budgets are constants to tune after `bench-path` | **NEEDS USER DECISION** (model is required for lockstep; values need sign-off) |
| OD-11-B | `UnitGroup` formation thread | Synchronous deterministic computation in the triggering tick (squads ≤ 50; ≤ 1 ms) | **NEEDS USER DECISION** only if a mod-scale squad (> 200) becomes a target; then move to a snapshot/join async process |
| R1 | Sibling plans `06`, `07`, `08`, `10` were **not present on disk** when this plan was authored (`02`,`04`,`05` were read). Interfaces referenced (tile events, `Build.valid_place`, payload entities, `WeaponMount` firing, `EntityDefs!` seam) are this plan's proposals | Implement against the interfaces in §3; re-verify names at each sibling's kickoff; no edits to sibling files from this plan | Orchestrator must reconcile with 06/07/08/10 authors |
| R2 | `EntityDefs!`/`FieldMeta` seam (plan 05 §3.5) vs this plan's `EntityDefSpec`/bundles | 12 unit defs registered via plan 05's `entity_def!`; `defs.rs` owns the constant tables; plan 05 consumes `FieldMeta` only | Reconcile with 05 (plan 05 §8 R6 already flags) |
| R3 | `SimCommand` needs `UnitCommandQueue`/`UnitStance` variants (§6.5) | Extend plan 05's enum at M4 | Reconcile with 05 + 21 |
| R4 | Controller serialization (`io/TypeIO.writeController/readController`) has no owner in plan 04's port map | Plan 11 specifies the codec and fixtures; plan 04 implements `io/entity/controller_codec.rs`; plan 21 consumes for snapshots | Reconcile with 04 + 21 |
| R5 | Arc `Rand` bit-parity for `Waves.generate`/`randomWaveAI` | Port exact Arc `Rand` algorithm + golden vectors from `DumpWaves.java`; unrelated RNG (Fx) stays `SimRng` | No user needed; verify in M6 |
| R6 | `basepartnames` generation lives in upstream Gradle; plan 03 owns the equivalent | Plan 03 generates from `baseparts/`; this plan consumes and ships a committed fallback list | Reconcile with 03/12 (`.msch` decode) |
| R7 | `Rules.spawns`/`TeamData`/`TeamRule` types owned by plan 12 | This plan defines `SpawnGroup` and cap math; plan 12 embeds them; difficulty via `WaveDifficulty` trait | Reconcile with 12 |
| R8 | `LogicAI` ↔ `LUnitControl` setters (plan 13) | `LogicAI` state public to plan 13; timeout/reset semantics here; `checkTargetTimer` radar cache exposed | Reconcile with 13 |
| R9 | Unit view data (parts/progress/trails) consumed by 16/17 | This plan exposes read-only scheduling data + `ClientHooks` call sites; no draw code | Reconcile with 16/17 (`Lod`, parts) |
| R10 | Per-tick cost of upstream `estimateStats`/`findEnemyTile`/raycast spam in `RtsAI`/`CommandAI` | Port as-is first (parity), then profile; any optimization must preserve target choice exactly or be flagged as a deviation | Watch; plan 23 benchmarks |
| R11 | `PrebuildAI` is upstream-labeled experimental | Full port (parity D3) with an `experimental` rustdoc note; excluded from `units_mid` budget until proven hot | No user needed |
| R12 | `RepairTurret` derives from `Block`, not `BaseTurret` in this upstream revision | Port as `Block` subclass; reuse plan 10 only for damage/beam helpers | Note for 10 reconciler |
| R13 | Unit cap rules depend on campaign/PvP flags (`Rules.pvp`, `state.isCampaign`) | Implement against plan 05's minimal `Rules` surface + plan 12 extension; default flags keep vanilla values | Reconcile with 12 |
| R14 | Mobile perf: 1 000 units + both pathfinders could exceed budgets on phones | Budgets measured desktop-first; `workers` and budget constants are config-tunable; plan 22 mobile pass re-baselines | No user needed now |

---

## 9. References

### Mindustry sources read for this plan

- AGENTS: `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `entities/AGENTS.md`, `ai/AGENTS.md`, `type/AGENTS.md`, `game/AGENTS.md`, `world/blocks/AGENTS.md`, `world/AGENTS.md`, `annotations/AGENTS.md`, `io/AGENTS.md`, `tests/AGENTS.md`.
- Components: `entities/comp/{UnitComp,MechComp,LegsComp,TankComp,WaterMoveComp,CrawlComp,SegmentComp,ElevationMoveComp,PayloadComp,BuilderComp,MinerComp,BlockUnitComp,UnitTetherComp,BuildingTetherComp,ElevationMoveComp}.java` read in full; remaining unit comps read/skimmed by API.
- Unit helpers: `entities/Units.java`, `entities/units/{UnitController,AIController,WeaponMount,BuildPlan,StatusEntry}.java`.
- AI: `ai/{Pathfinder,ControlPathfinder,PathfindQueue,Astar,UnitGroup,BlockIndexer,RtsAI,BaseBuilderAI,BaseRegistry,WaveSpawner}.java`; `ai/types/*` (all 17 controllers); `ai/{UnitCommand,UnitStance}.java` (API + load order).
- Game/waves: `game/{Waves,SpawnGroup}.java`.
- Blocks: `world/blocks/units/{UnitBlock,UnitFactory,Reconstructor,UnitAssembler,UnitAssemblerModule,RepairTower,RepairTurret,UnitCargoLoader,UnitCargoUnloadPoint}.java`.
- Types/IO: `type/UnitType.java` (`create`/`spawn`/`init`/`getUnitStances`/`estimateDps`), `io/TypeIO.java` (`writeController`/`readController`), `world/meta/BlockFlag.java`.
- Generated/committed data: `annotations/src/main/resources/classids.properties`, `annotations/src/main/resources/revisions/{alpha,mace,corvus,stell,risso,latum,elude,mega,manifold,missile,block,dummy}/*.json` (latest numbers listed §6.2).
- Tests: `tests/src/test/java/ApplicationTests.java` (`resetWorld`, `initialization`, `spawnWaves`, `checkPayloads`, `allPayloadBlockTest`, `testSectorValidity`).

### Plan set

- `HIGH_LEVEL_PLAN.md` (§0 locked decisions, §2 architecture, §3 plan row 11, §4 template, §5 phase gates, §6 conventions, §7 verification, §8 addons, §9 deviations, §10 ODs)
- `PRELIMINARY_PLAN.md` (history)
- `02_CONTENT_IMPLEMENTATION_PLAN.md` (`UnitTypeDef`, `WeaponDef`, `EntityDefSpec` note, `UnitCommandDef`/`UnitStanceDef`, `R5` reconciliation)
- `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (entity codec/revisions, `TypeIO`, `R2` def-component aggregation, controller codec gap)
- `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`Sim`, schedule `TickSet`/`EntitySet`, `Groups`, `Events`, `AsyncCore`, `SimCommand`, `FieldMeta`, `OD-05-A/B`)
- `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md`, `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`, `08_LOGISTICS_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (names/rows from `HIGH_LEVEL_PLAN.md` §3; not on disk at authoring time — see §8 R1)
- `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md`, `14_UI_IMPLEMENTATION_PLAN.md`, `15_INPUT_RTS_IMPLEMENTATION_PLAN.md`, `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md`, `17_FX_PARTS_IMPLEMENTATION_PLAN.md`, `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (consumers; reconciliation items in §8)

## Changelog

- 2026-10-01 — Draft v1 authored. `EntityDefSpec` vocabulary frozen (37 kinds, 12 defs), deterministic pathfinding budget model proposed (OD-11-A flagged), all 17 controllers and 9 unit blocks mapped. No milestones executed.
- 2026-10-02 — **M0 verified + non-weapon/non-payload foundations landed on `lane/11-units` (WIP `5485dca` → this commit).** The interrupted M0 scaffold was unverified; it now builds clean and all checks pass. Added: **M3 pathfinding math** `ai/astar.rs` (per-call `AstarScratch`, `manhattan`/`euclidean`/`octile`, `TileHeuristic`; 4 tests) and `ai/control_structs.rs` (frozen `FieldIndex`/`IntraEdge`/`NodeIndex` `@Struct` packing; 3 tests); **M4 RTS runtime** `ai/unit_command_runtime.rs` (`allows_command`/`default_command`/`command_controller`/`get_unit_stances`), `ai/unit_stance_runtime.rs` (`StanceBits [u64;1]`, `set_stance`/`disable_stance`, upstream `stop`/incompatible-bit semantics) and `ai/unit_group.rs` (`UnitGroup::calculate_formation`, synchronous/join-free deviation 5, `UNIT_COLLISION_RADIUS_SCALE = 0.6`); **M6 waves core** `game/spawn_group.rs` (`SpawnGroup` struct + exact upstream JSON keys/default omission, legacy numeric boss effect, `get_spawned`/`get_shield`/`can_spawn`/`create_unit` with plan-08 payload/effect/items/shield handoff); **M1 component/lifecycle completion** unit base-closure comps (`StatusComp`/`StatusEntry`, `ItemsComp`, `ShieldComp`, `MinerComp`, `BuilderComp`, `UnitTetherComp`, `ChildComp`), segmented spawn chain (`spawn_unit_def` offsets + `SegmentComp`/`ChildComp` links; `unit.segment_unit`/`segment_end_unit` override gap recorded), `queries::{best,can_create,get_cap}`; **weapon seam** `entities/comp/unit/weapon_mount.rs` (`WeaponMount`/`WeaponsComp` storage shaped to plan 10 §3.7 + `setup_weapons`, behavior explicitly `TODO(plan 10 M4)`). Wired `game::spawn_group`, `ai::{astar,control_structs,unit_command_runtime,unit_group,unit_stance_runtime}`. New headless scenarios `units_formation` (**checksum `5ae303eb6246a3c8`**) and `units_spawn_group` (**checksum `d38aaf2f726bbee5`**) with committed goldens; `committed_goldens_match` now iterates all three `units` scenarios (existing `units_spawn_path_arrive` unchanged, `e17325bcf75b8637`). Evidence: `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p mind-core` **762 lib + 3 blocks_golden + 3 combat_golden + 2 sim_core_determinism + 2 sim_core_meta + 1 sim_core_schedule = 773 passed / 2 ignored**; `cargo test -p mind-headless` **36 passed**; `cargo check -p mind-gdext` clean. **Blockers:** plan 10 M4 weapon firing (`update_weapon`/`shoot`/`handle_bullet`/`HealBeamMount`) is not on this branch — the mount storage/`setup_weapons` layout is the frozen integration seam; `ControlPathfinder` (clusters/portals/fields/raycast), `BlockIndexer`, `CommandAI`/the remaining 16 controllers, `Waves` table/`generate`, team AI and unit blocks remain open (M2–M8). Plan-08 payload API **is** on this branch (base `ff6852e`) and is consumed by `SpawnGroup`/payload units.
