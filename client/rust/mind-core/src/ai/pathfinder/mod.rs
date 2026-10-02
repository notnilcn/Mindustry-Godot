// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Pathfinding (plan 11 §3.7): `Pathfinder` flowfields, `PathTile` packing and
//! cost types. M0 ships cost-ground flowfields; M3 adds the remaining costs,
//! `PositionTarget`/`EnemyCoreField` refresh budgets and `ControlPathfinder`.

pub mod cost;
pub mod flowfield;
pub mod path_tile;
pub mod queue;
pub mod worker;

use std::collections::BTreeMap;

use crate::content::ContentRegistry;
use crate::world::{TilePos, WorldGrid};

pub use cost::{Cost, MAX_COSTS};
pub use flowfield::Flowfield;
pub use path_tile::PathTile;
pub use queue::PathfindQueue;
pub use worker::{
    AVOID_INTERVAL, CLUSTER_SIZE, CONTROL_INVALIDATE_INTERVAL_TICKS, CONTROL_NODES_PER_TICK,
    FIELD_IDLE_TIMEOUT_TICKS, FLOWFIELD_NODES_PER_TICK, FlowfieldOp, MAX_COMMAND_QUEUE,
    REFRESH_INTERVAL_TICKS, REQUEST_IDLE_TIMEOUT_TICKS,
};

/// Per-team, per-cost flowfield cache (`Pathfinder.cache[team][cost]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FieldKey {
    team: u8,
    cost: u8,
}

/// The pathfinder resource (upstream `Pathfinder` statics; plan 11 §3.7).
#[derive(Debug)]
pub struct Pathfinder {
    /// Grid width in tiles.
    pub width: i32,
    /// Grid height in tiles.
    pub height: i32,
    /// Packed `PathTile` per flat index.
    pub tiles: Vec<PathTile>,
    /// Cached fields keyed by `(team, cost)`.
    fields: BTreeMap<FieldKey, Flowfield>,
    /// Monotonic field-recompute counter (determinism/debug).
    pub updates: u64,
}

impl Pathfinder {
    /// Creates an empty pathfinder for a `width x height` grid.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            tiles: Vec::new(),
            fields: BTreeMap::new(),
            updates: 0,
        }
    }

    /// Rebuilds the packed tile layer, e.g. on `WorldLoadEvent` or a tile change.
    pub fn rebuild(&mut self, grid: &WorldGrid, content: &ContentRegistry, team: u8) {
        self.width = grid.width();
        self.height = grid.height();
        self.tiles = path_tile::build_tiles(grid, content, team);
        self.fields.clear();
    }

    /// Updates one tile's packed data (`TileChangeEvent` main-thread path).
    pub fn update_tile(
        &mut self,
        grid: &WorldGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
        team: u8,
    ) {
        if !grid.tiles.in_bounds(x, y) {
            return;
        }
        let index = grid.tiles.index(x, y);
        let tile = grid.tile_ref(index);
        let def = content.block(tile.block);
        let solid = def.map(|def| def.solid).unwrap_or(false);
        let health = def
            .map(|def| ((def.health as f32 / 40.0).min(80.0)) as u8)
            .unwrap_or(0);
        self.tiles[index] = PathTile::from_parts(health, team, solid, false, false);
        // Invalidate every cached field; it is rebuilt lazily on next access.
        self.fields.clear();
    }

    /// Returns (building if needed) the flowfield for `(team, cost)` targeting
    /// `targets` (upstream `getField`). `targets` are tile positions.
    pub fn get_field(&mut self, cost: Cost, team: u8, targets: &[TilePos]) -> &Flowfield {
        use std::collections::btree_map::Entry;
        let key = FieldKey {
            team,
            cost: cost.id(),
        };
        let mut target_indices: Vec<usize> = targets
            .iter()
            .filter(|pos| {
                self.width > 0
                    && self.height > 0
                    && pos.x() >= 0
                    && pos.y() >= 0
                    && (pos.x() as i32) < self.width
                    && (pos.y() as i32) < self.height
            })
            .map(|pos| pos.x() as usize + pos.y() as usize * self.width as usize)
            .collect();
        target_indices.sort_unstable();
        target_indices.dedup();

        match self.fields.entry(key) {
            Entry::Occupied(mut entry) => {
                if entry.get().targets != target_indices {
                    let field =
                        build_field(&self.tiles, self.width, self.height, cost, target_indices);
                    entry.insert(field);
                    self.updates = self.updates.wrapping_add(1);
                }
                entry.into_mut()
            }
            Entry::Vacant(entry) => {
                let field = build_field(&self.tiles, self.width, self.height, cost, target_indices);
                self.updates = self.updates.wrapping_add(1);
                entry.insert(field)
            }
        }
    }

    /// Number of cached fields (debug/tests).
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Advances the deterministic worker budget. M0 fields are computed lazily
    /// and synchronously, so this is a no-op until M3.
    pub fn step(&mut self, _budget: u32) {}
}

/// Builds and solves a flowfield for `targets`.
fn build_field(
    tiles: &[PathTile],
    width: i32,
    height: i32,
    cost: Cost,
    targets: Vec<usize>,
) -> Flowfield {
    let mut field = Flowfield::new(width, height);
    field.targets = targets;
    field.update(tiles, width, height, cost, true);
    field
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn ground_field_reaches_across_flat_grid() {
        let content = content();
        let mut grid = WorldGrid::new(32, 8);
        grid.fill(crate::content::BlockId::AIR, crate::content::BlockId::AIR);
        let mut path = Pathfinder::new(grid.width(), grid.height());
        path.rebuild(&grid, &content, 0);
        let field = path.get_field(Cost::Ground, 0, &[TilePos::new(30, 4)]);
        assert!(field.done);
        assert!(field.complete_weights[4 * 32 + 1].is_finite());
        // Cached second call returns the same field.
        let count = path.field_count();
        let _ = path.get_field(Cost::Ground, 0, &[TilePos::new(30, 4)]);
        assert_eq!(path.field_count(), count);
    }
}
