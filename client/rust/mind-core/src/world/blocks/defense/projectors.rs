// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `OverdriveProjector`, `RegenProjector`, `ShockwaveTower` and `BaseShield`
//! state machines (`world/blocks/defense/{OverdriveProjector,RegenProjector,
//! ShockwaveTower,BaseShield}.java`).
//!
//! These are the M7 projector/projector-adjacent blocks the plan-07
//! [`BuildingBehavior`](crate::world::behavior::BuildingBehavior) tick drives
//! without a content handle. Content-dependent bullet effects (shield
//! absorption multipliers, `Fx`) stay on the plan-10 combat pass.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::{Building, Health, Pos, TeamComp};

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

/// `BuildingComp.applyBoost`: raise `timeScale` for `duration` ticks.
pub fn apply_boost(world: &mut World, e: Entity, boost: f32, duration: f32) {
    if let Some(mut building) = world.get_mut::<Building>(e) {
        building.time_scale = building.time_scale.max(boost);
        building.time_scale_duration = building.time_scale_duration.max(duration);
    }
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
            apply_boost(world, target, boost, state.reload + 1.0);
        }
    }

    if efficiency > 0.0 {
        state.use_progress += 1.0;
    }
    if state.use_progress >= state.use_time {
        state.use_progress %= state.use_time;
    }
}

/// Buildings (excluding `origin`) within `range` world units of `origin`.
pub fn buildings_in_range(world: &World, origin: Entity, range: f32) -> Vec<Entity> {
    let Some(pos) = world.get::<Pos>(origin).copied() else {
        return Vec::new();
    };
    let range2 = range.max(0.0).powi(2);
    let mut out = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Building>().is_none() {
            continue;
        }
        if entity_ref.id() == origin {
            continue;
        }
        let Some(target_pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dx = target_pos.x - pos.x;
        let dy = target_pos.y - pos.y;
        if dx * dx + dy * dy <= range2 {
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

/// `BaseShieldBuild.updateTile` (`delta == 1`): grow toward `radius *
/// efficiency` and absorb enemy bullets passing through.
pub fn update_base_shield(
    world: &mut World,
    e: Entity,
    state: &mut BaseShieldState,
    efficiency: f32,
) {
    state.smooth_radius = lerp_delta(state.smooth_radius, state.radius * efficiency, 0.05);
    let radius = state.smooth_radius;
    if radius <= 1.0 {
        return;
    }
    let Some(pos) = world.get::<Pos>(e).copied() else {
        return;
    };
    let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
    let radius2 = radius * radius;
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
        if dx * dx + dy * dy <= radius2 {
            targets.push(entity_ref.id());
        }
    }
    for target in targets {
        let _ = world.despawn(target);
    }
}
