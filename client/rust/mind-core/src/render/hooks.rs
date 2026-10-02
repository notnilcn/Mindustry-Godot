// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RenderHooks` implementation over the chunk grids (plan 16 §3.1).
//!
//! Plan 06 fires the hooks from tile mutation; this adapter dirties the floor/
//! building chunk grids and queues minimap pixels. The Godot renderer reads the
//! grids through [`RenderInvalidation::with_floor`]/[`RenderInvalidation::take_minimap_dirty`].

use std::sync::Mutex;

use crate::render::block_cache::{BuildingCacheGrid, recache_wall_tiles};
use crate::render::floor_cache::FloorChunkGrid;
use crate::world::hooks::RenderHooks;

/// Mutable render-invalidation state behind the `Send + Sync` hook adapter.
#[derive(Debug)]
struct InvalidInner {
    floor: FloorChunkGrid,
    blocks: BuildingCacheGrid,
    minimap_dirty: Vec<(i16, i16)>,
}

/// Shared render-invalidation hook adapter (plan 06's `RenderHooksRes`).
#[derive(Debug)]
pub struct RenderInvalidation {
    inner: Mutex<InvalidInner>,
}

impl Default for RenderInvalidation {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl RenderInvalidation {
    /// Creates the grids for a `width`×`height` tile world.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            inner: Mutex::new(InvalidInner {
                floor: FloorChunkGrid::new(width, height),
                blocks: BuildingCacheGrid::new(width, height),
                minimap_dirty: Vec::new(),
            }),
        }
    }

    /// Rebuilds the grids after a world resize.
    pub fn resize(&self, width: i32, height: i32) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.floor = FloorChunkGrid::new(width, height);
            inner.blocks = BuildingCacheGrid::new(width, height);
            inner.minimap_dirty.clear();
        }
    }

    /// Runs `f` against the floor chunk grid.
    pub fn with_floor<R>(&self, f: impl FnOnce(&FloorChunkGrid) -> R) -> Option<R> {
        self.inner.lock().ok().map(|inner| f(&inner.floor))
    }

    /// Runs `f` mutably against the floor chunk grid (baker side).
    pub fn with_floor_mut<R>(&self, f: impl FnOnce(&mut FloorChunkGrid) -> R) -> Option<R> {
        self.inner.lock().ok().map(|mut inner| f(&mut inner.floor))
    }

    /// Runs `f` against the building chunk grid.
    pub fn with_blocks<R>(&self, f: impl FnOnce(&BuildingCacheGrid) -> R) -> Option<R> {
        self.inner.lock().ok().map(|inner| f(&inner.blocks))
    }

    /// Drains the queued minimap pixels.
    pub fn take_minimap_dirty(&self) -> Vec<(i16, i16)> {
        self.inner
            .lock()
            .map(|mut inner| std::mem::take(&mut inner.minimap_dirty))
            .unwrap_or_default()
    }
}

impl RenderHooks for RenderInvalidation {
    fn recache_tile(&self, x: i16, y: i16) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.floor.recache_tile(x as i32, y as i32);
        }
    }

    fn recache_wall(&self, x: i16, y: i16) {
        if let Ok(mut inner) = self.inner.lock() {
            let width = inner.floor.width();
            let height = inner.floor.height();
            for (cx, cy) in recache_wall_tiles(x as i32, y as i32, width, height) {
                inner.floor.recache_tile(cx, cy);
                inner.minimap_dirty.push((cx as i16, cy as i16));
            }
        }
    }

    fn add_floor_index(&self, x: i16, y: i16) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.floor.recache_tile(x as i32, y as i32);
        }
    }

    fn remove_floor_index(&self, x: i16, y: i16) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.floor.recache_tile(x as i32, y as i32);
        }
    }

    fn invalidate_tile(&self, x: i16, y: i16) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.floor.recache_tile(x as i32, y as i32);
        }
    }

    fn minimap_update(&self, x: i16, y: i16) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.minimap_dirty.push((x, y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recache_hooks_dirty_only_own_chunk() {
        let hooks = RenderInvalidation::new(64, 64);
        hooks.with_floor_mut(|grid| grid.clear_dirty());
        hooks.recache_tile(30, 5);
        let dirty = hooks.with_floor(|grid| grid.dirty_chunks()).unwrap();
        assert_eq!(dirty, vec![(1, 0)]);
    }

    #[test]
    fn recache_wall_dirties_dark_radius_and_minimap() {
        let hooks = RenderInvalidation::new(64, 64);
        hooks.with_floor_mut(|grid| grid.clear_dirty());
        hooks.recache_wall(10, 10);
        let dirty = hooks.with_floor(|grid| grid.dirty_chunks()).unwrap();
        assert!(!dirty.is_empty());
        // 9x9 radius clipped to the 30x30 chunk => only chunk (0,0).
        assert_eq!(dirty, vec![(0, 0)]);
        assert_eq!(hooks.take_minimap_dirty().len(), 81);
        assert!(hooks.take_minimap_dirty().is_empty());
    }
}
