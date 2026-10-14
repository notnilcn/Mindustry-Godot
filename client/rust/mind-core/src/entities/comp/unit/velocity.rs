// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The Java velocity model (PAR-0202): `VelComp.update`,
//! `EntityCollisions.move`, `UnitComp.moveAt/movePref/rotateMove` and
//! `PhysicsComp.impulse` (VelComp.java:23-90, EntityCollisions.java:36-104,
//! UnitComp.java:101-153, PhysicsComp.java:18-30).
//!
//! Every unit integrates `vel` exactly once per tick in [`vel_update`]
//! (`@MethodPriority(-1)`, so it runs before the controller), tile collisions
//! resolve through [`collisions_move`], and controllers only **add** velocity
//! through [`move_at`]/[`move_pref`]. External pushes (weather wind, weapon
//! knockback, the core-spawn impulse) go through [`impulse`].

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{
    HitboxComp, PhysicsComp, StatusComp, UnitCore, UnitTypeComp,
};
use crate::entities::comp::{Pos, Vel};
use crate::world::WorldGrid;

/// `Mathf.FLOAT_ROUNDING_ERROR` (`Mathf.equal`, Arc `Mathf.java`).
const FLOAT_ROUNDING_ERROR: f32 = 0.000001;
/// `EntityCollisions.seg`.
const SEG: f32 = 1.0;
/// `EntityCollisions.maxDelta`.
const MAX_DELTA: f32 = 1000.0;
/// `UnitType.accel` default (`UnitType.java`); used when a unit has no
/// `PhysicsComp` (tests/harness fixtures).
const DEFAULT_ACCEL: f32 = 0.5;

/// `Mathf.sign(f)` (`f < 0 ? -1 : 1`, Arc `Mathf.java:198`).
fn sign(f: f32) -> f32 {
    if f < 0.0 { -1.0 } else { 1.0 }
}

/// Axis-aligned rect (`arc.math.geom.Rect` subset used by `EntityCollisions`).
#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    /// `Rect.setCentered(x, y, w, h)` (`x = x - w / 2`).
    fn centered(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Self {
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            w,
            h,
        }
    }

    /// `Rect.overlaps(other)`: strict inequality on all four edges.
    fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }

    /// `Geometry.overlap(a, b, x)` (Geometry.java:346-397): the push that moves
    /// `self` out of `other` along the axis of least penetration. Returns
    /// `(0, 0)` when the rects do not overlap.
    fn overlap(&self, other: &Rect) -> (f32, f32) {
        let ax = self.x + self.w / 2.0;
        let bx = other.x + other.w / 2.0;
        let ay = self.y + self.h / 2.0;
        let by = other.y + other.h / 2.0;
        let nx = ax - bx;
        let ny = ay - by;
        let xoverlap = self.w / 2.0 + other.w / 2.0 - nx.abs();
        if xoverlap.abs() > 0.0 {
            let yoverlap = self.h / 2.0 + other.h / 2.0 - ny.abs();
            if yoverlap.abs() > 0.0 {
                let (n_x, n_y, pen) = if xoverlap.abs() < yoverlap.abs() {
                    (if nx < 0.0 { 1.0 } else { -1.0 }, 0.0, xoverlap)
                } else {
                    (0.0, if ny < 0.0 { 1.0 } else { -1.0 }, yoverlap)
                };
                let m = pen.max(0.0);
                return (-m * n_x, -m * n_y);
            }
        }
        (0.0, 0.0)
    }
}

/// `UnitComp.moveAt(vector, accel)`: `vel += (vector - vel).limit(accel * |vector|)`
/// (UnitComp.java:101-105; `Time.delta == 1` at the fixed 60 Hz step).
pub fn move_at(world: &mut World, entity: Entity, vx: f32, vy: f32, accel: f32) {
    let Some(mut vel) = world.get::<Vel>(entity).copied() else {
        return;
    };
    let len = (vx * vx + vy * vy).sqrt();
    let max_delta = accel * len;
    let dx = vx - vel.x;
    let dy = vy - vel.y;
    let dist = (dx * dx + dy * dy).sqrt();
    // `Vec2.limit` is a no-op on a zero delta vector.
    if dist > 0.0 {
        let scale = dist.min(max_delta) / dist;
        vel.x += dx * scale;
        vel.y += dy * scale;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
}

/// `UnitComp.rotateMove(vec)` (UnitComp.java:145-151): accelerate along the
/// current facing, then turn toward `vec` at `rotateSpeed * speedMultiplier`.
pub fn rotate_move(world: &mut World, content: &ContentRegistry, entity: Entity, vx: f32, vy: f32) {
    let Some(core) = world.get::<UnitCore>(entity).copied() else {
        return;
    };
    let len = (vx * vx + vy * vy).sqrt();
    let rad = core.rotation.to_radians();
    let accel = world
        .get::<PhysicsComp>(entity)
        .map_or(DEFAULT_ACCEL, |physics| physics.accel);
    move_at(world, entity, rad.cos() * len, rad.sin() * len, accel);
    if vx != 0.0 || vy != 0.0 {
        let def = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| content.unit(comp.type_id));
        let rotate_speed = def.map_or(0.0, |def| def.rotate_speed);
        let speed_mult = world
            .get::<StatusComp>(entity)
            .map_or(1.0, |status| status.speed_multiplier);
        let rotation = crate::weapons::angles_move_toward(
            core.rotation,
            vy.atan2(vx).to_degrees(),
            rotate_speed * speed_mult,
        );
        if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
            stored.rotation = rotation;
        }
    }
}

/// `UnitComp.movePref(movement)`: omni units accelerate straight at the
/// vector, turning units use [`rotate_move`] (UnitComp.java:134-143).
pub fn move_pref(world: &mut World, content: &ContentRegistry, entity: Entity, vx: f32, vy: f32) {
    let def = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id));
    if def.is_some_and(|def| def.omni_movement) {
        let accel = world
            .get::<PhysicsComp>(entity)
            .map_or(DEFAULT_ACCEL, |physics| physics.accel);
        move_at(world, entity, vx, vy, accel);
    } else {
        rotate_move(world, content, entity, vx, vy);
    }
}

/// `PhysicsComp.impulse(x, y)`: `vel += (x, y) / mass`, `mass = hitSize² π`
/// (PhysicsComp.java:18-30).
pub fn impulse(world: &mut World, entity: Entity, ix: f32, iy: f32) {
    let Some(hit_size) = world
        .get::<HitboxComp>(entity)
        .map(|hitbox| hitbox.hit_size)
    else {
        return;
    };
    let mass = hit_size * hit_size * std::f32::consts::PI;
    if let Some(mut vel) = world.get_mut::<Vel>(entity) {
        vel.x += ix / mass;
        vel.y += iy / mass;
    }
}

/// `EntityCollisions.moveDelta` (EntityCollisions.java:72-96): move the unit's
/// `hitboxTile` box by one segment, resolve the `(2r+1)²` neighbouring solid
/// tiles, and write the resolved offset back into the rect.
fn move_delta(rect: &mut Rect, deltax: f32, deltay: f32, r: i32, solid: &dyn Fn(i32, i32) -> bool) {
    let ts = crate::config::TILESIZE as f32;
    rect.x += deltax;
    rect.y += deltay;
    let tilex = WorldGrid::to_tile(rect.x + rect.w / 2.0);
    let tiley = WorldGrid::to_tile(rect.y + rect.h / 2.0);
    for dx in -r..=r {
        for dy in -r..=r {
            let wx = dx + tilex;
            let wy = dy + tiley;
            if solid(wx, wy) {
                let tmp = Rect::centered(wx as f32 * ts, wy as f32 * ts, ts, ts);
                if tmp.overlaps(rect) {
                    let (vx, vy) = rect.overlap(&tmp);
                    rect.x += vx;
                    rect.y += vy;
                }
            }
        }
    }
}

/// `EntityCollisions.move(entity, deltax, deltay, solidCheck)`
/// (EntityCollisions.java:36-70) with `HitboxComp.hitboxTile`'s
/// `min(hitSize * 0.66, 7.8)` box (HitboxComp.java:69-76).
///
/// Moves in 1-unit segments, resolving each against solid tiles; `pos` is the
/// final resolved centre (`entity.trns(r1.x - r2.x, r1.y - r2.y)` accumulated).
pub fn collisions_move(
    pos: &mut Pos,
    hit_size: f32,
    deltax: f32,
    deltay: f32,
    solid: &dyn Fn(i32, i32) -> bool,
) {
    // NaN guard (`deltax != deltax`).
    if (deltax.abs() < 0.0001 && deltay.abs() < 0.0001) || deltax.is_nan() || deltay.is_nan() {
        return;
    }
    let mut deltax = deltax.clamp(-MAX_DELTA, MAX_DELTA);
    let mut deltay = deltay.clamp(-MAX_DELTA, MAX_DELTA);
    let size = (hit_size * 0.66).min(7.8);
    let mut rect = Rect::centered(pos.x, pos.y, size, size);
    let ts = crate::config::TILESIZE as f32;
    // `int r = Math.max(Math.round(r1.width / tilesize), 1)` (`Mathf.round`).
    let r = (((size / ts) + 0.5).floor() as i32).max(1);

    let mut movedx = false;
    while deltax.abs() > 0.0 || !movedx {
        movedx = true;
        move_delta(
            &mut rect,
            deltax.abs().min(SEG) * sign(deltax),
            0.0,
            r,
            solid,
        );
        if deltax.abs() >= SEG {
            deltax -= SEG * sign(deltax);
        } else {
            deltax = 0.0;
        }
    }
    let mut movedy = false;
    while deltay.abs() > 0.0 || !movedy {
        movedy = true;
        move_delta(
            &mut rect,
            0.0,
            deltay.abs().min(SEG) * sign(deltay),
            r,
            solid,
        );
        if deltay.abs() >= SEG {
            deltay -= SEG * sign(deltay);
        } else {
            deltay = 0.0;
        }
    }
    pos.x = rect.x + rect.w / 2.0;
    pos.y = rect.y + rect.h / 2.0;
}

/// `VelComp.update` (`@MethodPriority(-1)`, VelComp.java:23-34): integrate
/// `vel`, zero an axis that tile collisions blocked (`Mathf.equal`), apply
/// drag. Called as the first per-unit step of the tick, before the controller.
pub fn vel_update(world: &mut World, grid: &WorldGrid, content: &ContentRegistry, entity: Entity) {
    let Some(mut pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let Some(mut vel) = world.get::<Vel>(entity).copied() else {
        return;
    };
    let drag = world
        .get::<PhysicsComp>(entity)
        .map_or(0.0, |physics| physics.drag);
    let hit_size = world
        .get::<HitboxComp>(entity)
        .map_or(0.0, |hitbox| hitbox.hit_size);
    let (px, py) = (pos.x, pos.y);

    // `SolidPred check = solidity(); if(check != null) collisions.move(..) else { x += cx; y += cy; }`
    // (VelComp.java:65-74). The port's per-kind `solidity()` is
    // `ground::unit_can_pass`; it always passes for flyers.
    if crate::combat::bullet::unit_is_flying(world, entity) {
        pos.x += vel.x;
        pos.y += vel.y;
    } else {
        let solid = |x: i32, y: i32| {
            !crate::ai::types::ground::unit_can_pass(world, grid, content, entity, x, y)
        };
        collisions_move(&mut pos, hit_size, vel.x, vel.y, &solid);
    }

    if (px - pos.x).abs() <= FLOAT_ROUNDING_ERROR {
        vel.x = 0.0;
    }
    if (py - pos.y).abs() <= FLOAT_ROUNDING_ERROR {
        vel.y = 0.0;
    }
    let scale = (1.0 - drag).max(0.0);
    vel.x *= scale;
    vel.y *= scale;

    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;

    #[test]
    fn move_at_limits_by_accel() {
        let mut harness = UnitHarness::new(16, 16, 1);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        // vel (0,0), move_at(2, 0, 0.5) -> limit 0.5 * 2 = 1 -> vel (1, 0).
        move_at(&mut harness.build.world, unit, 2.0, 0.0, 0.5);
        let vel = harness.build.world.get::<Vel>(unit).copied().expect("vel");
        assert!((vel.x - 1.0).abs() < 1e-6 && vel.y.abs() < 1e-6, "{vel:?}");
        // Again: (2, 0) is within one more limit step (1 remaining) -> (2, 0).
        move_at(&mut harness.build.world, unit, 2.0, 0.0, 0.5);
        let vel = harness.build.world.get::<Vel>(unit).copied().expect("vel");
        assert!((vel.x - 2.0).abs() < 1e-6 && vel.y.abs() < 1e-6, "{vel:?}");
    }

    #[test]
    fn collisions_move_stops_at_wall() {
        // 20x20 grid; a wall tile at (10, 5) spans [76, 84) on x.
        let mut harness = UnitHarness::new(20, 20, 2);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.build.place(10, 5, wall, 0, true));
        harness.refresh_path_tiles();
        let unit = harness.spawn("dagger", 0, 72.0, 40.0, 0.0).expect("dagger");
        // Dagger hitSize 8 -> box 5.28; a 4-px step right must stop at
        // `76 - 2.64` (the wall's left edge minus half the box).
        let mut pos = harness.build.world.get::<Pos>(unit).copied().expect("pos");
        let solid = |x: i32, y: i32| {
            !crate::ai::types::ground::unit_can_pass(
                &harness.build.world,
                &harness.build.grid,
                &harness.build.content,
                unit,
                x,
                y,
            )
        };
        collisions_move(&mut pos, 8.0, 4.0, 0.0, &solid);
        assert!(pos.x <= 76.0 - 2.64 + 0.001, "stopped at wall: {}", pos.x);
    }
}
