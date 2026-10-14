// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `GroundAI` (plan 11 §4.2). M0 ports the core loop: pathfind to the target,
//! steer toward the next flowfield tile, face movement, and stop on arrival.
//! Stuck detection/avoidance (`pathfind(..., avoidance)`) lands with M3.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::comp::{CrawlComp, LegsComp, WaterMoveComp};
use crate::entities::comp::unit::{HitboxComp, UnitCore};
use crate::entities::comp::{Pos, Vel};
use crate::world::{TilePos, WorldGrid};

use super::super::controller::UnitController;
use super::super::pathfinder::{Cost, Pathfinder};

/// Default arrival tolerance in world units when a unit has no hitbox.
pub const DEFAULT_ARRIVE: f32 = 4.0;

/// `GroundAI` per-unit timer/target state (`AIController.timer` +
/// `GroundAI.stuck*`, AIController.java:22-27, GroundAI.java:12-17).
///
/// Inserted lazily by the live unit loop; the Java controller keeps the same
/// fields on its instance (`keepState()` is false, so a save/load reset is
/// faithful with a fresh component).
#[derive(Debug, Clone, Copy, bevy_ecs::component::Component)]
pub struct GroundAiState {
    /// `AIController.timer.get(timerTarget, target == null ? 40 : 90)` countdown.
    pub retarget: f32,
    /// Main target (`AIController.target`).
    pub target: Option<Entity>,
    /// `GroundAI.stuckTime`.
    pub stuck_time: f32,
    /// Last position for the stuck check (`GroundAI.stuckX`); Java seeds -999.
    pub stuck_x: f32,
    /// Last position for the stuck check (`GroundAI.stuckY`); Java seeds -999.
    pub stuck_y: f32,
}

impl Default for GroundAiState {
    fn default() -> Self {
        Self {
            // Java `resetTimers` seeds `Mathf.random(40)`; the port takes the
            // earliest draw so the first acquisition runs on the first tick.
            retarget: 39.0,
            target: None,
            stuck_time: 0.0,
            stuck_x: -999.0,
            stuck_y: -999.0,
        }
    }
}

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

/// World-pixel of a tile's `worldx`/`worldy` (`Tile.worldx()`, Tile.java:166-171).
pub fn tile_center(x: i32, y: i32) -> (f32, f32) {
    let ts = crate::config::TILESIZE as f32;
    (x as f32 * ts, y as f32 * ts)
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
    content: &crate::content::ContentRegistry,
    pathfinder: &mut Pathfinder,
    team: u8,
    entity: Entity,
    target: TilePos,
) -> bool {
    let (pos, hit_size) = {
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            return false;
        };
        let hit_size = world
            .get::<HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        (pos, hit_size)
    };
    // `UnitComp.speed()` with floor/boost/status multipliers (PAR-0137).
    let speed = crate::entities::comp::unit::movement::unit_speed(world, grid, content, entity);

    let current = TilePos::new(
        WorldGrid::to_tile(pos.x) as i16,
        WorldGrid::to_tile(pos.y) as i16,
    );
    if !grid.tiles.in_bounds(current.x() as i32, current.y() as i32) {
        return false;
    }
    let arrive = hit_size.max(DEFAULT_ARRIVE);

    // Arrival is judged against the final target only: steering at an
    // intermediate flowfield tile uses a zero radius, otherwise a unit sitting
    // within `hitSize` of the next tile reported "arrived" on its first tick
    // and its move order was dropped before it moved (PAR-0140).
    let (dest, arrive) = if current == target {
        (tile_center(target.x() as i32, target.y() as i32), arrive)
    } else {
        let dest = {
            let width = pathfinder.width;
            let index = current.x() as usize + current.y() as usize * width as usize;
            let next = pathfinder.target_tile(Cost::Ground, team, &[target], index, true);
            match next {
                Some(next) => tile_center((next as i32) % width, (next as i32) / width),
                None => tile_center(target.x() as i32, target.y() as i32),
            }
        };
        (dest, 0.0)
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
    let arrived =
        approach(&mut pos, &mut vel, &mut core, speed, dest.0, dest.1, arrive) && current == target;
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

/// `AIController.pathfind(field, stopAtTargetTile, avoidance)`
/// (AIController.java:138-160): ask the team flowfield for the next tile and
/// steer at its centre at `prefSpeed`. Returns whether the controller decided
/// to move (Java's `moved`).
///
/// `targets` must be stable across units of a team within a tick, otherwise
/// the `(team, cost)` field cache rebuilds per unit (plan G01 §6.9).
#[allow(clippy::too_many_arguments)]
pub fn pathfind_field(
    world: &mut World,
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    pathfinder: &mut Pathfinder,
    team: u8,
    entity: Entity,
    cost: Cost,
    targets: &[TilePos],
    stop_at_target_tile: bool,
) -> bool {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return false;
    };
    // `Tile tile = unit.tileOn(); if(tile == null) return;`
    let current = TilePos::new(
        WorldGrid::to_tile(pos.x) as i16,
        WorldGrid::to_tile(pos.y) as i16,
    );
    if !grid.tiles.in_bounds(current.x() as i32, current.y() as i32) {
        return false;
    }
    let width = pathfinder.width;
    let index = current.x() as usize + current.y() as usize * width as usize;
    // `Flowfield.getNextTile(tile, avoidance ? unit.id : 0)`: the port's
    // flowfield has no per-unit avoidance flag, so the avoid id is unused.
    let next = pathfinder
        .target_tile(cost, team, targets, index, true)
        .map(|next| ((next as i32) % width, (next as i32) / width));
    let Some((nx, ny)) = next else {
        return false;
    };
    // `if((tile == targetTile && stopAtTargetTile) || !unit.canPass(...)) return;`
    if (current.x() as i32 == nx && current.y() as i32 == ny && stop_at_target_tile)
        || !unit_can_pass(world, grid, content, entity, nx, ny)
    {
        return false;
    }
    let speed = crate::entities::comp::unit::movement::unit_speed(world, grid, content, entity);
    let mut vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    let mut core = world
        .get::<UnitCore>(entity)
        .copied()
        .unwrap_or(UnitCore::new(0.0));
    let mut pos = pos;
    // `movePref(vec.trns(angleTo(targetTile.worldx(), targetTile.worldy()), prefSpeed()))`:
    // `Tile.worldx()` is the tile CORNER (`x * tilesize`), not the centre.
    let dest = (
        nx as f32 * crate::config::TILESIZE as f32,
        ny as f32 * crate::config::TILESIZE as f32,
    );
    let _ = approach(&mut pos, &mut vel, &mut core, speed, dest.0, dest.1, 0.0);
    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
    if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
        *stored = core;
    }
    true
}

/// `VelComp.canPass(x, y)` with the per-kind `solidity()` predicate
/// (VelComp.java:38-53, EntityCollisions.java:129-142).
pub fn unit_can_pass(
    world: &World,
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    entity: Entity,
    x: i32,
    y: i32,
) -> bool {
    // `ElevationMoveComp.solidity`: `isFlying() ? null : EntityCollisions::solid`
    // (ElevationMoveComp.java:14-16).
    if crate::combat::bullet::unit_is_flying(world, entity) {
        return true;
    }
    let solid = |x: i32, y: i32| -> bool {
        grid.tiles
            .getn(x, y)
            .is_none_or(|tile| crate::ai::pathfinder::path_tile::is_solid(content, tile.block))
    };
    // `WaterMoveComp.solidity`/`WaterCrawlComp.solidity` -> `waterSolid`
    // (WaterMoveComp.java:65-67).
    if world.get::<WaterMoveComp>(entity).is_some() {
        let table = world.get_resource::<crate::world::BlockTable>();
        return match table {
            Some(table) => {
                !crate::entities::comp::unit::water_move::water_solid(grid, content, table, x, y)
            }
            None => !solid(x, y),
        };
    }
    // `CrawlComp.solidity` -> `legsSolid`; `LegsComp.solidity` ->
    // `allowLegStep ? legsSolid : solid` (CrawlComp.java:30-32,
    // LegsComp.java:40-42).
    let leg_step = world.get::<LegsComp>(entity).is_some()
        && world
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
            .and_then(|comp| content.unit(comp.type_id))
            .is_some_and(|def| def.allow_leg_step);
    if leg_step || world.get::<CrawlComp>(entity).is_some() {
        // `EntityCollisions.legsSolid`: `tile == null || tile.legSolid()`.
        return grid
            .tiles
            .getn(x, y)
            .is_none_or(|tile| !tile.leg_solid(content));
    }
    !solid(x, y)
}
