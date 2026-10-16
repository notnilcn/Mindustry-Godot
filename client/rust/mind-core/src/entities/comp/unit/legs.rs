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

use super::comp::{Leg, LegsComp};

/// `LegsComp.defaultLegAngle(index)` (`LegsComp.java:246-248`):
/// `baseRotation + 360/legs.length * index + 360/legs.length/2`.
pub fn default_leg_angle(base_rotation: f32, leg_count: usize, index: usize) -> f32 {
    if leg_count == 0 {
        return base_rotation;
    }
    let step = 360.0 / leg_count as f32;
    base_rotation + step * index as f32 + step / 2.0
}

/// `LegsComp.legAngle(index)` (`LegsComp.java:240-245`): the sim's per-leg
/// direction, straightened toward ±90° of the base rotation when
/// `legStraightness > 0`.
pub fn leg_angle(base_rotation: f32, leg_count: usize, index: usize, leg_straightness: f32) -> f32 {
    let default = default_leg_angle(base_rotation, leg_count, index);
    if leg_straightness > 0.0 {
        let side = if index >= leg_count / 2 { -90.0 } else { 90.0 } + base_rotation;
        default + (side - default) * leg_straightness
    } else {
        default
    }
}

/// `LegsComp.legOffset(out, index)` (`LegsComp.java:236-247`): the hip anchor
/// offset from the unit center, with the `legStraightness` lerp.
pub fn leg_offset(
    base_rotation: f32,
    leg_count: usize,
    index: usize,
    leg_base_offset: f32,
    leg_straightness: f32,
    base_leg_straightness: f32,
    leg_straight_length: f32,
) -> (f32, f32) {
    let angle = default_leg_angle(base_rotation, leg_count, index);
    let (mut ox, mut oy) = trns(angle, leg_base_offset, 0.0);
    if leg_straightness > 0.0 {
        let (sx, mut sy) = trns(angle - base_rotation, leg_base_offset, 0.0);
        sy = sy.signum() * leg_base_offset * leg_straight_length;
        let (rx, ry) = trns(base_rotation, sx, sy);
        ox = lerp(ox, rx, base_leg_straightness);
        oy = lerp(oy, ry, base_leg_straightness);
    }
    (ox, oy)
}

/// `Angles.trnsx/trnsy` — rotate `(x, y)` by `angle` degrees.
fn trns(angle_deg: f32, x: f32, y: f32) -> (f32, f32) {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

/// `Mathf.lerp`.
fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

/// Builds the initial leg layout for a freshly spawned legs unit
/// (`LegsComp.resetLegs`, `LegsComp.java:95-110`).
pub fn reset_legs(unit: &UnitTypeDef, x: f32, y: f32, rotation: f32) -> LegsComp {
    let count = unit.leg_count.max(0) as u8;
    let leg_length = foot_distance(unit);
    let mut legs = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let leg = Leg {
            base_x: x,
            base_y: y,
            joint_x: x,
            joint_y: y,
            foot_x: x,
            foot_y: y,
            index: index as u8,
            moving: false,
            step: 0.0,
            side: index < count as usize / 2,
            group: 0,
            angle: default_leg_angle(rotation, count as usize, index),
        };
        let (base_x, base_y) = leg_offset(
            rotation,
            count as usize,
            index,
            unit.leg_base_offset,
            unit.leg_straightness,
            unit.base_leg_straightness,
            unit.leg_straight_length,
        );
        // `l.joint.trns(dstRot, legLength/2f).add(baseOffset)` and
        // `l.base.trns(dstRot, legLength).add(baseOffset)` with
        // `dstRot = defaultLegAngle(index)` (`LegsComp.java:101-105`).
        let (jx, jy) = trns(leg.angle, leg_length / 2.0, 0.0);
        let (fx, fy) = trns(leg.angle, leg_length * unit.leg_length_scl, 0.0);
        let mut leg = Leg {
            base_x: x + base_x,
            base_y: y + base_y,
            joint_x: x + base_x + jx,
            joint_y: y + base_y + jy,
            foot_x: x + base_x + fx,
            foot_y: y + base_y + fy,
            ..leg
        };
        leg = clause_leg(leg, unit);
        legs.push(leg);
    }
    LegsComp {
        leg_count: count,
        legs,
        base_rotation: rotation,
        walk_time: 0.0,
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

/// Java's post-solve clamps (`LegsComp.java:217-218`): the joint within
/// `[legMinLength*legLength/2, legMaxLength*legLength/2]` of the hip and the
/// foot within `[legMinLength*legLength, legMaxLength*legLength]`.
fn clause_leg(mut leg: Leg, unit: &UnitTypeDef) -> Leg {
    let leg_length = foot_distance(unit);
    let max_len = if unit.leg_max_length > 0.0 {
        leg_length * unit.leg_max_length
    } else {
        leg_length
    };
    let min_len = if unit.leg_min_length > 0.0 {
        leg_length * unit.leg_min_length
    } else {
        max_len * 0.5
    };
    let clamp = |x: f32, y: f32, min: f32, max: f32, cx: f32, cy: f32| -> (f32, f32) {
        let dx = x - cx;
        let dy = y - cy;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return (cx, cy);
        }
        let clamped = len.clamp(min, max);
        (cx + dx / len * clamped, cy + dy / len * clamped)
    };
    let (jx, jy) = clamp(
        leg.joint_x,
        leg.joint_y,
        min_len * 0.5,
        max_len * 0.5,
        leg.base_x,
        leg.base_y,
    );
    leg.joint_x = jx;
    leg.joint_y = jy;
    let (fx, fy) = clamp(
        leg.foot_x, leg.foot_y, min_len, max_len, leg.base_x, leg.base_y,
    );
    leg.foot_x = fx;
    leg.foot_y = fy;
    leg
}

/// Solves the knee position for `leg` with Java's IK inputs
/// (`LegsComp.java:211-213`):
/// `InverseKinematics.solve(legLength/2, legLength/2, foot - hip, side)` then
/// the post clamps.
pub fn solve_leg(leg: &Leg, unit: &UnitTypeDef) -> Leg {
    let hip = [leg.base_x, leg.base_y];
    let end = [leg.foot_x - hip[0], leg.foot_y - hip[1]];
    let leg_length = foot_distance(unit);
    let (ok, joint) = solve_side(leg_length / 2.0, leg_length / 2.0, end, leg.side);
    let mut solved = *leg;
    if ok && joint[0].is_finite() && joint[1].is_finite() {
        solved.joint_x = hip[0] + joint[0];
        solved.joint_y = hip[1] + joint[1];
    } else {
        // Fall back to a straight mid-segment so joints never NaN.
        solved.joint_x = hip[0] + end[0] * 0.5;
        solved.joint_y = hip[1] + end[1] * 0.5;
    }
    clause_leg(solved, unit)
}

/// `LegsComp.update`: animate and re-step the legs after the body moved
/// (`LegsComp.java:112-220`). The port keeps the deterministic stepping core
/// (a foot snaps to its destination once the drift passes `moveSpace`) but now
/// uses Java's geometry: the hip anchor from `legOffset`, the destination from
/// `legAngle(dstRot)` and the IK halves. Deviations (documented): Java lerps
/// the foot/joint (`lerpDelta` by the stage fraction / `moveSpeed`), applies
/// `curMoveOffset` and runs the leg splash damage/deep-floor scans.
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
    let Some(mut legs) = world.get_mut::<LegsComp>(entity) else {
        return 0;
    };

    let leg_length = foot_distance(unit);
    let div = (legs.leg_count as usize / unit.leg_group_size.max(1) as usize).max(2);
    let move_space = leg_length / 1.6 / (div as f32 / 2.0) * unit.leg_move_space;
    let moved_len = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    let count = legs.leg_count as usize;
    let base_rotation = legs.base_rotation;
    let mut stepped = 0u32;

    for index in 0..count {
        let Some(leg) = legs.legs.get_mut(index) else {
            continue;
        };
        let (hx, hy) = leg_offset(
            base_rotation,
            count,
            index,
            unit.leg_base_offset,
            unit.leg_straightness,
            unit.base_leg_straightness,
            unit.leg_straight_length,
        );
        let base_x = pos.x + hx;
        let base_y = pos.y + hy;
        leg.base_x = base_x;
        leg.base_y = base_y;
        // `Vec2 legDest = Tmp.v1.trns(dstRot, legLength * legLengthScl)
        // .add(baseOffset)` (`LegsComp.java:207`).
        let dst_rot = leg_angle(base_rotation, count, index, unit.leg_straightness);
        let (tx, ty) = trns(dst_rot, leg_length * unit.leg_length_scl, 0.0);
        let dest_x = base_x + tx;
        let dest_y = base_y + ty;

        let dx = dest_x - leg.foot_x;
        let dy = dest_y - leg.foot_y;
        let drift2 = dx * dx + dy * dy;
        if drift2.sqrt() > move_space {
            leg.foot_x = dest_x;
            leg.foot_y = dest_y;
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
