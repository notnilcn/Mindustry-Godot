// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawOperation` (`editor/DrawOperation.java`, plan 19 §3.3).
//!
//! A `Vec<u64>` of packed [`TileOp`]s. `undo` walks the list in reverse and
//! `redo` forward; each visit swaps the *live* tile value into the stored op and
//! writes the op's recorded value back to the tile. Because the swap is its own
//! inverse, `redo` after `undo` restores the drawn state exactly.

use crate::content::{BlockDef, BlockId, ContentRegistry};

use super::EditorGrid;
use super::tile_op::{
    OP_BLOCK, OP_DATA, OP_DATA_EXTRA, OP_FLOOR, OP_OVERLAY, OP_ROTATION, OP_TEAM,
};
use super::tile_op::{TileOp, TileOpData};

/// One undoable draw operation (`DrawOperation`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DrawOperation {
    ops: Vec<u64>,
}

impl DrawOperation {
    /// An empty operation.
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Wraps an existing packed op list.
    pub fn from_ops(ops: Vec<u64>) -> Self {
        Self { ops }
    }

    /// Whether the operation recorded nothing (`DrawOperation.isEmpty`).
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Number of packed ops (`DrawOperation.size`).
    pub fn size(&self) -> usize {
        self.ops.len()
    }

    /// The packed op list.
    pub fn ops(&self) -> &[u64] {
        &self.ops
    }

    /// Clears the operation while keeping its backing allocation
    /// (plan 19 §7d op-pool reuse).
    pub fn clear(&mut self) {
        self.ops.clear();
    }

    /// Backing allocation capacity (plan 19 M7 steady-state alloc audit).
    pub fn capacity(&self) -> usize {
        self.ops.capacity()
    }

    /// Reserves capacity up-front so steady-state recording never reallocates.
    pub fn reserve(&mut self, additional: usize) {
        self.ops.reserve(additional);
    }

    /// Drops the last `amount` ops (`DrawOperation.remove`).
    pub fn remove(&mut self, amount: usize) {
        let len = self.ops.len().saturating_sub(amount);
        self.ops.truncate(len);
    }

    /// Appends a packed op (`DrawOperation.addOperation`).
    pub fn add(&mut self, op: u64) {
        self.ops.push(op);
    }

    /// Reverses every op (`DrawOperation.undo`).
    pub fn undo(&mut self, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        for i in (0..self.ops.len()).rev() {
            self.update_tile(i, world, content);
        }
    }

    /// Re-applies every op (`DrawOperation.redo`).
    pub fn redo(&mut self, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        for i in 0..self.ops.len() {
            self.update_tile(i, world, content);
        }
    }

    /// Swap-in-previous + `setTile` (`DrawOperation.updateTile`).
    fn update_tile(&mut self, i: usize, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        let l = self.ops[i];
        let (x, y, ty) = (TileOp::x(l), TileOp::y(l), TileOp::ty(l));
        let current = Self::get_tile(world, x, y, ty);
        self.ops[i] = TileOp::get(x, y, ty, current);
        Self::set_tile(world, content, x, y, ty, TileOp::value(l));
    }

    /// Reads a tile field by op type (`DrawOperation.getTile`).
    pub fn get_tile(world: &dyn EditorGrid, x: i32, y: i32, ty: u8) -> i32 {
        match ty {
            OP_FLOOR => world.floor_id(x, y).raw() as i32,
            OP_OVERLAY => world.overlay_id(x, y).raw() as i32,
            OP_BLOCK => world.block_id(x, y).raw() as i32,
            OP_ROTATION => world.rotation(x, y),
            OP_TEAM => world.team_id(x, y) as i32,
            OP_DATA => {
                let (data, floor_data, overlay_data, _) = world.tile_data(x, y);
                TileOpData::get(data, floor_data, overlay_data)
            }
            OP_DATA_EXTRA => world.tile_data(x, y).3,
            _ => 0,
        }
    }

    /// Writes a tile field by op type (`DrawOperation.setTile`).
    pub fn set_tile(
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
        ty: u8,
        to: i32,
    ) {
        let fan_out = matches!(ty, OP_BLOCK | OP_TEAM | OP_ROTATION);
        if fan_out {
            for (lx, ly) in world.linked_tiles(x, y) {
                world.update_block(lx, ly);
                world.update_static(lx, ly);
            }
        } else {
            world.update_static(x, y);
        }

        // `editor.load(...)`: recording is suppressed while replaying.
        let prev_loading = world.is_loading();
        world.set_loading(true);
        match ty {
            OP_FLOOR => {
                if let Some(id) = content_id(to)
                    && is_floor(content.block(id))
                {
                    world.set_floor(x, y, id);
                }
            }
            OP_OVERLAY => {
                if let Some(id) = content_id(to)
                    && is_floor(content.block(id))
                {
                    world.set_overlay(x, y, id);
                }
            }
            OP_BLOCK => {
                if let Some(id) = content_id(to) {
                    let team = world.team_id(x, y);
                    let rot = world.rotation(x, y);
                    world.set_block(x, y, id, team, rot);
                }
            }
            OP_ROTATION => world.set_rotation(x, y, to),
            OP_TEAM => world.set_team(x, y, to as u8),
            OP_DATA => world.set_data(
                x,
                y,
                TileOpData::data(to),
                TileOpData::floor_data(to),
                TileOpData::overlay_data(to),
            ),
            OP_DATA_EXTRA => world.set_extra_data(x, y, to),
            _ => {}
        }
        world.set_loading(prev_loading);

        if fan_out {
            for (lx, ly) in world.linked_tiles(x, y) {
                world.update_block(lx, ly);
                world.update_static(lx, ly);
            }
        } else {
            world.update_static(x, y);
        }
    }
}

/// Decodes a packed content id, rejecting out-of-range/negative values.
pub fn content_id(to: i32) -> Option<BlockId> {
    (0..=u16::MAX as i32)
        .contains(&to)
        .then(|| BlockId::new(to as u16))
}

/// `Block instanceof Floor`.
pub fn is_floor(def: Option<&BlockDef>) -> bool {
    def.is_some_and(crate::maps::filters::block_info::is_floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_grid::TestGrid;

    /// `editor::tests::undo_redo_swap_identity`: apply → undo → redo restores
    /// every field the op touched.
    #[test]
    fn undo_redo_swap_identity() {
        let content = crate::content::test_support::test_registry();
        let mut world = TestGrid::new(4, 4);
        let stone = content.block_id("stone").unwrap();
        let wall = content.block_id("stone-wall").unwrap();

        let mut op = DrawOperation::new();
        op.add(TileOp::get(
            1,
            2,
            OP_FLOOR,
            content
                .block_id("sand")
                .map(|b| b.raw() as i32)
                .unwrap_or(0),
        ));
        op.add(TileOp::get(2, 2, OP_BLOCK, wall.raw() as i32));
        op.redo(&mut world, &content);
        assert_eq!(world.block_id(2, 2), wall);
        assert_ne!(world.floor_id(1, 2), stone);

        op.undo(&mut world, &content);
        assert_eq!(world.block_id(2, 2), BlockId::AIR);

        op.redo(&mut world, &content);
        assert_eq!(world.block_id(2, 2), wall);
    }

    /// Plan 19 M7 §7d steady-state allocation audit: once a draw operation is
    /// warmed up, recording more ops never grows the backing allocation.
    #[test]
    fn op_recording_capacity_is_stable_after_warmup() {
        // Pre-reserving keeps recording allocation-free across the reserved span.
        let mut op = DrawOperation::new();
        op.reserve(1000);
        let capacity = op.capacity();
        for i in 0..1000u64 {
            op.add(i);
        }
        assert_eq!(op.capacity(), capacity, "recording reallocated");
        // `remove` + re-add within the reserve also stays allocation-free.
        op.remove(500);
        for i in 0..500u64 {
            op.add(i);
        }
        assert_eq!(op.capacity(), capacity);
    }

    /// Plan 19 §7d MapView steady-state audit: after warming past the undo-stack
    /// cap, a repeated line → flush → undo → redo cycle must not allocate
    /// (scratch buffers + pooled `DrawOperation`s). Requires `--features
    /// alloc-audit`; the assertion mirrors the `editor bench --suite mapview`
    /// counters.
    #[cfg(feature = "alloc-audit")]
    #[test]
    fn steady_state_recording_is_allocation_free() {
        use bevy_ecs::world::World;

        use crate::content::test_support::test_registry;
        use crate::editor::grid::WorldEditorGrid;
        use crate::editor::stack::MAX_SIZE;
        use crate::editor::tool::touched_line;
        use crate::editor::{EditorTool, MapEditor};
        use crate::util::alloc::{alloc_count, enabled};
        use crate::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

        assert!(
            enabled(),
            "alloc-audit feature must be enabled for this test"
        );
        let content = test_registry();
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let mut editor = MapEditor::new();
        let mut grid = WorldGrid::new(0, 0);
        let mut ecs = World::new();
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        {
            let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            editor.begin_edit_size(&mut world, &content, 64, 64);
        }
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        let content_ref = &content;
        fn run_step(
            editor: &mut MapEditor,
            world: &mut WorldEditorGrid<'_>,
            content: &crate::content::ContentRegistry,
            wall: crate::content::BlockId,
        ) {
            editor.draw_block = wall;
            touched_line(editor, EditorTool::Line, world, content, 2, 2, 60, 2);
            editor.flush_op();
            editor.undo(world, content);
            editor.redo(world, content);
            // Frame boundary: drain the world-event log (retaining capacity).
            world.clear_events();
        }
        for _ in 0..(MAX_SIZE + 1) {
            run_step(&mut editor, &mut world, content_ref, wall);
        }
        let before = alloc_count();
        for _ in 0..40 {
            run_step(&mut editor, &mut world, content_ref, wall);
        }
        assert_eq!(
            alloc_count(),
            before,
            "steady-state editor recording/flush/undo/redo allocated"
        );
    }
}
