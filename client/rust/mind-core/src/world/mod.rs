// SPDX-License-Identifier: GPL-3.0-only

//! Minimal P0 world grid.
//!
//! Ported from `core/src/mindustry/core/World.java` and `world/Tiles.java`
//! (`resize`/`fill` semantics from `ApplicationTests.createMap`). Plan 06 replaces
//! this with the full `Tiles`/`World`/`Edges` implementation; the entity handle in
//! `tiles` is the ECS building entity.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;

pub mod blocks;

/// Integer tile position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TilePos(pub i16, pub i16);

impl TilePos {
    /// Creates a tile position.
    pub const fn new(x: i16, y: i16) -> Self {
        Self(x, y)
    }

    /// X coordinate.
    pub const fn x(self) -> i16 {
        self.0
    }

    /// Y coordinate.
    pub const fn y(self) -> i16 {
        self.1
    }
}

/// Errors raised by [`WorldGrid`].
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    /// The requested tile lies outside the grid.
    #[error("tile ({x}, {y}) is outside a {width}x{height} world")]
    OutOfBounds {
        /// Requested x.
        x: i16,
        /// Requested y.
        y: i16,
        /// World width.
        width: i32,
        /// World height.
        height: i32,
    },
}

/// Row-major tile store.
///
/// Indexing is `x + y * width`. `blocks` holds the wall/block id per tile (default
/// `air`), `floors` the floor id, and `tiles` the optional ECS building entity.
#[derive(Debug, Clone)]
pub struct WorldGrid {
    /// Grid width in tiles.
    pub width: i32,
    /// Grid height in tiles.
    pub height: i32,
    /// Optional building entity per tile.
    pub tiles: Vec<Option<Entity>>,
    /// Floor block id per tile.
    pub floors: Vec<BlockId>,
    /// Wall/block id per tile (`air` when empty).
    pub blocks: Vec<BlockId>,
    /// Team id per tile.
    pub teams: Vec<u8>,
    /// Rotation per tile.
    pub rots: Vec<u8>,
}

impl WorldGrid {
    /// Creates an all-air grid of the given size (negative sizes clamp to 0).
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(0);
        let height = height.max(0);
        let len = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            tiles: vec![None; len],
            floors: vec![BlockId::AIR; len],
            blocks: vec![BlockId::AIR; len],
            teams: vec![0; len],
            rots: vec![0; len],
        }
    }

    /// Replaces the grid with a fresh all-air grid of the new size.
    /// Ported from `World.resize(int, int)` (the old tiles are discarded).
    pub fn resize(&mut self, width: i32, height: i32) {
        *self = Self::new(width, height);
    }

    /// Fills every tile with the given floor and wall/block, resetting entities,
    /// teams and rotations. Ported from `Tiles.fill()` (`ApplicationTests.createMap`).
    pub fn fill(&mut self, floor: BlockId, wall: BlockId) {
        self.floors.fill(floor);
        self.blocks.fill(wall);
        self.teams.fill(0);
        self.rots.fill(0);
        self.tiles.fill(None);
    }

    /// Number of tiles.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether the grid has no tiles.
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Whether the position lies inside the grid.
    pub fn in_bounds(&self, pos: TilePos) -> bool {
        pos.0 >= 0 && pos.1 >= 0 && (pos.0 as i32) < self.width && (pos.1 as i32) < self.height
    }

    /// Row-major index of a position.
    pub fn index(&self, pos: TilePos) -> Result<usize, WorldError> {
        if !self.in_bounds(pos) {
            return Err(self.out_of_bounds(pos));
        }
        Ok(pos.0 as usize + pos.1 as usize * self.width as usize)
    }

    /// Sets the block/team/rotation of a tile.
    pub fn set_block(
        &mut self,
        pos: TilePos,
        block: BlockId,
        team: u8,
        rot: u8,
    ) -> Result<(), WorldError> {
        let index = self.index(pos)?;
        self.blocks[index] = block;
        self.teams[index] = team;
        self.rots[index] = rot;
        Ok(())
    }

    /// Sets (or clears) the ECS entity stored on a tile.
    pub fn set_entity(&mut self, pos: TilePos, entity: Option<Entity>) -> Result<(), WorldError> {
        let index = self.index(pos)?;
        self.tiles[index] = entity;
        Ok(())
    }

    /// Clears a tile to `air` with no entity.
    pub fn clear(&mut self, pos: TilePos) -> Result<(), WorldError> {
        let index = self.index(pos)?;
        self.blocks[index] = BlockId::AIR;
        self.teams[index] = 0;
        self.rots[index] = 0;
        self.tiles[index] = None;
        Ok(())
    }

    /// Wall/block at a position (`None` when out of bounds).
    pub fn block_at(&self, pos: TilePos) -> Option<BlockId> {
        self.index(pos).ok().map(|index| self.blocks[index])
    }

    /// Floor at a position (`None` when out of bounds).
    pub fn floor_at(&self, pos: TilePos) -> Option<BlockId> {
        self.index(pos).ok().map(|index| self.floors[index])
    }

    /// Team id at a position (`None` when out of bounds).
    pub fn team_at(&self, pos: TilePos) -> Option<u8> {
        self.index(pos).ok().map(|index| self.teams[index])
    }

    /// Rotation at a position (`None` when out of bounds).
    pub fn rot_at(&self, pos: TilePos) -> Option<u8> {
        self.index(pos).ok().map(|index| self.rots[index])
    }

    /// Entity at a position (`None` when out of bounds or empty).
    pub fn entity_at(&self, pos: TilePos) -> Option<Entity> {
        self.index(pos).ok().and_then(|index| self.tiles[index])
    }

    /// Iterates positions row-major by `(y, x)` — the canonical checksum/dump order.
    pub fn iter_row_major(&self) -> impl Iterator<Item = (TilePos, usize)> + '_ {
        let width = self.width;
        let height = self.height;
        (0..height).flat_map(move |y| {
            (0..width).map(move |x| {
                let index = x as usize + y as usize * width as usize;
                (TilePos(x as i16, y as i16), index)
            })
        })
    }

    fn out_of_bounds(&self, pos: TilePos) -> WorldError {
        WorldError::OutOfBounds {
            x: pos.0,
            y: pos.1,
            width: self.width,
            height: self.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `tests/src/test/java/ApplicationTests.java` `createMap()`:
    /// `world.resize(8, 8)` followed by `tiles.fill()`.
    #[test]
    fn resize_and_fill() {
        let mut grid = WorldGrid::new(0, 0);
        grid.resize(8, 8);
        grid.fill(BlockId::STONE_WALL, BlockId::AIR);

        assert_eq!(grid.width, 8);
        assert_eq!(grid.height, 8);
        assert_eq!(grid.len(), 64);
        assert!(!grid.is_empty());

        for (pos, index) in grid.iter_row_major() {
            assert_eq!(grid.floors[index], BlockId::STONE_WALL);
            assert_eq!(grid.blocks[index], BlockId::AIR);
            assert_eq!(grid.entity_at(pos), None);
        }

        // In-bounds mutation and out-of-bounds rejection.
        let pos = TilePos::new(3, 4);
        grid.set_block(pos, BlockId::STONE_WALL, 0, 0).unwrap();
        assert_eq!(grid.block_at(pos), Some(BlockId::STONE_WALL));
        assert_eq!(grid.block_at(TilePos::new(-1, 0)), None);
        assert!(matches!(
            grid.set_block(TilePos::new(8, 0), BlockId::STONE_WALL, 0, 0),
            Err(WorldError::OutOfBounds { .. })
        ));
        grid.clear(pos).unwrap();
        assert_eq!(grid.block_at(pos), Some(BlockId::AIR));
    }
}
