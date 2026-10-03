// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! In-memory [`EditorGrid`] for `mind-core` editor unit tests (plan 19 §7a).
//!
//! Keeps the per-tile fields the editor algorithm reads/writes without a full
//! `WorldGrid`; multiblock footprints are not expanded (op-log tests assert the
//! center tile only).

use crate::content::BlockId;

use super::EditorGrid;

/// A flat in-memory editor grid.
#[derive(Debug, Clone)]
pub struct TestGrid {
    width: i32,
    height: i32,
    floor: Vec<BlockId>,
    overlay: Vec<BlockId>,
    block: Vec<BlockId>,
    data: Vec<i8>,
    floor_data: Vec<i8>,
    overlay_data: Vec<i8>,
    extra: Vec<i32>,
    team: Vec<u8>,
    rotation: Vec<i32>,
    build: Vec<bool>,
    loading: bool,
}

impl TestGrid {
    /// An all-air grid; floors default to air (callers set stone explicitly).
    pub fn new(width: i32, height: i32) -> Self {
        let len = (width.max(0) as usize) * (height.max(0) as usize);
        Self {
            width,
            height,
            floor: vec![BlockId::AIR; len],
            overlay: vec![BlockId::AIR; len],
            block: vec![BlockId::AIR; len],
            data: vec![0; len],
            floor_data: vec![0; len],
            overlay_data: vec![0; len],
            extra: vec![0; len],
            team: vec![0; len],
            rotation: vec![0; len],
            build: vec![false; len],
            loading: false,
        }
    }

    fn index(&self, x: i32, y: i32) -> usize {
        x as usize + y as usize * self.width.max(0) as usize
    }
}

impl EditorGrid for TestGrid {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    fn is_loading(&self) -> bool {
        self.loading
    }

    fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    fn begin_map_load(&mut self) {}

    fn end_map_load(&mut self) {}

    fn resize(&mut self, width: i32, height: i32) {
        *self = TestGrid::new(width, height);
    }

    fn floor_id(&self, x: i32, y: i32) -> BlockId {
        self.floor[self.index(x, y)]
    }

    fn overlay_id(&self, x: i32, y: i32) -> BlockId {
        self.overlay[self.index(x, y)]
    }

    fn block_id(&self, x: i32, y: i32) -> BlockId {
        self.block[self.index(x, y)]
    }

    fn team_id(&self, x: i32, y: i32) -> u8 {
        self.team[self.index(x, y)]
    }

    fn rotation(&self, x: i32, y: i32) -> i32 {
        self.rotation[self.index(x, y)]
    }

    fn tile_data(&self, x: i32, y: i32) -> (i8, i8, i8, i32) {
        let i = self.index(x, y);
        (
            self.data[i],
            self.floor_data[i],
            self.overlay_data[i],
            self.extra[i],
        )
    }

    fn has_build(&self, x: i32, y: i32) -> bool {
        self.build[self.index(x, y)]
    }

    fn is_center(&self, _x: i32, _y: i32) -> bool {
        true
    }

    fn linked_tiles(&self, x: i32, y: i32) -> smallvec::SmallVec<[(i32, i32); 9]> {
        smallvec::smallvec![(x, y)]
    }

    fn set_floor(&mut self, x: i32, y: i32, floor: BlockId) {
        let i = self.index(x, y);
        self.floor[i] = floor;
    }

    fn set_overlay(&mut self, x: i32, y: i32, overlay: BlockId) {
        let i = self.index(x, y);
        self.overlay[i] = overlay;
    }

    fn set_block(&mut self, x: i32, y: i32, block: BlockId, team: u8, rot: i32) {
        let i = self.index(x, y);
        self.block[i] = block;
        self.build[i] = block != BlockId::AIR;
        if block != BlockId::AIR {
            self.team[i] = team;
            self.rotation[i] = rot;
        }
    }

    fn set_team(&mut self, x: i32, y: i32, team: u8) {
        let i = self.index(x, y);
        self.team[i] = team;
    }

    fn set_rotation(&mut self, x: i32, y: i32, rot: i32) {
        let i = self.index(x, y);
        self.rotation[i] = rot;
    }

    fn set_data(&mut self, x: i32, y: i32, data: i8, floor_data: i8, overlay_data: i8) {
        let i = self.index(x, y);
        self.data[i] = data;
        self.floor_data[i] = floor_data;
        self.overlay_data[i] = overlay_data;
    }

    fn set_extra_data(&mut self, x: i32, y: i32, extra: i32) {
        let i = self.index(x, y);
        self.extra[i] = extra;
    }

    fn update_static(&mut self, _x: i32, _y: i32) {}

    fn update_block(&mut self, _x: i32, _y: i32) {}

    fn clear_editor_darkness(&mut self) {
        self.data.iter_mut().for_each(|data| *data = 0);
    }

    fn recache_all(&mut self) {}

    fn resize_shift(
        &mut self,
        _content: &crate::content::ContentRegistry,
        width: i32,
        height: i32,
        shift_x: i32,
        shift_y: i32,
    ) {
        let old = self.clone();
        let (old_w, old_h) = (old.width, old.height);
        *self = TestGrid::new(width, height);
        for y in 0..old_h {
            for x in 0..old_w {
                let nx = x + shift_x;
                let ny = y + shift_y;
                if !self.in_bounds(nx, ny) {
                    continue;
                }
                let src = old.index(x, y);
                let dst = self.index(nx, ny);
                self.floor[dst] = old.floor[src];
                self.overlay[dst] = old.overlay[src];
                self.block[dst] = old.block[src];
                self.data[dst] = old.data[src];
                self.floor_data[dst] = old.floor_data[src];
                self.overlay_data[dst] = old.overlay_data[src];
                self.extra[dst] = old.extra[src];
                self.team[dst] = old.team[src];
                self.rotation[dst] = old.rotation[src];
                self.build[dst] = old.build[src];
            }
        }
    }
}
