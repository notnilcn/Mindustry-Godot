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

/// Deterministic replay driver over the mobile controller (plan 15 §7a).
#[derive(Debug, Clone)]
pub struct MobileReplayHarness {
    /// Mobile controller under test.
    pub controller: super::mobile::MobileController,
    /// Cursor in world pixels (from the last touch).
    pub cursor: (f32, f32),
    /// Monotonic replay clock in seconds (tick / 60).
    pub time: f64,
    /// Actions emitted by the run.
    pub actions: SmallVec<[super::action::RemoteAction; 16]>,
}

impl Default for MobileReplayHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl MobileReplayHarness {
    /// Creates a harness.
    pub fn new() -> Self {
        Self {
            controller: super::mobile::MobileController::new(),
            cursor: (0.0, 0.0),
            time: 0.0,
            actions: SmallVec::new(),
        }
    }

    /// Applies one raw event at `tick`, returning the emitted actions.
    pub fn step(
        &mut self,
        tick: u64,
        event: &RawEvent,
        world: &dyn PlacementWorld,
        block: Option<&super::line::LineBlock>,
        caps: &super::caps::TestCaps,
    ) -> SmallVec<[super::action::RemoteAction; 4]> {
        self.time = tick as f64 / 60.0;
        let mut emitted = SmallVec::new();
        match event {
            RawEvent::TouchDown { pointer, x, y } => {
                self.cursor = (*x, *y);
                let gestures = self
                    .controller
                    .detector
                    .touch_down(self.time, *x, *y, *pointer);
                for gesture in gestures {
                    emitted.extend(self.controller.handle_gesture(gesture, world, block, caps));
                }
            }
            RawEvent::TouchMove { pointer, x, y } => {
                self.cursor = (*x, *y);
                let gestures = self
                    .controller
                    .detector
                    .touch_dragged(self.time, *x, *y, *pointer);
                for gesture in gestures {
                    emitted.extend(self.controller.handle_gesture(gesture, world, block, caps));
                }
                // While in line mode the mobile update loop re-derives the drag
                // every frame (`MobileInput.update` lineMode branch).
                if self.controller.mode.line_mode {
                    let tile = crate::world::TilePos::new(
                        (*x / crate::config::TILESIZE as f32).floor() as i16,
                        (*y / crate::config::TILESIZE as f32).floor() as i16,
                    );
                    self.controller.drag_to(world, block, tile);
                }
            }
            RawEvent::TouchUp { pointer, x, y } => {
                self.cursor = (*x, *y);
                let gestures = self
                    .controller
                    .detector
                    .touch_up(self.time, *x, *y, *pointer);
                for gesture in gestures {
                    emitted.extend(self.controller.handle_gesture(gesture, world, block, caps));
                }
                if self.controller.mode.line_mode {
                    if self.controller.state.place_mode.is_placing() {
                        self.controller.confirm_line(world);
                    } else if self.controller.state.place_mode.is_breaking() {
                        if let Some(start) = self.controller.line_start {
                            let (tx, ty) = (
                                (*x / crate::config::TILESIZE as f32).floor() as i32,
                                (*y / crate::config::TILESIZE as f32).floor() as i32,
                            );
                            self.controller
                                .state
                                .break_rect(world, start.x() as i32, start.y() as i32, tx, ty);
                        }
                    }
                    self.controller.mode.line_mode = false;
                }
            }
            RawEvent::Magnify { factor } => {
                let base = if self.controller.last_zoom < 0.0 {
                    4.0
                } else {
                    self.controller.last_zoom
                };
                self.controller.zoom(1.0, *factor, base);
            }
            RawEvent::Action { action, value } => match action.as_str() {
                "tick" => {
                    if let Some(long) = self.controller.detector.update(self.time) {
                        emitted.extend(self.controller.handle_gesture(long, world, block, caps));
                    }
                }
                "confirm" => {
                    self.controller.confirm_plans(world);
                }
                "rotate" => {
                    self.controller.state.rotation =
                        (self.controller.state.rotation as i32 + *value as i32).rem_euclid(4) as u8;
                }
                _ => {}
            },
            _ => {}
        }
        self.actions.extend(emitted.iter().cloned());
        emitted
    }

    /// Runs a whole log and returns the number of interpreted events.
    pub fn run(
        &mut self,
        log: &InputLog,
        world: &dyn PlacementWorld,
        block: Option<&super::line::LineBlock>,
        caps: &super::caps::TestCaps,
    ) -> usize {
        for InputRecord { tick, ev } in &log.records {
            self.step(*tick, ev, world, block, caps);
        }
        log.records.len()
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
    use crate::input::mobile::{GestureDetector, GestureEvent, MobileController, PayloadTarget};

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
    fn mobile_longpress_line() {
        let mut detector = GestureDetector::new();
        detector.touch_down(0.0, 24.0, 24.0, 0);
        let event = detector.update(0.31).expect("long press");
        let world = ReplayWorld::new();
        let mut controller = MobileController::new();
        controller.state.select_block(Some(BlockId::STONE_WALL));
        controller.state.begin_place();
        controller.handle_gesture(event, &world, None, &caps());
        assert!(controller.mode.line_mode);
        assert_eq!(controller.line_start, Some(TilePos::new(3, 3)));
    }

    #[test]
    fn mobile_confirm_commit() {
        let world = ReplayWorld::new();
        let mut controller = MobileController::new();
        controller.add_select_plan(super::super::plan::ClientPlan::place(
            1,
            1,
            0,
            BlockId::STONE_WALL,
        ));
        controller.add_select_plan(super::super::plan::ClientPlan::place(
            2,
            1,
            0,
            BlockId::STONE_WALL,
        ));
        assert_eq!(controller.confirm_plans(&world), 2);
        assert_eq!(controller.queue.len(), 2);
    }

    #[test]
    fn mobile_pan_shift_plans() {
        let mut controller = MobileController::new();
        controller.selecting = true;
        controller.add_select_plan(super::super::plan::ClientPlan::place(
            5,
            5,
            0,
            BlockId::STONE_WALL,
        ));
        controller.pan(crate::config::TILESIZE as f32, 0.0, 800.0, 800.0);
        assert_eq!(controller.state.select_plans[0].x, 6);
    }

    #[test]
    fn mobile_zoom() {
        let mut detector = GestureDetector::new();
        detector.touch_down(0.0, 100.0, 100.0, 0);
        detector.touch_down(0.0, 200.0, 100.0, 1);
        let events = detector.touch_dragged(0.1, 300.0, 100.0, 1);
        assert!(events.iter().any(|e| matches!(e, GestureEvent::Zoom { .. })));
        let mut controller = MobileController::new();
        assert!((controller.zoom(100.0, 200.0, 4.0) - 8.0).abs() < 0.001);
    }

    #[test]
    fn mobile_edge_pan() {
        let mut controller = MobileController::new();
        let (vx, _vy) = controller.auto_pan(-1000.0, 540.0, 1920.0, 1080.0, 1920.0);
        assert!(vx < 0.0);
    }

    #[test]
    fn mobile_payload_target() {
        let mut controller = MobileController::new();
        // A friendly unit within 8 px wins over a building.
        let target = controller.resolve_payload_target(
            (10.0, 10.0),
            &[(7, 12.0, 10.0)],
            &[TilePos::new(1, 1)],
            false,
        );
        assert_eq!(target, PayloadTarget::Unit(7));
        // Otherwise a friendly building is selected.
        let target = controller.resolve_payload_target(
            (10.0, 10.0),
            &[],
            &[TilePos::new(1, 1)],
            false,
        );
        assert_eq!(target, PayloadTarget::Building(TilePos::new(1, 1)));
        // With a carried payload and no structure, drop at the position.
        let target = controller.resolve_payload_target((30.0, 40.0), &[], &[], true);
        assert_eq!(target, PayloadTarget::Position(30.0, 40.0));
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
