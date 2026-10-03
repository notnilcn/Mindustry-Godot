// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Editor world-load context and the tile-op recorder seam
//! (`editor/MapEditor.java` `Context`, `editor/EditorTile.java`, plan 19 §3.4).
//!
//! `EditorContext` implements plan 04's `WorldContext` for map/image loads into
//! the editor. `TileOpSink` is the optional recorder hook on the world (deviation
//! §2.3.1); it is `None` in normal play and installed while editing. M0 performs
//! draw recording explicitly on [`super::MapEditor`]; the sink is the M1 seam for
//! mutations that originate outside the draw path (07/12 hooks).

use crate::content::{BlockId, ContentRegistry};
use crate::ecs::TeamId;
use crate::io::IoError;
use crate::io::save::WorldContext;
use crate::io::wire::WireReader;
use crate::world::{Context, WorldGrid};

/// Optional tile-op recorder installed on the live world while editing.
///
/// Each method receives the **previous** value around a mutation; center
/// fan-out for multiblocks is the sink's responsibility (plan 19 §3.4).
pub trait TileOpSink {
    /// A floor changed (`EditorTile.setFloor`).
    fn floor_changed(&mut self, x: i32, y: i32, prev: BlockId);
    /// An overlay changed (`EditorTile.setOverlay`).
    fn overlay_changed(&mut self, x: i32, y: i32, prev: BlockId);
    /// A block changed; `was_center` distinguishes center vs proxy recording
    /// (`EditorTile.setBlock`).
    fn block_changed(
        &mut self,
        x: i32,
        y: i32,
        prev_block: BlockId,
        prev_rot: i32,
        prev_team: TeamId,
        was_center: bool,
    );
    /// A team changed (`EditorTile.setTeam`).
    fn team_changed(&mut self, x: i32, y: i32, prev_team: TeamId);
}

/// The editor map-load context (`MapEditor.Context`).
///
/// Delegates to the plan-06 [`Context`]; the optional [`TileOpSink`] is retained
/// here so M1's load path can suppress recording exactly like `isLoading()`.
pub struct EditorContext<'a> {
    /// The inner world-build context.
    pub inner: Context<'a>,
    /// The installed recorder (M1; `None` during a load).
    pub sink: Option<Box<dyn TileOpSink>>,
}

impl<'a> EditorContext<'a> {
    /// Creates an editor context over a grid.
    pub fn new(grid: &'a mut WorldGrid, content: &'a ContentRegistry) -> Self {
        Self {
            inner: Context::new(grid, content),
            sink: None,
        }
    }
}

impl WorldContext for EditorContext<'_> {
    fn tile_count(&self) -> usize {
        self.inner.tile_count()
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.inner.resize(width, height);
    }

    fn create(&mut self, x: u16, y: u16, floor: u16, overlay: u16, wall: u16) {
        self.inner.create(x, y, floor, overlay, wall);
    }

    fn is_generating(&self) -> bool {
        self.inner.is_generating()
    }

    fn begin(&mut self) {
        self.inner.begin();
    }

    fn end(&mut self) {
        self.inner.end();
    }

    fn set_block(&mut self, index: usize, block: u16) {
        self.inner.set_block(index, block);
    }

    fn has_building(&self, index: usize) -> bool {
        self.inner.has_building(index)
    }

    fn block_has_building_io(&self, index: usize) -> bool {
        self.inner.block_has_building_io(index)
    }

    fn set_tile_data(
        &mut self,
        index: usize,
        data: u8,
        floor_data: u8,
        overlay_data: u8,
        extra_data: i32,
    ) {
        self.inner
            .set_tile_data(index, data, floor_data, overlay_data, extra_data);
    }

    fn read_building(
        &mut self,
        index: usize,
        reader: &mut WireReader,
        version: u8,
    ) -> Result<(), IoError> {
        self.inner.read_building(index, reader, version)
    }

    fn is_map(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_context_delegates_create_resize() {
        let content = crate::content::test_support::test_registry();
        let mut grid = WorldGrid::new(0, 0);
        let mut ctx = EditorContext::new(&mut grid, &content);
        ctx.resize(3, 3);
        ctx.begin();
        ctx.create(1, 1, 80, 0, 0);
        ctx.end();
        assert_eq!(ctx.tile_count(), 9);
        assert_eq!(ctx.inner.grid.tiles.get(1, 1).floor, BlockId::new(80));
        assert!(ctx.is_map());
    }
}
