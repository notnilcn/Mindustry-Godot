// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindInput` — the `Spine/Input` node (plan 15 §3.1).
//!
//! Captures raw Godot `InputEvent`s into [`RawEvent`]s, owns client-local input
//! state (bindings, locks, focus) and drives the live desktop controller
//! ([`desktop::DesktopBridge`]): mouse/key actions become `SimCommand`s on
//! `MindSimHost` (applied at tick boundaries) and view effects on the camera/HUD.
//! No game rules and no sim reads/writes (I1/I2); the placement controllers live
//! in `mind-core`.

mod api_building;
mod api_core;
mod api_keybinds;
mod api_mobile;
mod api_selection;
mod api_state;
pub mod bindings;
pub mod desktop;
mod effects;
pub mod events;
pub mod gesture;
mod glue;
mod glue_mobile;
pub mod mobile;

use std::collections::HashSet;

use godot::classes::notify::NodeNotification;
use godot::classes::{Control, INode, Node};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::determinism::SimCommand;
use mind_core::input::{
    BindingState, BindingValue, FocusState, InputLocks, KeyBindTable, RawEvent, ids,
    key_display_name,
};
use mind_core::world::config::ConfigValue;

use crate::audio::MindAudio;
use crate::camera::MindCamera2D;
use crate::campaign::MindCampaign;
use crate::sim_host::MindSimHost;
use crate::ui::root::MindUiRoot;
use crate::wiring;

use desktop::{DesktopBridge, Effect};

/// The `/root/Spine/Input` input node.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindInput {
    base: Base<Node>,
    /// Sibling `SimHost` (`../SimHost`) — required; the sim command target.
    #[export]
    pub sim_host: Option<Gd<MindSimHost>>,
    /// Sibling `World/Camera2D` (`../World/Camera2D`) — required; aim/pan maths.
    #[export]
    pub camera: Option<Gd<MindCamera2D>>,
    /// Sibling `MindCampaign` (`../MindCampaign`) — optional; campaign gates,
    /// skip-wave and schematic seams.
    #[export]
    pub campaign: Option<Gd<MindCampaign>>,
    /// Sibling `MindAudio` (`../MindAudio`) — optional; the block-config click.
    #[export]
    pub audio: Option<Gd<MindAudio>>,
    /// `Ui/UiRoot` (`../Ui/UiRoot`) — required; the HUD fragment mediator.
    #[export]
    pub ui_root: Option<Gd<MindUiRoot>>,
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
    /// Live desktop controller bridge (placement, hotkeys, RTS).
    bridge: DesktopBridge,
    /// Touch pointers owned by a STOP-filter control (no world gestures).
    ui_pointers: HashSet<i32>,
    /// Tile of the building whose config panel was opened last
    /// (`input.config.getSelected()` mirror; H03 T3/T5). The node's
    /// `visible` flag decides whether it is still shown.
    config_tile: Option<(i32, i32)>,
    /// World point of a Ctrl+`ping` waiting for its `@ping.text` prompt answer
    /// (`DesktopInput.java:492-493`); `None` when no prompt is in flight.
    ping_pending: Option<(f32, f32)>,
}

#[godot_api]
impl INode for MindInput {
    fn init(base: Base<Node>) -> Self {
        KeyBindTable::init();
        Self {
            base,
            sim_host: None,
            camera: None,
            campaign: None,
            audio: None,
            ui_root: None,
            bindings: BindingState::new(),
            locks: InputLocks::new(),
            focus: FocusState::default(),
            pending: Vec::new(),
            frame: 0,
            last_event_count: 0,
            warned: false,
            mobile: mobile::MobileInputBridge::new(),
            mobile_time: 0.0,
            bridge: DesktopBridge::new(),
            ui_pointers: HashSet::new(),
            config_tile: None,
            ping_pending: None,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
        // A focus loss drops key releases; clear held state so the camera does
        // not keep panning until the key is pressed again.
        if what == NodeNotification::APPLICATION_FOCUS_OUT {
            self.bridge.clear_keys();
        }
    }

    fn input(&mut self, event: Gd<godot::classes::InputEvent>) {
        let mut translated = Vec::new();
        events::translate(&event, &mut translated);
        // Dialog/field state is only read for key/button/scroll edges (mouse
        // motion is high-frequency and cannot change it).
        let needs_state = translated
            .iter()
            .any(|raw| !matches!(raw, RawEvent::MouseMove { .. }));
        let dialog_open = needs_state && self.dialog_open();
        if needs_state {
            self.bridge.text_focus = self.text_field_focused();
            self.bridge.ui_dialog = dialog_open;
            self.refresh_game_state();
            self.refresh_top_dialog();
            let locked = self.locks.any();
            if self.bridge.locked != locked {
                self.bridge.locked = locked;
                if let Some(mut host) = self.bridge.host.clone() {
                    host.bind_mut().set_input_locked(locked);
                }
            }
        }
        for raw in translated {
            // Touch gestures drive the mobile bridge directly (plan-14 mobile
            // HUD is out of scope; pan/zoom already work without it).
            if self.route_touch(&raw) {
                continue;
            }
            self.refresh_command_mode_hold(&raw);
            // `DesktopInput.java:476-481`: the wheel only zooms while
            // `!scene.hasScroll()` and no console/chat fragment is shown
            // (PAR-0100). Skip the zoom WITHOUT consuming the event: scene2d
            // still delivers `scrolled` to the hovered pane, so the GUI
            // (`ScrollContainer`, `Slider`) must receive the wheel here —
            // marking it handled killed every UI scroll (J10 T2 item 3).
            if let RawEvent::Scroll { .. } = &raw {
                if self.bridge.locked {
                    self.mark_input_handled();
                    continue;
                }
                let (mx, my) = self.bridge.mouse;
                if self.ui_scroll_at(mx, my) || self.console_or_chat_visible() {
                    continue;
                }
            }
            // World mouse presses: skip when a STOP-filter `Control` owns the
            // event position (upstream `!Core.scene.hasMouse()`); otherwise
            // consume the event so `MindSimHost::unhandled_input` cannot
            // double-place. The positional walk is used (not
            // `gui_get_hovered_control`) so synthetic/MCP clicks at viewport
            // coordinates test the same point.
            let is_press = matches!(
                &raw,
                RawEvent::MouseButton { button, down: true, .. }
                    if button == "left" || button == "right" || button == "middle"
            );
            if is_press {
                let RawEvent::MouseButton { x, y, .. } = &raw else {
                    continue;
                };
                if self.ui_captures_at(*x, *y) {
                    continue;
                }
                // Behind an open dialog no world click may land, but the click
                // must not reach `MindSimHost::unhandled_input` either.
                if dialog_open {
                    self.mark_input_handled();
                    continue;
                }
                // `state.isGame()` (`Control.java:690`): the menu has no world;
                // the click must not reach `MindSimHost::unhandled_input`.
                if !self.bridge.in_game {
                    self.mark_input_handled();
                    continue;
                }
            }
            self.bridge.handle(&self.bindings, raw);
            if is_press {
                self.mark_input_handled();
            }
        }
    }

    fn process(&mut self, delta: f64) {
        self.frame += 1;
        // Test/MCP-injected events (`inject_key`/`push_event`) are dispatched
        // here; real events are dispatched immediately in `input`.
        let pending = std::mem::take(&mut self.pending);
        self.last_event_count = pending.len();
        self.bridge.text_focus = self.text_field_focused();
        self.bridge.ui_dialog = self.dialog_open();
        self.refresh_game_state();
        self.refresh_top_dialog();
        // `InputHandler.locked()`: the host zeroes the player axis while any
        // lock is active (PAR-0100).
        let locked = self.locks.any();
        if self.bridge.locked != locked {
            self.bridge.locked = locked;
            if let Some(mut host) = self.bridge.host.clone() {
                host.bind_mut().set_input_locked(locked);
            }
        }
        for event in pending {
            self.refresh_command_mode_hold(&event);
            self.bridge.handle(&self.bindings, event);
        }
        // `DesktopInput.update` applies the command-mode branch every frame, so
        // state changes that arrive without a key edge (block selection from a
        // fragment, focus changes) still close the gate.
        self.bridge.update_command_mode(&self.bindings, None);
        // `pollInputPlayer` L762-768: `overrideLineRotation` clears whenever the
        // frame is not an active placement line.
        self.bridge.sync_line_override();
        // `drawUnitSelection` L1354-1368: the command drag box's second corner
        // is the live mouse world point, recomputed every frame.
        self.bridge.sync_command_rect();
        // Cargo keys repeat while held (`DesktopInput.updateMovement`).
        self.bridge.update_payload_hold(&self.bindings);
        // `DesktopInput.java:591-594`: auto-pause building while the player
        // unit's queue is empty (`buildautopause`).
        self.bridge.update_build_autopause();
        // `DI:313`: `selectedUnits.removeAll(u -> !u.allowCommand() ||
        // !u.isValid() || u.team != player.team());`
        {
            let team = self.bridge.player_team();
            let valid: std::collections::HashSet<i32> = self
                .bridge
                .selectable
                .iter()
                .filter(|unit| unit.commandable && unit.team == team)
                .map(|unit| unit.id)
                .collect();
            let before = self.bridge.controller.state.selected_units.len();
            self.bridge
                .controller
                .state
                .selected_units
                .retain(|id| valid.contains(id));
            if self.bridge.controller.state.selected_units.len() != before {
                self.bridge.emit_selection();
            }
        }
        // G05 T4b hint triggers while a key is held (`boost`,
        // `rebuild_selecting`).
        self.bridge.refresh_hints(&self.bindings);
        // G04 T4 `CommandModeObjective`: a selected unit with a live order
        // satisfies the objective each frame (`MapObjectives.java:671-675`).
        let selected: Vec<i32> = self
            .bridge
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        if let Some(mut host) = self.bridge.host.clone() {
            let sat = host.bind().any_unit_has_command(&selected);
            let current = host.bind().command_mode_satisfied();
            if current != sat {
                host.bind_mut().set_command_mode_satisfied(sat);
            }
        }
        self.drain_effects();
        // M3: drive the mobile gesture detector's long-press timer. The touch
        // stream itself is folded in by the `mobile_*` callbacks (plan 14).
        self.mobile_time += delta;
        if let Some(event) = self.mobile.tick(self.mobile_time) {
            // `longPressTask` (`InputHandler.add`): a held finger enters line
            // mode / payload targeting.
            self.fold_mobile_gesture(event);
        }
        self.mobile.update_transitions();
        // `MobileInput.update` lineMode branch: the drag follows the held
        // finger even without a new drag event (screen-edge auto-pan).
        self.update_mobile_line();
    }
}

#[godot_api]
impl MindInput {
    /// Emitted when a registered binding fires (`action` is the binding name).
    #[signal]
    fn hotkey(action: GString);

    /// Emitted when the RTS selection changes (JSON array of unit ids).
    #[signal]
    fn selection_changed(units_json: GString);
}
