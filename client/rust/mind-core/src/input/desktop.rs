// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Desktop decision tree (`InputHandler` + `DesktopInput`, plan 15 §3.1/§3.5).
//!
//! Platform-neutral: the Godot key/mouse translation and cursor application live
//! in `mind-gdext::input::desktop`; this module owns the branch logic.

use smallvec::SmallVec;

use crate::world::TilePos;

use super::action::{CommandTarget, RemoteAction};
use super::caps::InputCaps;
use super::client_input::{CONTROL_GROUP_DOUBLE_TAP_MS, CONTROL_GROUPS, InputState};
use super::command_emit::ActionBatcher;
use super::line::{LineBlock, LineParams};
use super::placement::PlacementWorld;
use super::plan::ClientPlan;
use super::queue::BuildQueue;
use super::rts::{SelectRect, SelectableUnit, enemy_unit_at, select_units_rect};

/// Outcome of a wheel/scroll event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOutcome {
    /// Suppressed by focus/UI.
    Ignore,
    /// Rotated the placement by this value (`0..=3`).
    Rotate(u8),
    /// Zoom the camera.
    Zoom,
}

/// Desktop controller (`DesktopInput` port).
#[derive(Debug, Clone)]
pub struct DesktopController {
    /// Client input state.
    pub state: InputState,
    /// Build queue seam (plan 11).
    pub queue: BuildQueue,
    /// Relay batcher.
    pub batcher: ActionBatcher,
}

impl DesktopController {
    /// Creates a controller for a match.
    pub fn new(match_id: u64) -> Self {
        Self {
            state: InputState::new(),
            queue: BuildQueue::new(),
            batcher: ActionBatcher::new(match_id),
        }
    }

    /// `canDepositItem`.
    pub fn can_deposit_item(&self, cooldown: f32) -> bool {
        cooldown <= 0.0
    }

    /// `update()` zoom/rotate gating: wheel rotates a placement unless zoom is
    /// blocked by focus/chat/console/scroll (upstream `DesktopInput.update`).
    pub fn handle_scroll(&mut self, delta: i32, caps: &dyn InputCaps) -> ScrollOutcome {
        if caps.focus().blocks_zoom() {
            return ScrollOutcome::Ignore;
        }
        if self.state.is_placing() && self.state.block.is_some() {
            let rotation = (self.state.rotation as i32 + delta).rem_euclid(4) as u8;
            self.state.rotation = rotation;
            self.state.preview.rotation = rotation;
            return ScrollOutcome::Rotate(rotation);
        }
        ScrollOutcome::Zoom
    }

    /// `rotateBlock`: rotate an existing placed building.
    pub fn rotate_placed(&mut self, x: i16, y: i16, direction: bool) -> RemoteAction {
        RemoteAction::Rotate { x, y, direction }
    }

    /// Releases the current drag, committing valid plans to the build queue.
    pub fn flush(&mut self, world: &dyn PlacementWorld) -> usize {
        self.state.flush_plans(&mut self.queue, world, false)
    }

    /// Updates the drag line from `start` to `end` (desktop `updateLine`).
    pub fn drag_line(
        &mut self,
        world: &dyn PlacementWorld,
        block_meta: Option<&LineBlock>,
        start: TilePos,
        end: TilePos,
        diagonal: bool,
    ) {
        let params = LineParams {
            diagonal,
            rotation: self.state.rotation,
            override_line_rotation: self.state.override_line_rotation,
            ..LineParams::default()
        };
        self.state
            .update_line(world, block_meta, start, end, &params);
    }

    /// Right-click drag break rectangle: queues deconstruction plans and returns
    /// the `deletePlans` action for relay.
    pub fn break_rect(
        &mut self,
        world: &dyn PlacementWorld,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    ) -> SmallVec<[RemoteAction; 4]> {
        self.state.place_mode = super::place_mode::PlaceMode::Breaking;
        let positions = self.state.break_rect(world, x1, y1, x2, y2);
        for packed in &positions {
            let pos = TilePos::from_pack(*packed);
            self.queue.add_build(
                ClientPlan::break_plan(pos.x() as i32, pos.y() as i32),
                false,
            );
        }
        if positions.is_empty() {
            return SmallVec::new();
        }
        smallvec::smallvec![RemoteAction::DeletePlans { positions }]
    }

    /// `createControlGroup`: snapshots the current selection into a group.
    pub fn create_control_group(&mut self, index: usize, distinct: bool) -> bool {
        if index >= CONTROL_GROUPS || self.state.selected_units.is_empty() {
            return false;
        }
        let selected: SmallVec<[i32; 64]> = self.state.selected_units.iter().copied().collect();
        if distinct {
            for (slot, group) in self.state.control_groups.iter_mut().enumerate() {
                if slot == index {
                    continue;
                }
                group.retain(|id| !selected.contains(id));
            }
        }
        self.state.control_groups[index] = selected;
        self.state.last_ctrl_group = index;
        true
    }

    /// `recallControlGroup`: selects the group; returns `true` on a double-tap
    /// (`< 400 ms`) which centers the camera on the group.
    pub fn recall_control_group(&mut self, index: usize, now_ms: u64) -> bool {
        if index >= CONTROL_GROUPS {
            return false;
        }
        let double_tap = index == self.state.last_ctrl_group
            && now_ms.saturating_sub(self.state.last_ctrl_group_tap_ms)
                < CONTROL_GROUP_DOUBLE_TAP_MS;
        self.state.selected_units = self.state.control_groups[index].iter().copied().collect();
        self.state.last_ctrl_group = index;
        self.state.last_ctrl_group_tap_ms = now_ms;
        self.state.clock_ms = now_ms;
        double_tap
    }

    /// Drag-rect unit selection (`selectUnitsRect`).
    pub fn select_units(&mut self, units: &[SelectableUnit], player_team: u8, rect: SelectRect) {
        select_units_rect(units, player_team, rect, &mut self.state.selected_units);
        self.state.command_rect = Some((rect.x, rect.y, rect.w, rect.h));
        self.state.tapped_one = self.state.selected_units.len() == 1;
    }

    /// `commandTap`: resolves target and builds the `commandUnits` action.
    pub fn command_tap(
        &mut self,
        units: &[SelectableUnit],
        player_team: u8,
        x: f32,
        y: f32,
        queue: bool,
    ) -> Option<RemoteAction> {
        if self.state.selected_units.is_empty() {
            return None;
        }
        let target = if let Some(enemy) = enemy_unit_at(units, player_team, x, y, 11.0) {
            CommandTarget::Unit(enemy)
        } else {
            CommandTarget::Position { x, y }
        };
        Some(RemoteAction::CommandUnits {
            units: self.state.selected_units.iter().copied().collect(),
            target,
            queue,
            final_batch: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::input::caps::TestCaps;
    use crate::input::focus::FocusState;

    #[test]
    fn deposit_cooldown_gate() {
        // ApplicationTests.inventoryDeposit (input half).
        let controller = DesktopController::new(1);
        assert!(controller.can_deposit_item(0.0));
        assert!(controller.can_deposit_item(-1.0));
        assert!(!controller.can_deposit_item(0.5));
    }

    #[test]
    fn zoom_gating_rotates_then_zooms() {
        let mut controller = DesktopController::new(1);
        controller.state.select_block(Some(BlockId::STONE_WALL));
        controller.state.begin_place();
        let caps = TestCaps::default();
        assert_eq!(controller.handle_scroll(1, &caps), ScrollOutcome::Rotate(2));
        assert_eq!(controller.handle_scroll(1, &caps), ScrollOutcome::Rotate(3));

        // Chat/scroll focus blocks zoom and rotation.
        let blocked = TestCaps {
            focus: FocusState {
                has_scroll: true,
                ..FocusState::default()
            },
            ..TestCaps::default()
        };
        assert_eq!(controller.handle_scroll(1, &blocked), ScrollOutcome::Ignore);

        // No placement selected -> zoom.
        controller.state.select_block(None);
        assert_eq!(controller.handle_scroll(1, &caps), ScrollOutcome::Zoom);
    }
}
