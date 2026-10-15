// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FlyingAI` (plan 11 §4.2). Air units ignore terrain and steer directly toward
//! their target; flag/core targeting and `randomWaveAI` come with M3/M6. The
//! steering math is shared with [`super::ground::steer_to`].

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::{HitboxComp, UnitCore};
use crate::entities::comp::{Pos, Vel};
use crate::world::TilePos;
use crate::world::WorldGrid;

use super::super::controller::UnitController;
use super::ground::{DEFAULT_ARRIVE, steer_to, tile_center};
use super::missile::move_toward_angle;

/// Flying controller state (`FlyingAI`).
#[derive(Debug, Default, Clone, Copy)]
pub struct FlyingAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current move target.
    pub target: Option<TilePos>,
}

/// `FlyingAI` per-unit timer/target state (`AIController.timer`/`target`).
///
/// Inserted lazily by the live unit loop; mirrors [`super::ground::GroundAiState`]
/// without the stuck bookkeeping (FlyingAI has none).
#[derive(Debug, Clone, Copy, bevy_ecs::component::Component)]
pub struct FlyingAiState {
    /// `AIController.timer.get(timerTarget, target == null ? 40 : 90)` countdown.
    pub retarget: f32,
    /// Main target (`AIController.target`), chosen by `FlyingAI.findMainTarget`.
    pub target: Option<Entity>,
}

impl Default for FlyingAiState {
    fn default() -> Self {
        Self {
            // Java `resetTimers` seeds `Mathf.random(40)`; the port takes the
            // earliest draw so the first acquisition runs on the first tick.
            retarget: 39.0,
            target: None,
        }
    }
}

/// `AIController.turningAway` for `circleAttack` (`AIController.java:325`):
/// set while peeling away from an overlapping same-layer unit.
#[derive(bevy_ecs::component::Component, Debug, Default, Clone, Copy)]
pub struct FlyingCircleState {
    /// `turningAway`.
    pub turning_away: bool,
}

/// `Angles.angleDist` on degrees.
fn angle_dist(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    180.0 - (d - 180.0).abs()
}

fn unit_control(
    world: &World,
    content: &crate::content::ContentRegistry,
    entity: Entity,
) -> (bool, f32, f32) {
    let def = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id));
    let omni = def.map(|def| def.omni_movement).unwrap_or(false);
    let rotate_speed = def.map(|def| def.rotate_speed).unwrap_or(0.0);
    let accel = world
        .get::<crate::entities::comp::unit::comp::PhysicsComp>(entity)
        .map(|physics| physics.accel)
        .unwrap_or(0.5);
    (omni, rotate_speed, accel)
}

/// `AIController.circleAttack` (`AIController.java:318-351`): steer around
/// `target` at roughly `circle_length` distance, peeling away from overlapping
/// same-layer units. Only `vel`/`rotation` change; `VelComp.update` integrates.
pub fn circle_attack(
    world: &mut World,
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    entity: Entity,
    target: Entity,
    circle_length: f32,
) {
    let (Some(target_pos), Some(pos)) = (
        world.get::<Pos>(target).copied(),
        world.get::<Pos>(entity).copied(),
    ) else {
        return;
    };
    let core = world
        .get::<UnitCore>(entity)
        .copied()
        .unwrap_or(UnitCore::new(0.0));
    let vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    let speed = crate::entities::comp::unit::movement::unit_speed(world, grid, content, entity);
    let (omni, _, _) = unit_control(world, content, entity);

    // `vec.set(target).sub(unit)`.
    let (mut vx, mut vy) = (target_pos.x - pos.x, target_pos.y - pos.y);
    let len = (vx * vx + vy * vy).sqrt();
    let ang = vy.atan2(vx).to_degrees();
    let diff = angle_dist(ang, core.rotation);

    // `target instanceof Unit u && u.collisionLayer() == unit.collisionLayer()`.
    let same_layer_unit = world.get::<UnitCore>(target).is_some()
        && crate::combat::bullet::unit_is_flying(world, target)
            == crate::combat::bullet::unit_is_flying(world, entity);
    if same_layer_unit {
        let target_hit = world
            .get::<HitboxComp>(target)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        let avoid_dist = target_hit + 30.0;
        let mut state = world
            .get::<FlyingCircleState>(entity)
            .copied()
            .unwrap_or_default();
        if state.turning_away {
            // `vec.setLength(prefSpeed()).scl(-1f); unit.movePref(vec);`.
            let (ux, uy) = if len > 0.0001 {
                (vx / len, vy / len)
            } else {
                (1.0, 0.0)
            };
            crate::entities::comp::unit::velocity::move_pref(
                world,
                content,
                entity,
                -ux * speed,
                -uy * speed,
            );
            let still_close = len <= 60.0 * 0.5 + target_hit;
            if !still_close {
                state.turning_away = false;
                world.entity_mut(entity).insert(state);
            }
            return;
        } else if len <= avoid_dist {
            state.turning_away = true;
            world.entity_mut(entity).insert(state);
        }
    }

    if diff > 70.0 && len < circle_length {
        // `vec.setAngle(unit.vel().angle())`.
        let vel_len = (vel.x * vel.x + vel.y * vel.y).sqrt();
        if vel_len > 0.0001 {
            vx = vel.x;
            vy = vel.y;
        }
    } else if omni {
        // `vec.setAngle(Angles.moveToward(unit.vel().angle(), vec.angle(), 6f))`.
        let current = vel.y.atan2(vel.x).to_degrees();
        let want = vy.atan2(vx).to_degrees();
        let angle = move_toward_angle(current, want, 6.0).to_radians();
        vx = angle.cos();
        vy = angle.sin();
    }
    // `vec.setLength(prefSpeed())`.
    let vlen = (vx * vx + vy * vy).sqrt();
    if vlen > 0.0001 {
        vx = vx / vlen * speed;
        vy = vy / vlen * speed;
    }
    crate::entities::comp::unit::velocity::move_pref(world, content, entity, vx, vy);
}

/// `AIController.moveTo(target, circleLength, 100f)` (keepDistance = flying)
/// plus `FlyingAI`'s trailing `unit.lookAt(target)` (`FlyingAI.java:25-26`).
pub fn move_to(
    world: &mut World,
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    entity: Entity,
    target: Entity,
    circle_length: f32,
) {
    let Some(target_pos) = world.get::<Pos>(target).copied() else {
        return;
    };
    if world.get::<Pos>(entity).is_none() {
        return;
    }
    move_to_point(
        world,
        grid,
        content,
        entity,
        target_pos.x,
        target_pos.y,
        circle_length,
        100.0,
        true,
    );
}

/// `AIController.moveTo(target, circleLength, smooth, keepDistance=flying)`
/// (AIController.java:371-420) at an explicit point. `smooth` scales the
/// approach length; `look_at` drives the trailing `unit.lookAt` (FlyingAI
/// target-chasing only — the idle spawner shuttle does not look at its point).
#[allow(clippy::too_many_arguments)]
pub fn move_to_point(
    world: &mut World,
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    entity: Entity,
    target_x: f32,
    target_y: f32,
    circle_length: f32,
    smooth: f32,
    look_at: bool,
) {
    move_to_point_keep_distance(
        world,
        grid,
        content,
        entity,
        target_x,
        target_y,
        circle_length,
        smooth,
        true,
        look_at,
    );
}

/// [`move_to_point`] with the explicit `keepDistance` flag: a point closer
/// than `circle_length - smooth/2` backs away when `keep_distance` is set
/// (`AIController.moveTo`'s rotate-180 branch), otherwise the velocity is
/// zeroed. `LogicAI` uses `keep_distance = unit.isFlying()` for `move`/
/// `pathfind` and `true` for `approach`.
#[allow(clippy::too_many_arguments)]
pub fn move_to_point_keep_distance(
    world: &mut World,
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    entity: Entity,
    target_x: f32,
    target_y: f32,
    circle_length: f32,
    smooth: f32,
    keep_distance: bool,
    look_at: bool,
) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let speed = crate::entities::comp::unit::movement::unit_speed(world, grid, content, entity);

    let (mut vx, mut vy) = (target_x - pos.x, target_y - pos.y);
    let dst = (vx * vx + vy * vy).sqrt();
    let length = if circle_length <= 0.001 {
        1.0
    } else {
        ((dst - circle_length) / smooth).clamp(-1.0, 1.0)
    };
    if dst > 0.0001 {
        vx = vx / dst * speed * length;
        vy = vy / dst * speed * length;
    } else {
        vx = 0.0;
        vy = 0.0;
    }
    if length < -0.5 {
        if keep_distance {
            // `keepDistance`: rotate the vector 180 degrees.
            vx = -vx;
            vy = -vy;
        } else {
            vx = 0.0;
            vy = 0.0;
        }
    } else if length < 0.0 {
        vx = 0.0;
        vy = 0.0;
    }
    // `if(vec.isNaN() || vec.isInfinite() || vec.isZero()) return;`
    if (vx.is_nan() || vy.is_nan() || vx.is_infinite() || vy.is_infinite())
        || (vx == 0.0 && vy == 0.0)
    {
        if look_at {
            look_at_point(world, entity, &pos, target_x, target_y, content);
        }
        return;
    }
    crate::entities::comp::unit::velocity::move_pref(world, content, entity, vx, vy);
    if look_at {
        look_at_point(world, entity, &pos, target_x, target_y, content);
    }
}

/// The trailing `unit.lookAt(target)` of [`move_to_point`].
fn look_at_point(
    world: &mut World,
    entity: Entity,
    pos: &Pos,
    target_x: f32,
    target_y: f32,
    content: &crate::content::ContentRegistry,
) {
    let (_, rotate_speed, _) = unit_control(world, content, entity);
    let rotation = world
        .get::<UnitCore>(entity)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    let looked = move_toward_angle(
        rotation,
        ang_to(
            pos,
            Pos {
                x: target_x,
                y: target_y,
            },
        ),
        rotate_speed,
    );
    if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
        stored.rotation = looked;
    }
}

/// `AIController.circle(target, circleLength)` (AIController.java:353-365): with
/// `vec = target - unit`, rotate by `(circleLength - len)/circleLength * 180`
/// when closer than `circleLength`, then move at `prefSpeed` in that direction.
pub fn circle_pos(
    world: &mut World,
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    entity: Entity,
    target_x: f32,
    target_y: f32,
    circle_length: f32,
) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let speed = crate::entities::comp::unit::movement::unit_speed(world, grid, content, entity);

    // `vec.set(target).sub(unit)`.
    let (mut vx, mut vy) = (target_x - pos.x, target_y - pos.y);
    let len = (vx * vx + vy * vy).sqrt();
    if len < circle_length && circle_length > 0.0001 {
        // `vec.rotate((circleLength - len) / circleLength * 180f)`.
        let angle = (circle_length - len) / circle_length * 180.0;
        let radians = angle.to_radians();
        let (sin, cos) = radians.sin_cos();
        let rx = vx * cos - vy * sin;
        let ry = vx * sin + vy * cos;
        vx = rx;
        vy = ry;
    }
    // `vec.setLength(speed)`.
    let vlen = (vx * vx + vy * vy).sqrt();
    if vlen > 0.0001 {
        vx = vx / vlen * speed;
        vy = vy / vlen * speed;
    }
    if vx.abs() > 0.0001 || vy.abs() > 0.0001 {
        crate::entities::comp::unit::velocity::move_pref(world, content, entity, vx, vy);
    }
}

fn ang_to(from: &Pos, to: Pos) -> f32 {
    (to.y - from.y).atan2(to.x - from.x).to_degrees()
}

impl FlyingAi {
    /// Creates a controller with no target.
    pub fn new() -> Self {
        Self::default()
    }
}

impl UnitController for FlyingAi {
    fn unit(&self) -> Option<Entity> {
        self.unit
    }

    fn set_unit(&mut self, unit: Entity) {
        self.unit = Some(unit);
    }
}

/// Runs one flying-AI tick for `entity`: straight-line steering to `target`.
///
/// Returns `true` when the unit arrived this tick.
pub fn update_flying(
    world: &mut World,
    grid: &crate::world::WorldGrid,
    content: &crate::content::ContentRegistry,
    entity: Entity,
    target: TilePos,
) -> bool {
    if world.get::<Pos>(entity).is_none() {
        return false;
    }
    let hit_size = world
        .get::<HitboxComp>(entity)
        .map(|hitbox| hitbox.hit_size)
        .unwrap_or(0.0);
    let dest = tile_center(target.x() as i32, target.y() as i32);
    let arrive = hit_size.max(DEFAULT_ARRIVE);
    steer_to(world, grid, content, entity, dest.0, dest.1, arrive)
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;

    #[test]
    fn flying_unit_crosses_impassable_terrain() {
        let mut harness = UnitHarness::new(64, 16, 3);
        // A solid wall spanning the map would block a ground unit.
        let wall = harness
            .content()
            .block_id("copper-wall")
            .expect("copper-wall");
        for y in 0..16 {
            assert!(harness.build.place(32, y, wall, 0, true));
        }
        let unit = harness.spawn("flare", 0, 40.0, 40.0, 0.0).expect("flare");
        assert!(harness.command_move(unit, 60, 5));
        for _ in 0..1200 {
            harness.tick();
        }
        assert_eq!(harness.unit_count(), 1, "flying unit survives the wall");
    }
}
