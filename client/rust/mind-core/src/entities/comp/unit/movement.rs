// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-kind movement/kinematics dispatch (plan 11 §3.6).
//!
//! Runs the kind-specific update bodies after the AI controller moved the unit:
//! leg IK (`LegsComp`), mech walk (`MechComp`), tank treads (`TankComp`), crawl
//! segment rotation (`CrawlComp`), naval trails (`WaterMoveComp`) and tether
//! validity. Mirrors the upstream `update()` merge order (plan 11 §3.3).

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::world::World;

use crate::ai::controller::{AiKind, ControllerSlot};
use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{
    BlockUnitComp, CrawlComp, HitboxComp, LegsComp, MechComp, PhysicsComp, StatusComp, TankComp,
    TargetDummyComp, UnitCore, UnitTypeComp, WaterMoveComp,
};
use crate::entities::comp::{Pos, Unit, Vel};
use crate::world::WorldGrid;

use super::crawl::update_crawl;
use super::legs::update_legs;
use super::mech::update_mech;
use super::tank::update_tank;
use super::tether::check_tether;
use super::water_move::update_water_move;

/// `UnitComp.speed()` (`UnitComp.java:190`) with `floorSpeedMultiplier`
/// (`UnitComp.java:107`) and the `CrawlComp`/`TankComp` overrides
/// (`CrawlComp.java:39`, `TankComp.java:117`).
///
/// `type.speed * strafePenalty * boost * floorSpeedMultiplier`, where the
/// floor term carries the status speed multiplier (`speedMultiplier`), the
/// floor's `speedMultiplier ^ type.floorMultiplier`, the deep-liquid crawl
/// penalty (0.45) and the kind slowdown (`lastCrawlSlowdown`/`lastSlowdown`).
pub fn unit_speed(world: &World, grid: &WorldGrid, content: &ContentRegistry, unit: Entity) -> f32 {
    let Some(physics) = world.get::<PhysicsComp>(unit) else {
        return 1.0;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(unit)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return physics.speed;
    };
    let core = world
        .get::<UnitCore>(unit)
        .copied()
        .unwrap_or(UnitCore::new(0.0));
    let status_mult = world
        .get::<StatusComp>(unit)
        .map(|status| status.speed_multiplier)
        .unwrap_or(1.0);

    // `isFlying() || type.hovering ? Blocks.air.asFloor() : floorOn()`.
    let flying = crate::combat::bullet::unit_is_flying(world, unit);
    let floor = if flying || def.hovering {
        status_mult
    } else {
        let floor_id = world
            .get::<Pos>(unit)
            .and_then(|pos| {
                grid.tiles
                    .getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y))
            })
            .map(|tile| tile.floor);
        let base = floor_id
            .and_then(|id| content.block(id))
            .map(|block| block.speed_multiplier)
            .unwrap_or(1.0);
        let deep = floor_id.is_some_and(|id| {
            world
                .get_resource::<crate::world::BlockTable>()
                .is_some_and(|table| crate::world::behavior::environment::floor_is_deep(table, id))
        });
        let mut on = base;
        let mut slow = 1.0f32;
        if let Some(crawl) = world.get::<CrawlComp>(unit) {
            // `CrawlComp.floorSpeedMultiplier`: deep floors use 0.45.
            if deep {
                on = 0.45;
            }
            slow = crawl.last_crawl_slowdown;
        } else if let Some(tank) = world.get::<TankComp>(unit) {
            slow = tank.last_slowdown;
        }
        on.powf(def.floor_multiplier) * status_mult * slow
    };

    // `strafePenalty`: airborne player units lose speed while strafing.
    let strafe = if world
        .get::<ControllerSlot>(unit)
        .is_some_and(|slot| slot.kind == AiKind::Player)
        && core.elevation >= 0.001
    {
        let vel = world
            .get::<Vel>(unit)
            .copied()
            .unwrap_or(Vel { x: 0.0, y: 0.0 });
        let vel_angle = vel.y.atan2(vel.x).to_degrees();
        let t = crate::weapons::angle_dist(vel_angle, core.rotation) / 180.0;
        1.0 + (def.strafe_penalty - 1.0) * t
    } else {
        1.0
    };
    // `boost = lerp(1, canBoost ? boostMultiplier : 1, elevation)`.
    let boost_mult = if physics.can_boost {
        physics.boost_multiplier
    } else {
        1.0
    };
    let boost = 1.0 + (boost_mult - 1.0) * core.elevation;

    physics.speed * strafe * boost * floor
}

/// `UnitComp.physicSize()` (`UnitComp.java:170-173`): `hitSize * 0.7`.
fn physic_size(world: &World, entity: Entity) -> f32 {
    world
        .get::<HitboxComp>(entity)
        .map_or(0.0, |hitbox| hitbox.hit_size)
        * 0.7
}

/// `UnitComp.isGrounded()` (`UnitComp.java:80-82`): `elevation < 0.001`.
///
/// This is the predicate `canLand` filters other units by. It is not the
/// negation of `isFlying()` (which is `elevation >= 0.09`, `UnitComp.java:84-86`):
/// a unit between the two thresholds is neither grounded nor flying.
fn is_grounded(world: &World, entity: Entity) -> bool {
    world
        .get::<UnitCore>(entity)
        .is_none_or(|core| core.elevation < 0.001)
}

/// `UnitComp.isFlying()` (`UnitComp.java:84-86`): `elevation >= 0.09`.
fn is_flying(world: &World, entity: Entity) -> bool {
    world
        .get::<UnitCore>(entity)
        .is_some_and(|core| core.elevation >= 0.09)
}

/// `PosComp.onSolid()` (`PosComp.java:64-67`): the tile under the unit is solid
/// (out-of-bounds tiles are treated as non-solid by this port).
fn on_solid(world: &World, grid: &WorldGrid, content: &ContentRegistry, entity: Entity) -> bool {
    world
        .get::<Pos>(entity)
        .and_then(|pos| {
            grid.tiles
                .getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y))
        })
        .is_some_and(|tile| crate::ai::pathfinder::path_tile::is_solid(content, tile.block))
}

/// `UnitComp.canLand()` (`UnitComp.java:176-179`): there is solid, unoccupied
/// ground under this unit.
///
/// Mirrors `!onSolid() && !Units.any(x - psize/2f, y - psize/2f, psize, psize,
/// f -> f != self() && f.isGrounded())` with `psize = physicSize()`. `Units.any`
/// is a `Groups.unit` quadtree intersect (`Units.java:429-431`); every unit's
/// tree hitbox is its centered `hitSize` square (`HitboxComp.hitbox`,
/// `HitboxComp.java:65-67`), so the scan below is exact. It is a boolean `any`,
/// so iteration order cannot change the result; block-unit proxies are skipped
/// because Java never adds them to `Groups.unit` (see `TargetQueries::build`).
pub fn can_land(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
) -> bool {
    // `TargetDummyComp.canLand()` (`@Replace`, `TargetDummyComp.java:27-32`):
    // dummies respect the boost config even when the landing area is obstructed
    // (usually by other dummies).
    if world.get::<TargetDummyComp>(entity).is_some() {
        return true;
    }
    if on_solid(world, grid, content, entity) {
        return false;
    }
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return true;
    };
    let psize = physic_size(world, entity);
    let (x, y) = (pos.x - psize / 2.0, pos.y - psize / 2.0);
    // The port's `Groups.unit` equivalent: the `Unit`-marked ECS query, minus
    // detached turret proxies (`BlockUnitComp`), exactly like `TargetQueries`.
    let mut query =
        world.query_filtered::<(Entity, &Pos, &HitboxComp), (With<Unit>, Without<BlockUnitComp>)>();
    for (other, other_pos, hitbox) in query.iter(world) {
        if other == entity || !is_grounded(world, other) {
            continue;
        }
        // `Rect.overlaps`: strict inequalities on all four edges.
        let half = hitbox.hit_size / 2.0;
        if x < other_pos.x + half
            && x + psize > other_pos.x - half
            && y < other_pos.y + half
            && y + psize > other_pos.y - half
        {
            return false;
        }
    }
    true
}

/// Advances every kind-specific movement state for `entity`.
///
/// Returns the tiles a tank overran with `unitMoveBreakable` blocks; the caller
/// removes them through plan-06 `WorldCtx` (this pass only borrows the grid).
pub fn update_kinematics(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    delta: (f32, f32),
) -> Vec<(i16, i16)> {
    // `StatusComp.update` runs in the merged `Unit.update` before the
    // kind-specific movement bodies; the live unit loop may instead call
    // [`crate::combat::damage::status::update_unit_status`] directly.
    crate::combat::damage::status::update_unit_status(world, content, entity);
    // `for(Ability a : abilities) a.update(self())` at the end of `Unit.update`.
    crate::combat::abilities::update_unit_abilities(world, content, entity);

    let Some(unit_def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return Vec::new();
    };
    let mut breaks = Vec::new();
    if world.get::<LegsComp>(entity).is_some() {
        update_legs(world, unit_def, entity, delta);
    }
    if world.get::<MechComp>(entity).is_some() {
        update_mech(world, entity, delta);
    }
    if world.get::<TankComp>(entity).is_some() {
        breaks = update_tank(world, grid, content, entity, delta, unit_def);
    }
    if world.get::<CrawlComp>(entity).is_some() {
        update_crawl(world, grid, content, entity, delta, unit_def);
    }
    if world.get::<WaterMoveComp>(entity).is_some() {
        update_water_move(world, entity, delta);
    }
    // `UnitComp.updateBoosting` (`UnitComp.java:121-130`): a `canBoost` unit
    // lifts off while boosting, standing on a solid block, or hovering over a
    // grounded unit (`isFlying() && !canLand()`), so a mech that stops boosting
    // overhead keeps elevation 1 instead of landing on top of it.
    if unit_def.can_boost
        && !world
            .get::<crate::entities::comp::Health>(entity)
            .is_some_and(|health| health.dead)
    {
        let boosting = world
            .get::<UnitCore>(entity)
            .is_some_and(|core| core.boosting);
        let solid = on_solid(world, grid, content, entity);
        let should_boost = boosting
            || solid
            || (is_flying(world, entity) && !can_land(world, grid, content, entity));
        let speed = if should_boost {
            unit_def.rise_speed
        } else {
            unit_def.descent_speed
        };
        let target = if should_boost { 1.0 } else { 0.0 };
        if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
            // `Mathf.approachDelta`: linear approach, one tick of `speed`.
            core.elevation = if (core.elevation - target).abs() <= speed {
                target
            } else {
                core.elevation + (target - core.elevation).signum() * speed
            };
        }
    }
    check_tether(world, entity);
    breaks
}

#[cfg(test)]
mod tests {
    use bevy_ecs::entity::Entity;

    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::comp::UnitCore;
    use crate::entities::comp::unit::movement::can_land;

    fn elevation(harness: &UnitHarness, entity: Entity) -> f32 {
        harness
            .build
            .world
            .get::<UnitCore>(entity)
            .map_or(f32::NAN, |core| core.elevation)
    }

    fn set_elevation(harness: &mut UnitHarness, entity: Entity, value: f32) {
        if let Some(mut core) = harness.build.world.get_mut::<UnitCore>(entity) {
            core.elevation = value;
        }
    }

    /// `canLand` for `entity` against the harness world.
    fn landed(harness: &mut UnitHarness, entity: Entity) -> bool {
        can_land(
            &mut harness.build.world,
            &harness.build.grid,
            &harness.build.content,
            entity,
        )
    }

    /// `canBoost` mechs hover over grounded units instead of landing on top of
    /// them (`UnitComp.updateBoosting`'s `isFlying() && !canLand()` term,
    /// `UnitComp.java:124`).
    #[test]
    fn can_boost_mech_keeps_hovering_over_grounded_unit() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let mech = harness.spawn("nova", 1, 64.0, 64.0, 0.0).expect("nova");
        let blocker = harness.spawn("dagger", 1, 64.0, 64.0, 0.0).expect("dagger");
        set_elevation(&mut harness, mech, 1.0);
        assert_eq!(elevation(&harness, blocker), 0.0);
        assert!(
            !harness
                .build
                .world
                .get::<UnitCore>(mech)
                .is_some_and(|core| core.boosting),
            "boost is off"
        );
        assert!(
            !landed(&mut harness, mech),
            "a grounded unit blocks the landing area"
        );

        // Twenty ticks of `descentSpeed` (0.08/tick) would take a free mech
        // from elevation 1 to 0; with the hover term it must stay airborne.
        for _ in 0..20 {
            harness.tick();
        }
        assert!(
            elevation(&harness, mech) >= 0.09,
            "mech landed on a grounded unit (elevation {})",
            elevation(&harness, mech)
        );
        assert_eq!(elevation(&harness, mech), 1.0);

        // Removing the underlying unit frees the landing area again.
        assert!(harness.remove(blocker));
        for _ in 0..20 {
            harness.tick();
        }
        assert_eq!(elevation(&harness, mech), 0.0);
    }

    /// `canLand`'s `f != self() && f.isGrounded()` predicate
    /// (`UnitComp.java:176-179`): self never blocks, and only units with
    /// `elevation < 0.001` (`isGrounded`, `UnitComp.java:80-82`) count.
    #[test]
    fn can_land_excludes_self_and_ignores_airborne_units() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let mech = harness.spawn("nova", 1, 64.0, 64.0, 0.0).expect("nova");
        assert_eq!(elevation(&harness, mech), 0.0);
        // A grounded lone mech does not block itself (`f != self()`).
        assert!(landed(&mut harness, mech));

        // 0.05 is neither grounded (`< 0.001`) nor flying (`>= 0.09`).
        let between = harness.spawn("dagger", 1, 64.0, 64.0, 0.0).expect("dagger");
        set_elevation(&mut harness, between, 0.05);
        assert!(landed(&mut harness, mech));
        set_elevation(&mut harness, between, 0.009);
        assert!(landed(&mut harness, mech));
        // Grounded units do block the physical box.
        set_elevation(&mut harness, between, 0.0);
        assert!(!landed(&mut harness, mech));

        // `TargetDummyComp.canLand()` (`@Replace`) always allows landing, even
        // when another dummy obstructs the area.
        let dummy = harness.spawn("dummy", 1, 96.0, 96.0, 0.0).expect("dummy");
        let other = harness.spawn("dummy", 1, 96.0, 96.0, 0.0).expect("dummy");
        set_elevation(&mut harness, dummy, 1.0);
        assert_eq!(elevation(&harness, other), 0.0);
        assert!(landed(&mut harness, dummy));
    }
}
