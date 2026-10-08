// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DesktopBridge` — live desktop interaction for `MindInput` (plan 15 §3.1/§3.5).
//!
//! Translates captured [`RawEvent`]s through the pure `mind_core::input`
//! controller into [`SimCommand`]s enqueued on `MindSimHost` (they apply at the
//! next tick start, like the relay path). All branch logic lives in `mind-core`;
//! this file is Godot node plumbing only. UI-affecting actions are reported as
//! [`Effect`]s so `MindInput` can resolve the scene nodes.

use std::collections::{HashMap, HashSet};

use godot::classes::Time;
use godot::obj::Singleton;
use godot::prelude::*;
use smallvec::SmallVec;

use mind_core::content::{BlockId, BlockKind, ContentRegistry, ContentType};
use mind_core::determinism::SimCommand;
use mind_core::input::{
    BindingState, BindingValue, ClientPlan, DesktopController, InventoryKind, KeyBindTable,
    PayloadAction, PlaceMode, PlacementWorld, RawEvent, RemoteAction, SelectRect, SelectableUnit,
    ids, line::LineBlock, line::LineParams, select_typed_units, select_unit_tap, select_units_rect,
};
use mind_core::world::TilePos;
use mind_core::world::build::can_replace;

use crate::camera::MindCamera2D;
use crate::sim_host::MindSimHost;

/// Maximum tiles one drag action emits (defensive; previews cap at `MAX_LENGTH`).
const DRAG_TILE_CAP: usize = 256;
/// `Build.validPlace` hit radius for tap selection (world pixels).
const UNIT_TAP_RADIUS: f32 = 11.0;
/// Double-tap window for `selectTypedUnits` (`Time.timeSinceMillis < 300`).
const UNIT_TAP_INTERVAL_MS: u64 = 300;

/// One world interaction in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragKind {
    /// Left-drag placement line.
    Place,
    /// Right-drag deconstruction rectangle.
    Break,
    /// Command-mode selection rectangle.
    SelectRect,
}

/// The active pointer drag.
#[derive(Debug, Clone, Copy)]
struct Drag {
    kind: DragKind,
    start: (i32, i32),
    last: (i32, i32),
    moved: bool,
}

/// A UI-side action the shell must resolve against the scene tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// `MindUi.open_dialog(name, ctx)`.
    OpenDialog {
        /// Registered dialog name.
        name: String,
        /// Context JSON (may be empty).
        ctx: String,
    },
    /// Toggle a HUD fragment (console/minimap): `toggle()` when present,
    /// otherwise `visible = !visible`.
    ToggleFragment(String),
    /// Open the block-config fragment.
    OpenBlockConfig {
        /// Tile x.
        x: i32,
        /// Tile y.
        y: i32,
        /// Viewport anchor (click position).
        screen: (f32, f32),
        /// Config-spec JSON consumed by `block_config_fragment.gd`.
        spec: String,
    },
    /// Open the block-inventory fragment.
    OpenBlockInventory {
        /// Tile x.
        x: i32,
        /// Tile y.
        y: i32,
        /// Viewport anchor (click position).
        screen: (f32, f32),
        /// `{item_name: amount}` JSON.
        items: String,
    },
    /// Capture the running viewport.
    Screenshot,
    /// A binding fired (probe/UI signal).
    Hotkey(String),
    /// The RTS selection changed; payload is a JSON id array.
    SelectionChanged(String),
}

/// Read-only [`PlacementWorld`] over the live sim grid/content.
///
/// `valid_place` defers to the in-bounds test: the simulation validates and
/// rejects invalid placements deterministically when the command applies.
struct HostWorld<'a> {
    grid: &'a mind_core::world::WorldGrid,
    content: Option<&'a ContentRegistry>,
}

impl PlacementWorld for HostWorld<'_> {
    fn in_bounds(&self, x: i32, y: i32) -> bool {
        self.grid.tiles.in_bounds(x, y)
    }

    fn block_at(&self, x: i32, y: i32) -> BlockId {
        let Ok(x) = i16::try_from(x) else {
            return BlockId::AIR;
        };
        let Ok(y) = i16::try_from(y) else {
            return BlockId::AIR;
        };
        self.grid
            .block_at(TilePos::new(x, y))
            .unwrap_or(BlockId::AIR)
    }

    fn floor_deep(&self, _x: i32, _y: i32) -> bool {
        false
    }

    fn always_replace(&self, x: i32, y: i32) -> bool {
        self.block_at(x, y) == BlockId::AIR
    }

    fn can_replace(&self, target: BlockId, other: BlockId) -> bool {
        let Some(content) = self.content else {
            return false;
        };
        match (content.block(target), content.block(other)) {
            (Some(target), Some(other)) => can_replace(target, other),
            _ => false,
        }
    }

    fn valid_place(&self, _block: BlockId, x: i32, y: i32, _rotation: u8) -> bool {
        self.in_bounds(x, y)
    }
}

/// Parses a selectable-unit snapshot (`[{id,type,x,y,team,commandable}]`).
fn parse_selectable_units(json: &str) -> Vec<SelectableUnit> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(list) = value.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            Some(SelectableUnit {
                id: entry.get("id")?.as_i64()? as i32,
                type_id: entry.get("type")?.as_i64()? as i32,
                x: entry.get("x")?.as_f64()? as f32,
                y: entry.get("y")?.as_f64()? as f32,
                team: entry.get("team")?.as_u64()? as u8,
                commandable: entry
                    .get("commandable")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
            })
        })
        .collect()
}

/// Live desktop interaction state (kept out of `MindInput`'s Godot glue).
pub struct DesktopBridge {
    /// Pure controller: `InputState` + `BuildQueue` + `ActionBatcher`.
    pub controller: DesktopController,
    /// Held key name set (binding polling without a Godot `Key` table).
    pressed: HashSet<String>,
    /// Last pointer position in viewport pixels.
    pub mouse: (f32, f32),
    /// Active drag.
    drag: Option<Drag>,
    /// Buildable block names per catalogue category (1-0 block select).
    catalog: Vec<Vec<String>>,
    /// Currently visible catalogue category (synced by the placement fragment).
    pub catalog_category: usize,
    /// Scene sim host (world reads + command enqueue).
    pub host: Option<Gd<MindSimHost>>,
    /// Scene camera (screen↔tile, zoom, recenter).
    pub camera: Option<Gd<MindCamera2D>>,
    /// Selectable units fed by the sim/GDScript (`set_selectable_units_json`).
    pub selectable: Vec<SelectableUnit>,
    /// Last fired binding name (probe).
    pub last_action: String,
    /// Fired binding count (probe).
    pub action_count: u64,
    /// Pending UI effects for `MindInput` to resolve.
    effects: Vec<Effect>,
    /// Last placement-line preview (`input_state_json`).
    pub preview_json: String,
    /// Item name→dense id cache for configure/inventory commands.
    item_ids: HashMap<String, u16>,
    /// Last unit-tap timestamp (`Time.millis`, double-tap typed select).
    last_tap_ms: u64,
    /// Last tapped unit type (double-tap typed select).
    last_tap_type: Option<i32>,
    /// A LineEdit/TextEdit owns keyboard focus (`scene.hasField()`): key edges
    /// update held state but fire no bindings and the camera does not pan.
    pub text_focus: bool,
    /// A dialog is open (`scene.hasDialog()`): key edges fire no bindings and
    /// the camera does not pan/zoom.
    pub ui_dialog: bool,
    /// `settings.getBool("commandmodehold", true)`: command mode follows the
    /// held command key when on (upstream default) and toggles on a key press
    /// when off.
    command_mode_hold: bool,
}

impl Default for DesktopBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopBridge {
    /// Empty bridge (bindings live in `MindInput`).
    pub fn new() -> Self {
        Self {
            controller: DesktopController::new(0),
            pressed: HashSet::new(),
            mouse: (0.0, 0.0),
            drag: None,
            catalog: Vec::new(),
            catalog_category: 0,
            host: None,
            camera: None,
            selectable: Vec::new(),
            last_action: String::new(),
            action_count: 0,
            effects: Vec::new(),
            preview_json: String::from("[]"),
            item_ids: HashMap::new(),
            last_tap_ms: 0,
            last_tap_type: None,
            text_focus: false,
            ui_dialog: false,
            command_mode_hold: true,
        }
    }

    /// Replaces the catalogue from `mind_core::ui::campaign::block_catalog`.
    pub fn set_catalog(&mut self, catalog: Vec<Vec<String>>) {
        self.catalog = catalog;
        if self.catalog_category >= self.catalog.len() {
            self.catalog_category = 0;
        }
    }

    /// Cache item name→dense id pairs for `Configure`/`Inventory` commands.
    pub fn refresh_items(&mut self) {
        self.item_ids.clear();
        let Some(host) = self.host.clone() else {
            return;
        };
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return;
        };
        for entry in content.entries(ContentType::Item) {
            if let Some(name) = entry.name {
                self.item_ids.insert(name.to_owned(), entry.id);
            }
        }
    }

    /// Replaces the selectable-unit list from JSON; returns the parsed count.
    pub fn set_selectable_units_json(&mut self, json: &str) -> usize {
        self.selectable = parse_selectable_units(json);
        self.selectable.len()
    }

    /// Drops held keys (focus loss must not leave the camera panning).
    pub fn clear_keys(&mut self) {
        self.pressed.clear();
    }

    /// Removes and returns the pending effects.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

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

    /// Whether an axis/key binding drives the wheel (`scroll`).
    fn axis_is_scroll(bindings: &BindingState, id: u16) -> bool {
        match bindings.value(id) {
            BindingValue::Axis { negative, positive } => {
                negative.as_deref() == Some("scroll") || positive.as_deref() == Some("scroll")
            }
            BindingValue::Key(name) => name == "scroll",
            BindingValue::Unset => false,
        }
    }

    /// Whether the negative direction of an axis binding is currently held.
    fn axis_negative(&self, bindings: &BindingState, id: u16) -> bool {
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
        let mut command_edge: Option<bool> = None;
        match event {
            RawEvent::KeyDown { code } => {
                let fresh = self.pressed.insert(code.clone());
                if bindings.name(ids::COMMAND_MODE) == Some(code.as_str()) {
                    command_edge = Some(true);
                }
                if fresh && !self.text_focus && !self.ui_dialog {
                    self.key_down_event(bindings, &code);
                }
            }
            RawEvent::KeyUp { code } => {
                self.pressed.remove(&code);
                if bindings.name(ids::COMMAND_MODE) == Some(code.as_str()) {
                    command_edge = Some(false);
                }
            }
            RawEvent::MouseMove { x, y } => {
                self.mouse = (x, y);
                self.mouse_move(bindings);
            }
            RawEvent::MouseButton { button, down, x, y } => {
                self.mouse = (x, y);
                self.mouse_button(bindings, &button, down);
            }
            RawEvent::Scroll { y, .. } => self.scroll(bindings, y),
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
            self.dispatch_action(bindings, name);
        }
    }

    /// `DesktopInput.update` command-mode gate: only while no placement block is
    /// selected and no UI field/dialog owns input. The upstream boost-conflict
    /// guard (a live player unit that can boost sharing the key) needs the
    /// possessed-unit feed, which the bridge does not have yet.
    fn command_mode_allowed(&self) -> bool {
        self.controller.state.block.is_none() && !self.text_focus && !self.ui_dialog
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
        if !self.command_mode_allowed() {
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

    /// Runs one named binding (`Binding.name`).
    fn dispatch_action(&mut self, bindings: &BindingState, name: &'static str) {
        match name {
            "respawn" => self.recenter_camera(),
            "rotateplaced" => self.rotate_under_cursor(bindings),
            "rotate" => {
                // Axis edges (`rotate` defaults to the wheel).
                let step = if self.axis_negative(bindings, ids::ROTATE) {
                    -1
                } else {
                    1
                };
                self.rotate_placement_step(bindings, step);
            }
            "zoom" => {
                let amount = if self.axis_negative(bindings, ids::ZOOM) {
                    -1.0
                } else {
                    1.0
                };
                self.zoom(amount);
                self.fire_hotkey("zoom");
            }
            "clear_building" => {
                self.controller.state.select_block(None);
                self.fire_hotkey(name);
            }
            "pause_building" => {
                self.controller.state.is_building = !self.controller.state.is_building;
                self.fire_hotkey(name);
            }
            "block_select_01" | "block_select_02" | "block_select_03" | "block_select_04"
            | "block_select_05" | "block_select_06" | "block_select_07" | "block_select_08"
            | "block_select_09" | "block_select_10" => {
                let index = name
                    .rsplit('_')
                    .next()
                    .and_then(|suffix| suffix.parse::<usize>().ok())
                    .map(|n| n.saturating_sub(1))
                    .unwrap_or(0);
                if self.controller.state.command_mode {
                    self.control_group(bindings, index);
                } else {
                    self.select_catalog_block(index);
                }
            }
            "command_mode" => {
                // `input.keyTap` is the press edge (`justPressed`); hold mode is
                // applied per frame by `update_command_mode`.
                if !self.command_mode_hold && self.command_mode_allowed() {
                    self.toggle_command_mode();
                }
            }
            "select_all_units" | "select_all_unit_transport" => {
                self.select_all_units(bindings, false);
            }
            "select_all_unit_factories" => self.select_all_units(bindings, true),
            "cancel_orders" | "deselect" => {
                self.controller.state.selected_units.clear();
                self.controller.state.command_buildings.clear();
                self.emit_selection();
            }
            "pickupCargo" => self.payload(PayloadAction::PickupUnit),
            "dropCargo" => self.payload(PayloadAction::Drop),
            "unit_stance_hold_fire" => self.set_stance(0, true),
            "unit_stance_pursue_target" => self.set_stance(1, true),
            "unit_stance_patrol" => self.set_stance(2, true),
            "unit_stance_ram" => self.set_stance(3, true),
            "unit_stance_boost" => self.set_stance(4, true),
            "unit_stance_hold_position" => self.set_stance(5, true),
            "unit_command_move" => self.command_units(0),
            "unit_command_repair" => self.command_units(1),
            "unit_command_rebuild" => self.command_units(2),
            "unit_command_assist" => self.command_units(3),
            "unit_command_mine" => self.command_units(4),
            "minimap" => {
                self.effects
                    .push(Effect::ToggleFragment(String::from("minimap")));
                self.fire_hotkey(name);
            }
            "console" => {
                self.effects
                    .push(Effect::ToggleFragment(String::from("console")));
                self.fire_hotkey(name);
            }
            "research" => {
                self.open_dialog("research", "");
                self.fire_hotkey(name);
            }
            "planet_map" => {
                self.open_dialog("planet", "");
                self.fire_hotkey(name);
            }
            "schematic_menu" => {
                self.open_dialog("schematics", "");
                self.fire_hotkey(name);
            }
            "block_info" => self.block_info_hotkey(),
            "pick" => self.pick_block(),
            "category_prev" | "category_next" => {
                // The placement fragment owns the category rail; it reacts to
                // the hotkey signal and pushes the new index back.
                self.fire_hotkey(name);
            }
            "pause" => {
                if let Some(mut host) = self.host.clone() {
                    let paused = host.bind().is_paused();
                    host.bind_mut().set_paused(!paused);
                }
                self.fire_hotkey(name);
            }
            "screenshot" => self.effects.push(Effect::Screenshot),
            "detach_camera" => {
                if let Some(mut camera) = self.camera.clone() {
                    let following = camera.bind().is_following();
                    camera.bind_mut().set_follow(!following);
                }
                self.fire_hotkey(name);
            }
            _ => {}
        }
    }

    /// `commandMode` toggle (`DesktopInput.update`).
    pub fn toggle_command_mode(&mut self) {
        self.controller.state.command_mode = !self.controller.state.command_mode;
        if !self.controller.state.command_mode {
            self.controller.state.selected_units.clear();
            self.controller.state.command_buildings.clear();
            self.controller.state.command_rect = None;
            self.emit_selection();
        }
        self.fire_hotkey("command_mode");
    }

    /// Selects the Nth buildable block of the active catalogue category.
    pub fn select_catalog_block(&mut self, index: usize) {
        let Some(category) = self.catalog.get(self.catalog_category) else {
            return;
        };
        let Some(name) = category.get(index).cloned() else {
            return;
        };
        let Some(mut host) = self.host.clone() else {
            return;
        };
        if host.bind_mut().select_block(GString::from(&name)) {
            let guard = host.bind();
            if let Some(content) = guard.content_registry()
                && let Some(id) = content.block_id(&name)
            {
                self.controller.state.select_block(Some(id));
            }
            self.fire_hotkey("block_select");
        }
    }

    /// `createControlGroup`/`recallControlGroup` for a digit binding.
    fn control_group(&mut self, bindings: &BindingState, index: usize) {
        let creating = self.key_down(bindings, ids::CREATE_CONTROL_GROUP);
        let now_ms = Time::singleton().get_ticks_msec();
        if creating {
            self.controller.create_control_group(index, true);
        } else if self.controller.recall_control_group(index, now_ms) {
            self.center_on_selection();
        }
        self.emit_selection();
    }

    fn center_on_selection(&mut self) {
        if self.controller.state.selected_units.is_empty() {
            return;
        }
        let mut sum = (0.0f32, 0.0f32);
        let mut count = 0.0f32;
        for id in &self.controller.state.selected_units {
            if let Some(unit) = self.selectable.iter().find(|unit| unit.id == *id) {
                sum.0 += unit.x;
                sum.1 += unit.y;
                count += 1.0;
            }
        }
        if count <= 0.0 {
            return;
        }
        if let Some(mut camera) = self.camera.clone() {
            camera
                .bind_mut()
                .pan_to((sum.0 / count) as f64, (sum.1 / count) as f64);
        }
    }

    fn select_all_units(&mut self, bindings: &BindingState, factories: bool) {
        // `selectAllUnits` only runs in command mode upstream; enabling it here
        // gives the player a direct entry point without first holding the
        // command-mode key.
        if !self.controller.state.command_mode {
            self.toggle_command_mode();
        }
        let screens = self.key_down(bindings, ids::SELECT_ACROSS_SCREEN);
        if factories {
            // Command buildings come from the sim selection feed.
            self.controller.state.command_buildings.clear();
            self.controller.state.selected_units.clear();
            self.emit_selection();
            return;
        }
        let rect = if screens {
            self.viewport_world_rect()
        } else {
            SelectRect {
                x: f32::NEG_INFINITY,
                y: f32::NEG_INFINITY,
                w: f32::INFINITY,
                h: f32::INFINITY,
            }
        };
        let mut out = SmallVec::new();
        select_units_rect(&self.selectable, 0, rect, &mut out);
        self.controller.state.selected_units = out;
        self.controller.state.command_buildings.clear();
        self.emit_selection();
    }

    /// Viewport rectangle in world pixels (selection across screen).
    fn viewport_world_rect(&self) -> SelectRect {
        let Some(camera) = self.camera.clone() else {
            return SelectRect::default();
        };
        let size = camera.bind().viewport_size();
        let (x0, y0) = camera_world_corner(&camera, 0.0, 0.0);
        let (x1, y1) = camera_world_corner(&camera, size.x, size.y);
        SelectRect {
            x: x0.min(x1),
            y: y0.min(y1),
            w: (x1 - x0).abs(),
            h: (y1 - y0).abs(),
        }
    }

    /// `commandUnits` with an explicit command id over the current selection.
    fn command_units(&mut self, command: u16) {
        if self.controller.state.selected_units.is_empty() {
            return;
        }
        let units: SmallVec<[i32; 32]> = self
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        self.emit_action(RemoteAction::SetUnitCommand { units, command });
    }

    /// `setUnitStance` over the current selection.
    pub fn set_stance(&mut self, stance: u16, enabled: bool) {
        if self.controller.state.selected_units.is_empty() {
            return;
        }
        let units: SmallVec<[i32; 32]> = self
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        self.emit_action(RemoteAction::SetUnitStance {
            units,
            stance,
            enabled,
        });
    }

    /// `requestUnitPayload`/`requestDropPayload` at the cursor.
    fn payload(&mut self, kind: PayloadAction) {
        let Some((x, y)) = self.cursor_world() else {
            return;
        };
        self.emit_action(RemoteAction::Payload {
            kind,
            x,
            y,
            target: None,
        });
    }

    /// Payload pickup at the cursor (`pickupCargo`).
    pub fn pickup_payload(&mut self) {
        self.payload(PayloadAction::PickupUnit);
    }

    /// Payload drop at the cursor (`dropCargo`).
    pub fn drop_payload(&mut self) {
        self.payload(PayloadAction::Drop);
    }

    /// `tryDropItems`/`requestItem`: withdraw or deposit at a tile.
    pub fn transfer_item(&mut self, x: i32, y: i32, item: &str, amount: i32, deposit: bool) {
        let item = self.item_ids.get(item).copied();
        self.emit_action(RemoteAction::Inventory {
            kind: if deposit {
                InventoryKind::Deposit
            } else {
                InventoryKind::Withdraw
            },
            x: x as i16,
            y: y as i16,
            item,
            amount,
            angle: 0.0,
        });
    }

    /// `rotateBlock` on the building under the cursor; rotates the pending
    /// placement when a block is selected (`Binding.rotateplaced`).
    pub fn rotate_under_cursor(&mut self, bindings: &BindingState) {
        if self.controller.state.block.is_some() {
            self.rotate_placement(bindings);
            return;
        }
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        self.emit_action(RemoteAction::Rotate {
            x: x as i16,
            y: y as i16,
            direction: true,
        });
        self.fire_hotkey("rotateplaced");
    }

    /// Rotates the pending placement by +1 (`input.rotation`).
    pub fn rotate_placement(&mut self, bindings: &BindingState) {
        self.rotate_placement_step(bindings, 1);
    }

    /// Rotates the pending placement by `step` (`input.rotation`).
    fn rotate_placement_step(&mut self, bindings: &BindingState, step: i32) {
        self.controller.state.rotation =
            (self.controller.state.rotation as i32 + step).rem_euclid(4) as u8;
        self.controller.state.override_line_rotation = true;
        if let Some(drag) = self.drag
            && drag.kind == DragKind::Place
        {
            self.update_place_line(bindings, drag.start, drag.last);
        }
        self.fire_hotkey("rotate");
    }

    /// `Renderer.scaleCamera(amount)` from the `zoom` binding.
    fn zoom(&mut self, amount: f32) {
        if let Some(mut camera) = self.camera.clone() {
            camera.bind_mut().zoom_by(amount as f64);
        }
    }

    /// Opens the config UI or content info for the block under the cursor.
    fn block_info_hotkey(&mut self) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        if let Some(spec) = self.config_spec(x, y) {
            self.effects.push(Effect::OpenBlockConfig {
                x,
                y,
                screen: self.mouse,
                spec,
            });
            self.fire_hotkey("block_info");
            return;
        }
        if let Some(name) = self.block_name_at(x, y) {
            self.open_dialog("content", &format!("{{\"content\":\"{name}\"}}"));
        }
        self.fire_hotkey("block_info");
    }

    fn open_dialog(&mut self, name: &str, ctx: &str) {
        self.effects.push(Effect::OpenDialog {
            name: name.to_owned(),
            ctx: ctx.to_owned(),
        });
    }

    /// Selects the block under the cursor (middle-click `pick`).
    fn pick_block(&mut self) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        let Some(name) = self.block_name_at(x, y) else {
            return;
        };
        if name == "air" {
            return;
        }
        let Some(mut host) = self.host.clone() else {
            return;
        };
        if host.bind_mut().select_block(GString::from(&name)) {
            let guard = host.bind();
            if let Some(content) = guard.content_registry()
                && let Some(id) = content.block_id(&name)
            {
                self.controller.state.select_block(Some(id));
            }
            self.fire_hotkey("pick");
        }
    }

    /// One `Effect::Hotkey` for the fired binding (probe/UI signal).
    fn fire_hotkey(&mut self, name: &str) {
        self.last_action = name.to_owned();
        self.action_count += 1;
        self.effects.push(Effect::Hotkey(name.to_owned()));
    }

    /// Broadcasts the current selection (`SelectionChanged`).
    pub fn emit_selection(&mut self) {
        let ids: Vec<i32> = self
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        self.effects.push(Effect::SelectionChanged(
            serde_json::Value::Array(ids.into_iter().map(serde_json::Value::from).collect())
                .to_string(),
        ));
    }

    /// `command_tap` on the world (right click in command mode).
    fn command_tap(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_world() else {
            return;
        };
        let queue = self.key_down(bindings, ids::COMMAND_QUEUE);
        let action = self
            .controller
            .command_tap(&self.selectable, 0, x, y, queue);
        if let Some(action) = action {
            self.emit_action(action);
        }
    }

    /// Emits one [`RemoteAction`] through the batcher into the sim command queue.
    pub fn emit_action(&mut self, action: RemoteAction) {
        let batches = match self.controller.batcher.emit(0, action) {
            Ok(batches) => batches,
            Err(error) => {
                log::debug!("input action rejected: {error}");
                return;
            }
        };
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let mut guard = host.bind_mut();
        for batch in batches {
            for command in batch.commands {
                guard.enqueue_sim_command(command);
            }
        }
    }

    // ---- pointer path ----

    /// Cursor tile from the camera transform.
    pub fn cursor_tile(&self) -> Option<(i32, i32)> {
        let camera = self.camera.clone()?;
        let tile = camera
            .bind()
            .screen_to_tile(self.mouse.0 as f64, self.mouse.1 as f64);
        Some((tile.x, tile.y))
    }

    /// World pixel center of the cursor tile.
    fn cursor_world(&self) -> Option<(f32, f32)> {
        let (x, y) = self.cursor_tile()?;
        let unit = mind_core::config::TILESIZE as f32;
        Some(((x as f32 + 0.5) * unit, (y as f32 + 0.5) * unit))
    }

    /// Closest commandable unit under the cursor (`tapCommandUnit`).
    fn tap_unit_at_cursor(&self) -> Option<i32> {
        let (x, y) = self.cursor_world()?;
        select_unit_tap(&self.selectable, 0, x, y, UNIT_TAP_RADIUS)
    }

    fn mouse_button(&mut self, bindings: &BindingState, button: &str, down: bool) {
        match (button, down) {
            ("left", true) => self.left_press(bindings),
            ("left", false) => self.left_release(),
            ("right", true) => self.right_press(bindings),
            ("right", false) => self.right_release(),
            ("middle", true) => self.pick_block(),
            _ => {}
        }
    }

    fn left_press(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        if self.controller.state.command_mode {
            // `Binding.control` + `Binding.select`: possess the unit under the
            // cursor (`Call.unitControl`).
            if self.key_down(bindings, ids::CONTROL)
                && let Some(id) = self.tap_unit_at_cursor()
            {
                self.emit_action(RemoteAction::UnitControl { unit: Some(id) });
            }
            self.drag = Some(Drag {
                kind: DragKind::SelectRect,
                start: (x, y),
                last: (x, y),
                moved: false,
            });
            self.controller.state.command_rect = None;
            return;
        }
        if self.controller.state.block.is_none() {
            // No placement selected: a left tap is `tileTapped`
            // (`InputHandler.java:1973`), which owns config/inventory.
            self.tile_tapped(x, y);
            return;
        }
        if !self.controller.state.is_building {
            return;
        }
        self.controller.state.begin_place();
        self.drag = Some(Drag {
            kind: DragKind::Place,
            start: (x, y),
            last: (x, y),
            moved: false,
        });
        self.update_place_line(bindings, (x, y), (x, y));
    }

    fn left_release(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag.kind {
            DragKind::Place => self.commit_place(drag),
            DragKind::SelectRect => self.commit_select(drag),
            DragKind::Break => {}
        }
    }

    /// `tileTapped` (`InputHandler.java:1973`): a left tap on a placed building
    /// opens its config UI, or its inventory when it stores items.
    fn tile_tapped(&mut self, x: i32, y: i32) {
        if let Some(spec) = self.config_spec(x, y) {
            self.effects.push(Effect::OpenBlockConfig {
                x,
                y,
                screen: self.mouse,
                spec,
            });
            return;
        }
        let Some(host) = self.host.clone() else {
            return;
        };
        let items = host.bind().block_items_json(x, y);
        if let Some(items) = items {
            self.effects.push(Effect::OpenBlockInventory {
                x,
                y,
                screen: self.mouse,
                items,
            });
        }
    }

    fn right_press(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        if self.controller.state.command_mode {
            self.command_tap(bindings);
            return;
        }
        // `Binding.breakBlock` (`DesktopInput.pollInputPlayer`): the right
        // button always enters breaking mode; config/inventory open on the
        // left tap.
        self.controller.state.begin_break();
        self.drag = Some(Drag {
            kind: DragKind::Break,
            start: (x, y),
            last: (x, y),
            moved: false,
        });
    }

    fn right_release(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let mut commands: Vec<SimCommand> = Vec::new();
        {
            let guard = host.bind();
            let world = HostWorld {
                grid: guard.grid(),
                content: guard.content_registry(),
            };
            let (x1, x2) = (drag.start.0.min(drag.last.0), drag.start.0.max(drag.last.0));
            let (y1, y2) = (drag.start.1.min(drag.last.1), drag.start.1.max(drag.last.1));
            'outer: for y in y1..=y2 {
                for x in x1..=x2 {
                    if commands.len() >= DRAG_TILE_CAP {
                        break 'outer;
                    }
                    if world.block_at(x, y) == BlockId::AIR {
                        continue;
                    }
                    let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
                        continue;
                    };
                    commands.push(SimCommand::Break { x, y, player: None });
                }
            }
        }
        self.enqueue_commands(&commands);
        self.controller.state.place_mode = PlaceMode::None;
    }

    fn mouse_move(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if drag.last != (x, y) || (x, y) != drag.start {
            drag.last = (x, y);
            drag.moved = true;
        }
        let drag = *drag;
        if drag.kind == DragKind::Place {
            self.update_place_line(bindings, drag.start, (x, y));
        }
    }

    /// Rebuilds the placement line from `start` to `end` (`updateLine`).
    fn update_place_line(&mut self, bindings: &BindingState, start: (i32, i32), end: (i32, i32)) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let diagonal = self.key_down(bindings, ids::DIAGONAL_PLACEMENT);
        let params = LineParams {
            diagonal,
            rotation: self.controller.state.rotation,
            override_line_rotation: self.controller.state.override_line_rotation,
            ..LineParams::default()
        };
        {
            let guard = host.bind();
            let world = HostWorld {
                grid: guard.grid(),
                content: guard.content_registry(),
            };
            let block_meta = self.line_block_for(&world);
            self.controller.state.update_line(
                &world,
                block_meta.as_ref(),
                TilePos::new(start.0 as i16, start.1 as i16),
                TilePos::new(end.0 as i16, end.1 as i16),
                &params,
            );
        }
        self.controller.state.refresh_preview();
        self.preview_json = serde_json::Value::Array(
            self.controller
                .state
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
                .collect(),
        )
        .to_string();
    }

    /// Block metadata for `iterateLine` (plan-07 seam; `None` when unknown).
    fn line_block_for(&self, world: &HostWorld<'_>) -> Option<LineBlock> {
        let block = self.controller.state.block?;
        let content = world.content?;
        let def = content.block(block)?;
        Some(LineBlock {
            id: block,
            size: def.size,
            offset: def.offset,
            allow_diagonal: true,
            conveyor_placement: matches!(
                def.kind,
                BlockKind::Conveyor | BlockKind::ArmoredConveyor | BlockKind::Duct
            ),
            allow_rectangle_placement: false,
            swap_diagonal_placement: false,
            ignore_line_rotation: false,
        })
    }

    /// Commits a placement drag into queued `SimCommand::Place`s.
    fn commit_place(&mut self, drag: Drag) {
        let Some(block) = self.controller.state.block else {
            return;
        };
        let mut plans: Vec<ClientPlan> = self.controller.state.line_plans.clone();
        if !drag.moved || plans.is_empty() {
            plans = vec![ClientPlan::place(
                drag.last.0,
                drag.last.1,
                self.controller.state.rotation,
                block,
            )];
        }
        let commands: Vec<SimCommand> = plans
            .iter()
            .take(DRAG_TILE_CAP)
            .filter(|plan| !plan.breaking && plan.block != BlockId::AIR)
            .map(|plan| SimCommand::Place {
                x: plan.x as i16,
                y: plan.y as i16,
                block: plan.block.raw(),
                rotation: plan.rotation as i8,
                team: 0,
                player: None,
            })
            .collect();
        self.enqueue_commands(&commands);
        self.controller.state.line_plans.clear();
        self.controller.state.line.clear();
        self.controller.state.place_mode = PlaceMode::None;
        self.controller.state.preview.clear();
    }

    /// Commits a command-mode selection drag (`selectUnitsRect`).
    fn commit_select(&mut self, drag: Drag) {
        let unit = mind_core::config::TILESIZE as f32;
        let (x1, x2) = (drag.start.0.min(drag.last.0), drag.start.0.max(drag.last.0));
        let (y1, y2) = (drag.start.1.min(drag.last.1), drag.start.1.max(drag.last.1));
        if !drag.moved {
            if let Some((x, y)) = self.cursor_world() {
                match select_unit_tap(&self.selectable, 0, x, y, UNIT_TAP_RADIUS) {
                    Some(id) => {
                        let now_ms = Time::singleton().get_ticks_msec();
                        let type_id = self
                            .selectable
                            .iter()
                            .find(|unit| unit.id == id)
                            .map(|unit| unit.type_id);
                        let double_tap = type_id.is_some()
                            && type_id == self.last_tap_type
                            && now_ms.saturating_sub(self.last_tap_ms) < UNIT_TAP_INTERVAL_MS;
                        self.controller.state.selected_units.clear();
                        if double_tap {
                            // Double tap selects every unit of the tapped type.
                            let mut out = SmallVec::new();
                            select_typed_units(&self.selectable, 0, type_id.unwrap_or(0), &mut out);
                            self.controller.state.selected_units = out;
                        } else {
                            self.controller.state.selected_units.push(id);
                        }
                        self.controller.state.tapped_one = true;
                        self.last_tap_ms = now_ms;
                        self.last_tap_type = type_id;
                    }
                    None => {
                        self.controller.state.selected_units.clear();
                    }
                }
            }
        } else {
            let rect = SelectRect {
                x: x1 as f32 * unit,
                y: y1 as f32 * unit,
                w: (x2 - x1 + 1) as f32 * unit,
                h: (y2 - y1 + 1) as f32 * unit,
            };
            let mut out = SmallVec::new();
            select_units_rect(&self.selectable, 0, rect, &mut out);
            self.controller.state.selected_units = out;
            self.controller.state.command_rect = Some((rect.x, rect.y, rect.w, rect.h));
            self.controller.state.tapped_one = self.controller.state.selected_units.len() == 1;
        }
        self.controller.state.command_buildings.clear();
        self.emit_selection();
    }

    /// `update()` scroll gating: rotate the placement, else zoom the camera.
    /// Honours the `rotate`/`zoom` bindings (both default to `scroll`).
    fn scroll(&mut self, bindings: &BindingState, delta: f32) {
        if delta == 0.0 || self.ui_dialog {
            return;
        }
        let rotate_on_scroll = Self::axis_is_scroll(bindings, ids::ROTATE);
        let zoom_on_scroll = Self::axis_is_scroll(bindings, ids::ZOOM);
        if rotate_on_scroll
            && self.controller.state.is_building
            && self.controller.state.block.is_some()
        {
            let step = if delta > 0.0 { 1 } else { -1 };
            self.controller.state.rotation =
                (self.controller.state.rotation as i32 + step).rem_euclid(4) as u8;
            self.controller.state.override_line_rotation = true;
            if let Some(drag) = self.drag
                && drag.kind == DragKind::Place
            {
                self.update_place_line(bindings, drag.start, drag.last);
            }
            return;
        }
        if zoom_on_scroll && let Some(mut camera) = self.camera.clone() {
            camera.bind_mut().zoom_by(delta as f64);
        }
    }

    // ---- world queries ----

    /// Block name at a tile (`None` out of bounds/unloaded).
    pub fn block_name_at(&self, x: i32, y: i32) -> Option<String> {
        let host = self.host.clone()?;
        let guard = host.bind();
        let content = guard.content_registry()?;
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let block = guard.grid().block_at(TilePos::new(tx, ty))?;
        content.block(block).map(|def| def.name.clone())
    }

    /// Builds the config-spec JSON for a configurable block.
    pub fn config_spec(&self, x: i32, y: i32) -> Option<String> {
        let host = self.host.clone()?;
        let guard = host.bind();
        let content = guard.content_registry()?;
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let block = guard.grid().block_at(TilePos::new(tx, ty))?;
        let def = content.block(block)?;
        if !def.configurable {
            return None;
        }
        let mut options: Vec<serde_json::Value> = Vec::new();
        if def.has_items {
            for entry in content.entries(ContentType::Item) {
                if let Some(name) = entry.name {
                    options.push(serde_json::json!({
                        "name": name,
                        "label": format!("item.{name}.name"),
                    }));
                }
            }
        }
        if options.is_empty() && def.has_liquids {
            for entry in content.entries(ContentType::Liquid) {
                if let Some(name) = entry.name {
                    options.push(serde_json::json!({
                        "name": name,
                        "label": format!("liquid.{name}.name"),
                    }));
                }
            }
        }
        let text = def.kind == BlockKind::LogicBlock;
        Some(
            serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "text": text,
                "options": options,
            })
            .to_string(),
        )
    }

    fn enqueue_commands(&self, commands: &[SimCommand]) {
        if commands.is_empty() {
            return;
        }
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let mut guard = host.bind_mut();
        for command in commands {
            guard.enqueue_sim_command(command.clone());
        }
    }

    /// Camera recenter on the player core (`Binding.respawn`); also clears any
    /// possessed unit (`Call.unitClear`).
    pub fn recenter_camera(&mut self) {
        self.emit_action(RemoteAction::UnitClear);
        if let Some(mut camera) = self.camera.clone() {
            camera.bind_mut().recenter_player();
        }
        self.fire_hotkey("respawn");
    }
}

/// Camera world coordinate for a viewport point (screen→tile center).
fn camera_world_corner(camera: &Gd<MindCamera2D>, x: f32, y: f32) -> (f32, f32) {
    let tile = camera.bind().screen_to_tile(x as f64, y as f64);
    let unit = mind_core::config::TILESIZE as f32;
    ((tile.x as f32 + 0.5) * unit, (tile.y as f32 + 0.5) * unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mode_hold_tracks_the_shared_default_key() {
        // `DesktopInput.update` + `commandmodehold` default true: command mode
        // follows the held command key (`boost` shares `shiftLeft`).
        let mut bridge = DesktopBridge::new();
        let bindings = BindingState::new();
        assert_eq!(bindings.name(ids::COMMAND_MODE), Some("shiftLeft"));
        assert_eq!(bindings.name(ids::BOOST), Some("shiftLeft"));

        bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
        assert!(bridge.controller.state.command_mode);
        assert_eq!(bridge.last_action, "command_mode");
        bridge.handle(&bindings, RawEvent::key_up("shiftLeft"));
        assert!(!bridge.controller.state.command_mode);
    }

    #[test]
    fn command_mode_tap_toggles_when_hold_disabled() {
        // `commandmodehold=false`: `input.keyTap` toggles on the press edge.
        let mut bridge = DesktopBridge::new();
        bridge.set_command_mode_hold(false);
        let bindings = BindingState::new();

        bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
        assert!(bridge.controller.state.command_mode);
        bridge.handle(&bindings, RawEvent::key_up("shiftLeft"));
        assert!(bridge.controller.state.command_mode);

        bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
        assert!(!bridge.controller.state.command_mode);
    }

    #[test]
    fn command_mode_ignored_while_placing() {
        // `DesktopInput.update` requires `block == null`.
        let mut bridge = DesktopBridge::new();
        let bindings = BindingState::new();
        bridge
            .controller
            .state
            .select_block(Some(BlockId::STONE_WALL));

        bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
        assert!(!bridge.controller.state.command_mode);
    }

    #[test]
    fn command_mode_api_latch_survives_events_without_the_key() {
        // `MindInput.toggle_command_mode()` (MCP/test) latches command mode;
        // hold mode must not clear it while the command key is up.
        let mut bridge = DesktopBridge::new();
        let bindings = BindingState::new();
        bridge.toggle_command_mode();
        assert!(bridge.controller.state.command_mode);

        bridge.handle(&bindings, RawEvent::MouseMove { x: 10.0, y: 10.0 });
        bridge.update_command_mode(&bindings, None);
        assert!(bridge.controller.state.command_mode);

        // A command-key edge still takes over.
        bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
        bridge.handle(&bindings, RawEvent::key_up("shiftLeft"));
        assert!(!bridge.controller.state.command_mode);
    }

    #[test]
    fn command_mode_released_when_placement_block_selected() {
        // `DesktopInput.update` assigns `commandMode = false` in the else
        // branch, so selecting a block while holding the key drops it.
        let mut bridge = DesktopBridge::new();
        let bindings = BindingState::new();
        bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
        assert!(bridge.controller.state.command_mode);

        bridge
            .controller
            .state
            .select_block(Some(BlockId::STONE_WALL));
        bridge.update_command_mode(&bindings, None);
        assert!(!bridge.controller.state.command_mode);
    }
}
