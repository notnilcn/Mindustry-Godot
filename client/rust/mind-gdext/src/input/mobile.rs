// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Touch glue + mobile UI callbacks (plan 15 §3.1/§3.5).
//!
//! Holds the platform-neutral [`MobileController`] and folds the time-stamped
//! Godot touch stream into its `GestureDetector`. Godot's
//! `InputEventScreenTouch`/`_Drag`/`Magnify` translation is the single-editor
//! MCP follow-up; the state machine itself is fully exercised by `mind-core`'s
//! replay tests (deviation I1).

use smallvec::SmallVec;

use mind_core::config::TILESIZE;
use mind_core::content::{BlockId, ContentRegistry};
use mind_core::input::focus::FocusState;
use mind_core::input::line::{
    LineBlock, PlanBlockInfo, line_block_from_def, plan_block_info_from_def,
};
use mind_core::input::line_rules::{LineRules, line_rules_for};
use mind_core::input::mobile::{MobileController, MobileTouchResult};
use mind_core::input::{
    InputCaps, InputState, MobileMode, PayloadTarget, PlacementWorld, RemoteAction,
};
use mind_core::world::TilePos;

use super::gesture::{GestureEvent, input_handler_detector};

/// JSON view of one gesture event (MCP/GDScript debugging).
pub fn gesture_json(event: &GestureEvent) -> serde_json::Value {
    match event {
        GestureEvent::TouchDown { x, y, pointer } => serde_json::json!({
            "name": event.name(),
            "x": x,
            "y": y,
            "pointer": pointer,
        }),
        GestureEvent::Tap { x, y, count } => serde_json::json!({
            "name": event.name(),
            "x": x,
            "y": y,
            "count": count,
        }),
        GestureEvent::LongPress { x, y } => {
            serde_json::json!({"name": event.name(), "x": x, "y": y})
        }
        GestureEvent::Pan {
            x,
            y,
            delta_x,
            delta_y,
        } => serde_json::json!({
            "name": event.name(),
            "x": x,
            "y": y,
            "dx": delta_x,
            "dy": delta_y,
        }),
        GestureEvent::Fling {
            velocity_x,
            velocity_y,
        } => serde_json::json!({
            "name": event.name(),
            "vx": velocity_x,
            "vy": velocity_y,
        }),
        GestureEvent::Zoom {
            initial_distance,
            distance,
        } => serde_json::json!({
            "name": event.name(),
            "initial": initial_distance,
            "distance": distance,
        }),
        GestureEvent::PanStop | GestureEvent::PinchStop => {
            serde_json::json!({"name": event.name()})
        }
    }
}

/// Converts a gesture event's viewport position into world pixels
/// (`Core.input.mouseWorld(x, y)` in Java); pan/zoom deltas stay in viewport
/// pixels and zoom distances are untouched.
pub fn event_to_world(
    event: &GestureEvent,
    to_world: impl Fn(f32, f32) -> (f32, f32),
) -> GestureEvent {
    match *event {
        GestureEvent::TouchDown { x, y, pointer } => {
            let (x, y) = to_world(x, y);
            GestureEvent::TouchDown { x, y, pointer }
        }
        GestureEvent::Tap { x, y, count } => {
            let (x, y) = to_world(x, y);
            GestureEvent::Tap { x, y, count }
        }
        GestureEvent::LongPress { x, y } => {
            let (x, y) = to_world(x, y);
            GestureEvent::LongPress { x, y }
        }
        GestureEvent::Pan {
            x,
            y,
            delta_x,
            delta_y,
        } => {
            let (x, y) = to_world(x, y);
            GestureEvent::Pan {
                x,
                y,
                delta_x,
                delta_y,
            }
        }
        other => other,
    }
}

/// The selected block's line metadata from the content registry (`None` = no
/// block selected or no content).
fn line_meta(
    content: Option<&ContentRegistry>,
    block: Option<BlockId>,
) -> (Option<LineBlock>, Option<LineRules>) {
    let (Some(content), Some(block)) = (content, block) else {
        return (None, None);
    };
    let Some(def) = content.block(block) else {
        return (None, None);
    };
    (
        Some(line_block_from_def(block, def)),
        Some(line_rules_for(content, block)),
    )
}

/// `Block.size/offset/rotate/invertFlip` for the schematic rotate/flip buttons
/// (`InputHandler.schemOriginX/Y` + `rotatePlans`/`flipPlans`).
fn plan_info(content: Option<&ContentRegistry>, block: BlockId) -> PlanBlockInfo {
    content
        .and_then(|content| content.block(block))
        .map(plan_block_info_from_def)
        .unwrap_or(PlanBlockInfo {
            size: 1,
            offset: 0.0,
            rotate: false,
            invert_flip: false,
        })
}

fn payload_target_json(target: PayloadTarget) -> serde_json::Value {
    match target {
        PayloadTarget::None => serde_json::Value::Null,
        PayloadTarget::Unit(id) => serde_json::json!({"kind": "unit", "id": id}),
        PayloadTarget::Building(pos) => {
            serde_json::json!({"kind": "building", "x": pos.x(), "y": pos.y()})
        }
        PayloadTarget::Position(x, y) => {
            serde_json::json!({"kind": "position", "x": x, "y": y})
        }
    }
}

/// Live [`InputCaps`] for the mobile gesture controller.
///
/// `MindInput` refreshes the fields it can observe each event: menu/editor
/// state, dialog/text focus, the `keyboard`/`blockreplace`/`swapdiagonal`
/// settings and the cutscene lock.
#[derive(Debug, Clone)]
pub struct MobileCaps {
    /// UI focus flags.
    pub focus: FocusState,
    /// `state.isMenu()`.
    pub menu: bool,
    /// `state.isEditor()`.
    pub editor: bool,
    /// `renderer.isCutscene()`.
    pub cutscene: bool,
    /// `settings.getBool("keyboard")` (mobile shoot-on-touch mode).
    pub keyboard: bool,
    /// `settings.getBool("blockreplace", true)`.
    pub block_replace: bool,
    /// `settings.getBool("swapdiagonal")`.
    pub swap_diagonal: bool,
}

impl Default for MobileCaps {
    fn default() -> Self {
        Self {
            focus: FocusState::default(),
            menu: false,
            editor: false,
            cutscene: false,
            keyboard: false,
            // `game.checkPref("blockreplace", true)`.
            block_replace: true,
            swap_diagonal: false,
        }
    }
}

impl InputCaps for MobileCaps {
    fn focus(&self) -> FocusState {
        self.focus.clone()
    }

    fn is_cutscene(&self) -> bool {
        self.cutscene
    }

    fn is_menu(&self) -> bool {
        self.menu
    }

    fn is_editor(&self) -> bool {
        self.editor
    }

    fn block_replace(&self) -> bool {
        self.block_replace
    }

    fn swap_diagonal(&self) -> bool {
        self.swap_diagonal
    }

    fn mobile(&self) -> bool {
        true
    }

    fn mobile_keyboard(&self) -> bool {
        self.keyboard
    }
}

/// Mobile session state owned by `/root/Spine/Input` when the handler is mobile.
#[derive(Debug, Clone)]
pub struct MobileInputBridge {
    /// The core controller.
    pub controller: MobileController,
    /// Frame counter (tick stamp for emitted commands).
    pub frame: u64,
    /// Last pointer position in viewport pixels (the gesture coordinate space).
    pointer: Option<(f32, f32)>,
}

impl Default for MobileInputBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl MobileInputBridge {
    /// Creates a bridge with the `InputHandler.add()` detector thresholds.
    pub fn new() -> Self {
        let controller = MobileController {
            detector: input_handler_detector(),
            ..MobileController::new()
        };
        Self {
            controller,
            frame: 0,
            pointer: None,
        }
    }

    /// The input state (for the inspector/state dump).
    pub fn state(&self) -> &InputState {
        &self.controller.state
    }

    /// Mobile mode flags.
    pub fn mode(&self) -> MobileMode {
        self.controller.mode
    }

    /// Current payload target.
    pub fn payload_target(&self) -> PayloadTarget {
        self.controller.payload_target
    }

    /// `setInput` preserves `block` (upstream `Control.setInput`).
    pub fn set_block(&mut self, block: Option<mind_core::content::BlockId>) {
        self.controller.state.select_block(block);
    }

    /// Folds a raw touch event into the gesture detector and returns the emitted
    /// gesture events. `time` is seconds from the client monotonic clock.
    pub fn touch_down(
        &mut self,
        time: f64,
        x: f32,
        y: f32,
        pointer: i32,
    ) -> SmallVec<[GestureEvent; 2]> {
        self.controller.detector.touch_down(time, x, y, pointer)
    }

    /// Touch drag.
    pub fn touch_drag(
        &mut self,
        time: f64,
        x: f32,
        y: f32,
        pointer: i32,
    ) -> SmallVec<[GestureEvent; 2]> {
        self.controller.detector.touch_dragged(time, x, y, pointer)
    }

    /// Touch up.
    pub fn touch_up(
        &mut self,
        time: f64,
        x: f32,
        y: f32,
        pointer: i32,
    ) -> SmallVec<[GestureEvent; 2]> {
        self.controller.detector.touch_up(time, x, y, pointer)
    }

    /// Fires a pending long press (called once per frame with the current time).
    pub fn tick(&mut self, time: f64) -> Option<GestureEvent> {
        self.frame += 1;
        self.controller.detector.update(time)
    }

    /// Records the raw pointer position (viewport pixels) so the per-frame line
    /// update and the touch-up flush resolve the same tile
    /// (`MobileInput.lastLineX/Y`).
    pub fn note_pointer(&mut self, x: f32, y: f32) {
        self.pointer = Some((x, y));
    }

    /// Forgets the tracked pointer (touch up / session reset).
    pub fn clear_pointer(&mut self) {
        self.pointer = None;
    }

    /// Last pointer position in viewport pixels.
    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.pointer
    }

    /// Whether a placement/break line drag is active (`MobileInput.lineMode`).
    pub fn line_mode(&self) -> bool {
        self.controller.mode.line_mode
    }

    /// Folds one gesture event through the shared decision tree with the
    /// selected block's [`LineBlock`] + [`LineRules`]
    /// (`MobileInput`/`InputHandler.updateLine`), so a long-press drag gets
    /// junction replacement, bridge linking and node spacing.
    pub fn handle_gesture(
        &mut self,
        event: GestureEvent,
        world: &dyn PlacementWorld,
        content: Option<&ContentRegistry>,
        caps: &dyn InputCaps,
    ) -> SmallVec<[RemoteAction; 4]> {
        let (line, rules) = line_meta(content, self.controller.state.block);
        self.controller
            .handle_gesture_with(event, world, line.as_ref(), rules.as_ref(), caps)
    }

    /// `MobileInput.update` lineMode branch (`MobileInput.java:881-889`): while
    /// a long-press line is active, re-derive it at the live pointer world
    /// position (`InputHandler.updateLine`).
    pub fn update_line(
        &mut self,
        world: &dyn PlacementWorld,
        content: Option<&ContentRegistry>,
        world_x: f32,
        world_y: f32,
    ) {
        if !self.controller.mode.line_mode {
            return;
        }
        let tile = TilePos::new(
            (world_x / TILESIZE as f32).floor() as i16,
            (world_y / TILESIZE as f32).floor() as i16,
        );
        if self.controller.state.place_mode.is_breaking() {
            // `MobileInput.update` re-derives the line only while placing
            // (`isPlacing()`); a breaking drag just tracks the release tile for
            // `removeSelection` (`MobileInput.java:881-889`, `:565-568`).
            self.controller.last_line = (tile.x() as i32, tile.y() as i32);
            return;
        }
        let (line, rules) = line_meta(content, self.controller.state.block);
        self.controller
            .drag_to_with(world, line.as_ref(), rules.as_ref(), tile);
    }

    /// `MobileInput.touchUp` (`MobileInput.java:557-580`): commit a placing line
    /// or collect the tiles a breaking line deconstructs.
    pub fn finish_touch(&mut self, world: &dyn PlacementWorld) -> MobileTouchResult {
        self.controller.finish_line_touch(world)
    }

    /// Confirm button (`MobileInput.java:222-266`): commit the queued selection
    /// plans (placements + deconstructions).
    pub fn confirm(&mut self, world: &dyn PlacementWorld) -> MobileTouchResult {
        self.controller.confirm_plans_result(world)
    }

    /// `mobile_keyboard` setting branch: keyboard mode disables touch pan/zoom.
    ///
    /// Plan 22 owns the native keyboard setting; the value is pushed by plan 14
    /// through [`Self::set_keyboard`] and defaults to `false`.
    pub fn keyboard_mode(&self) -> bool {
        self.controller.keyboard()
    }

    /// Pushes the `keyboard` mobile setting (plan-14 button / settings).
    pub fn set_keyboard(&mut self, value: bool) {
        self.controller.set_keyboard(value);
    }

    /// Clears mobile state (`updateState` menu branch).
    pub fn reset(&mut self) {
        self.controller.reset();
        self.pointer = None;
    }

    /// Current interaction mode names (plan-14 mobile UI read model).
    pub fn mode_json(&self) -> serde_json::Value {
        let mode = self.mode();
        serde_json::json!({
            "line_mode": mode.line_mode,
            "schematic_mode": mode.schematic_mode,
            "rebuild_mode": mode.rebuild_mode,
            "queue_command_mode": mode.queue_command_mode,
            "confirm_pending": mode.confirm_pending,
            "place_mode": self.controller.state.place_mode.name(),
        })
    }

    /// A compact state dump for the inspector (`get_input_state_json` subset).
    pub fn state_json(&self) -> serde_json::Value {
        let state = self.state();
        serde_json::json!({
            "mobile": true,
            "mode": state.place_mode.name(),
            "block": state.block.map(|block| block.raw()),
            "rotation": state.rotation,
            "is_building": state.is_building,
            "command_mode": state.command_mode,
            "queue_mode": state.queue_mode,
            "line_plans": state.line_plans.len(),
            "select_plans": state.select_plans.len(),
            "selected_units": state.selected_units.len(),
            "command_buildings": state.command_buildings.len(),
            "keyboard": self.keyboard_mode(),
            "payload_target": payload_target_json(self.payload_target()),
        })
    }

    /// `rotate` button (`MobileInput.java:205-219`): a rotatable selected block
    /// advances `rotation`; otherwise the button toggles schematic-select mode
    /// (`MobileController::rotate_button`). Returns whether the placement
    /// rotated.
    pub fn rotate_button(&mut self, content: Option<&ContentRegistry>) -> bool {
        let rotates = self
            .controller
            .state
            .block
            .and_then(|block| content?.block(block))
            .is_some_and(|def| def.rotate);
        self.controller.rotate_button(rotates)
    }

    /// Reverse rotation probe (`MindInput::mobile_rotate` with a negative
    /// delta); Java's mobile UI has no reverse button.
    pub fn rotate(&mut self, delta: i32) {
        let rotation = (self.controller.state.rotation as i32 + delta).rem_euclid(4) as u8;
        self.controller.state.rotation = rotation;
        self.controller.state.preview.rotation = rotation;
    }

    /// `schematic_flip_x`/`_y` buttons (`MobileInput.java:341-342`):
    /// `flipPlans(selectPlans, flipX)` about the mobile schematic origin with
    /// the real block size/offset (`InputHandler.flipPlans`).
    pub fn flip_selection(&mut self, flip_x: bool, content: Option<&ContentRegistry>) {
        let info = |block: BlockId| plan_info(content, block);
        self.controller.flip_selection(flip_x, &info);
    }

    /// `rotate` plans button (`MobileInput.java:344`):
    /// `rotatePlans(selectPlans, direction)` about the mobile schematic origin
    /// (`MobileController::rotate_selection`).
    pub fn rotate_selection(&mut self, direction: i32, content: Option<&ContentRegistry>) {
        let info = |block: BlockId| plan_info(content, block);
        self.controller.rotate_selection(direction, &info);
    }

    /// `toggle_command_mode` button.
    pub fn toggle_command_mode(&mut self) -> bool {
        self.controller.state.command_mode = !self.controller.state.command_mode;
        self.controller.state.command_mode
    }

    /// `toggle_queue_mode` button (`command_queue`).
    pub fn toggle_queue_mode(&mut self) -> bool {
        self.controller.state.queue_mode = !self.controller.state.queue_mode;
        self.controller.state.queue_mode
    }

    /// `toggle_schematic` button.
    pub fn toggle_schematic(&mut self) -> bool {
        self.controller.mode.schematic_mode = !self.controller.mode.schematic_mode;
        self.controller.state.has_schematic = self.controller.mode.schematic_mode;
        self.controller.mode.schematic_mode
    }

    /// `clear_building` button: drops queued selection plans.
    pub fn clear_select_plans(&mut self) {
        self.controller.state.select_plans.clear();
        self.controller.mode.confirm_pending = false;
    }

    /// Number of queued selection plans (plan-14 confirm button badge).
    pub fn select_plan_count(&self) -> usize {
        self.controller.state.select_plans.len()
    }

    /// Runs the per-frame `update()` state transitions (modes/line/gating).
    pub fn update_transitions(&mut self) {
        self.controller.update_transitions();
    }

    /// `MobileInput.update` keyboard-less camera move for the current frame.
    pub fn camera_move(&mut self, axis_x: f32, axis_y: f32, delta: f32) -> (f32, f32) {
        self.controller.camera_move(axis_x, axis_y, delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesture_positions_convert_to_world_and_deltas_survive() {
        // Java gesture callbacks receive screen points and convert through
        // `Core.input.mouseWorld`; pan/zoom payloads stay viewport pixels.
        let event = GestureEvent::Tap {
            x: 100.0,
            y: 40.0,
            count: 1,
        };
        let world = event_to_world(&event, |x, y| (x * 8.0, y * 8.0));
        match world {
            GestureEvent::Tap { x, y, count } => {
                assert_eq!((x, y, count), (800.0, 320.0, 1));
            }
            other => panic!("unexpected {other:?}"),
        }
        let pan = GestureEvent::Pan {
            x: 10.0,
            y: 20.0,
            delta_x: -3.0,
            delta_y: 4.0,
        };
        match event_to_world(&pan, |x, y| (x + 1.0, y + 2.0)) {
            GestureEvent::Pan {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                assert_eq!((x, y), (11.0, 22.0));
                assert_eq!((delta_x, delta_y), (-3.0, 4.0));
            }
            other => panic!("unexpected {other:?}"),
        }
        // Fling/zoom carry no position and pass through unchanged.
        let fling = GestureEvent::Fling {
            velocity_x: 1.0,
            velocity_y: 2.0,
        };
        assert!(matches!(
            event_to_world(&fling, |_, _| (0.0, 0.0)),
            GestureEvent::Fling { .. }
        ));
    }

    #[test]
    fn mobile_caps_default_to_the_upstream_settings() {
        let caps = MobileCaps::default();
        assert!(caps.mobile());
        assert!(caps.block_replace());
        assert!(!caps.swap_diagonal());
        assert!(!caps.mobile_keyboard());
        assert!(!caps.is_menu());
        assert_eq!(caps.player_select_range(), 17.0);
        let caps = MobileCaps {
            keyboard: true,
            swap_diagonal: true,
            ..MobileCaps::default()
        };
        assert!(caps.mobile_keyboard());
        assert!(caps.swap_diagonal());
    }
}
