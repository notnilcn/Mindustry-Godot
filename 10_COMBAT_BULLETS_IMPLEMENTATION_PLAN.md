# 10 — Combat, Bullets, Turrets & Defense Implementation Plan

> Inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions D1–D9), §2 (architecture), §4 (nine-section template), §6–§9 (conventions). Where this file conflicts with `HIGH_LEVEL_PLAN.md`, the high-level plan wins.
> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry sources and `AGENTS.md` files in §1.

---

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Active — M0–M4 complete 2026-10-02 (plan-02 `BulletKind`/`BulletDef` gap reconciled append-only); M5 core (turret engine, item/liquid/power ammo) partial; M6–M9 open. |
| **Phase** | P4 — Combat, units, campaign (HIGH_LEVEL_PLAN §5). This plan is the first half of the P4 gate “Turret kills a unit”. |
| **Depends on** | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (Bevy schedule, entity framework, `EntityCollisions`, `Events`/`Trigger`, `Time.run`, `Groups`, `Tmp`, `SimCommand`), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (**not written at plan-write time** — building framework, `Building` lifecycle, consumers, `BlockIndexer`, `BuildingBehavior`; interface assumptions in §3.14 must be reconciled when 07 lands), `02_CONTENT_IMPLEMENTATION_PLAN.md` (`BlockDef`, `BulletDef`, `WeaponDef`, `StatusEffect`, IDs), `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (entity revisions for `Fire`/`Puddle`; `Bullet` is `serialize=false`), `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (**not written at plan-write time** — power/liquid/heat execution behind 07 consumers; contract items in §3.14). |
| **Blocks** | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (unit weapon mounts, target queries, shield/status components, movement collision callbacks), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Rules` damage multipliers, `Env.oxygen`, `Team`, `TeamData` presence), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (bullet/turret/shield/laser draw passes, `Layer` mapping), `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (`Fx.*` bodies, `DrawPart`s, screen shake, trails), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (relayed `createNet` bullet commands, building/turret snapshot fields, desync checksums), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (suite registration). |
| **Sources (read in full or skimmed as noted)** | `entities/AGENTS.md` (full); `world/blocks/AGENTS.md` (full); `content/AGENTS.md` (full); `type/AGENTS.md` (full); `graphics/AGENTS.md` (full). Java: `entities/bullet/*.java` (all 27 types, `BulletType.java` full), `entities/comp/BulletComp.java`, `entities/Damage.java`, `entities/Lightning.java`, `entities/Fires.java`, `entities/Puddles.java`, `entities/comp/FireComp.java`, `entities/comp/PuddleComp.java`, `entities/comp/ShieldComp.java`, `entities/comp/HealthComp.java`, `entities/comp/DamageComp.java`, `entities/Predict.java`, `entities/TargetPriority.java`, `entities/UnitSorts.java`, `entities/EntityCollisions.java` (collision callback contract), `entities/pattern/*.java` (all), `type/Weapon.java`, `type/weapons/{BuildWeapon,MineWeapon,PointDefenseWeapon,PointDefenseBulletWeapon,RepairBeamWeapon}.java`, `world/blocks/defense/turrets/*.java` (all 12), `world/blocks/defense/{BaseShield,ForceProjector,MendProjector,ShieldWall,ShockMine,TargetDummy}.java`, `world/blocks/ExplosionShield.java`, `content/Bullets.java`. Tests: `tests/src/test/java/ApplicationTests.java`, `DataAssetTests.java`, `PatcherTests.java` (grepped for damage/bullet tests — see §7a). |
| **Extends spine** | Adds to the plan-00 rig: (a) `MindSimHost` `#[func]` combat probe methods (§3.12) used by MCP; (b) a `Combat` tab in `/root/Spine/Ui/StateInspector` (live bullets, turret ammo/reload/heat, damage counters); (c) `scenarios/combat_*.json` fixtures; (d) `mind-headless` subcommands `combat dump|trace|bench` registered in §7b; (e) deterministic allocation counters surfaced through `Sim::checksum()`/`TickReport`. |

**Locked inputs treated as constants:** pure Rust, Godot-free `mind-core` (D1); fixed 60 Hz with `Time.delta == 1.0` inside ticks (D8); no Godot physics for sim entities; content IDs/names append-only (02); GPL-3.0 headers (D6); full parity target (D3); bullets are not part of the save format (upstream `@EntityDef(serialize = false)`), so combat uses plan 21 snapshot/relay rules only.

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Bullet framework** — the Rust equivalent of `entities/comp/BulletComp.java` plus `BulletType`’s behavior half:
   - bullet entity lifecycle (`add`/`remove`, `justSpawned`, `keepAlive`, `hit`/`absorbed` flags, pooled reuse, `serialize = false`);
   - motion (`vel`, drag, accel, `moveRelative`, `turn`, rotation semantics, weaving, homing, `circleShooter`, `followAimSpeed`);
   - collision: `EntityCollisions` callbacks (`collides`, `collision`, `getCollisions`, `hitbox`, `deltaX/Y`), sticky targets, pierce list (`collided: SmallVec<Entity>`), `pierceCap`, `pierceBuilding`, `removeAfterPierce`, `pierceDamageFactor`, `maxDamageFraction`;
   - tile raycast (`tileRaycast` Bresenham copy), `checkUnderBuild`, `collideFloor`/`collideTerrain`, `collidesTeam`, `laserAbsorb`, `hitUnder`;
   - per-kind state machines and hooks: `init(Bullet)`, `update(Bullet)`, `hit`/`hitTile`/`hitEntity`, `despawned`/`removed`, `drawLight` data, `createFrags`/`createPuddles`/`createIncend`/`createUnits`/`createSplashDamage`, `intervalBullet`, `spawnBullets`, `spawnUnit`/`despawnUnit`, lighting (`lightning`, `lightningType`), trails (view-only), `heal` behavior, `killShooter`/`instantDisappear`, `reflectable`/`absorbable`/`hittable`.
   - `BulletType` hierarchy 1:1 as behavior kinds: `BulletType` (base), `BasicBulletType`, `ArtilleryBulletType`, `MissileBulletType`, `LaserBulletType`, `ContinuousBulletType`, `ContinuousLaserBulletType`, `ContinuousFlameBulletType`, `PointBulletType`, `PointLaserBulletType`, `ShrapnelBulletType`, `LiquidBulletType`, `SpaceLiquidBulletType`, `FireBulletType`, `LightningBulletType`, `SapBulletType`, `EmpBulletType`, `RailBulletType`, `MultiBulletType`, `ExplosionBulletType`, `BombBulletType`, `InterceptorBulletType`, `MassDriverBolt`, `FlakBulletType`, `LaserBoltBulletType`, `EmptyBulletType`.
   - `create(...)` overload set and `createNet(...)` (relay handshake in §3.5), bullet pooling through plan 05’s `EntityPool`, allocation-free spawning.
2. **`Damage`** — all of `entities/Damage.java`: area unit damage with distance falloff, armor (`applyArmor`, `damagePierce`, `damageArmorMult`), splash scaling and building multiplier, `completeDamage`, `tileDamage` (raycast explosion algorithm), `collideLine`/`collideLine` with lasers and pierce caps, `collidePoint`, `findLength`/`findLaserLength`/`findPierceLength`/`collideLaser`, `linecast`, `dynamicExplosion`, `createIncend`, `applySuppression`, `status-area`, `damageUnits`, `Damage.Collided` pool replacement.
3. **Targeting helpers** — `Predict` (intercept quadratics, all overloads), `TargetPriority` constants, `UnitSorts` sort predicates (`closest/farthest/strongest/weakest/mostArmor/leastArmor/mostShield/leastShield/grouped`, `buildingDefault`, `buildingWater`), plus the `TargetQueries` trait 11 implements (nearby enemies, best target/enemy, invalidate, ally tile).
4. **Weapons (behavior half)** — `Weapon.update/shoot/bullet/handleBullet/flip/copy` semantics, `WeaponMount` state and per-kind mount subclasses (`HealBeamMount`), the `ShootPattern` family (`ShootPattern`, `ShootAlternate`, `ShootBarrel`, `ShootHelix`, `ShootMulti`, `ShootSine`, `ShootSpread`, `ShootSummon`), rotate/limit/recoil/heat/warmup/alternate/mirror/top/layer-offset fields, `parts` hook (bodies in 17), `findTarget`/`checkTarget`/`bulletRotation`, `shootStatus`, `shootOnDeath`.
   - Visual-only weapons: `BuildWeapon`, `MineWeapon`, `RepairBeamWeapon` (storage and plans owned by 11; behavior ships here because it manipulates weapon mounts and repair beams).
   - `PointDefenseWeapon`, `PointDefenseBulletWeapon` (bullet interception), `InterceptorBulletType`.
5. **Turrets** — `BaseTurret` → `ReloadTurret` → `Turret` + `TurretBuild` semantics, `ItemTurret`, `LiquidTurret`, `PowerTurret`, `ContinuousTurret`, `ContinuousLiquidTurret`, `PayloadAmmoTurret`, `LaserTurret`, `PointDefenseTurret`, `TractorBeamTurret`, `BuildTurret`: targeting (`findTarget`/`findEnemy`/`targetPosition`/`validateTarget`), ammo stacks and consumption, reload/coolant/heat efficiency, shoot warmup/charge/queued bullets, recoil, shoot effects/sounds hooks, logic/player control surface (`ControlBlock`, sense/control), save fields and revision versions.
6. **Defense blocks** — `ForceProjector` (polygon bullet absorption, shield buildup/break/cooldown, phase boost, `ExplosionShield`), `MendProjector`, `ShieldWall`, `ShockMine`, `BaseShield` (shared “shield block” behavior), plus `TargetDummy` as the canonical test fixture (required by §7c) and the `ExplosionShield` trait.
7. **Tile-bound combat entities** — `Fires` + `FireComp`, `Puddles` + `PuddleComp`, `Lightning` chain helper.
8. **Status application API** — the application half of `StatusEffect` (per-unit `apply`, area application, `statusChance`, immunities hook, opposite/transition dispatch is 11’s component; struct is 02’s).

### 2.2 “Done” means

- `cargo test -p mind-core` covers every behavior listed in §2.1; `mind-headless run combat_*` scenarios pass with golden checksums; the MCP scenario §7c passes with a screenshot and exact HP numbers; the budgets in §7d are met.
- A player-placed `duo` turret fed copper kills a `target-dummy` with the same tick count and total damage as an upstream parity worksheet (computed once from the Java reference and committed under `parity/combat/`).
- Bullet creation, travel, collision, pierce/frag, shield absorption, fire spread, puddle decay, and point-defense interception are deterministic under `--workers 1` and `--workers 4`.
- `mind-core` stays Godot-free/tokio-free; every ported file carries the GPL header; no `HashMap` iteration in sim paths.

### 2.3 Explicit boundaries (who owns what)

| Area | This plan (10) owns | Owned elsewhere |
|---|---|---|
| Bullet **metadata** (`BulletDef` fields, IDs, kind tags, `init()` derivations) | Behavior reads it; §6.1 lists fields 10 requires added/confirmed in 02 | `02_CONTENT_IMPLEMENTATION_PLAN.md` |
| Bullet **component** (`Bullet` struct, spawn/despawn, `Bullet` entity def) | Everything | — (replaces plan 05’s placeholder `entities/comp/bullet.rs`; see §3.14) |
| Entity framework, groups, pools, schedule sets, `EntityCollisions.updatePhysics`, `SimCommand`/checksums | Bullet-specific `Hitbox` callbacks and systems | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` |
| `Building` lifecycle, consumers execution, `BuildingBehavior` registration, proxmity, `BlockIndexer`, DrawBlock framework | Turret/defense behaviors registered into it | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` |
| Items into turrets, payloads into `PayloadAmmoTurret` | `acceptItem`/`handleItem`/`acceptPayload` on turret behaviors | `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (transport), `07` (module storage) |
| Power/liquid/heat **graph execution** and `Consume*` ticking; coolant heat capacity effects | Turret `canConsume`/`shouldConsume`/`updateCooling` reads | `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (plus `07` consumer plumbing) |
| Unit entities, controllers, weapon-mount **storage**, `StatusComp`, `ShieldComp`, pathfinding, movement solid checks that trigger `unitOn` | `Weapon`/`WeaponMount` behavior + `TargetQueries` consumer; ShockMine’s `unitOn` callback body | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| `Rules` fields (`fire`, `damageExplosions`, `unitDamage`, `blockDamage`, `unitHealth`, `infiniteResources`, `buildCostMultiplier`, `fog`), `Team`/`Teams`/`TeamData.present` | Reads them through narrow accessors | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` |
| Tile grid, `Tiles` fire/puddle slot arrays, `Tile.flammability`, floor `liquidDrop`, `World.raycast*`, `Tile.worldx/worldy` | Consumes through the §3.14 trait | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` |
| Z-order, sprite batches, shaders, shields/lasers/lightning **rendering**, pixelation | Emits view structs (`BulletDrawState`, `TurretDrawState`, `ShieldDrawState`, `LaserDrawState`) | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| `Fx.*` bodies, `DrawPart` bodies, sound playback, screen shake implementation | Calls `CombatFx` hooks (no-op in headless) | `17_FX_PARTS_IMPLEMENTATION_PLAN.md`, `18_AUDIO_IMPLEMENTATION_PLAN.md` |
| Input/RTS control of turrets and weapons, build-plan UI | Logic/control state machine only | `14_UI_IMPLEMENTATION_PLAN.md`, `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` |
| Snapshot/relay of bullets and turret state, desync detection, `createNet` transport | `BulletSpawn` event shape + `SimCommand` variant proposal | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Addon adoption (BlastBullets2D) | Evaluation + decision recorded; simulation never leaves Rust | `16`/`17` own rendering if adopted |

### 2.4 Deliberate deviations (reason stated)

1. **No Java inheritance for bullet behavior.** `BulletType` subclasses become `BulletDef { kind: BulletKind, ... }` (02) plus a static `BulletBehavior` implementation per kind dispatched through `bullets()`. Vanilla bullets are field-configured only (verified: `content/Blocks.java` creates 164 bullets, all `{{ field = ... }}` with zero method overrides), and JSON mods select behavior by class name (`type: LaserBulletType`), which maps to a kind. A per-def `behavior_override: Option<BehaviorId>` is reserved for plan 20/OD1 scripting. Observable behavior identical.
2. **`Object data` becomes a typed enum** (`BulletData`). Vanilla uses `data` for interceptor targets, mass-driver payloads, and logic `shootp`; a closed enum + `Custom(u32)` slot preserves behavior without `Any`/downcasts; serialization of transient data is unnecessary (`Bullet` is not saved).
3. **Trails live in the view layer.** Java updates `b.trail` only when `!headless` (`BulletType.updateTrail` guards on `headless`). Rust keeps `mind-core` allocation-free and emits trail points through `CombatFx`; plan 17’s `Trail` renders them. Same visuals, no sim state.
4. **`Effect.at`/`Sound.at`/screen shake are hooks.** Sim only calls `CombatFx` (no-op default). This preserves the upstream `headless` guards structurally instead of by branch.
5. **`Time.run(0f, ...)` sites** (`Damage.tileDamage`, delayed `Weapon` shots, `createFrags` delay) use plan 05’s deterministic `SimClock::run`, preserving tick ordering; delayed bullets are created at the same schedule point as Java because `TimeRuns` runs before `EntityUpdate`.
6. **`net.client()` guards become authority checks** via plan 21’s `Authority` resource (`authority.client()`), not `Vars.net`. Headless matches default to authoritative.
7. **Float parity is Rust↔Rust** (HIGH_LEVEL §9). All sim trig/sqrt/pow go through `mind_core::math` wrappers; §8 OD-10-A records the deterministic-math default.
8. **`Groups.bullet.intersect` quad-tree** is plan 05’s `EntityGroup::intersect`; this plan does not add its own spatial index.
9. **`MassDriverBolt`’s `DriverBulletData`** type is owned by 08’s `MassDriver`; 10 consumes a `MassDriverPayload` trait to avoid a 08→10 cycle (§3.14).
10. **Draw code ports as data, not immediate drawing.** Bullet/laser/shield geometry generators are pure functions writing `*DrawState`; plans 16/17 own Godot draw calls.

### 2.5 Deferred to sibling plans

`Wall`/`Door`/`Radar`/`RegenProjector`/`OverdriveProjector`/`Thruster`/shockwave tower and all non-combat blocks (07); unit weapons mounting/firing schedules, AI target selection, `StatusComp`, unit shields (11); campaign damage rules and `Rules` I/O (12); mlog `ControlBlock` sense protocol bindings and world-processor privileges (13); all rendering (16), FX/sound (17/18); net bullets/turret snapshots (21); golden dumps/perf CI (23).

---

## 3. Target design

All names below are final unless marked. `mind-core` is Godot-free; `bevy_ecs` is a library pinned by plan 00. No `HashMap` iteration in sim paths; ordered `IndexMap`/`BTreeMap` only. Hot paths use `SmallVec`, scratch buffers and plan 05’s `Tmp`.

### 3.1 Module layout (`mind-core`)

```
client/rust/mind-core/src/
  combat/
    mod.rs                  # CombatPlugin; entity defs (Bullet/Fire/Puddle); public API re-exports
    bullet/
      mod.rs                # `Bullet` component (BulletComp port), lifecycle, def registration
      spawn.rs              # BulletSpawn/create()/create_net()/BulletData
      raycast.rs            # tileRaycast, check_under_build, sticky attach/release
      behavior.rs           # BulletBehavior trait, BulletKind dispatch, BEHAVIOR_TABLE
      kinds/
        basic.rs artillery.rs missile.rs laser.rs continuous_laser.rs continuous_flame.rs
        point.rs point_laser.rs shrapnel.rs liquid.rs space_liquid.rs fire.rs lightning.rs
        sap.rs emp.rs rail.rs multi.rs explosion.rs bomb.rs interceptor.rs mass_driver.rs
        flak.rs laser_bolt.rs empty.rs
    damage/
      mod.rs                # public Damage API
      area.rs line.rs point.rs explosion.rs suppress.rs armor.rs
      shield.rs             # ExplosionShield trait, BlockFlag::SHIELD helper
      status.rs             # StatusApply trait + area application
    lightning.rs            # Lightning.create + chain algorithm
    fires.rs                # Fires API + Fire component/system (tile-bound pooled entity)
    puddles.rs              # Puddles API + Puddle component/system
    targeting.rs            # Predict, TargetPriority, UnitSorts, TargetQueries
    view.rs                 # BulletDrawState/TurretDrawState/ShieldDrawState/LaserDrawState + CombatFx hooks
  weapons/
    mod.rs                  # Weapon behavior (Weapon.update/shoot/bullet/flip/load-hook), WeaponKind dispatch
    mount.rs                # WeaponMount + factory kinds (HealBeamMount)
    pattern.rs              # ShootPattern trait + all 7 patterns + BulletHandler
    point_defense_weapon.rs point_defense_bullet_weapon.rs
    repair_beam_weapon.rs build_weapon.rs mine_weapon.rs
  world/blocks/defense/
    mod.rs                  # BlockBehavior registrations for defense kinds
    base_shield.rs force_projector.rs mend_projector.rs shield_wall.rs shock_mine.rs target_dummy.rs
    turrets/
      mod.rs                # BaseTurret/ReloadTurret/Turret behavior + TurretState component
      item_turret.rs liquid_turret.rs power_turret.rs continuous_turret.rs
      continuous_liquid_turret.rs payload_ammo_turret.rs laser_turret.rs
      point_defense_turret.rs tractor_beam_turret.rs build_turret.rs
```

The `mindustry.type.weapons` package maps to `mind_core::weapons` because `type` is a Rust keyword (documented deviation). Metadata halves stay in plan 02 (`content/registries/bullets.rs`, `content/registries/units/weapon.rs`).

### 3.2 Schedule & ECS integration (plan 05’s sets)

`CombatPlugin` registers exactly one plugin and never edits 05 core files. Systems are inserted into the existing sets (`05` §3.4):

| Java call / behavior | Rust placement | Notes |
|---|---|---|
| `Groups.bullet.updatePhysics()` | `EntitySet::PhysicsBullets` (05) | 05 records `last` positions; 10 implements `Hitbox` accessors/mass. |
| `Groups.all.update()` → Fire/Puddle `update()` | `EntitySet::UpdateAll` | 10 adds `update_fires` + `update_puddles` systems; both filtered to the `all` group membership. |
| `Groups.bullet.update()` | `EntitySet::UpdateBullets` | 10 system `update_bullets` in slot order: motion, mover, accel, kind `update`, sticky/raycast, pierce-cap removal, `keepAlive`. |
| `Groups.bullet.collide()` | `EntitySet::CollideBullets` (05) | 05 iterates and calls `Bullet::collides`/`collision`/`get_collisions`. |
| `Groups.build.update()` → `TurretBuild.updateTile()` | `EntitySet::UpdateBuildings` | 10 registers `BuildingBehavior` impls for turret/defense kinds (07 extension point); fallback `UpdateTurrets` system if 07’s table is not extensible (see §8 R-10-1). |
| `Time.run(0f, tileDamage)`, delayed shots | `TickSet::TimeRuns` (05) | Same ordering as Java. |
| `Events.fire(Trigger.*)` (`turretCool`, `flameAmmo`, `resupplyTurret`, `forceProjectorBreak`, `fireCreate`, `fireExtinguish`, exclusion death) | `TickSet::DrainEvents` | 10 fires the combat subset of `SimEvent`/`Trigger`. |

Determinism invariants:
- Bullet iteration follows plan 05’s insertion-stable `EntityGroup` order; `Groups.bullet.intersect` results are stable-sorted before use.
- `damage` operations never iterate a `HashMap`; the splash target list uses `SmallVec` filled in group slot order.
- RNG: bullets/damage use `RngStream::Combat` (05), created at boot from the match seed. `Lightning`’s Java static `lastSeed++` becomes a `CombatRng::next_lightning_seed()` monotonic counter reset on `ResetEvent` so replays are stable.
- No wall-clock reads; `x/y` are f32 world units; `state.tick` is plan 05’s clock.

### 3.3 Bullet entity & lifecycle

```rust
/// Ported from entities/comp/BulletComp.java
#[derive(Component)]
pub struct Bullet {
    pub def: BulletId,                 // content id of BulletDef
    pub damage: f32,                   // instance damage (includes damageMultiplier at spawn)
    pub building_damage_multiplier: f32,
    pub data: BulletData,
    pub fdata: f32,
    pub vel: Vec2f,
    pub rotation: f32,
    pub hit_size: f32,
    pub last: (f32, f32),
    pub aim: (f32, f32),
    pub origin: (f32, f32),
    pub aim_tile: Option<(i16, i16)>,
    pub time: f32,
    pub lifetime: f32,
    pub collided: SmallVec<[Entity; 6]>,
    pub sticky: Option<StickyTarget>,
    pub flags: BulletFlags,            // keep_alive, just_spawned, absorbed, hit, owner_local
    pub frags: i32,
    pub keep_alive: bool,
}

#[derive(Component, Default)]
pub struct OwnerRef(pub Option<Entity>);
#[derive(Component)]
pub struct ShooterRef(pub Option<Entity>);

pub enum BulletData {
    None,
    Bullet(Entity),                    // interceptor target
    MassDriver(Entity),                // trait object via MassDriverPayload (§3.14)
    Custom(u32),
}
```

Lifecycle (exact upstream behavior):
- `BulletType.create(...)` (all overloads funnel into `spawn.rs::create`) — applies `angleOffset`/`randomAngleOffset`, `createChance`, `ignoreSpawnAngle`, `spawnUnit` branch (11 hook: `UnitType::spawn` + `Units::notify_unit_spawn`), then builds a pooled bullet through plan 05’s `EntityPool`: sets `type`, `owner`, `shooter`, `team`, `time=0`, `origin`, `aimTile`/`aimX`/`aimY`, `initVel(angle, speed*velocityScl*rand)`, position/last position, `lifetime*lifetimeScl*rand`, `data`, `hitSize`, `mover`, `damage = (damage<0 ? def.damage : damage) * damage_multiplier()`, `buildingDamageMultiplier`, adds to groups, applies `keepVelocity`/`scaleKeepVelocity`.
- `add()` → `behavior.init(bullet)`; `remove()` → if `!hit` call `despawned`, always call `removed`, clear `collided`. `absorb()` sets `absorbed` then `remove()`.
- `collision()` is called by 05’s entity collision pass; sticky bullets attach (`stickTo`) instead of hitting; non-pierce bullets set `hit` and `remove`; pierce bullets push the other entity’s id into `collided`.
- `update()` ordering is byte-for-byte: move (skipped one tick for `justSpawned`), drag, mover, accel, kind `update`, sticky re-position or tile raycast, `removeAfterPierce` cap check, `keepAlive` decrement.
- `hit_size` is collision size; `drawSize` only affects culling/clip (`clipSize` replacement).
- `Senseable`/`Settable` (`LAccess` mapping in §3.11) — mlog sensor hooks consumed by 13.

### 3.4 Bullet behavior dispatch (`combat/bullet/kinds/`)

```rust
pub trait BulletBehavior: Sync + 'static {
    fn init(&self, w: &mut BulletWorld, b: Entity) {}
    fn update(&self, w: &mut BulletWorld, b: Entity) {}
    fn hit(&self, w: &mut BulletWorld, b: Entity, x: f32, y: f32, create_frags: bool) { default_hit(self, w, b, x, y, create_frags) }
    fn hit_tile(&self, w: &mut BulletWorld, b: Entity, build: Entity, x: f32, y: f32, initial_health: f32, direct: bool) { default_hit_tile(...) }
    fn hit_entity(&self, w: &mut BulletWorld, b: Entity, other: Entity, health: f32) { default_hit_entity(...) }
    fn despawned(&self, w: &mut BulletWorld, b: Entity) { default_despawned(...) }
    fn removed(&self, w: &mut BulletWorld, b: Entity) { default_removed(...) }
    fn handle_pierce(&self, w: &mut BulletWorld, b: Entity, initial_health: f32, x: f32, y: f32) { default_handle_pierce(...) }
    fn test_collision(&self, def: &BulletDef, bullet_team: TeamId, build: Entity) -> bool;
    fn building_damage(&self, b: &Bullet) -> f32;
    fn shield_damage(&self, b: &Bullet) -> f32;
    fn continuous_damage(&self, def: &BulletDef) -> f32 { -1.0 }
    fn current_length(&self, w: &BulletWorld, b: Entity) -> f32;
    fn range(&self, def: &BulletDef) -> f32;      // def.range from 02
    fn draw_state(&self, w: &BulletWorld, b: Entity, out: &mut BulletDrawState); // view-only
}

pub static BEHAVIOR_TABLE: [&dyn BulletBehavior; BulletKind::COUNT] = [...];
pub fn behavior(def: &BulletDef) -> &'static dyn BulletBehavior;
```

Default hook bodies port `BulletType` exactly (frags/puddles/incend/units/shake/suppression/splash/lightning in `hit`; heal + lifesteal + `handlePierce` in `hitTile`/`hitEntity`). Per-kind behavior notes (all read from source; behavior must match field-for-field):

| Kind (Java) | Distinct behavior to port |
|---|---|
| `BulletType` base | `init` (killShooter, instantDisappear, spawnBullets), `update` chain (trail view hook, homing, weaving, trail effects, interval bullets), all hit/despawn/remove defaults. |
| `BasicBulletType` | front/back sprite names, `width/height`, `shrinkX/Y`, spin, `mixColorFrom/To`, muzzle flash offset; draw data only. |
| `ArtilleryBulletType` | `trailMult`/`trailSize` visual fields; `hit()` splash defaults via base. |
| `MissileBulletType` | `trailEffect`/`smokeEffect` defaults + homing-friendly defaults; no algorithm override (inherits base). |
| `LaserBulletType` | `init(Bullet)` runs `Damage.collideLaser` immediately, sets `b.fdata` and time; `chargeEffect`; side-lightning scheduling; `largeHit`. |
| `ContinuousBulletType` | `continuous` flag, `applyDamage` via `Damage.collideLine` every `damageInterval` tick (`timer(1)`), `timescaleDamage`, `currentLength`, `length` range override. |
| `ContinuousLaserBulletType` | draws segmented laser with `colors[]`; `currentLength` smooth; light stroke. |
| `ContinuousFlameBulletType` | flame drawing data + `lengthInterp`, divisions, flare; no gameplay override. |
| `PointBulletType` | `init`: centers aim, iterates trail, sets `time=lifetime`, teleports to end point, picks closest target within 1 unit, hits/removes, zeroes velocity. |
| `PointLaserBulletType` | `continuousDamage`/`update` continuous beam + periodic trail/beam effects (view hook). |
| `ShrapnelBulletType` | serrated draw data; `init` sets `pierce = false`, `collides = false`, uses `Damage.collideLine`-style instant hit at `init`? (source: `init(Bullet)` calls `Damage.collideLaser`/point damage — implement exactly). |
| `LiquidBulletType` | orb draw data; `update` extinguishes fire on contact tile and evaporates when `will_boil`; `hit` deposits puddle, no damage; `despawned` deposits; `knockback`/`drag`; applies `liquid.effect` status. |
| `SpaceLiquidBulletType` | orb size, no puddle; space-env rendering. |
| `FireBulletType` | random flame trail; on hit calls `Damage.createIncend`/`Fires.create`; no direct damage defaults. |
| `LightningBulletType` | instant lightning branch at `init` (`Lightning.create`), no body; `estimateDPS`/`calculateRange` overrides. |
| `SapBulletType` | `init` finds target via `Units.closestTarget` and sets `initVelocity` toward it; on hit deals `sapStrength`-scaled damage; view line data. |
| `EmpBulletType` | `hit` override: area EMP — power damage scaling (`powerDamageScl`/`powerSclDecrease`), unit damage (`unitDamageScl`), `timeIncrease/timeDuration` applied to buildings via `applyHealSuppression`-adjacent API, effects. |
| `RailBulletType` | `handlePierce` override (no damage reduction but repeated `pointEffect`), `lineEffect`/`endEffect`, `testCollision`/`hitTile` overrides for piercing terrain. |
| `MultiBulletType` | spawns all `bullets[]` `repeat` times at init; `calculateRange` max. |
| `ExplosionBulletType` | `hit` spawns `Damage.dynamicExplosion`; no body. |
| `BombBulletType` | `hit` spawns `Damage.dynamicExplosion` with item stats; despawn = explode. |
| `InterceptorBulletType` | `update` checks `BulletData::Bullet` overlap with `EntityCollisions::collide`; on intercept sets position, `hit`, removes; reduces target damage or removes it. |
| `MassDriverBolt` | `update` intersection with `MassDriverPayload::handle_payload`; `despawned` drops items; `hit` computes explosiveness/flammability/charge and calls `Damage.dynamicExplosion`. |
| `FlakBulletType` | `explodeRange`/`explodeDelay`/`flakDelay`/`flakInterval` proximity airburst logic (update override). |
| `LaserBoltBulletType` | draw-only (width/height bolt). |
| `EmptyBulletType` | no-op behavior (visual weapons). |

### 3.5 Spawning, pooled creation & `createNet`

```rust
pub struct BulletSpawn {
    pub def: BulletId,
    pub owner: Option<Entity>,
    pub shooter: Option<Entity>,
    pub team: TeamId,
    pub x: f32, pub y: f32, pub angle: f32,
    pub damage: f32,          // -1 = def default
    pub velocity_scl: f32,
    pub lifetime_scl: f32,
    pub data: BulletData,
    pub mover: Option<MoverRef>,
    pub aim_x: f32, pub aim_y: f32,      // -1 = none
    pub target: Option<Entity>,
}

pub fn create(w: &mut World, spawn: BulletSpawn) -> Option<Entity>;        // local deterministic
pub fn create_net(w: &mut World, spawn: BulletSpawn) -> Option<Entity>;    // + relay command
```

- Local weapon/turret/lightning/puddle bullets call `create`; only server-authoritative sources call `create_net` (`Puddles` fireball, `FireComp` fireball, `Damage.dynamicExplosion` fireballs, `Lightning` when `net.server`, mass-driver? — upstream `MassDriverBolt` is created locally via `createNet` from `MassDriver`; 08 decides). `create_net` emits `SimCommand::Bullet(spawn)` **through plan 21’s ordered relay**, preserving Java `Call.createBullet` semantics (`unreliable`, no owner/shooter).
- Plan 21 owns the `SimCommand::Bullet` variant; this plan owns the payload struct and guarantees application through the identical `create` code path (no divergence between local and relayed).
- Pooling: bullet def is `pooled = true, serialize = false` (05 `EntityPool`); spawn from pool inserts base components + `Bullet` and resets all fields; `EntityIds` monotonic (never reuse ids across a match, matching 05).
- Bullets are **never** in saves and **not** in plan 21 world snapshots; late joiners recreate the deterministic local stream and rely on relayed `create_net` events for server-only sources.

### 3.6 Damage system (`combat/damage/`)

Public API (names/exact overloads ported from `Damage.java`):

```rust
pub fn apply_suppression(w, team, x, y, range, reload, max_delay, particle_chance, source: Option<Entity>, color);
pub fn dynamic_explosion(w, x, y, flammability, explosiveness, power, radius, damage: bool, fire: bool, ignore_team: Option<TeamId>, fx: EffectId, base_shake);
pub fn create_incend(w, x, y, range, amount);
pub fn find_length(w, bullet, length, laser, pierce_cap) -> f32;
pub fn find_laser_length(w, bullet, length) -> f32;
pub fn find_pierce_length(w, bullet, pierce_cap, laser, length) -> f32;
pub fn collide_laser(w, bullet, length, large, laser, pierce_cap) -> f32;   // stores b.fdata
pub fn collide_line(w, hitter, team, x, y, angle, length, large, laser, pierce_cap);
pub fn collide_point(w, hitter, team, effect: EffectId, x, y);
pub fn linecast(w, hitter, x, y, angle, length) -> Option<Entity>;
pub fn damage_units(w, team, x, y, size, damage, predicate, acceptor);
pub fn damage(w, team: Option<TeamId>, x, y, radius, damage, opts: DamageOpts);
pub fn status(w, team, x, y, radius, effect: StatusId, duration, air, ground, chance);
pub fn tile_damage(w, team, tx, ty, base_radius, damage, source: Option<Entity>);
pub fn apply_armor(damage: f32, armor: f32) -> f32;                        // max(d - a, 0.15*d)
```

Ported mechanics and gotchas (must-test list):
- `minArmorDamage` = 0.15 (Arc `Vars`-adjacent constant in `Damage`); `applyArmor` exact.
- `damage` falloff: `lerp(1 - dist/radius, 1, 0.4)`; with `scaled`, distance is clamped to `max(0, dst - unit.hit_size/2)`.
- Unit velocity displacement `(1 - dst/radius) * 2 / mass` always applied (even zero damage).
- Shield interop: `hitEntity` adds `shield` to effective health, caps by `maxDamageFraction`, applies `damagePierce`/`damageArmorMult`; `ForceProjector` reads `bullet.type.shieldDamage(b)`.
- Building damage uses `source.type.buildingDamageMultiplier`; healing bullets (`heals()`) skip enemy damage and call `testCollision`.
- `tileDamage` uses the exact same-angle raycast accumulation (`Point2.pack` map, `edgeScale = 0.6`), with the multiblock compensation branch; ported as a `SmallVec`/sorted array, **no `IntFloatMap` iteration order leak** (accumulate then iterate in insertion order).
- Suppression scales particle count by target count, uses `Time.run(random(max_delay))`.
- `dynamicExplosion` lightning/fire/wave scheduling (`power/700` clamp, `explosiveness/11` waves), `ExplosionShield` absorption via 07’s `BlockIndexer::get_enemy(team, BlockFlag::SHIELD)`.
- `completeDamage` square radius semantics; `collideLine` neighbor-block expansion (`Geometry.d4`, `large` flag), collided-sort by `dst2`, pierce cap counting, heal pass.
- All target enumeration via `TargetQueries` (11) and building index via 07.

### 3.7 Weapons & patterns (`mind-core/src/weapons/`)

```rust
pub trait ShootPattern: Send + Sync {
    fn shots(&self) -> i32;
    fn first_shot_delay(&self) -> f32;
    fn shot_delay(&self) -> f32;
    fn shoot(&self, total_shots: i32, handler: &mut dyn BulletHandler, barrel_inc: Option<&mut dyn FnMut()>);
    fn flip(&mut self) {}
    fn copy_box(&self) -> ShootPatternSpec;    // data form; runtime dispatch via kind tag (02)
}
pub enum ShootPatternKind { Single, Alternate { barrels, spread, barrel_offset, mirror }, Barrel { .. }, Helix { .. }, Multi { .. }, Sine { .. }, Spread { .. }, Summon { .. } }
```

`WeaponDef` (02) stays metadata; this module ships `WeaponKind` behavior dispatch and the mount engine:

```rust
pub struct WeaponMount {
    pub reload: f32, pub recoil: f32, pub recoils: SmallVec<[f32; 4]>,
    pub smooth_reload: f32, pub charge: f32, pub warmup: f32, pub heat: f32,
    pub rotation: f32, pub target_rotation: f32,
    pub aim: (f32, f32), pub target: Option<Entity>, pub retarget: f32,
    pub shoot: bool, pub rotate: bool, pub side: bool, pub charging: bool,
    pub barrel_counter: i32, pub total_shots: i32,
    pub bullet: Option<Entity>, pub last_length: f32,
    pub allow_shoot_effects: bool,
}
pub fn update_weapon(w: &mut World, unit: Entity, weapon: &WeaponDef, mount: &mut WeaponMount);
```

- `update_weapon` ports `Weapon.update` unconditionally: reload decay, recoil (`recoils` array), smooth reload, charge, warmup (`linear`/lerp), mount/axis geometry, auto-target (`findTarget`/`checkTarget`/`targetInterval`/`targetSwitchInterval`), rotation limits, alternate side flip (`other_side`), sound loop hook, velocity gates (`minShootVelocity`/`maxShootVelocity`), shooting condition, `shoot()` and reload reset.
- `Weapon::shoot` wraps `ShootPattern::shoot` with delay scheduling through `SimClock::run` and barrel-counter save/restore; `bullet()` ports spread, life scale (`scaleLife` distance clamp), inaccuracy, `shooter` selection (missile AI pass-through via 11’s controller hook), `mount.bullet`, effects hooks, unit recoil impulse, shake, heat/recoil set.
- `handleBullet` extension points for `PointDefenseBulletWeapon` (set `data`) and `Continuous` weapons (aim length).
- `flip()`/`copy()` ported; mirrored mounts and doubled reload are derived in 02’s `UnitTypeDef.init`.
- `RepairBeamWeapon`: `HealBeamMount` state (`offset`, `last_end`, `strength`, `effect_timer`), `update` heal logic + target snapping raycast, `draw` data for 17; `RepairTurret::drawBeam` geometry helper shared with `BuildTurret`.
- `BuildWeapon`/`MineWeapon`: aim at `BuildPlan`/mine tile from 11’s unit state, never shoot.
- `PointDefenseWeapon`/`PointDefenseBulletWeapon`: bullet-group target selection (`Groups.bullet.intersect` + min by `dst2` or `dst2 - damage*damageTargetWeight`), damage application rules, `InterceptorBulletType` data hand-off.

### 3.8 Turrets (`world/blocks/defense/turrets/`)

`TurretState` is a Bevy component inserted on building spawn for turret kinds (07 extension point; fallback system in §3.2):

```rust
#[derive(Component)]
pub struct TurretState {
    pub rotation: f32, pub reload_counter: f32, pub cur_recoil: f32,
    pub cur_recoils: SmallVec<[f32; 4]>, pub heat: f32, pub heat_req: f32,
    pub side_heat: [f32; 4],
    pub shoot_warmup: f32, pub charge: f32, pub warmup_hold: f32,
    pub total_shots: i32, pub barrel_counter: i32, pub queued_bullets: i32,
    pub logic_control_time: f32, pub logic_shooting: bool,
    pub target: Option<Entity>, pub target_pos: Vec2f,
    pub was_shooting: bool, pub is_shooting: bool,
    pub ammo: SmallVec<[AmmoEntry; 4]>, pub total_ammo: i32,
    pub phase: TurretPhase,     // activation, laser bullets, continuous bullets, payload subset
}
```

`TurretBehavior` (07 trait impl) port order in `update_tile`: `validateTarget` → `isShooting` → proxy-unit sync (`unit.tile/team/rotation/ammo`) → `soundLoop` hook → warmup (hold/maintain) → recoil decay → heat decay → charge → `recoilOffset` → logic timer → heat `calculateHeat` (09) → rotate sync for output rotation → `handleReload` (`updateReload` + `updateCooling`) → fog radius update (12) → activation timer → targeting (`findTarget` timer, `targetPosition` with `Predict` and accurate-delay offset, `turnToTarget`, cone check) → `updateShooting`.
- `updateShooting` → `shoot(type)`: pattern from `def.shoot_pattern` if set else turret `shoot`; `Time.run` delays; `consumeAmmoOnce` handling; `bullet()` creation with life scaling (`scaleLifetimeOffset`, `minRange/range`), effects/sounds hooks, `ammoUseEffect`, recoil/heat/totalShots; optional per-shot ammo consumption.
- `useAmmo`/`peekAmmo`/`hasAmmo` ported with the “swap a usable entry to the back” rule; `ItemTurret` ammo entry map keyed by `ItemId` (insertion-ordered `IndexMap`), `handleItem`/`acceptItem`/`acceptStack` rules, `ammoMultiplier`; `LiquidTurret` liquid map + extinguish fire targeting + `liquids.remove`; `PowerTurret` no ammo but power gate; `PayloadAmmoTurret` payload counts (08 hook); `ContinuousTurret` bullet list + keep-alive/length update; `ContinuousLiquidTurret` activation window; `LaserTurret` coolant-fueled reload + bullet lifetime; `PointDefenseTurret` bullet interception + `bulletDamage`; `TractorBeamTurret` pull/status/damage; `BuildTurret` proxy-unit building loop (11/15 hooks).
- `estimateDps`, `progress`, `range`/`minRange`/`trackingRange`, `warmup`, `sense`/`control` (mlog 13), `fogRadius` (12), `canControl`/`controlled`/`logicControlled`, `ActivationTimer` bar data.
- Save fields per class revision: `TurretBuild` version 1 (`reloadCounter`, `rotation`), `ItemTurret` version 2 (`ammo[size]`, item short ids + amounts, revision-aware `ub`/`short`), `ContinuousTurret` version 3 (`lastLength`); serialized through 07’s building chunk and 04’s revision manifests.

### 3.9 Defense blocks

- `ForceProjector`: field/coolant consumers (07/09), `phaseHeat` item booster, `buildup`/`broken` state, per-tick `deflectBullets` polygon test over `Groups.bullet` (`Intersector::is_in_regular_polygon` port in `mind_core::math`), `shield_damage` accumulation, break/cooldown (`cooldownNormal/Liquid/BrokenBase`, coolant heat-capacity factor), `ExplosionShield::absorb_explosion`, `realRadius = (radius + phaseHeat*phaseRadiusBoost)*radscl`, `sense(shield/heat)`, overwrote transfer, save fields.
- `MendProjector`: `heat`/`charge`/`phaseHeat`/`smoothEfficiency`, `indexer.each_block` heal loop (`damaged() && !isHealSuppressed()`), item booster consumption timer, `consume()` gating, effect/sound hooks.
- `ShieldWall`: `shield`/`breakTimer`/`shieldRadius`, damage interception order (`shieldTaken`, overflow to `super.damage`), `broken()` = timer or `!canConsume()`, regen, save field.
- `BaseShield`: power-scaled `smoothRadius`, bullet absorb loop, unit push-out/kill logic (uses `Units.nearbyEnemies` and `unit.move` from 11), save v1 (`smoothRadius`, `broken`).
- `ShockMine`: `unit_on` trigger (`enabled` + `timer(cooldown)`), lightning tendrils + `bullet` fan; `update = false`/`destructible = true` semantics handled by 07.
- `TargetDummy`: fixture and combat test oracle — proxy dummy unit (11), tether config `[5]` int array, DPS/hits counters, `dummyHit` hook used by `HealthComp` (unit armor/hp loop), `collide`/`collision` returning false so bullets hit the unit, save fields. Owned here because §7c depends on it; registered as a test-only block behavior if 07 flags it non-gameplay.
- `ExplosionShield` trait (root `world/blocks/ExplosionShield.java`) ships in `damage/shield.rs`; 07 re-exports for other blocks.

### 3.10 Fires, puddles, lightning

- `Fires`/`Fire` — tile slot API contract with 06 (`Tiles::fire/set_fire`), create/refresh (`baseLifetime = 1000`, refresh resets `time`), `has`, `extinguish`, `remove`, `register`; `Fire` component update ports `FireComp.update` exactly (animation view data, water attribute speed multiplier, flammability accumulation from floor + puddle, spread timer `22`, fireball timer `40` + `create_net`, `damageTimer 40` with `tileDamage 1.8` and unit burn `3`/5 s, `Env.oxygen` requirement checked in `Fires::create`). Fire is `pooled`, saved (not `serialize=false`), `Syncc`; def entry registered in 05/04’s table.
- `Puddles`/`Puddle` — deposit chain (`will_boil`, space env liquid bullets, liquid-floor reaction, solid floor reject, client no-create guard, `maxLiquid = 70`, `accepting` smoothing), `react_puddle` (fire on flammable/hot pairs, steam, `Liquid::react` in 09), `has_liquid`, `remove`, `register`; `Puddle` update ports amount decay/spread to `d4` neighbors, cap behavior, unit status/ripple hook, `puddleOn` building hook, particle/sound hooks. Liquid methods (`will_boil`, `can_stay_on`, `react`, `update`, `draw_puddle`) are 02 data + 09 behavior.
- `Lightning` — `create` overloads, chain algorithm with `Rand` (05 `RngStream::Combat` + monotonic seed), insulation raycast snapping, `maxChain = 8`, `hitRange = 30`, furthest-unit selection, `pierceCap` interaction, effect data for 17.

### 3.11 Status application API

`combat/damage/status.rs`:
```rust
pub trait StatusApply {
    fn apply(&mut self, w: &mut World, entity: Entity, status: StatusId, duration: f32);
    fn is_immune(&self, w: &World, entity: Entity, status: StatusId) -> bool;
}
pub fn apply_status(w, entity, status, duration);
pub fn status_area(w, team, x, y, radius, effect, duration, air, ground, chance);
```
Bullets (`status`, `statusChance`, `statusDuration`), `createSplashDamage` status pass, `TractorBeamTurret`, and `LiquidBulletType` (`liquid.effect`) route here; plan 11’s `StatusComp` implements `StatusApply` and owns transitions/opposites/interval damage (02 struct + init tables).

### 3.12 Godot seams (view-only)

- `MindSimHost` new `#[func]`s (plan 00 node `/root/Spine/SimHost`; add, never rename):
  - `combat_stats(x: i64, y: i64) -> Dictionary` — turret/dummy snapshot: `{kind, ammo, ammo_type, total_ammo, reload, rotation, heat, target, shots, hp, max_hp}`;
  - `combat_counters() -> Dictionary` — `{bullets_live, bullets_created, bullets_removed, shots_fired, damage_dealt, damage_absorbed}`;
  - `spawn_bullet(def_name: String, x: f32, y: f32, angle: f32, team: i64) -> bool` — dev probe; routes through `BulletSpawn` + `SimCommand` path;
  - `set_block_health(x: i64, y: i64, hp: f32) -> bool` and `set_ammo(x: i64, y: i64, item: String, amount: i64) -> bool` — deterministic test setup without UI.
- State inspector `Combat` tab reads `combat_counters()` and the live bullet list (first N) from a `combat.dump` API.
- Rendering consumes `combat/view.rs` draw states: `BulletDrawState { def, x, y, rotation, fin, fdata, shield_alpha }`, `TurretDrawState { rotation, recoil, heat, warmup, charge, ammo_fraction }`, `ShieldDrawState { center, radius, sides, rotation, color, hit, broken }`, `LaserDrawState { from, to, width, colors, life }`. 16 maps these to `Layer`/atlas regions; 17 plays `CombatFx` events and maintains trails.

### 3.13 STDB surfaces (21 handshake)

- No tables/reducers/views are defined here. Combat contributes to plan 21:
  1. `SimCommand::Bullet(BulletSpawn)` for relayed `createNet` events (fireballs, mass-driver bolts if 08 chooses, lightning in space);
  2. turret building state fields (revision bytes) inside the building chunk — no separate table;
  3. `Sim::checksum()` coverage for `Bullet`, `Fire`, `Puddle`, `TurretState` variables (05 hasher walks registered defs and `FieldKind::Plain` fields);
  4. desync rule: bullets are never snapshotted, so peers must agree on relay ordering of `createNet` commands and reset `CombatRng` deterministically.

### 3.14 Cross-plan interface ledger (reconcile by filename)

| Sibling plan/file | Interface this plan requires | Status at plan-write time |
|---|---|---|
| `02_CONTENT...` `content/registries/bullets.rs` / `BulletDef` | All behavior fields in §6.1 present; `BulletKind` enum with one variant per Java class (list in §2.1); `shoot_pattern: Option<ShootPatternSpec>`, `spawn_bullets`, `lightning_type: Option<BulletId>`, `parts` specs | Present; field-gap addendum in §6.1 |
| `02` `content/registries/units/weapon.rs` / `WeaponDef` | `kind: WeaponKind`, `mount_kind`, `shoot: ShootPatternSpec`, all fields in §3.7 | Present |
| `02` `StatusEffect` | `StatusId`, transition tables; `StatusApply` impl in 11 | Present |
| `05` `entities/comp/bullet.rs` (placeholder) | **10 takes ownership**; file becomes `combat/bullet/mod.rs`; 05 keeps only `Hitbox`/physics accessors it needs | Needs file-ownership hand-off note in 05 changelog |
| `05` `entities/group.rs` / `Groups` | `Groups.bullet.intersect`, stable iteration, `EntityPool`, `EntityDefs!` registration entry for `Bullet`/`Fire`/`Puddle` | Present (05 §3.6/§3.5) |
| `05` `entities/defs.rs` / `io/entity/registry.rs` | Def entries: `Bullet` (`pooled`, `serialize=false`), `Fire` (`pooled`, serialize), `Puddle` (`pooled`, serialize) | Contract defined in 04 R2; 10 supplies components |
| `05` `async_work/physics.rs` | Bullet physics only records last positions; no bullet AI | Present |
| `05` `sim/events.rs` | Combat events/triggers: `BulletCreateEvent`, `UnitDamageEvent`, `UnitBulletDestroyEvent`, `fireCreate/fireExtinguish`, `turretCool`, `flameAmmo`, `resupplyTurret`, `forceProjectorBreak`, exclusion death | Listed; 10 fires them |
| `06_WORLD...` `world/grid.rs` (`Tiles`/`Tile`) | `fire(idx)`, `set_fire`, `puddle(idx)`, `set_puddle`, `flammability()`, `liquid_drop()`, `worldx/worldy`, `World::raycast/raycast_each/no_diagonal`, `build(x,y)`, `build_world` | **06 not written**; contract frozen here |
| `07_BLOCKS...` `world/blocks/behavior.rs` (`BuildingBehavior`) | `update_tile`, `accept_item/handle_item/accept_stack/handle_stack/remove_stack`, `accept_liquid/handle_liquid`, `accept_payload/handle_payload`, `should_consume/can_consume`, `on_placed/on_removed/on_destroyed`, `unit_on`, `sense/control`, `write/read/version`, `draw_state`; `BlockBehaviorRegistry` keyed by `BlockKind`; building spawn inserts kind components | **07 not written**; default fallback in §8 R-10-1 |
| `07` `BlockIndexer` | `each_block(team?, x, y, range, pred, cons)`, `get_enemy(team, BlockFlag)`, `each_block_rect` | **07 not written**; contract frozen here (05 §3.12 says 11 owns `BlockIndexer`; 10 requires it from whichever lands first — orchestrator decision) |
| `08_LOGISTICS...` `MassDriver` | `MassDriverPayload { from, to, items: &[i32], handle_payload(bullet, data), dead() }`; item transport into turrets via 07 modules | **08 not written** |
| `09_POWER...` | `calculate_heat(build, side_heat) -> f32`; coolant consumer efficiency/heat-capacity reads; power status gate | **09 not written** |
| `11_UNITS...` `units/queries.rs` | `TargetQueries` implementation (nearby/best/closest/invalidate/ally-tile), `UnitType::spawn`, `Units::notify_unit_spawn`, `unit.hitbox`, `unit.apply`, `StatusComp`, `ShieldComp`, `unit.move` used by BaseShield/TractorBeam, `BuildPlan` access for BuildWeapon/BuildTurret | **11 not written**; `TargetQueries` trait declared here |
| `12_CAMPAIGN...` `game/rules.rs`, `game/teams.rs` | `rules.fire`, `has_env(Env)`, `damage_explosions`, `unit_damage(team)`, `block_damage(team)`, `unit_health(team)`, `infinite_resources`, `build_cost_multiplier`, `fog`; `Team`/`TeamId`, `TeamData.present` | **12 not written**; minimal surface already promised by 05 §3.12 |
| `13_LOGIC...` | `Senseable`/`Settable`/`ControlBlock` registration for `Bullet`/`TurretBuild` (`LAccess` map by 13; 10 provides sense/control functions) | Not written; functions defined here |
| `16_RENDER...` `Layer` | `Layer::{bullet, turret, turretHeat, shields, blockAdditive, effect, debris}` mapping + draw state consumption | Not written |
| `17_FX...` | `CombatFx` implementation (`effect`, `shake`, `light`, `trail`, `sound`, `sound_loop`, `tether_beam`, `laser`), `DrawPart` for bullets/weapons/turrets | Not written |
| `21_MULTIPLAYER...` | `SimCommand::Bullet`, authority checks, turret field sync metadata, checksum inclusion | Not written |
| `23_PARITY...` | scenario + bench registration, golden files under `parity/combat/` | Continuous |

---

## 4. Port map

Legend: **owner** = plan that ships the code; 10 = this plan.

| Mindustry source | Target Rust module | Notes |
|---|---|---|
| `entities/comp/BulletComp.java` | `combat/bullet/mod.rs` | Component + lifecycle + `tileRaycast` in `raycast.rs`; replaces 05 placeholder. |
| `entities/bullet/BulletType.java` | `combat/bullet/behavior.rs` | Trait + defaults; fields read from 02 `BulletDef`. |
| `entities/bullet/BasicBulletType.java` | `combat/bullet/kinds/basic.rs` | Draw state + shrink/spin. |
| `entities/bullet/ArtilleryBulletType.java` | `kinds/artillery.rs` | Trail view fields. |
| `entities/bullet/MissileBulletType.java` | `kinds/missile.rs` | Defaults only. |
| `entities/bullet/LaserBulletType.java` | `kinds/laser.rs` | Instant `Damage.collideLaser`. |
| `entities/bullet/ContinuousBulletType.java` | `kinds/continuous_laser.rs` (shared helper in `behavior.rs`) | `applyDamage`, `currentLength`. |
| `entities/bullet/ContinuousLaserBulletType.java` | `kinds/continuous_laser.rs` | Segmented draw state. |
| `entities/bullet/ContinuousFlameBulletType.java` | `kinds/continuous_flame.rs` | Flame draw state. |
| `entities/bullet/PointBulletType.java` | `kinds/point.rs` | Teleport + hit in `init`. |
| `entities/bullet/PointLaserBulletType.java` | `kinds/point_laser.rs` | Continuous beam + interval effects. |
| `entities/bullet/ShrapnelBulletType.java` | `kinds/shrapnel.rs` | Serration draw state; instant hit semantics. |
| `entities/bullet/LiquidBulletType.java` | `kinds/liquid.rs` | Puddles/extinguish/status. |
| `entities/bullet/SpaceLiquidBulletType.java` | `kinds/space_liquid.rs` | Space liquid rendering data. |
| `entities/bullet/FireBulletType.java` | `kinds/fire.rs` | Incend on hit. |
| `entities/bullet/LightningBulletType.java` | `kinds/lightning.rs` | Instant branch. |
| `entities/bullet/SapBulletType.java` | `kinds/sap.rs` | Target-finding init + sap damage. |
| `entities/bullet/EmpBulletType.java` | `kinds/emp.rs` | Power/time/unit EMP hit. |
| `entities/bullet/RailBulletType.java` | `kinds/rail.rs` | Pierce/no-falloff + terrain. |
| `entities/bullet/MultiBulletType.java` | `kinds/multi.rs` | Multi-spawn init. |
| `entities/bullet/ExplosionBulletType.java` | `kinds/explosion.rs` | `dynamic_explosion` on hit. |
| `entities/bullet/BombBulletType.java` | `kinds/bomb.rs` | Item-stat explosion. |
| `entities/bullet/InterceptorBulletType.java` | `kinds/interceptor.rs` | Bullet-vs-bullet overlap. |
| `entities/bullet/MassDriverBolt.java` | `kinds/mass_driver.rs` | `MassDriverPayload` trait (08). |
| `entities/bullet/FlakBulletType.java` | `kinds/flak.rs` | Airburst timers. |
| `entities/bullet/LaserBoltBulletType.java` | `kinds/laser_bolt.rs` | Draw only. |
| `entities/bullet/EmptyBulletType.java` | `kinds/empty.rs` | No-op. |
| `content/Bullets.java` | 02 `registries/bullets.rs`; behavior via kinds | `damageLightning` copy flags verified. |
| `entities/Damage.java` | `combat/damage/*.rs` | Full API; `Collided` pool → `SmallVec`. |
| `entities/Lightning.java` | `combat/lightning.rs` | `Rand` → `RngStream::Combat` + monotonic seed. |
| `entities/Fires.java` + `comp/FireComp.java` | `combat/fires.rs` | Tile slot + pooled entity. |
| `entities/Puddles.java` + `comp/PuddleComp.java` | `combat/puddles.rs` | Tile slot + pooled entity; liquid hooks 09. |
| `entities/Predict.java` | `combat/targeting.rs` | All overloads + `quad`. |
| `entities/TargetPriority.java` | `combat/targeting.rs` | Constants. |
| `entities/UnitSorts.java` | `combat/targeting.rs` | `Sortf` becomes `fn(&World, Entity, f32, f32) -> f32`. |
| `type/Weapon.java` | `weapons/mod.rs` | Behavior half; `WeaponDef` metadata 02. |
| `entities/units/WeaponMount.java` (gen) | `weapons/mount.rs` | Struct owned here, stored by 11. |
| `entities/pattern/*.java` | `weapons/pattern.rs` | Kind enum + data in 02; behavior here. |
| `type/weapons/PointDefenseWeapon.java` | `weapons/point_defense_weapon.rs` | Bullet-group targeting. |
| `type/weapons/PointDefenseBulletWeapon.java` | `weapons/point_defense_bullet_weapon.rs` | `Interceptor` data. |
| `type/weapons/RepairBeamWeapon.java` | `weapons/repair_beam_weapon.rs` | `HealBeamMount`. |
| `type/weapons/BuildWeapon.java` | `weapons/build_weapon.rs` | Aim at build plan; beam geometry. |
| `type/weapons/MineWeapon.java` | `weapons/mine_weapon.rs` | Aim at mine tile. |
| `world/blocks/defense/turrets/BaseTurret.java` | `world/blocks/defense/turrets/mod.rs` | Placement overlap/fog init; activation timer. |
| `.../ReloadTurret.java` | `turrets/mod.rs` | Reload/coolant. |
| `.../Turret.java` (+ gen `TurretBuild`) | `turrets/mod.rs` | Core state machine + save v1. |
| `.../ItemTurret.java` | `turrets/item_turret.rs` | Ammo entries + save v2. |
| `.../LiquidTurret.java` | `turrets/liquid_turret.rs` | Liquid ammo + extinguish. |
| `.../PowerTurret.java` | `turrets/power_turret.rs` | Power gate. |
| `.../ContinuousTurret.java` | `turrets/continuous_turret.rs` | Bullet list + save v3. |
| `.../ContinuousLiquidTurret.java` | `turrets/continuous_liquid_turret.rs` | Activation window. |
| `.../PayloadAmmoTurret.java` | `turrets/payload_ammo_turret.rs` | Payload counts (08). |
| `.../LaserTurret.java` | `turrets/laser_turret.rs` | Coolant-driven reload. |
| `.../PointDefenseTurret.java` | `turrets/point_defense_turret.rs` | Bullet interception. |
| `.../TractorBeamTurret.java` | `turrets/tractor_beam_turret.rs` | Pull/status. |
| `.../BuildTurret.java` | `turrets/build_turret.rs` | Proxy unit + plan loop (11/15). |
| `world/blocks/defense/ForceProjector.java` | `world/blocks/defense/force_projector.rs` | Polygon absorb + `ExplosionShield`. |
| `world/blocks/defense/MendProjector.java` | `mend_projector.rs` | Area heal. |
| `world/blocks/defense/ShieldWall.java` | `shield_wall.rs` | Damage interception. |
| `world/blocks/defense/BaseShield.java` | `base_shield.rs` | Shared shield block. |
| `world/blocks/defense/ShockMine.java` | `shock_mine.rs` | `unit_on` trigger. |
| `world/blocks/defense/TargetDummy.java` | `target_dummy.rs` | Test oracle fixture. |
| `world/blocks/ExplosionShield.java` | `combat/damage/shield.rs` | Trait only. |
| `graphics/Layer.java` (combat subset) | consumed by 16; constants documented in `combat/view.rs` | No rendering here. |

---

## 5. Milestones & task breakdown

Each milestone ends with `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, and the named harness command; evidence goes in the Changelog. Order is strict. Owned-test stubs for later plans are `#[ignore = "plan NN"]`.

**M0 — Vertical slice: damage math + one bullet kind.**
- `combat/damage/{mod,area,armor}.rs` (area damage + `apply_armor`), `combat/bullet/{mod,spawn,behavior}.rs`, `kinds/basic.rs`, entity def registration, `CombatPlugin` wiring into `EntitySet::UpdateBullets`/`CollideBullets`.
- Smallest slice: spawn one `fuse` bullet on the plan-05 flat fixture, travel exactly `speed*lifetime`, hit a `target-dummy`-less `stone-wall`, reduce health by `damage * buildingDamageMultiplier`.
- Verify: `cargo test -p mind-core combat::tests::basic_bullet_travel_hits_building`; `mind-headless run combat_basic --seed 7`.

**M1 — Full bullet framework.**
- All bullet fields, pooling, sticky, pierce/pierceCap/pierceDamageFactor, frags/puddles/incend/units/splash/lightning in `hit`, interval/spawn bullets, homing/weave/circle/turn, `create` overloads, `BulletData`, tile raycast (`collideFloor`/`collideTerrain`/`checkUnderBuild`/`collidesTeam`), `despawned`/`removed`.
- Verify: `combat::bullet::tests::{pierce_cap_order, sticky_target_removal, tile_raycast_terrain, frag_counts, just_spawned_no_move, remove_after_pierce}`; `mind-headless run combat_bullet_pierce_frag`.

**M2 — All behavior kinds.**
- Port every `kinds/*.rs` with field-driven behavior; `Damage.collideLine/collide_point/linecast/find_pierce_length/collide_laser/tile_damage/complete_damage/dynamic_explosion/suppression/status`.
- Verify: per-kind tests (`laser_instant_collide`, `liquid_puddle_deposit`, `emp_power_drain`, `rail_no_falloff`, `point_teleport_hit`, `interceptor_bullet_overlap`, `mass_driver_payload_delivery`, `multi_spawn_repeat`, `sap_target_init`, `fire_incend`); `mind-headless run combat_damage_matrix`.

**M3 — Lightning + Fires + Puddles.**
- `combat/lightning.rs`, `combat/fires.rs`, `combat/puddles.rs`; 06 tile-slot contract stub (`TilesStub`) until 06 lands; 09 liquid hooks stubbed behind traits.
- Verify: `combat::lightning::tests::chain_seed_determinism`, `combat::fires::tests::{spread_timer, extinguish, water_speed}`, `combat::puddles::tests::{deposit_cap, reaction_fire, spread_neighbors}`; `mind-headless run combat_fire_puddle_tick`.

**M4 — Weapon engine + patterns.**
- `weapons/{mod,mount,pattern}.rs`; all seven patterns; `flip`/`copy`; fire scheduling via `SimClock::run`; alternate/mirror/side; velocity/life rnd; visual-only weapons.
- Verify: `weapons::pattern::tests::*` (offset/delay/barrel order per pattern), `weapons::tests::{flip_mirrors_geometry, burst_timing, alternate_side_flip, shoot_on_death}`; a unit-less fixture mount fires on a dummy (host unit from 11 fixture or a `TestUnit` spawn from 05).

**M5 — Core turrets (Item/Liquid/Power) + consumers.**
- `TurretBehavior`, `TurretState`, targeting, reload/coolant/heat, ammo stacks, shoot pipeline, save fields; `ItemTurret`, `LiquidTurret`, `PowerTurret`.
- Verify: `world::blocks::defense::turrets::tests::{ammo_swap, ammo_consumption, coolant_reload, power_gate, targeting_priority, cone_gate}`; `mind-headless run combat_turret_ammo`.

**M6 — Advanced turrets.**
- `ContinuousTurret`/`ContinuousLiquidTurret`, `LaserTurret`, `PointDefenseTurret`, `TractorBeamTurret`, `PayloadAmmoTurret`, `BuildTurret`; all turret sense/control hooks.
- Verify: `continuous_bullet_keepalive`, `laser_coolant_reload`, `point_defense_intercept`, `tractor_pull_and_status`, `payload_ammo_consume`, `build_turret_plan_follow`; `mind-headless run combat_point_defense`.

**M7 — Defense blocks + shields.**
- `BaseShield`, `ForceProjector`, `MendProjector`, `ShieldWall`, `ShockMine`, `TargetDummy`, `ExplosionShield`; shield math tests.
- Verify: `force_projector::{polygon_absorb, buildup_break_cooldown, phase_boost, explosion_absorb}`, `shield_wall::overflow_damage`, `mend_projector::heal_loop`, `shock_mine::cooldown`, `target_dummy::dps_counters`; `mind-headless run combat_shield_absorb`.

**M8 — View seams, MCP, inspector.**
- `combat/view.rs` draw states + `CombatFx` trait; `MindSimHost` combat `#[func]`s; inspector `Combat` tab; `scenarios/combat_*.json`.
- Verify: MCP §7c green with screenshot; `godot_log errors` clean; draw-state dump golden.

**M9 — Perf, parity audit, exit.**
- `combat_bench` + `cargo bench -p mind-core --bench combat`; allocation-audit gate; parity ledger `parity/ledgers/combat.md` (every Java file in scope → ported, zero rows unported); §7e checklist green.

---

### 5.1 Milestone status (append-only; updated at each milestone)

- [x] **M0 — Vertical slice: damage math + one bullet kind** (2026-10-02): `combat/{mod,harness,view}.rs`, `combat/damage/{mod,armor,area}.rs`, `combat/bullet/{mod,spawn,behavior,raycast}.rs` + `kinds/{mod,basic}.rs`. `apply_armor`, area/complete damage, `Bullet` component + lifecycle, `fuse`/`rail`/`laser`/… fixture bullets, tile-raycast collision, insertion-ordered harness, `FxSink`/`CombatFx` seam, `mind-headless combat scenario|dump|trace|bench`, golden `tests/golden/combat.json`. Tests: `combat::tests::*` + `tests/combat_golden.rs` (3). Scenarios: `combat_basic` (`377ce4f14c84b621`), `combat_bullet_pierce` (`3e09539ab47e0fd0`), `combat_determinism` (first `076bed2100b7b27c`).
- [x] **M1 — Full bullet framework** (2026-10-02): `CombatCtx` borrow bundle; sticky attach/offset/drop; `createFrags` seeded spread; `createSplashDamage` (enemy-only); interval + spawn/child plumbing; `despawned`/`removed`/`hit` pipelines; `collideFloor`/`collideTerrain`/out-of-bounds stop; `pierceCap`/`removeAfterPierce` order. Fixtures `frag`/`sticky`/`terrain`/`interval`/`splash`. Tests: `pierce_damages_each_wall_in_order`, `remove_after_pierce_cap_order`, `frag_bullets_spawn_on_hit`, `terrain_bullet_stops_at_wall`, `sticky_bullet_removed_when_target_removed`. Goldens unchanged.
- [x] **M2 — All behavior kinds** (2026-10-02): appended `BulletKind` variants `Continuous`/`Multi`/`Point`/`PointLaser`/`ContinuousFlame`/`Interceptor`/`MassDriver`/`Empty` (ids 18–25) and the §6.1 `BulletDef`/`BulletSpec` fields `angleOffset`/`randomAngleOffset`/`createChance`/`ignoreSpawnAngle`/`velocityScaleRand{Min,Max}`/`lifeScaleRand{Min,Max}`/`maxDamageFraction`/`sticky`/`stickyExtraLifetime`/`targetBlocks`/`targetMissiles`/`multiRepeat` (append-only; `spawnBullets` already present). Spawn now applies angle offsets, create chance, ignore-spawn-angle and velocity/life randomization; `!collides` bullets no longer hit buildings; `def.sticky` attaches on first hit. Behavior dispatch ported to kind modules: `point` (teleport+endpoint hit), `multi` (child spawns ×repeat), `emp` (radius power/unit damage + status), `flak` (airburst countdown), `sap` (instant linecast hit), `shrapnel` (instant `collideLine`), `interceptor` (bullet-vs-bullet overlap), `mass_driver` (`MassDriverPayload` trait for 08), `continuous` (line damage each `damageInterval`), `laser`, `empty`; plus `combat/lightning.rs` chain (M3 pulled forward) and `damage/status.rs` (`StatusApply`/`StatusApplier` resource, `status_area`). New fixtures `point`/`multi`/`emp`/`flak`/`sap`/`shrapnel`/`interceptor`/`mass_driver`/`continuous`/`empty`; scenario `combat_damage_matrix` (`a78f7c27762a45e8`). Tests: `point_bullet_teleports_and_hits`, `multi_spawns_children`, `emp_area_damages_neighbor_building`, `flak_airburst_explodes`, `sap_instant_line_hit`, `shrapnel_instant_line_damages_all`, `interceptor_removes_target_bullet`, `continuous_line_damages_each_interval`, `empty_bullet_ignores_buildings`, `lightning::{chain_is_deterministic_for_same_seed, chain_stops_after_max_links}`, `status::apply_status_routes_through_resource`. Evidence: `cargo test -p mind-core` **683 lib + 3 `blocks_golden` + 3 `combat_golden` + 2 + 2 + 1** pass; fmt/clippy clean; M0/M1 goldens unchanged (`377ce4f14c84b621`, `3e09539ab47e0fd0`). **Not yet:** `Lightning`/`Liquid` kind gameplay hooks that need M3 `Fires`/`Puddles`; `MassDriverPayload` delivery is a no-op until plan 08 installs its sink; status transitions/immunities remain plan 11.
- [ ] **M3 — Lightning + Fires + Puddles** *(2026-10-02: `combat/lightning.rs` chain + monotonic `LightningCounter` landed with M2 (pulled forward); `combat/fires.rs` (`FireState`, `Fires::{create,has,find_at,extinguish,remove}`, `CombatEnv` gate, `update_fires` with water-attribute speed, spread to `d4`, tile damage; view via `FxSink`) and `combat/puddles.rs` (`PuddleState`, `Puddles::{deposit,find_at,has_liquid,remove}`, `update_puddles` with viscosity decay, `maxLiquid/1.5` neighbor spread, cap/evaporate; `Tiles::{clear_fire_slots,clear_puddle_slots}` slot cache). `Liquid` kind deposits/extinguishes, `Fire` kind trails fire, checksum covers fires/puddles by tile. Scenario `combat_fire_puddle_tick` → `1a92177b71b624fd`; tests `fires::{create_refresh_and_extinguish, water_attribute_speeds_up_burn, spread_timer_ignites_neighbor_with_flammable_puddle}`, `puddles::{deposit_caps_at_max_liquid, gaseous_liquid_does_not_pool, spread_neighbors_above_threshold, zero_amount_evaporates}`, `lightning::{chain_is_deterministic_for_same_seed, chain_stops_after_max_links}`. **Remaining:** `willBoil`/heat-env, `Tile.getFlammability` floor attrs, liquid `react`/floor liquid-drop reactions and `liquid.update` are plan 06/09/12 stubs; fireball `createNet` emission waits on plan 21.)*
- [x] **M4 — Weapon engine + patterns** (2026-10-02): `weapons/mod.rs` (`Weapon` update/shoot/bullet/flip + `WeaponMount` engine + auto-target/rotation/cone/side-alternate/velocity gates), `weapons/mount.rs` (`WeaponMount`/`HealBeamMount`), `weapons/pattern.rs` (`ShootPattern` trait + all 7 subclass algorithms: single, alternate, barrel, helix, multi, sine, spread, summon), visual-only `build_weapon.rs`/`mine_weapon.rs`, `point_defense_weapon.rs`/`point_defense_bullet_weapon.rs` (target selection + interception), `repair_beam_weapon.rs` (`HealBeamMount` heal loop); `UnitWeapons`/`UnitState` fixture storage + `update_weapons`. New scenario `combat_weapon_volley` → `8bb803a5e97dc72b`. Tests: `pattern::tests::*` (8), `weapons::tests::{mirror_pass_doubles_reload_and_links_sides, flip_mirrors_geometry, reload_decays_then_fires, burst_timing_queues_delays, min_max_shoot_velocity_gate, shoot_on_death_gate_stops_after_first, alternate_side_flip_toggles_mounts, interpolate_life_scale_clamps_to_aim}`. `WeaponDef`/`WeaponSpec`/`ShootPatternSpec` extended append-only in plan 02 (`weapons/mod.rs` metadata half). Unit-side mounting/controllers remain plan 11; draw parts remain plan 17.
- [ ] **M5 — Core turrets (Item/Liquid/Power) + consumers** *(2026-10-02: **core engine landed, partial** — `world/blocks/defense/turrets/mod.rs` ports `TurretState` (`rotation`/`reload_counter`/recoil/heat/warmup/charge/ammo/target/counters), `TurretConfig` + `config_for` (resolved side-table), the `BaseTurret`→`ReloadTurret`→`Turret` pipeline (`update_turret`: validate target → warmup → recoil/heat/charge decay → `handle_reload`/`update_cooling` gate on power/heat → activation timer → target scan → turn → cone gate → `update_shooting`/`shoot`), ammo semantics for `ItemTurret` (`accept_item`/`handle_item`/merge/swap/`useAmmo`/`peekAmmo`), `LiquidTurret` (`accept_liquid`/`useAmmo` drains `1/ammoMultiplier`), and `PowerTurret` (power-gated reload, never depletes ammo); plan-10 `register_bullets` ports the `duo` ammo bullets (copper/graphite/silicon) → `Blocks.java` `ammoTypes`. `CombatHarness` gains `spawn_test_turret`/`turret_state`/`update_turrets_only` and drives turrets from `tick`. Scenario `combat_turret_ammo` → `9a62579a5f3d90e5`; tests `turrets::tests::{item_ammo_acceptance_and_merge, ammo_consumption_decrements_total, coolant_speeds_reload, power_gate_blocks_until_powered, cone_gate_delays_first_shot_until_aligned, activation_timer_blocks_shooting}`. **Remaining (M5/M6):** every other vanilla turret ammo map (only `duo` ported), `BuildingBehavior`/`update_buildings` registration + logistics `accept_item` feeding + `TargetQueries` (plan 11) instead of the harness-side scan (plan 10 §3.2 fallback, R-10-1), heat-requirement turrets, save fields/revisions (§6.3), `Continuous`/`Laser`/`PointDefense`/`Tractor`/`PayloadAmmo`/`Build` turrets (M6), and the `stats`/`setBars` hooks.)*
- [ ] **M6 — Advanced turrets**
- [ ] **M7 — Defense blocks + shields**
- [ ] **M8 — View seams, MCP, inspector**
- [ ] **M9 — Perf, parity audit, exit**

## 6. Data & formats

### 6.1 `BulletDef` field ownership / addendum to plan 02

Plan 02 owns `BulletDef`; this plan requires that it carries **every** field of every `BulletType` subclass (162 public fields in `BulletType` plus subclass fields), specifically including ones easy to miss: `lifeScaleRandMin/Max`, `velocityScaleRandMin/Max`, `angleOffset`, `randomAngleOffset`, `maxDamageFraction`, `optimalLifeFract`, `killShooter`, `instantDisappear`, `pierceFragCap`, `delayFrags`, `intervalDelay`, `underwater`, `healColor`, `healEffect`, `healSound/Volume`, `spawnBulletRandomSpread`, `spawnBullets`, `shootPattern`, `spawnUnit`, `despawnUnit{Chance,Count,Radius,faceOutwards}`, `circleShooter*`, `trail*` (all 14), `homing*`, `followAimSpeed`, `suppression*`, `lightning*`, `lightningType`, `weave*`, `rotateSpeed`, `puddles/puddle*`, `splashDamagePierce`, `incend*`, `blockArmorMultiplier`, `shieldDamageMultiplier`, `statLiquidConsumed`, `light*`, `parts`, `targetBlocks`, `targetMissiles`, `collideFloor`, `collideTerrain`, `scaleKeepVelocity`, `hitSound*`, `despawnSound`, `recoil`, `impact`, `ammoMultiplier`, `reloadMultiplier`. `BulletKind` must have exactly the 26 variants in §2.1 plus `Base`; ordering is ABI (serialized by 20) and frozen at first release.
`ShootPatternSpec` (02 owns data) must be a tagged union matching `weapons/pattern.rs` kinds.

### 6.2 Entity def registration (plan 05/04 table additions)

```text
Bullet  = [BaseEntity, SimId, DefId, Pos, Vel, TeamComp, OwnerRef, ShooterRef, Healthish?, TimeComp, Bullet]  # pooled=true, serialize=false, genio=false
Fire    = [BaseEntity, SimId, DefId, Pos, TimeComp, SyncState, FireState]                                    # pooled=true, serialize=true
Puddle  = [BaseEntity, SimId, DefId, Pos, SyncState, PuddleState]                                            # pooled=true, serialize=true
```
`Healthish` is not added: bullets keep `damage` in `Bullet`; `Shielder` fields (shield/armor) are not needed for vanilla bullets (`BulletType.shieldDamageMultiplier` is read from the def) but are reserved as a `BulletShield` component if a mod bullet ever sets a shield (upstream `BulletComp` implements `Shielderc`; ship the component now to preserve `Shieldc` queries).

### 6.3 Turret save fields (revisioned building chunks, 04 manifests)

| Def name pattern | Version | Fields (append-only) |
|---|---|---|
| `TurretBuild` (all turrets) | 1 | `reloadCounter: f32`, `rotation: f32` |
| `ItemTurretBuild` | 2 | + `ammo_count: u8`, per entry `item: u16`, `amount: i16` (revision < 2 reads `u8` item) |
| `ContinuousTurretBuild` | 3 | + `lastLength: f32` |
| `LaserTurretBuild` | inherits 3 | (none) |
| `PointDefenseBuild`/`TractorBeamBuild` | inherits 1 | `rotation` only |
| `BuildTurretBuild` | 1 | + `rotation`, `TypeIO.writePlans(plans)` |
| `ForceBuild` | 1 | `broken: bool`, `buildup`, `radscl`, `warmup`, `phaseHeat` |
| `MendBuild` | 1 | `heat`, `phaseHeat` |
| `ShieldWallBuild` | 1 | `shield` (`shieldRadius=1` on read if `shield>0`) |
| `BaseShieldBuild` | 1 | `smoothRadius`, `broken` |
| `TargetDummyBuild` | 1 | `unit_id: i32`, `boosting: bool`, `unitArmor`, `resetTime`, `dummySize`, `unitTeam: u8` |
| `Fire` | 1 | `tile`, `time`, `lifetime` (pooled, `afterRead` re-registers) |
| `Puddle` | 1 | `amount`, `tile`, `liquid` (pooled, `afterRead` re-registers) |

All reads are revision-tolerant exactly like upstream (missing fields keep defaults); manifests are written by 04’s `check-revisions --update` and never edited.

### 6.4 Wire/checksum shapes

- `BulletSpawn` is a runtime struct only; the relay encoding is plan 21’s; field order fixed as declared in §3.5. Bullets are excluded from saves/snapshots; checksum covers live bullets per 05’s hasher (def id, pos, vel, time, lifetime, damage, team, collided ids in order).
- `Fire`/`Puddle` are saved per §6.3 and included in checksums.
- `TurretState` participates in building chunks and checksums by field order in §6.3.

### 6.5 Parity artifacts

- `parity/combat/duo_dummy_worksheet.json` — tick-by-tick shot cadence, per-shot damage, dummy HP and total damage from the Java reference for the §7c scenario (generated once; committed).
- `parity/ledgers/combat.md` — one row per Java file in the §4 table with ported checkbox and wave.
- `tests/golden/combat_*.json` — deterministic state dumps from §7b.

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (`tests/src/test/java` → `cargo test`)

The upstream test tree contains **no bullet/turret/damage behavior tests** (`ApplicationTests` has power/logistics/save tests; grep for `turret|bullet|damage|shield|puddle|weapon|projector` finds only `DataAssetTests.basicUnit`, `PatcherTests.unitWeapons`/`addWeapon`/turret ammo patching, and `ModTestAllure`). The port therefore uses these as metadata/interface oracles and defines new behavior tests:

| Upstream test | Rust test (crate `mind-core`) | Notes |
|---|---|---|
| `ApplicationTests.allBlockTest` (turret metadata half) | `world::blocks::defense::turrets::tests::turret_metadata_valid` | `ItemTurret.ammoTypes` non-empty, `PowerTurret.shootType` present, `LiquidTurret.ammoTypes` non-empty, sizes/consumers consistent; behavior half here. |
| `DataAssetTests.basicUnit` (bullet class parsing) | `combat::bullet::tests::laser_def_parses_fields` | `LaserBulletType.length == 1000`; owned end-to-end by 20, fixture lives here. |
| `PatcherTests.unitWeapons` / `uUnitWeaponReassign` / turret ammo patch | `weapons::tests::patcher_kind_and_field_resolution` | Executed under plan 20; 10 exposes kind resolution + reset semantics. |
| `ApplicationTests.buildingDestruction` (ordering half) | `combat::tests::entity_order_bullets_after_buildings` | Full behavior plans 07/10/11; asserts set order only. |
| (new) | `combat::bullet::tests::{basic_bullet_travel_hits_building, pierce_cap_order, sticky_target_removal, tile_raycast_terrain, remove_after_pierce, simple_bullet_despawn_effect_hook}` | Core lifecycle. |
| (new) | `combat::damage::tests::{area_falloff_and_armor, max_damage_fraction, complete_damage_grid, tile_damage_multiblock_compensation, collide_line_pierce_sorted, suppression_scaling, explosion_shield_absorb}` | Damage math. |
| (new) | `combat::puddles::tests::{deposit_cap, reaction_fire, spread_neighbors, boil_vapor_no_puddle}`, `combat::fires::tests::{spread_timer, extinguish, water_speed_multiplier, oxygen_gate}` | Tile entities. |
| (new) | `combat::lightning::tests::{chain_order_seeded, insulation_snap, max_chain_and_pierce_cap}` | Chain. |
| (new) | `world::blocks::defense::turrets::tests::{ammo_swap_peek, item_ammo_multiplier, liquid_ammo_consume, power_never_empty, coolant_reload_boost, heat_efficiency_multiplier, activation_timer, tracking_range, lens_of_cone, charged_first_shot_delay, recoil_pattern}` | Turrets. |
| (new) | `weapons::pattern::tests::{single_delays, alternate_barrel_order, spread_centering, multi_composition, helix_signs, sine_offsets, barrel_cycle, summon_radius}` | Patterns. |
| (new) | `weapons::tests::{flip_mirrors_geometry, shoot_on_death, min_max_shoot_velocity, interpolate_life_scale}` | Weapon engine. |
| (new) | `world::blocks::defense::tests::{force_projector_polygon_absorb, force_projector_break_cycle, shield_wall_overflow, mend_projector_heal, shock_mine_trigger, target_dummy_counters}` | Defense. |

Assertions use exact float comparisons where upstream arithmetic is deterministic (`applyArmor`, falloff, reload counters), epsilon `1e-5` for trig-derived positions.

### 7b. Headless harness scenarios (`mind-headless`)

Registered under `mind-headless/src/scenarios/combat_*.rs`; all run on the plan-05 flat 128×128 fixture unless stated:

1. `run combat_basic --seed 7 --ticks 120 --dump out/combat_basic.json` — one `fuse` bullet from (10,10) angle 0 at speed 4, drag 0: death tile exactly `(10 + speed*lifetime/8, 10)`; building HP reduced by `damage`; checksum == `tests/golden/combat_basic.checksum`.
2. `run combat_bullet_pierce_frag --seed 11 --ticks 600` — `rail` through three dummy walls: all three damaged in order, no falloff; `frag` bullet spawns exactly `fragBullets` children with seeded spread; `pierceDamageFactor` pattern verified; golden JSON.
3. `run combat_turret_ammo --seed 13 --ticks 1200` — `duo` with 30 copper fires exactly `30 / ammoPerShot` shots on cadence `reload`; `totalAmmo` accounting matches; `ItemTurret.ammoMultiplier` for silicon entries; `liquid turret` drains `1/ammoMultiplier` per shot; `power turret` shoots with power ≥ requirement and never depletes ammo.
4. `run combat_point_defense --seed 17 --ticks 300` — `point-defense` intercepts each spawned enemy bullet after `retargetTime`; bullet removed; `bulletDamage` subtracted from bullet damage when not lethal; no friendly interception.
5. `run combat_shield_absorb --seed 19 --ticks 900` — `force-projector` absorbs polygon bullets, `buildup += bullet.type.shieldDamage(b)`, breaks at `shieldHealth`, `cooldownBrokenBase` regen until unbroken, coolant multiplies regen; `shield-wall` splits damage; `dynamicExplosion` absorbed via `ExplosionShield`; golden JSON.
6. `run combat_fire_puddle_tick --seed 23 --ticks 1500` — pyratite deposit on flammable floor → fire created; fire spread timer fires to a `d4` neighbor exactly on schedule; fire damage ticks (`1.8` building / `3` unit burn), water attribute speed multiplier; puddle spread to 4 neighbors above `maxLiquid/1.5`, cap at 70, boil → no puddle; golden JSON.
7. `run combat_determinism --seed 29 --ticks 3600 --checksum-every 60 --workers 1` and `--workers 4` — 12 turrets, 200 bullets live, waves of spawned `dagger` fixtures; all checksums identical across runs and worker counts; golden checksums file.
8. `combat dump --bullets-live` / `combat trace --kind laser` — inspector feeds.
9. `mind-headless bench combat --bullets 2000 --turrets 400 --ticks 3600` (see §7d).

### 7c. MCP playtest scenario (open-godot-mcp) — turret + dummy, exact HP numbers

Preconditions: plan 00 rig (`res://scenes/spine.tscn`, `/root/Spine/SimHost`), plan 07 building placement for `duo`/`target-dummy`, plan 08 item injection via `set_ammo`, milestone M8. Node paths/API from §3.12.

1. `godot_health check` → `{ok:true}`.
2. `godot_game play(scene="res://scenes/spine.tscn", frozen=false)`.
3. PID/liveness + reset: `godot_exec eval` returns `{pid: OS.get_process_id(), tick: ...}`; `godot_exec call /root/Spine/SimHost load_scenario ["res://scenarios/combat_duo_dummy.json"]` → `true`; `set_paused [true]`.
4. Setup assertion: `godot_exec call /root/Spine/SimHost combat_stats [64, 64]` → turret `{kind:"item-turret", ammo:30, ammo_type:"copper"}`; `[80, 64]` → dummy `{hp: 1000, max_hp: 1000}`; `godot_exec call /root/Spine/SimHost set_ammo [64, 64, "copper", 30]` → `true`.
5. Fire: `godot_exec call /root/Spine/SimHost set_paused [false]`; `godot_exec call /root/Spine/SimHost step [240]` (4 s = 8 shots at `duo` copper reload 10).
6. Assert numbers: `combat_stats [80,64].hp` equals `1000 - 8 * copper_bullet_damage` from `parity/combat/duo_dummy_worksheet.json`; `combat_counters()` → `shots_fired == 8`, `bullets_created == 8`, `damage_dealt == 8 * dmg`, `bullets_live == 0`; `godot_runtime_state inspect /root/Spine/SimHost` shows same tick.
7. Pierce probe (optional): `spawn_bullet ["fuse", 20, 64, 0, 1]` → `combat_counters().bullets_created` increments; bullet removed after wall hit.
8. Visual evidence: `godot_screenshot game` saved; pixel-diff that the turret and dummy regions changed between pre-shot and post-shot screenshots; attach path to Changelog.
9. Negative check: `set_paused [true]`; `step [600]`; HP unchanged (paused determinism).
10. `godot_log get source=game count=100` → no `panic`/`CombatError`; `godot_game stop`.

### 7d. Performance budget & measurement

Benchmark: `cargo bench -p mind-core --bench combat` (criterion) and `mind-headless bench combat --bullets 2000 --turrets 400 --ticks 3600`; P50/P95 on the dev machine, recorded in the Changelog. Measurement method: plan 05 `TickReport` per-system spans (`UpdateBullets`, `CollideBullets`, `UpdateBuildings::turrets`, `UpdateAll::fires/puddles`) + criterion + the plan-00 allocation counter.

| Metric | Budget |
|---|---|
| 2,000 live traveling bullets + 400 firing turrets | `UpdateBullets` + `CollideBullets` + turret update ≤ 1.2 ms P95 total; sim tick ≤ 4 ms overall (HLP §7.4) |
| 10,000 live bullets (stress), drag+homing+weave | bullet update ≤ 2.5 ms P95 |
| 200 turrets idle (no target) | ≤ 0.15 ms P95 |
| `Damage.tileDamage` radius 32 (~200 blocks) | ≤ 0.3 ms P95 |
| `dynamicExplosion` (30 waves, radius 50) | ≤ 0.5 ms P95 |
| Bullet spawn+despawn churn 1,000/s | ≤ 10 µs/spawn average, pooled, zero `Vec` growth after warmup |
| Steady-state allocations in combat sets | 0 bytes/tick (allocation counter delta, 3,600-tick soak) |
| Chain lightning 8 links | ≤ 5 µs each |

CI treats budgets as recording-only until plan 23 wires hard gates; regressions block the P4 gate.

### 7e. Exit criteria checklist

- [ ] All §7a tests green under `cargo test -p mind-core` (no Godot, no network).
- [ ] All §7b scenarios pass with committed golden dumps/checksums; `--workers 1` == `--workers 4`.
- [ ] §7c MCP scenario passes with exact HP/damage numbers and a screenshot attached to the Changelog.
- [ ] §7d budgets measured and recorded; allocation counter shows zero steady-state bytes.
- [ ] Every Java file in §4 has a `parity/ledgers/combat.md` row marked ported (or an explicit hand-off note).
- [ ] `BulletDef`/`WeaponDef`/`ShootPatternSpec` gap addendum (§6.1) merged into 02 with no missing field.
- [ ] `TargetQueries`, `BuildingBehavior`, `BlockIndexer`, `Tiles` fire/puddle slots, heat/power hooks, `MassDriverPayload` interfaces reconciled with 06/07/08/09/11 (or stubs clearly marked).
- [ ] Turret/Fire/Puddle revisions committed (04 manifests); save/load round-trips a turret with ammo and a live fire/puddle.
- [ ] `cargo tree -p mind-core` still shows no `godot`/`tokio`; GPL header on every ported file; clippy/rustfmt clean.
- [ ] BlastBullets2D decision recorded (default: not adopted; sim stays in Rust).

---

## 8. Risks & open decisions

| # | Item | Default taken | Flag |
|---|---|---|---|
| OD-10-A | Cross-platform float determinism of transcendental functions used in combat math (trig, `pow`, `sqrt`), required by HLP §2.4 “determinism across platforms for lockstep command replay”. Rust `std` forwards to platform libm, which is not bit-identical everywhere. | All sim math routes through `mind_core::math` wrappers backed by the pure-Rust `libm` crate (same algorithm on every target); view/interpolation may use `std`. Revisit only if a platform’s `libm` deviates in CI (23 adds a cross-platform checksum job). | **NEEDS USER DECISION** (default continues) |
| R-10-1 | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` does not exist yet; building spawn, consumers, `BuildingBehavior`, `BlockIndexer` interfaces are assumptions. | Interface contract frozen in §3.14; fallback `UpdateTurrets`/post-spawn `Added<Building>` query used until 07’s table exists. | Reconcile with 07 at 07-plan review |
| R-10-2 | `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` missing: `calculateHeat`, coolant efficiency, power status. | Trait stubs (`HeatHook`, `CoolantHook`) with unit tests; 09 swaps in real impls. | Reconcile with 09 |
| R-10-3 | `BlockIndexer` ownership is ambiguous (05 §3.12 says 11; 07/09 also need it). | 10 consumes `BlockIndexer` from whichever lands first; if 11 is late, a minimal `DamageBlockIndex` (grid scan) implementation ships in 07/10 behind the same trait. | Orchestrator |
| R-10-4 | Bullet behavior via kind dispatch cannot represent every future mod method override (OD1 script mods). | Vanilla has zero method overrides (verified); `behavior_override: Option<BehaviorId>` reserved; scripting engine decision deferred to 20/OD1. | Reconcile with 02/20 |
| R-10-5 | `MassDriverBolt` needs `DriverBulletData` owned by 08; using 08 types from 10 creates a dependency cycle (10 < 11, 08 independent). | `MassDriverPayload` trait in `combat/bullet/kinds/mass_driver.rs`; 08 implements it on its `MassDriverBuild`. | Reconcile with 08 |
| R-10-6 | `Damage.tileDamage` accumulates into an `IntFloatMap` then iterates; Java hash order leaks into damage application order for overlapping rays. | Rust accumulates into a `SmallVec<(Point2, f32)>` in first-touch order and applies in that order, so Rust↔Rust replay is stable; exact Java order is not required (HLP §9). | No user needed |
| R-10-7 | `TurretBuild.unit` proxy (`BlockUnitc`) player-control feature crosses 10/11/13/15. | Turret control surface (`control`/`sense`/`controlled`) ships in 10; proxy unit component + input wiring in 11/15; 13 binds mlog. Player entering turrets is verified by the P5 gate, not here. | Orchestrator |
| R-10-8 | `TargetQueries` (11) required by damage/turrets before 11 lands. | Trait + `FixtureTargets` implementation in 10 tests; 11 provides the ECS impl. | Reconcile with 11 |
| R-10-9 | BlastBullets2D addon (HLP §8) | Not adopted; bullets drawn from `BulletDrawState` by 16/17 (MultiMesh/sprite batch); sim stays Rust. Decision recorded in `THIRD_PARTY_NOTICES.md` if reversed. | OD6 (no user) |
| R-10-10 | `TargetDummy` is a test fixture but a real vanilla block; if 07 classifies it as non-gameplay its behavior still ships here. | Ship behavior in 10; mark kind `test_fixture` in dev builds only if 07 requires. | Reconcile with 07 |
| R-10-11 | ShockMine `update=false` + `destructible=true` (Building exists without per-tick update). | `unit_on` hook drives behavior; no `update_tile` registration; 07 must still create the building. | Reconcile with 07 |
| R-10-12 | `SyncField` metadata for bullets (`Bullet` not synced; turret fields synced through building snapshots). | Bullet def `serialize=false, sync=false`; turret fields exposed via §6.3 revisions; 21 decides snapshot contents. | Reconcile with 21 |

---

## 9. References

### Mindustry sources read

- `core/src/mindustry/entities/AGENTS.md` (full)
- `core/src/mindustry/world/blocks/AGENTS.md` (full)
- `core/src/mindustry/content/AGENTS.md` (full)
- `core/src/mindustry/type/AGENTS.md` (full)
- `core/src/mindustry/graphics/AGENTS.md` (full)
- `core/src/mindustry/entities/{Damage,Fires,Puddles,Lightning,Predict,TargetPriority,UnitSorts,EntityCollisions}.java`
- `core/src/mindustry/entities/comp/{BulletComp,FireComp,PuddleComp,ShieldComp,HealthComp,DamageComp}.java`
- `core/src/mindustry/entities/bullet/*.java` (all 27)
- `core/src/mindustry/entities/pattern/*.java` (all 8)
- `core/src/mindustry/type/Weapon.java`, `core/src/mindustry/type/weapons/*.java` (all 5)
- `core/src/mindustry/world/blocks/defense/turrets/*.java` (all 12)
- `core/src/mindustry/world/blocks/defense/{BaseShield,ForceProjector,MendProjector,ShieldWall,ShockMine,TargetDummy}.java`
- `core/src/mindustry/world/blocks/ExplosionShield.java`
- `core/src/mindustry/content/Bullets.java`
- `tests/src/test/java/{ApplicationTests,DataAssetTests,PatcherTests}.java` (grep + targeted reads)

### Project plans read

- `HIGH_LEVEL_PLAN.md` (§0, §2, §3 plan 10 row, §4, §6–§9)
- `PRELIMINARY_PLAN.md`
- `02_CONTENT_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`, `05_SIM_CORE_IMPLEMENTATION_PLAN.md`
- `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (§3.5 node map, §7c MCP recipe)

## Changelog

- 2026-10-01 — Draft v1 written (this file). No milestones started. Orchestrator notes: plans 07/08/09/11/16/17/21 were not present at write time; §3.14 freezes the interfaces this plan needs from them.
- 2026-10-02 — **M4 COMPLETE + M5 core (partial)** on `lane/10-combat`, resumed from the interrupted WIP commit `0e9df19` (the weapon-engine WIP compiled and its tests passed after one clippy fix; M4 was completed and verified, not restarted). **M4:** `weapons/{mod,mount,pattern}.rs` — `Weapon.update/shoot/bullet/flip` + `WeaponMount`/`HealBeamMount` state, all seven shoot-pattern algorithms (single/alternate/barrel/helix/multi/sine/spread/summon), visual-only `build_weapon`/`mine_weapon`, `point_defense_weapon`/`point_defense_bullet_weapon`, `repair_beam_weapon` (`HealBeamMount` heal loop), `UnitWeapons`/`UnitState` fixture storage + `update_weapons` driven from `CombatHarness`; plan-02 `WeaponSpec`/`ShootPatternSpec` grew append-only. Golden `combat_weapon_volley` → `8bb803a5e97dc72b`. **M5 core:** new `world/blocks/defense/turrets/mod.rs` — `TurretState` + `TurretConfig`/`config_for`, the `BaseTurret`→`ReloadTurret`→`Turret` pipeline (`update_turret`: validate → warmup → recoil/heat/charge decay → `handle_reload`/`update_cooling` (power/heat gated) → activation timer → target scan → turn → cone gate → `update_shooting`/`shoot`) and ammo semantics for `ItemTurret` (`accept_item`/`handle_item` merge+swap+consume), `LiquidTurret` (`accept_liquid`, drain `1/ammoMultiplier`), `PowerTurret` (power-gated, never depletes); plan-10 `register_bullets` ports the `duo` ammo (copper/graphite/silicon) per `Blocks.java` `ammoTypes`; `CombatHarness::{spawn_test_turret,turret_state,update_turrets_only}` drive turrets from `tick`. Golden `combat_turret_ammo` → `9a62579a5f3d90e5`. Evidence: `cargo test -p mind-core` **747 lib + 3 `blocks_golden` + 5 `combat_golden` + 2 `sim_core_determinism` + 2 `sim_core_meta` + 1 `sim_core_schedule` = 760 passed / 2 ignored**; `cargo test -p mind-headless` **34 passed**; fmt + workspace clippy `-D warnings` clean; `cargo check -p mind-gdext` clean; all pre-existing combat scenarios pass with unchanged checksums (`377ce4f14c84b621`, `3e09539ab47e0fd0`, `076bed2100b7b27c`, `a78f7c27762a45e8`, `1a92177b71b624fd`). **M5 remaining:** the rest of the vanilla turret ammo content (only `duo` ported), `BuildingBehavior`/`update_buildings` + logistics `accept_item` + plan-11 `TargetQueries` integration (this slice uses the §3.2/R-10-1 harness fallback because plan 07's `update_tile` has no content handle), heat-requirement turrets, §6.3 save fields/revisions; **M6** advanced turrets, **M7** defense blocks, **M8** MCP/view, **M9** perf remain open.
- 2026-10-02 — **M3 complete** on `lane/10-combat`. Ported `combat/fires.rs` (`FireState` + `Fires::{create,has,find_at,extinguish,remove}`, `CombatEnv{fire,oxygen}` gate, `update_fires`: water-attribute speed multiplier, flammable-floor `d4` spread, `tileDamage 1.8` every `damageDelay`, tile-slot cache sync, `FxSink` fire-remove hook), `combat/puddles.rs` (`PuddleState` + `Puddles::{deposit,find_at,has_liquid,remove}`, `update_puddles`: viscosity decay, `maxLiquid/1.5` spread to empty `d4` neighbors, cap/evaporate, gas rejection), and `Tiles::{clear_fire_slots,clear_puddle_slots}`. Wired `Liquid` kind (`hit` deposits + extinguishes cold liquids) and `Fire` kind (`update` trails fire); checksum now folds fires/puddles by tile order. New scenario `combat_fire_puddle_tick` → `1a92177b71b624fd` (`spread_neighbors:4`). Tests: `fires::{create_refresh_and_extinguish, water_attribute_speeds_up_burn, spread_timer_ignites_neighbor_with_flammable_puddle}`, `puddles::{deposit_caps_at_max_liquid, gaseous_liquid_does_not_pool, spread_neighbors_above_threshold, zero_amount_evaporates}` (plus the M2-pulled `lightning` chain tests). Evidence: `cargo test -p mind-core` **690 lib + 3 `blocks_golden` + 3 `combat_golden` + 5 sim** pass; fmt + workspace clippy `-D warnings` clean; M0/M1 goldens and `combat_damage_matrix` unchanged. **Reconciliation/stubs:** `Liquid::willBoil`, `Tile.getFlammability` floor attributes and `Liquid::react`/floor liquid-drop reactions remain plan 06/09/12 stubs; fireball `createNet` waits on plan 21; `liquid.update` (plan 09) is not invoked.
- 2026-10-02 — **M2 complete + plan-02 gap reconciled** on `lane/10-combat`. Appended `BulletKind::{Continuous,Multi,Point,PointLaser,ContinuousFlame,Interceptor,MassDriver,Empty}` (ids 18–25, never reordered) and the §6.1 fields `angleOffset`/`randomAngleOffset`/`createChance`/`ignoreSpawnAngle`/`velocityScaleRand{Min,Max}`/`lifeScaleRand{Min,Max}`/`maxDamageFraction`/`sticky`/`stickyExtraLifetime`/`targetBlocks`/`targetMissiles`/`multiRepeat` to `BulletDef` + `BulletSpec` + `from_spec`; spawn applies offsets/chance/randomization and `def.sticky` attaches on first hit; `!collides` bullets no longer hit buildings (fixed `rail` fixture `collides=true` for M1 semantics). Behavior dispatch moved to `kinds/{point,multi,emp,flak,sap,shrapnel,interceptor,mass_driver,continuous,laser,empty,liquid,fire,lightning}.rs` behind `behavior_for`; `Damage` gained `status.rs` (`StatusApply`/`StatusApplier` resource + `status_area`) and `lightning.rs` (chain, monotonic `LightningCounter`; pulled forward from M3). Fixtures + `combat_damage_matrix` scenario (`a78f7c27762a45e8`). Evidence: `cargo test -p mind-core` **683 lib + 3 `blocks_golden` + 3 `combat_golden` + 5 sim** pass; fmt + workspace clippy `-D warnings` clean; `cargo check -p mind-gdext` clean; M0/M1 goldens unchanged. **Remaining:** `Liquid`/`Lightning` kind gameplay needs M3 `Fires`/`Puddles`; `MassDriverPayload` delivery waits on plan 08's sink; status transitions/immunities are plan 11.
- 2026-10-02 — **M2 partial** on `lane/10-combat`: added `combat/damage/{line,explosion}.rs` (`collide_line`/`linecast`/`find_length`, `dynamic_explosion`/`tile_damage`) and `damage_entity`; wired `apply_kind_init` into `CombatCtx::spawn` (Laser deals its line damage on spawn and is removed; `harness.spawn_def` now routes through `CombatCtx::spawn`). Tests: `collide_line_damages_enemy_in_order`, `linecast_finds_first_building`, `dynamic_explosion_damages_radius`, `tile_damage_is_square`, `laser_instant_collide`. Evidence: `cargo test -p mind-core` **653 lib + 3 `combat_golden`** pass; fmt/clippy clean; M0/M1 goldens unchanged. **Not complete:** `BulletKind` lacks `Multi`/`Point`/`PointLaser`/`ContinuousFlame`/`Interceptor`/`MassDriver`/`Empty` and `BulletDef` lacks `sticky`/`spawnBullet` fields, so those kinds cannot be dispatched yet; status application deferred to plan 11.
- 2026-10-02 — **M1 complete** on `lane/10-combat`: `CombatCtx`, full motion (homing/weave/accel/drag), sticky, frags, splash (enemy-only), interval bullets, `despawned`/`removed`/`hit` effect pipelines, terrain/floor collision, pierce cap ordering. Fixtures `frag`/`sticky`/`terrain`/`interval`/`splash`. Evidence: `cargo test -p mind-core` **648 lib + 3 `combat_golden`** pass; fmt/clippy clean; M0 goldens unchanged (`377ce4f14c84b621`, `3e09539ab47e0fd0`). Still open in M1 scope: puddles/incend/units/suppression/lightning hooks are stubs (M2/M3/11) and `create` overloads are covered by `BulletSpawn` defaults.
- 2026-10-02 — **M0 complete** on `lane/10-combat`. Delivered `mind-core::combat` (harness, view/FX seam, damage armor/area, bullet component/spawn/behavior/raycast, `basic` kind), the `combat` headless subcommand (`scenario|dump|trace|bench`), and `tests/golden/combat.json`. Verification: `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p mind-core` 644 lib tests + 3 `combat_golden` integration tests pass; scenarios `combat_basic`/`combat_bullet_pierce`/`combat_determinism` pass. Notes/gaps: (a) vanilla bullets have no name lookup (upstream bullets are anonymous/weapon-inline), so headless scenarios use named fixture bullets registered on the harness; (b) `BulletDef` lacks the §6.1 fields `angleOffset`, `randomAngleOffset`, `createChance`, `ignoreSpawnAngle`, `velocityScaleRand*`, `lifeScaleRand*` — spawn uses def `speed`/`lifetime` directly and the gap is recorded for plan 02 reconciliation; (c) plan 05's schedule is left untouched (plan 10 owns a host-driven `CombatHarness`/`CombatPlugin` seam) so the P0 golden is unchanged.
