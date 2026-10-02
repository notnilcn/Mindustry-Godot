// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LegsComp` behavior (plan 11 §3.6/§4.1).
//!
//! Ported from `core/src/mindustry/entities/comp/LegsComp.java`. The port keeps
//! the deterministic stepping core: legs are laid out around the body, each leg
//! re-steps when its planted foot drifts past `legMoveSpace`, and the knee is
//! placed with the plan-16 `InverseKinematics` solver. Leg splash damage, walk
//! FX and `unitMoveBreakable` deconstruction are plan-10/17 seams (marked).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::registries::units::UnitTypeDef;
use crate::entities::comp::Pos;
use crate::render::math::inverse_kinematics::solve_side;

use super::comp::{Leg, LegsComp, UnitCore};

/// Builds the initial leg layout for a freshly spawned legs unit.
pub fn reset_legs(unit: &UnitTypeDef, x: f32, y: f32, rotation: f32) -> LegsComp {
    let count = unit.leg_count.max(0) as u8;
    let group_size = unit.leg_group_size.max(1) as u8;
    let mut legs = Vec::with_capacity(count as usize);
    for index in 0..count {
        let side = index % 2 == 1;
        let group = index / group_size;
        // Spread evenly around the body; keep within a 90° forward arc spread.
        let angle = rotation + 360.0 * (index as f32) / (count.max(1) as f32);
        let (sin, cos) = angle.to_radians().sin_cos();
        let hip = hip_distance(unit);
        let base_x = x + cos * hip;
        let base_y = y + sin * hip;
        // The foot rests one `legLength` outward from the hip.
        let reach = hip + foot_distance(unit);
        let foot_x = x + cos * reach;
        let foot_y = y + sin * reach;
        let leg = Leg {
            base_x,
            base_y,
            joint_x: (base_x + foot_x) / 2.0,
            joint_y: (base_y + foot_y) / 2.0,
            foot_x,
            foot_y,
            index,
            moving: false,
            step: 0.0,
            side,
            group,
            angle,
        };
        legs.push(solve_leg(&leg, unit));
    }
    LegsComp {
        leg_count: count,
        legs,
        base_rotation: rotation,
        walk_time: 0.0,
    }
}

/// Hip distance from the body center (`legBaseOffset`, absolute world units).
fn hip_distance(unit: &UnitTypeDef) -> f32 {
    if unit.leg_base_offset > 0.0 {
        unit.leg_base_offset
    } else {
        (unit.hit_size * 0.25).max(1.0)
    }
}

/// Nominal foot reach (`legLength`).
fn foot_distance(unit: &UnitTypeDef) -> f32 {
    if unit.leg_length > 0.0 {
        unit.leg_length
    } else {
        unit.hit_size
    }
}

/// Solves the knee position for `leg`, clamping to valid segment lengths.
///
/// `legMaxLength`/`legMinLength` are upstream **multipliers** of `legLength`.
pub fn solve_leg(leg: &Leg, unit: &UnitTypeDef) -> Leg {
    let hip = [leg.base_x, leg.base_y];
    let end = [leg.foot_x - hip[0], leg.foot_y - hip[1]];
    let base_len = foot_distance(unit);
    let max_len = if unit.leg_max_length > 0.0 {
        base_len * unit.leg_max_length
    } else {
        base_len
    };
    let min_len = if unit.leg_min_length > 0.0 {
        base_len * unit.leg_min_length
    } else {
        max_len * 0.5
    };
    let (ok, joint) = solve_side(max_len, min_len, end, leg.side);
    let mut solved = *leg;
    if ok && joint[0].is_finite() && joint[1].is_finite() {
        solved.joint_x = hip[0] + joint[0];
        solved.joint_y = hip[1] + joint[1];
    } else {
        // Fall back to a straight mid-segment so joints never NaN.
        solved.joint_x = hip[0] + end[0] * 0.5;
        solved.joint_y = hip[1] + end[1] * 0.5;
    }
    solved
}

/// `LegsComp.update`: animate and re-step the legs after the body moved.
///
/// `delta` is the body displacement since the last tick. Returns the number of
/// legs that stepped.
pub fn update_legs(
    world: &mut World,
    unit: &UnitTypeDef,
    entity: Entity,
    delta: (f32, f32),
) -> u32 {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return 0;
    };
    let Some(core) = world.get::<UnitCore>(entity).copied() else {
        return 0;
    };
    let Some(mut legs) = world.get_mut::<LegsComp>(entity) else {
        return 0;
    };

    let move_space = if unit.leg_move_space > 0.0 {
        unit.leg_move_space
    } else {
        unit.hit_size * 0.5
    };
    let moved_len = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    let hip = hip_distance(unit);
    let reach = hip + foot_distance(unit);
    let base_rotation = legs.base_rotation;
    let mut stepped = 0u32;

    for leg in &mut legs.legs {
        let angle = core.rotation + leg.angle - base_rotation;
        let (sin, cos) = angle.to_radians().sin_cos();
        let target_x = pos.x + cos * reach;
        let target_y = pos.y + sin * reach;
        leg.base_x = pos.x + cos * hip;
        leg.base_y = pos.y + sin * hip;

        let dx = target_x - leg.foot_x;
        let dy = target_y - leg.foot_y;
        let drift2 = dx * dx + dy * dy;
        if drift2.sqrt() > move_space {
            leg.foot_x = target_x;
            leg.foot_y = target_y;
            leg.moving = moved_len > 0.001;
            leg.step = 1.0;
            stepped += 1;
        } else if leg.moving && moved_len <= 0.001 {
            leg.moving = false;
            leg.step = 0.0;
        }
        let solved = solve_leg(leg, unit);
        leg.joint_x = solved.joint_x;
        leg.joint_y = solved.joint_y;
    }
    legs.walk_time += moved_len;
    stepped
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::LegsComp;
    use crate::entities::comp::unit::legs::{reset_legs, solve_leg};

    fn legs_def(harness: &UnitHarness) -> crate::content::registries::units::UnitTypeDef {
        harness
            .content()
            .unit_by_name("corvus")
            .expect("corvus")
            .clone()
    }

    #[test]
    fn reset_legs_builds_solved_joints() {
        let harness = UnitHarness::new(32, 32, 1);
        let def = legs_def(&harness);
        let legs = reset_legs(&def, 64.0, 64.0, 0.0);
        assert!(legs.leg_count > 0);
        assert_eq!(legs.legs.len(), legs.leg_count as usize);
        for leg in &legs.legs {
            assert!(leg.joint_x.is_finite() && leg.joint_y.is_finite());
            let a =
                ((leg.joint_x - leg.base_x).powi(2) + (leg.joint_y - leg.base_y).powi(2)).sqrt();
            assert!(a.is_finite() && a > 0.0);
        }
    }

    #[test]
    fn update_legs_never_nans_and_is_deterministic() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("corvus", 0, 64.0, 64.0, 0.0).expect("corvus");
        let def = harness.content().unit_by_name("corvus").unwrap().clone();
        for _ in 0..50 {
            crate::entities::comp::unit::legs::update_legs(
                &mut harness.build.world,
                &def,
                unit,
                (1.0, 0.5),
            );
        }
        let legs = harness.build.world.get::<LegsComp>(unit).unwrap();
        for leg in &legs.legs {
            assert!(leg.joint_x.is_finite() && leg.joint_y.is_finite());
            assert!(leg.foot_x.is_finite() && leg.foot_y.is_finite());
        }
        // Solver is pure: same input -> same joint.
        let leg = legs.legs[0];
        let a = solve_leg(&leg, &def);
        let b = solve_leg(&leg, &def);
        assert_eq!((a.joint_x, a.joint_y), (b.joint_x, b.joint_y));
    }
}
