// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic input replay harness + desktop flow tests (plan 15 §7a).
//!
//! The JSONL [`super::InputLog`] is the canonical replay format; these focused
//! tests exercise the desktop decision tree directly (the same path the log
//! drives) so they run without Godot.

use std::collections::HashSet;

use smallvec::SmallVec;

use crate::content::BlockId;
use crate::world::TilePos;

use super::caps::TestCaps;
use super::desktop::DesktopController;
use super::input_log::{InputLog, InputRecord, RawEvent};
use super::placement::PlacementWorld;
#[cfg(test)]
use super::rts::{SelectRect, SelectableUnit};

/// A grid world with occupied tiles for replay tests.
#[derive(Debug, Clone, Default)]
pub struct ReplayWorld {
    occupied: HashSet<i32>,
}

impl ReplayWorld {
    /// Empty world.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an occupied tile.
    pub fn place(&mut self, x: i32, y: i32, block: BlockId) {
        if block != BlockId::AIR {
            self.occupied
                .insert(TilePos::new(x as i16, y as i16).pack());
        }
    }
}

impl PlacementWorld for ReplayWorld {
    fn in_bounds(&self, _x: i32, _y: i32) -> bool {
        true
    }

    fn block_at(&self, x: i32, y: i32) -> BlockId {
        if self
            .occupied
            .contains(&TilePos::new(x as i16, y as i16).pack())
        {
            BlockId::STONE_WALL
        } else {
            BlockId::AIR
        }
    }

    fn floor_deep(&self, _x: i32, _y: i32) -> bool {
        false
    }

    fn always_replace(&self, x: i32, y: i32) -> bool {
        self.block_at(x, y) == BlockId::AIR
    }

    fn can_replace(&self, _target: BlockId, other: BlockId) -> bool {
        other == BlockId::AIR
    }

    fn valid_place(&self, _block: BlockId, _x: i32, _y: i32, _rotation: u8) -> bool {
        true
    }
}

/// Deterministic replay driver over a controller.
#[derive(Debug, Clone)]
pub struct ReplayHarness {
    /// Desktop controller under test.
    pub controller: DesktopController,
    /// Cursor tile (from the last `MouseMove`/button event).
    pub cursor: (f32, f32),
    /// Commands emitted by the run.
    pub emitted: SmallVec<[super::action::RemoteAction; 8]>,
}

impl ReplayHarness {
    /// Creates a harness.
    pub fn new(match_id: u64) -> Self {
        Self {
            controller: DesktopController::new(match_id),
            cursor: (0.0, 0.0),
            emitted: SmallVec::new(),
        }
    }

    /// Applies one raw event. Only the event subset used by the replay tests is
    /// interpreted; the full desktop tree lives in `DesktopController`.
    pub fn step(&mut self, event: &RawEvent, world: &dyn PlacementWorld, caps: &TestCaps) {
        match event {
            RawEvent::MouseMove { x, y } => self.cursor = (*x, *y),
            RawEvent::Scroll { y, .. } => {
                let _ = self.controller.handle_scroll(*y as i32, caps);
            }
            _ => {
                let _ = world;
            }
        }
    }

    /// Runs a whole log and returns the number of interpreted events.
    pub fn run(&mut self, log: &InputLog, world: &dyn PlacementWorld, caps: &TestCaps) -> usize {
        for InputRecord { ev, .. } in &log.records {
            self.step(ev, world, caps);
        }
        log.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_units() -> Vec<SelectableUnit> {
        (0..6)
            .map(|id| SelectableUnit {
                id,
                type_id: 0,
                x: 10.0 + id as f32,
                y: 10.0,
                team: 0,
                commandable: true,
            })
            .collect()
    }

    fn caps() -> TestCaps {
        TestCaps::default()
    }

    #[test]
    fn desktop_place_line() {
        let world = ReplayWorld::new();
        let mut controller = DesktopController::new(1);
        controller.state.select_block(Some(BlockId::STONE_WALL));
        controller.drag_line(&world, None, TilePos::new(2, 2), TilePos::new(6, 2), false);
        assert_eq!(controller.state.line_plans.len(), 5);
        assert!(controller.state.is_placing());
        let committed = controller.flush(&world);
        assert_eq!(committed, 5);
        assert_eq!(controller.queue.len(), 5);
        assert!(controller.queue.iter().all(|plan| !plan.breaking));
    }

    #[test]
    fn desktop_break_rect() {
        let mut world = ReplayWorld::new();
        for x in 0..4 {
            for y in 0..4 {
                world.place(x, y, BlockId::STONE_WALL);
            }
        }
        let mut controller = DesktopController::new(1);
        let actions = controller.break_rect(&world, 0, 0, 3, 3);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            super::super::action::RemoteAction::DeletePlans { positions } => {
                assert_eq!(positions.len(), 16);
            }
            other => panic!("expected DeletePlans, got {other:?}"),
        }
        assert_eq!(controller.queue.len(), 16);
        assert!(controller.queue.iter().all(|plan| plan.breaking));
    }

    #[test]
    fn desktop_control_groups() {
        let mut controller = DesktopController::new(1);
        controller.state.selected_units = smallvec::smallvec![1, 2, 3];
        assert!(controller.create_control_group(0, true));
        controller.state.selected_units = smallvec::smallvec![4];
        assert!(controller.create_control_group(1, true));
        // Recall group 0 restores [1,2,3]; first recall is not a double-tap.
        assert!(!controller.recall_control_group(0, 1000));
        assert_eq!(controller.state.selected_units.as_slice(), &[1, 2, 3]);
        // Double-tap within 400 ms centers the camera.
        assert!(controller.recall_control_group(0, 1300));
        assert!(!controller.recall_control_group(0, 2000));
    }

    #[test]
    fn desktop_command_rect() {
        let units = flat_units();
        let mut controller = DesktopController::new(1);
        controller.select_units(
            &units,
            0,
            SelectRect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        );
        assert_eq!(controller.state.selected_units.len(), 6);
        assert_eq!(
            controller.state.command_rect,
            Some((0.0, 0.0, 100.0, 100.0))
        );
        let action = controller
            .command_tap(&units, 0, 60.0, 60.0, false)
            .expect("command");
        match action {
            super::super::action::RemoteAction::CommandUnits {
                units,
                target,
                final_batch,
                ..
            } => {
                assert_eq!(units.len(), 6);
                assert!(final_batch);
                assert!(matches!(
                    target,
                    super::super::action::CommandTarget::Position { .. }
                ));
            }
            other => panic!("expected CommandUnits, got {other:?}"),
        }
    }

    #[test]
    fn desktop_replay_harness_runs_log() {
        let mut log = InputLog::new(super::super::input_log::InputHeader::default());
        log.push(0, RawEvent::MouseMove { x: 1.0, y: 2.0 });
        log.push(1, RawEvent::Scroll { x: 0.0, y: 1.0 });
        let world = ReplayWorld::new();
        let mut harness = ReplayHarness::new(1);
        assert_eq!(harness.run(&log, &world, &caps()), 2);
        assert_eq!(harness.cursor, (1.0, 2.0));
    }
}
