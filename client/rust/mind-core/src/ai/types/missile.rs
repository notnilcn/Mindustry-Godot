// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MissileAI` (plan 11 §4.2). Homing missiles steer at the shooter's aim,
//! always move forward at `MissileUnitType` speed, and die on enemy building
//! contact or when the `TimedKill` lifetime expires. Ported from
//! `core/src/mindustry/ai/types/MissileAI.java`.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;

use crate::ai::ai_controller::AiCtx;
use crate::entities::comp::unit::comp::{PhysicsComp, TimedKillComp, UnitCore};
use crate::entities::comp::{Health, Pos, TeamComp, Vel};
use crate::weapons::UnitWeapons;
use crate::world::WorldGrid;

/// `MissileAI.shooter`: the unit whose `aimX/aimY` steers the missile
/// (`MissileAI.java:13`), stored on the live missile entity.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MissileShooter(pub Option<Entity>);

/// `MissileAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissileAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current target (`AIController.target`; the building the missile was
    /// aimed at, for the `build == target` contact branch).
    pub target: Option<Entity>,
    /// Steering shooter (`MissileAI.shooter`).
    pub shooter: Option<Entity>,
}

/// `MissileAI.updateMovement`: home toward the shooter's aim, move forward
/// forever, and die instantly on enemy building contact.
///
/// Returns `false` once the missile has been marked dead.
pub fn update_missile(ctx: &mut AiCtx, unit: Entity, state: &mut MissileAi) -> bool {
    // `float time = unit instanceof TimedKillc t ? t.time() : 1000000f;`
    let time = ctx
        .world
        .get::<TimedKillComp>(unit)
        .map(|timed| timed.time)
        .unwrap_or(1_000_000.0);
    let Some(type_id) = ctx.unit_type(unit) else {
        return true;
    };
    let Some(def) = ctx.content.unit(type_id) else {
        return true;
    };
    let (homing_delay, missile_accel_time, rotate_speed, target_ground) = (
        def.homing_delay,
        def.missile_accel_time,
        def.rotate_speed,
        def.target_ground,
    );

    let status_speed = ctx
        .world
        .get::<crate::entities::comp::unit::comp::StatusComp>(unit)
        .map(|status| status.speed_multiplier)
        .unwrap_or(1.0);

    // `if(time >= unit.type.homingDelay && shooter != null && !shooter.dead()) unit.lookAt(shooter.aimX, shooter.aimY);`
    if time >= homing_delay
        && let Some(shooter) = state.shooter
        && ctx.world.get_entity(shooter).is_ok()
        && !ctx
            .world
            .get::<Health>(shooter)
            .is_some_and(|health| health.dead)
    {
        // `Unit.aimX()/aimY()`: mount 0's aim, else the shooter's position.
        let aim = ctx
            .world
            .get::<UnitWeapons>(shooter)
            .and_then(|weapons| weapons.mounts.first())
            .map(|mount| (mount.aim_x, mount.aim_y))
            .or_else(|| ctx.world.get::<Pos>(shooter).map(|pos| (pos.x, pos.y)));
        if let (Some((aim_x, aim_y)), Some(pos)) = (aim, ctx.world.get::<Pos>(unit).copied()) {
            let angle = (aim_y - pos.y).atan2(aim_x - pos.x).to_degrees();
            if let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit) {
                core.rotation =
                    move_toward_angle(core.rotation, angle, rotate_speed * status_speed);
            }
        }
    }

    // `unit.moveAt(vec.trns(unit.rotation, accelTime <= 0 ? unit.speed() : pow(min(time / accelTime, 1), 2) * unit.speed()))`.
    let pref = ctx.pref_speed(unit);
    let speed = if missile_accel_time <= 0.0 {
        pref
    } else {
        (time / missile_accel_time).min(1.0).powi(2) * pref
    };
    let accel = ctx
        .world
        .get::<PhysicsComp>(unit)
        .map(|physics| physics.accel)
        .unwrap_or(0.5);
    let rotation = ctx
        .world
        .get::<UnitCore>(unit)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    let rad = rotation.to_radians();
    let target_vel = (rad.cos() * speed, rad.sin() * speed);
    let (mut pos, mut vel) = match (
        ctx.world.get::<Pos>(unit).copied(),
        ctx.world.get::<Vel>(unit).copied(),
    ) {
        (Some(pos), Some(vel)) => (pos, vel),
        _ => return true,
    };
    // `Unit.moveAt(vector, type.accel)`: `vel += (vector - vel).limit(accel * |vector| * delta)`.
    let (dx, dy) = (target_vel.0 - vel.x, target_vel.1 - vel.y);
    let len = (dx * dx + dy * dy).sqrt();
    let max_delta = accel * speed;
    if len > max_delta && len > 0.0 {
        vel.x += dx / len * max_delta;
        vel.y += dy / len * max_delta;
    } else {
        vel.x = target_vel.0;
        vel.y = target_vel.1;
    }
    if let Some(mut stored) = ctx.world.get_mut::<Vel>(unit) {
        *stored = vel;
    }
    // `VelComp.update`: `move(vel.x * Time.delta, vel.y * Time.delta)`.
    pos.x += vel.x;
    pos.y += vel.y;
    if let Some(mut stored) = ctx.world.get_mut::<Pos>(unit) {
        *stored = pos;
    }

    // `var build = unit.buildOn();` — kill instantly on enemy building contact.
    let tile = ctx
        .grid
        .tiles
        .getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y));
    if let Some(build) = tile.and_then(|tile| tile.build) {
        let team = ctx
            .world
            .get::<TeamComp>(unit)
            .map(|team| team.team)
            .unwrap_or(0);
        let build_team = ctx.world.get::<TeamComp>(build).map(|team| team.team);
        // `(build == target || !build.block.underBullets)` — `Block.underBullets`
        // is not on `BlockDef` yet; default false (Java `Block.underBullets`).
        if target_ground && build_team != Some(team) {
            if let Some(mut health) = ctx.world.get_mut::<Health>(unit) {
                health.health = 0.0;
                health.dead = true;
            }
            state.target = Some(build);
            return false;
        }
    }
    true
}

/// `Angles.moveToward(from, to, speed)`.
pub fn move_toward_angle(from: f32, to: f32, speed: f32) -> f32 {
    let mut delta = (to - from).rem_euclid(360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    if delta.abs() <= speed || speed == 0.0 {
        to.rem_euclid(360.0)
    } else {
        (from + delta.signum() * speed).rem_euclid(360.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn missile_homes_toward_shooter_aim() {
        let mut harness = UnitHarness::new(64, 64, 1);
        // `missile` is a codegen-only def; attach the marker to a flyer.
        let shooter = harness.spawn("flare", 0, 40.0, 40.0, 0.0).expect("flare");
        // The shooter aims far to +x; the missile steers at `shooter.aimX/aimY`.
        if let Some(mut weapons) = harness.build.world.get_mut::<UnitWeapons>(shooter)
            && let Some(mount) = weapons.mounts.first_mut()
        {
            mount.aim_x = 400.0;
            mount.aim_y = 40.0;
        }
        let a = harness.spawn("flare", 0, 40.0, 40.0, 0.0).expect("missile");
        harness.build.world.entity_mut(a).insert(TimedKillComp {
            time: 100.0,
            lifetime: 100.0,
        });
        let mut state = MissileAi {
            unit: Some(a),
            target: None,
            shooter: Some(shooter),
        };
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        update_missile(&mut ctx, a, &mut state);
        let snap = harness.snapshot(a).expect("alive");
        assert!(snap.x > 40.0);
    }
}
