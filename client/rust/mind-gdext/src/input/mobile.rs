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

use mind_core::input::{InputState, MobileMode, PayloadTarget};
use mind_core::input::mobile::MobileController;

use super::gesture::{GestureEvent, input_handler_detector};

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
    /// Plan 22 owns the native keyboard setting; until it lands the value is the
    /// upstream default (`false`).
    pub fn keyboard_mode(&self) -> bool {
        false
    }

    /// Clears mobile state (`updateState` menu branch).
    pub fn reset(&mut self) {
        self.controller.reset();
    }
}
