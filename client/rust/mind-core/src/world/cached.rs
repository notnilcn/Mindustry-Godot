// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Event-free preview tile views and the generation brush (`world/CachedTile.java`,
//! `world/TileGen.java`; plan 06 §3.7).
//!
//! Ported semantics: `MapIO.generatePreview` (plan 04 §3.9) and the editor read
//! tiles through an event-free view; no `Entity` is ever spawned. Pixels/textures
//! stay out of `mind-core` (plan 19).

use crate::content::BlockId;

/// A stubbed building view (no behavior) for previews (`CachedBuild`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CachedBuild {
    /// Building team.
    pub team: u8,
    /// Building rotation.
    pub rot: u8,
}

/// One event-free preview tile (`CachedTile`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CachedTile {
    /// Floor content id.
    pub floor: BlockId,
    /// Overlay content id.
    pub overlay: BlockId,
    /// Block content id.
    pub block: BlockId,
    /// Whether a building is present at this tile.
    pub has_build: bool,
    /// Building stub when `has_build` is true.
    pub build: CachedBuild,
    /// Floor save data.
    pub floor_data: u8,
    /// Overlay save data.
    pub overlay_data: u8,
}

/// Event-free preview grid (`CachedTile` array).
#[derive(Debug, Clone, Default)]
pub struct CachedTiles {
    /// Grid width.
    pub width: i32,
    /// Grid height.
    pub height: i32,
    tiles: Vec<CachedTile>,
}

impl CachedTiles {
    /// Creates a `width`×`height` grid of all-`air` preview tiles.
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(0);
        let height = height.max(0);
        Self {
            width,
            height,
            tiles: vec![CachedTile::default(); (width as usize) * (height as usize)],
        }
    }

    /// Row-major index of `(x, y)`.
    pub fn index(&self, x: i32, y: i32) -> usize {
        x as usize + y as usize * self.width as usize
    }

    /// Tile at `(x, y)`.
    pub fn get(&self, x: i32, y: i32) -> &CachedTile {
        &self.tiles[self.index(x, y)]
    }

    /// Mutable tile at `(x, y)`.
    pub fn get_mut(&mut self, x: i32, y: i32) -> &mut CachedTile {
        let index = self.index(x, y);
        &mut self.tiles[index]
    }

    /// All tiles, row-major.
    pub fn tiles(&self) -> &[CachedTile] {
        &self.tiles
    }

    /// Sets `(x, y)` (`CachedTile` create; never touches proximity/events).
    pub fn create(&mut self, x: i32, y: i32, floor: BlockId, overlay: BlockId, wall: BlockId) {
        let tile = self.get_mut(x, y);
        tile.floor = floor;
        tile.overlay = overlay;
        tile.block = wall;
        tile.has_build = false;
    }
}

/// The generation brush (`TileGen`): floor/block/overlay triples mutated by
/// `PlanetGenerator::gen_tile` and the editor (plan 19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGen {
    /// Floor to write.
    pub floor: BlockId,
    /// Wall/block to write.
    pub block: BlockId,
    /// Overlay to write.
    pub overlay: BlockId,
}

impl Default for TileGen {
    fn default() -> Self {
        Self {
            floor: BlockId::AIR,
            block: BlockId::AIR,
            overlay: BlockId::AIR,
        }
    }
}

impl TileGen {
    /// Resets to `air`/`air`/`air` (`TileGen.reset`).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_tiles_are_event_free_and_indexed() {
        let mut tiles = CachedTiles::new(4, 4);
        tiles.create(1, 2, BlockId::STONE_WALL, BlockId::AIR, BlockId::AIR);
        assert_eq!(tiles.get(1, 2).floor, BlockId::STONE_WALL);
        assert!(!tiles.get(1, 2).has_build);
        assert_eq!(tiles.tiles().len(), 16);
    }

    #[test]
    fn tile_gen_reset() {
        let mut brush = TileGen {
            floor: BlockId::STONE_WALL,
            block: BlockId::STONE_WALL,
            overlay: BlockId::STONE_WALL,
        };
        brush.reset();
        assert_eq!(brush, TileGen::default());
    }
}
