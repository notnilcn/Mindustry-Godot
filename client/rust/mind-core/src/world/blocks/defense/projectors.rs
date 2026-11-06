// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `OverdriveProjector`, `RegenProjector`, `ShockwaveTower` and `BaseShield`
//! state machines (`world/blocks/defense/{OverdriveProjector,RegenProjector,
//! ShockwaveTower,BaseShield}.java`).
//!
//! These are the M7 projector/projector-adjacent blocks the plan-07
//! [`BuildingBehavior`](crate::world::behavior::BuildingBehavior) tick drives.
//! `BaseShield`'s consumers need bullet metadata (`BulletType.absorbable`) and
//! the view FX seam, supplied through [`BaseShieldEnv`] by the behavior (which
//! borrows them from the live unit runtime).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::combat::bullet::{ABSORBED, Bullet};
use crate::combat::view::FxSink;
use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{HitboxComp, UnitCore};
use crate::entities::comp::{Building, Health, Pos, TeamComp, Vel};
use crate::world::WorldGrid;

fn lerp_delta(from: f32, to: f32, alpha: f32) -> f32 {
    from + (to - from) * alpha.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// OverdriveProjector
// ---------------------------------------------------------------------------

/// `OverdriveBuild` state (`world/blocks/defense/OverdriveProjector.java`).
#[derive(Debug, Clone, Component)]
pub struct OverdriveProjectorState {
    /// `OverdriveProjector.reload`.
    pub reload: f32,
    /// `OverdriveProjector.range`.
    pub range: f32,
    /// `OverdriveProjector.speedBoost`.
    pub speed_boost: f32,
    /// `OverdriveProjector.speedBoostPhase`.
    pub speed_boost_phase: f32,
    /// `OverdriveProjector.useTime`.
    pub use_time: f32,
    /// `OverdriveProjector.phaseRangeBoost`.
    pub phase_range_boost: f32,
    /// `OverdriveBuild.heat`.
    pub heat: f32,
    /// `OverdriveBuild.charge`.
    pub charge: f32,
    /// `OverdriveBuild.phaseHeat`.
    pub phase_heat: f32,
    /// `OverdriveBuild.smoothEfficiency`.
    pub smooth_efficiency: f32,
    /// `OverdriveBuild.useProgress`.
    pub use_progress: f32,
}

impl Default for OverdriveProjectorState {
    fn default() -> Self {
        Self {
            reload: 60.0,
            range: 80.0,
            speed_boost: 1.5,
            speed_boost_phase: 0.75,
            use_time: 400.0,
            phase_range_boost: 20.0,
            heat: 0.0,
            charge: 0.0,
            phase_heat: 0.0,
            smooth_efficiency: 0.0,
            use_progress: 0.0,
        }
    }
}

impl OverdriveProjectorState {
    /// `OverdriveBuild.realBoost()`.
    pub fn real_boost(&self, efficiency: f32) -> f32 {
        (self.speed_boost + self.phase_heat * self.speed_boost_phase) * efficiency
    }
}

/// `BuildingComp.applyBoost` (`BuildingComp.java:467-474`): raise `timeScale`
/// for `duration` ticks, unless the target's block opts out of overdrive.
pub fn apply_boost(world: &mut World, e: Entity, boost: f32, duration: f32) {
    if !can_overdrive(world, e) {
        return;
    }
    if let Some(mut building) = world.get_mut::<Building>(e) {
        // Do not refresh the duration when a lower intensity arrives.
        if boost >= building.time_scale - 0.001 {
            building.time_scale_duration = building.time_scale_duration.max(duration);
        }
        building.time_scale = building.time_scale.max(boost);
    }
}

/// `Block.canOverdrive` of an entity's block (`Block.java:314`).
pub fn can_overdrive(world: &World, e: Entity) -> bool {
    let Some(block) = world.get::<Building>(e).map(|building| building.block) else {
        return false;
    };
    world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(block))
        .map(|inst| inst.def.can_overdrive)
        .unwrap_or(true)
}

/// `OverdriveBuild.updateTile` (`delta == 1`).
pub fn update_overdrive_projector(
    world: &mut World,
    e: Entity,
    state: &mut OverdriveProjectorState,
    efficiency: f32,
    optional_efficiency: f32,
) {
    state.smooth_efficiency = lerp_delta(state.smooth_efficiency, efficiency, 0.08);
    state.heat = lerp_delta(state.heat, if efficiency > 0.0 { 1.0 } else { 0.0 }, 0.08);
    state.charge += state.heat;
    state.phase_heat = lerp_delta(state.phase_heat, optional_efficiency, 0.1);

    if state.charge >= state.reload {
        state.charge = 0.0;
        let real_range = state.range + state.phase_heat * state.phase_range_boost;
        let boost = state.real_boost(efficiency);
        let targets = buildings_in_range(world, e, real_range);
        for target in targets {
            // `OverdriveProjector.java:110`: only `other.block.canOverdrive`
            // blocks are boosted (`applyBoost` re-checks).
            if can_overdrive(world, target) {
                apply_boost(world, target, boost, state.reload + 1.0);
            }
        }
    }

    if efficiency > 0.0 {
        state.use_progress += 1.0;
    }
    if state.use_progress >= state.use_time {
        // `OverdriveProjector.java:118-121`: `consume(); useProgress %= useTime`.
        crate::world::update::trigger_consumers(world, e);
        state.use_progress %= state.use_time;
    }
}

/// `Block.suppressable` (`MendProjector.java:46`, `RegenProjector.java:46`,
/// `RepairTower.java:31`, `BuildTurret.java:46`): the healing blocks whose
/// healing heal-suppression stops.
fn suppressable(world: &World, e: Entity) -> bool {
    use crate::content::BlockKind;
    world
        .get::<Building>(e)
        .and_then(|building| {
            world
                .get_resource::<crate::world::block::BlockTable>()
                .and_then(|table| table.get(building.block))
        })
        .is_some_and(|instance| {
            matches!(
                instance.def.kind,
                BlockKind::MendProjector
                    | BlockKind::RegenProjector
                    | BlockKind::RepairTower
                    | BlockKind::BuildTurret
            )
        })
}

/// `BuildingComp.isHealSuppressed()` (`BuildingComp.java:493-495`):
/// `block.suppressable && Time.time <= healSuppressionTime`. The port's
/// `Time.time` is the [`crate::world::update::BuildClock`]; a building never
/// suppressed keeps `heal_suppression_time <= 0` (Java's `-1` default), which
/// never counts (L06 T3).
pub fn is_heal_suppressed(world: &World, e: Entity) -> bool {
    let Some(time) = world.get::<Building>(e).map(|b| b.heal_suppression_time) else {
        return false;
    };
    time > 0.0 && crate::world::update::build_time(world) <= time && suppressable(world, e)
}

/// `BuildingComp.applyHealSuppression(amount)` (`BuildingComp.java:485-491`):
/// `healSuppressionTime = max(healSuppressionTime, Time.time + amount)`.
pub fn apply_heal_suppression(world: &mut World, e: Entity, amount: f32) {
    let now = crate::world::update::build_time(world);
    if let Some(mut building) = world.get_mut::<Building>(e) {
        building.heal_suppression_time = building.heal_suppression_time.max(now + amount);
    }
}

/// `Damage.applySuppression` (`Damage.java:46-60`), sim half: every
/// non-privileged building of another team with
/// `within(x, y, range + hitSize / 2)` (`BlockIndexer.allBuildings`,
/// `BlockIndexer.java:435-454`) gets `applyHealSuppression(reload + 1)`. The
/// `regenSuppressSeek` particles are view-only (not ported here).
pub fn apply_suppression(world: &mut World, team: u8, x: f32, y: f32, range: f32, reload: f32) {
    let targets: Vec<Entity> = {
        let table = world.get_resource::<crate::world::block::BlockTable>();
        // Each target gets the same idempotent update, so visit order is moot.
        world
            .iter_entities()
            .filter_map(|entity| {
                let building = entity.get::<Building>()?;
                let pos = entity.get::<Pos>()?;
                if entity.get::<TeamComp>().map_or(0, |t| t.team) == team {
                    return None;
                }
                let def = table
                    .and_then(|table| table.get(building.block))
                    .map(|i| &i.def)?;
                if def.privileged {
                    return None;
                }
                let reach = range + def.size as f32 * crate::world::block::TILE_SIZE / 2.0;
                let d2 = (pos.x - x).powi(2) + (pos.y - y).powi(2);
                (d2 <= reach * reach).then_some(entity.id())
            })
            .collect()
    };
    for target in targets {
        apply_heal_suppression(world, target, reload + 1.0);
    }
}

/// Buildings of the origin's team within `range` world units of `origin`
/// (`BlockIndexer.eachBlock(team, wx, wy, range, ...)`,
/// `BlockIndexer.java:328-361`): same team only, `b.within(wx, wy, range +
/// b.hitSize()/2f)`, `privileged` blocks excluded. `range` is world units;
/// `hitSize = size * 8` (T4).
pub fn buildings_in_range(world: &World, origin: Entity, range: f32) -> Vec<Entity> {
    let Some(pos) = world.get::<Pos>(origin).copied() else {
        return Vec::new();
    };
    let Some(team) = world.get::<TeamComp>(origin).map(|team| team.team) else {
        return Vec::new();
    };
    let table = world.get_resource::<crate::world::block::BlockTable>();
    let mut out = Vec::new();
    for entity_ref in world.iter_entities() {
        let Some(building) = entity_ref.get::<Building>() else {
            continue;
        };
        if entity_ref.id() == origin {
            continue;
        }
        let Some(target_pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let same_team = entity_ref
            .get::<TeamComp>()
            .is_some_and(|target_team| target_team.team == team);
        if !same_team {
            continue;
        }
        let Some(inst) = table.and_then(|table| table.get(building.block)) else {
            continue;
        };
        if inst.def.privileged {
            continue;
        }
        let hit_size = inst.def.size.max(1) as f32 * 8.0;
        let reach = range.max(0.0) + hit_size / 2.0;
        let dx = target_pos.x - pos.x;
        let dy = target_pos.y - pos.y;
        if dx * dx + dy * dy <= reach * reach {
            out.push(entity_ref.id());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// RegenProjector
// ---------------------------------------------------------------------------

/// `RegenProjectorBuild` state (`world/blocks/defense/RegenProjector.java`).
#[derive(Debug, Clone, Component)]
pub struct RegenProjectorState {
    /// `RegenProjector.range` (tiles).
    pub range: i32,
    /// `RegenProjector.healPercent` (per frame).
    pub heal_percent: f32,
    /// `RegenProjector.optionalMultiplier`.
    pub optional_multiplier: f32,
    /// `RegenProjector.optionalUseTime`.
    pub optional_use_time: f32,
    /// `RegenProjectorBuild.warmup`.
    pub warmup: f32,
    /// `RegenProjectorBuild.totalTime`.
    pub total_time: f32,
    /// `RegenProjectorBuild.optionalTimer`.
    pub optional_timer: f32,
    /// `RegenProjectorBuild.anyTargets`.
    pub any_targets: bool,
    /// `RegenProjectorBuild.didRegen`.
    pub did_regen: bool,
}

impl Default for RegenProjectorState {
    fn default() -> Self {
        Self {
            range: 14,
            heal_percent: 12.0 / 60.0,
            optional_multiplier: 2.0,
            optional_use_time: 60.0 * 8.0,
            warmup: 0.0,
            total_time: 0.0,
            optional_timer: 0.0,
            any_targets: false,
            did_regen: false,
        }
    }
}

/// `RegenProjectorBuild.updateTile` (`delta == 1`, grid-free target scan).
pub fn update_regen_projector(
    world: &mut World,
    e: Entity,
    state: &mut RegenProjectorState,
    efficiency: f32,
    optional_efficiency: f32,
) {
    state.warmup = if state.did_regen {
        (state.warmup + 1.0 / 70.0).min(1.0)
    } else {
        (state.warmup - 1.0 / 70.0).max(0.0)
    };
    state.total_time += state.warmup;
    state.did_regen = false;
    state.any_targets = false;

    // "no healing when suppressed" (`RegenProjector.java:134-137`).
    if is_heal_suppressed(world, e) {
        return;
    }

    let range = state.range as f32;
    let targets = buildings_in_range(world, e, range);
    let mut any = false;
    for target in &targets {
        if world
            .get::<Health>(*target)
            .is_some_and(|health| health.damaged())
        {
            any = true;
            break;
        }
    }
    state.any_targets = any;

    if efficiency > 0.0 {
        state.optional_timer += optional_efficiency;
        if state.optional_timer >= state.optional_use_time {
            state.optional_timer = 0.0;
        }

        let heal_amount =
            (1.0f32 + (state.optional_multiplier - 1.0) * optional_efficiency) * state.heal_percent;
        for target in targets {
            // `if(!build.damaged() || build.isHealSuppressed()) continue;`
            // (`RegenProjector.java:151`).
            if is_heal_suppressed(world, target) {
                continue;
            }
            let Some(health) = world.get::<Health>(target) else {
                continue;
            };
            if !health.damaged() {
                continue;
            }
            state.did_regen = true;
            let heal = heal_amount / 100.0 * efficiency * health.max_health;
            if let Some(mut health) = world.get_mut::<Health>(target) {
                health.health = (health.health + heal).min(health.max_health);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ShockwaveTower
// ---------------------------------------------------------------------------

/// `ShockwaveTowerBuild` state (`world/blocks/defense/ShockwaveTower.java`).
#[derive(Debug, Clone, Component)]
pub struct ShockwaveTowerState {
    /// `ShockwaveTower.reload`.
    pub reload: f32,
    /// `ShockwaveTower.range`.
    pub range: f32,
    /// `ShockwaveTower.bulletDamage`.
    pub bullet_damage: f32,
    /// `ShockwaveTower.falloffCount`.
    pub falloff_count: f32,
    /// `ShockwaveTower.checkInterval`.
    pub check_interval: f32,
    /// `ShockwaveTowerBuild.reloadCounter`.
    pub reload_counter: f32,
    /// `ShockwaveTowerBuild.heat`.
    pub heat: f32,
    /// `timer(timerCheck, checkInterval)` accumulator.
    pub check_timer: f32,
}

impl Default for ShockwaveTowerState {
    fn default() -> Self {
        Self {
            reload: 60.0 * 1.5,
            range: 110.0,
            bullet_damage: 160.0,
            falloff_count: 20.0,
            check_interval: 8.0,
            reload_counter: 0.0,
            heat: 0.0,
            check_timer: 0.0,
        }
    }
}

/// `ShockwaveTowerBuild.updateTile` (`delta == 1`): damage/despawn nearby enemy
/// bullets in one burst; absorbs `min(bulletDamage, bulletDamage * falloff /
/// targets)` from each.
pub fn update_shockwave_tower(
    world: &mut World,
    e: Entity,
    state: &mut ShockwaveTowerState,
    potential_efficiency: f32,
) {
    if state.check_timer > 0.0 {
        state.check_timer -= 1.0;
    }
    if potential_efficiency > 0.0 {
        state.reload_counter += 1.0;
    }
    if potential_efficiency > 0.0
        && state.reload_counter >= state.reload
        && state.check_timer <= 0.0
    {
        state.check_timer = state.check_interval;
        let Some(pos) = world.get::<Pos>(e).copied() else {
            return;
        };
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        let range2 = (state.range + 1.0).powi(2);
        let mut targets: Vec<Entity> = Vec::new();
        for entity_ref in world.iter_entities() {
            if entity_ref.get::<crate::combat::bullet::Bullet>().is_none() {
                continue;
            }
            if entity_ref.get::<TeamComp>().map(|t| t.team) == Some(team) {
                continue;
            }
            let Some(bullet_pos) = entity_ref.get::<Pos>() else {
                continue;
            };
            let dx = bullet_pos.x - pos.x;
            let dy = bullet_pos.y - pos.y;
            if dx * dx + dy * dy <= range2 {
                targets.push(entity_ref.id());
            }
        }
        if !targets.is_empty() {
            state.heat = 1.0;
            state.reload_counter = 0.0;
            let wave_damage = state
                .bullet_damage
                .min(state.bullet_damage * state.falloff_count / targets.len() as f32);
            for target in targets {
                let remaining = world
                    .get::<crate::combat::bullet::Bullet>(target)
                    .map(|bullet| bullet.damage - wave_damage)
                    .unwrap_or(0.0);
                if remaining > 0.0 {
                    if let Some(mut bullet) = world.get_mut::<crate::combat::bullet::Bullet>(target)
                    {
                        bullet.damage = remaining;
                    }
                } else {
                    let _ = world.despawn(target);
                }
            }
        }
    }
    state.heat = (state.heat - 1.0 / state.reload.max(0.0001)).max(0.0);
}

// ---------------------------------------------------------------------------
// BaseShield
// ---------------------------------------------------------------------------

/// `BaseShieldBuild` state (`world/blocks/defense/BaseShield.java`).
#[derive(Debug, Clone, Component)]
pub struct BaseShieldState {
    /// `BaseShield.radius`.
    pub radius: f32,
    /// `BaseShield.sides`.
    pub sides: i32,
    /// `BaseShieldBuild.smoothRadius`.
    pub smooth_radius: f32,
    /// `BaseShieldBuild.broken`.
    pub broken: bool,
    /// `BaseShieldBuild.hit`.
    pub hit: f32,
}

impl Default for BaseShieldState {
    fn default() -> Self {
        Self {
            radius: 200.0,
            sides: 24,
            smooth_radius: 0.0,
            broken: false,
            hit: 0.0,
        }
    }
}

/// `BaseShieldBuild.radius()`.
pub fn base_shield_radius(state: &BaseShieldState) -> f32 {
    state.smooth_radius
}

/// The live content/grid/FX seam `BaseShield`'s consumers read
/// (`BaseShield.java:27-57`): `bullet.type.absorbable`, tile collisions for
/// `unit.move`, and the `Fx.circleColorSpark` sink.
pub struct BaseShieldEnv<'a> {
    /// Content registry (`BulletDef.absorbable`).
    pub content: &'a ContentRegistry,
    /// Terrain snapshot for `EntityCollisions.move` (`VelComp.move`,
    /// VelComp.java:65-74).
    pub grid: &'a WorldGrid,
    /// View-only effect sink (`Fx.circleColorSpark`).
    pub fx: &'a dyn FxSink,
}

/// `smoothRadius = Mathf.lerpDelta(smoothRadius, radius * efficiency, 0.05f)`
/// (`BaseShield.java:89`). Split out so the plan-07 `BuildHarness` world (no
/// bullet/unit `UnitRuntime`, hence no content registry) still animates the
/// dome.
pub fn update_base_shield_radius(state: &mut BaseShieldState, efficiency: f32) {
    state.smooth_radius = lerp_delta(state.smooth_radius, state.radius * efficiency, 0.05);
}

/// `BaseShieldBuild.updateTile` (`delta == 1`, `BaseShield.java:87-99`): grow
/// toward `radius * efficiency`, absorb absorbable enemy bullets inside the
/// dome, then repel enemy units overlapping it.
pub fn update_base_shield(
    world: &mut World,
    e: Entity,
    state: &mut BaseShieldState,
    efficiency: f32,
    env: &BaseShieldEnv<'_>,
) {
    update_base_shield_radius(state, efficiency);
    let radius = base_shield_radius(state);
    if radius <= 1.0 {
        return;
    }
    let Some(pos) = world.get::<Pos>(e).copied() else {
        return;
    };
    let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
    let center = (pos.x, pos.y);
    absorb_bullets(world, env.content, team, center, radius);
    // `Units.nearbyEnemies(team, x, y, rad + 10f, unitConsumer)`
    // (`BaseShield.java:96-97`).
    repel_units(world, env, team, center, radius);
}

/// `BaseShield.bulletConsumer` (`BaseShield.java:27-36`): every bullet of
/// another team whose type is `absorbable` and that is inside the dome is
/// absorbed. Non-absorbable bullets (`BulletType.absorbable = false`) pass
/// through untouched.
fn absorb_bullets(
    world: &mut World,
    content: &ContentRegistry,
    team: u8,
    center: (f32, f32),
    radius: f32,
) {
    let mut targets: Vec<Entity> = Vec::new();
    for entity_ref in world.iter_entities() {
        let Some(bullet) = entity_ref.get::<Bullet>() else {
            continue;
        };
        if entity_ref.get::<TeamComp>().map(|t| t.team) == Some(team) {
            continue;
        }
        // `bullet.type.absorbable`; a bullet without a resolved def cannot be
        // classified, so it is left alone (upstream types always resolve).
        if !content.bullet(bullet.def).is_some_and(|def| def.absorbable) {
            continue;
        }
        let Some(bullet_pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dx = bullet_pos.x - center.0;
        let dy = bullet_pos.y - center.1;
        // `bullet.within(paramBuild, paramBuild.radius())`: strict `dst < r`
        // (`Position.within`, Arc `Position.java:44`).
        if dx * dx + dy * dy < radius * radius {
            targets.push(entity_ref.id());
        }
    }
    targets.sort_by_key(|entity| entity.index());
    for target in targets {
        // `bullet.absorb()` (`BulletComp.java:100-103`): flag it; the bullet
        // pass later runs `despawned`/`removed` and despawns it.
        if let Some(mut bullet) = world.get_mut::<Bullet>(target) {
            bullet.set(ABSORBED);
        }
    }
}

/// `BaseShield.unitConsumer` (`BaseShield.java:38-57`) over
/// `Units.nearbyEnemies(team, x, y, radius + 10, ...)`: live units of other
/// teams within `(radius + 10) + hitSize / 2` (`Units.java:482-488`).
fn repel_units(
    world: &mut World,
    env: &BaseShieldEnv<'_>,
    team: u8,
    center: (f32, f32),
    radius: f32,
) {
    let mut targets: Vec<Entity> = Vec::new();
    for entity_ref in world.iter_entities() {
        // `Groups.unit` holds live units only; a unit already flagged dead is
        // awaiting the death pass and is skipped (`UnitComp.killed`).
        let Some(core) = entity_ref.get::<UnitCore>() else {
            continue;
        };
        if core.dead {
            continue;
        }
        if entity_ref.get::<TeamComp>().map(|t| t.team) == Some(team) {
            continue;
        }
        let Some(hitbox) = entity_ref.get::<HitboxComp>() else {
            continue;
        };
        let Some(unit_pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dx = unit_pos.x - center.0;
        let dy = unit_pos.y - center.1;
        let reach = radius + 10.0 + hitbox.hit_size / 2.0;
        if dx * dx + dy * dy < reach * reach {
            targets.push(entity_ref.id());
        }
    }
    targets.sort_by_key(|entity| entity.index());
    for unit in targets {
        repel_unit(world, env, team, center, radius, unit);
    }
}

/// One `BaseShield.unitConsumer` call (`BaseShield.java:38-57`).
fn repel_unit(
    world: &mut World,
    env: &BaseShieldEnv<'_>,
    team: u8,
    center: (f32, f32),
    radius: f32,
    unit: Entity,
) {
    let Some(unit_pos) = world.get::<Pos>(unit).copied() else {
        return;
    };
    let hit_size = world
        .get::<HitboxComp>(unit)
        .map_or(0.0, |hitbox| hitbox.hit_size);
    let dx = unit_pos.x - center.0;
    let dy = unit_pos.y - center.1;
    let dst = (dx * dx + dy * dy).sqrt();
    // `float overlapDst = (unit.hitSize/2f + paramBuild.radius()) - unit.dst(paramBuild);`
    let overlap = (hit_size / 2.0 + radius) - dst;
    if overlap <= 0.0 {
        return;
    }
    if overlap > hit_size * 1.5 {
        // `unit.kill()` for units stuck deep inside the dome
        // (`BaseShield.java:43-45`).
        mark_unit_dead(world, unit);
        return;
    }
    // `unit.vel.setZero()`, then `unit.move(Tmp.v1.set(unit).sub(paramBuild)
    // .setLength(overlapDst + 0.01f))`.
    if let Some(mut vel) = world.get_mut::<Vel>(unit) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    // `Vec2.setLength` is a no-op on a zero vector (Arc `Vec2.java:299-307`),
    // which is what happens when the unit sits exactly on the dome centre.
    if dst > 0.0 {
        let scale = (overlap + 0.01) / dst;
        move_unit_by(world, env, unit, dx * scale, dy * scale);
    }
    // `if(Mathf.chanceDelta(0.12f * Time.delta)) Fx.circleColorSpark.at(
    // unit.x, unit.y, paramBuild.team.color);`
    if spark_chance(world, unit) {
        let (x, y) = world
            .get::<Pos>(unit)
            .map_or((unit_pos.x, unit_pos.y), |pos| (pos.x, pos.y));
        crate::fx::sink::emit_named(
            env.fx,
            crate::content::EffectId::CIRCLE_COLOR_SPARK,
            x,
            y,
            0.0,
            crate::game::team::Team::get(team).color,
        );
    }
}

/// `unit.move(...)` (`VelComp.move`, VelComp.java:61-74): ground units resolve
/// solid tiles through `EntityCollisions.move`; flying units translate
/// directly (`ElevationMoveComp.solidity()` is null in the air).
fn move_unit_by(world: &mut World, env: &BaseShieldEnv<'_>, unit: Entity, dx: f32, dy: f32) {
    let Some(mut pos) = world.get::<Pos>(unit).copied() else {
        return;
    };
    let hit_size = world
        .get::<HitboxComp>(unit)
        .map_or(0.0, |hitbox| hitbox.hit_size);
    if crate::combat::bullet::unit_is_flying(world, unit) {
        pos.x += dx;
        pos.y += dy;
    } else {
        let solid = |tx: i32, ty: i32| {
            !crate::ai::types::ground::unit_can_pass(world, env.grid, env.content, unit, tx, ty)
        };
        crate::entities::comp::unit::velocity::collisions_move(&mut pos, hit_size, dx, dy, &solid);
    }
    if let Some(mut stored) = world.get_mut::<Pos>(unit) {
        *stored = pos;
    }
}

/// `unit.kill()` (`HealthComp.java:33-40`, `UnitComp.kill`): zero health and
/// flag the unit dead. The sim's `resolve_deaths` pass (same tick) runs the
/// `killed`/`destroy` path, including the falling-wreck branch for flying
/// units (`UnitComp.java:980-991`).
fn mark_unit_dead(world: &mut World, unit: Entity) {
    if let Some(mut health) = world.get_mut::<Health>(unit) {
        health.health = health.health.min(0.0);
        health.dead = true;
    }
    if let Some(mut core) = world.get_mut::<UnitCore>(unit) {
        core.dead = true;
    }
}

/// `Mathf.chanceDelta(0.12f * Time.delta)` for the view-only push spark.
///
/// The roll never feeds the simulation, so it uses a deterministic seeded
/// [`crate::math::ArcRand`] (like `emit_wreck_decals`) instead of the sim RNG
/// stream, seeded from the unit's `EntitySeq` and the build clock tick.
fn spark_chance(world: &World, unit: Entity) -> bool {
    let seq = world
        .get::<crate::ecs::EntitySeq>(unit)
        .map_or(0, |seq| seq.0);
    let tick = crate::world::update::build_time(world) as u64;
    let mut rand = crate::math::ArcRand::new(seq ^ tick.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    rand.chance(0.12)
}

#[cfg(test)]
mod suppression_tests {
    use crate::combat::harness::CombatHarness;
    use crate::world::blocks::defense::projectors::is_heal_suppressed;

    /// L06 T3: a bullet with `suppressionRange` (`BulletType.java:572-575`)
    /// suppresses enemy healing blocks around the bullet
    /// (`Damage.applySuppression`, `BuildingComp.applyHealSuppression`), not
    /// non-suppressable blocks; a suppressed mender does not heal
    /// (`MendProjector.java:94,112`).
    #[test]
    fn suppression_bullet_stops_enemy_mender() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let mender = harness.content().block_id("mender").expect("mender");
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, mender, 0, true));
        assert!(harness.place(11, 10, wall, 0, true));
        let fuse = harness.bullet_id("fuse").expect("fuse");
        if let Some(def) = harness.build.content.bullet_mut(fuse) {
            def.suppression_range = 40.0;
        }
        let mender_e = harness.build_at(10, 10).expect("mender entity");
        let wall_e = harness.build_at(11, 10).expect("wall entity");
        assert!(!is_heal_suppressed(&harness.build.world, mender_e));
        // Team-1 fuse flies into the team-0 wall row from the left.
        let (x, y) = CombatHarness::tile_origin(4, 10);
        let _ = harness.spawn_bullet("fuse", x, y, 0.0, 1);
        for _ in 0..30 {
            harness.step_bullets_only();
        }
        assert!(
            is_heal_suppressed(&harness.build.world, mender_e),
            "enemy mender suppressed"
        );
        assert!(
            !is_heal_suppressed(&harness.build.world, wall_e),
            "a wall is not suppressable"
        );
    }
}

#[cfg(test)]
mod base_shield_tests {
    use std::sync::{Arc, Mutex};

    use bevy_ecs::entity::Entity;

    use crate::combat::bullet::{ABSORBED, Bullet};
    use crate::combat::harness::CombatHarness;
    use crate::combat::view::FxSink;
    use crate::content::registries::fx_meta::EffectRef;
    use crate::content::{EffectId, Rgba};
    use crate::entities::comp::unit::comp::{HitboxComp, UnitCore};
    use crate::entities::comp::{Health, Pos, TeamComp, Vel};
    use crate::world::blocks::defense::projectors::{
        BaseShieldEnv, BaseShieldState, update_base_shield,
    };

    /// Spawns a `BaseShield` fixture (`shield-projector` dome) at the tile
    /// centre with a fully grown radius.
    fn spawn_shield(
        harness: &mut CombatHarness,
        tile: (i32, i32),
        team: u8,
        radius: f32,
    ) -> Entity {
        let (x, y) = CombatHarness::tile_center(tile.0, tile.1);
        let seq = harness.seq;
        harness.seq = harness.seq.wrapping_add(1);
        harness
            .build
            .world
            .spawn((
                crate::ecs::EntitySeq(seq),
                Pos { x, y },
                TeamComp { team },
                BaseShieldState {
                    radius,
                    smooth_radius: radius,
                    ..BaseShieldState::default()
                },
            ))
            .id()
    }

    /// Spawns a real content unit (`UnitType.spawn` bundle: `UnitCore`,
    /// `HitboxComp`, `Vel`, `Health`).
    fn spawn_unit(harness: &mut CombatHarness, name: &str, team: u8, x: f32, y: f32) -> Entity {
        let seq = harness.seq;
        harness.seq = harness.seq.wrapping_add(1);
        crate::entities::comp::unit::lifecycle::spawn_unit(
            &mut harness.build.world,
            &harness.build.content,
            seq,
            name,
            team,
            x,
            y,
            0.0,
        )
        .expect("unit content")
    }

    /// Drives one `BaseShieldBuild.updateTile` with the harness as the live
    /// runtime seam (content/grid/FX).
    fn step_shield(harness: &mut CombatHarness, shield: Entity, efficiency: f32) {
        let Some(mut state) = harness
            .build
            .world
            .entity_mut(shield)
            .take::<BaseShieldState>()
        else {
            return;
        };
        let env = BaseShieldEnv {
            content: &harness.build.content,
            grid: &harness.build.grid,
            fx: harness.fx.as_ref(),
        };
        update_base_shield(
            &mut harness.build.world,
            shield,
            &mut state,
            efficiency,
            &env,
        );
        harness.build.world.entity_mut(shield).insert(state);
    }

    fn bullet_absorbed(harness: &CombatHarness, bullet: Entity) -> bool {
        harness
            .build
            .world
            .get::<Bullet>(bullet)
            .is_some_and(|bullet| bullet.has(ABSORBED))
    }

    fn unit_pos(harness: &CombatHarness, unit: Entity) -> (f32, f32) {
        harness
            .build
            .world
            .get::<Pos>(unit)
            .map_or((0.0, 0.0), |pos| (pos.x, pos.y))
    }

    /// `BaseShield.bulletConsumer` (`BaseShield.java:27-36`): an absorbable
    /// enemy bullet inside the dome is absorbed (`bullet.absorb()` sets the
    /// `absorbed` flag; the bullet pass finishes it).
    #[test]
    fn absorbable_enemy_bullet_is_absorbed() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        let def = harness.bullet_id("fuse").expect("fuse");
        assert!(
            harness
                .build
                .content
                .bullet(def)
                .expect("bullet def")
                .absorbable,
            "fixture sanity: fuse is absorbable"
        );
        let bullet = harness
            .spawn_bullet("fuse", cx + 10.0, cy, 0.0, 1)
            .expect("bullet");
        step_shield(&mut harness, shield, 1.0);
        assert!(bullet_absorbed(&harness, bullet), "bullet absorbed");
        // The bullet pass then runs `BulletComp.remove` (`despawned`/`removed`)
        // and drops the entity.
        harness.step_bullets_only();
        assert!(
            harness.build.world.get_entity(bullet).is_err(),
            "the absorbed bullet is removed by the bullet pass"
        );
    }

    /// PAR-0475: `bullet.type.absorbable` gates the consumer — a
    /// non-absorbable round passes through the dome untouched.
    #[test]
    fn non_absorbable_bullet_passes_through() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let def = harness.bullet_id("fuse").expect("fuse");
        if let Some(bullet_def) = harness.build.content.bullet_mut(def) {
            bullet_def.absorbable = false;
        }
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        let bullet = harness
            .spawn_bullet("fuse", cx + 10.0, cy, 0.0, 1)
            .expect("bullet");
        step_shield(&mut harness, shield, 1.0);
        assert!(
            !bullet_absorbed(&harness, bullet),
            "non-absorbable bullet passes through"
        );
        harness.step_bullets_only();
        assert!(
            crate::combat::bullet::bullet_alive(&harness.build.world, bullet),
            "the bullet survives the pass"
        );
    }

    /// The consumer also gates on team (`bullet.team != paramBuild.team`).
    #[test]
    fn friendly_bullet_is_not_absorbed() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        let bullet = harness
            .spawn_bullet("fuse", cx + 10.0, cy, 0.0, 0)
            .expect("bullet");
        step_shield(&mut harness, shield, 1.0);
        assert!(
            !bullet_absorbed(&harness, bullet),
            "team-0 bullet ignored by a team-0 dome"
        );
    }

    /// `BaseShield.unitConsumer` (`BaseShield.java:42-55`): a shallow overlap
    /// (`overlapDst <= hitSize * 1.5`) zeroes velocity and pushes the unit out
    /// by `overlapDst + 0.01`.
    #[test]
    fn shallow_enemy_unit_is_pushed_out() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        // dagger: hitSize 8 -> overlap 60 + 4 - 58 = 6 <= 12.
        let unit = spawn_unit(&mut harness, "dagger", 1, cx + 58.0, cy);
        if let Some(mut vel) = harness.build.world.get_mut::<Vel>(unit) {
            vel.x = 1.5;
            vel.y = -0.5;
        }
        step_shield(&mut harness, shield, 1.0);
        let (x, y) = unit_pos(&harness, unit);
        let dst = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        assert!(
            dst >= 60.0 + 4.0,
            "unit pushed outside the dome: dst = {dst}"
        );
        let vel = harness.build.world.get::<Vel>(unit).expect("vel");
        assert_eq!((vel.x, vel.y), (0.0, 0.0), "velocity zeroed");
        let health = harness.build.world.get::<Health>(unit).expect("health");
        assert!(!health.dead, "a shallow overlap is not lethal");
    }

    /// `BaseShield.java:43-45`: an overlap deeper than `hitSize * 1.5` kills
    /// the stuck unit (`unit.kill()` => health/dead flags; the sim's
    /// `resolve_deaths` pass destroys it in the same tick).
    #[test]
    fn deep_enemy_unit_is_killed() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        // dagger: hitSize 8 -> overlap 60 + 4 - 40 = 24 > 12.
        let unit = spawn_unit(&mut harness, "dagger", 1, cx + 40.0, cy);
        step_shield(&mut harness, shield, 1.0);
        let health = harness.build.world.get::<Health>(unit).expect("health");
        assert!(health.dead && health.health <= 0.0, "stuck unit killed");
        assert!(
            harness
                .build
                .world
                .get::<UnitCore>(unit)
                .expect("core")
                .dead,
            "UnitComp.killed sets dead"
        );
        // Not pushed: the kill branch returns before the move.
        let (x, _) = unit_pos(&harness, unit);
        assert!((x - (cx + 40.0)).abs() < 1e-3, "a killed unit is not moved");
    }

    /// `Units.nearbyEnemies` only visits other teams (`Units.java:482-488`);
    /// a friendly unit inside the dome is untouched.
    #[test]
    fn friendly_unit_is_ignored() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        let unit = spawn_unit(&mut harness, "dagger", 0, cx + 40.0, cy);
        step_shield(&mut harness, shield, 1.0);
        let (x, y) = unit_pos(&harness, unit);
        assert!((x - (cx + 40.0)).abs() < 1e-3 && (y - cy).abs() < 1e-3);
        assert!(
            !harness
                .build
                .world
                .get::<Health>(unit)
                .expect("health")
                .dead
        );
    }

    /// `Mathf.chanceDelta(0.12f * Time.delta)` plays `Fx.circleColorSpark` at
    /// the pushed unit (`BaseShield.java:52-54`).
    #[test]
    fn pushed_unit_emits_circle_color_spark() {
        #[derive(Default)]
        struct RecordingSink {
            effects: Mutex<Vec<EffectId>>,
        }

        impl FxSink for RecordingSink {
            fn effect(&self, effect: &EffectRef, _x: f32, _y: f32, _rotation: f32, _color: Rgba) {
                if let EffectRef::Named(id) = effect {
                    self.effects.lock().expect("lock").push(*id);
                }
            }
        }

        let mut harness = CombatHarness::new(48, 32, 5);
        let sink = Arc::new(RecordingSink::default());
        harness.set_fx(sink.clone());
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        let unit = spawn_unit(&mut harness, "dagger", 1, cx + 58.0, cy);
        for _ in 0..120 {
            // The shield pushes the unit out, so re-seat it in the overlap band
            // and advance the build clock (the view roll is seeded by tick).
            harness.build.tick();
            if let Some(mut pos) = harness.build.world.get_mut::<Pos>(unit) {
                pos.x = cx + 58.0;
                pos.y = cy;
            }
            step_shield(&mut harness, shield, 1.0);
        }
        let effects = sink.effects.lock().expect("lock");
        assert!(
            effects.contains(&EffectId::CIRCLE_COLOR_SPARK),
            "expected at least one circleColorSpark over 120 pushed ticks: {effects:?}"
        );
    }

    /// `Units.nearbyEnemies(team, x, y, rad + 10f, ...)` also keeps units just
    /// outside the dome but inside the widened query (big hitbox) — they are
    /// not overlapping, so nothing happens.
    #[test]
    fn unit_outside_the_overlap_band_is_untouched() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let shield = spawn_shield(&mut harness, (20, 16), 0, 60.0);
        let (cx, cy) = CombatHarness::tile_center(20, 16);
        // dst 68: inside the `rad + 10 + hitSize/2` query (72) but outside the
        // `rad + hitSize/2` overlap band (64).
        let unit = spawn_unit(&mut harness, "dagger", 1, cx + 68.0, cy);
        step_shield(&mut harness, shield, 1.0);
        let (x, _) = unit_pos(&harness, unit);
        assert!(
            (x - (cx + 68.0)).abs() < 1e-3,
            "no push outside the overlap"
        );
        assert!(
            !harness
                .build
                .world
                .get::<Health>(unit)
                .expect("health")
                .dead
        );
        // The fixture's hitbox is the dagger 8 px box.
        assert_eq!(
            harness
                .build
                .world
                .get::<HitboxComp>(unit)
                .expect("hitbox")
                .hit_size,
            8.0
        );
    }
}
