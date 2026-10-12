// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-kind movement/kinematics dispatch (plan 11 §3.6).
//!
//! Runs the kind-specific update bodies after the AI controller moved the unit:
//! leg IK (`LegsComp`), mech walk (`MechComp`), tank treads (`TankComp`), crawl
//! segment rotation (`CrawlComp`), naval trails (`WaterMoveComp`) and tether
//! validity. Mirrors the upstream `update()` merge order (plan 11 §3.3).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::ai::controller::{AiKind, ControllerSlot};
use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{
    CrawlComp, LegsComp, MechComp, PhysicsComp, StatusComp, TankComp, UnitCore, UnitTypeComp,
    WaterMoveComp,
};
use crate::entities::comp::{Pos, Vel};
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
    check_tether(world, entity);
    breaks
}
