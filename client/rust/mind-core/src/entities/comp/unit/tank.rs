// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TankComp` tread state (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/TankComp.java`. The port keeps
//! the deterministic tread timer (`treadTime`); tread dust rects and
//! `crushFragile` are plan-10 collision/view seams.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use super::comp::TankComp;

/// `TankComp.update`: advance the tread animation from the body displacement.
pub fn update_tank(world: &mut World, entity: Entity, delta: (f32, f32)) {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    if let Some(mut tank) = world.get_mut::<TankComp>(entity) {
        tank.tread_time += speed;
    }
    // TODO(plan 10): `crushFragile` damage to units/buildings under the treads.
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::TankComp;
    use crate::entities::comp::unit::movement::update_kinematics;

    #[test]
    fn tank_tread_time_advances() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("stell", 0, 64.0, 64.0, 0.0).expect("stell");
        for _ in 0..8 {
            update_kinematics(
                &mut harness.build.world,
                &harness.build.content,
                unit,
                (1.5, 0.0),
            );
        }
        assert!(
            harness
                .build
                .world
                .get::<TankComp>(unit)
                .unwrap()
                .tread_time
                > 0.0
        );
    }
}
