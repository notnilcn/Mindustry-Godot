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

use mind_core::input::mobile::MobileController;
use mind_core::input::{InputState, MobileMode, PayloadTarget};

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

/// Mobile session state owned by `/root/Spine/Input` when the handler is mobile.
#[derive(Debug, Clone)]
pub struct MobileInputBridge {
    /// The core controller.
    pub controller: MobileController,
    /// Frame counter (tick stamp for emitted commands).
    pub frame: u64,
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

    /// `rotate` button: advances `input.rotation`.
    pub fn rotate(&mut self, delta: i32) {
        let rotation = (self.controller.state.rotation as i32 + delta).rem_euclid(4) as u8;
        self.controller.state.rotation = rotation;
        self.controller.state.preview.rotation = rotation;
    }

    /// `flip` button (`schematic_flip_x`/`_y`): mirrors queued plans.
    pub fn flip(&mut self, flip_x: bool, flip_y: bool) {
        mind_core::input::flip_plans(&mut self.controller.state.select_plans, flip_x, flip_y);
    }

    /// `rotate` plans button: rotates queued plans around the origin.
    pub fn rotate_plans(&mut self, direction: i32) {
        mind_core::input::rotate_plans(&mut self.controller.state.select_plans, direction);
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
