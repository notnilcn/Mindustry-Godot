// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! State JSON endpoints and the read-only control/placement probes.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

#[godot_api(secondary)]
impl MindInput {
    /// The state dump (§6.3 shape; playback fields come from the live bridge).
    #[func]
    pub fn get_input_state_json(&self) -> GString {
        let focus = serde_json::to_value(&self.focus).unwrap_or(serde_json::Value::Null);
        let state = &self.bridge.controller.state;
        let block = state.block.and_then(|id| {
            let host = self.bridge.host.clone()?;
            let guard = host.bind();
            guard
                .content_registry()?
                .block(id)
                .map(|def| def.name.clone())
        });
        let line_plans: Vec<serde_json::Value> = state
            .line_plans
            .iter()
            .map(|plan| {
                serde_json::json!({
                    "x": plan.x,
                    "y": plan.y,
                    "rotation": plan.rotation,
                    "block": plan.block.raw(),
                })
            })
            .collect();
        let selected: Vec<i32> = state.selected_units.iter().copied().collect();
        let buildings: Vec<[i32; 2]> = state
            .command_buildings
            .iter()
            .map(|pos| [pos.x() as i32, pos.y() as i32])
            .collect();
        let groups: Vec<Vec<i32>> = state
            .control_groups
            .iter()
            .map(|group| group.iter().copied().collect())
            .collect();
        let value = serde_json::json!({
            "format": 1,
            "tick": self.frame,
            "mobile": false,
            "mode": state.place_mode.name(),
            "block": block,
            "rotation": state.rotation,
            "is_building": state.is_building,
            "command_mode": state.command_mode,
            "queue_mode": state.queue_mode,
            "cursor": serde_json::Value::Null,
            "line_plans": line_plans,
            "select_plans": state.select_plans.len(),
            "player_plans_mirror": state.last_plans.len(),
            "selected_units": selected,
            "command_buildings": buildings,
            "command_rect": state.command_rect,
            "control_groups": groups,
            "last_action": self.bridge.last_action,
            "action_count": self.bridge.action_count,
            "locks": self.locks.names(),
            "focus": focus,
            "emitted_commands": [],
        });
        GString::from(&value.to_string())
    }

    /// Last placement-line preview (`[{x,y,rotation,block}]`).
    #[func]
    pub fn placement_preview_json(&self) -> GString {
        GString::from(self.bridge.preview_json.as_str())
    }

    /// `move_x`/`move_y` binding axis (`MindCamera2D` poll).
    #[func]
    pub fn pan_axis(&self) -> Vector2 {
        let (x, y) = self.bridge.pan_axis(&self.bindings);
        Vector2::new(x, y)
    }

    /// Raw binding axis by name (`Core.input.axis(Binding)`): -1/0/1 from the
    /// pressed keys of an `Axis` bind, with no gameplay/dialog gate. The map
    /// editor polls `move_x`/`move_y` through this while its dialog is open
    /// (`MapView.act`, `editor/MapView.java:180-185`); unknown or non-axis
    /// binds read 0.
    #[func]
    pub fn binding_axis(&self, name: GString) -> f32 {
        self.bridge.binding_axis(&self.bindings, &name.to_string())
    }

    /// `player.shooting` probe (left button held on a free tile).
    #[func]
    pub fn is_shooting(&self) -> bool {
        self.bridge.shooting
    }

    /// G05 T4b hint completions (`HintsFragment`): returns and clears the hint
    /// trigger keys collected since the last call
    /// (`shoot`, `boost_down`, `schematic_select_release`, `pick_tap`,
    /// `diagonal_release`, `rebuild_selecting`).
    #[func]
    pub fn take_hint_keys(&mut self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for key in self.bridge.take_hint_keys() {
            out.push(&GString::from(key));
        }
        out
    }

    /// `boost` binding held (`MindCamera2D` poll).
    #[func]
    pub fn boost_pressed(&self) -> bool {
        self.bridge.key_down(&self.bindings, ids::BOOST)
    }

    /// `pan` binding held (mouse-forward/middle pan).
    #[func]
    pub fn pan_pressed(&self) -> bool {
        self.bridge.key_down(&self.bindings, ids::PAN)
    }

    /// Whether the world accepts gameplay input (no open dialog/text field).
    #[func]
    pub fn gameplay_input_active(&self) -> bool {
        !self.bridge.text_focus && !self.bridge.ui_dialog
    }

    /// Toggles RTS command mode (`Binding.command_mode`).
    #[func]
    pub fn toggle_command_mode(&mut self) -> bool {
        self.bridge.toggle_command_mode();
        self.bridge.controller.state.command_mode
    }

    /// Whether command mode is active.
    #[func]
    pub fn command_mode(&self) -> bool {
        self.bridge.controller.state.command_mode
    }
}

impl MindInput {
    /// `DesktopInput.updateMovement` inputs for the sim host:
    /// `(axis_x, axis_y, shooting, aim_x, aim_y, boosting)`. Only `bind`s the
    /// camera (the caller may hold the host) — never touches `MindSimHost`.
    pub fn player_control(&self) -> (f32, f32, bool, f32, f32, bool, bool) {
        let (axis_x, axis_y) = self.bridge.pan_axis(&self.bindings);
        let (aim_x, aim_y) = self.bridge.aim_world().unwrap_or((0.0, 0.0));
        let boosting =
            self.bridge.key_down(&self.bindings, ids::BOOST) && self.gameplay_input_active();
        let mouse_move =
            self.bridge.key_down(&self.bindings, ids::MOUSE_MOVE) && self.gameplay_input_active();
        (
            axis_x,
            axis_y,
            self.bridge.shooting_allowed(),
            aim_x,
            aim_y,
            boosting,
            mouse_move,
        )
    }

    /// Whether a placement block is selected (`state.block`); `false` after the
    /// deselect key clears it (`InputHandler.tryBeginMine` gate).
    pub fn block_selected(&self) -> bool {
        self.bridge.controller.state.block.is_some()
    }

    /// `mouseMove` binding held (mouse-back cursor pull; T9 uploads it to the
    /// player unit, T10 makes it pan while paused/dead).
    pub fn mouse_move_pressed(&self) -> bool {
        self.bridge.key_down(&self.bindings, ids::MOUSE_MOVE)
    }

    /// Last tracked cursor position in viewport coordinates
    /// (`Core.input.mouseX/Y`; the camera pan reads it, T10).
    pub fn mouse_position(&self) -> (f32, f32) {
        self.bridge.mouse
    }

    /// `InputHandler.locked()`: any active input lock gates the camera pan and
    /// follow (`DesktopInput.java:276`; T10).
    pub fn locked(&self) -> bool {
        self.locks.any()
    }

    /// Read-only placement state for the world overlay pass (G03 T2).
    /// Rust-only seam (`DesktopBridge::placement_view`).
    pub fn placement_view(&self) -> desktop::PlacementView {
        self.bridge.placement_view()
    }
}
