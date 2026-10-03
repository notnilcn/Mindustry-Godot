// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindInput` — the `Spine/Input` node (plan 15 §3.1).
//!
//! Captures raw Godot `InputEvent`s into [`RawEvent`]s and owns client-local
//! input state (bindings, locks, focus). No game rules and no sim reads/writes
//! (I1/I2); the placement controllers live in `mind-core`.

pub mod bindings;
pub mod events;
pub mod gesture;
pub mod mobile;

use godot::classes::{INode, Node};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::input::{BindingState, FocusState, InputLocks, KeyBindTable, RawEvent};

/// The `/root/Spine/Input` input node.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindInput {
    base: Base<Node>,
    /// Client-local bind values (`Binding.state`).
    bindings: BindingState,
    /// Input locks (`InputHandler.inputLocks`).
    locks: InputLocks,
    /// UI focus flags (plan 14 push).
    focus: FocusState,
    /// Events captured this frame, drained by `_process`.
    pending: Vec<RawEvent>,
    /// Render-frame counter (also the input tick stamp).
    frame: u64,
    /// Events drained on the last `_process`.
    last_event_count: usize,
    /// Whether the malformed-event warning has been logged once.
    warned: bool,
    /// Mobile touch session (constructed when the handler is mobile).
    mobile: mobile::MobileInputBridge,
    /// Client monotonic clock in seconds for the gesture detector.
    mobile_time: f64,
}

#[godot_api]
impl INode for MindInput {
    fn init(base: Base<Node>) -> Self {
        KeyBindTable::init();
        Self {
            base,
            bindings: BindingState::new(),
            locks: InputLocks::new(),
            focus: FocusState::default(),
            pending: Vec::new(),
            frame: 0,
            last_event_count: 0,
            warned: false,
            mobile: mobile::MobileInputBridge::new(),
            mobile_time: 0.0,
        }
    }

    fn ready(&mut self) {
        self.bindings = bindings::load();
        log::info!(
            "MindInput ready ({} keybinds, {} rebinds)",
            KeyBindTable::len(),
            self.rebind_count()
        );
    }

    fn input(&mut self, event: Gd<godot::classes::InputEvent>) {
        let mut translated = Vec::new();
        events::translate(&event, &mut translated);
        self.pending.extend(translated);
    }

    fn process(&mut self, delta: f64) {
        self.frame += 1;
        // M0: events are logged/counted; the desktop controller is M2.
        self.last_event_count = self.pending.len();
        self.pending.clear();
        // M3: drive the mobile gesture detector's long-press timer. The touch
        // stream itself is folded in by the `mobile_*` callbacks (plan 14).
        self.mobile_time += delta;
        let _ = self.mobile.tick(self.mobile_time);
        self.mobile.update_transitions();
    }
}

#[godot_api]
impl MindInput {
    /// Frames pumped.
    #[func]
    pub fn frame(&self) -> i64 {
        self.frame as i64
    }

    /// Events drained on the last `_process`.
    #[func]
    pub fn last_event_count(&self) -> i64 {
        self.last_event_count as i64
    }

    /// Number of binds that differ from upstream defaults.
    #[func]
    pub fn rebind_count(&self) -> i64 {
        let defaults = BindingState::new();
        (0..KeyBindTable::len())
            .filter(|index| {
                let id = *index as u16;
                self.bindings.value(id) != defaults.value(id)
            })
            .count() as i64
    }

    /// Injects a key edge (GDScript/MCP testing).
    #[func]
    pub fn inject_key(&mut self, code: GString, down: bool) {
        let code = code.to_string();
        self.pending.push(if down {
            RawEvent::key_down(code)
        } else {
            RawEvent::key_up(code)
        });
    }

    /// Parses and queues one `RawEvent` JSON object (`{"t":...}`).
    #[func]
    pub fn push_event(&mut self, json: GString) -> bool {
        match serde_json::from_str::<RawEvent>(&json.to_string()) {
            Ok(event) => {
                self.pending.push(event);
                true
            }
            Err(error) => {
                if !self.warned {
                    log::warn!("MindInput: bad RawEvent JSON ({error})");
                    self.warned = true;
                }
                false
            }
        }
    }

    /// Sets the UI focus flags from a `FocusState` JSON object.
    #[func]
    pub fn set_ui_focus(&mut self, json: GString) -> bool {
        match serde_json::from_str::<FocusState>(&json.to_string()) {
            Ok(focus) => {
                self.focus = focus;
                true
            }
            Err(error) => {
                log::warn!("MindInput: bad FocusState JSON ({error})");
                false
            }
        }
    }

    /// Replaces the lock set from a JSON list of lock names.
    #[func]
    pub fn set_locks(&mut self, json: GString) -> bool {
        match serde_json::from_str::<Vec<mind_core::input::LockId>>(&json.to_string()) {
            Ok(locks) => {
                let mut state = InputLocks::new();
                for lock in locks {
                    state.add_lock(lock);
                }
                self.locks = state;
                true
            }
            Err(error) => {
                log::warn!("MindInput: bad lock JSON ({error})");
                false
            }
        }
    }

    /// Rebinds a single key by registry name and persists it.
    #[func]
    pub fn rebind(&mut self, name: GString, code: GString) -> bool {
        if bindings::rebind(&mut self.bindings, &name.to_string(), &code.to_string()) {
            bindings::save(&self.bindings);
            true
        } else {
            false
        }
    }

    /// The full binding registry + current values as JSON.
    #[func]
    pub fn keybinds_json(&self) -> GString {
        let mut list = Vec::new();
        for (index, bind) in KeyBindTable::all().iter().enumerate() {
            let id = index as u16;
            list.push(serde_json::json!({
                "name": bind.name,
                "category": bind.category.map(|c| c.name()),
                "axis": bind.kind == mind_core::input::KeyKind::Axis,
                "bundle": bind.bundle_key(),
                "value": self.bindings.name(id),
            }));
        }
        GString::from(&serde_json::Value::Array(list).to_string())
    }

    /// The state dump (§6.3 shape; fields owned by later milestones are stubs).
    #[func]
    pub fn get_input_state_json(&self) -> GString {
        let focus = serde_json::to_value(&self.focus).unwrap_or(serde_json::Value::Null);
        let value = serde_json::json!({
            "format": 1,
            "tick": self.frame,
            "mobile": false,
            "mode": "none",
            "block": serde_json::Value::Null,
            "rotation": 0,
            "is_building": true,
            "command_mode": false,
            "queue_mode": false,
            "cursor": serde_json::Value::Null,
            "line_plans": [],
            "select_plans": [],
            "player_plans_mirror": 0,
            "selected_units": [],
            "command_buildings": [],
            "command_rect": serde_json::Value::Null,
            "control_groups": [],
            "locks": self.locks.names(),
            "focus": focus,
            "emitted_commands": [],
        });
        GString::from(&value.to_string())
    }

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

    /// `rotate` mobile button.
    #[func]
    pub fn mobile_rotate(&mut self, delta: i64) {
        self.mobile.rotate(delta as i32);
    }

    /// `flip` mobile button.
    #[func]
    pub fn mobile_flip(&mut self, flip_x: bool, flip_y: bool) {
        self.mobile.flip(flip_x, flip_y);
    }

    /// `rotate plans` mobile button.
    #[func]
    pub fn mobile_rotate_plans(&mut self, direction: i64) {
        self.mobile.rotate_plans(direction as i32);
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
