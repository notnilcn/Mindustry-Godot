// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `GroundAI` (plan 11 §4.2). M0 ports the core loop: pathfind to the target,
//! steer toward the next flowfield tile, face movement, and stop on arrival.
//! Stuck detection/avoidance (`pathfind(..., avoidance)`) lands with M3.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::{HitboxComp, PhysicsComp, UnitCore};
use crate::entities::comp::{Pos, Vel};
use crate::world::{TilePos, WorldGrid};

use super::super::controller::UnitController;
use super::super::pathfinder::{Cost, Pathfinder};

/// Default arrival tolerance in world units when a unit has no hitbox.
pub const DEFAULT_ARRIVE: f32 = 4.0;

/// Ground controller state (`GroundAI`).
#[derive(Debug, Default, Clone, Copy)]
pub struct GroundAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current move target.
    pub target: Option<TilePos>,
}

impl GroundAi {
    /// Creates a controller with no target.
    pub fn new() -> Self {
        Self::default()
    }
}

impl UnitController for GroundAi {
    fn unit(&self) -> Option<Entity> {
        self.unit
    }

    fn set_unit(&mut self, unit: Entity) {
        self.unit = Some(unit);
    }
}

/// World-pixel center of a tile (`(x + 0.5) * TILESIZE`).
pub fn tile_center(x: i32, y: i32) -> (f32, f32) {
    let ts = crate::config::TILESIZE as f32;
    ((x as f32 + 0.5) * ts, (y as f32 + 0.5) * ts)
}

/// Normalizes degrees into `[0, 360)`.
pub fn normalize_angle(degrees: f32) -> f32 {
    let mut a = degrees % 360.0;
    if a < 0.0 {
        a += 360.0;
    }
    a
}

/// Steers `pos`/`vel` toward `(dest_x, dest_y)` at up to `speed` units/tick.
///
/// Returns `true` when within `arrive` of the destination (velocity zeroed).
/// The step is capped to the remaining distance so a unit never overshoots.
pub fn approach(
    pos: &mut Pos,
    vel: &mut Vel,
    core: &mut UnitCore,
    speed: f32,
    dest_x: f32,
    dest_y: f32,
    arrive: f32,
) -> bool {
    let dx = dest_x - pos.x;
    let dy = dest_y - pos.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist <= arrive {
        vel.x = 0.0;
        vel.y = 0.0;
        return true;
    }
    let dir_x = dx / dist;
    let dir_y = dy / dist;
    let step = speed.min(dist);
    vel.x = dir_x * step;
    vel.y = dir_y * step;
    pos.x += vel.x;
    pos.y += vel.y;
    // Face movement (upstream `UnitComp.rotateMove` fallback).
    if step > 0.0001 {
        core.rotation = normalize_angle(dir_y.atan2(dir_x).to_degrees());
    }
    false
}

/// Runs one ground-AI tick for `entity`.
///
/// Reads the current tile from `grid`, asks `pathfinder` for the next tile, and
/// steers toward it. Returns `true` when the unit arrived this tick.
pub fn update_ground(
    world: &mut World,
    grid: &WorldGrid,
    pathfinder: &mut Pathfinder,
    team: u8,
    entity: Entity,
    target: TilePos,
) -> bool {
    let (pos, speed, hit_size) = {
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            return false;
        };
        let speed = world
            .get::<PhysicsComp>(entity)
            .map(|physics| physics.speed)
            .unwrap_or(1.0);
        let hit_size = world
            .get::<HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        (pos, speed, hit_size)
    };

    let current = TilePos::new(
        WorldGrid::to_tile(pos.x) as i16,
        WorldGrid::to_tile(pos.y) as i16,
    );
    if !grid.tiles.in_bounds(current.x() as i32, current.y() as i32) {
        return false;
    }
    let arrive = hit_size.max(DEFAULT_ARRIVE);

    let dest = if current == target {
        tile_center(target.x() as i32, target.y() as i32)
    } else {
        let width = pathfinder.width;
        let height = pathfinder.height;
        let index = current.x() as usize + current.y() as usize * width as usize;
        let next = pathfinder
            .get_field(Cost::Ground, team, &[target])
            .get_target_tile(index, width, height, true);
        match next {
            Some(next) => tile_center((next as i32) % width, (next as i32) / width),
            None => tile_center(target.x() as i32, target.y() as i32),
        }
    };

    let mut pos = pos;
    let mut vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    let mut core = world
        .get::<UnitCore>(entity)
        .copied()
        .unwrap_or(UnitCore::new(0.0));
    let arrived = approach(&mut pos, &mut vel, &mut core, speed, dest.0, dest.1, arrive);
    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
    if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
        *stored = core;
    }
    arrived
}
