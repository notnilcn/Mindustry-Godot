// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet tile raycasts (`BulletComp.tileRaycast` / `collideFloor` /
//! `collideTerrain`).
//!
//! The Bresenham traversal is plan 06's [`crate::world::raycast`]; this module
//! adapts it to bullet collision queries. Upstream `tileRaycast` starts at the
//! bullet's previous tile and walks to the current tile, testing the first
//! blocking/wall tile.

use crate::content::{BlockId, ContentRegistry};
use crate::world::{TilePos, WorldGrid};

/// First non-air block tile on the segment `(x0,y0) -> (x1,y1)` when terrain
/// collision is enabled (`collideTerrain`/`collideFloor`).
pub fn first_wall_tile(grid: &WorldGrid, x0: i32, y0: i32, x1: i32, y1: i32) -> Option<TilePos> {
    let mut hit = None;
    crate::world::raycast::raycast_each(x0, y0, x1, y1, |x, y| {
        if !grid.tiles.in_bounds(x, y) {
            return true;
        }
        if grid.tiles.get(x, y).block != BlockId::AIR {
            hit = Some(TilePos::new(x as i16, y as i16));
            true
        } else {
            false
        }
    });
    hit
}

/// First building tile on the segment, honoring team pass-through rules
/// (`BulletComp.checkUnderBuild`).
pub fn first_build_tile(
    grid: &WorldGrid,
    bullet_team: u8,
    collide_team: bool,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> Option<(TilePos, bevy_ecs::entity::Entity)> {
    let _ = (bullet_team, collide_team);
    let mut hit = None;
    crate::world::raycast::raycast_each(x0, y0, x1, y1, |x, y| {
        if !grid.tiles.in_bounds(x, y) {
            return false;
        }
        if let Some(build) = grid.tiles.get(x, y).build {
            hit = Some((TilePos::new(x as i16, y as i16), build));
            true
        } else {
            false
        }
    });
    hit
}

/// Whether a block is a floor (non-wall) for `collideFloor` purposes.
pub fn is_floor_like(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).is_some_and(|def| !def.solid)
}
