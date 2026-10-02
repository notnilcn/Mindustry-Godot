// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The tile grid (`world/Tiles.java`, plan 06 §3.3).
//!
//! Flat `Vec<Tile>` indexed `x + y*width`, puddle/fire entity slots and
//! generation scratch buffers. Iteration order is a determinism contract:
//! `each()` is x-outer/y-inner (upstream), `iter()`/`each_tile()` is row-major.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;

use super::TilePos;
use super::tile::Tile;

/// The tile container (`Tiles`).
#[derive(Debug, Clone)]
pub struct Tiles {
    /// Grid width in tiles.
    pub width: i32,
    /// Grid height in tiles.
    pub height: i32,
    array: Vec<Tile>,
    puddles: Vec<Option<Entity>>,
    fires: Vec<Option<Entity>>,
    tmp_floor_state: Vec<i64>,
    tmp_block_state: Vec<i64>,
}

impl Default for Tiles {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl Tiles {
    /// An empty grid of the given size (negative sizes clamp to 0).
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(0);
        let height = height.max(0);
        let len = (width as usize).saturating_mul(height as usize);
        let mut array = Vec::with_capacity(len);
        for y in 0..height {
            for x in 0..width {
                array.push(Tile::new(x as i16, y as i16));
            }
        }
        Self {
            width,
            height,
            array,
            puddles: vec![None; len],
            fires: vec![None; len],
            tmp_floor_state: Vec::new(),
            tmp_block_state: Vec::new(),
        }
    }

    /// Number of tiles.
    pub fn len(&self) -> usize {
        self.array.len()
    }

    /// Whether the grid is empty.
    pub fn is_empty(&self) -> bool {
        self.array.is_empty()
    }

    /// Whether `(x, y)` lies inside the grid.
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    /// Row-major index (`x + y*width`).
    pub fn index(&self, x: i32, y: i32) -> usize {
        debug_assert!(self.in_bounds(x, y), "tile index out of bounds: ({x}, {y})");
        x as usize + y as usize * self.width as usize
    }

    /// Tile at `(x, y)`; panics out of bounds (upstream `Tiles.get`).
    pub fn get(&self, x: i32, y: i32) -> &Tile {
        &self.array[self.index(x, y)]
    }

    /// Mutable tile at `(x, y)`.
    pub fn get_mut(&mut self, x: i32, y: i32) -> &mut Tile {
        let idx = self.index(x, y);
        &mut self.array[idx]
    }

    /// Tile at `(x, y)` or `None` out of bounds (`Tiles.getn`).
    pub fn getn(&self, x: i32, y: i32) -> Option<&Tile> {
        self.in_bounds(x, y).then(|| self.get(x, y))
    }

    /// Tile at `(x, y)` clamped to the grid (`Tiles.getc`).
    pub fn getc(&self, x: i32, y: i32) -> &Tile {
        let x = x.clamp(0, (self.width - 1).max(0));
        let y = y.clamp(0, (self.height - 1).max(0));
        self.get(x, y)
    }

    /// Tile by flat index (`Tiles.geti`).
    pub fn geti(&self, index: usize) -> &Tile {
        &self.array[index]
    }

    /// Mutable tile by flat index.
    pub fn geti_mut(&mut self, index: usize) -> &mut Tile {
        &mut self.array[index]
    }

    /// Tile at a packed position (`Tiles.getp`).
    pub fn getp(&self, pos: TilePos) -> &Tile {
        self.get(pos.x() as i32, pos.y() as i32)
    }

    /// Whether a packed position is inside (`Tiles.in`).
    pub fn contains(&self, pos: TilePos) -> bool {
        self.in_bounds(pos.x() as i32, pos.y() as i32)
    }

    /// Flat array index of a tile (`Tiles.array`-equivalent).
    pub fn array_of(&self, tile: &Tile) -> usize {
        tile.x as usize + tile.y as usize * self.width as usize
    }

    /// Packs `(x, y)` into the Arc `Point2` form (`Tiles.pos`).
    pub fn pos_of(x: i32, y: i32) -> i32 {
        (x as u16 as i32) | ((y as u16 as i32) << 16)
    }

    /// Iterates x-outer, y-inner (`Tiles.each`).
    pub fn each(&self) -> impl Iterator<Item = &Tile> + '_ {
        let width = self.width;
        let height = self.height;
        (0..width).flat_map(move |x| (0..height).map(move |y| self.get(x, y)))
    }

    /// Iterates row-major `(y, x)` (`Tiles.iter`).
    pub fn iter(&self) -> impl Iterator<Item = &Tile> + '_ {
        self.array.iter()
    }

    /// The whole flat array.
    pub fn array(&self) -> &[Tile] {
        &self.array
    }

    /// Mutable whole flat array.
    pub fn array_mut(&mut self) -> &mut [Tile] {
        &mut self.array
    }

    /// Fills every tile with the given floor and wall, clearing overlays/builds.
    /// Ported from `Tiles.fill()`.
    pub fn fill(&mut self, floor: BlockId, wall: BlockId) {
        for tile in &mut self.array {
            tile.floor = floor;
            tile.block = wall;
            tile.overlay = BlockId::AIR;
            tile.build = None;
            tile.data = 0;
            tile.floor_data = 0;
            tile.overlay_data = 0;
            tile.extra_data = 0;
            tile.changing = false;
        }
        self.puddles.fill(None);
        self.fires.fill(None);
    }

    /// Resets all tiles to all-`air`.
    pub fn clear(&mut self) {
        self.fill(BlockId::AIR, BlockId::AIR);
    }

    /// Puddle entity at a flat index (`Tiles.puddles`).
    pub fn puddle_at(&self, index: usize) -> Option<Entity> {
        self.puddles.get(index).copied().flatten()
    }

    /// Sets the puddle entity at a flat index.
    pub fn set_puddle(&mut self, index: usize, entity: Option<Entity>) {
        if let Some(slot) = self.puddles.get_mut(index) {
            *slot = entity;
        }
    }

    /// Fire entity at a flat index (`Tiles.fires`).
    pub fn fire_at(&self, index: usize) -> Option<Entity> {
        self.fires.get(index).copied().flatten()
    }

    /// Sets the fire entity at a flat index.
    pub fn set_fire(&mut self, index: usize, entity: Option<Entity>) {
        if let Some(slot) = self.fires.get_mut(index) {
            *slot = entity;
        }
    }

    /// Lazily sizes the floor scratch to the grid (`Tiles.tmpFloorState`).
    pub fn tmp_floor_state(&mut self) -> &mut [i64] {
        if self.tmp_floor_state.len() != self.array.len() {
            self.tmp_floor_state = vec![0; self.array.len()];
        }
        &mut self.tmp_floor_state
    }

    /// Lazily sizes the block scratch to the grid (`Tiles.tmpBlockState`).
    pub fn tmp_block_state(&mut self) -> &mut [i64] {
        if self.tmp_block_state.len() != self.array.len() {
            self.tmp_block_state = vec![0; self.array.len()];
        }
        &mut self.tmp_block_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iteration_order_is_row_major() {
        // Ported from `ApplicationTests.arrayIterators` (world half).
        let tiles = Tiles::new(3, 2);
        let iter_order: Vec<(i16, i16)> = tiles.iter().map(|t| (t.x, t.y)).collect();
        assert_eq!(
            iter_order,
            vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]
        );
        // `each` is x-outer, y-inner (upstream order).
        let each_order: Vec<(i16, i16)> = tiles.each().map(|t| (t.x, t.y)).collect();
        assert_eq!(
            each_order,
            vec![(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]
        );
    }

    #[test]
    fn getn_getc_bounds() {
        let tiles = Tiles::new(4, 4);
        assert!(tiles.getn(-1, 0).is_none());
        assert!(tiles.getn(4, 0).is_none());
        assert_eq!(tiles.getn(1, 2).unwrap().x, 1);
        assert_eq!(tiles.getc(-5, 9).pos(), TilePos::new(0, 3));
    }

    #[test]
    fn fill_resets_grid() {
        let mut tiles = Tiles::new(2, 2);
        tiles.get_mut(1, 1).block = BlockId::STONE_WALL;
        tiles.get_mut(0, 0).overlay = BlockId::STONE_WALL;
        tiles.fill(BlockId::STONE_WALL, BlockId::AIR);
        for tile in tiles.iter() {
            assert_eq!(tile.floor, BlockId::STONE_WALL);
            assert_eq!(tile.block, BlockId::AIR);
            assert_eq!(tile.overlay, BlockId::AIR);
            assert!(tile.build.is_none());
        }
    }
}
