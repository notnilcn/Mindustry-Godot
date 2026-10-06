// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Defense blocks (M7): `ForceProjector`, `MendProjector`, `ShieldWall`,
//! `ShockMine`, `TargetDummy` and the regular-polygon shield geometry from
//! `arc.math.geom.Intersector.isInRegularPolygon`.
//!
//! Ported from `world/blocks/defense/{ForceProjector,MendProjector,ShieldWall,
//! ShockMine,TargetDummy}.java`. The behaviors are pure state machines driven
//! through the plan-10 [`CombatHarness`](crate::combat::CombatHarness); the
//! `BuildingBehavior` registration is the plan-07 seam (R-10-1), so these
//! fixtures are spawned by the harness like the turret engine.

use std::f32::consts::PI;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::combat::bullet::CombatCtx;
use crate::entities::comp::{Health, Pos, TeamComp};

/// `Intersector.isInRegularPolygon(sides, x, y, radius, rotation, px, py)`.
///
/// `rotation` is in degrees. Points outside the circumscribed circle are
/// rejected first; the edge distance is `apothem / cos(rel)`.
pub fn is_in_regular_polygon(
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
    let dx = px - x;
    let dy = py - y;
    let dist2 = dx * dx + dy * dy;
    if dist2 > radius * radius {
        return false;
    }
    if dist2 <= 1e-8 {
        return true;
    }
    let step = 2.0 * PI / sides as f32;
    let phi = dy.atan2(dx);
    let rel = (phi - rotation.to_radians()).rem_euclid(step) - step / 2.0;
    let edge = radius * (step / 2.0).cos() / rel.cos();
    dist2.sqrt() <= edge + 1e-4
}

fn lerp_delta(from: f32, to: f32, alpha: f32) -> f32 {
    from + (to - from) * alpha.clamp(0.0, 1.0)
}

fn position(world: &bevy_ecs::world::World, e: Entity) -> Option<(f32, f32)> {
    world.get::<Pos>(e).map(|p| (p.x, p.y))
}

// ---------------------------------------------------------------------------
// ForceProjector
// ---------------------------------------------------------------------------

/// `ForceBuild` state (`world/blocks/defense/ForceProjector.java`).
#[derive(Debug, Clone, Component)]
pub struct ForceProjectorState {
    /// `ForceProjector.radius`.
    pub radius: f32,
    /// `ForceProjector.sides`.
    pub sides: i32,
    /// `ForceProjector.shieldRotation` (degrees).
    pub shield_rotation: f32,
    /// `ForceProjector.shieldHealth`.
    pub shield_health: f32,
    /// `ForceProjector.cooldownNormal` (buildup decay/tick).
    pub cooldown_normal: f32,
    /// `ForceProjector.cooldownLiquid`.
    pub cooldown_liquid: f32,
    /// `ForceProjector.cooldownBrokenBase`.
    pub cooldown_broken_base: f32,
    /// `ForceProjector.phaseRadiusBoost`.
    pub phase_radius_boost: f32,
    /// `ForceProjector.phaseShieldBoost`.
    pub phase_shield_boost: f32,
    /// `ForceProjector.crashDamageMultiplier`.
    pub crash_damage_multiplier: f32,
    /// `ForceBuild.broken`.
    pub broken: bool,
    /// `ForceBuild.buildup`.
    pub buildup: f32,
    /// `ForceBuild.radscl`.
    pub radscl: f32,
    /// `ForceBuild.hit`.
    pub hit: f32,
    /// `ForceBuild.warmup`.
    pub warmup: f32,
    /// `ForceBuild.phaseHeat`.
    pub phase_heat: f32,
}

impl Default for ForceProjectorState {
    fn default() -> Self {
        Self {
            radius: 101.7,
            sides: 6,
            shield_rotation: 0.0,
            shield_health: 700.0,
            cooldown_normal: 1.75,
            cooldown_liquid: 1.5,
            cooldown_broken_base: 0.35,
            phase_radius_boost: 80.0,
            phase_shield_boost: 400.0,
            crash_damage_multiplier: 2.0,
            broken: true,
            buildup: 0.0,
            radscl: 0.0,
            hit: 0.0,
            warmup: 0.0,
            phase_heat: 0.0,
        }
    }
}

impl ForceProjectorState {
    /// `ForceBuild.realRadius()`.
    pub fn real_radius(&self) -> f32 {
        (self.radius + self.phase_heat * self.phase_radius_boost) * self.radscl
    }

    /// Plan-16 draw state (`Layer::shields`); view-only.
    pub fn draw_state(
        &self,
        x: f32,
        y: f32,
        color: crate::content::Rgba,
    ) -> crate::combat::view::ShieldDrawState {
        crate::combat::view::ShieldDrawState {
            x,
            y,
            radius: self.real_radius(),
            sides: self.sides,
            rotation: self.shield_rotation,
            color,
            hit: self.hit > 0.0,
            broken: self.broken,
        }
    }

    /// `ForceBuild.absorbExplosion` (`ExplosionShield`).
    pub fn absorb_explosion(&mut self, x: f32, y: f32, ex: f32, ey: f32, damage: f32) -> bool {
        let absorb = !self.broken
            && is_in_regular_polygon(
                self.sides,
                x,
                y,
                self.real_radius(),
                self.shield_rotation,
                ex,
                ey,
            );
        if absorb {
            self.hit = 1.0;
            self.buildup += damage * self.crash_damage_multiplier;
        }
        absorb
    }
}

/// `ForceBuild.updateTile` state half (`delta == 1`), without the bullet pass.
///
/// `coolant_heat_capacity` is the active coolant's `heatCapacity` (`0` = none),
/// supplied by plan 09 through the consumer pass.
pub fn update_force_projector_state(
    world: &World,
    e: Entity,
    state: &mut ForceProjectorState,
    eff: f32,
    phase_valid: bool,
    coolant_heat_capacity: f32,
) {
    if position(world, e).is_none() {
        return;
    }
    state.phase_heat = lerp_delta(state.phase_heat, if phase_valid { 1.0 } else { 0.0 }, 0.1);
    state.warmup = lerp_delta(state.warmup, eff, 0.1);
    state.radscl = lerp_delta(
        state.radscl,
        if state.broken { 0.0 } else { state.warmup },
        0.05,
    );

    if state.buildup > 0.0 {
        let mut scale = if !state.broken {
            state.cooldown_normal
        } else {
            state.cooldown_broken_base
        };
        if coolant_heat_capacity > 0.0 {
            scale *= state.cooldown_liquid * (1.0 + (coolant_heat_capacity - 0.4) * 0.9);
        }
        state.buildup -= scale;
        if state.buildup < 0.0 {
            state.buildup = 0.0;
        }
    }
    if state.broken && state.buildup <= 0.0 {
        state.broken = false;
    }
    if state.buildup >= state.shield_health + state.phase_shield_boost * state.phase_heat
        && !state.broken
    {
        state.broken = true;
        state.buildup = state.shield_health;
    }
    if state.hit > 0.0 {
        state.hit -= 1.0 / 5.0;
    }
}

/// `ForceBuild.updateTile` + `deflectBullets` (`delta == 1`).
///
/// `coolant_heat_capacity` is the active coolant's `heatCapacity` (`0` = none),
/// supplied by plan 09 through the consumer pass.
pub fn update_force_projector(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut ForceProjectorState,
    team: u8,
    eff: f32,
    phase_valid: bool,
    coolant_heat_capacity: f32,
) {
    if position(ctx.world, e).is_none() {
        return;
    }
    update_force_projector_state(ctx.world, e, state, eff, phase_valid, coolant_heat_capacity);
    let Some((x, y)) = position(ctx.world, e) else {
        return;
    };
    deflect_bullets(ctx, state, team, x, y);
}

/// `ForceBuild.deflectBullets` over the live bullet set.
fn deflect_bullets(
    ctx: &mut CombatCtx<'_>,
    state: &mut ForceProjectorState,
    team: u8,
    x: f32,
    y: f32,
) {
    let radius = state.real_radius();
    if radius <= 0.0 || state.broken {
        return;
    }
    let bullets: Vec<Entity> = ctx
        .world
        .iter_entities()
        .filter(|entity_ref| entity_ref.contains::<crate::combat::bullet::Bullet>())
        .map(|entity_ref| entity_ref.id())
        .collect();
    for bullet in bullets {
        if ctx.world.get::<TeamComp>(bullet).map(|t| t.team) == Some(team) {
            continue;
        }
        let (absorbable, damage) = {
            let Some(b) = ctx.world.get::<crate::combat::bullet::Bullet>(bullet) else {
                continue;
            };
            let Some(def) = ctx.content.bullet(b.def) else {
                continue;
            };
            (
                def.absorbable && !b.has(crate::combat::bullet::ABSORBED),
                b.damage * def.shield_damage_multiplier,
            )
        };
        if !absorbable {
            continue;
        }
        let Some((bx, by)) = position(ctx.world, bullet) else {
            continue;
        };
        if is_in_regular_polygon(state.sides, x, y, radius, state.shield_rotation, bx, by) {
            if let Some(mut b) = ctx.world.get_mut::<crate::combat::bullet::Bullet>(bullet) {
                b.set(crate::combat::bullet::ABSORBED);
            }
            state.hit = 1.0;
            state.buildup += damage;
        }
    }
}

// ---------------------------------------------------------------------------
// ShieldWall
// ---------------------------------------------------------------------------

/// `ShieldWallBuild` state (`world/blocks/defense/ShieldWall.java`).
#[derive(Debug, Clone, Component)]
pub struct ShieldWallState {
    /// `ShieldWall.shieldHealth`.
    pub shield_health: f32,
    /// `ShieldWall.breakCooldown`.
    pub break_cooldown: f32,
    /// `ShieldWall.regenSpeed`.
    pub regen_speed: f32,
    /// `ShieldWallBuild.shield`.
    pub shield: f32,
    /// `ShieldWallBuild.shieldRadius`.
    pub shield_radius: f32,
    /// `ShieldWallBuild.breakTimer`.
    pub break_timer: f32,
    /// `hit`.
    pub hit: f32,
}

impl Default for ShieldWallState {
    fn default() -> Self {
        Self {
            shield_health: 900.0,
            break_cooldown: 600.0,
            regen_speed: 2.0,
            shield: 900.0,
            shield_radius: 0.0,
            break_timer: 0.0,
            hit: 0.0,
        }
    }
}

impl ShieldWallState {
    /// `ShieldWallBuild.broken()` (`canConsume` supplied by `powered`).
    pub fn broken(&self, powered: bool) -> bool {
        self.break_timer > 0.0 || !powered
    }

    /// `ShieldWallBuild.damage(float)`; returns the overflow passed to `super`.
    pub fn damage(&mut self, damage: f32) -> f32 {
        let broken = self.broken(true);
        let shield_taken = if broken { 0.0 } else { self.shield.min(damage) };
        self.shield -= shield_taken;
        if shield_taken > 0.0 {
            self.hit = 1.0;
        }
        if self.shield <= 0.00001 && shield_taken > 0.0 {
            self.break_timer = self.break_cooldown;
        }
        (damage - shield_taken).max(0.0)
    }
}

/// `ShieldWallBuild.updateTile` (`delta == 1`).
pub fn update_shield_wall(state: &mut ShieldWallState, eff: f32, powered: bool) {
    if state.break_timer > 0.0 {
        state.break_timer -= 1.0;
    } else {
        state.shield = (state.shield + state.regen_speed * eff).clamp(0.0, state.shield_health);
    }
    if state.hit > 0.0 {
        state.hit -= 0.1;
        state.hit = state.hit.max(0.0);
    }
    state.shield_radius = lerp_delta(
        state.shield_radius,
        if state.broken(powered) { 0.0 } else { 1.0 },
        0.12,
    );
}

// ---------------------------------------------------------------------------
// MendProjector
// ---------------------------------------------------------------------------

/// `MendBuild` state (`world/blocks/defense/MendProjector.java`).
#[derive(Debug, Clone, Component)]
pub struct MendProjectorState {
    /// `MendProjector.reload`.
    pub reload: f32,
    /// `MendProjector.range`.
    pub range: f32,
    /// `MendProjector.healPercent`.
    pub heal_percent: f32,
    /// `MendProjector.phaseBoost`.
    pub phase_boost: f32,
    /// `MendProjector.phaseRangeBoost`.
    pub phase_range_boost: f32,
    /// `MendBuild.heat`.
    pub heat: f32,
    /// `MendBuild.charge`.
    pub charge: f32,
    /// `MendBuild.phaseHeat`.
    pub phase_heat: f32,
    /// `MendBuild.smoothEfficiency`.
    pub smooth_efficiency: f32,
}

impl Default for MendProjectorState {
    fn default() -> Self {
        Self {
            reload: 250.0,
            range: 60.0,
            heal_percent: 12.0,
            phase_boost: 12.0,
            phase_range_boost: 50.0,
            heat: 0.0,
            charge: 0.0,
            phase_heat: 0.0,
            smooth_efficiency: 0.0,
        }
    }
}

/// `MendBuild.updateTile` (`delta == 1`).
pub fn update_mend_projector(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut MendProjectorState,
    eff: f32,
    can_heal: bool,
    phase_valid: bool,
) {
    update_mend_projector_world(ctx.world, e, state, eff, can_heal, phase_valid);
}

/// [`update_mend_projector`] over a plain `&mut World` (normal behavior tick).
pub fn update_mend_projector_world(
    world: &mut World,
    e: Entity,
    state: &mut MendProjectorState,
    eff: f32,
    can_heal: bool,
    phase_valid: bool,
) {
    let Some((x, y)) = position(world, e) else {
        return;
    };
    state.smooth_efficiency = lerp_delta(state.smooth_efficiency, eff, 0.08);
    state.heat = lerp_delta(
        state.heat,
        if eff > 0.0 && can_heal { 1.0 } else { 0.0 },
        0.08,
    );
    state.charge += state.heat;
    state.phase_heat = lerp_delta(state.phase_heat, if phase_valid { 1.0 } else { 0.0 }, 0.1);

    if state.charge < state.reload || !can_heal {
        return;
    }
    state.charge = 0.0;
    let real_range = state.range + state.phase_heat * state.phase_range_boost;
    let range2 = real_range * real_range;
    let amount_percent = (state.heal_percent + state.phase_heat * state.phase_boost) / 100.0;
    let targets: Vec<Entity> = world
        .iter_entities()
        .filter_map(|entity_ref| {
            let health = entity_ref.get::<Health>()?;
            entity_ref.get::<crate::entities::comp::Building>()?;
            if !health.damaged() {
                return None;
            }
            let pos = entity_ref.get::<Pos>()?;
            let dst2 = (pos.x - x).powi(2) + (pos.y - y).powi(2);
            if dst2 > range2 {
                return None;
            }
            Some(entity_ref.id())
        })
        .collect();
    for target in targets {
        if let Some(mut health) = world.get_mut::<Health>(target) {
            let heal = health.max_health * amount_percent * eff;
            health.health = (health.health + heal).min(health.max_health);
        }
    }
}

// ---------------------------------------------------------------------------
// ShockMine
// ---------------------------------------------------------------------------

/// `ShockMineBuild` state (`world/blocks/defense/ShockMine.java`).
#[derive(Debug, Clone, Component)]
pub struct ShockMineState {
    /// `ShockMine.cooldown`.
    pub cooldown: f32,
    /// `ShockMine.tileDamage`.
    pub tile_damage: f32,
    /// `ShockMine.damage`.
    pub damage: f32,
    /// `ShockMine.length`.
    pub length: i32,
    /// `ShockMine.tendrils`.
    pub tendrils: i32,
    /// `ShockMine.shots`.
    pub shots: i32,
    /// `timer(timerDamage, cooldown)` accumulator.
    pub timer_damage: f32,
}

impl Default for ShockMineState {
    fn default() -> Self {
        Self {
            cooldown: 80.0,
            tile_damage: 5.0,
            damage: 13.0,
            length: 10,
            tendrils: 6,
            shots: 6,
            timer_damage: 0.0,
        }
    }
}

/// `ShockMineBuild.unitOn(unit)`; returns `true` when the mine triggered.
pub fn shock_mine_unit_on(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut ShockMineState,
    team: u8,
    unit_team: u8,
) -> bool {
    if unit_team == team {
        return false;
    }
    if state.timer_damage > 0.0 {
        state.timer_damage -= 1.0;
        return false;
    }
    state.timer_damage = state.cooldown;
    let Some((x, y)) = position(ctx.world, e) else {
        return false;
    };
    // `Lightning.create(Bullets.damageLightningGround, ...)` per tendril.
    for _ in 0..state.tendrils.max(0) {
        let angle = ctx
            .rng
            .range(crate::determinism::RngStream::Sim, 0.0, 360.0);
        let _ = crate::combat::lightning::create(
            ctx,
            team,
            crate::content::Rgba::WHITE,
            state.damage,
            x,
            y,
            angle,
            state.length,
        );
    }
    true
}

// ---------------------------------------------------------------------------
// TargetDummy
// ---------------------------------------------------------------------------

/// `TargetDummyBuild` state (`world/blocks/defense/TargetDummy.java`).
#[derive(Debug, Clone, Component)]
pub struct TargetDummyState {
    /// `TargetDummyBuild.resetTime`.
    pub reset_time: f32,
    /// `TargetDummyBuild.total`.
    pub total: f32,
    /// `TargetDummyBuild.reset`.
    pub reset: f32,
    /// `TargetDummyBuild.time`.
    pub time: f32,
    /// `TargetDummyBuild.dps`.
    pub dps: f32,
    /// `TargetDummyBuild.hits`.
    pub hits: i32,
    /// `timer(dpsUpdateTime, 20)` accumulator (counts down to 0).
    pub dps_timer: f32,
    /// `TargetDummyBuild.unitArmor`.
    pub armor: f32,
}

impl Default for TargetDummyState {
    fn default() -> Self {
        Self {
            reset_time: 120.0,
            total: 0.0,
            reset: 120.0,
            time: 0.0,
            dps: 0.0,
            hits: 0,
            dps_timer: 20.0,
            armor: 0.0,
        }
    }
}

impl TargetDummyState {
    /// `TargetDummyBuild.dummyHit(damage)` (called from `HealthComp`).
    pub fn dummy_hit(&mut self, damage: f32) {
        self.reset = 0.0;
        self.total += damage;
        self.hits += 1;
    }
}

/// `TargetDummyBuild.updateTile` (`delta == 1`).
pub fn update_target_dummy(state: &mut TargetDummyState) {
    state.time += 1.0;
    state.reset += 1.0;
    state.dps_timer -= 1.0;
    if state.dps_timer <= 0.0 {
        state.dps = if state.time > 0.0 {
            state.total / state.time * 60.0
        } else {
            0.0
        };
        state.dps_timer = 20.0;
    }
    if state.reset >= state.reset_time {
        state.total = 0.0;
        state.time = 0.0;
        state.dps = 0.0;
        state.hits = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn regular_polygon_center_and_edges() {
        // Hexagon, radius 100, no rotation, vertex at angle 0. The boundary at
        // angle 0 (vertex direction) is the circumradius; at angle 30 it is the
        // apothem (86.6).
        assert!(is_in_regular_polygon(6, 0.0, 0.0, 100.0, 0.0, 0.0, 0.0));
        assert!(is_in_regular_polygon(6, 0.0, 0.0, 100.0, 0.0, 86.0, 0.0));
        // 90 degrees is an edge-midpoint direction, where the boundary is the
        // apothem (86.6), so 95 is outside even though it is inside the circle.
        assert!(!is_in_regular_polygon(6, 0.0, 0.0, 100.0, 0.0, 0.0, 95.0));
        assert!(!is_in_regular_polygon(6, 0.0, 0.0, 100.0, 0.0, 0.0, 88.0));
        // Vertex is on the circumscribed circle.
        assert!(is_in_regular_polygon(6, 0.0, 0.0, 100.0, 0.0, 100.0, 0.0));
        assert!(!is_in_regular_polygon(6, 0.0, 0.0, 100.0, 0.0, 101.0, 0.0));
    }

    #[test]
    fn force_projector_absorbs_bullet_and_builds_up() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let projector = harness.spawn_test_force_projector(16, 16, 1, 100.0);
        // Fully-grown shield so the test is independent of the radscl ramp.
        {
            let mut state = harness
                .build
                .world
                .get_mut::<ForceProjectorState>(projector)
                .unwrap();
            state.broken = false;
            state.radscl = 1.0;
            state.warmup = 1.0;
        }
        // A slow enemy bullet spawned inside the shield volume.
        let (x, y) = CombatHarness::tile_center(16, 17);
        let bullet = harness
            .spawn_bullet("fuse_slow", x, y, 0.0, 2)
            .expect("bullet");
        let mut peak = 0.0f32;
        for _ in 0..3 {
            harness.tick();
            let buildup = harness
                .build
                .world
                .get::<ForceProjectorState>(projector)
                .unwrap()
                .buildup;
            peak = peak.max(buildup);
        }
        assert!(peak > 0.0, "shield buildup increased (peak {peak})");
        assert!(
            harness.build.world.get_entity(bullet).is_err(),
            "absorbed bullet removed"
        );
    }

    #[test]
    fn force_projector_breaks_and_cools_down() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let projector = harness.spawn_test_force_projector(16, 16, 1, 100.0);
        {
            let mut state = harness
                .build
                .world
                .get_mut::<ForceProjectorState>(projector)
                .unwrap();
            state.shield_health = 30.0;
            state.broken = false;
            state.radscl = 1.0;
            state.warmup = 1.0;
        }
        let (x, y) = CombatHarness::tile_center(16, 17);
        // `fuse` deals 40 shield damage -> over the 30 HP shield, so it breaks.
        let _ = harness.spawn_bullet("fuse", x, y, 0.0, 2);
        for _ in 0..3 {
            harness.tick();
        }
        assert!(
            harness
                .build
                .world
                .get::<ForceProjectorState>(projector)
                .unwrap()
                .broken,
            "projector broke under fire"
        );
        // Broken shields cool at `cooldownBrokenBase`; unbreak when empty.
        for _ in 0..500 {
            harness.tick();
        }
        assert!(
            !harness
                .build
                .world
                .get::<ForceProjectorState>(projector)
                .unwrap()
                .broken,
            "projector unbreaks after cooldown"
        );
    }

    #[test]
    fn force_projector_explosion_absorb() {
        let mut state = ForceProjectorState {
            broken: false,
            radscl: 1.0,
            ..Default::default()
        };
        assert!(state.absorb_explosion(0.0, 0.0, 0.0, 0.0, 100.0));
        assert_eq!(state.buildup, 200.0);
        // Past the apothem on an edge-midpoint direction (angle 30 for a
        // hexagon at rotation 0) but still inside the circumcircle.
        let x = 95.0 * (30.0f32).to_radians().cos();
        let y = 95.0 * (30.0f32).to_radians().sin();
        assert!(!state.absorb_explosion(0.0, 0.0, x, y, 100.0));
    }

    #[test]
    fn shield_wall_splits_damage_and_breaks() {
        let mut wall = ShieldWallState::default();
        let overflow = wall.damage(300.0);
        assert_eq!(overflow, 0.0);
        assert_eq!(wall.shield, 600.0);
        let overflow = wall.damage(700.0);
        assert_eq!(overflow, 100.0, "overflow passes through");
        assert!(wall.break_timer > 0.0, "wall broke");
        // Regen after cooldown.
        for _ in 0..601 {
            update_shield_wall(&mut wall, 1.0, true);
        }
        assert!(wall.shield > 0.0);
    }

    #[test]
    fn mend_projector_heals_damaged_building() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let target = harness.build_at(10, 10).expect("building");
        harness
            .build
            .world
            .get_mut::<Health>(target)
            .unwrap()
            .health = 100.0;
        let mend = harness.spawn_test_mend_projector(10, 12, 1);
        // `reload` 250; set charge/heat near completion.
        {
            let mut state = harness
                .build
                .world
                .get_mut::<MendProjectorState>(mend)
                .unwrap();
            state.charge = 249.5;
            state.heat = 1.0;
        }
        for _ in 0..5 {
            harness.tick();
        }
        let healed = harness.build.world.get::<Health>(target).unwrap().health;
        assert!(healed > 100.0, "mend projector healed the wall: {healed}");
    }

    #[test]
    fn shock_mine_triggers_on_enemy_unit_with_cooldown() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let mine = harness.spawn_test_shock_mine(8, 8, 1);
        let (x, y) = CombatHarness::tile_center(9, 8);
        let unit = harness.spawn_test_unit(x, y, 2, Vec::new());
        let mut state = harness
            .build
            .world
            .entity_mut(mine)
            .take::<ShockMineState>()
            .unwrap();
        let first = shock_mine_unit_on(&mut harness.combat_ctx(), mine, &mut state, 1, 2);
        harness.claim_scratch_spawned();
        assert!(first, "mine triggered on enemy unit");
        // Immediate re-trigger is blocked by the cooldown.
        let second = shock_mine_unit_on(&mut harness.combat_ctx(), mine, &mut state, 1, 2);
        harness.claim_scratch_spawned();
        assert!(!second, "cooldown blocks immediate retrigger");
        harness.build.world.entity_mut(mine).insert(state);
        let _ = unit;
    }

    #[test]
    fn target_dummy_counts_hits_and_dps() {
        let mut state = TargetDummyState::default();
        state.dummy_hit(50.0);
        state.dummy_hit(50.0);
        assert_eq!(state.total, 100.0);
        assert_eq!(state.hits, 2);
        // 20 ticks -> dps update; time is sample denominator.
        for _ in 0..40 {
            update_target_dummy(&mut state);
        }
        // `reset` was cleared on the last hit then advanced 40 < resetTime 120.
        assert!(state.total > 0.0);
        for _ in 0..120 {
            update_target_dummy(&mut state);
        }
        assert_eq!(state.total, 0.0, "counters reset after resetTime");
        assert_eq!(state.hits, 0);
    }
}
