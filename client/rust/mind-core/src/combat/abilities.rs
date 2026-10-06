// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit ability runtime (`core/src/mindustry/entities/abilities/*`).
//!
//! Plan 02 owns the [`AbilitySpec`] metadata; this module owns the behavior
//! half. [`update_unit_abilities`] is the per-entity entry point the unit
//! update loop calls (the headless harness reaches it through
//! [`crate::entities::comp::unit::movement::update_kinematics`]), and
//! [`run_death_abilities`] ports `Ability.death` for `SpawnDeathAbility`/
//! `LiquidExplodeAbility`.
//!
//! Implemented kinds: `ShieldRegenField`, `RepairField`, `ForceField`,
//! `StatusField`, `EnergyField`, `SuppressionField`, `ShieldArc`, `Regen` and
//! `LiquidRegen` update, plus the `SpawnDeath`/`LiquidExplode` death hooks.
//! `MoveEffectAbility` is view-only upstream (`Vars.headless` early return) and
//! stays out of the simulation. `Ability.data` (ShieldArc pool) is persisted in
//! [`AbilityState::data`]; the other per-ability state lives in the same record.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::combat::bullet::{ABSORBED, Bullet};
use crate::combat::damage::{apply_status, damage_entity, heal_health};
use crate::combat::puddles::{self, PuddleState};
use crate::content::registries::units::UnitTypeDef;
use crate::content::registries::units::ability::{AbilityKind, AbilitySpec};
use crate::content::{ContentRegistry, LiquidId};
use crate::determinism::{RngStream, SimRng};
use crate::entities::comp::unit::comp::{
    HitboxComp, PhysicsComp, ShieldComp, UnitCore, UnitTypeComp,
};
use crate::entities::comp::unit::queries;
use crate::entities::comp::{Building, Health, Pos, TeamComp, Vel};
use crate::world::tiles::Tiles;

/// `RepairFieldAbility.smartInterval` (ticks).
const SMART_INTERVAL: f32 = 20.0;
/// `SuppressionFieldAbility.maxDelay` (ticks); vanilla never overrides it.
const SUPPRESSION_MAX_DELAY: f32 = 60.0 * 1.5;
/// `LiquidRegenAbility.slurpSpeed`.
const SLURP_SPEED: f32 = 5.0;
/// `LiquidRegenAbility.regenPerSlurp`.
const REGEN_PER_SLURP: f32 = 6.0;
/// `LiquidExplodeAbility` defaults.
const LIQUID_EXPLODE_AMOUNT: f32 = 120.0;
const LIQUID_EXPLODE_RAD_AMOUNT_SCALE: f32 = 5.0;
const LIQUID_EXPLODE_RAD_SCALE: f32 = 1.0;
const LIQUID_EXPLODE_NOISE_MAG: f32 = 6.5;
const LIQUID_EXPLODE_NOISE_SCL: f32 = 5.0;
/// `ShieldArcAbility.reflectTime` default (`1f - 0.5f`).
const SHIELD_ARC_REFLECT_TIME: f32 = 0.5;

/// One ability's runtime state (`Ability.data` plus the concrete subclass
/// counters; only `data` is synced upstream).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AbilityState {
    /// Generic reload timer (`timer` fields; `Ability.data` for some kinds).
    pub timer: f32,
    /// Synced `Ability.data` (ShieldArc shield pool).
    pub data: f32,
    /// Scratch: EnergyField `curStroke`, ShieldArc `alpha`.
    pub aux: f32,
    /// ForceField `wasBroken`.
    pub was_broken: bool,
    /// RepairField `hasHealed`.
    pub has_healed: bool,
    /// RepairField `downTimer`.
    pub down_timer: f32,
    /// RepairField `healthMissing`.
    pub health_missing: f32,
    /// RepairField `healthChange`.
    pub health_change: f32,
    /// Deterministic per-ability random state (deflection chance rolls).
    pub rng: u64,
}

/// Per-ability runtime states, index-aligned with `UnitType.abilities`
/// (`UnitComp.abilities`).
#[derive(Debug, Clone, PartialEq, Component)]
pub struct AbilityComp {
    /// One state per ability (`Ability[] abilities`).
    pub states: Vec<AbilityState>,
}

impl AbilityComp {
    /// Creates zeroed states for a unit's abilities (`setType`/`copy`).
    pub fn for_def(def: &UnitTypeDef) -> Self {
        Self {
            states: vec![AbilityState::default(); def.abilities.len()],
        }
    }
}

/// Initializes a freshly spawned unit's ability state (`Ability.created`).
///
/// `ForceFieldAbility`/`ShieldArcAbility` start at their max shield.
pub fn init_unit_abilities(world: &mut World, entity: Entity, def: &UnitTypeDef) {
    let mut comp = AbilityComp::for_def(def);
    if let Some(seq) = world.get::<crate::ecs::EntitySeq>(entity) {
        for state in &mut comp.states {
            state.rng = seq.0.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        }
    }
    for (index, spec) in def.abilities.iter().enumerate() {
        match spec.kind {
            AbilityKind::ForceField => {
                if let Some(mut shield) = world.get_mut::<ShieldComp>(entity) {
                    shield.shield = spec.max;
                }
            }
            AbilityKind::ShieldArc => {
                if let Some(state) = comp.states.get_mut(index) {
                    state.data = spec.max;
                }
            }
            _ => {}
        }
    }
    world.entity_mut(entity).insert(comp);
}

/// Returns the unit's `(x, y, team, hit_size, flying)` if it is still valid.
fn unit_frame(world: &World, entity: Entity) -> Option<(f32, f32, u8, f32, bool)> {
    let pos = world.get::<Pos>(entity)?;
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
    let hit_size = world
        .get::<HitboxComp>(entity)
        .map(|h| h.hit_size)
        .unwrap_or(0.0);
    let flying = world
        .get::<PhysicsComp>(entity)
        .map(|p| p.flying)
        .unwrap_or(false);
    Some((pos.x, pos.y, team, hit_size, flying))
}

/// `Angles.within(angle, base, range)`: shortest angular distance check.
fn angles_within(angle: f32, base: f32, range: f32) -> bool {
    let mut delta = (angle - base) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    delta.abs() <= range
}

/// `Intersector.isInRegularPolygon`: point-in-polygon for a regular polygon.
fn is_in_regular_polygon(
    sides: i32,
    x: f32,
    y: f32,
    radius: f32,
    rotation: f32,
    px: f32,
    py: f32,
) -> bool {
    if sides < 3 || radius <= 0.0 {
        return false;
    }
    let edge = 360.0 / sides as f32;
    let inradius = (edge.to_radians() / 2.0).cos() * radius;
    for i in 0..sides {
        let normal = (rotation + edge * i as f32 + edge / 2.0).to_radians();
        if (px - x) * normal.cos() + (py - y) * normal.sin() > inradius {
            return false;
        }
    }
    true
}

/// Deterministic `[0, 1)` roll for per-ability chances.
fn ability_chance(state: &mut AbilityState) -> f32 {
    if state.rng == 0 {
        state.rng = 0x9E37_79B9_7F4A_7C15;
    }
    let mut x = state.rng;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    state.rng = x;
    let value = x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40;
    value as f32 / (1u32 << 24) as f32
}

/// Advances every live unit's abilities (`Groups.unit` ability pass).
pub fn update_abilities(world: &mut World, content: &ContentRegistry) {
    let mut entities: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter(|entity_ref| entity_ref.get::<AbilityComp>().is_some())
        .map(|entity_ref| {
            let seq = entity_ref
                .get::<crate::ecs::EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            (seq, entity_ref.id())
        })
        .collect();
    entities.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    for (_, entity) in entities {
        update_unit_abilities(world, content, entity);
    }
}

/// Advances one unit's abilities one tick (`for(Ability a : abilities) a.update`).
///
/// Missing [`AbilityComp`] is created lazily so harness fixtures that predate
/// abilities still run the pass.
pub fn update_unit_abilities(world: &mut World, content: &ContentRegistry, entity: Entity) {
    let Some(type_id) = world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id) else {
        return;
    };
    let Some(def) = content.unit(type_id) else {
        return;
    };
    if world.get::<AbilityComp>(entity).is_none() {
        let mut comp = AbilityComp::for_def(def);
        if let Some(seq) = world.get::<crate::ecs::EntitySeq>(entity) {
            for state in &mut comp.states {
                state.rng = seq.0.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            }
        }
        world.entity_mut(entity).insert(comp);
    }
    let Some(mut comp) = world.entity_mut(entity).take::<AbilityComp>() else {
        return;
    };
    if comp.states.len() != def.abilities.len() {
        comp.states
            .resize(def.abilities.len(), AbilityState::default());
    }
    for index in 0..def.abilities.len() {
        let spec = def.abilities[index].clone();
        let state = &mut comp.states[index];
        update_ability(world, content, entity, &spec, state);
    }
    world.entity_mut(entity).insert(comp);
}

/// Dispatches one ability's `update(Unit)`.
fn update_ability(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    spec: &AbilitySpec,
    state: &mut AbilityState,
) {
    let Some((x, y, team, hit_size, flying)) = unit_frame(world, entity) else {
        return;
    };
    match spec.kind {
        // `ShieldRegenFieldAbility.update`.
        AbilityKind::ShieldRegenField => {
            state.timer += 1.0;
            if state.timer >= spec.reload {
                for target in queries::in_radius(world, x, y, spec.range, Some(team)) {
                    if let Some(mut shield) = world.get_mut::<ShieldComp>(target)
                        && shield.shield < spec.max
                    {
                        shield.shield = (shield.shield + spec.amount).min(spec.max);
                    }
                }
                state.timer = 0.0;
            }
        }
        // `RepairFieldAbility.update`.
        AbilityKind::RepairField => {
            state.timer += 1.0;
            if spec.smart_heal
                && state.health_change >= state.health_missing
                && state.health_missing > 0.0
            {
                state.down_timer += 1.0;
            } else {
                state.down_timer = 0.0;
            }
            if state.timer >= spec.reload {
                repair_field_tick(world, entity, spec, state, x, y, team);
            }
        }
        // `ForceFieldAbility.update`.
        AbilityKind::ForceField => {
            let mut shield = world
                .get::<ShieldComp>(entity)
                .map(|comp| comp.shield)
                .unwrap_or(0.0);
            if shield <= 0.0 && !state.was_broken {
                shield -= spec.cooldown * spec.regen;
            }
            state.was_broken = shield <= 0.0;
            if shield < spec.max {
                shield += spec.regen;
            }
            if shield > 0.0 {
                absorb_bullets_force_field(world, content, spec, x, y, team, &mut shield);
            }
            if let Some(mut comp) = world.get_mut::<ShieldComp>(entity) {
                comp.shield = shield;
            }
        }
        // `StatusFieldAbility.update` (`onShoot` is never set by vanilla).
        AbilityKind::StatusField => {
            state.timer += 1.0;
            if state.timer >= spec.reload {
                if let Some(name) = spec.effect
                    && let Some(status) = content.status_id(name)
                {
                    for target in queries::in_radius(world, x, y, spec.range, Some(team)) {
                        apply_status(world, target, status, spec.duration);
                    }
                }
                state.timer = 0.0;
            }
        }
        // `EnergyFieldAbility.update`.
        AbilityKind::EnergyField => {
            state.timer += 1.0;
            if state.timer >= spec.reload {
                energy_field_tick(world, content, entity, spec, x, y, team);
                state.timer = 0.0;
            }
        }
        // `SuppressionFieldAbility.update`.
        AbilityKind::SuppressionField => {
            if !spec.active {
                return;
            }
            state.timer += 1.0;
            if state.timer >= SUPPRESSION_MAX_DELAY {
                let (rx, ry) = rotate_offset(x, y, spec.x, spec.y, unit_rotation(world, entity));
                for (_, target) in buildings_in_radius(world, rx, ry, spec.range) {
                    let enemy = world
                        .get::<TeamComp>(target)
                        .is_some_and(|comp| comp.team != team);
                    if enemy && let Some(mut building) = world.get_mut::<Building>(target) {
                        building.heal_suppression_time =
                            building.heal_suppression_time.max(spec.reload + 1.0);
                    }
                }
                state.timer = 0.0;
            }
        }
        // `ShieldArcAbility.update`.
        AbilityKind::ShieldArc => {
            if state.data < spec.max {
                state.data += spec.regen;
            }
            let active =
                state.data > 0.0 && (device_is_shooting(world, entity) || !spec.when_shooting);
            if active {
                shield_arc_absorb(world, content, entity, spec, state, x, y, team);
            }
        }
        // `RegenAbility.update`.
        AbilityKind::Regen => {
            if let Some(health) = world.get::<Health>(entity) {
                let amount = health.max_health * spec.percent_amount / 100.0 + spec.amount;
                if amount > 0.0 {
                    heal_health(world, entity, amount);
                }
            }
        }
        // `LiquidRegenAbility.update`.
        AbilityKind::LiquidRegen => {
            if let Some(name) = spec.liquid
                && let Some(liquid) = content.liquid_by_name(name)
            {
                liquid_regen_tick(world, entity, liquid.id, x, y, hit_size, flying);
            }
        }
        // View-only (`MoveEffectAbility.update` returns in headless) or handled
        // by the death hook (`SpawnDeath`, `LiquidExplode`).
        AbilityKind::MoveEffect | AbilityKind::SpawnDeath | AbilityKind::LiquidExplode => {}
    }
}

/// Unit facing in degrees.
fn unit_rotation(world: &World, entity: Entity) -> f32 {
    world
        .get::<UnitCore>(entity)
        .map(|core| core.rotation)
        .unwrap_or(0.0)
}

/// `Tmp.v1.trns(unit.rotation - 90f, x, y).add(unit)`.
fn rotate_offset(x: f32, y: f32, ox: f32, oy: f32, rotation: f32) -> (f32, f32) {
    let rad = (rotation - 90.0).to_radians();
    (
        x + ox * rad.cos() - oy * rad.sin(),
        y + ox * rad.sin() + oy * rad.cos(),
    )
}

/// `RepairFieldAbility.update` reload branch.
fn repair_field_tick(
    world: &mut World,
    entity: Entity,
    spec: &AbilitySpec,
    state: &mut AbilityState,
    x: f32,
    y: f32,
    team: u8,
) {
    let unit_type = world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id);
    let limit_targets = spec.max_targets >= 0;
    let heal_percent_mult = spec.heal_percent / 100.0;
    let mut targets: Vec<Entity> = Vec::new();
    let mut heal_now = false;
    let mut health_missing = 0.0f32;
    let mut sum_max_health = 0.0f32;
    let mut sum_type_mult = 0.0f32;

    for other in queries::in_radius(world, x, y, spec.range, Some(team)) {
        if limit_targets && targets.len() >= spec.max_targets as usize + 2 {
            break;
        }
        let Some(health) = world.get::<Health>(other).copied() else {
            continue;
        };
        if health.health >= health.max_health - 0.001 {
            continue;
        }
        targets.push(other);
        if spec.smart_heal {
            health_missing += health.max_health - health.health;
            sum_max_health += health.max_health;
            let same_type = unit_type.is_some()
                && world.get::<UnitTypeComp>(other).map(|comp| comp.type_id) == unit_type;
            sum_type_mult += if same_type {
                spec.same_type_heal_mult
            } else {
                1.0
            };
            if health.max_health > 0.0 && health.health / health.max_health < 0.0 {
                heal_now = true;
            }
        }
    }

    let target_count = targets.len();
    // `smartHealPercent` is 0 for vanilla, so `healNow` never fires; kept for
    // structural parity with the upstream expression.
    let _ = heal_now;

    let ratio = if target_count > 0 {
        spec.amount + heal_percent_mult * sum_max_health * (sum_type_mult / target_count as f32)
    } else {
        spec.amount
    };
    let required_heals = if target_count > 0 && ratio > 0.0 {
        let divisor = if limit_targets {
            spec.max_targets.max(1) as f32
        } else {
            target_count as f32
        };
        (health_missing * 0.7 + health_missing / divisor * 0.3) / ratio
    } else {
        f32::INFINITY
    };

    let should_heal = required_heals >= 1.0
        || !spec.smart_heal
        || heal_now
        || state.down_timer >= spec.smart_downtime;

    state.health_change = state.health_missing;
    state.health_missing = health_missing;

    if should_heal && target_count > 0 {
        if limit_targets {
            let is_same_type = spec.same_type_heal_mult < 1.0;
            targets.sort_by(|a, b| {
                let key = |target: &Entity| -> f32 {
                    let dst2 = world
                        .get::<Pos>(*target)
                        .map(|pos| (pos.x - x).powi(2) + (pos.y - y).powi(2))
                        .unwrap_or(f32::MAX);
                    let same = is_same_type
                        && unit_type.is_some()
                        && world.get::<UnitTypeComp>(*target).map(|comp| comp.type_id) == unit_type;
                    dst2 + if same { 6400.0 } else { 0.0 }
                };
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.index().cmp(&b.index()))
            });
        }
        let len = if limit_targets {
            target_count.min(spec.max_targets.max(0) as usize)
        } else {
            target_count
        };
        state.has_healed = false;
        for target in targets.iter().take(len) {
            let Some(health) = world.get::<Health>(*target).copied() else {
                continue;
            };
            if health.health >= health.max_health - 0.001 {
                continue;
            }
            let same_type = unit_type.is_some()
                && world.get::<UnitTypeComp>(*target).map(|comp| comp.type_id) == unit_type;
            let heal_mult = if same_type {
                spec.same_type_heal_mult
            } else {
                1.0
            };
            let amount = (spec.amount + heal_percent_mult * health.max_health) * heal_mult;
            heal_health(world, *target, amount);
            state.has_healed = true;
        }
        if state.has_healed {
            state.timer = 0.0;
        }
    } else if spec.smart_heal && target_count > 0 {
        state.timer = if spec.reload >= 2.0 * SMART_INTERVAL {
            spec.reload - SMART_INTERVAL
        } else {
            SMART_INTERVAL
        };
    } else if !spec.smart_heal {
        // Stays loaded and retries next tick until a target is healed.
    } else {
        state.timer = 0.0;
    }
}

/// `EnergyFieldAbility.update` reload branch.
fn energy_field_tick(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    spec: &AbilitySpec,
    x: f32,
    y: f32,
    team: u8,
) {
    let (rx, ry) = rotate_offset(x, y, spec.x, spec.y, unit_rotation(world, entity));
    let unit_type = world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id);
    let damage_multiplier = 1.0;

    let mut candidates: Vec<(f32, Entity)> = Vec::new();
    for target in queries::in_radius(world, rx, ry, spec.range, None) {
        if target == entity {
            continue;
        }
        let target_team = world.get::<TeamComp>(target).map(|comp| comp.team);
        let damaged = world
            .get::<Health>(target)
            .is_some_and(|health| health.health < health.max_health - 0.001);
        if target_team != Some(team) || damaged {
            candidates.push((dst2(world, target, rx, ry), target));
        }
    }
    for (dst2, target) in buildings_in_radius(world, rx, ry, spec.range) {
        let target_team = world.get::<TeamComp>(target).map(|comp| comp.team);
        let damaged = world
            .get::<Health>(target)
            .is_some_and(|health| health.health < health.max_health - 0.001);
        if target_team != Some(team) || damaged {
            candidates.push((dst2, target));
        }
    }
    candidates.sort_by_key(|(dst2, target)| (dst2.to_bits(), target.index()));

    let scaled_damage = spec.amount * damage_multiplier;
    let status = spec.status.and_then(|name| content.status_id(name));
    for (_, target) in candidates.iter().take(spec.max_targets.max(0) as usize) {
        let target_team = world.get::<TeamComp>(*target).map(|comp| comp.team);
        if target_team == Some(team) {
            let Some(health) = world.get::<Health>(*target).copied() else {
                continue;
            };
            if health.health >= health.max_health - 0.001 {
                continue;
            }
            let same_type = unit_type.is_some()
                && world.get::<UnitTypeComp>(*target).map(|comp| comp.type_id) == unit_type;
            let heal_mult = if same_type {
                spec.same_type_heal_mult
            } else {
                1.0
            };
            let heal = spec.heal_percent / 100.0 * health.max_health * heal_mult;
            heal_health(world, *target, heal);
        } else {
            if let Some(status) = status {
                apply_status(world, *target, status, spec.duration);
            }
            damage_entity(world, content, *target, scaled_damage, false, 1.0);
        }
    }
}

/// `LiquidRegenAbility.update`.
fn liquid_regen_tick(
    world: &mut World,
    entity: Entity,
    liquid: LiquidId,
    x: f32,
    y: f32,
    hit_size: f32,
    flying: bool,
) {
    let damaged = world
        .get::<Health>(entity)
        .is_some_and(|health| health.health < health.max_health - 0.001);
    if !damaged || flying {
        return;
    }
    let ts = crate::config::TILESIZE as f32;
    let tx = crate::world::WorldGrid::to_tile(x) as i16;
    let ty = crate::world::WorldGrid::to_tile(y) as i16;
    let rad = ((hit_size / ts * 0.6) as i32).max(1);
    let mut healed = 0.0f32;
    for dx in -rad..=rad {
        for dy in -rad..=rad {
            if dx * dx + dy * dy > rad * rad {
                continue;
            }
            let Some(puddle_entity) = puddles::find_at(world, tx + dx as i16, ty + dy as i16)
            else {
                continue;
            };
            if let Some(mut puddle) = world.get_mut::<PuddleState>(puddle_entity)
                && puddle.liquid == liquid
                && puddle.amount > 0.0
            {
                let taken = puddle.amount.min(SLURP_SPEED);
                puddle.amount -= taken;
                healed += taken * REGEN_PER_SLURP;
            }
        }
    }
    if healed > 0.0 {
        heal_health(world, entity, healed);
    }
}

/// Distance squared from an entity to `(x, y)` (`Entity.dst2`).
fn dst2(world: &World, entity: Entity, x: f32, y: f32) -> f32 {
    world
        .get::<Pos>(entity)
        .map(|pos| (pos.x - x).powi(2) + (pos.y - y).powi(2))
        .unwrap_or(f32::MAX)
}

/// Enemy buildings within `radius` as `(dst2, entity)`, stable-ordered.
fn buildings_in_radius(world: &World, x: f32, y: f32, radius: f32) -> Vec<(f32, Entity)> {
    let radius2 = radius * radius;
    let mut out: Vec<(f32, Entity)> = world
        .iter_entities()
        .filter_map(|entity_ref| {
            let building = entity_ref.get::<Building>()?;
            entity_ref.get::<Health>()?;
            let (bx, by) = crate::world::BuildHarness::tile_center(
                building.tile.x() as i32,
                building.tile.y() as i32,
            );
            let dst2 = (bx - x).powi(2) + (by - y).powi(2);
            (dst2 <= radius2).then_some((dst2, entity_ref.id()))
        })
        .collect();
    out.sort_by_key(|(dst2, entity)| (dst2.to_bits(), entity.index()));
    out
}

/// Whether any weapon mount is currently shooting (`Unit.isShooting`).
fn device_is_shooting(world: &World, entity: Entity) -> bool {
    world
        .get::<crate::weapons::UnitWeapons>(entity)
        .is_some_and(|weapons| weapons.mounts.iter().any(|mount| mount.shoot))
}

/// All bullet entities as `(EntitySeq, entity)`, unsorted.
fn bullet_entities(world: &World) -> Vec<(u64, Entity)> {
    world
        .iter_entities()
        .filter(|entity_ref| entity_ref.get::<Bullet>().is_some())
        .map(|entity_ref| {
            let seq = entity_ref
                .get::<crate::ecs::EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            (seq, entity_ref.id())
        })
        .collect()
}

/// `ForceFieldAbility.update` bullet shield: absorb absorbable enemy bullets
/// inside the polygon, draining `shield` by `b.type.shieldDamage(b)`.
fn absorb_bullets_force_field(
    world: &mut World,
    content: &ContentRegistry,
    spec: &AbilitySpec,
    x: f32,
    y: f32,
    team: u8,
    shield: &mut f32,
) -> usize {
    let mut bullets = bullet_entities(world);
    bullets.sort_by_key(|(seq, bullet)| (*seq, bullet.index()));
    let mut absorbed = 0usize;
    for (_, bullet_entity) in bullets {
        if *shield <= 0.0 {
            break;
        }
        let Some(bullet) = world.get::<Bullet>(bullet_entity).cloned() else {
            continue;
        };
        if bullet.flags & (ABSORBED | crate::combat::bullet::HIT) != 0 {
            continue;
        }
        let Some(def) = content.bullet(bullet.def) else {
            continue;
        };
        if !def.absorbable {
            continue;
        }
        let bullet_team = world
            .get::<TeamComp>(bullet_entity)
            .map(|comp| comp.team)
            .unwrap_or(0);
        if bullet_team == team {
            continue;
        }
        let Some(pos) = world.get::<Pos>(bullet_entity).copied() else {
            continue;
        };
        if !is_in_regular_polygon(spec.sides, x, y, spec.range, spec.rotation, pos.x, pos.y) {
            continue;
        }
        if let Some(mut stored) = world.get_mut::<Bullet>(bullet_entity) {
            stored.flags |= ABSORBED;
        }
        *shield -= bullet.damage * def.shield_damage_multiplier;
        absorbed += 1;
    }
    absorbed
}

/// `ShieldArcAbility.update` bullet shield: absorb or deflect bullets in the
/// arc, draining `data` by the def's shield damage.
#[allow(clippy::too_many_arguments)]
fn shield_arc_absorb(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    spec: &AbilitySpec,
    state: &mut AbilityState,
    x: f32,
    y: f32,
    team: u8,
) {
    let rotation = unit_rotation(world, entity);
    let (rx, ry) = rotate_offset(x, y, spec.x, spec.y, rotation);
    let inner = spec.range - spec.width;
    let outer = spec.range + spec.width;
    let half = spec.angle / 2.0;
    let mut bullets = bullet_entities(world);
    bullets.sort_by_key(|(seq, bullet)| (*seq, bullet.index()));
    for (_, bullet_entity) in bullets {
        if state.data <= 0.0 {
            break;
        }
        let Some(bullet) = world.get::<Bullet>(bullet_entity).cloned() else {
            continue;
        };
        if bullet.flags & (ABSORBED | crate::combat::bullet::HIT) != 0 {
            continue;
        }
        let Some(def) = content.bullet(bullet.def) else {
            continue;
        };
        if !def.absorbable {
            continue;
        }
        let bullet_team = world
            .get::<TeamComp>(bullet_entity)
            .map(|comp| comp.team)
            .unwrap_or(0);
        if bullet_team == team {
            continue;
        }
        let Some(pos) = world.get::<Pos>(bullet_entity).copied() else {
            continue;
        };
        let vel = world
            .get::<Vel>(bullet_entity)
            .copied()
            .unwrap_or(Vel { x: 0.0, y: 0.0 });
        let current = (pos.x, pos.y);
        let last = bullet.last;
        let next = (pos.x + vel.x, pos.y + vel.y);
        let within = |px: f32, py: f32, radius: f32| {
            let dx = px - rx;
            let dy = py - ry;
            dx * dx + dy * dy < radius * radius
        };
        // Inner exclusion (already inside the arc volume) and outer reach
        // (`ShieldArcAbility.shieldConsumer`).
        if within(current.0, current.1, inner) && within(last.0, last.1, inner) {
            continue;
        }
        if !(within(next.0, next.1, outer) || within(current.0, current.1, outer)) {
            continue;
        }
        let angle_of = |px: f32, py: f32| (py - ry).atan2(px - rx).to_degrees();
        if !(angles_within(angle_of(current.0, current.1), rotation, half)
            || angles_within(angle_of(next.0, next.1), rotation, half))
        {
            continue;
        }

        let speed = (vel.x * vel.x + vel.y * vel.y).sqrt();
        let chance = spec.chance_deflect;
        let deflect = chance > 0.0
            && speed >= 0.1
            && def.reflectable
            && (chance >= 1.0 || ability_chance(state) < chance);
        if deflect {
            // `b.trns(-b.vel.x, -b.vel.y)`: move back to the collision frame.
            let mut shifted = pos;
            shifted.x -= vel.x;
            shifted.y -= vel.y;
            if let Some(mut stored) = world.get_mut::<Pos>(bullet_entity) {
                *stored = shifted;
            }
            let nx0 = shifted.x - rx;
            let ny0 = shifted.y - ry;
            let nlen = (nx0 * nx0 + ny0 * ny0).sqrt();
            let (nx, ny) = if nlen > 0.0001 {
                (nx0 / nlen, ny0 / nlen)
            } else {
                (0.0, 0.0)
            };
            let dot = vel.x * nx + vel.y * ny;
            let rxx = vel.x - 2.0 * dot * nx;
            let ryy = vel.y - 2.0 * dot * ny;
            let out_dot = rxx * nx + ryy * ny;
            let normal_x = out_dot * nx;
            let normal_y = out_dot * ny;
            let tang_x = rxx - normal_x;
            let tang_y = ryy - normal_y;
            // `ShieldArcAbility.reflectVel` default is 1.
            if let Some(mut stored) = world.get_mut::<Vel>(bullet_entity) {
                stored.x = normal_x + tang_x;
                stored.y = normal_y + tang_y;
            }
            if let Some(mut stored) = world.get_mut::<Bullet>(bullet_entity) {
                stored.time = stored.lifetime * SHIELD_ARC_REFLECT_TIME;
            }
            if let Some(mut stored) = world.get_mut::<TeamComp>(bullet_entity) {
                stored.team = team;
            }
        } else if let Some(mut stored) = world.get_mut::<Bullet>(bullet_entity) {
            stored.flags |= ABSORBED;
        }

        if state.data <= bullet.damage {
            state.data -= spec.cooldown * spec.regen;
        }
        state.data -= bullet.damage * def.shield_damage_multiplier;
    }
}

/// Runs `Ability.death` hooks for `SpawnDeathAbility`/`LiquidExplodeAbility`.
///
/// `seq` is the caller's deterministic entity-sequence allocator (the harness
/// passes its own; the live loop must pass the sim sequencer).
#[allow(clippy::too_many_arguments)]
pub fn run_death_abilities(
    world: &mut World,
    content: &ContentRegistry,
    tiles: &Tiles,
    rng: &mut SimRng,
    seq: &mut u64,
    entity: Entity,
) {
    let Some(type_id) = world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id) else {
        return;
    };
    let Some(def) = content.unit(type_id) else {
        return;
    };
    let Some((x, y, team, hit_size, _)) = unit_frame(world, entity) else {
        return;
    };
    let abilities: Vec<AbilitySpec> = def.abilities.clone();
    for spec in abilities {
        match spec.kind {
            AbilityKind::SpawnDeath => spawn_death(world, content, rng, seq, &spec, x, y, team),
            AbilityKind::LiquidExplode => {
                liquid_explode(world, content, tiles, rng, &spec, x, y, hit_size)
            }
            _ => {}
        }
    }
}

/// `SpawnDeathAbility.death`.
#[allow(clippy::too_many_arguments)]
fn spawn_death(
    world: &mut World,
    content: &ContentRegistry,
    rng: &mut SimRng,
    seq: &mut u64,
    spec: &AbilitySpec,
    x: f32,
    y: f32,
    team: u8,
) {
    let Some(name) = spec.unit else {
        return;
    };
    let extra = rng.range(RngStream::Sim, 0.0, spec.rand_amount.max(0) as f32) as i32;
    let spawned = spec.amount as i32 + extra;
    for _ in 0..spawned.max(0) {
        let angle = rng.range(RngStream::Sim, 0.0, 360.0);
        let dist = rng.range(RngStream::Sim, 0.0, spec.spread);
        let rad = angle.to_radians();
        let sx = x + rad.cos() * dist;
        let sy = y + rad.sin() * dist;
        let current = *seq;
        *seq = seq.wrapping_add(1);
        crate::entities::comp::unit::lifecycle::spawn_unit(
            world, content, current, name, team, sx, sy, angle,
        );
    }
}

/// `LiquidExplodeAbility.death`.
#[allow(clippy::too_many_arguments)]
fn liquid_explode(
    world: &mut World,
    content: &ContentRegistry,
    tiles: &Tiles,
    rng: &mut SimRng,
    spec: &AbilitySpec,
    x: f32,
    y: f32,
    hit_size: f32,
) {
    let Some(name) = spec.liquid else {
        return;
    };
    let Some(liquid) = content.liquid_by_name(name) else {
        return;
    };
    let ts = crate::config::TILESIZE as f32;
    let tx = crate::world::WorldGrid::to_tile(x);
    let ty = crate::world::WorldGrid::to_tile(y);
    let rad = ((hit_size / ts * LIQUID_EXPLODE_RAD_SCALE) as i32).max(1);
    let real_noise = hit_size / LIQUID_EXPLODE_NOISE_MAG;
    for dx in -rad..=rad {
        for dy in -rad..=rad {
            let noise = crate::math::noise::noise2d(
                0,
                2,
                0.5,
                (1.0 / LIQUID_EXPLODE_NOISE_SCL) as f64,
                (dx + tx) as f64,
                (dy + ty) as f64,
            );
            let allowance = (rad * rad) as f32 - noise * real_noise * real_noise;
            if (dx * dx + dy * dy) as f32 > allowance {
                continue;
            }
            let dst = ((dx * dx + dy * dy) as f32).sqrt();
            let scaling = (1.0 - dst / rad as f32) * LIQUID_EXPLODE_RAD_AMOUNT_SCALE;
            puddles::deposit(
                world,
                tiles,
                content,
                (tx + dx) as i16,
                (ty + dy) as i16,
                liquid.id,
                LIQUID_EXPLODE_AMOUNT * scaling,
                rng,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::content::registries::units::ability::AbilityKind;
    use crate::entities::comp::unit::comp::StatusComp;
    use crate::entities::comp::unit::queries;

    fn ability_index(harness: &UnitHarness, unit: &str, kind: AbilityKind) -> usize {
        harness
            .content()
            .unit_by_name(unit)
            .and_then(|def| def.abilities.iter().position(|spec| spec.kind == kind))
            .expect("unit ability")
    }

    fn spec_of(harness: &UnitHarness, unit: &str, kind: AbilityKind) -> AbilitySpec {
        harness
            .content()
            .unit_by_name(unit)
            .and_then(|def| def.abilities.iter().find(|spec| spec.kind == kind))
            .expect("unit ability")
            .clone()
    }

    #[test]
    fn force_field_starts_full_and_regenerates() {
        let mut harness = UnitHarness::new(32, 32, 7);
        let unit = harness.spawn("quasar", 0, 64.0, 64.0, 0.0).expect("quasar");
        let max = spec_of(&harness, "quasar", AbilityKind::ForceField).max;
        assert_eq!(
            harness.build.world.get::<ShieldComp>(unit).unwrap().shield,
            max,
            "ForceFieldAbility.created sets the shield to max"
        );
        harness
            .build
            .world
            .get_mut::<ShieldComp>(unit)
            .unwrap()
            .shield = 100.0;
        update_unit_abilities(&mut harness.build.world, &harness.build.content, unit);
        let shield = harness.build.world.get::<ShieldComp>(unit).unwrap().shield;
        assert!(
            (shield - (100.0 + 0.4)).abs() < 1e-4,
            "ForceFieldAbility.regen applied (100 -> {shield})"
        );
    }

    #[test]
    fn shield_regen_field_pulses_allies() {
        let mut harness = UnitHarness::new(32, 32, 7);
        let caster = harness
            .spawn("scepter", 0, 64.0, 64.0, 0.0)
            .expect("scepter");
        let ally = harness.spawn("dagger", 0, 80.0, 64.0, 0.0).expect("dagger");
        assert_eq!(
            harness.build.world.get::<ShieldComp>(ally).unwrap().shield,
            0.0
        );
        let index = ability_index(&harness, "scepter", AbilityKind::ShieldRegenField);
        let reload = spec_of(&harness, "scepter", AbilityKind::ShieldRegenField).reload;
        harness
            .build
            .world
            .get_mut::<AbilityComp>(caster)
            .unwrap()
            .states[index]
            .timer = reload;
        update_unit_abilities(&mut harness.build.world, &harness.build.content, caster);
        let shield = harness.build.world.get::<ShieldComp>(ally).unwrap().shield;
        assert!(shield > 0.0, "ally shield pulsed ({shield})");
    }

    #[test]
    fn repair_field_heals_damaged_ally() {
        let mut harness = UnitHarness::new(32, 32, 7);
        let healer = harness.spawn("poly", 0, 64.0, 64.0, 0.0).expect("poly");
        let ally = harness.spawn("dagger", 0, 80.0, 64.0, 0.0).expect("dagger");
        harness.build.world.get_mut::<Health>(ally).unwrap().health = 50.0;
        let index = ability_index(&harness, "poly", AbilityKind::RepairField);
        let reload = spec_of(&harness, "poly", AbilityKind::RepairField).reload;
        harness
            .build
            .world
            .get_mut::<AbilityComp>(healer)
            .unwrap()
            .states[index]
            .timer = reload;
        update_unit_abilities(&mut harness.build.world, &harness.build.content, healer);
        let health = harness.build.world.get::<Health>(ally).unwrap().health;
        assert!(
            health > 50.0,
            "repair field healed the ally (50 -> {health})"
        );
    }

    #[test]
    fn status_field_applies_status_to_allies() {
        let mut harness = UnitHarness::new(32, 32, 7);
        crate::combat::damage::status::install_status_applier(
            &mut harness.build.world,
            &harness.build.content,
        );
        let caster = harness.spawn("oxynoe", 0, 64.0, 64.0, 0.0).expect("oxynoe");
        let ally = harness.spawn("dagger", 0, 80.0, 64.0, 0.0).expect("dagger");
        let index = ability_index(&harness, "oxynoe", AbilityKind::StatusField);
        let reload = spec_of(&harness, "oxynoe", AbilityKind::StatusField).reload;
        harness
            .build
            .world
            .get_mut::<AbilityComp>(caster)
            .unwrap()
            .states[index]
            .timer = reload;
        update_unit_abilities(&mut harness.build.world, &harness.build.content, caster);
        let overclock = harness.content().status_id("overclock").expect("overclock");
        assert!(
            harness
                .build
                .world
                .get::<StatusComp>(ally)
                .expect("status")
                .has_effect_of(overclock),
            "StatusFieldAbility applied overclock"
        );
    }

    #[test]
    fn spawn_death_spawns_units() {
        let mut harness = UnitHarness::new(32, 32, 7);
        let unit = harness.spawn("latum", 0, 64.0, 64.0, 0.0).expect("latum");
        assert_eq!(queries::all(&mut harness.build.world).len(), 1);
        let mut seq = harness.seq;
        let killed = crate::entities::comp::unit::lifecycle::kill_unit_with_abilities(
            &mut harness.build.world,
            &harness.build.content,
            &harness.build.grid.tiles,
            &mut harness.rng,
            &mut seq,
            unit,
        );
        assert!(killed);
        let spawned = queries::all(&mut harness.build.world);
        assert_eq!(
            spawned.len(),
            5,
            "SpawnDeathAbility(renale, 5) spawned the death units"
        );
    }

    #[test]
    fn liquid_regen_slurps_neoplasm() {
        let mut harness = UnitHarness::new(32, 32, 7);
        let unit = harness.spawn("renale", 0, 64.0, 64.0, 0.0).expect("renale");
        harness.build.world.get_mut::<Health>(unit).unwrap().health = 50.0;
        let neoplasm = harness.content().liquid_id("neoplasm").expect("neoplasm");
        let tx = crate::world::WorldGrid::to_tile(64.0) as i16;
        let ty = crate::world::WorldGrid::to_tile(64.0) as i16;
        harness.build.world.spawn(PuddleState {
            tile: (tx, ty),
            liquid: neoplasm,
            amount: 50.0,
            accepting: 0.0,
            update_time: 0.0,
            last_ripple: 0.0,
        });
        update_unit_abilities(&mut harness.build.world, &harness.build.content, unit);
        let health = harness.build.world.get::<Health>(unit).unwrap().health;
        assert!(
            health > 50.0,
            "LiquidRegenAbility slurped the neoplasm puddle (50 -> {health})"
        );
    }
}
