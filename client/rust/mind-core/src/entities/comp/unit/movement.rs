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

use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{
    CrawlComp, LegsComp, MechComp, TankComp, UnitTypeComp, WaterMoveComp,
};

use super::crawl::update_crawl;
use super::legs::update_legs;
use super::mech::update_mech;
use super::tank::update_tank;
use super::tether::check_tether;
use super::water_move::update_water_move;

/// Advances every kind-specific movement state for `entity`.
pub fn update_kinematics(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    delta: (f32, f32),
) {
    let Some(unit_def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return;
    };
    if world.get::<LegsComp>(entity).is_some() {
        update_legs(world, unit_def, entity, delta);
    }
    if world.get::<MechComp>(entity).is_some() {
        update_mech(world, entity, delta);
    }
    if world.get::<TankComp>(entity).is_some() {
        update_tank(world, entity, delta);
    }
    if world.get::<CrawlComp>(entity).is_some() {
        update_crawl(world, entity, delta, unit_def.segment_max_rot);
    }
    if world.get::<WaterMoveComp>(entity).is_some() {
        update_water_move(world, entity, delta);
    }
    check_tether(world, entity);
}
