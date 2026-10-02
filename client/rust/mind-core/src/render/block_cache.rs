// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building-cache chunk state and darkness/shadow invalidation
//! (`BlockRenderer` half, plan 16 §3.6).
//!
//! The upstream global `SpriteCache` page pool becomes per-chunk mesh epochs
//! (plan 16 §3.6 deviation S16-2); this module owns the deterministic dirty/
//! epoch bookkeeping and the `recacheWall`/`updateShadow` tile-set math.

use smallvec::SmallVec;

/// `BlockRenderer.maxSpritesPerCacheTile`.
pub const MAX_SPRITES_PER_CACHE_TILE: usize = 6;
/// `BlockRenderer.chunkSize` (shares the floor chunk size).
pub const CHUNK_SIZE: i32 = 30;
/// `Vars.darkRadius`.
pub const DARK_RADIUS: i32 = 4;
/// `BlockRenderer.crackRegions`.
pub const CRACK_REGIONS: usize = 8;
/// `BlockRenderer.maxCrackSize`.
pub const MAX_CRACK_SIZE: i32 = 7;

/// `BuildingCacheLayer.amount`.
pub const BUILDING_CACHE_LAYERS: usize = 2;

/// One chunk's per-layer cache state.
#[derive(Debug, Clone, Default)]
struct CacheChunk {
    dirty: [bool; BUILDING_CACHE_LAYERS],
    epoch: [u64; BUILDING_CACHE_LAYERS],
}

/// Per-chunk building-cache grid (30×30).
#[derive(Debug, Clone, Default)]
pub struct BuildingCacheGrid {
    width: i32,
    height: i32,
    chunks_x: i32,
    chunks_y: i32,
    chunks: Vec<CacheChunk>,
    epoch: u64,
}

impl BuildingCacheGrid {
    /// Creates a grid covering a `width`×`height` tile world.
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(0);
        let height = height.max(0);
        let chunks_x = (width + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let chunks_y = (height + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let count = (chunks_x as usize) * (chunks_y as usize);
        let mut chunks = vec![CacheChunk::default(); count];
        for chunk in &mut chunks {
            // `CacheChunk` initializer fills `dirty` true.
            chunk.dirty = [true, true];
        }
        Self {
            width,
            height,
            chunks_x,
            chunks_y,
            chunks,
            epoch: 0,
        }
    }

    /// World width in tiles.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// World height in tiles.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Chunk columns.
    pub fn chunks_x(&self) -> i32 {
        self.chunks_x
    }

    /// Chunk rows.
    pub fn chunks_y(&self) -> i32 {
        self.chunks_y
    }

    /// `(x / 30, y / 30)`.
    pub fn chunk_of(x: i32, y: i32) -> (i32, i32) {
        (x.div_euclid(CHUNK_SIZE), y.div_euclid(CHUNK_SIZE))
    }

    fn index(&self, cx: i32, cy: i32) -> Option<usize> {
        if cx < 0 || cy < 0 || cx >= self.chunks_x || cy >= self.chunks_y {
            return None;
        }
        Some(cx as usize + cy as usize * self.chunks_x as usize)
    }

    /// `BlockRenderer.recacheBuilding`: dirties one layer of the tile's chunk.
    /// Out-of-bounds is a no-op.
    pub fn recache_building(&mut self, layer: usize, x: i32, y: i32) {
        if layer >= BUILDING_CACHE_LAYERS {
            return;
        }
        let (cx, cy) = Self::chunk_of(x, y);
        if let Some(i) = self.index(cx, cy) {
            self.chunks[i].dirty[layer] = true;
        }
    }

    /// Whether a chunk/layer needs a rebuild.
    pub fn is_dirty(&self, layer: usize, cx: i32, cy: i32) -> bool {
        self.index(cx, cy)
            .is_some_and(|i| self.chunks[i].dirty[layer])
    }

    /// Number of dirty chunk/layer pairs.
    pub fn dirty_count(&self) -> usize {
        self.chunks
            .iter()
            .map(|c| c.dirty.iter().filter(|d| **d).count())
            .sum()
    }

    /// Marks a chunk/layer baked and returns its new mesh epoch.
    pub fn mark_baked(&mut self, layer: usize, cx: i32, cy: i32) -> u64 {
        self.epoch += 1;
        let epoch = self.epoch;
        if let Some(i) = self.index(cx, cy) {
            self.chunks[i].dirty[layer] = false;
            self.chunks[i].epoch[layer] = epoch;
        }
        epoch
    }

    /// Current mesh epoch for a chunk/layer (`0` = never baked).
    pub fn mesh_epoch(&self, layer: usize, cx: i32, cy: i32) -> u64 {
        self.index(cx, cy)
            .map(|i| self.chunks[i].epoch[layer])
            .unwrap_or(0)
    }
}

/// `BlockRenderer.recacheWall` tile set: a `darkRadius` square around `(x, y)`,
/// clipped to the world. Returns `(x, y)` pairs in row-major order.
pub fn recache_wall_tiles(x: i32, y: i32, width: i32, height: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for cy in (y - DARK_RADIUS)..=(y + DARK_RADIUS) {
        for cx in (x - DARK_RADIUS)..=(x + DARK_RADIUS) {
            if cx >= 0 && cy >= 0 && cx < width && cy < height {
                out.push((cx, cy));
            }
        }
    }
    out
}

/// `BlockRenderer.updateShadow`: the `size`×`size` footprint anchored at
/// `(x + sizeOffset, y + sizeOffset)`.
pub fn update_shadow_tiles(
    x: i32,
    y: i32,
    size: i32,
    size_offset: i32,
) -> SmallVec<[(i32, i32); 16]> {
    let size = size.max(1);
    let mut out = SmallVec::new();
    for ty in 0..size {
        for tx in 0..size {
            out.push((x + tx + size_offset, y + ty + size_offset));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recache_wall_dirties_dark_radius() {
        let tiles = recache_wall_tiles(10, 10, 64, 64);
        // (2*4+1)^2 = 81 tiles for an interior wall.
        assert_eq!(tiles.len(), 81);
        assert!(tiles.contains(&(6, 6)));
        assert!(tiles.contains(&(14, 14)));
        assert!(!tiles.contains(&(15, 15)));
    }

    #[test]
    fn recache_wall_clips_at_border() {
        let tiles = recache_wall_tiles(0, 0, 64, 64);
        // x,y in 0..=4 each => 25.
        assert_eq!(tiles.len(), 25);
        assert!(tiles.iter().all(|(x, y)| *x >= 0 && *y >= 0));
    }

    #[test]
    fn update_shadow_covers_multiblock() {
        // A 3×3 block at (1,1) with sizeOffset -1 covers (0,0)..(2,2).
        let tiles = update_shadow_tiles(1, 1, 3, -1);
        assert_eq!(tiles.len(), 9);
        assert!(tiles.contains(&(0, 0)));
        assert!(tiles.contains(&(2, 2)));
        // A 1×1 block at (5,6) covers itself.
        assert_eq!(update_shadow_tiles(5, 6, 1, 0).as_slice(), &[(5, 6)]);
    }

    #[test]
    fn recache_building_marks_only_own_chunk() {
        let mut grid = BuildingCacheGrid::new(64, 64);
        // Clear was implicit true; bake chunk (0,0), layer 1 then dirty it.
        grid.mark_baked(1, 0, 0);
        assert!(!grid.is_dirty(1, 0, 0));
        grid.recache_building(1, 3, 3);
        assert!(grid.is_dirty(1, 0, 0));
        // Layer 0 untouched.
        assert!(grid.is_dirty(0, 0, 0));
        // Out of bounds is a no-op.
        let before = grid.dirty_count();
        grid.recache_building(0, 9999, 9999);
        assert_eq!(grid.dirty_count(), before);
    }
}
