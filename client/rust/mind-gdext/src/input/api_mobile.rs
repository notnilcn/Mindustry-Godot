// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-callable mobile endpoints: state/mode JSON, block/rotate/flip, confirm, touch callbacks.
//! Upstream: core/src/mindustry/input/MobileInput.java

use super::*;

#[godot_api(secondary)]
impl MindInput {
    /// `/root/Spine/Input` mobile state JSON (plan-14 mobile HUD).
    #[func]
    pub fn mobile_state_json(&self) -> GString {
        GString::from(&self.mobile.state_json().to_string())
    }

    /// Mobile interaction mode flags JSON (plan-14 mobile HUD).
    #[func]
    pub fn mobile_mode_json(&self) -> GString {
        GString::from(&self.mobile.mode_json().to_string())
    }

    /// Pushes the `keyboard` mobile setting (plan 22 owns native input).
    #[func]
    pub fn set_mobile_keyboard(&mut self, keyboard: bool) {
        self.mobile.set_keyboard(keyboard);
    }

    /// Selects the mobile placement block by raw content id (`-1` clears).
    #[func]
    pub fn set_mobile_block(&mut self, raw: i64) {
        let block = if raw < 0 {
            None
        } else {
            Some(mind_core::content::BlockId::new(raw as u16))
        };
        self.mobile.set_block(block);
    }

    /// `rotate` mobile button (`MobileInput.java:205-219`): a rotatable selected
    /// block advances its rotation, otherwise the button toggles schematic
    /// select (`MobileController::rotate_button`). A negative `delta` is the
    /// MCP reverse-rotate probe (Java's UI has no reverse button).
    #[func]
    pub fn mobile_rotate(&mut self, delta: i64) {
        if delta < 0 {
            self.mobile.rotate(delta as i32);
            return;
        }
        if delta == 0 {
            return;
        }
        let Some(host) = self.sim_host.clone() else {
            return;
        };
        let guard = host.bind();
        self.mobile.rotate_button(guard.content_registry());
    }

    /// `flip` mobile button (`MobileInput.java:341-342`):
    /// `flipPlans(selectPlans, flipX/flipY)` about the mobile schematic origin,
    /// with the real block size/offset (`MobileController::flip_selection`).
    #[func]
    pub fn mobile_flip(&mut self, flip_x: bool, flip_y: bool) {
        let Some(host) = self.sim_host.clone() else {
            return;
        };
        let guard = host.bind();
        let content = guard.content_registry();
        if flip_x {
            self.mobile.flip_selection(true, content);
        }
        if flip_y {
            self.mobile.flip_selection(false, content);
        }
    }

    /// `rotate plans` mobile button (`MobileInput.java:344`):
    /// `rotatePlans(selectPlans, direction)` about the mobile schematic origin
    /// (`MobileController::rotate_selection`).
    #[func]
    pub fn mobile_rotate_plans(&mut self, direction: i64) {
        let Some(host) = self.sim_host.clone() else {
            return;
        };
        let guard = host.bind();
        self.mobile
            .rotate_selection(direction as i32, guard.content_registry());
    }

    /// `toggle command mode` mobile button.
    #[func]
    pub fn mobile_toggle_command_mode(&mut self) -> bool {
        self.mobile.toggle_command_mode()
    }

    /// `toggle queue mode` mobile button.
    #[func]
    pub fn mobile_toggle_queue_mode(&mut self) -> bool {
        self.mobile.toggle_queue_mode()
    }

    /// `toggle schematic` mobile button.
    #[func]
    pub fn mobile_toggle_schematic(&mut self) -> bool {
        self.mobile.toggle_schematic()
    }

    /// Confirm mobile button (`MobileInput.java:222-266`): commits the queued
    /// selection plans — valid placements become sim place commands, breaking
    /// plans become break commands. Returns the committed plan count.
    #[func]
    pub fn mobile_confirm(&mut self) -> i64 {
        let Some(mut host) = self.sim_host.clone() else {
            return 0;
        };
        let result = {
            let guard = host.bind();
            let world = desktop::HostWorld::new(&guard);
            self.mobile.confirm(&world)
        };
        let count = (result.placed.len() + result.breaks.len()) as i64;
        self.enqueue_mobile_result(&mut host, result);
        count
    }

    /// `clear building` mobile button.
    #[func]
    pub fn mobile_clear_select_plans(&mut self) {
        self.mobile.clear_select_plans();
    }

    /// Queued selection-plan count (confirm button badge).
    #[func]
    pub fn mobile_select_plan_count(&self) -> i64 {
        self.mobile.select_plan_count() as i64
    }

    /// Folds a Godot `InputEventScreenTouch` into the gesture detector.
    #[func]
    pub fn mobile_touch_down(&mut self, time: f64, x: f32, y: f32, pointer: i64) -> GString {
        let events = self.mobile.touch_down(time, x, y, pointer as i32);
        gestures_json(&events)
    }

    /// Folds a Godot `InputEventScreenDrag` into the gesture detector.
    #[func]
    pub fn mobile_touch_drag(&mut self, time: f64, x: f32, y: f32, pointer: i64) -> GString {
        let events = self.mobile.touch_drag(time, x, y, pointer as i32);
        gestures_json(&events)
    }

    /// Folds a Godot `InputEventScreenTouch` release into the gesture detector.
    #[func]
    pub fn mobile_touch_up(&mut self, time: f64, x: f32, y: f32, pointer: i64) -> GString {
        let events = self.mobile.touch_up(time, x, y, pointer as i32);
        gestures_json(&events)
    }

    /// Fires a pending long press at `time` and returns it as JSON.
    #[func]
    pub fn mobile_tick(&mut self, time: f64) -> GString {
        match self.mobile.tick(time) {
            Some(event) => GString::from(&mobile::gesture_json(&event).to_string()),
            None => GString::from("null"),
        }
    }

    /// Keyboard-less mobile camera move for the autoload's `_process`.
    #[func]
    pub fn mobile_camera_move(&mut self, axis_x: f32, axis_y: f32, delta: f64) -> Vector2 {
        let (x, y) = self.mobile.camera_move(axis_x, axis_y, delta as f32);
        Vector2::new(x, y)
    }

    /// Clears the mobile session (`updateState` menu branch).
    #[func]
    pub fn mobile_reset(&mut self) {
        self.mobile.reset();
    }
}

/// Serializes gesture events to a JSON array (GDScript/MCP consumers).
fn gestures_json(events: &[mind_core::input::GestureEvent]) -> GString {
    let array: Vec<serde_json::Value> = events.iter().map(mobile::gesture_json).collect();
    GString::from(&serde_json::Value::Array(array).to_string())
}
