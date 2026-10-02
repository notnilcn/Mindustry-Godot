// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MechComp` walk state (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/MechComp.java`: the mech walk
//! timer advances with movement and `walked` flags a step. Step FX/shake and
//! `baseRotation` decoupling land with the view plans (16/17).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use super::comp::MechComp;

/// `MechComp.update`: advance the walk animation from the body displacement.
pub fn update_mech(world: &mut World, entity: Entity, delta: (f32, f32)) {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    let body = body_rotation(world, entity);
    if let Some(mut mech) = world.get_mut::<MechComp>(entity) {
        mech.walk_time += speed;
        mech.walked = speed > 0.001;
        if mech.walked {
            // `baseRotation` trails the body rotation (MechComp.rotateMove).
            mech.base_rotation = rotate_toward(mech.base_rotation, body, 5.0);
        }
    }
}

fn body_rotation(world: &World, entity: Entity) -> f32 {
    world
        .get::<super::comp::UnitCore>(entity)
        .map(|core| core.rotation)
        .unwrap_or(0.0)
}

fn rotate_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let mut diff = (target - current) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    current + diff.clamp(-max_step, max_step)
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::MechComp;
    use crate::entities::comp::unit::movement::update_kinematics;

    #[test]
    fn mech_walk_time_advances_with_movement() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        for _ in 0..10 {
            update_kinematics(
                &mut harness.build.world,
                &harness.build.content,
                unit,
                (2.0, 0.0),
            );
        }
        let mech = harness.build.world.get::<MechComp>(unit).unwrap();
        assert!(mech.walk_time > 0.0);
        assert!(mech.walked);
    }
}
