# Ledger — combat, bullets, turrets & defense (plan 10 §4)

> Mechanical audit of every Mindustry source file in plan 10's §4 port map.
> `ported` = the behavior half landed in `mind-core`; `hand-off` = the owning
> sibling plan implemented it (`02`/`05`/`07`/`08`/`09`/`11`); `partial` =
> headless behavior landed with a documented residual. Plan-10 scope is the
> `mind-core` behavior half only — view/FX/audio bodies are plan 16/17/18.
> Wave numbers follow the milestone Changelog (M0–M9).

## Bullet framework

| Mindustry source | Rust target | Status | Wave | Notes |
|---|---|---|---|---|
| `entities/comp/BulletComp.java` | `combat/bullet/mod.rs` | [x] ported | M1 | Component + lifecycle + tile raycast (`raycast.rs`). |
| `entities/bullet/BulletType.java` | `combat/bullet/behavior.rs` | [x] ported | M1 | Trait + defaults; fields read from plan-02 `BulletDef`. |
| `entities/bullet/BasicBulletType.java` | `combat/bullet/kinds/basic.rs` | [x] ported | M1 | |
| `entities/bullet/ArtilleryBulletType.java` | `kinds/artillery.rs` | [x] ported | M2 | |
| `entities/bullet/MissileBulletType.java` | `kinds/missile.rs` | [x] ported | M2 | Defaults only. |
| `entities/bullet/LaserBulletType.java` | `kinds/laser.rs` | [x] ported | M2 | Instant `Damage.collideLaser`. |
| `entities/bullet/ContinuousBulletType.java` | `kinds/continuous_laser.rs` | [x] ported | M2 | Shared helper in `behavior.rs`. |
| `entities/bullet/ContinuousLaserBulletType.java` | `kinds/continuous_laser.rs` | [x] ported | M2 | |
| `entities/bullet/ContinuousFlameBulletType.java` | `kinds/continuous_flame.rs` | [x] ported | M2 | |
| `entities/bullet/PointBulletType.java` | `kinds/point.rs` | [x] ported | M2 | |
| `entities/bullet/PointLaserBulletType.java` | `kinds/point_laser.rs` | [x] ported | M2 | |
| `entities/bullet/ShrapnelBulletType.java` | `kinds/shrapnel.rs` | [x] ported | M2 | |
| `entities/bullet/LiquidBulletType.java` | `kinds/liquid.rs` | [x] ported | M2/M3 | Puddles/extinguish/status. |
| `entities/bullet/SpaceLiquidBulletType.java` | `kinds/space_liquid.rs` | [x] ported | M2 | |
| `entities/bullet/FireBulletType.java` | `kinds/fire.rs` | [x] ported | M2/M3 | Incend on hit. |
| `entities/bullet/LightningBulletType.java` | `kinds/lightning.rs` | [x] ported | M2 | |
| `entities/bullet/SapBulletType.java` | `kinds/sap.rs` | [x] ported | M2 | |
| `entities/bullet/EmpBulletType.java` | `kinds/emp.rs` | [x] ported | M2 | |
| `entities/bullet/RailBulletType.java` | `kinds/rail.rs` | [x] ported | M2 | |
| `entities/bullet/MultiBulletType.java` | `kinds/multi.rs` | [x] ported | M2 | |
| `entities/bullet/ExplosionBulletType.java` | `kinds/explosion.rs` | [x] ported | M2 | |
| `entities/bullet/BombBulletType.java` | `kinds/bomb.rs` | [x] ported | M2 | |
| `entities/bullet/InterceptorBulletType.java` | `kinds/interceptor.rs` | [x] ported | M2 | |
| `entities/bullet/MassDriverBolt.java` | `kinds/mass_driver.rs` | [x] ported | M2 | `MassDriverPayload` trait (08 implements). |
| `entities/bullet/FlakBulletType.java` | `kinds/flak.rs` | [x] ported | M2 | |
| `entities/bullet/LaserBoltBulletType.java` | `kinds/laser_bolt.rs` | [x] ported | M2 | |
| `entities/bullet/EmptyBulletType.java` | `kinds/empty.rs` | [x] ported | M2 | |
| `content/Bullets.java` | plan-02 `registries/bullets.rs` | [x] hand-off | M0–M2 | Metadata half; behavior via kinds. |
| `entities/Damage.java` | `combat/damage/*.rs` | [x] ported | M0–M2 | `Collided` pool → `SmallVec`. |
| `entities/Lightning.java` | `combat/lightning.rs` | [x] ported | M2/M3 | Monotonic seed. |
| `entities/Fires.java` + `comp/FireComp.java` | `combat/fires.rs` | [x] ported | M3 | `willBoil`/heat-env is plan 09. |
| `entities/Puddles.java` + `comp/PuddleComp.java` | `combat/puddles.rs` | [x] ported | M3 | Liquid reactions are plan 06/09. |

## Targeting & weapons

| Mindustry source | Rust target | Status | Wave | Notes |
|---|---|---|---|---|
| `entities/Predict.java` | `combat/targeting.rs` | [~] partial | M9 | Turrets use closest-target only; intercept quadratics still open (aim lead is not applied). |
| `entities/TargetPriority.java` | `content/registries/blocks/mod.rs` + `ai/block_indexer.rs` | [x] hand-off | M9 | Block priority constants/types live with the block metadata + plan-11 indexer. |
| `entities/UnitSorts.java` | — | [~] partial | M9 | `UnitSort` predicates are not yet wired into turret target selection (closest-first). |
| `type/Weapon.java` | `weapons/mod.rs` | [x] ported | M4 | Behavior half; `WeaponDef` is plan 02. |
| `entities/units/WeaponMount.java` (gen) | `weapons/mount.rs` | [x] ported | M4 | Struct owned here. |
| `entities/pattern/*.java` | `weapons/pattern.rs` | [x] ported | M4 | All 8 patterns. |
| `type/weapons/PointDefenseWeapon.java` | `weapons/point_defense_weapon.rs` | [x] ported | M4 | |
| `type/weapons/PointDefenseBulletWeapon.java` | `weapons/point_defense_bullet_weapon.rs` | [x] ported | M4 | |
| `type/weapons/RepairBeamWeapon.java` | `weapons/repair_beam_weapon.rs` | [x] ported | M4 | |
| `type/weapons/BuildWeapon.java` | `weapons/build_weapon.rs` | [x] ported | M4 | |
| `type/weapons/MineWeapon.java` | `weapons/mine_weapon.rs` | [x] ported | M4 | |

## Turrets

| Mindustry source | Rust target | Status | Wave | Notes |
|---|---|---|---|---|
| `.../turrets/BaseTurret.java` | `turrets/mod.rs` | [x] ported | M5 | Activation timer, placement/fog seam. |
| `.../turrets/ReloadTurret.java` | `turrets/mod.rs` | [x] ported | M5 | Reload/coolant. |
| `.../turrets/Turret.java` (+ gen `TurretBuild`) | `turrets/mod.rs` | [x] ported | M5 | Core state machine + save v1. |
| `.../turrets/ItemTurret.java` | `turrets/mod.rs`/`advanced.rs` | [x] ported | M5/M6 | Ammo entries + save v2. |
| `.../turrets/LiquidTurret.java` | `turrets/mod.rs`/`advanced.rs` | [x] ported | M5 | Liquid ammo + extinguish. |
| `.../turrets/PowerTurret.java` | `turrets/mod.rs`/`advanced.rs` | [x] ported | M5 | Power gate. |
| `.../turrets/ContinuousTurret.java` | `turrets/advanced.rs` | [x] ported | M6 | Keep-alive beam + save v3. |
| `.../turrets/ContinuousLiquidTurret.java` | `turrets/advanced.rs` | [~] partial | M6 | Modeled as `Continuous`; the liquid activation window (`liquidConsumed`/`newTargetInterval`) is open. |
| `.../turrets/PayloadAmmoTurret.java` | `turrets/mod.rs` | [x] ported | M6 | Payload ammo stacks + accept/handle/consume/save. |
| `.../turrets/LaserTurret.java` | `turrets/advanced.rs` | [x] ported | M6 | Coolant-driven reload. |
| `.../turrets/PointDefenseTurret.java` | `turrets/advanced.rs` | [x] ported | M6 | Bullet interception. |
| `.../turrets/TractorBeamTurret.java` | `turrets/advanced.rs` | [x] ported | M6 | Pull/status/damage. |
| `.../turrets/BuildTurret.java` | `turrets/mod.rs` | [~] partial | M6 | Plan-following + `ConstructState` progress landed; plan-07 `construct_tick` owns final placement, and proxy-unit mlog sense is plan 13. |

Vanilla ammo coverage (`Blocks.java` `ammo`/`shootType`) — all `ItemTurret`/
`LiquidTurret`/`PowerTurret`/`LaserTurret`/`PointDefenseTurret`/
`TractorBeamTurret` tables are ported except the missile-`spawnUnit` turret
`scathe` (needs plan-11 missile units + `BulletDef.spawn_unit` plumbing) and the
`ContinuousLiquidTurret` `sublimate` activation window (see above).

| Turret | Class | Ammo ported |
|---|---|---|
| `duo`, `scatter`, `scorch`, `hail`, `salvo`, `swarmer`, `fuse`, `ripple`, `cyclone`, `foreshadow`, `spectre`, `breach`, `diffuse` | Item | [x] |
| `titan`, `disperse`, `smite` | Item | [x] (M9/F24) |
| `wave`, `tsunami` | Liquid | [x] |
| `lancer`, `arc`, `afflict`, `malign` | Power | [x] (afflict/malign M9/F24) |
| `meltdown`, `lustre` | Laser/Continuous | [x] (lustre M9/F24) |
| `parallax`, `segment` | Tractor/PointDefense | [x] |
| `scathe` | Item (`spawnUnit`) | [ ] deferred — plan-11 missile units |
| `sublimate` | ContinuousLiquid | [ ] deferred — activation window |

## Defense blocks

| Mindustry source | Rust target | Status | Wave | Notes |
|---|---|---|---|---|
| `world/blocks/defense/ForceProjector.java` | `defense/shields.rs` | [x] ported | M7 | Polygon absorb + `ExplosionShield`. |
| `world/blocks/defense/MendProjector.java` | `defense/shields.rs` | [x] ported | M7 | |
| `world/blocks/defense/ShieldWall.java` | `defense/shields.rs` | [x] ported | M7 | |
| `world/blocks/defense/BaseShield.java` | `defense/shields.rs` | [~] partial | M7 | Shared shield math; unit push-out helpers are plan 11. |
| `world/blocks/defense/ShockMine.java` | `defense/shields.rs` | [x] ported | M7 | |
| `world/blocks/defense/TargetDummy.java` | `defense/shields.rs` | [x] ported | M7 | |
| `world/blocks/ExplosionShield.java` | `combat/damage/shield.rs` | [x] ported | M7 | Trait only. |

## View / seams / hand-offs

| Mindustry source | Rust target | Status | Wave | Notes |
|---|---|---|---|---|
| `graphics/Layer.java` (combat subset) | consumed by 16; documented in `combat/view.rs` | [x] hand-off | M8 | No rendering here. |
| `entities/comp/ShieldComp.java`, `HealthComp`, `DamageComp` | `entities/comp/*` | [x] hand-off | — | Plan 05/11 own the component bodies; 10 reads them. |
| `ai/BlockIndexer.java` | `ai/block_indexer.rs` | [x] hand-off | M9 | Consumed by `combat/targeting.rs` for turret building targets. |
| `ai/UnitGroup`/`Units.java` target queries | `combat/targeting.rs` + plan-11 `ai` | [~] partial | M9 | Unit snapshot + building index; full `UnitSort` predicates open. |
