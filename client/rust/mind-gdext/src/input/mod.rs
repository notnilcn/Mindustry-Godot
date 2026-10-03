// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindInput` — the `Spine/Input` node (plan 15 §3.1).
//!
//! Captures raw Godot `InputEvent`s into [`RawEvent`]s and owns client-local
//! input state (bindings, locks, focus). No game rules and no sim reads/writes
//! (I1/I2); the placement controllers live in `mind-core`.

pub mod bindings;
pub mod events;

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

    fn process(&mut self, _delta: f64) {
        self.frame += 1;
        // M0: events are logged/counted; M2 feeds the desktop/mobile controllers.
        self.last_event_count = self.pending.len();
        self.pending.clear();
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
}
