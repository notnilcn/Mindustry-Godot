// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Visible-set scan and `processBlocks` early-out (plan 16 §3.5/§3.6/§3.11).
//!
//! Godot-free half of `BlockRenderer.processBlocks` and
//! `FloorRenderer.drawFloor`'s preliminary layer pass. M0 provides the camera
//! model and the early-out; M1 adds the floor-layer pass; M2 fills the ordered
//! visible lists.

use smallvec::SmallVec;

use crate::content::{BlockId, ContentRegistry};
use crate::render::floor_cache::{CHUNK_UNITS, FloorChunkGrid};
use crate::render::layer::CacheLayerId;
use crate::world::WorldGrid;
use crate::world::tile::is_static_kind;

/// The camera view used for culling (world pixels + team).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    /// Camera center x in world pixels.
    pub x: f32,
    /// Camera center y in world pixels.
    pub y: f32,
    /// Visible width in world pixels (`screen / camerascale`).
    pub w: f32,
    /// Visible height in world pixels.
    pub h: f32,
    /// Camera scale (`Renderer.camerascale`).
    pub zoom: f32,
    /// Viewing team.
    pub team: u8,
}

impl Default for CameraView {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 320.0,
            h: 180.0,
            zoom: 1.0,
            team: 0,
        }
    }
}

impl CameraView {
    /// The view bounds `[x, y, w, h]` in world pixels.
    pub fn bounds(&self) -> [f32; 4] {
        [self.x - self.w / 2.0, self.y - self.h / 2.0, self.w, self.h]
    }

    /// Whether an AABB `[x, y, w, h]` overlaps the view bounds.
    pub fn overlaps(&self, aabb: [f32; 4]) -> bool {
        let [vx, vy, vw, vh] = self.bounds();
        let [ax, ay, aw, ah] = aabb;
        ax < vx + vw && ax + aw > vx && ay < vy + vh && ay + ah > vy
    }
}

/// Chunk coordinate sets touched by a scan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChunkSet {
    /// Floor chunks (row-major).
    pub floor: Vec<(i32, i32)>,
    /// Building-cache chunks (row-major).
    pub blocks: Vec<(i32, i32)>,
}

/// Output of [`process_blocks`].
#[derive(Clone, Debug, Default)]
pub struct ProcessBlocksOut {
    /// Dynamic floor indices to re-run `updateRender`.
    pub update_floors: Vec<usize>,
    /// Light index tiles (`blockLight`/floor/overlay emit-light).
    pub lightview: Vec<usize>,
    /// Dynamic block tile indices.
    pub tileview: Vec<usize>,
    /// Cached non-dynamic tile indices.
    pub tile_extra_cached_view: Vec<usize>,
    /// Consumer-building tiles for status bars.
    pub tile_with_consumer_view: Vec<usize>,
    /// Chunks to draw this frame.
    pub chunks_to_draw: ChunkSet,
    /// Whether the early-out reused the previous frame's lists.
    pub early_out: bool,
}

/// Scan errors.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// The world is empty.
    #[error("scan on an empty world")]
    EmptyWorld,
}

/// `processBlocks` early-out state (camera tile + range + team).
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessBlocksState {
    /// Last camera tile x (`-99` = invalid).
    pub last_cam_x: i32,
    /// Last camera tile y.
    pub last_cam_y: i32,
    /// Last range x.
    pub last_range_x: i32,
    /// Last range y.
    pub last_range_y: i32,
    /// Last team.
    pub last_team: u8,
    /// Whether the previous lists are valid.
    pub valid: bool,
}

impl ProcessBlocksState {
    /// `BlockRenderer.invalidateTile` / camera-change reset.
    pub fn invalidate(&mut self) {
        self.last_cam_x = -99;
        self.last_cam_y = -99;
        self.valid = false;
    }
}

/// The `(camera tile, range, team)` early-out key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraKey {
    /// Camera tile x.
    pub cam_x: i32,
    /// Camera tile y.
    pub cam_y: i32,
    /// Half-range x in tiles.
    pub range_x: i32,
    /// Half-range y in tiles.
    pub range_y: i32,
    /// Team.
    pub team: u8,
}

impl CameraKey {
    /// Computes the key from a camera view (`avgx = camera.x / tilesize`, etc.).
    pub fn from_view(view: &CameraView, tilesize: f32) -> Self {
        let avg_x = (view.x / tilesize) as i32;
        let avg_y = (view.y / tilesize) as i32;
        let range_x = (view.w / tilesize / 2.0) as i32 + 3;
        let range_y = (view.h / tilesize / 2.0) as i32 + 3;
        Self {
            cam_x: avg_x,
            cam_y: avg_y,
            range_x,
            range_y,
            team: view.team,
        }
    }
}

/// Computes whether the visible lists must be rebuilt. When `false`, the caller
/// reuses the previous [`ProcessBlocksOut`] unchanged (upstream early-out).
pub fn process_blocks(
    state: &mut ProcessBlocksState,
    view: &CameraView,
    tilesize: f32,
) -> (bool, ProcessBlocksOut) {
    let key = CameraKey::from_view(view, tilesize);
    let changed = !state.valid
        || state.last_cam_x != key.cam_x
        || state.last_cam_y != key.cam_y
        || state.last_range_x != key.range_x
        || state.last_range_y != key.range_y
        || state.last_team != key.team;
    if !changed {
        return (
            false,
            ProcessBlocksOut {
                early_out: true,
                ..ProcessBlocksOut::default()
            },
        );
    }
    state.last_cam_x = key.cam_x;
    state.last_cam_y = key.cam_y;
    state.last_range_x = key.range_x;
    state.last_range_y = key.range_y;
    state.last_team = key.team;
    state.valid = true;
    // M2 fills the ordered visible lists here. M0 has no content index yet.
    (true, ProcessBlocksOut::default())
}

/// A dynamic (non-static) block tile in view (`processBlocks` `tileview`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleBlock {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Block content id.
    pub block: BlockId,
}

/// The dynamic block tiles in view, in row-major order (`processBlocks`
/// `tileview`). Static walls bake into the floor pass and are excluded.
///
/// This is the deterministic visible set the headless oracle asserts; the
/// in-engine `BlockRenderer` consumes it to emit dynamic sprites at
/// `Layer::block` every frame (plan 16 §3.6).
pub fn visible_blocks(
    world: &WorldGrid,
    content: &ContentRegistry,
    view: &CameraView,
) -> Vec<VisibleBlock> {
    let size = crate::config::TILESIZE as f32;
    let grow = size * 2.0;
    let [bx, by, bw, bh] = view.bounds();
    let min_x = ((bx - grow) / size).floor().max(0.0) as i32;
    let min_y = ((by - grow) / size).floor().max(0.0) as i32;
    let max_x = (((bx + bw + grow) / size).ceil() as i32).min(world.width() - 1);
    let max_y = (((by + bh + grow) / size).ceil() as i32).min(world.height() - 1);

    let mut out = Vec::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let tile = world.tile(x, y);
            if tile.block == BlockId::AIR {
                continue;
            }
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            if is_static_kind(def.kind) {
                continue;
            }
            out.push(VisibleBlock {
                x,
                y,
                block: tile.block,
            });
        }
    }
    out
}

/// Cached (`drawCached`) center tiles in view (`processBlocks`
/// `tileExtraCachedView`). `is_cached` is the plan-02 `draw_cached` predicate
/// ([`crate::render::draw_meta::BlockDrawMeta::draw_cached`] once it lands);
/// multiblocks are indexed once at their center (`Tile.isCenter`).
pub fn visible_cached_blocks(
    world: &WorldGrid,
    content: &ContentRegistry,
    view: &CameraView,
    is_cached: impl Fn(&crate::content::BlockDef) -> bool,
) -> Vec<VisibleBlock> {
    let size = crate::config::TILESIZE as f32;
    let grow = size * 2.0;
    let [bx, by, bw, bh] = view.bounds();
    let min_x = ((bx - grow) / size).floor().max(0.0) as i32;
    let min_y = ((by - grow) / size).floor().max(0.0) as i32;
    let max_x = (((bx + bw + grow) / size).ceil() as i32).min(world.width() - 1);
    let max_y = (((by + bh + grow) / size).ceil() as i32).min(world.height() - 1);

    let mut out = Vec::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let tile = world.tile(x, y);
            if tile.block == BlockId::AIR {
                continue;
            }
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            if !is_cached(def) || !tile.is_center_with(tile.pos()) {
                continue;
            }
            out.push(VisibleBlock {
                x,
                y,
                block: tile.block,
            });
        }
    }
    out
}

/// `FloorRenderer.drawFloor` preliminary pass: the cache layers present in/// view, sorted by id, skipping `walls` (walls draw with the block pass).
pub fn floor_layers_in_view(grid: &FloorChunkGrid, view: &CameraView) -> Vec<CacheLayerId> {
    let min_x = ((view.x - view.w / 2.0 - 4.0) / CHUNK_UNITS)
        .floor()
        .max(0.0) as i32;
    let min_y = ((view.y - view.h / 2.0 - 4.0) / CHUNK_UNITS)
        .floor()
        .max(0.0) as i32;
    let max_x = (((view.x + view.w / 2.0 + 4.0) / CHUNK_UNITS).ceil() as i32)
        .min(grid.chunks_x().saturating_sub(1));
    let max_y = (((view.y + view.h / 2.0 + 4.0) / CHUNK_UNITS).ceil() as i32)
        .min(grid.chunks_y().saturating_sub(1));

    let mut set: SmallVec<[CacheLayerId; 9]> = SmallVec::new();
    for cy in min_y..=max_y {
        for cx in min_x..=max_x {
            if cx < 0 || cy < 0 || cx >= grid.chunks_x() || cy >= grid.chunks_y() {
                continue;
            }
            if !view.overlaps(FloorChunkGrid::chunk_aabb(cx, cy)) {
                continue;
            }
            for layer in grid.used_layers(cx, cy) {
                if *layer == CacheLayerId::Walls {
                    continue;
                }
                if !set.contains(layer) {
                    set.push(*layer);
                }
            }
        }
    }
    set.sort_by_key(|l| l.id());
    set.into_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    #[test]
    fn process_blocks_early_out() {
        let mut state = ProcessBlocksState::default();
        let view = CameraView {
            x: 128.0,
            y: 128.0,
            w: 320.0,
            h: 180.0,
            zoom: 1.0,
            team: 0,
        };
        let (changed, out) = process_blocks(&mut state, &view, 8.0);
        assert!(changed);
        assert!(!out.early_out);
        // Same camera => early-out.
        let (changed2, out2) = process_blocks(&mut state, &view, 8.0);
        assert!(!changed2);
        assert!(out2.early_out);
        // Team change invalidates.
        let mut other = view;
        other.team = 1;
        let (changed3, _) = process_blocks(&mut state, &other, 8.0);
        assert!(changed3);
        // invalidate() forces a rebuild.
        state.invalidate();
        let (changed4, _) = process_blocks(&mut state, &view, 8.0);
        assert!(changed4);
    }

    #[test]
    fn visible_blocks_excludes_static_walls_and_air() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::world::TilePos;

        let mut content =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        content.init().expect("init");
        content.post_init().expect("post_init");
        content.load().expect("load");

        let stone = content.block_id("stone").expect("stone");
        let wall = content.block_id("stone-wall").expect("stone-wall");
        let router = content.block_id("router").expect("router");
        let mut world = WorldGrid::new(16, 16);
        world.fill(stone, BlockId::AIR);
        world
            .set_block(TilePos::new(4, 4), wall, 0, 0)
            .expect("wall");
        world
            .set_block(TilePos::new(5, 5), router, 0, 0)
            .expect("router");

        let view = CameraView {
            x: 64.0,
            y: 64.0,
            w: 320.0,
            h: 180.0,
            zoom: 1.0,
            team: 0,
        };
        let blocks = visible_blocks(&world, &content, &view);
        assert!(
            blocks
                .iter()
                .any(|b| b.block == router && b.x == 5 && b.y == 5)
        );
        assert!(!blocks.iter().any(|b| b.block == wall));
        assert!(!blocks.iter().any(|b| b.block == BlockId::AIR));
    }

    #[test]
    fn visible_cached_blocks_uses_predicate() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::world::TilePos;

        let mut content =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        content.init().expect("init");
        content.post_init().expect("post_init");
        content.load().expect("load");

        let stone = content.block_id("stone").expect("stone");
        let router = content.block_id("router").expect("router");
        let mut world = WorldGrid::new(16, 16);
        world.fill(stone, BlockId::AIR);
        world
            .set_block(TilePos::new(5, 5), router, 0, 0)
            .expect("router");

        let view = CameraView {
            x: 64.0,
            y: 64.0,
            w: 320.0,
            h: 180.0,
            zoom: 1.0,
            team: 0,
        };
        // A predicate that matches routers selects the tile.
        let cached = visible_cached_blocks(&world, &content, &view, |def| def.name == "router");
        assert_eq!(cached.len(), 1);
        assert_eq!((cached[0].x, cached[0].y), (5, 5));
        // A predicate that matches nothing yields an empty set.
        let none = visible_cached_blocks(&world, &content, &view, |_| false);
        assert!(none.is_empty());
    }

    #[test]
    fn floor_layers_in_view_sorted_skips_walls() {
        let mut grid = FloorChunkGrid::new(64, 64);
        grid.set_used_layers(
            0,
            0,
            smallvec![
                CacheLayerId::Walls,
                CacheLayerId::Normal,
                CacheLayerId::Water
            ],
        );
        let view = CameraView {
            x: 60.0,
            y: 60.0,
            w: 100.0,
            h: 100.0,
            zoom: 1.0,
            team: 0,
        };
        let layers = floor_layers_in_view(&grid, &view);
        assert_eq!(layers, vec![CacheLayerId::Water, CacheLayerId::Normal]);
    }
}
