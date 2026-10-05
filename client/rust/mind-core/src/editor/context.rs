// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Editor world-load context and the tile-op recorder seam
//! (`editor/MapEditor.java` `Context`, `editor/EditorTile.java`, plan 19 §3.4).
//!
//! `EditorContext` implements plan 04's `WorldContext` for map/image loads into
//! the editor. It deliberately does **not** wrap plan-06's `Context`: a map load
//! needs `SaveReadState.content` to hold a `&mut ContentRegistry` while the
//! context is also live, so the editor context precomputes the one content fact
//! it needs (`Block.hasBuilding`) into a table and owns the grid directly.
//!
//! `TileOpSink` is the optional recorder hook on the world (deviation §2.3.1);
//! it is `None` in normal play and installed while editing. The draw path records
//! ops explicitly on [`super::MapEditor`]; the sink is the seam for mutations
//! that originate outside the draw path (07/12 hooks). [`EditorRecorder`] is the
//! concrete sink installed by the M1 lifecycle.

use std::sync::{Arc, Mutex};

use crate::content::{BlockId, ContentRegistry};
use crate::ecs::TeamId;
use crate::io::IoError;
use crate::io::save::WorldContext;
use crate::io::wire::WireReader;
use crate::world::WorldGrid;

use super::tile_op::{OP_BLOCK, OP_FLOOR, OP_OVERLAY, OP_TEAM, TileOp};

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

/// The concrete [`TileOpSink`] the editor installs on a [`super::grid::WorldEditorGrid`].
///
/// It appends packed [`TileOp`]s to a shared buffer that [`super::MapEditor`]
/// drains into its current [`super::DrawOperation`] (`EditorTile.op`). Center
/// fan-out for multiblock proxies records on the center tile only, matching
/// `EditorTile.setBlock`'s center branch for the mutations the sink observes.
#[derive(Clone, Default)]
pub struct EditorRecorder {
    ops: Arc<Mutex<Vec<u64>>>,
}

impl EditorRecorder {
    /// A recorder sharing `ops` (the buffer the editor drains).
    pub fn new(ops: Arc<Mutex<Vec<u64>>>) -> Self {
        Self { ops }
    }

    /// The shared op buffer.
    pub fn shared(&self) -> Arc<Mutex<Vec<u64>>> {
        self.ops.clone()
    }

    fn push(&self, op: u64) {
        if let Ok(mut ops) = self.ops.lock() {
            ops.push(op);
        }
    }
}

impl TileOpSink for EditorRecorder {
    fn floor_changed(&mut self, x: i32, y: i32, prev: BlockId) {
        self.push(TileOp::get(x, y, OP_FLOOR, prev.raw() as i32));
    }

    fn overlay_changed(&mut self, x: i32, y: i32, prev: BlockId) {
        self.push(TileOp::get(x, y, OP_OVERLAY, prev.raw() as i32));
    }

    fn block_changed(
        &mut self,
        x: i32,
        y: i32,
        prev_block: BlockId,
        prev_rot: i32,
        prev_team: TeamId,
        was_center: bool,
    ) {
        // Proxy tiles fan out to the center, which is recorded separately by the
        // grid adapter; ignore the proxy callback to avoid duplicate ops.
        if !was_center {
            return;
        }
        // `EditorTile.setBlock` records the old rotation/team when a building was
        // present. A nonzero rotation/team is the observable proxy for that here
        // (the tile had a live build); a sharded team building is recorded by the
        // explicit draw path's `record_block_change`.
        if prev_rot != 0 || prev_team != TeamId(0) {
            self.push(TileOp::get(x, y, 2, prev_rot));
            self.push(TileOp::get(x, y, OP_TEAM, prev_team.0 as i32));
        }
        self.push(TileOp::get(x, y, OP_BLOCK, prev_block.raw() as i32));
    }

    fn team_changed(&mut self, x: i32, y: i32, prev_team: TeamId) {
        self.push(TileOp::get(x, y, OP_TEAM, prev_team.0 as i32));
    }
}

/// The editor map-load context (`MapEditor.Context`).
///
/// Owns the grid directly and carries a precomputed `hasBuilding` table so a
/// `&mut ContentRegistry` stays available for `SaveReadState` (see the module
/// docs). Decoded building payloads are queued for the ECS-owning host, exactly
/// like plan 06's `Context`.
pub struct EditorContext<'a> {
    /// The grid being built.
    pub grid: &'a mut WorldGrid,
    /// `Block.hasBuilding` by content id (`read_map` decides entity-chunk skips).
    has_building: Vec<bool>,
    /// Decoded tile-entity payloads awaiting spawn by the ECS-owning host.
    pub pending_buildings: Vec<(usize, crate::world::building_io::DecodedBase)>,
    /// Legacy `MSAV` tile-entity layout (version byte inside the payload).
    legacy_entities: bool,
    /// The installed recorder (M1; dropped during a load).
    pub sink: Option<Box<dyn TileOpSink>>,
}

impl<'a> EditorContext<'a> {
    /// Creates an editor context over a grid, snapshotting `Block.hasBuilding`.
    pub fn new(grid: &'a mut WorldGrid, content: &ContentRegistry) -> Self {
        let has_building: Vec<bool> = content
            .blocks()
            .iter()
            .map(|def| crate::world::block_has_building(content, def.id))
            .collect();
        Self {
            grid,
            has_building,
            pending_buildings: Vec::new(),
            legacy_entities: false,
            sink: None,
        }
    }

    /// Drains decoded building payloads (`(tile index, base)`).
    pub fn take_pending_buildings(
        &mut self,
    ) -> Vec<(usize, crate::world::building_io::DecodedBase)> {
        std::mem::take(&mut self.pending_buildings)
    }

    /// Read-only view of the decoded building payloads.
    pub fn pending_buildings(&self) -> &[(usize, crate::world::building_io::DecodedBase)] {
        &self.pending_buildings
    }

    fn has_building_for(&self, block: BlockId) -> bool {
        self.has_building
            .get(block.raw() as usize)
            .copied()
            .unwrap_or(false)
    }
}

impl WorldContext for EditorContext<'_> {
    fn tile_count(&self) -> usize {
        self.grid.tiles.len()
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.grid.resize(width as i32, height as i32);
    }

    fn create(&mut self, x: u16, y: u16, floor: u16, overlay: u16, wall: u16) {
        if !self.grid.tiles.in_bounds(x as i32, y as i32) {
            return;
        }
        let index = self.grid.tiles.index(x as i32, y as i32);
        let tile = self.grid.tiles.geti_mut(index);
        tile.floor = BlockId::new(floor);
        tile.overlay = BlockId::new(overlay);
        tile.block = BlockId::new(wall);
        tile.build = None;
    }

    fn is_generating(&self) -> bool {
        self.grid.generating
    }

    fn begin(&mut self) {
        self.grid.generating = true;
    }

    fn end(&mut self) {
        self.grid.generating = false;
    }

    fn set_block(&mut self, index: usize, block: u16) {
        let tile = self.grid.tiles.geti_mut(index);
        tile.block = BlockId::new(block);
        tile.build = None;
    }

    fn has_building(&self, index: usize) -> bool {
        self.grid.tiles.geti(index).build.is_some()
    }

    fn block_has_building_io(&self, index: usize) -> bool {
        self.has_building_for(self.grid.tiles.geti(index).block)
    }

    fn set_tile_data(
        &mut self,
        index: usize,
        data: u8,
        floor_data: u8,
        overlay_data: u8,
        extra_data: i32,
    ) {
        let tile = self.grid.tiles.geti_mut(index);
        tile.data = data as i8;
        tile.floor_data = floor_data as i8;
        tile.overlay_data = overlay_data as i8;
        tile.extra_data = extra_data;
    }

    fn read_building(
        &mut self,
        index: usize,
        reader: &mut WireReader,
        version: u8,
    ) -> Result<(), IoError> {
        let mut decoded = crate::world::building_io::DecodedBase::default();
        if self.legacy_entities {
            crate::world::building_io::read_base_legacy(&mut decoded, reader)?;
        } else {
            crate::world::building_io::read_base(&mut decoded, reader, version)?;
        }
        self.pending_buildings.push((index, decoded));
        Ok(())
    }

    fn is_map(&self) -> bool {
        true
    }

    fn set_legacy_entities(&mut self, legacy: bool) {
        self.legacy_entities = legacy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn editor_context_delegates_create_resize() {
        let content = test_registry();
        let mut grid = WorldGrid::new(0, 0);
        let mut ctx = EditorContext::new(&mut grid, &content);
        ctx.resize(3, 3);
        ctx.begin();
        ctx.create(1, 1, 80, 0, 0);
        ctx.end();
        assert_eq!(ctx.tile_count(), 9);
        assert_eq!(ctx.grid.tiles.get(1, 1).floor, BlockId::new(80));
        assert!(ctx.is_map());
    }

    #[test]
    fn recorder_shared_buffer_receives_ops() {
        let ops = Arc::new(Mutex::new(Vec::new()));
        let mut recorder = EditorRecorder::new(ops.clone());
        recorder.floor_changed(1, 2, BlockId::AIR);
        recorder.team_changed(3, 4, TeamId(1));
        recorder.block_changed(5, 6, BlockId::AIR, 0, TeamId(0), true);
        let recorded = ops.lock().unwrap();
        assert_eq!(recorded.len(), 3);
        assert_eq!(TileOp::x(recorded[0]), 1);
        assert_eq!(TileOp::ty(recorded[0]), OP_FLOOR);
        assert_eq!(TileOp::ty(recorded[1]), OP_TEAM);
        assert_eq!(TileOp::ty(recorded[2]), OP_BLOCK);
    }
}
