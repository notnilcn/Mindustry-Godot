// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Key bindings, axes and the raw-event `handle` entry point.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    /// Whether a single-key binding is currently held.
    pub fn key_down(&self, bindings: &BindingState, id: u16) -> bool {
        match bindings.value(id) {
            BindingValue::Key(name) => self.pressed.contains(name),
            BindingValue::Axis { negative, positive } => {
                negative
                    .as_ref()
                    .is_some_and(|name| self.pressed.contains(name))
                    || positive
                        .as_ref()
                        .is_some_and(|name| self.pressed.contains(name))
            }
            BindingValue::Unset => false,
        }
    }

    /// Held axis value (`-1`/`0`/`1`) for an axis binding.
    pub fn key_axis(&self, bindings: &BindingState, id: u16) -> f32 {
        match bindings.value(id) {
            BindingValue::Axis { negative, positive } => {
                let mut value = 0.0;
                if negative
                    .as_ref()
                    .is_some_and(|name| self.pressed.contains(name))
                {
                    value -= 1.0;
                }
                if positive
                    .as_ref()
                    .is_some_and(|name| self.pressed.contains(name))
                {
                    value += 1.0;
                }
                value
            }
            _ => 0.0,
        }
    }

    /// `Core.input.axis(Binding)` by registry name (`MindInput.binding_axis`):
    /// `-1`/`0`/`1` from the held keys, with no gameplay/dialog gate. The map
    /// editor polls `move_x`/`move_y` through this while its dialog is open
    /// (`MapView.act`, `editor/MapView.java:180-185`); unknown or non-axis
    /// binds read 0.
    pub fn binding_axis(&self, bindings: &BindingState, name: &str) -> f32 {
        let Some(index) = KeyBindTable::all()
            .iter()
            .position(|bind| bind.name == name)
        else {
            return 0.0;
        };
        self.key_axis(bindings, index as u16)
    }

    /// Whether an axis/key binding drives the wheel (`scroll`).
    pub(super) fn axis_is_scroll(bindings: &BindingState, id: u16) -> bool {
        match bindings.value(id) {
            BindingValue::Axis { negative, positive } => {
                negative.as_deref() == Some("scroll") || positive.as_deref() == Some("scroll")
            }
            BindingValue::Key(name) => name == "scroll",
            BindingValue::Unset => false,
        }
    }

    /// Whether the negative direction of an axis binding is currently held.
    pub(super) fn axis_negative(&self, bindings: &BindingState, id: u16) -> bool {
        match bindings.value(id) {
            BindingValue::Axis {
                negative: Some(name),
                ..
            } => self.pressed.contains(name),
            _ => false,
        }
    }

    /// X/Y pan axis from the `move_x`/`move_y` bindings.
    pub fn pan_axis(&self, bindings: &BindingState) -> (f32, f32) {
        if self.text_focus || self.ui_dialog {
            return (0.0, 0.0);
        }
        (
            self.key_axis(bindings, ids::MOVE_X),
            self.key_axis(bindings, ids::MOVE_Y),
        )
    }

    /// Consumes one raw event (bindings are only read).
    pub fn handle(&mut self, bindings: &BindingState, event: RawEvent) {
        // `DesktopInput.java:465-469`: while a dialog owns input, the held
        // select button no longer keeps `player.shooting` latched.
        if self.ui_dialog && !matches!(event, RawEvent::MouseMove { .. }) {
            self.shooting = false;
        }
        let mut command_edge: Option<bool> = None;
        match event {
            RawEvent::KeyDown { code } => {
                let fresh = self.pressed.insert(code.clone());
                // `DesktopInput.drawTop` reads both selection keys from the
                // binding table; mirror them for the draw pass (`isPlacing`
                // only refreshes on key edges, matching Java's keyDown poll).
                self.refresh_selection_held(bindings);
                if bindings.name(ids::COMMAND_MODE) == Some(code.as_str()) {
                    command_edge = Some(true);
                }
                if fresh && !self.text_focus {
                    if self.ui_dialog {
                        // ResearchDialog.java:145-149 / PlanetDialog.java:92:
                        // the dialog's own key closes it (J/N toggle).
                        self.close_dialog_key(bindings, &code);
                    } else if self.in_game {
                        // `Control.java:690-760`: gameplay binds run only while
                        // `state.isGame()`.
                        self.key_down_event(bindings, &code);
                    } else if bindings.name(ids::FULLSCREEN) == Some(code.as_str()) {
                        // `fullscreen` is a `Control` bind handled before the
                        // `state.isGame()` gate and works on the menu.
                        self.dispatch_action(bindings, "fullscreen");
                    }
                }
            }
            RawEvent::KeyUp { code } => {
                self.pressed.remove(&code);
                // Runs before the text-focus gate so a release swallowed by a
                // focused field still clears the F/B held flag.
                self.refresh_selection_held(bindings);
                if bindings.name(ids::COMMAND_MODE) == Some(code.as_str()) {
                    command_edge = Some(false);
                }
                if self.in_game && !self.text_focus {
                    self.key_up_event(bindings, &code);
                }
            }
            RawEvent::MouseMove { x, y } => {
                self.mouse = (x, y);
                self.mouse_move(bindings);
                // `pollInputPlayer` L657-665: the grabbed `splan` follows the
                // pointer (`x = round((mouseWorld + offset) / tilesize)`).
                self.update_splan();
            }
            RawEvent::MouseButton { button, down, x, y } => {
                self.mouse = (x, y);
                if self.locked && down && (button == "left" || button == "right") {
                    // `InputHandler.locked()`: world presses are gated off.
                    return;
                }
                self.mouse_button(bindings, &button, down);
            }
            RawEvent::Scroll { y, .. } => {
                if self.locked {
                    return;
                }
                self.scroll(bindings, y);
            }
            RawEvent::Magnify { factor } => {
                if let Some(mut camera) = self.camera.clone() {
                    camera.bind_mut().zoom_by((factor - 1.0) as f64);
                }
            }
            _ => {}
        }
        // `DesktopInput.update` runs once per frame; applying it per edge keeps
        // hold mode in step with the event stream.
        self.update_command_mode(bindings, command_edge);
    }

    /// `ResearchDialog.java:145-149` / `PlanetDialog.java:92`: while the dialog
    /// is topmost, its own binding key closes it (`J`/`N` toggle).
    fn close_dialog_key(&mut self, bindings: &BindingState, code: &str) {
        let closes = if self.top_dialog == "research" {
            bindings.name(ids::RESEARCH) == Some(code)
        } else if self.top_dialog == "planet" {
            bindings.name(ids::PLANET_MAP) == Some(code)
        } else {
            false
        };
        if closes {
            self.effects.push(Effect::CloseTopDialog);
        }
    }

    /// Dispatches every binding whose current key matches a down edge.
    fn key_down_event(&mut self, bindings: &BindingState, code: &str) {
        let mut fired: Vec<&'static str> = Vec::new();
        for (index, bind) in KeyBindTable::all().iter().enumerate() {
            let id = index as u16;
            let matched = match bindings.value(id) {
                BindingValue::Key(name) => name == code,
                BindingValue::Axis { negative, positive } => {
                    negative.as_deref() == Some(code) || positive.as_deref() == Some(code)
                }
                BindingValue::Unset => false,
            };
            if matched {
                fired.push(bind.name);
            }
        }
        for name in fired {
            if POINTER_BINDS.contains(&name) {
                self.pointer_action(bindings, name, true);
            } else {
                self.dispatch_action(bindings, name);
            }
        }
    }

    /// A pointer binding edge from the keyboard path (T9: rebinding `select` to
    /// a key routes the same world handlers; the mouse path calls them from
    /// `mouse_button`).
    fn pointer_action(&mut self, bindings: &BindingState, name: &str, down: bool) {
        if down {
            match name {
                "select" => self.left_press(bindings),
                "break_block" | "deselect" => self.right_press(bindings),
                "pick" | "command_queue" => {
                    if self.controller.state.command_mode {
                        self.command_tap_queued();
                    } else {
                        self.pick_block();
                    }
                }
                _ => {}
            }
        } else {
            match name {
                "select" => self.left_release(bindings),
                "break_block" | "deselect" => self.right_release(bindings),
                _ => {}
            }
        }
    }

    /// `DesktopInput.update` command-mode gate: only while no placement block is
    /// selected and no UI field/dialog owns input. The upstream boost-conflict
    /// guard (a live player unit that can boost sharing the key) needs the
    /// possessed-unit feed, which the bridge does not have yet.
    /// `DesktopInput.java:300-311`: command mode needs an active game, an
    /// unlocked input, no selected block and no text focus/dialog. It is
    /// disabled while the player's unit can boost and `commandMode` shares one
    /// key with `boost` (the default `shiftLeft`).
    pub(super) fn command_mode_allowed(&self, bindings: &BindingState) -> bool {
        let shared = bindings.value(ids::COMMAND_MODE) == bindings.value(ids::BOOST);
        let boost_guard = shared
            && self
                .host
                .as_ref()
                .is_some_and(|host| host.bind().player_unit_can_boost());
        self.in_game
            && !self.locked
            && self.controller.state.block.is_none()
            && !self.text_focus
            && !self.ui_dialog
            && !boost_guard
    }

    /// Re-applies the `DesktopInput.update` command-mode branch after one event.
    ///
    /// The gate closing always clears command mode (upstream else-branch).
    /// While the gate is open and `commandmodehold` is on, command mode follows
    /// the command key: a command-key edge sets it, and any event while the key
    /// is held keeps it on. A released key does not clear a mode latched through
    /// `toggle_command_mode` (the MCP/test entry point), so that probe stays
    /// usable without holding the key.
    pub fn update_command_mode(&mut self, bindings: &BindingState, command_edge: Option<bool>) {
        if !self.command_mode_allowed(bindings) {
            self.set_command_mode(false);
            return;
        }
        if !self.command_mode_hold {
            // The tap branch toggles on the key press edge (`dispatch_action`);
            // the latched value persists until the gate closes.
            return;
        }
        match command_edge {
            Some(down) => self.set_command_mode(down),
            None if self.key_down(bindings, ids::COMMAND_MODE) => self.set_command_mode(true),
            None => {}
        }
    }

    /// Applies the `commandmodehold` client setting (defaults to upstream true).
    pub fn set_command_mode_hold(&mut self, hold: bool) {
        self.command_mode_hold = hold;
    }

    /// `settings.blockreplace` (read once in `bootstrap`).
    pub fn set_block_replace(&mut self, replace: bool) {
        self.block_replace = replace;
    }

    /// `settings.buildautopause` (read once in `bootstrap`).
    pub fn set_build_autopause(&mut self, enabled: bool) {
        self.build_autopause = enabled;
    }

    /// `DesktopInput.java:591-594` auto-pause plus the Java resume rule: the
    /// auto-pause latch clears when a plan exists again (`ConstructBlock.tapped`
    /// resumes building after queueing its plan, `ConstructBlock.java:206-208`).
    pub fn update_build_autopause(&mut self) {
        if !self.build_autopause {
            return;
        }
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let plans = host.bind().player_plan_count();
        if self.controller.state.is_building && plans == 0 {
            // `isBuilding = false; buildWasAutoPaused = true;`
            self.controller.state.is_building = false;
            self.build_was_auto_paused = true;
            host.bind_mut().set_player_building_paused(true);
        } else if self.build_was_auto_paused && !self.controller.state.is_building && plans > 0 {
            self.controller.state.is_building = true;
            self.build_was_auto_paused = false;
            host.bind_mut().set_player_building_paused(false);
        }
    }

    /// Sets command mode to `enabled` (`DesktopInput.update` assignment).
    /// Turning it off clears the RTS selection like `toggle_command_mode`.
    pub fn set_command_mode(&mut self, enabled: bool) {
        if self.controller.state.command_mode == enabled {
            return;
        }
        if enabled {
            self.controller.state.command_mode = true;
            self.fire_hotkey("command_mode");
        } else {
            self.toggle_command_mode();
        }
    }
}
