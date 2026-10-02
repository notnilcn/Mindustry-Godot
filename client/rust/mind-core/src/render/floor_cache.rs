// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Floor-chunk dirty/used/epoch bookkeeping (`FloorRenderer` half, plan 16 §3.5).
//!
//! The Godot `ArrayMesh` bake lives in `mind-gdext::render::floor`; this module
//! owns the deterministic chunk grid so the headless harness can assert dirty
//! minimality and cold-rebuild equality without a GPU.

use smallvec::SmallVec;

use crate::render::layer::CacheLayerId;

/// `FloorRenderer.chunksize`.
pub const CHUNK_SIZE: i32 = 30;
/// `chunksize * tilesize`.
pub const CHUNK_UNITS: f32 = CHUNK_SIZE as f32 * 8.0;
/// `FloorRenderer.packPad`.
pub const PACK_PAD: f32 = 64.0;
/// Chunk AABB grow (`[cx*240-4 .. (cx+1)*240+4]`).
pub const CHUNK_AABB_PAD: f32 = 4.0;

/// Per-chunk floor cache state.
#[derive(Debug, Clone, Default)]
struct Chunk {
    dirty: bool,
    used: SmallVec<[CacheLayerId; 9]>,
    /// `0` = never baked; otherwise the bake epoch.
    mesh_epoch: u64,
}

/// The floor chunk grid (`FloorRenderer.recacheTile` targets).
#[derive(Debug, Clone, Default)]
pub struct FloorChunkGrid {
    width: i32,
    height: i32,
    chunks_x: i32,
    chunks_y: i32,
    chunks: Vec<Chunk>,
    /// Global bake counter (`FloorRenderer.epoch`).
    epoch: u64,
}

impl FloorChunkGrid {
    /// Creates a grid covering a `width`×`height` tile world.
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(0);
        let height = height.max(0);
        let chunks_x = (width + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let chunks_y = (height + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let count = (chunks_x as usize) * (chunks_y as usize);
        let mut chunks = vec![Chunk::default(); count];
        for chunk in &mut chunks {
            chunk.dirty = true;
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

    /// Total chunk count.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// The chunk containing tile `(x, y)` (`x / 30`, `y / 30`).
    pub fn chunk_of(x: i32, y: i32) -> (i32, i32) {
        (x.div_euclid(CHUNK_SIZE), y.div_euclid(CHUNK_SIZE))
    }

    fn index(&self, cx: i32, cy: i32) -> Option<usize> {
        if cx < 0 || cy < 0 || cx >= self.chunks_x || cy >= self.chunks_y {
            return None;
        }
        Some(cx as usize + cy as usize * self.chunks_x as usize)
    }

    fn chunk(&self, cx: i32, cy: i32) -> Option<&Chunk> {
        self.index(cx, cy).map(|i| &self.chunks[i])
    }

    fn chunk_mut(&mut self, cx: i32, cy: i32) -> Option<&mut Chunk> {
        self.index(cx, cy).map(|i| &mut self.chunks[i])
    }

    /// `FloorRenderer.recacheTile`: dirties the chunk containing the tile.
    /// Out-of-bounds tiles are a no-op.
    pub fn recache_tile(&mut self, x: i32, y: i32) {
        let (cx, cy) = Self::chunk_of(x, y);
        if let Some(chunk) = self.chunk_mut(cx, cy) {
            chunk.dirty = true;
        }
    }

    /// Dirties a chunk directly.
    pub fn mark_dirty(&mut self, cx: i32, cy: i32) {
        if let Some(chunk) = self.chunk_mut(cx, cy) {
            chunk.dirty = true;
        }
    }

    /// Whether a chunk needs a rebuild.
    pub fn is_dirty(&self, cx: i32, cy: i32) -> bool {
        self.chunk(cx, cy).is_some_and(|c| c.dirty)
    }

    /// Dirty chunks in row-major order.
    pub fn dirty_chunks(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        for cy in 0..self.chunks_y {
            for cx in 0..self.chunks_x {
                if self.is_dirty(cx, cy) {
                    out.push((cx, cy));
                }
            }
        }
        out
    }

    /// Number of dirty chunks.
    pub fn dirty_count(&self) -> usize {
        self.chunks.iter().filter(|c| c.dirty).count()
    }

    /// Clears every dirty flag (after a full rebuild).
    pub fn clear_dirty(&mut self) {
        for chunk in &mut self.chunks {
            chunk.dirty = false;
        }
    }

    /// Marks the chunk clean and records a new mesh epoch, returning it.
    pub fn mark_baked(&mut self, cx: i32, cy: i32) -> u64 {
        self.epoch += 1;
        let epoch = self.epoch;
        if let Some(chunk) = self.chunk_mut(cx, cy) {
            chunk.dirty = false;
            chunk.mesh_epoch = epoch;
        }
        epoch
    }

    /// Current mesh epoch of a chunk (`0` = never baked).
    pub fn mesh_epoch(&self, cx: i32, cy: i32) -> u64 {
        self.chunk(cx, cy).map(|c| c.mesh_epoch).unwrap_or(0)
    }

    /// Global bake epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The used cache layers recorded for a chunk.
    pub fn used_layers(&self, cx: i32, cy: i32) -> &[CacheLayerId] {
        self.chunk(cx, cy).map(|c| c.used.as_slice()).unwrap_or(&[])
    }

    /// Replaces the used cache layers for a chunk (computed by the scan).
    pub fn set_used_layers(&mut self, cx: i32, cy: i32, layers: SmallVec<[CacheLayerId; 9]>) {
        if let Some(chunk) = self.chunk_mut(cx, cy) {
            chunk.used = layers;
        }
    }

    /// Chunk AABB in world pixels (`[x, y, w, h]`), with the `4` px grow.
    pub fn chunk_aabb(cx: i32, cy: i32) -> [f32; 4] {
        let x = cx as f32 * CHUNK_UNITS - CHUNK_AABB_PAD;
        let y = cy as f32 * CHUNK_UNITS - CHUNK_AABB_PAD;
        [
            x,
            y,
            CHUNK_UNITS + CHUNK_AABB_PAD * 2.0,
            CHUNK_UNITS + CHUNK_AABB_PAD * 2.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recache_marks_only_own_chunk() {
        let mut grid = FloorChunkGrid::new(64, 64);
        grid.clear_dirty();
        grid.recache_tile(30, 5);
        // x=30 => chunk 1; y=5 => chunk 0.
        assert!(grid.is_dirty(1, 0));
        assert_eq!(grid.dirty_count(), 1);
        assert!(!grid.is_dirty(0, 0));
        assert!(!grid.is_dirty(2, 0));
    }

    #[test]
    fn recache_out_of_bounds_is_noop() {
        let mut grid = FloorChunkGrid::new(32, 32);
        grid.clear_dirty();
        grid.recache_tile(-1, 0);
        grid.recache_tile(0, -5);
        grid.recache_tile(9999, 9999);
        assert_eq!(grid.dirty_count(), 0);
    }

    #[test]
    fn chunk_counts_and_bake_epochs() {
        let mut grid = FloorChunkGrid::new(64, 64);
        assert_eq!(grid.chunks_x(), 3);
        assert_eq!(grid.chunks_y(), 3);
        assert_eq!(grid.chunk_count(), 9);
        assert_eq!(grid.dirty_count(), 9);
        let epoch = grid.mark_baked(0, 0);
        assert_eq!(epoch, 1);
        assert_eq!(grid.mesh_epoch(0, 0), 1);
        assert_eq!(grid.mesh_epoch(1, 0), 0);
        assert_eq!(FloorChunkGrid::chunk_aabb(0, 0), [-4.0, -4.0, 248.0, 248.0]);
        assert_eq!(
            FloorChunkGrid::chunk_aabb(1, 1),
            [236.0, 236.0, 248.0, 248.0]
        );
    }
}
