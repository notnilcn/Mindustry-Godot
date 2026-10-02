// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `WaterMoveComp` naval movement state (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/WaterMoveComp.java`. The port
//! keeps the two wave-trail timers; the water-solid predicate and shallow-floor
//! speed rules read plan-06 floor data (marked).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use super::comp::WaterMoveComp;

/// `WaterMoveComp.update`: advance the twin wave-trail timers with movement.
pub fn update_water_move(world: &mut World, entity: Entity, delta: (f32, f32)) {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    if let Some(mut water) = world.get_mut::<WaterMoveComp>(entity) {
        water.trail_time += speed;
    }
    // TODO(plan 06): water-solid predicate + `floorSpeedMultiplier` shallow rules.
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::WaterMoveComp;
    use crate::entities::comp::unit::movement::update_kinematics;

    #[test]
    fn water_trail_time_advances() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("risso", 0, 64.0, 64.0, 0.0).expect("risso");
        update_kinematics(
            &mut harness.build.world,
            &harness.build.content,
            unit,
            (3.0, 0.0),
        );
        assert!(
            harness
                .build
                .world
                .get::<WaterMoveComp>(unit)
                .unwrap()
                .trail_time
                > 0.0
        );
    }
}
