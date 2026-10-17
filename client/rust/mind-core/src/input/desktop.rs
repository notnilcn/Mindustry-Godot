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

    /// `canDepositItem` (`InputHandler.java:2310-2316`): a block with
    /// `depositCooldown >= 0` replaces the global
    /// `Rules.itemDepositCooldown`, gating on
    /// `itemDepositCooldown - rules.itemDepositCooldown <= -depositCooldown`.
    ///
    /// `cooldown` is the live `itemDepositCooldown` timer (reset to the rules
    /// value on every deposit); `block_cooldown` is the hovered block's
    /// `Block.depositCooldown` (`-1` = global gate).
    pub fn can_deposit_item(
        &self,
        cooldown: f32,
        block_cooldown: f32,
        rules_cooldown: f32,
    ) -> bool {
        if block_cooldown >= 0.0 {
            cooldown - rules_cooldown <= -block_cooldown
        } else {
            cooldown <= 0.0
        }
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

    /// `commandTap` (`InputHandler.java:1165-1201`): resolves the target and
    /// builds the `commandUnits` / `commandBuilding` actions.
    ///
    /// `enemy_building` is the tile under the cursor when it holds another
    /// team's building (`attack instanceof Building`); Java keeps a derelict
    /// building (`team == derelict != player.team()`) as the target too.
    pub fn command_tap(
        &mut self,
        units: &[SelectableUnit],
        player_team: u8,
        x: f32,
        y: f32,
        queue: bool,
        enemy_building: Option<TilePos>,
    ) -> SmallVec<[RemoteAction; 2]> {
        let mut actions: SmallVec<[RemoteAction; 2]> = SmallVec::new();
        if !self.state.selected_units.is_empty() {
            let target = if let Some(pos) = enemy_building {
                CommandTarget::Building(pos)
            } else if let Some(enemy) = enemy_unit_at(units, player_team, x, y, 11.0) {
                CommandTarget::Unit(enemy)
            } else {
                CommandTarget::Position { x, y }
            };
            actions.push(RemoteAction::CommandUnits {
                units: self.state.selected_units.iter().copied().collect(),
                target,
                queue,
                final_batch: true,
            });
        }
        if !self.state.command_buildings.is_empty() {
            actions.push(RemoteAction::CommandBuilding {
                positions: self
                    .state
                    .command_buildings
                    .iter()
                    .map(|pos| pos.pack())
                    .collect(),
                x,
                y,
            });
        }
        actions
    }
}

/// `DesktopInput` wheel context for [`route_scroll`]
/// (`DesktopInput.java:475-480`, `843-858`, `924-926`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollCtx {
    /// `Binding.diagonalPlacement` held (zoom override, rotation gate).
    pub diagonal_held: bool,
    /// `Binding.rotateplaced` held (rotates the building under the cursor).
    pub rotate_placed_held: bool,
    /// `Binding.zoom.value.equals(Binding.rotate.value)` (both default to the
    /// wheel).
    pub zoom_is_rotate: bool,
    /// `player.isBuilder() && isPlacing()`.
    pub placing: bool,
    /// The selected block rotates (`block.rotate`).
    pub block_rotates: bool,
    /// `!selectPlans.isEmpty()`.
    pub select_plans: bool,
    /// The building under the cursor is interactable and
    /// `block.rotate && block.quickRotate`.
    pub cursor_build_quick_rotates: bool,
}

/// Which wheel behaviours run for one step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollRoute {
    /// `renderer.scaleCamera(axisTap(zoom))`.
    pub zoom: bool,
    /// Rotate the pending placement or the copied select plans.
    pub rotate_selection: bool,
    /// `Call.rotateBlock` on the building under the cursor.
    pub rotate_placed: bool,
}

/// `DesktopInput.java:608-610`: `keyTap(deselect) && !ui.minimapfrag.shown() &&
/// !isPlacing() && player.unit().plans.isEmpty() && !commandMode` clears
/// `player.unit().mineTile`.
pub fn deselect_stops_mining(block_selected: bool, command_mode: bool, plans: usize) -> bool {
    !block_selected && !command_mode && plans == 0
}

/// Routes one wheel step exactly like `DesktopInput.update`/`pollInputPlayer`
/// (Java: zoom needs `!keyDown(rotatePlaced)` and either the diagonal key, a
/// distinct zoom binding, or a non-rotatable/non-placing cursor with no select
/// plans; rotation is gated only on the diagonal key).
pub fn route_scroll(ctx: &ScrollCtx) -> ScrollRoute {
    ScrollRoute {
        zoom: !ctx.rotate_placed_held
            && (ctx.diagonal_held
                || !ctx.zoom_is_rotate
                || ((!ctx.placing || !ctx.block_rotates) && !ctx.select_plans)),
        rotate_selection: !ctx.diagonal_held,
        rotate_placed: !ctx.placing && ctx.rotate_placed_held && ctx.cursor_build_quick_rotates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::input::caps::TestCaps;
    use crate::input::focus::FocusState;

    #[test]
    fn scroll_routing_matches_java() {
        // (a) placing a rotatable block: rotate, no zoom.
        let conveyor = ScrollCtx {
            zoom_is_rotate: true,
            placing: true,
            block_rotates: true,
            ..ScrollCtx::default()
        };
        let route = route_scroll(&conveyor);
        assert!(route.rotate_selection);
        assert!(!route.zoom);

        // (b) placing a non-rotating block (drill): zoom + rotate.
        let drill = ScrollCtx {
            block_rotates: false,
            ..conveyor
        };
        let route = route_scroll(&drill);
        assert!(route.rotate_selection);
        assert!(route.zoom, "non-rotating placement does not block zoom");

        // (c) diagonal held while placing: zoom only.
        let diagonal = ScrollCtx {
            diagonal_held: true,
            ..conveyor
        };
        let route = route_scroll(&diagonal);
        assert!(route.zoom);
        assert!(!route.rotate_selection);

        // (d) R held over a quick-rotatable build, nothing selected: rotate the
        // placed block and suppress zoom.
        let placed = ScrollCtx {
            rotate_placed_held: true,
            cursor_build_quick_rotates: true,
            ..ScrollCtx::default()
        };
        let route = route_scroll(&placed);
        assert!(route.rotate_placed);
        assert!(!route.zoom);

        // (e) nothing selected: zoom.
        let route = route_scroll(&ScrollCtx::default());
        assert!(route.zoom);
        assert!(route.rotate_selection);
        assert!(!route.rotate_placed);
    }

    #[test]
    fn deposit_cooldown_gate() {
        // ApplicationTests.inventoryDeposit (input half).
        let controller = DesktopController::new(1);
        // No block gate: the global `Rules.itemDepositCooldown` path
        // (`itemDepositCooldown <= 0`).
        assert!(controller.can_deposit_item(0.0, -1.0, 0.5));
        assert!(controller.can_deposit_item(-1.0, -1.0, 0.5));
        assert!(!controller.can_deposit_item(0.5, -1.0, 0.5));

        // J04 T5 (`InputHandler.java:2312-2313`): a block with
        // `depositCooldown = 2.0` (duo) needs the timer to fall 2.0 below the
        // rules value before the next deposit.
        let rules = 0.5;
        assert!(
            !controller.can_deposit_item(0.5, 2.0, rules),
            "fresh timer blocks"
        );
        assert!(
            !controller.can_deposit_item(-1.0, 2.0, rules),
            "1.5 below the rules value is not enough"
        );
        assert!(
            controller.can_deposit_item(-1.5, 2.0, rules),
            "2.0 below the rules value passes"
        );
        // `depositCooldown = 0.5` (scatter) gates at `rules - 0.5` (0.0 here).
        assert!(
            controller.can_deposit_item(0.0, 0.5, rules),
            "at the threshold"
        );
        assert!(!controller.can_deposit_item(0.1, 0.5, rules));
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

    #[test]
    fn deselect_stops_mining_cases() {
        // `DesktopInput.java:608-610`: the deselect tap clears `mineTile` only
        // with no selected block, no command mode and no queued unit plans.
        assert!(deselect_stops_mining(false, false, 0));
        assert!(!deselect_stops_mining(true, false, 0), "block selected");
        assert!(!deselect_stops_mining(false, true, 0), "command mode");
        assert!(!deselect_stops_mining(false, false, 1), "queued plans");
    }

    fn selectable(id: i32, team: u8, x: f32, y: f32) -> SelectableUnit {
        SelectableUnit {
            id,
            type_id: 0,
            x,
            y,
            team,
            commandable: false,
            hit_size: 8.0,
            select_global: true,
            payload: false,
        }
    }

    #[test]
    fn command_tap_targets_enemy_building() {
        // `InputHandler.java:1165-1171`: the building under the cursor wins
        // when its team is not the player's, even over an enemy unit.
        let mut controller = DesktopController::new(1);
        controller.state.selected_units.push(7);
        let units = [selectable(9, 2, 100.0, 100.0)];
        let actions =
            controller.command_tap(&units, 1, 100.0, 100.0, false, Some(TilePos::new(140, 62)));
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            RemoteAction::CommandUnits { target, .. } => {
                assert_eq!(*target, CommandTarget::Building(TilePos::new(140, 62)));
            }
            other => panic!("expected CommandUnits, got {other:?}"),
        }
    }

    #[test]
    fn command_tap_commands_buildings() {
        // `InputHandler.java:1200-1201`: selected commandable buildings get a
        // `commandBuilding` rally point even with no unit selection.
        let mut controller = DesktopController::new(1);
        controller
            .state
            .command_buildings
            .push(TilePos::new(10, 12));
        let actions = controller.command_tap(&[], 1, 50.0, 60.0, false, None);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            RemoteAction::CommandBuilding { positions, x, y } => {
                assert_eq!(positions.as_slice(), &[TilePos::new(10, 12).pack()]);
                assert_eq!((*x, *y), (50.0, 60.0));
            }
            other => panic!("expected CommandBuilding, got {other:?}"),
        }
    }
}
