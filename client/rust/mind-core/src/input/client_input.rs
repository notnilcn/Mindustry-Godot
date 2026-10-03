// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `InputState` — the `InputHandler` state (plan 15 §3.5).
//!
//! Client-local only (deviation I2/I3): never ECS data, never checksummed. The
//! platform-neutral decision trees in `desktop`/`mobile` mutate this state and
//! produce `RemoteAction`s.

use smallvec::SmallVec;

use crate::content::BlockId;
use crate::world::TilePos;

use super::line::{LineBlock, LineParams, PlaceLine, iterate_line};
use super::place_mode::PlaceMode;
use super::placement::PlacementWorld;
use super::plan::{ClientPlan, PlanMirror, PreviewState};
use super::queue::BuildQueue;

/// Number of RTS control groups (`InputHandler.controlGroups`).
pub const CONTROL_GROUPS: usize = 10;
/// Double-tap window for control-group centering (`400 ms`).
pub const CONTROL_GROUP_DOUBLE_TAP_MS: u64 = 400;
/// `InputHandler.controlInterval` (70 ticks).
pub const CONTROL_INTERVAL_TICKS: u32 = 70;

/// The full client input state (`InputHandler` fields, §3.5).
#[derive(Debug, Clone)]
pub struct InputState {
    /// Selected block (`input.block`).
    pub block: Option<BlockId>,
    /// Placement rotation (`input.rotation`, default 1).
    pub rotation: u8,
    /// Active interaction.
    pub place_mode: PlaceMode,
    /// Line/area plans being dragged.
    pub line_plans: Vec<ClientPlan>,
    /// Schematic/select plans.
    pub select_plans: Vec<ClientPlan>,
    /// Mirror of `player.unit().plans`.
    pub last_plans: PlanMirror,
    /// `overrideLineRotation`.
    pub override_line_rotation: bool,
    /// Resolved drag steps (`PlaceLine`).
    pub line: Vec<PlaceLine>,
    /// Selected RTS units.
    pub selected_units: SmallVec<[i32; 128]>,
    /// Selected command buildings.
    pub command_buildings: SmallVec<[TilePos; 32]>,
    /// Named control groups.
    pub control_groups: [SmallVec<[i32; 64]>; CONTROL_GROUPS],
    /// Last recalled control group.
    pub last_ctrl_group: usize,
    /// Timestamp of the last control-group tap (monotonic ms).
    pub last_ctrl_group_tap_ms: u64,
    /// Command mode active.
    pub command_mode: bool,
    /// Queue command mode.
    pub queue_mode: bool,
    /// Whether the builder unit is active (`isBuilding`).
    pub is_building: bool,
    /// Schematic select mode (`f`).
    pub schematic_select: bool,
    /// Rebuild-select mode (`b`).
    pub rebuild_select: bool,
    /// Plan move (`splan`).
    pub splan: bool,
    /// `last_schematic` presence.
    pub has_schematic: bool,
    /// Command drag rectangle `(x, y, w, h)`.
    pub command_rect: Option<(f32, f32, f32, f32)>,
    /// `tappedOne` (single-unit tap selection).
    pub tapped_one: bool,
    /// Preview state handed to plan 16.
    pub preview: PreviewState,
    /// Monotonic render clock in milliseconds (client-local; not sim time).
    pub clock_ms: u64,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    /// Upstream defaults (`rotation = 1`, `isBuilding = true`).
    pub fn new() -> Self {
        Self {
            block: None,
            rotation: 1,
            place_mode: PlaceMode::None,
            line_plans: Vec::new(),
            select_plans: Vec::new(),
            last_plans: PlanMirror::new(),
            override_line_rotation: false,
            line: Vec::new(),
            selected_units: SmallVec::new(),
            command_buildings: SmallVec::new(),
            control_groups: std::array::from_fn(|_| SmallVec::new()),
            last_ctrl_group: 0,
            last_ctrl_group_tap_ms: 0,
            command_mode: false,
            queue_mode: false,
            is_building: true,
            schematic_select: false,
            rebuild_select: false,
            splan: false,
            has_schematic: false,
            command_rect: None,
            tapped_one: false,
            preview: PreviewState::default(),
            clock_ms: 0,
        }
    }

    /// Selected block id.
    pub fn selected_block(&self) -> Option<BlockId> {
        self.block
    }

    /// Selects a block (`input.block`), keeping the current rotation.
    pub fn select_block(&mut self, block: Option<BlockId>) {
        self.block = block;
        if block.is_none() {
            self.line_plans.clear();
            self.place_mode = PlaceMode::None;
        }
    }

    /// `begin_place`.
    pub fn begin_place(&mut self) {
        self.place_mode = PlaceMode::Placing;
        self.splan = false;
    }

    /// `begin_break`.
    pub fn begin_break(&mut self) {
        self.place_mode = PlaceMode::Breaking;
    }

    /// Whether currently placing.
    pub fn is_placing(&self) -> bool {
        self.place_mode.is_placing()
    }

    /// Whether currently breaking.
    pub fn is_breaking(&self) -> bool {
        self.place_mode.is_breaking()
    }

    /// Whether rebuilding derelicts.
    pub fn is_rebuild_selecting(&self) -> bool {
        self.rebuild_select || self.place_mode.is_rebuild_selecting()
    }

    /// `updateState()` transient clearing (menu/editor branch).
    pub fn reset(&mut self) {
        self.command_buildings.clear();
        self.selected_units.clear();
        for group in &mut self.control_groups {
            group.clear();
        }
        self.line_plans.clear();
        self.select_plans.clear();
        self.line.clear();
        self.command_mode = false;
        self.queue_mode = false;
        self.place_mode = PlaceMode::None;
        self.schematic_select = false;
        self.rebuild_select = false;
        self.splan = false;
        self.command_rect = None;
        self.tapped_one = false;
        self.last_plans.clear();
        self.preview.clear();
    }

    /// `updateLine`: rebuild `line_plans` for a drag.
    #[allow(clippy::too_many_arguments)]
    pub fn update_line(
        &mut self,
        world: &dyn PlacementWorld,
        block_meta: Option<&LineBlock>,
        start: TilePos,
        end: TilePos,
        params: &LineParams,
    ) {
        iterate_line(
            world,
            block_meta,
            params,
            start,
            end,
            &super::line::NoLineHooks,
            &mut self.line,
        );
        let block = self.block.unwrap_or(BlockId::AIR);
        self.line_plans.clear();
        self.line_plans.extend(
            self.line
                .iter()
                .map(|step| ClientPlan::place(step.x, step.y, step.rotation, block)),
        );
        self.place_mode = PlaceMode::Placing;
        self.preview.block = self.block;
        self.preview.rotation = params.rotation;
        self.preview.place_mode = self.place_mode;
        self.preview.line_plans.clear();
        self.preview.line_plans.extend_from_slice(&self.line_plans);
        self.preview.valid = !self.line_plans.is_empty();
    }

    /// Rebuilds [`Self::preview`] in place from the current state, reusing the
    /// existing allocations (plan 15 M6: zero steady-state preview allocations).
    pub fn refresh_preview(&mut self) {
        self.preview.block = self.block;
        self.preview.rotation = self.rotation;
        self.preview.place_mode = self.place_mode;
        self.preview.valid = !self.line_plans.is_empty();
        self.preview.splan = self.splan;
        self.preview.command_rect = self.command_rect;
        self.preview.line_plans.clear();
        self.preview.line_plans.extend_from_slice(&self.line_plans);
        self.preview.select_plans.clear();
        self.preview
            .select_plans
            .extend_from_slice(&self.select_plans);
        self.preview.selected_units.clear();
        self.preview
            .selected_units
            .extend_from_slice(self.selected_units.as_slice());
        self.preview.command_buildings.clear();
        self.preview
            .command_buildings
            .extend(self.command_buildings.iter().map(|pos| (pos.x(), pos.y())));
        self.preview.cached_valid.clear();
        self.preview
            .cached_valid
            .extend(self.line_plans.iter().map(|plan| !plan.breaking));
    }

    /// `flushPlans`: valid line plans are copied into the build queue.
    ///
    /// Returns the number of plans committed. Writes only through
    /// [`BuildQueue`] (the plan-11 seam); no direct sim mutation.
    pub fn flush_plans(
        &mut self,
        queue: &mut BuildQueue,
        world: &dyn PlacementWorld,
        boost: bool,
    ) -> usize {
        let plans = self.line_plans.clone();
        let mut committed = 0usize;
        for plan in &plans {
            if plan.breaking || plan.block == BlockId::AIR {
                continue;
            }
            if world.valid_place(plan.block, plan.x, plan.y, plan.rotation) {
                let copy = plan.clone();
                queue.add_build(copy, boost);
                committed += 1;
            }
        }
        self.line_plans.clear();
        self.line.clear();
        self.place_mode = PlaceMode::None;
        self.preview.clear();
        committed
    }

    /// `removeSelection`/break-rect: accumulate deconstruction positions.
    pub fn break_rect(
        &mut self,
        world: &dyn PlacementWorld,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    ) -> SmallVec<[i32; 32]> {
        let (min_x, max_x) = (x1.min(x2), x1.max(x2));
        let (min_y, max_y) = (y1.min(y2), y1.max(y2));
        let mut positions: SmallVec<[i32; 32]> = SmallVec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if world.block_at(x, y) == BlockId::AIR {
                    continue;
                }
                positions.push(TilePos::new(x as i16, y as i16).pack());
            }
        }
        positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::placement::PlacementWorld;

    struct FlatWorld;
    impl PlacementWorld for FlatWorld {
        fn in_bounds(&self, _x: i32, _y: i32) -> bool {
            true
        }
        fn block_at(&self, _x: i32, _y: i32) -> BlockId {
            BlockId::AIR
        }
        fn floor_deep(&self, _x: i32, _y: i32) -> bool {
            false
        }
        fn always_replace(&self, _x: i32, _y: i32) -> bool {
            true
        }
        fn can_replace(&self, _t: BlockId, _o: BlockId) -> bool {
            true
        }
        fn valid_place(&self, _b: BlockId, _x: i32, _y: i32, _r: u8) -> bool {
            true
        }
    }

    #[test]
    fn defaults_match_input_handler() {
        let state = InputState::new();
        assert_eq!(state.rotation, 1);
        assert!(state.is_building);
        assert_eq!(state.control_groups.len(), CONTROL_GROUPS);
        assert!(!state.is_placing());
    }

    #[test]
    fn update_line_then_flush_commits() {
        let world = FlatWorld;
        let mut state = InputState::new();
        state.select_block(Some(BlockId::STONE_WALL));
        state.update_line(
            &world,
            None,
            TilePos::new(0, 0),
            TilePos::new(4, 0),
            &LineParams {
                rotation: 1,
                ..LineParams::default()
            },
        );
        assert_eq!(state.line_plans.len(), 5);
        let mut queue = BuildQueue::new();
        let committed = state.flush_plans(&mut queue, &world, false);
        assert_eq!(committed, 5);
        assert_eq!(queue.len(), 5);
        assert!(state.line_plans.is_empty());
    }

    #[test]
    fn refresh_preview_reuses_capacity() {
        let world = FlatWorld;
        let mut state = InputState::new();
        state.select_block(Some(BlockId::STONE_WALL));
        state.update_line(
            &world,
            None,
            TilePos::new(0, 0),
            TilePos::new(4, 0),
            &LineParams {
                rotation: 1,
                ..LineParams::default()
            },
        );
        state.refresh_preview();
        let capacity = state.preview.line_plans.capacity();
        assert!(capacity >= 5);
        // A shorter line must not shrink (and therefore not reallocate) the vec.
        state.update_line(
            &world,
            None,
            TilePos::new(0, 0),
            TilePos::new(1, 0),
            &LineParams {
                rotation: 1,
                ..LineParams::default()
            },
        );
        state.refresh_preview();
        assert_eq!(state.preview.line_plans.capacity(), capacity);
        assert_eq!(state.preview.line_plans.len(), 2);
        assert_eq!(state.preview.cached_valid, vec![true, true]);
    }
}
