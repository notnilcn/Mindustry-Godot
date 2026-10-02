// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Tile → building-entity index (`World.tile(x,y).build` lookup for behaviors).
//!
//! Rust behaviors receive only the ECS `World`; Java blocks call
//! `tile.nearby(dir)` which needs the grid. This resource mirrors the grid's
//! occupancy and is kept in sync by [`crate::world::ops::WorldCtx`] mutations
//! and by the harness, so logistics blocks can resolve the building on an
//! adjacent/range tile without a `WorldGrid` borrow.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;

use super::WorldGrid;

/// Dense tile → building-entity mirror of the grid occupancy.
#[derive(Debug, Default, Clone, Resource)]
pub struct TileBuilds {
    /// Grid width.
    pub width: i32,
    /// Grid height.
    pub height: i32,
    /// Row-major cells.
    pub cells: Vec<Option<Entity>>,
}

impl TileBuilds {
    /// Rebuilds the mirror from a grid.
    pub fn rebuild(&mut self, grid: &WorldGrid) {
        self.width = grid.width();
        self.height = grid.height();
        self.cells.clear();
        self.cells.extend(grid.tiles.iter().map(|tile| tile.build));
    }

    /// Building entity occupying `(x, y)`, if any.
    pub fn get(&self, x: i32, y: i32) -> Option<Entity> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        self.cells
            .get((x + y * self.width) as usize)
            .copied()
            .flatten()
    }

    /// Whether the index matches the given grid shape.
    pub fn matches(&self, grid: &WorldGrid) -> bool {
        self.width == grid.width() && self.height == grid.height() && self.cells.len() == grid.len()
    }
}
