// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map editor model (plan 19; `editor/MapEditor.java`, `editor/EditorTile.java`).
//!
//! `mind-core::editor` is Godot-free and tokio-free (D1, OD19-B). The editor
//! mutates the live world through the [`EditorGrid`] seam; the concrete
//! [`WorldEditorGrid`](crate::editor::grid::WorldEditorGrid) adapts it to
//! [`crate::world::WorldGrid`] + `WorldCtx`, while tests use an in-memory
//! [`test_grid::TestGrid`]. Tile op recording is explicit here (the upstream
//! `EditorTile` subclass is replaced by the M1 `TileOpSink`; plan 19 §2.3.1).

pub mod assets;
pub mod banned;
pub mod context;
pub mod draw_op;
// Plan 19 §3.1 names this file `gen.rs`; `gen` is a Rust 2024 reserved keyword,
// so the module is exported as `generate` via an explicit path.
#[path = "gen.rs"]
pub mod generate;
pub mod grid;
pub mod lifecycle;
pub mod maps_glue;
pub mod objectives;
pub mod playtest;
pub mod preview;
pub mod processors;
pub mod stack;
pub mod tile_op;
pub mod tool;
pub mod wave_graph;

#[cfg(test)]
pub mod test_grid;

pub use draw_op::DrawOperation;
pub use stack::OperationStack;
pub use tile_op::{TileOp, TileOpData};
pub use tool::EditorTool;

use std::sync::{Arc, Mutex};

use indexmap::IndexMap;

use crate::content::{BlockDef, BlockId, ContentRegistry};
use crate::maps::filters::block_info;

/// Brush-size table (`MapEditor.brushSizes`).
pub const BRUSH_SIZES: [f32; 9] = [1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 9.0, 15.0, 20.0];

/// Editor-relevant classification of one block (`Block`/`Floor` predicates).
///
/// `rotate` upstream lives on the generated `BlockInstance` (plan 07); M0 leaves
/// it `false` and the draw path records rotations only when the caller opts in
/// (plan 19 M1 wires `BlockInstance::rotate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorBlockInfo {
    /// Block content id.
    pub id: BlockId,
    /// Multiblock edge size (`Block.size`).
    pub size: i32,
    /// `Block instanceof Floor`.
    pub is_floor: bool,
    /// `Block instanceof OverlayFloor`.
    pub is_overlay: bool,
    /// `Block.isMultiblock()`.
    pub is_multiblock: bool,
    /// `Block.synthetic()` (`update || destructible`).
    pub synthetic: bool,
    /// `Block.rotate` (plan-07 `BlockInstance::rotate` derived from the kind).
    pub rotate: bool,
    /// `Block.saveData`.
    pub save_data: bool,
    /// `Block.saveConfig`.
    pub save_config: bool,
    /// `Block.wallOre`.
    pub wall_ore: bool,
    /// `Block.solid`.
    pub solid: bool,
    /// `Floor.supportsOverlay` (approximation: surface, non-liquid).
    pub supports_overlay: bool,
    /// `Block.isStatic()`.
    pub is_static: bool,
    /// `Floor.isLiquid`.
    pub is_liquid: bool,
}

impl EditorBlockInfo {
    /// The air block's classification (all predicates false, size 1).
    pub fn air() -> Self {
        Self {
            id: BlockId::AIR,
            size: 1,
            is_floor: false,
            is_overlay: false,
            is_multiblock: false,
            synthetic: false,
            rotate: false,
            save_data: false,
            save_config: false,
            wall_ore: false,
            solid: false,
            supports_overlay: false,
            is_static: false,
            is_liquid: false,
        }
    }

    /// Classifies a block definition (`None` = air).
    pub fn from_def(def: Option<&BlockDef>) -> Self {
        let Some(def) = def else {
            return Self::air();
        };
        Self {
            id: def.id,
            size: def.size.max(1),
            is_floor: block_info::is_floor(def),
            is_overlay: block_info::is_overlay(def),
            is_multiblock: def.size > 1,
            synthetic: block_info::synthetic(def),
            rotate: crate::world::block::kind_rotates(def.kind),
            save_data: def.save_data,
            save_config: def.save_config,
            wall_ore: def.wall_ore,
            solid: def.solid,
            supports_overlay: block_info::has_surface(def) && !block_info::is_liquid(def),
            is_static: block_info::is_static(def),
            is_liquid: block_info::is_liquid(def),
        }
    }
}

/// The tile-access seam the editor draws through (plan 19 §2.3.1/§3.4).
///
/// Implemented by [`grid::WorldEditorGrid`] over the live world and by
/// [`test_grid::TestGrid`] for unit tests. All mutation is centralized here so
/// the editor model stays free of `WorldGrid` internals.
pub trait EditorGrid {
    /// Grid width in tiles.
    fn width(&self) -> i32;
    /// Grid height in tiles.
    fn height(&self) -> i32;
    /// Whether `(x, y)` lies inside the grid.
    fn in_bounds(&self, x: i32, y: i32) -> bool;
    /// Whether op recording is currently suppressed (`editor.isLoading()`).
    fn is_loading(&self) -> bool;
    /// Sets the loading flag (`load(...)`).
    fn set_loading(&mut self, loading: bool);
    /// Begins a map load (`world.beginMapLoad`).
    fn begin_map_load(&mut self);
    /// Ends a map load (`world.endMapLoad`).
    fn end_map_load(&mut self);
    /// Replaces the grid with a fresh all-air grid (`world.resize`).
    fn resize(&mut self, width: i32, height: i32);

    /// Floor content id.
    fn floor_id(&self, x: i32, y: i32) -> BlockId;
    /// Overlay content id.
    fn overlay_id(&self, x: i32, y: i32) -> BlockId;
    /// Block content id.
    fn block_id(&self, x: i32, y: i32) -> BlockId;
    /// Team id (`0` when no building).
    fn team_id(&self, x: i32, y: i32) -> u8;
    /// Building rotation (`0` when no building).
    fn rotation(&self, x: i32, y: i32) -> i32;
    /// `(data, floor_data, overlay_data, extra_data)`.
    fn tile_data(&self, x: i32, y: i32) -> (i8, i8, i8, i32);
    /// Whether a building entity occupies the tile.
    fn has_build(&self, x: i32, y: i32) -> bool;
    /// Whether the tile is the center of its building (`Tile.isCenter`).
    fn is_center(&self, x: i32, y: i32) -> bool;
    /// Every tile belonging to the building at `(x, y)` (footprint; includes
    /// `(x, y)` when standalone). `SmallVec` keeps the common 1..9-tile cases
    /// (single blocks through 3×3 multiblocks) heap-free on the op hot path.
    fn linked_tiles(&self, x: i32, y: i32) -> smallvec::SmallVec<[(i32, i32); 9]>;

    /// Sets the floor (`Tile.setFloor`).
    fn set_floor(&mut self, x: i32, y: i32, floor: BlockId);
    /// Sets the overlay (`Tile.setOverlay`).
    fn set_overlay(&mut self, x: i32, y: i32, overlay: BlockId);
    /// Sets the block/team/rotation (`Tile.setBlock`).
    fn set_block(&mut self, x: i32, y: i32, block: BlockId, team: u8, rot: i32);
    /// Sets the team of the building at `(x, y)` (`Tile.setTeam`).
    fn set_team(&mut self, x: i32, y: i32, team: u8);
    /// Sets the rotation of the building at `(x, y)`.
    fn set_rotation(&mut self, x: i32, y: i32, rot: i32);
    /// Sets the private data bytes (`Tile.setPackedData` fields).
    fn set_data(&mut self, x: i32, y: i32, data: i8, floor_data: i8, overlay_data: i8);
    /// Sets `Tile.extraData`.
    fn set_extra_data(&mut self, x: i32, y: i32, extra: i32);
    /// Recaches one tile's static sprites (`renderer.updateStatic`).
    fn update_static(&mut self, x: i32, y: i32);
    /// Marks one tile's chunk dirty (`renderer.updateBlock`).
    fn update_block(&mut self, x: i32, y: i32);

    /// Clears static-wall edge darkness (`EditorRenderer.resize` `StaticWall.data = 0`).
    ///
    /// Plan 19 M1; the render side is a `RenderHooks` no-op in headless.
    fn clear_editor_darkness(&mut self);
    /// Recaches every tile after a load/resize (`EditorRenderer.resize`).
    fn recache_all(&mut self);
    /// Rebuilds the grid resized by `(width, height)` with every in-bounds tile
    /// shifted by `(shift_x, shift_y)` and shifted building centers reattached
    /// (`MapEditor.resize`). Out-of-bounds tiles become the default stone floor.
    fn resize_shift(
        &mut self,
        content: &ContentRegistry,
        width: i32,
        height: i32,
        shift_x: i32,
        shift_y: i32,
    );
}

/// The editor singleton (`MapEditor`).
#[derive(bevy_ecs::prelude::Resource)]
pub struct MapEditor {
    /// `name`/`description`/`author`/`rules`/`genfilters`/`locales`/`steamid`.
    pub tags: IndexMap<String, String>,
    /// Brush radius; always one of [`BRUSH_SIZES`] (`reset` → `1.0`).
    pub brush_size: f32,
    /// Placement rotation `0..=3`.
    pub rotation: i32,
    /// Block being drawn (default `stone`).
    pub draw_block: BlockId,
    /// Team being drawn (default sharded `0`).
    pub draw_team: u8,
    /// Whether terrain (walls) is shown.
    pub show_terrain: bool,
    /// Whether floors are shown.
    pub show_floor: bool,
    /// Whether buildings are shown.
    pub show_buildings: bool,
    /// Grid overlay (`ctrl+g`).
    pub grid: bool,
    /// Active tool (default [`EditorTool::Zoom`]).
    pub tool: EditorTool,
    /// Per-tool alternate mode (`-1` = standard).
    pub tool_modes: [i32; EditorTool::COUNT],
    /// Suppresses recording (`MapEditor.loading`).
    pub loading: bool,
    /// Informational dirty flag (upstream never reads it; plan 19 §2.3.9).
    pub saved: bool,
    /// Whether `show()` should not reset the world.
    pub shown_with_map: bool,
    stack: OperationStack,
    current_op: DrawOperation,
    /// Recycled [`DrawOperation`] allocations (plan 19 §7d zero-alloc audit).
    op_pool: Vec<DrawOperation>,
    /// Reused Bresenham point scratch (`tool::touched_line`).
    line_scratch: Vec<(i32, i32)>,
    /// Reused brush footprint scratch (`MapEditor.draw_blocks*`).
    draw_scratch: Vec<(i32, i32)>,
    /// Shared buffer of the installed [`context::TileOpSink`] (`EditorTile.op`).
    recorder: Option<Arc<Mutex<Vec<u64>>>>,
}

impl Default for MapEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl MapEditor {
    /// A fresh editor with upstream defaults (draw block `air` until the first
    /// `reset` resolves `stone`; brush `1`, sharded team, all views on, zoom).
    pub fn new() -> Self {
        Self {
            tags: IndexMap::new(),
            brush_size: 1.0,
            rotation: 0,
            draw_block: BlockId::AIR,
            draw_team: 0,
            show_terrain: true,
            show_floor: true,
            show_buildings: true,
            grid: false,
            tool: EditorTool::Zoom,
            tool_modes: [-1; EditorTool::COUNT],
            loading: false,
            saved: false,
            shown_with_map: false,
            stack: OperationStack::new(),
            current_op: DrawOperation::new(),
            op_pool: Vec::new(),
            line_scratch: Vec::new(),
            draw_scratch: Vec::new(),
            recorder: None,
        }
    }

    /// `MapEditor.reset()`: clears the op stack, resets brush to `1`, draw block
    /// to `stone` and clears tags (team is *not* reset upstream).
    pub fn reset(&mut self, content: &ContentRegistry) {
        self.clear_op();
        self.brush_size = 1.0;
        self.draw_block = content.block_id("stone").unwrap_or(BlockId::STONE_WALL);
        self.tags.clear();
    }

    /// Whether the editor is loading (`MapEditor.isLoading`).
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Resolves `draw_block` classification.
    pub fn draw_info(&self, content: &ContentRegistry) -> EditorBlockInfo {
        EditorBlockInfo::from_def(content.block(self.draw_block))
    }

    /// `beginEdit(width, height)`: reset + create a stone-floored grid.
    pub fn begin_edit_size(
        &mut self,
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        width: i32,
        height: i32,
    ) {
        self.reset(content);
        self.loading = true;
        world.begin_map_load();
        world.resize(width, height);
        let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
        let overlay = content.block_id("air").unwrap_or(BlockId::AIR);
        for y in 0..height {
            for x in 0..width {
                world.set_floor(x, y, stone);
                world.set_overlay(x, y, overlay);
            }
        }
        world.end_map_load();
        self.loading = false;
    }

    /// Appends a packed op to the current operation (`MapEditor.addTileOp`).
    pub fn add_tile_op(&mut self, data: u64) {
        if self.loading {
            return;
        }
        self.current_op.add(data);
    }

    /// Records the current value of op type `ty` on `(x, y)`.
    pub fn record_op(&mut self, world: &dyn EditorGrid, x: i32, y: i32, ty: u8) {
        let value = DrawOperation::get_tile(world, x, y, ty);
        self.add_tile_op(tile_op::TileOp::get(x, y, ty, value));
    }

    /// Flushes the accumulated operation to the undo stack
    /// (`MapEditor.flushOp`).
    ///
    /// The finished [`DrawOperation`] moves into the stack while a recycled
    /// spare (from the op pool / stack eviction) becomes the next current op, so
    /// steady-state recording never allocates a fresh tile vector (plan 19 §7d).
    pub fn flush_op(&mut self) {
        if self.current_op.is_empty() {
            return;
        }
        // Move the finished op into the stack first so any stack eviction lands
        // in the pool before we pick the next current op; once the stack is at
        // its cap this always reuses a capacity-bearing op (cleared in place).
        let done = std::mem::take(&mut self.current_op);
        self.stack.add_recycling(done, &mut self.op_pool);
        let mut next = self.op_pool.pop().unwrap_or_default();
        next.clear();
        self.current_op = next;
    }

    /// Pushes a complete operation onto the undo stack (dev/oracle path).
    pub fn push_op(&mut self, op: DrawOperation) {
        if !op.is_empty() {
            self.stack.add(op);
        }
    }

    /// Clears the undo stack (`MapEditor.clearOp`).
    pub fn clear_op(&mut self) {
        self.stack.clear();
        self.current_op.clear();
    }

    /// Number of ops in the current operation (`MapEditor.ops`).
    pub fn ops(&self) -> usize {
        self.current_op.size()
    }

    /// Number of retained undo operations (`OperationStack.len`).
    pub fn retained_ops(&self) -> usize {
        self.stack.len()
    }

    /// The retained undo operations as packed op lists, oldest first
    /// (plan 19 M3 `dev_op_log`).
    pub fn op_log(&self) -> Vec<Vec<u64>> {
        self.stack
            .ops()
            .iter()
            .map(|operation| operation.ops().to_vec())
            .collect()
    }

    /// Removes the last `amount` ops from the current operation.
    pub fn remove_last_ops(&mut self, amount: usize) {
        if self.loading {
            return;
        }
        self.current_op.remove(amount);
    }

    /// Installs this editor's [`context::TileOpSink`] on a live world
    /// (`EditorTile` replacement; plan 19 §2.3.1/§3.4).
    pub fn install_recorder(&mut self, world: &mut grid::WorldEditorGrid<'_>) {
        let ops: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        world.install_sink(Box::new(context::EditorRecorder::new(ops.clone())));
        self.recorder = Some(ops);
    }

    /// Removes the recorder from the live world (`hide`/`reset`).
    pub fn remove_recorder(&mut self, world: &mut grid::WorldEditorGrid<'_>) {
        world.take_sink();
        self.recorder = None;
    }

    /// Drains the installed recorder's buffer into the current operation
    /// (`EditorTile.op`); returns the number of packed ops moved.
    pub fn drain_recorded_ops(&mut self) -> usize {
        let Some(buffer) = &self.recorder else {
            return 0;
        };
        let drained: Vec<u64> = match buffer.lock() {
            Ok(mut ops) => std::mem::take(&mut *ops),
            Err(_) => return 0,
        };
        let count = drained.len();
        for op in drained {
            self.add_tile_op(op);
        }
        count
    }

    /// Rust form of `EditorTile.skip()`: recording is suppressed while loading,
    /// while the world generates, and during normal play (`!editor`).
    pub fn should_record_ops(&self, state_is_game: bool, world_generating: bool) -> bool {
        !self.loading && !world_generating && !state_is_game
    }

    /// Whether an undo is available.
    pub fn can_undo(&self) -> bool {
        self.stack.can_undo()
    }

    /// Whether a redo is available.
    pub fn can_redo(&self) -> bool {
        self.stack.can_redo()
    }

    /// Undoes one operation (`MapEditor.undo`).
    pub fn undo(&mut self, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        self.stack.undo(world, content);
    }

    /// Redoes one operation (`MapEditor.redo`).
    pub fn redo(&mut self, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        self.stack.redo(world, content);
    }

    // --- drawing primitives (editor/MapEditor.java:139-306) ---

    /// `drawBlocksReplace`: replace everything occupied or when drawing a floor.
    pub fn draw_blocks_replace(
        &mut self,
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
    ) {
        let draw_block = self.draw_block;
        let is_floor = EditorBlockInfo::from_def(content.block(draw_block)).is_floor;
        self.draw_blocks_where(world, content, x, y, false, false, &move |w, _c, tx, ty| {
            w.block_id(tx, ty) != BlockId::AIR || is_floor
        });
    }

    /// `drawBlocks(x, y)`.
    pub fn draw_blocks(
        &mut self,
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
    ) {
        self.draw_blocks_where(world, content, x, y, false, false, &|_, _, _, _| true);
    }

    /// `drawBlocks(x, y, square, forceOverlay, tester)`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_blocks_where(
        &mut self,
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
        square: bool,
        force_overlay: bool,
        tester: &dyn Fn(&dyn EditorGrid, &ContentRegistry, i32, i32) -> bool,
    ) {
        let info = self.draw_info(content);
        if info.is_multiblock {
            let max_x = (world.width() - info.size / 2 - 1).max(0);
            let max_y = (world.height() - info.size / 2 - 1).max(0);
            let cx = x.clamp((info.size - 1) / 2, max_x);
            let cy = y.clamp((info.size - 1) / 2, max_y);
            if !self.has_overlap(world, content, cx, cy) {
                self.record_block_change(world, cx, cy);
                world.set_block(cx, cy, self.draw_block, self.draw_team, self.rotation);
                self.add_tile_op(tile_op::TileOp::get(
                    cx,
                    cy,
                    tile_op::OP_TEAM,
                    self.draw_team as i32,
                ));
            }
            return;
        }

        let is_floor = info.is_floor && self.draw_block != BlockId::AIR;
        let mut targets = std::mem::take(&mut self.draw_scratch);
        targets.clear();
        if square {
            self.for_each_square(world, x, y, &mut |tx, ty| targets.push((tx, ty)));
        } else {
            self.for_each_circle(world, x, y, &mut |tx, ty| targets.push((tx, ty)));
        }

        for &(tx, ty) in &targets {
            if !tester(world, content, tx, ty) {
                continue;
            }
            self.draw_one(world, content, tx, ty, force_overlay, is_floor, &info);
        }
        self.draw_scratch = targets;
    }

    /// `drawBlocks(x, y, square=true, ...)` with an always-true tester.
    pub fn draw_blocks_square(
        &mut self,
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
        force_overlay: bool,
    ) {
        self.draw_blocks_where(world, content, x, y, true, force_overlay, &|_, _, _, _| {
            true
        });
    }

    /// One tile's draw (`MapEditor.drawBlocks` inner `drawer`).
    #[allow(clippy::too_many_arguments)]
    fn draw_one(
        &mut self,
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        tx: i32,
        ty: i32,
        force_overlay: bool,
        is_floor: bool,
        info: &EditorBlockInfo,
    ) {
        let block = self.draw_block;
        let mut changed = false;

        let mut did_data_op = false;
        let (mut old_data1, mut old_data2) = (0i32, 0i32);
        if info.save_data || should_save_data(world, content, tx, ty) {
            let (data, floor_data, overlay_data, extra) = world.tile_data(tx, ty);
            self.add_tile_op(tile_op::TileOp::get(
                tx,
                ty,
                tile_op::OP_DATA,
                TileOpData::get(data, floor_data, overlay_data),
            ));
            self.add_tile_op(tile_op::TileOp::get(tx, ty, tile_op::OP_DATA_EXTRA, extra));
            old_data1 = TileOpData::get(data, floor_data, overlay_data);
            old_data2 = extra;
            did_data_op = true;
        }
        let pre_data_ops = self.ops();

        if is_floor {
            if force_overlay {
                self.record_op(world, tx, ty, tile_op::OP_OVERLAY);
                world.set_overlay(tx, ty, block);
                changed = true;
            } else if !(info.wall_ore
                && !EditorBlockInfo::from_def(content.block(world.block_id(tx, ty))).solid)
            {
                if world.floor_id(tx, ty) != block {
                    self.record_op(world, tx, ty, tile_op::OP_FLOOR);
                }
                world.set_floor(tx, ty, block);
                let overlay = world.overlay_id(tx, ty);
                if !tool::is_overlay(content.block(overlay)) && !info.supports_overlay {
                    if overlay != BlockId::AIR {
                        self.record_op(world, tx, ty, tile_op::OP_OVERLAY);
                    }
                    world.set_overlay(tx, ty, BlockId::AIR);
                }
                changed = true;
            }
        } else {
            let tile_block = world.block_id(tx, ty);
            let tile_multiblock =
                EditorBlockInfo::from_def(content.block(tile_block)).is_multiblock;
            if !(tile_multiblock && !info.is_multiblock) {
                if info.rotate && world.rotation(tx, ty) != self.rotation {
                    self.add_tile_op(tile_op::TileOp::get(
                        tx,
                        ty,
                        tile_op::OP_ROTATION,
                        self.rotation,
                    ));
                }
                self.record_block_change(world, tx, ty);
                world.set_block(tx, ty, block, self.draw_team, self.rotation);
                changed = !info.synthetic;
                if info.synthetic {
                    self.add_tile_op(tile_op::TileOp::get(
                        tx,
                        ty,
                        tile_op::OP_TEAM,
                        self.draw_team as i32,
                    ));
                }
            }
        }

        if changed && info.save_config {
            // `drawBlock.placeEnded(tile, ...)` is plan 07's block-config hook
            // (plan 19 M1); M0 records the op only.
        }

        // Data did not change and no mutation op was recorded: drop the pre-ops.
        if did_data_op && self.ops() == pre_data_ops {
            let (data, floor_data, overlay_data, extra) = world.tile_data(tx, ty);
            if old_data1 == TileOpData::get(data, floor_data, overlay_data) && old_data2 == extra {
                self.remove_last_ops(2);
            }
        }
    }

    /// Records the previous rotation/team (when a build exists) and block
    /// (`EditorTile.setBlock` center branch).
    pub fn record_block_change(&mut self, world: &dyn EditorGrid, x: i32, y: i32) {
        if world.has_build(x, y) {
            self.add_tile_op(tile_op::TileOp::get(
                x,
                y,
                tile_op::OP_ROTATION,
                world.rotation(x, y),
            ));
            self.add_tile_op(tile_op::TileOp::get(
                x,
                y,
                tile_op::OP_TEAM,
                world.team_id(x, y) as i32,
            ));
        }
        self.add_tile_op(tile_op::TileOp::get(
            x,
            y,
            tile_op::OP_BLOCK,
            world.block_id(x, y).raw() as i32,
        ));
    }

    /// `MapEditor.hasOverlap`.
    pub fn has_overlap(
        &self,
        world: &dyn EditorGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
    ) -> bool {
        let info = self.draw_info(content);
        let tile = world.block_id(x, y);
        if world.is_center(x, y) && tile != self.draw_block {
            let other = EditorBlockInfo::from_def(content.block(tile));
            if other.size == info.size {
                return false;
            }
        }
        let offset = -(info.size - 1) / 2;
        for dx in 0..info.size {
            for dy in 0..info.size {
                let wx = dx + offset + x;
                let wy = dy + offset + y;
                if world.in_bounds(wx, wy)
                    && EditorBlockInfo::from_def(content.block(world.block_id(wx, wy)))
                        .is_multiblock
                {
                    return true;
                }
            }
        }
        false
    }

    /// `MapEditor.addCliffs` (8-neighbour bitmask, static blocks only).
    pub fn add_cliffs(&mut self, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        let cliff = content.block_id("cliff").unwrap_or(BlockId::AIR);
        let air = content.block_id("air").unwrap_or(BlockId::AIR);
        let (width, height) = (world.width(), world.height());
        for x in 0..width {
            for y in 0..height {
                let block = world.block_id(x, y);
                let info = EditorBlockInfo::from_def(content.block(block));
                if !info.is_static || block == cliff {
                    continue;
                }
                let mut rotation = 0i32;
                for (i, (dx, dy)) in D8.iter().enumerate() {
                    let (ox, oy) = (x + dx, y + dy);
                    if world.in_bounds(ox, oy) {
                        let other = world.block_id(ox, oy);
                        if !EditorBlockInfo::from_def(content.block(other)).is_static {
                            rotation |= 1 << i;
                        }
                    }
                }
                if rotation != 0 {
                    world.set_block(x, y, cliff, world.team_id(x, y), world.rotation(x, y));
                }
                let (_, floor_data, overlay_data, _) = world.tile_data(x, y);
                world.set_data(x, y, rotation as i8, floor_data, overlay_data);
            }
        }
        for x in 0..width {
            for y in 0..height {
                let block = world.block_id(x, y);
                if block != cliff && EditorBlockInfo::from_def(content.block(block)).is_static {
                    world.set_block(x, y, air, 0, 0);
                }
            }
        }
        self.flush_op();
    }

    /// Scanline flood fill (`EditorTool.fill`): `replace` walks every tile,
    /// otherwise the unbuffered stack fill runs from `(x, y)`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill(
        &mut self,
        world: &mut dyn EditorGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
        replace: bool,
        tester: &dyn Fn(&dyn EditorGrid, &ContentRegistry, i32, i32) -> bool,
        setter: &mut dyn FnMut(&mut MapEditor, &mut dyn EditorGrid, i32, i32),
    ) {
        let (width, height) = (world.width(), world.height());
        if replace {
            for cx in 0..width {
                for cy in 0..height {
                    if tester(world, content, cx, cy) {
                        setter(self, world, cx, cy);
                    }
                }
            }
            return;
        }

        let mut stack: Vec<(i32, i32)> = vec![(x, y)];
        while let Some((mut x1, py)) = stack.pop() {
            if py < 0 || py >= height {
                continue;
            }
            while x1 >= 0 && tester(world, content, x1, py) {
                x1 -= 1;
            }
            x1 += 1;
            let mut span_above = false;
            let mut span_below = false;
            while x1 < width && tester(world, content, x1, py) {
                setter(self, world, x1, py);

                if !span_above && py > 0 && tester(world, content, x1, py - 1) {
                    stack.push((x1, py - 1));
                    span_above = true;
                } else if span_above && py > 0 && !tester(world, content, x1, py - 1) {
                    span_above = false;
                }

                if !span_below && py < height - 1 && tester(world, content, x1, py + 1) {
                    stack.push((x1, py + 1));
                    span_below = true;
                } else if span_below && py < height - 1 && !tester(world, content, x1, py + 1) {
                    span_below = false;
                }
                x1 += 1;
            }
        }
    }

    /// Iterates the circle-brush footprint (`MapEditor.drawCircle`).
    pub fn for_each_circle(
        &self,
        world: &dyn EditorGrid,
        x: i32,
        y: i32,
        f: &mut dyn FnMut(i32, i32),
    ) {
        let clamped = self.brush_size as i32;
        let radius = self.brush_size - 0.5 + 0.0001;
        for rx in -clamped..=clamped {
            for ry in -clamped..=clamped {
                let (fx, fy) = (rx as f32, ry as f32);
                if fx * fx + fy * fy <= radius * radius {
                    let (wx, wy) = (x + rx, y + ry);
                    if wx < 0 || wy < 0 || wx >= world.width() || wy >= world.height() {
                        continue;
                    }
                    f(wx, wy);
                }
            }
        }
    }

    /// Iterates the square-brush footprint (`MapEditor.drawSquare`).
    pub fn for_each_square(
        &self,
        world: &dyn EditorGrid,
        x: i32,
        y: i32,
        f: &mut dyn FnMut(i32, i32),
    ) {
        let clamped = self.brush_size as i32;
        for rx in -clamped..=clamped {
            for ry in -clamped..=clamped {
                let (wx, wy) = (x + rx, y + ry);
                if wx < 0 || wy < 0 || wx >= world.width() || wy >= world.height() {
                    continue;
                }
                f(wx, wy);
            }
        }
    }

    /// Applies `f` to every tile in the circle brush
    /// (`MapEditor.drawCircle(x, y, Cons)`).
    pub fn draw_circle(
        &mut self,
        world: &mut dyn EditorGrid,
        x: i32,
        y: i32,
        f: &mut dyn FnMut(&mut dyn EditorGrid, i32, i32),
    ) {
        let mut targets = std::mem::take(&mut self.draw_scratch);
        targets.clear();
        self.for_each_circle(world, x, y, &mut |tx, ty| targets.push((tx, ty)));
        for &(tx, ty) in &targets {
            f(world, tx, ty);
        }
        self.draw_scratch = targets;
    }

    /// Applies `f` to every tile in the square brush.
    pub fn draw_square(
        &mut self,
        world: &mut dyn EditorGrid,
        x: i32,
        y: i32,
        f: &mut dyn FnMut(&mut dyn EditorGrid, i32, i32),
    ) {
        let mut targets = std::mem::take(&mut self.draw_scratch);
        targets.clear();
        self.for_each_square(world, x, y, &mut |tx, ty| targets.push((tx, ty)));
        for &(tx, ty) in &targets {
            f(world, tx, ty);
        }
        self.draw_scratch = targets;
    }
}

/// `Tile.shouldSaveData`: any of floor/overlay/block saves data.
pub fn should_save_data(world: &dyn EditorGrid, content: &ContentRegistry, x: i32, y: i32) -> bool {
    let save = |id: BlockId| content.block(id).is_some_and(|def| def.save_data);
    save(world.floor_id(x, y)) || save(world.overlay_id(x, y)) || save(world.block_id(x, y))
}

/// `Geometry.d8` neighbour order.
pub const D8: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

// `block_info` is re-exported for the draw path and downstream classifiers.

#[cfg(test)]
mod tests {
    use super::*;
    use test_grid::TestGrid;

    fn stone(content: &ContentRegistry) -> BlockId {
        content.block_id("stone").unwrap()
    }

    #[test]
    fn draw_circle_and_square_footprints() {
        let mut editor = MapEditor::new();
        let world = TestGrid::new(16, 16);

        editor.brush_size = 1.5;
        let mut circle = Vec::new();
        editor.for_each_circle(&world, 8, 8, &mut |x, y| circle.push((x, y)));
        // radius = 1.5-0.5+0.0001 = 1.0001: the 4-neighbour plus centre.
        assert!(circle.contains(&(8, 8)));
        assert!(circle.contains(&(9, 8)));
        assert!(circle.contains(&(7, 8)));
        assert!(circle.contains(&(8, 9)));
        assert!(circle.contains(&(8, 7)));
        assert!(!circle.contains(&(9, 9)));
        assert_eq!(circle.len(), 5);

        let mut square = Vec::new();
        editor.for_each_square(&world, 8, 8, &mut |x, y| square.push((x, y)));
        assert_eq!(square.len(), 9);
        assert!(square.contains(&(7, 7)));
        assert!(square.contains(&(9, 9)));
    }

    #[test]
    fn draw_blocks_multiblock_clamp_overlap() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(8, 8);
        let core = content.block_id("core-shard").unwrap();
        editor.draw_block = core;

        assert!(editor.draw_info(&content).is_multiblock);
        // Clamp near the border pulls the 3x3 core inside; the center lands at
        // (1, 1) (TestGrid stores the center only).
        editor.draw_blocks(&mut world, &content, 0, 0);
        assert_eq!(world.block_id(1, 1), core);
        assert_eq!(editor.ops(), 2); // opBlock + explicit opTeam

        // A clear footprint is placeable; overlapping the existing core is not.
        assert!(!editor.has_overlap(&world, &content, 5, 5));
        assert!(editor.has_overlap(&world, &content, 2, 2));
    }

    #[test]
    fn data_op_rollback_removes_unchanged_pre_ops() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(4, 4);
        // A saveData block whose placement leaves data untouched: the floor
        // path over the same floor records no change.
        let floor = stone(&content);
        world.set_floor(1, 1, floor);
        editor.draw_block = floor;
        // Draw the floor it already is: setFloor early-outs, so the 2 data pre-ops
        // (if any) are removed. `stone` does not saveData, so no pre-ops exist.
        editor.draw_blocks(&mut world, &content, 1, 1);
        assert_eq!(editor.ops(), 0);
        editor.flush_op();
        assert!(!editor.can_undo());
    }

    #[test]
    fn fill_flood_tuning_and_replace() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(8, 8);
        let wall = content.block_id("stone-wall").unwrap();
        for x in 0..8 {
            for y in 0..8 {
                world.set_block(x, y, wall, 0, 0);
            }
        }
        // Flood replace all walls with air via a tester/setter pair.
        let tester =
            |w: &dyn EditorGrid, _c: &ContentRegistry, x: i32, y: i32| w.block_id(x, y) == wall;
        let mut count = 0;
        let mut setter = |_editor: &mut MapEditor, w: &mut dyn EditorGrid, x: i32, y: i32| {
            w.set_block(x, y, BlockId::AIR, 0, 0);
            count += 1;
        };
        editor.fill(&mut world, &content, 0, 0, true, &tester, &mut setter);
        assert_eq!(count, 64);
        assert!(world.block_id(3, 3) == BlockId::AIR);
    }

    #[test]
    fn begin_edit_size_fills_stone_floor() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(0, 0);
        editor.begin_edit_size(&mut world, &content, 6, 5);
        assert_eq!((world.width(), world.height()), (6, 5));
        assert_eq!(world.floor_id(0, 0), stone(&content));
        assert_eq!(editor.draw_block, stone(&content));
        assert!(!editor.can_undo());
    }

    /// `editor::tests::recording_suppressed` (plan 19 §7a): the `loading` gate
    /// suppresses op recording while still mutating, and `set_tile` restores it.
    #[test]
    fn recording_suppressed_while_loading() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(8, 8);
        let wall = content.block_id("stone-wall").unwrap();
        editor.draw_block = wall;

        editor.loading = true;
        editor.draw_blocks(&mut world, &content, 2, 2);
        assert_eq!(editor.ops(), 0, "recording suppressed while loading");
        assert_eq!(world.block_id(2, 2), wall, "mutation still happens");

        editor.loading = false;
        editor.draw_blocks(&mut world, &content, 3, 3);
        assert!(editor.ops() > 0, "recording resumes after loading");

        let mut op = DrawOperation::from_ops(vec![crate::editor::tile_op::TileOp::get(
            4,
            4,
            crate::editor::tile_op::OP_BLOCK,
            wall.raw() as i32,
        )]);
        op.redo(&mut world, &content);
        assert!(!world.is_loading(), "set_tile restores the loading flag");
        assert_eq!(world.block_id(4, 4), wall);
    }

    /// `editor::tests::fill_flood_tuning` (plan 19 §7a): scanline flood fill only
    /// reaches the connected region.
    #[test]
    fn fill_flood_tuning_only_reaches_connected_region() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(8, 8);
        let wall = content.block_id("stone-wall").unwrap();
        let copper = content.block_id("copper-wall").unwrap();
        for (x, y) in [(0, 0), (1, 0), (2, 0), (0, 1), (0, 2)] {
            world.set_block(x, y, wall, 0, 0);
        }
        world.set_block(6, 6, wall, 0, 0);

        editor.draw_block = copper;
        editor.tool = EditorTool::Fill;
        editor.tool_modes[EditorTool::Fill.index()] = -1;
        crate::editor::tool::touched(&mut editor, EditorTool::Fill, &mut world, &content, 0, 0);

        assert_eq!(world.block_id(0, 0), copper);
        assert_eq!(world.block_id(2, 0), copper);
        assert_eq!(world.block_id(0, 2), copper);
        assert_eq!(world.block_id(6, 6), wall, "disconnected region untouched");
        assert_eq!(world.block_id(3, 0), BlockId::AIR);
    }

    /// `editor::tests::fill_replace` (plan 19 §7a): replace-all fills the whole
    /// grid, connected or not.
    #[test]
    fn fill_replace_all_reaches_disconnected_region() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(8, 8);
        let wall = content.block_id("stone-wall").unwrap();
        let copper = content.block_id("copper-wall").unwrap();
        world.set_block(0, 0, wall, 0, 0);
        world.set_block(6, 6, wall, 0, 0);

        editor.draw_block = copper;
        editor.tool = EditorTool::Fill;
        editor.tool_modes[EditorTool::Fill.index()] = 0;
        crate::editor::tool::touched(&mut editor, EditorTool::Fill, &mut world, &content, 0, 0);

        assert_eq!(world.block_id(0, 0), copper);
        assert_eq!(
            world.block_id(6, 6),
            copper,
            "replace-all reaches all tiles"
        );
    }

    /// `editor::tests::fill_erase` (plan 19 §7a): `fillerase` removes the
    /// connected block region only.
    #[test]
    fn fill_erase_removes_connected_blocks_only() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(8, 8);
        let wall = content.block_id("stone-wall").unwrap();
        for (x, y) in [(0, 0), (1, 0), (2, 0), (0, 1)] {
            world.set_block(x, y, wall, 0, 0);
        }
        world.set_block(6, 6, wall, 0, 0);

        editor.tool = EditorTool::Fill;
        editor.tool_modes[EditorTool::Fill.index()] = 2;
        crate::editor::tool::touched(&mut editor, EditorTool::Fill, &mut world, &content, 0, 0);

        assert_eq!(world.block_id(0, 0), BlockId::AIR);
        assert_eq!(world.block_id(2, 0), BlockId::AIR);
        assert_eq!(world.block_id(6, 6), wall, "disconnected block kept");
    }

    /// `editor::tests::touched_line_bresenham` (plan 19 §3.5): the line tool
    /// paints every Bresenham point and flushes one undoable operation.
    #[test]
    fn touched_line_bresenham_flushes_one_operation() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(8, 8);
        let copper = content.block_id("copper-wall").unwrap();
        editor.draw_block = copper;
        editor.tool = EditorTool::Line;
        editor.tool_modes[EditorTool::Line.index()] = -1;

        crate::editor::tool::touched_line(
            &mut editor,
            EditorTool::Line,
            &mut world,
            &content,
            0,
            0,
            3,
            0,
        );

        for x in 0..=3 {
            assert_eq!(world.block_id(x, 0), copper, "tile ({x},0) drawn");
        }
        assert!(editor.can_undo(), "line flushed one operation");
        assert_eq!(editor.ops(), 0, "current op cleared by flush");
    }

    /// `editor::tests::add_cliffs` (plan 19 §5): an isolated static block becomes
    /// a cliff whose data byte is the 8-neighbour non-static bitmask.
    #[test]
    fn add_cliffs_autotiles_isolated_static_block() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut world = TestGrid::new(5, 5);
        let wall = content.block_id("stone-wall").unwrap();
        let cliff = content.block_id("cliff").unwrap();
        world.set_block(1, 1, wall, 0, 0);

        editor.add_cliffs(&mut world, &content);

        assert_eq!(world.block_id(1, 1), cliff);
        assert_eq!(world.tile_data(1, 1).0, 0xFFu8 as i8);
    }
}
