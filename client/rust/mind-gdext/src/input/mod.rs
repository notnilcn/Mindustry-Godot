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

pub mod bindings;
pub mod desktop;
pub mod events;
pub mod gesture;
pub mod mobile;

use std::collections::HashSet;

use godot::classes::notify::NodeNotification;
use godot::classes::{Control, INode, Node};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::input::{
    BindingState, BindingValue, FocusState, InputLocks, KeyBindTable, RawEvent, ids,
    key_display_name,
};
use mind_core::io::{NativeFs, Paths, SettingsStore};
use mind_core::world::config::ConfigValue;

use crate::camera::MindCamera2D;
use crate::sim_host::MindSimHost;

use desktop::{DesktopBridge, Effect};

/// Path of the block-config overlay in the UI root.
const BLOCK_CONFIG_PATH: &str = "../Ui/UiRoot/OverlayLayer/block_config";
/// Path of the block-inventory overlay in the UI root.
const BLOCK_INVENTORY_PATH: &str = "../Ui/UiRoot/OverlayLayer/block_inventory";
/// Path of a HUD fragment under the UI root (`minimap`, `console`, ...).
const HUD_FRAGMENT_DIR: &str = "../Ui/UiRoot/HudGroup";

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
    /// Live desktop controller bridge (placement, hotkeys, RTS).
    bridge: DesktopBridge,
    /// Touch pointers owned by a STOP-filter control (no world gestures).
    ui_pointers: HashSet<i32>,
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
            bridge: DesktopBridge::new(),
            ui_pointers: HashSet::new(),
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
        }
        for raw in translated {
            // Touch gestures drive the mobile bridge directly (plan-14 mobile
            // HUD is out of scope; pan/zoom already work without it).
            if self.route_touch(&raw) {
                continue;
            }
            self.refresh_command_mode_hold(&raw);
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
        for event in pending {
            self.refresh_command_mode_hold(&event);
            self.bridge.handle(&self.bindings, event);
        }
        // `DesktopInput.update` applies the command-mode branch every frame, so
        // state changes that arrive without a key edge (block selection from a
        // fragment, focus changes) still close the gate.
        self.bridge.update_command_mode(&self.bindings, None);
        self.drain_effects();
        // M3: drive the mobile gesture detector's long-press timer. The touch
        // stream itself is folded in by the `mobile_*` callbacks (plan 14).
        self.mobile_time += delta;
        let _ = self.mobile.tick(self.mobile_time);
        self.mobile.update_transitions();
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

    /// Rebuilds the client-local binding state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). The binding state
    /// is overwritten, so a re-run never duplicates.
    fn bootstrap(&mut self) {
        self.bindings = bindings::load();
        self.bridge
            .set_command_mode_hold(Self::read_command_mode_hold());
        // Scene wiring (tscn-first): the spine declares SimHost as a sibling of
        // Input and Camera2D under World.
        self.bridge.host = self.base().try_get_node_as::<MindSimHost>("../SimHost");
        self.bridge.camera = self
            .base()
            .try_get_node_as::<MindCamera2D>("../World/Camera2D");
        self.bridge.refresh_items();
        self.sync_selected_block();
        self.load_catalog();
        log::info!(
            "MindInput ready ({} keybinds, {} rebinds)",
            KeyBindTable::len(),
            self.rebind_count()
        );
    }

    /// Adopts `MindSimHost.selected_block()` as the live placement block.
    fn sync_selected_block(&mut self) {
        let Some(host) = self.bridge.host.clone() else {
            return;
        };
        let name = host.bind().selected_block().to_string();
        let guard = host.bind();
        if let Some(content) = guard.content_registry()
            && let Some(id) = content.block_id(&name)
        {
            self.bridge.controller.state.select_block(Some(id));
        }
    }

    /// Loads the buildable-block catalogue for the `1`..`0` block-select binds.
    fn load_catalog(&mut self) {
        let catalog = mind_core::ui::campaign::block_catalog();
        let categories: Vec<Vec<String>> = catalog
            .categories
            .iter()
            .map(|category| {
                category
                    .blocks
                    .iter()
                    .map(|block| block.name.clone())
                    .collect()
            })
            .collect();
        self.bridge.set_catalog(categories);
    }

    /// `settings.getBool("commandmodehold", true)` from the plan-04 store.
    fn read_command_mode_hold() -> bool {
        let store = SettingsStore::load(&NativeFs, &Paths::resolve(None));
        store.get_bool("commandmodehold", true)
    }

    /// Re-reads the hold setting when a command-mode key edge arrives so a live
    /// settings-dialog change applies on the next tap (`MindUi.settings_set`
    /// persists the store).
    fn refresh_command_mode_hold(&mut self, raw: &RawEvent) {
        let code = match raw {
            RawEvent::KeyDown { code } | RawEvent::KeyUp { code } => code,
            _ => return,
        };
        if self.bindings.name(ids::COMMAND_MODE) != Some(code.as_str()) {
            return;
        }
        self.bridge
            .set_command_mode_hold(Self::read_command_mode_hold());
    }

    /// Whether a STOP-filter `Control` owns the given event position
    /// (upstream `Core.scene.hasMouse()`). PASS controls fall through to
    /// `_unhandled_input`, so only STOP controls (or a STOP ancestor) block the
    /// world.
    fn ui_captures_at(&self, x: f32, y: f32) -> bool {
        let tree = self.base().get_tree();
        let root = tree.get_root();
        // `owned = false`: the default `owned = true` finds nothing under the
        // ownerless root Window, so runtime-created Controls (`Button.new()` in
        // the UI factories) would be missed and their clicks consumed as world
        // presses instead of reaching `_gui_input`.
        let controls = root.find_children_ex("*").owned(false).done();
        let point = Vector2::new(x, y);
        // Later siblings are drawn on top: walk in reverse tree order.
        for index in (0..controls.len()).rev() {
            let Some(node) = controls.get(index) else {
                continue;
            };
            let Ok(control) = node.clone().try_cast::<Control>() else {
                continue;
            };
            if !control.is_visible_in_tree() {
                continue;
            }
            if matches!(control.get("mouse_filter").try_to::<i64>(), Ok(2)) {
                continue;
            }
            if !control.get_global_rect().contains_point(point) {
                continue;
            }
            // `mouse_filter`: 0 = STOP, 1 = PASS, 2 = IGNORE.
            if matches!(control.get("mouse_filter").try_to::<i64>(), Ok(0)) {
                return true;
            }
            let mut parent = control.get_parent();
            while let Some(current) = parent {
                if let Ok(parent_control) = current.clone().try_cast::<Control>() {
                    if !parent_control.is_visible_in_tree() {
                        break;
                    }
                    match parent_control.get("mouse_filter").try_to::<i64>() {
                        Ok(0) => return true,
                        Ok(2) => break,
                        _ => {}
                    }
                }
                parent = current.get_parent();
            }
        }
        false
    }

    /// Consumes the current input dispatch (prevents `unhandled_input`).
    fn mark_input_handled(&self) {
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.call("set_input_as_handled", &[]);
        }
    }

    /// Whether a `LineEdit`/`TextEdit` owns keyboard focus
    /// (`Core.scene.hasField()`): field typing must not fire gameplay binds.
    fn text_field_focused(&self) -> bool {
        let Some(mut viewport) = self.base().get_viewport() else {
            return false;
        };
        let owner = viewport.call("gui_get_focus_owner", &[]);
        let Ok(control) = owner.try_to::<Gd<Control>>() else {
            return false;
        };
        control.is_class("LineEdit") || control.is_class("TextEdit")
    }

    /// Whether any dialog is open (`scene.hasDialog()`): gameplay binds and
    /// camera pan/zoom are gated off.
    fn dialog_open(&self) -> bool {
        let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") else {
            return false;
        };
        if !ui.has_method("dialog_stack") {
            return false;
        }
        ui.call("dialog_stack", &[])
            .try_to::<PackedStringArray>()
            .map(|stack| !stack.is_empty())
            .unwrap_or(false)
    }

    /// Folds a Godot touch event into the mobile gesture bridge and applies the
    /// resulting camera pan/zoom. Returns whether the event was a touch event.
    fn route_touch(&mut self, raw: &RawEvent) -> bool {
        let time = self.mobile_time;
        let events = match raw {
            RawEvent::TouchDown { pointer, x, y } => {
                if self.ui_captures_at(*x, *y) {
                    self.ui_pointers.insert(*pointer);
                    return true;
                }
                self.mobile.touch_down(time, *x, *y, *pointer)
            }
            RawEvent::TouchMove { pointer, x, y } => {
                if self.ui_pointers.contains(pointer) {
                    return true;
                }
                self.mobile.touch_drag(time, *x, *y, *pointer)
            }
            RawEvent::TouchUp { pointer, x, y } => {
                if self.ui_pointers.remove(pointer) {
                    return true;
                }
                self.mobile.touch_up(time, *x, *y, *pointer)
            }
            _ => return false,
        };
        self.apply_gestures(&events);
        true
    }

    /// Applies gesture events that map directly to the camera rig (mobile pan
    /// and pinch zoom); taps/long-presses stay with the plan-14 mobile HUD.
    fn apply_gestures(&mut self, events: &[gesture::GestureEvent]) {
        let Some(camera) = self.bridge.camera.clone() else {
            return;
        };
        for event in events {
            match event {
                gesture::GestureEvent::Pan {
                    delta_x, delta_y, ..
                } => {
                    camera
                        .clone()
                        .bind_mut()
                        .mobile_pan(*delta_x as f64, *delta_y as f64);
                }
                gesture::GestureEvent::Zoom {
                    initial_distance,
                    distance,
                } if *initial_distance > 0.0 => {
                    let amount = (*distance / *initial_distance - 1.0) * 4.0;
                    camera.clone().bind_mut().zoom_by(amount as f64);
                }
                _ => {}
            }
        }
    }

    /// Resolves UI effects produced by the desktop bridge.
    fn drain_effects(&mut self) {
        let effects = self.bridge.take_effects();
        for effect in effects {
            match effect {
                Effect::OpenDialog { name, ctx } => {
                    if let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") {
                        let _ = ui.call(
                            "open_dialog",
                            &[
                                GString::from(name.as_str()).to_variant(),
                                GString::from(ctx.as_str()).to_variant(),
                            ],
                        );
                    }
                }
                Effect::ToggleFragment(name) => {
                    let path = format!("{HUD_FRAGMENT_DIR}/{name}");
                    let Some(mut node) = self.base().try_get_node_as::<Node>(&path) else {
                        continue;
                    };
                    if node.has_method("toggle") {
                        let _ = node.call("toggle", &[]);
                    } else {
                        let visible = node.get("visible").try_to::<bool>().unwrap_or(false);
                        node.set("visible", &(!visible).to_variant());
                    }
                }
                Effect::OpenBlockConfig { screen, spec, .. } => {
                    let Some(mut node) = self.base().try_get_node_as::<Node>(BLOCK_CONFIG_PATH)
                    else {
                        continue;
                    };
                    let _ = node.call(
                        "configure",
                        &[
                            Vector2::new(screen.0, screen.1).to_variant(),
                            GString::from(spec.as_str()).to_variant(),
                        ],
                    );
                }
                Effect::OpenBlockInventory {
                    x,
                    y,
                    screen,
                    items,
                } => {
                    let Some(mut node) = self.base().try_get_node_as::<Node>(BLOCK_INVENTORY_PATH)
                    else {
                        continue;
                    };
                    node.set("position", &Vector2::new(screen.0, screen.1).to_variant());
                    let _ = node.call(
                        "open_at",
                        &[
                            Vector2i::new(x, y).to_variant(),
                            GString::from(items.as_str()).to_variant(),
                        ],
                    );
                }
                Effect::Screenshot => {
                    if let Some(mut host) = self.bridge.host.clone() {
                        let _ = host.call(
                            "capture",
                            &[GString::from("user://screenshot.png").to_variant()],
                        );
                    }
                }
                Effect::Hotkey(action) => {
                    let _ = self
                        .base_mut()
                        .emit_signal("hotkey", &[GString::from(action.as_str()).to_variant()]);
                }
                Effect::SelectionChanged(json) => {
                    let _ = self.base_mut().emit_signal(
                        "selection_changed",
                        &[GString::from(json.as_str()).to_variant()],
                    );
                }
            }
        }
    }

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

    /// Rebinds `name` from a Godot key display string (`OS.get_keycode_string`),
    /// normalizing it to the registry name (GDScript capture path).
    #[func]
    pub fn rebind_key(&mut self, name: GString, godot_text: GString) -> bool {
        let code = events::normalize_key_name(&godot_text.to_string());
        if bindings::rebind(&mut self.bindings, &name.to_string(), &code) {
            bindings::save(&self.bindings);
            true
        } else {
            false
        }
    }

    /// Resets one binding to its upstream default and persists it.
    #[func]
    pub fn reset_keybind(&mut self, name: GString) -> bool {
        let Some(id) = self.bind_id(&name.to_string()) else {
            return false;
        };
        *self.bindings.value_mut(id) = mind_core::input::binding::default_value_for(id);
        bindings::save(&self.bindings);
        true
    }

    /// Resets every binding to its upstream default and persists it.
    #[func]
    pub fn reset_keybinds(&mut self) {
        self.bindings.reset();
        bindings::save(&self.bindings);
    }

    /// Unbinds one binding and persists it (`Binding.unset`).
    #[func]
    pub fn unbind_keybind(&mut self, name: GString) -> bool {
        let Some(id) = self.bind_id(&name.to_string()) else {
            return false;
        };
        self.bindings.clear(id);
        bindings::save(&self.bindings);
        true
    }

    /// Numeric id for a registry binding name.
    fn bind_id(&self, name: &str) -> Option<u16> {
        KeyBindTable::all()
            .iter()
            .position(|bind| bind.name == name)
            .map(|index| index as u16)
    }

    /// The full binding registry + current values as JSON. `display` and
    /// `negativeDisplay` are the Arc `KeyCode.getName()` strings the keybind
    /// dialog renders; `default` drives the per-row Reset disabled state.
    #[func]
    pub fn keybinds_json(&self) -> GString {
        let mut list = Vec::new();
        for (index, bind) in KeyBindTable::all().iter().enumerate() {
            let id = index as u16;
            let (negative, positive) = match self.bindings.value(id) {
                BindingValue::Axis { negative, positive } => {
                    (negative.as_deref(), positive.as_deref())
                }
                other => (None, other.display_name()),
            };
            list.push(serde_json::json!({
                "name": bind.name,
                "category": bind.category.map(|c| c.name()),
                "axis": bind.kind == mind_core::input::KeyKind::Axis,
                "bundle": bind.bundle_key(),
                "value": self.bindings.name(id),
                "display": positive.map(key_display_name),
                "negativeDisplay": negative.map(key_display_name),
                "default": self.bindings.is_default(id),
            }));
        }
        GString::from(&serde_json::Value::Array(list).to_string())
    }

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

    /// Feeds the selectable-unit list (`[{id,type,x,y,team,commandable}]`).
    #[func]
    pub fn set_selectable_units_json(&mut self, json: GString) -> i64 {
        self.bridge.set_selectable_units_json(&json.to_string()) as i64
    }

    /// Current RTS selection as a JSON id array.
    #[func]
    pub fn selection_json(&self) -> GString {
        let ids: Vec<i32> = self
            .bridge
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        GString::from(
            serde_json::Value::Array(ids.into_iter().map(serde_json::Value::from).collect())
                .to_string()
                .as_str(),
        )
    }

    /// Sets a unit stance on the current selection (`setUnitStance`).
    #[func]
    pub fn set_unit_stance(&mut self, stance: i64, enabled: bool) {
        self.bridge.set_stance(stance.max(0) as u16, enabled);
    }

    /// Selects a buildable block by name and syncs the placement state.
    #[func]
    pub fn select_block_by_name(&mut self, name: GString) -> bool {
        let Some(mut host) = self.bridge.host.clone() else {
            return false;
        };
        let name_text = name.to_string();
        if !host.bind_mut().select_block(name) {
            return false;
        }
        let guard = host.bind();
        if let Some(content) = guard.content_registry()
            && let Some(id) = content.block_id(&name_text)
        {
            self.bridge.controller.state.select_block(Some(id));
        }
        true
    }

    /// Rotates the pending placement 90° (`Binding.rotate`).
    #[func]
    pub fn rotate_placement(&mut self) {
        self.bridge.rotate_placement(&self.bindings);
    }

    /// Rotates the placed building under the cursor or the pending placement.
    #[func]
    pub fn rotate_placed(&mut self) {
        self.bridge.rotate_under_cursor(&self.bindings);
    }

    /// Clears the selected block (`Binding.clear_building`).
    #[func]
    pub fn clear_building(&mut self) {
        self.bridge.controller.state.select_block(None);
    }

    /// Toggles placement building pause (`Binding.pause_building`).
    #[func]
    pub fn set_building_paused(&mut self, paused: bool) {
        self.bridge.controller.state.is_building = !paused;
    }

    /// Sets the active catalogue category for the `1`..`0` binds.
    #[func]
    pub fn set_catalog_category(&mut self, index: i64) {
        self.bridge.catalog_category = index.max(0) as usize;
        self.bridge.select_catalog_block(0);
    }

    /// Submits a building config (`tileConfig`) through the sim command queue.
    ///
    /// `kind` is `none`/`string`/`content`/`item`/`number`/`bool`; content-ish
    /// kinds carry the content name in `value`.
    #[func]
    pub fn configure_building(&mut self, x: i32, y: i32, kind: GString, value: GString) -> bool {
        let kind = kind.to_string();
        let value = value.to_string();
        let config = match kind.as_str() {
            "none" => ConfigValue::None,
            "number" => match value.parse::<f64>() {
                Ok(number) => ConfigValue::Number(number),
                Err(_) => return false,
            },
            "bool" => ConfigValue::Bool(value == "true" || value == "1"),
            _ => ConfigValue::String(value),
        };
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        self.bridge
            .emit_action(mind_core::input::RemoteAction::Configure {
                x,
                y,
                value: config,
            });
        true
    }

    /// Withdraws/deposits an item at a block (`tryDropItems`/`requestItem`).
    #[func]
    pub fn transfer_item(&mut self, x: i32, y: i32, item: GString, amount: i64, deposit: bool) {
        self.bridge
            .transfer_item(x, y, &item.to_string(), amount as i32, deposit);
    }

    /// Payload pickup (`[`): `requestUnitPayload` at the cursor.
    #[func]
    pub fn pickup_payload(&mut self) {
        self.bridge.pickup_payload();
    }

    /// Payload drop (`]`): `requestDropPayload` at the cursor.
    #[func]
    pub fn drop_payload(&mut self) {
        self.bridge.drop_payload();
    }

    /// Latches `keybinds.json` into `BindingState` (rebind dialogs call this).
    #[func]
    pub fn reload_bindings(&mut self) {
        self.bindings = bindings::load();
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
