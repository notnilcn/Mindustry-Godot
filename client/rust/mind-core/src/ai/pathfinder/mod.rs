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
    /// Fields with an in-progress incremental solve, in deterministic enqueue
    /// order (`Pathfinder.queue` + `threadList`).
    pending: Vec<FieldKey>,
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
            pending: Vec::new(),
            updates: 0,
        }
    }

    /// Rebuilds the packed tile layer, e.g. on `WorldLoadEvent` or a tile change.
    pub fn rebuild(&mut self, grid: &WorldGrid, content: &ContentRegistry, team: u8) {
        self.width = grid.width();
        self.height = grid.height();
        self.tiles = path_tile::build_tiles(grid, content, team);
        self.fields.clear();
        self.pending.clear();
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
        self.pending.clear();
    }

    /// Returns (building if needed) the flowfield for `(team, cost)` targeting
    /// `targets` (upstream `getField`). `targets` are tile positions.
    pub fn get_field(&mut self, cost: Cost, team: u8, targets: &[TilePos]) -> &Flowfield {
        use std::collections::btree_map::Entry;
        let key = FieldKey {
            team,
            cost: cost.id(),
        };
        let target_indices = self.normalize_targets(targets);

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

    /// Filters and packs valid target tiles (`Flowfield.getPositions` output).
    fn normalize_targets(&self, targets: &[TilePos]) -> Vec<usize> {
        let mut indices: Vec<usize> = targets
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
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    /// Registers (or re-targets) a field for incremental building; returns
    /// whether a solve was started. [`step`](Self::step) advances it. This is
    /// the deterministic replacement for the upstream background
    /// `Pathfinder.registerPath`/`updateFrontier` pair; [`get_field`](Self::get_field)
    /// remains the eager single-call path.
    pub fn request_field(&mut self, cost: Cost, team: u8, targets: &[TilePos]) -> bool {
        let key = FieldKey {
            team,
            cost: cost.id(),
        };
        let target_indices = self.normalize_targets(targets);
        let needs_start = match self.fields.get(&key) {
            Some(field) => field.targets != target_indices,
            None => true,
        };
        if !needs_start {
            return false;
        }
        let width = self.width;
        let height = self.height;
        let field = self
            .fields
            .entry(key)
            .or_insert_with(|| Flowfield::new(width, height));
        let expected = (width.max(0) as usize) * (height.max(0) as usize);
        if field.complete_weights.len() != expected {
            *field = Flowfield::new(width, height);
        }
        field.targets = target_indices;
        field.begin_update(width, height);
        self.updates = self.updates.wrapping_add(1);
        if !self.pending.contains(&key) {
            self.pending.push(key);
        }
        true
    }

    /// Advances the deterministic incremental frontier by `budget` node pops,
    /// split evenly across in-progress fields (`FLOWFIELD_NODES_PER_TICK`).
    pub fn step(&mut self, budget: u32) {
        if self.pending.is_empty() || self.tiles.is_empty() || budget == 0 {
            return;
        }
        let share = (budget / self.pending.len() as u32).max(1);
        let pending = std::mem::take(&mut self.pending);
        let mut still = Vec::with_capacity(pending.len());
        for key in pending {
            let Some(cost) = Cost::from_id(key.cost) else {
                continue;
            };
            let Some(field) = self.fields.get_mut(&key) else {
                continue;
            };
            let done = field.advance(&self.tiles, self.width, self.height, cost, true, share);
            if !done {
                still.push(key);
            }
        }
        self.pending = still;
    }
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

    #[test]
    fn incremental_step_matches_eager_solve() {
        let content = content();
        let mut grid = WorldGrid::new(24, 16);
        grid.fill(crate::content::BlockId::AIR, crate::content::BlockId::AIR);
        // A partial wall so the field has interesting structure.
        for y in 4..12i16 {
            grid.set_block(
                TilePos::new(9, y),
                crate::content::BlockId::STONE_WALL,
                0,
                0,
            )
            .expect("wall");
        }
        let targets = [TilePos::new(20, 8)];

        let mut eager = Pathfinder::new(grid.width(), grid.height());
        eager.rebuild(&grid, &content, 0);
        let expected = eager
            .get_field(Cost::Ground, 0, &targets)
            .complete_weights
            .clone();

        let mut inc = Pathfinder::new(grid.width(), grid.height());
        inc.rebuild(&grid, &content, 0);
        assert!(inc.request_field(Cost::Ground, 0, &targets));
        // A small per-tick budget must converge over several calls.
        let mut calls = 0;
        while inc
            .fields
            .get(&FieldKey { team: 0, cost: 0 })
            .is_some_and(|f| !f.done)
        {
            inc.step(4);
            calls += 1;
            assert!(calls < 1_000, "incremental field never completed");
        }
        let got = inc
            .get_field(Cost::Ground, 0, &targets)
            .complete_weights
            .clone();
        assert_eq!(got, expected, "incremental == eager solve");
    }
}
