// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Centralised tile mutation (`world/Tile.java` + `core.World`, plan 06 §3.4).
//!
//! Every write to a [`Tile`] goes through [`WorldCtx`] so event counters,
//! proximity, multiblock linkage and recache stay in one place (plan 06 §3.13).
//! Java's `@Remote` static helpers become plain methods here; plan 21 wraps them
//! as reducers.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};

use super::WorldGrid;
use super::events::{TileChangeEvent, TileFloorChangeEvent, TileOverlayChangeEvent};
use super::hooks::{NewBuilding, RenderHooks, WorldHooks};
use super::tiles::Tiles;

/// Collected tile events for one operation batch (plan-05 bus registration is a
/// follow-up; observably identical to upstream counters — plan 06 §3.4).
#[derive(Debug, Default, Clone)]
pub struct WorldEventLog {
    /// `TileChangeEvent` records, in fire order.
    pub tile_changes: Vec<TileChangeEvent>,
    /// `TileFloorChangeEvent` records.
    pub floor_changes: Vec<TileFloorChangeEvent>,
    /// `TileOverlayChangeEvent` records.
    pub overlay_changes: Vec<TileOverlayChangeEvent>,
}

impl WorldEventLog {
    /// Clears the log.
    pub fn clear(&mut self) {
        self.tile_changes.clear();
        self.floor_changes.clear();
        self.overlay_changes.clear();
    }
}

/// The explicit world-operation bundle (deviation §2.3.1).
pub struct WorldCtx<'a> {
    /// The world grid being mutated.
    pub grid: &'a mut WorldGrid,
    /// Content registry (block metadata lookups).
    pub content: &'a ContentRegistry,
    /// ECS world that owns building entities.
    pub ecs: &'a mut World,
    /// Building-lifecycle hooks.
    pub hooks: &'a dyn WorldHooks,
    /// Render invalidation hooks.
    pub render: &'a dyn RenderHooks,
    /// Event sink.
    pub log: &'a mut WorldEventLog,
}

impl WorldCtx<'_> {
    fn in_bounds(&self, x: i16, y: i16) -> bool {
        self.grid.tiles.in_bounds(x as i32, y as i32)
    }

    fn index(&self, x: i16, y: i16) -> usize {
        self.grid.tiles.index(x as i32, y as i32)
    }

    /// Sets the floor of a tile (`Tile.setFloor`).
    pub fn set_floor(&mut self, x: i16, y: i16, floor: BlockId) {
        if !self.in_bounds(x, y) {
            return;
        }
        let index = self.index(x, y);
        let prev = self.grid.tiles.geti(index).floor;
        if prev == floor {
            return;
        }
        if self.grid.generating {
            self.grid.tiles.geti_mut(index).floor = floor;
            return;
        }
        self.grid.tiles.geti_mut(index).floor = floor;
        // Upstream `World` counts only `TileFloorChangeEvent` (not tileChanges).
        self.grid.floor_changes = self.grid.floor_changes.wrapping_add(1);
        self.log.floor_changes.push(TileFloorChangeEvent {
            x,
            y,
            prev,
            next: floor,
        });
        self.render.remove_floor_index(x, y);
        if let Some(entity) = self.grid.tiles.geti(index).build {
            self.hooks.update_proximity(self.ecs, entity);
        }
        self.hooks.floor_changed(floor, x, y);
    }

    /// Sets the overlay of a tile (`Tile.setOverlay`).
    pub fn set_overlay(&mut self, x: i16, y: i16, overlay: BlockId) {
        if !self.in_bounds(x, y) {
            return;
        }
        let index = self.index(x, y);
        let prev = self.grid.tiles.geti(index).overlay;
        if prev == overlay {
            return;
        }
        self.grid.tiles.geti_mut(index).overlay = overlay;
        if self.grid.generating {
            return;
        }
        // Overlay events increment no upstream counter.
        self.log.overlay_changes.push(TileOverlayChangeEvent {
            x,
            y,
            prev,
            next: overlay,
        });
    }

    /// Sets the floor and overlay together (`Tile.setFloor` + `setOverlay`).
    pub fn set_floor_and_overlay(&mut self, x: i16, y: i16, floor: BlockId, overlay: BlockId) {
        self.set_floor(x, y, floor);
        self.set_overlay(x, y, overlay);
    }

    /// Sets the block of a tile (`Tile.setBlock`/`Tile.changeBuild`).
    ///
    /// Implements the upstream order: pre-change, old-building removal,
    /// multiblock two-pass assignment, post-change proximity + recache.
    pub fn set_block(&mut self, x: i16, y: i16, block: BlockId, team: u8, rot: u8) {
        if !self.in_bounds(x, y) {
            return;
        }
        let index = self.index(x, y);
        let old_entity = self.grid.tiles.geti(index).build;
        let old_block = self.grid.tiles.geti(index).block;

        // Remove the old building (and all its multiblock tiles).
        if let Some(entity) = old_entity {
            self.clear_building(entity);
        }

        self.grid.tiles.geti_mut(index).changing = true;

        // Create the new building entity (if the block has one).
        let new_entity = if super::block_has_building(self.content, block) {
            let request = NewBuilding {
                block,
                x,
                y,
                team,
                rot,
            };
            self.hooks.new_building(self.ecs, request)
        } else {
            None
        };

        self.assign_footprint(x, y, block, new_entity);

        let changed = old_block != block;
        if changed && !self.grid.generating {
            self.grid.tile_changes = self.grid.tile_changes.wrapping_add(1);
            self.log.tile_changes.push(TileChangeEvent { x, y });
        }

        if let Some(entity) = self.grid.tiles.geti(index).build {
            if !self.grid.generating {
                self.hooks.update_proximity(self.ecs, entity);
            }
        } else if !self.grid.generating {
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if self.in_bounds(nx, ny) {
                    let n_index = self.index(nx, ny);
                    if let Some(entity) = self.grid.tiles.geti(n_index).build {
                        self.hooks.update_proximity(self.ecs, entity);
                    }
                }
            }
        }

        self.render.recache_tile(x, y);
        self.render.recache_wall(x, y);
        self.hooks.block_changed(block, x, y);
        self.grid.tiles.geti_mut(index).changing = false;
    }

    /// Removes a block, setting the tile to `air` (`Tile.remove`).
    pub fn remove_block(&mut self, x: i16, y: i16) {
        self.set_block(x, y, BlockId::AIR, 0, 0);
    }

    /// Sets the tile to `air` without firing block-change hooks
    /// (`Tile.setAir`); used for erase operations.
    pub fn set_air(&mut self, x: i16, y: i16) {
        if !self.in_bounds(x, y) {
            return;
        }
        let index = self.index(x, y);
        if let Some(entity) = self.grid.tiles.geti(index).build {
            self.clear_building(entity);
        }
        let tile = self.grid.tiles.geti_mut(index);
        tile.block = BlockId::AIR;
        tile.build = None;
        if !self.grid.generating {
            self.grid.tile_changes = self.grid.tile_changes.wrapping_add(1);
            self.log.tile_changes.push(TileChangeEvent { x, y });
        }
    }

    /// Assigns `block`/`build` across a block's footprint
    /// (`Tile.changeBuild` multiblock loop). `entity` is the center entity.
    pub fn assign_footprint(&mut self, x: i16, y: i16, block: BlockId, entity: Option<Entity>) {
        let size = self
            .content
            .block(block)
            .map(|def| def.size)
            .unwrap_or(1)
            .max(1);
        let offset = -(size - 1) / 2;
        for dx in 0..size {
            for dy in 0..size {
                let tx = x as i32 + offset + dx;
                let ty = y as i32 + offset + dy;
                if !self.grid.tiles.in_bounds(tx, ty) {
                    continue;
                }
                let t_index = self.grid.tiles.index(tx, ty);
                // Remove any *other* building already covering this tile.
                let existing = self.grid.tiles.geti(t_index).build;
                if let Some(other) = existing
                    && Some(other) != entity
                {
                    self.clear_building(other);
                }
                let tile = self.grid.tiles.geti_mut(t_index);
                tile.block = block;
                tile.build = entity;
            }
        }
    }

    /// Clears every tile belonging to a building entity and despawns it.
    pub fn clear_building(&mut self, entity: Entity) {
        for index in 0..self.grid.tiles.len() {
            if self.grid.tiles.geti(index).build == Some(entity) {
                let tile = self.grid.tiles.geti_mut(index);
                tile.block = BlockId::AIR;
                tile.build = None;
            }
        }
        self.hooks.on_removed(self.ecs, entity);
        self.ecs.despawn(entity);
    }

    /// Fills a rectangular region with a block (`World.fillTileBlocks` shape).
    pub fn fill_blocks(&mut self, x0: i16, y0: i16, x1: i16, y1: i16, block: BlockId) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.set_block(x, y, block, 0, 0);
            }
        }
    }

    /// Fills a rectangular region with a floor.
    pub fn fill_floors(&mut self, x0: i16, y0: i16, x1: i16, y1: i16, floor: BlockId) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.set_floor(x, y, floor);
            }
        }
    }

    /// Access to the underlying tiles (read-only).
    pub fn tiles(&self) -> &Tiles {
        &self.grid.tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    fn setup<'a>(
        world: &'a mut World,
        grid: &'a mut WorldGrid,
        content: &'a ContentRegistry,
        hooks: &'a NoopWorldHooks,
        render: &'a NoopRenderHooks,
        log: &'a mut WorldEventLog,
    ) -> WorldCtx<'a> {
        WorldCtx {
            grid,
            content,
            ecs: world,
            hooks,
            render,
            log,
        }
    }

    #[test]
    fn floor_and_overlay_ops_track_counters() {
        let content = test_registry();
        let mut world = World::new();
        let mut grid = WorldGrid::new(4, 4);
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        let mut log = WorldEventLog::default();
        let mut ctx = setup(&mut world, &mut grid, &content, &hooks, &render, &mut log);

        let before = ctx.grid.tile_changes;
        ctx.set_floor(1, 1, BlockId::STONE_WALL);
        // Floor changes count in `floor_changes` only (upstream listener rule).
        assert_eq!(ctx.grid.tile_changes, before);
        assert_eq!(ctx.grid.floor_changes, 2);
        // Repeat is a no-op.
        ctx.set_floor(1, 1, BlockId::STONE_WALL);
        assert_eq!(ctx.grid.floor_changes, 2);
        ctx.set_overlay(2, 2, BlockId::STONE_WALL);
        assert_eq!(ctx.grid.tile_changes, before);
        assert_eq!(ctx.log.floor_changes.len(), 1);
        assert_eq!(ctx.log.overlay_changes.len(), 1);
    }

    #[test]
    fn generating_suppresses_events() {
        let content = test_registry();
        let mut world = World::new();
        let mut grid = WorldGrid::new(4, 4);
        grid.generating = true;
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        let mut log = WorldEventLog::default();
        let mut ctx = setup(&mut world, &mut grid, &content, &hooks, &render, &mut log);
        ctx.set_floor(1, 1, BlockId::STONE_WALL);
        ctx.set_block(2, 2, BlockId::STONE_WALL, 0, 0);
        assert_eq!(ctx.grid.tile_changes, 1);
        assert!(ctx.log.tile_changes.is_empty());
        assert!(ctx.log.floor_changes.is_empty());
    }
}
