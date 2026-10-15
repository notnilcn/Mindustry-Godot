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
    ids, line::LineBlock, line::LineParams, line::PlanBlockInfo, line::flip_plans_about,
    line::rotate_plans_about, select_typed_units, select_unit_tap, select_units_rect,
};
use mind_core::world::build::can_replace;
use mind_core::world::{ConfigKind, TilePos, WorldGrid};

use crate::camera::MindCamera2D;
use crate::sim_host::MindSimHost;

/// Maximum tiles one drag action emits (defensive; previews cap at `MAX_LENGTH`).
const DRAG_TILE_CAP: usize = 256;
/// `Build.validPlace` hit radius for tap selection (world pixels).
const UNIT_TAP_RADIUS: f32 = 11.0;
/// Double-tap window for `selectTypedUnits` (`Time.timeSinceMillis < 300`).
const UNIT_TAP_INTERVAL_MS: u64 = 300;

/// `Units.closest` search radius for `selectedUnit()` (`InputHandler.java:2152`).
const CONTROL_RADIUS: f32 = 40.0;
/// `selectedUnit()`'s hitbox grow per side (`InputHandler.java:2156`).
const CONTROL_HITBOX_GROW: f32 = 6.0;

/// Bindings whose world gesture runs directly from a pointer edge (T9); every
/// other binding sharing the same mouse key keeps its normal key-down edge.
const POINTER_BINDS: [&str; 5] = ["select", "deselect", "break_block", "pick", "command_queue"];

/// Arc mouse key name for a raw button (`MouseButton.getName`).
fn mouse_key_name(button: &str) -> Option<&'static str> {
    match button {
        "left" => Some("mouseLeft"),
        "right" => Some("mouseRight"),
        "middle" => Some("mouseMiddle"),
        "back" => Some("mouseBack"),
        "forward" => Some("mouseForward"),
        _ => None,
    }
}

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

/// `Binding.selectAllUnits` variants (`DesktopInput.java:315-357`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectAllKind {
    /// Own commandable units with `type.controlSelectGlobal`.
    Units,
    /// Own commandable units `instanceof Payloadc`.
    Transport,
    /// Own commandable buildings (`command_buildings`).
    Factories,
}

/// The active pointer drag.
#[derive(Debug, Clone, Copy)]
struct Drag {
    kind: DragKind,
    start: (i32, i32),
    last: (i32, i32),
    moved: bool,
}

/// Read-only view for the placement overlay (`DesktopInput.drawBottom` /
/// `InputHandler.drawBreakSelection`; G03 T2).
#[derive(Debug, Clone)]
pub struct PlacementView {
    /// Cursor tile from the camera transform (`cursor_tile`).
    pub cursor: Option<(i32, i32)>,
    /// Selected block (`input.block`; may be `Some(BlockId::AIR)`).
    pub block: Option<BlockId>,
    /// Placement rotation.
    pub rotation: u8,
    /// Active interaction (`InputHandler.mode`).
    pub mode: PlaceMode,
    /// Line/area plans being dragged (cloned only while `mode == Placing`).
    pub line_plans: Vec<ClientPlan>,
    /// Right-drag break rectangle as `(start, last)` in tiles.
    pub break_rect: Option<((i32, i32), (i32, i32))>,
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
    /// `inv.hide()` (`InputHandler.tileTapped`: a non-inventory tap hides the
    /// open inventory).
    HideInventory,
    /// `config.hideConfig(); inv.hide(); planConfig.hide();`
    /// (`InputHandler.tileTapped` on an empty tile).
    HidePanels,
    /// Capture the running viewport.
    Screenshot,
    /// `MindUi.close_top_dialog()` (research/planet toggle keys).
    CloseTopDialog,
    /// A binding fired (probe/UI signal).
    Hotkey(String),
    /// Call a `#[func]` on the `/root/MindUi` autoload by name
    /// (`toggle_power_lines` F5 / `toggle_fullscreen` F11).
    UiCall(String),
    /// The RTS selection changed; payload is a JSON id array.
    SelectionChanged(String),
}

/// Read-only [`PlacementWorld`] over the live sim grid/content.
///
/// `valid_place` is the real `Build.validPlace` for the player team; build
/// rotation and the `ChainedBuilding` walk read the live buildings. The host
/// is borrowed immutably (`HostWorld` never mutates the sim).
struct HostWorld<'a> {
    grid: &'a mind_core::world::WorldGrid,
    content: Option<&'a ContentRegistry>,
    host: &'a MindSimHost,
}

impl HostWorld<'_> {
    /// Block kind at a tile (`None` out of bounds/unloaded).
    fn kind_at(&self, x: i32, y: i32) -> Option<mind_core::content::BlockKind> {
        let content = self.content?;
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let block = self.grid.block_at(TilePos::new(x, y))?;
        content.block(block).map(|def| def.kind)
    }
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

    fn valid_place(&self, block: BlockId, x: i32, y: i32, rotation: u8) -> bool {
        // `Build.validPlace(block, team, x, y, rotation)`; the exact check
        // `InputHandler.flushPlans`/`updateLine` runs through `addBuild`.
        self.host.can_place_player(x, y, block, rotation)
    }

    fn build_rotation(&self, x: i32, y: i32) -> Option<u8> {
        // `plan.build().rotation` for `iterateLine`'s `end.rotation`; the
        // existing `BuildingComp.rot` seam reads `0` without a building, so
        // the build-presence check (`tile.build != null`) gates it.
        self.host
            .building_team(x, y)
            .map(|_| self.host.building_rotation(x, y))
    }

    fn is_chained(&self, x: i32, y: i32) -> bool {
        // `world.build(x,y) instanceof ChainedBuilding` (IH:2444-2449).
        self.kind_at(x, y)
            .is_some_and(mind_core::input::line::is_chained_kind)
    }

    fn block_kind(&self, block: BlockId) -> Option<mind_core::content::BlockKind> {
        // Java block class tag for the line-family checks (`getReplacement`,
        // `calculateBridges` `avoid`).
        self.content?.block(block).map(|def| def.kind)
    }

    fn chained_next(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        // `ConveyorBuild.next()` = `nextc` (the front tile's conveyor of the
        // same team, `Conveyor.java:222/486-488`); `ConduitBuild.next()` is the
        // front conduit with no team check (`Conduit.java:259-267`).
        let kind = self.kind_at(x, y)?;
        if !mind_core::input::line::is_chained_kind(kind) {
            return None;
        }
        // A chained grid block always has its building (`BuildingComp.rot`).
        let rotation = self.host.building_rotation(x, y) as usize % 4;
        let (dx, dy) = mind_core::input::placement::D4[rotation];
        let (nx, ny) = (x + dx, y + dy);
        let next_kind = self.kind_at(nx, ny)?;
        use mind_core::content::BlockKind as K;
        let matches_front = match kind {
            K::Conduit | K::ArmoredConduit => matches!(next_kind, K::Conduit | K::ArmoredConduit),
            _ => {
                matches!(next_kind, K::Conveyor | K::ArmoredConveyor)
                    && self.host.building_team(x, y) == self.host.building_team(nx, ny)
            }
        };
        matches_front.then_some((nx, ny))
    }
}

/// How close a cursor must be to the carrying player unit to start a drop
/// gesture (`Vars.playerSelectRange`, `InputHandler.java:2032-2043`).
const PLAYER_SELECT_RANGE: f32 = 11.0;

/// `rules.itemDepositCooldown` default 0.5 s (`Rules.java`), the minimum gap
/// between two tap-unit deposits.
const ITEM_DEPOSIT_COOLDOWN_MS: u64 = 500;

/// `settings.getBool("console")` (default false, `SettingsMenuDialog.java:566`):
/// the F8 console toggle is gated on it (`ConsoleFragment.java:43`).
fn console_setting_enabled() -> bool {
    crate::settings_hub::with_settings(|store| store.get_bool("console", false))
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
                hit_size: entry
                    .get("hit_size")
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0) as f32,
                select_global: entry
                    .get("select_global")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
                payload: entry
                    .get("payload")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
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
    /// `Block.lastConfig` per block (`Block.java:827-832` `nextConfig`): the
    /// last submitted config, carried onto the next plan of that block.
    last_config: HashMap<u16, mind_core::world::config::ConfigValue>,
    /// Tile of the building whose config panel is open
    /// (`config.getSelected()`), or `None` when the panel is hidden or was
    /// never opened (H02 T6 tap-to-link; H08 keeps this state).
    pub config_selected: Option<(i32, i32)>,
    /// `schemX/schemY`: the start tile of an F/B selection drag
    /// (`InputHandler.schemX`; `-1` cleared).
    schem_start: Option<(i32, i32)>,
    /// `schematicX/schematicY`: the copied plans' reference tile; plan shifts
    /// are deltas from the raw cursor to it (`DesktopInput.pollInputPlayer`).
    schematic_origin: (i32, i32),
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
    /// Top of `MindUi.dialog_stack()` (refreshed with `ui_dialog`); the
    /// research/planet dialogs close on their own binding key.
    pub top_dialog: String,
    /// `state.isGame()` mirror (`Control.java:690-760`): gameplay bindings fire
    /// only while the in-game HUD is up (no menu group).
    pub in_game: bool,
    /// `state.isCampaign()` mirror: J/N open research/planet only in a campaign.
    pub campaign_active: bool,
    /// `InputHandler.locked()` mirror (PAR-0100): world clicks and scroll are
    /// ignored while set.
    pub locked: bool,
    /// `player.shooting`: left button held on a free tile
    /// (`DesktopInput.java:701-721`); uploaded through `MindInput::player_control`.
    pub shooting: bool,
    /// `droppingItem`: a left press landed on the carrying player unit
    /// (`InputHandler.tryTapPlayer`); the release deposits or drops.
    dropping_item: bool,
    /// Last successful deposit (`Time.millis`) for `itemDepositCooldown`.
    last_deposit_ms: u64,
    /// Last cargo-key tap (`DesktopInput.lastPayloadKeyTapMillis`).
    payload_tap_ms: u64,
    /// Last cargo-key hold repeat (`DesktopInput.lastPayloadKeyHoldMillis`).
    payload_hold_ms: u64,
    /// `HintsFragment` completion triggers collected since the last drain
    /// (G05 T4b): `"shoot"`, `"boost_down"`, `"schematic_select_release"`,
    /// `"pick_tap"`, `"diagonal_release"`, `"rebuild_selecting"`.
    hint_keys: Vec<&'static str>,
    /// `settings.getBool("commandmodehold", true)`: command mode follows the
    /// held command key when on (upstream default) and toggles on a key press
    /// when off.
    command_mode_hold: bool,
    /// `settings.blockreplace`: `InputHandler.updateLine` runs the
    /// `getReplacement`/`handlePlacementLine` half (junction/bridge placement)
    /// only when set.
    pub block_replace: bool,
    /// `settings.buildautopause` (`DesktopInput.java:591-594`).
    pub build_autopause: bool,
    /// `InputHandler.buildWasAutoPaused` (`DesktopInput.java:593/673`,
    /// resumed by `ConstructBlock.tapped`, `ConstructBlock.java:206-208`).
    build_was_auto_paused: bool,
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
            last_config: HashMap::new(),
            config_selected: None,
            schem_start: None,
            schematic_origin: (0, 0),
            last_action: String::new(),
            action_count: 0,
            effects: Vec::new(),
            preview_json: String::from("[]"),
            item_ids: HashMap::new(),
            last_tap_ms: 0,
            last_tap_type: None,
            text_focus: false,
            ui_dialog: false,
            top_dialog: String::new(),
            in_game: false,
            campaign_active: false,
            locked: false,
            shooting: false,
            dropping_item: false,
            last_deposit_ms: 0,
            payload_tap_ms: 0,
            payload_hold_ms: 0,
            hint_keys: Vec::new(),
            command_mode_hold: true,
            block_replace: true,
            build_autopause: false,
            build_was_auto_paused: false,
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
        // `DesktopInput.java:465-469`: while a dialog owns input, the held
        // select button no longer keeps `player.shooting` latched.
        if self.ui_dialog && !matches!(event, RawEvent::MouseMove { .. }) {
            self.shooting = false;
        }
        let mut command_edge: Option<bool> = None;
        match event {
            RawEvent::KeyDown { code } => {
                let fresh = self.pressed.insert(code.clone());
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
                "select" => self.left_release(),
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
    fn command_mode_allowed(&self, bindings: &BindingState) -> bool {
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

    /// Runs one named binding (`Binding.name`).
    fn dispatch_action(&mut self, bindings: &BindingState, name: &'static str) {
        match name {
            "respawn" => {
                // `DesktopInput.java:423-431`: V clears the possession and
                // returns to the core unit (`Call.unitClear`); the camera
                // recentering follows the new player unit.
                self.dropping_item = false;
                self.emit_action(RemoteAction::UnitClear);
                self.fire_hotkey(name);
            }
            "rotateplaced" => {
                // T9: `rotateplaced` is a wheel modifier now (`cursor.build !=
                // null && ... && keyDown(rotatePlaced) && axisTap(rotate) != 0`,
                // DesktopInput.java:924-926); the key edge alone does nothing.
            }
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
                // `Binding.clearBuilding` (`DesktopInput.java:612`): the player's
                // queue is cleared too (`player.unit().clearBuilding()`), and
                // `pollInputPlayer` L632 also drops the copied plans.
                self.controller.state.select_block(None);
                self.controller.state.override_line_rotation = false;
                self.controller.state.select_plans.clear();
                self.refresh_select_preview();
                if let Some(mut host) = self.host.clone() {
                    host.bind_mut().clear_player_build_plans();
                }
                self.fire_hotkey(name);
            }
            "schematic_select" | "rebuild_select" => {
                // `DesktopInput.java:616-618`: F/B latch the selection start
                // (raw cursor); a break drag owns the tiles instead.
                if !self
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.kind == DragKind::Break)
                {
                    self.schem_start = self.raw_cursor_tile();
                }
                self.fire_hotkey(name);
            }
            "schematic_flip_x" => {
                self.flip_select_plans(true);
                self.fire_hotkey(name);
            }
            "schematic_flip_y" => {
                self.flip_select_plans(false);
                self.fire_hotkey(name);
            }
            "pause_building" => {
                // `DesktopInput.java:671-678`: toggles `isBuilding`, clears the
                // auto-pause latch and stops shooting when resuming; applied to
                // the unit in `InputHandler.update` (`updateBuilding`).
                self.controller.state.is_building = !self.controller.state.is_building;
                self.build_was_auto_paused = false;
                if self.controller.state.is_building {
                    self.shooting = false;
                }
                if let Some(mut host) = self.host.clone() {
                    host.bind_mut()
                        .set_player_building_paused(!self.controller.state.is_building);
                }
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
                if !self.command_mode_hold && self.command_mode_allowed(bindings) {
                    self.toggle_command_mode();
                }
            }
            "select_all_units" => self.select_all_units(bindings, SelectAllKind::Units),
            "select_all_unit_transport" => {
                self.select_all_units(bindings, SelectAllKind::Transport)
            }
            "select_all_unit_factories" => {
                self.select_all_units(bindings, SelectAllKind::Factories)
            }
            "cancel_orders" => {
                // PlacementFragment command table: `cancelOrders` orders stance
                // `stop` (it does NOT deselect).
                if self.controller.state.command_mode {
                    self.set_stance_named("stop", true);
                }
            }
            "deselect" => {
                self.controller.state.selected_units.clear();
                self.controller.state.command_buildings.clear();
                self.emit_selection();
            }
            "pickupCargo" => {
                // `DesktopInput.java:1012-1040`: a tap sets the hold timer.
                self.payload_tap_ms = Time::singleton().get_ticks_msec();
                self.payload(PayloadAction::PickupUnit);
            }
            "dropCargo" => {
                self.payload_tap_ms = Time::singleton().get_ticks_msec();
                self.payload(PayloadAction::Drop);
            }
            "boost" => {
                // `HintsFragment.boost` = `keyDown(boost)`; the edge and the
                // per-frame refresh both push the trigger.
                self.push_hint("boost_down");
            }
            "unit_stance_hold_fire"
            | "unit_stance_pursue_target"
            | "unit_stance_patrol"
            | "unit_stance_ram"
            | "unit_stance_boost"
            | "unit_stance_hold_position" => {
                // PlacementFragment command table: names resolved through the
                // stance registry; command mode only.
                if self.controller.state.command_mode {
                    let stance = match name {
                        "unit_stance_hold_fire" => "holdfire",
                        "unit_stance_pursue_target" => "pursuetarget",
                        "unit_stance_patrol" => "patrol",
                        "unit_stance_ram" => "ram",
                        "unit_stance_boost" => "boost",
                        _ => "holdposition",
                    };
                    self.set_stance_named(stance, true);
                }
            }
            "unit_command_move"
            | "unit_command_repair"
            | "unit_command_rebuild"
            | "unit_command_assist"
            | "unit_command_mine"
            | "unit_command_enter_payload"
            | "unit_command_load_units"
            | "unit_command_load_blocks"
            | "unit_command_unload_payload"
            | "unit_command_loop_payload" => {
                if self.controller.state.command_mode {
                    let command = match name {
                        "unit_command_move" => "move",
                        "unit_command_repair" => "repair",
                        "unit_command_rebuild" => "rebuild",
                        "unit_command_assist" => "assist",
                        "unit_command_mine" => "mine",
                        "unit_command_enter_payload" => "enterPayload",
                        "unit_command_load_units" => "loadUnits",
                        "unit_command_load_blocks" => "loadBlocks",
                        "unit_command_unload_payload" => "unloadPayload",
                        _ => "loopPayload",
                    };
                    self.command_units_named(command);
                }
            }
            "minimap" => {
                self.effects
                    .push(Effect::ToggleFragment(String::from("minimap")));
                self.fire_hotkey(name);
            }
            "console" => {
                // `ConsoleFragment.java:43`: the console key only acts when the
                // `console` client setting is on (default false,
                // SettingsMenuDialog.java:566).
                if console_setting_enabled() {
                    self.effects
                        .push(Effect::ToggleFragment(String::from("console")));
                }
                self.fire_hotkey(name);
            }
            "research" => {
                // `DesktopInput.java:446-461`: J/N toggle campaign screens only
                // while a campaign sector is live.
                if self.campaign_active {
                    self.open_dialog("research", "");
                }
                self.fire_hotkey(name);
            }
            "planet_map" => {
                if self.campaign_active {
                    self.open_dialog("planet", "");
                }
                self.fire_hotkey(name);
            }
            "schematic_menu" => {
                self.open_dialog("schematics", "");
                self.fire_hotkey(name);
            }
            "block_info" => self.block_info_hotkey(),
            "pick" => self.pick_block(),
            "toggle_block_status" => {
                // `DesktopInput.java:455-457`: F6 flips `settings.blockstatus`;
                // the HUD fragment owns the setting write and the marker pass
                // reads it (`Renderer.drawStatus`).
                self.fire_hotkey(name);
            }
            "toggle_power_lines" => {
                // `DesktopInput.java:459-466`: F5 swaps `lasersopacity` with
                // `preferredlaseropacity` (the node owns the setting write).
                self.effects
                    .push(Effect::UiCall("toggle_power_lines".to_owned()));
                self.fire_hotkey(name);
            }
            "fullscreen" => {
                // `Control.java:690-694`: F11 (and the menu) flips the
                // persisted `fullscreen` setting and the window mode.
                self.effects
                    .push(Effect::UiCall("toggle_fullscreen".to_owned()));
            }
            "toggle_menus" => {
                // `HudFragment.java:455-461`: the HUD fragment flips its tables.
                self.fire_hotkey(name);
            }
            "category_prev" | "category_next" => {
                // The placement fragment owns the category rail; it reacts to
                // the hotkey signal and pushes the new index back.
                self.fire_hotkey(name);
            }
            "pause" => {
                // `Control.java:730-734`: the pause key is ignored while
                // `renderer.isCutscene()` (landing/launch).
                let cutscene = self
                    .camera
                    .clone()
                    .map(|camera| camera.bind().is_cutscene())
                    .unwrap_or(false);
                if !cutscene && let Some(mut host) = self.host.clone() {
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

    /// `pollInputPlayer` L632: selecting a block (`isPlacing()`) clears the
    /// copied plans.
    pub fn clear_select_plans(&mut self) {
        if !self.controller.state.select_plans.is_empty() {
            self.controller.state.select_plans.clear();
            self.refresh_select_preview();
        }
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
            self.clear_select_plans();
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

    /// Live player team (`player.team().id`) for RTS selection filters.
    pub fn player_team(&self) -> u8 {
        self.host
            .as_ref()
            .map(|host| host.bind().default_team().clamp(0, 255) as u8)
            .unwrap_or(0)
    }

    fn select_all_units(&mut self, bindings: &BindingState, kind: SelectAllKind) {
        // `DI:315-357`: select-all runs only while command mode is active.
        if !self.controller.state.command_mode {
            return;
        }
        let screens = self.key_down(bindings, ids::SELECT_ACROSS_SCREEN);
        let rect = screens.then(|| self.viewport_world_rect());
        self.controller.state.command_buildings.clear();
        match kind {
            SelectAllKind::Factories => {
                // `build.isCommandable()` buildings (`CoreBlock`, `UnitFactory`,
                // `Reconstructor`, `UnitAssembler`, `PayloadSource`).
                self.controller.state.selected_units.clear();
                let tile = mind_core::config::TILESIZE as f32;
                if let Some(host) = self.host.clone() {
                    self.controller.state.command_buildings = host
                        .bind()
                        .commandable_buildings()
                        .into_iter()
                        .filter(|(x, y)| {
                            rect.is_none_or(|rect| {
                                rect.contains(*x as f32 * tile, *y as f32 * tile)
                            })
                        })
                        .map(|(x, y)| TilePos::new(x, y))
                        .collect();
                }
            }
            SelectAllKind::Units | SelectAllKind::Transport => {
                // `type.controlSelectGlobal` (`mono` excluded) or
                // `unit instanceof Payloadc`; no infinite rect (NaN bounds).
                let transport = kind == SelectAllKind::Transport;
                let team = self.player_team();
                self.controller.state.selected_units = self
                    .selectable
                    .iter()
                    .filter(|unit| {
                        unit.team == team
                            && unit.commandable
                            && if transport {
                                unit.payload
                            } else {
                                unit.select_global
                            }
                            && rect.is_none_or(|rect| rect.contains(unit.x, unit.y))
                    })
                    .map(|unit| unit.id)
                    .collect();
            }
        }
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
    pub(crate) fn command_units(&mut self, command: u16) {
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

    /// `PlacementFragment` unit-type filter (`PlacementFragment.java:537-548`):
    /// left click keeps only units of `keep`, right click drops `drop`;
    /// `type_of` maps a selected id to its unit type id. Returns whether the
    /// selection changed (emits `selection_changed` like `select_rect`).
    pub(crate) fn retain_selection_types(
        &mut self,
        type_of: &dyn Fn(i32) -> Option<i32>,
        keep: Option<i32>,
        drop: Option<i32>,
    ) -> bool {
        let before = self.controller.state.selected_units.len();
        self.controller.state.selected_units.retain(|id| {
            let Some(type_id) = type_of(*id) else {
                return false;
            };
            keep.is_none_or(|keep| type_id == keep) && drop != Some(type_id)
        });
        let changed = self.controller.state.selected_units.len() != before;
        if changed {
            self.controller.state.command_buildings.clear();
            self.emit_selection();
        }
        changed
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

    /// `Call.setUnitStance(player, selected ids, stance, enabled)` by
    /// `UnitStance` name. Java toggles `!activeStances.get(id)`; without a
    /// per-unit active-stance feed the port passes `enabled` through (callers
    /// send `true`).
    pub fn set_stance_named(&mut self, name: &str, enabled: bool) -> bool {
        if self.controller.state.selected_units.is_empty() {
            return false;
        }
        let Some(stance) = self.host.as_ref().and_then(|host| {
            host.bind()
                .content_registry()
                .and_then(|content| content.unit_stance_by_name(name))
                .map(|def| def.id.get())
        }) else {
            return false;
        };
        self.set_stance(stance, enabled);
        true
    }

    /// `Call.setUnitCommand(player, selected ids, command)` by `UnitCommand`
    /// name (`false` when nothing is selected or the name is unknown).
    pub fn command_units_named(&mut self, name: &str) -> bool {
        if self.controller.state.selected_units.is_empty() {
            return false;
        }
        let Some(command) = self.host.as_ref().and_then(|host| {
            host.bind()
                .content_registry()
                .and_then(|content| content.unit_command_by_name(name))
                .map(|def| def.id.get())
        }) else {
            return false;
        };
        self.command_units(command);
        true
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

    /// `DesktopInput.updateMovement` payload hold-repeat
    /// (`DesktopInput.java:1012-1040`): after a tap, holding the cargo key
    /// repeats the action every >20 ms once the tap is >200 ms old.
    pub fn update_payload_hold(&mut self, bindings: &BindingState) {
        let held = if self.key_down(bindings, ids::PICKUP_CARGO) {
            Some(PayloadAction::PickupUnit)
        } else if self.key_down(bindings, ids::DROP_CARGO) {
            Some(PayloadAction::Drop)
        } else {
            None
        };
        let Some(action) = held else {
            return;
        };
        let now = Time::singleton().get_ticks_msec();
        if now.saturating_sub(self.payload_tap_ms) > 200
            && now.saturating_sub(self.payload_hold_ms) > 20
        {
            self.payload(action);
            self.payload_hold_ms = now;
        }
    }

    /// Per-frame `pollInputPlayer` L762-768 mirror: outside an active placement
    /// line (`mode == placing && block != null`) the wheel snap latch clears.
    pub fn sync_line_override(&mut self) {
        if !(self.controller.state.is_placing() && self.controller.state.block.is_some()) {
            self.controller.state.override_line_rotation = false;
        }
    }

    /// `HintsFragment` triggers that fire while a key is held (G05 T4b):
    /// `boost` (`keyDown(boost)`) and `rebuildSelecting`
    /// (`input.isRebuildSelecting()` = `keyDown(rebuildSelect)`).
    pub fn refresh_hints(&mut self, bindings: &BindingState) {
        if self.key_down(bindings, ids::BOOST) {
            self.push_hint("boost_down");
        }
        if self.key_down(bindings, ids::REBUILD_SELECT) {
            self.push_hint("rebuild_selecting");
        }
    }

    /// Returns and clears the collected hint triggers (order of first
    /// occurrence).
    pub fn take_hint_keys(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.hint_keys)
    }

    /// Deduplicated hint trigger push.
    fn push_hint(&mut self, key: &'static str) {
        if !self.hint_keys.contains(&key) {
            self.hint_keys.push(key);
        }
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
        // Same snap as the wheel path (`pollInputPlayer` L762-768 runs for any
        // `axisTap(rotate)`, wheel or bound key, before L836-849 steps it).
        let diagonal_held = self.key_down(bindings, ids::DIAGONAL_PLACEMENT);
        if let Some(drag) = self.drag
            && drag.kind == DragKind::Place
            && !self.controller.state.override_line_rotation
            && !diagonal_held
            && drag.start != drag.last
        {
            self.controller.state.rotation = mind_core::input::placement::line_angle_rotation(
                drag.start.0,
                drag.start.1,
                drag.last.0,
                drag.last.1,
            );
            self.controller.state.override_line_rotation = true;
        }
        self.controller.state.rotation =
            (self.controller.state.rotation as i32 + step).rem_euclid(4) as u8;
        if let Some(drag) = self.drag
            && drag.kind == DragKind::Place
        {
            self.update_place_line(bindings, drag.start, drag.last);
        } else if !self.controller.state.select_plans.is_empty() {
            // `pollInputPlayer` L843-858: the axis rotates copied plans when no
            // placement line owns the wheel.
            self.rotate_select_plans(step);
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
            self.clear_select_plans();
            // `HintsFragment` `schematicSelect` includes `keyTap(pick)`.
            self.push_hint("pick_tap");
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
        let queue = self.key_down(bindings, ids::COMMAND_QUEUE);
        self.command_tap_with_queue(queue);
    }

    /// `DesktopInput.touchDown` (DI:924-934): the middle click
    /// (`Binding.commandQueue`) issues a queued order in command mode.
    fn command_tap_queued(&mut self) {
        self.command_tap_with_queue(true);
    }

    fn command_tap_with_queue(&mut self, queue: bool) {
        let Some((x, y)) = self.cursor_world() else {
            return;
        };
        // `InputHandler.commandTap` IH:1166-1168: the building under the
        // cursor is the attack target when its team is not the player's
        // (derelict counts as an enemy target, as in Java).
        let player_team = self.player_team();
        let enemy_building = self.cursor_tile().and_then(|(tx, ty)| {
            let host = self.host.clone()?;
            let team = host.bind().building_team(tx, ty)?;
            (team != player_team).then_some(mind_core::world::TilePos::new(tx as i16, ty as i16))
        });
        let actions =
            self.controller
                .command_tap(&self.selectable, player_team, x, y, queue, enemy_building);
        for action in actions {
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

    /// Exact cursor world position (`input.mouseWorldX/Y`), not tile-snapped.
    pub fn aim_world(&self) -> Option<(f32, f32)> {
        let camera = self.camera.clone()?;
        let point = camera
            .bind()
            .screen_to_world(self.mouse.0 as f64, self.mouse.1 as f64);
        Some((point.x, point.y))
    }

    /// `InputHandler.tileX/tileY` (`InputHandler.java:2105-2120`): while a block
    /// is selected (and not breaking), the cursor tile is shifted by the block's
    /// `offset`, centring even-size blocks under the cursor.
    fn placement_cursor_tile(&self) -> Option<(i32, i32)> {
        let block = self.controller.state.block?;
        if self.controller.state.place_mode == PlaceMode::Breaking {
            return self.cursor_tile();
        }
        let offset = self
            .host
            .clone()
            .and_then(|host| {
                host.bind()
                    .content_registry()
                    .and_then(|content| content.block(block))
                    .map(|def| def.offset)
            })
            .unwrap_or(0.0);
        let (wx, wy) = self.aim_world()?;
        Some(mind_core::input::cursor::placement_tile(wx, wy, offset))
    }

    /// `player.shooting` after the input-side `canShoot` gates
    /// (`InputHandler.java:2269-2272`; the sim half re-checks building/mining).
    pub fn shooting_allowed(&self) -> bool {
        self.shooting
            && !self.dropping_item
            && self.controller.state.block.is_none()
            && !self.controller.state.command_mode
    }

    /// Raw cursor tile (`InputHandler.rawTileX/rawTileY`), no block offset.
    pub fn raw_cursor_tile(&self) -> Option<(i32, i32)> {
        self.cursor_tile()
    }

    /// `PlanBlockInfo` resolver for [`rotate_plans_about`]/[`flip_plans_about`]
    /// (G08 T6b): content size/offset, `Block.rotate` and the `electrolyzer`
    /// `invertFlip` (`Blocks.java:1363`).
    fn plan_info(host: &Gd<MindSimHost>) -> impl Fn(BlockId) -> PlanBlockInfo + '_ {
        move |block| {
            let guard = host.bind();
            let def = guard
                .content_registry()
                .and_then(|content| content.block(block));
            PlanBlockInfo {
                size: def.map(|def| def.size).unwrap_or(1),
                offset: def.map(|def| def.offset).unwrap_or(0.0),
                rotate: guard.block_rotates(block),
                invert_flip: def.is_some_and(|def| def.name == "electrolyzer"),
            }
        }
    }

    /// Select-plan preview (`[{x,y,rotation,block}]`, same shape as the
    /// placement line).
    fn refresh_select_preview(&mut self) {
        self.preview_json = serde_json::Value::Array(
            self.controller
                .state
                .select_plans
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

    /// Adopts copied plans (`useSchematic`: dialog or F-drag) at the cursor and
    /// refreshes the preview; the block selection is cleared.
    pub fn adopt_select_plans(&mut self, plans: Vec<ClientPlan>, origin: (i32, i32)) {
        self.controller.state.select_block(None);
        self.controller.state.select_plans = plans;
        self.schematic_origin = origin;
        self.refresh_select_preview();
    }

    /// `pollInputPlayer` L843-858: wheel/rotate over the copied plans.
    fn rotate_select_plans(&mut self, step: i32) {
        if self.controller.state.select_plans.is_empty() {
            return;
        }
        let Some((ox, oy)) = self.raw_cursor_tile() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let info = Self::plan_info(&host);
        rotate_plans_about(&mut self.controller.state.select_plans, step, ox, oy, &info);
        self.refresh_select_preview();
    }

    /// `pollInputPlayer` L641: Z/X flip the copied plans about the raw cursor.
    fn flip_select_plans(&mut self, flip_x: bool) {
        if self.controller.state.select_plans.is_empty() {
            return;
        }
        let Some((ox, oy)) = self.raw_cursor_tile() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let info = Self::plan_info(&host);
        flip_plans_about(
            &mut self.controller.state.select_plans,
            flip_x,
            ox,
            oy,
            &info,
        );
        self.refresh_select_preview();
    }

    /// `keyRelease(schematicSelect)` (`DesktopInput.java:633-640`):
    /// `schematics.create` copies the F-selection into select plans, clears the
    /// block selection and keeps the plans selected for a paste.
    fn finish_schematic_copy(&mut self, bindings: &BindingState) {
        let Some(start) = self.schem_start else {
            return;
        };
        let Some(end) = self.raw_cursor_tile() else {
            return;
        };
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let plans = {
            let guard = host.bind();
            guard.copy_schematic_plans(start, end, end)
        };
        self.controller.state.select_block(None);
        self.controller.state.command_rect = None;
        {
            let mut guard = host.bind_mut();
            guard.select_block(GString::from("air"));
        }
        self.adopt_select_plans(plans, end);
        // `if(!keyDown(rebuildSelect)){ schemX = schemY = -1 }` (the B-release
        // half is T7's).
        if !self.key_down(bindings, ids::REBUILD_SELECT) {
            self.schem_start = None;
        }
    }

    /// Key-release bindings (`DesktopInput.pollInputPlayer` release half).
    fn key_up_event(&mut self, bindings: &BindingState, code: &str) {
        // T9: a pointer binding rebound to a key releases through the same
        // handlers (`select` -> left release, `break_block`/`deselect` ->
        // right release).
        if bindings.name(ids::SELECT) == Some(code) {
            self.left_release();
        }
        if bindings.name(ids::BREAK_BLOCK) == Some(code)
            || bindings.name(ids::DESELECT) == Some(code)
        {
            self.right_release(bindings);
        }
        if bindings.name(ids::DIAGONAL_PLACEMENT) == Some(code) {
            // `HintsFragment.conveyorPathfind` = `keyRelease(diagonalPlacement)`.
            self.push_hint("diagonal_release");
        }
        if bindings.name(ids::SCHEMATIC_SELECT) == Some(code) {
            // `HintsFragment.schematicSelect` = `keyRelease(schematicSelect)`.
            self.push_hint("schematic_select_release");
        }
        let schematic = bindings.name(ids::SCHEMATIC_SELECT) == Some(code);
        let rebuild = bindings.name(ids::REBUILD_SELECT) == Some(code);
        if !(schematic || rebuild) || self.schem_start.is_none() || self.drag.is_some() {
            return;
        }
        if schematic {
            self.finish_schematic_copy(bindings);
        } else {
            self.finish_rebuild_select(bindings);
        }
    }

    /// `keyRelease(rebuildSelect)` (`InputHandler.java:2318-2330`): queue the
    /// team ghost plans overlapping the B-selection. (The `tryRepairDerelict`
    /// scan, `InputHandler.java:2331-2341`, is deferred: G02 T8.)
    fn finish_rebuild_select(&mut self, bindings: &BindingState) {
        let Some(start) = self.schem_start else {
            return;
        };
        let Some(end) = self.raw_cursor_tile() else {
            return;
        };
        if let Some(mut host) = self.host.clone() {
            let _ = host
                .bind_mut()
                .queue_team_rebuild(start.0, start.1, end.0, end.1);
        }
        // `if(!keyDown(schematicSelect)){ schemX = schemY = -1 }`.
        if !self.key_down(bindings, ids::SCHEMATIC_SELECT) {
            self.schem_start = None;
        }
    }

    /// `BuildingComp.configure` -> `Block.lastConfig` (`Block.java:827-832`):
    /// remembers the submitted config per saveConfig block for `nextConfig`.
    pub fn record_last_config(
        &mut self,
        x: i32,
        y: i32,
        value: mind_core::world::config::ConfigValue,
    ) {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return;
        };
        let Some(block) = guard.grid().block_at(TilePos::new(tx, ty)) else {
            return;
        };
        let Some(def) = content.block(block) else {
            return;
        };
        if !def.save_config {
            return;
        }
        // The wire carries content names (`to_sim_config`); resolve them like
        // the sim's `Configure` arm so `nextConfig` stores the typed value.
        use mind_core::world::config::ConfigValue;
        let value = match value {
            ConfigValue::String(name) => {
                if let Some(item) = content.item_by_name(&name) {
                    ConfigValue::Item(item.id)
                } else if let Some(block) = content.block_by_name(&name) {
                    ConfigValue::Block(block.id)
                } else if let Some(unit) = content.unit_by_name(&name) {
                    ConfigValue::Unit(unit.id)
                } else if let Some(liquid) = content.liquid_by_name(&name) {
                    ConfigValue::Liquid(liquid.id)
                } else {
                    ConfigValue::String(name)
                }
            }
            other => other,
        };
        self.last_config.insert(block.raw(), value);
    }

    /// `Block.saveConfig` for `block` (`nextConfig` gate, `Block.java:827-832`).
    fn block_save_config(&self, block: BlockId) -> bool {
        self.host
            .clone()
            .and_then(|host| {
                host.bind()
                    .content_registry()
                    .and_then(|content| content.block(block))
                    .map(|def| def.save_config)
            })
            .unwrap_or(false)
    }

    /// Snapshot for the placement overlay pass (G03 T2).
    pub fn placement_view(&self) -> PlacementView {
        let mode = self.controller.state.place_mode;
        PlacementView {
            cursor: self.cursor_tile(),
            block: self.controller.state.block,
            rotation: self.controller.state.rotation,
            mode,
            line_plans: if mode == PlaceMode::Placing {
                self.controller.state.line_plans.clone()
            } else {
                Vec::new()
            },
            break_rect: match self.drag {
                Some(drag) if drag.kind == DragKind::Break => Some((drag.start, drag.last)),
                _ => None,
            },
        }
    }

    /// World pixel center of the cursor tile (Java `Tile.worldx()`).
    fn cursor_world(&self) -> Option<(f32, f32)> {
        let (x, y) = self.cursor_tile()?;
        let unit = mind_core::config::TILESIZE as f32;
        Some((x as f32 * unit, y as f32 * unit))
    }

    /// `InputHandler.selectedUnit()` (`InputHandler.java:2148-2163`):
    /// `Units.closest(player.team(), mouseX, mouseY, 40, u -> u.isAI() &&
    /// u.playerControllable)`, accepted only when the hitbox grown by 6 on each
    /// side contains the cursor.
    fn selected_unit_for_control(&self) -> Option<i32> {
        let (wx, wy) = self.aim_world()?;
        let team = self.player_team();
        let best = self
            .selectable
            .iter()
            .filter(|unit| unit.team == team && unit.commandable)
            .filter_map(|unit| {
                let dx = unit.x - wx;
                let dy = unit.y - wy;
                let dst2 = dx * dx + dy * dy;
                (dst2 <= CONTROL_RADIUS * CONTROL_RADIUS).then_some((dst2, unit))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let unit = best.1;
        let half = unit.hit_size / 2.0 + CONTROL_HITBOX_GROW;
        ((unit.x - wx).abs() <= half && (unit.y - wy).abs() <= half).then_some(unit.id)
    }

    /// `InputHandler.selectedControlBuild()` (`InputHandler.java:2165-2171`):
    /// a same-team building with `canControlSelect(player.unit())` under the
    /// cursor — `CoreBlock` (core respawn) and the `PayloadConveyor`/
    /// `PayloadRouter` family (enter payload).
    ///
    /// Turrets are `ControlBlock`s, NOT `canControlSelect`; the port routes
    /// them through `selected_unit_for_control`'s ControlBlock branch, which
    /// needs H01's block-unit model (skipped, recorded).
    fn selected_control_build(&self) -> Option<(i32, i32)> {
        let (x, y) = self.cursor_tile()?;
        let kind = self.block_kind_at(x, y)?;
        let payload_build = matches!(kind, BlockKind::PayloadConveyor | BlockKind::PayloadRouter);
        if kind != BlockKind::CoreBlock && !payload_build {
            return None;
        }
        let host = self.host.clone()?;
        let team = host.bind().building_team(x, y)?;
        if team != self.player_team() {
            return None;
        }
        if !payload_build {
            // `CoreBlock.canControlSelect(unit)` = `player.isPlayer()`.
            return Some((x, y));
        }
        // `PayloadConveyor.canControlSelect` (PayloadConveyor.java:85-87):
        // `unit.tileOn().build == this` (the `spawnedByCore`/payload gate is
        // approximated by the tile condition; the sim half of
        // `BuildingControlSelect` is H01's).
        let (px, py) = host.bind().player_unit_position()?;
        let (tx, ty) = (WorldGrid::to_tile(px), WorldGrid::to_tile(py));
        (tx == x && ty == y).then_some((x, y))
    }

    fn mouse_button(&mut self, bindings: &BindingState, button: &str, down: bool) {
        let Some(key) = mouse_key_name(button) else {
            return;
        };
        if down {
            self.pressed.insert(key.to_owned());
        } else {
            self.pressed.remove(key);
        }
        // `Binding.select`/`deselect`/`break_block`/`pick`/`command_queue` own
        // the pointer gestures (T9); the raw button is only their default key
        // (`DesktopInput` reads the bindings, not the button).
        let uses = |id: u16| matches!(bindings.value(id), BindingValue::Key(name) if name == key);
        let select = uses(ids::SELECT);
        let right = uses(ids::BREAK_BLOCK) || uses(ids::DESELECT);
        let middle = uses(ids::PICK) || uses(ids::COMMAND_QUEUE);
        if down {
            if select {
                self.left_press(bindings);
            }
            if right {
                self.right_press(bindings);
            }
            if middle {
                if self.controller.state.command_mode {
                    // DI:924-934: command mode queues the order on the middle
                    // click (`Binding.commandQueue`); outside it, pick (T5).
                    self.command_tap_queued();
                } else {
                    self.pick_block();
                }
            }
        } else {
            if select {
                self.left_release();
            }
            if right {
                self.right_release(bindings);
            }
        }
        // Every other binding sharing the button still gets its key-down edge
        // (`pan` = mouseForward, `mouseMove` = mouseBack, user rebinds).
        let mut fired: Vec<&'static str> = Vec::new();
        for (index, bind) in KeyBindTable::all().iter().enumerate() {
            if POINTER_BINDS.contains(&bind.name) {
                continue;
            }
            if uses(index as u16) {
                fired.push(bind.name);
            }
        }
        for name in fired {
            if down {
                self.dispatch_action(bindings, name);
            }
        }
    }

    fn left_press(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        // `DesktopInput.update` L408-421: Ctrl+select possesses an own AI unit
        // (`Call.unitControl`) or a controllable building
        // (`Call.buildingControlSelect`). Java does not return here, so the
        // press still runs its normal select/place handling.
        if self.key_down(bindings, ids::CONTROL) {
            if let Some(id) = self.selected_unit_for_control() {
                self.emit_action(RemoteAction::UnitControl { unit: Some(id) });
                self.shooting = false;
            } else if let Some((bx, by)) = self.selected_control_build() {
                self.emit_action(RemoteAction::BuildingControlSelect {
                    x: bx as i16,
                    y: by as i16,
                });
            }
        }
        if self.controller.state.command_mode {
            self.drag = Some(Drag {
                kind: DragKind::SelectRect,
                start: (x, y),
                last: (x, y),
                moved: false,
            });
            self.controller.state.command_rect = None;
            return;
        }
        // `pollInputPlayer` select tap (`DesktopInput.java:704-705`): a click
        // with copied plans flushes them to the player queue and keeps them
        // selected for a repeat paste (`validPlace` filters inside).
        if !self.controller.state.select_plans.is_empty() {
            let plans = self.controller.state.select_plans.clone();
            if let Some(mut host) = self.host.clone() {
                for plan in plans {
                    let _ = host.bind_mut().queue_player_place_config(
                        plan.x as i16,
                        plan.y as i16,
                        plan.block,
                        plan.rotation,
                        plan.config,
                    );
                }
            }
            return;
        }
        if self.controller.state.block.is_none() {
            // `InputHandler.java:726` (`!checkConfigTap() && selected != null &&
            // !tryRepairDerelict(selected)`; H08 T11): a derelict building
            // under the cursor queues its repair before the
            // tap-player/tile-tap chain.
            if let Some(mut host) = self.host.clone()
                && host.bind_mut().try_repair_derelict(x, y)
            {
                return;
            }
            // `DesktopInput.java:701-721` order: `tryTapPlayer`, `tileTapped`,
            // the active build plan, then `tryStopMine`/`tryBeginMine`;
            // anything consumed keeps `player.shooting` false.
            if let Some((wx, wy)) = self.aim_world()
                && self.can_tap_player(wx, wy)
            {
                self.dropping_item = true;
                return;
            }
            if self.tile_tapped(x, y) {
                return;
            }
            if self
                .host
                .as_ref()
                .is_some_and(|host| host.bind().player_actively_building())
            {
                return;
            }
            if self.try_mine_at(x, y) {
                return;
            }
            // `shouldShoot = !scene.hasMouse() && !locked && !state.isEditor()`;
            // the UI gates ran in `MindInput::input`.
            self.shooting = !self.locked;
            if self.shooting {
                // `HintsFragment.desktopShoot` (`player.shooting`).
                self.push_hint("shoot");
            }
            return;
        }
        if !self.controller.state.is_building {
            return;
        }
        // `tileX/tileY`: even-size blocks are centred on the cursor.
        let Some((x, y)) = self.placement_cursor_tile() else {
            return;
        };
        self.controller.state.begin_place();
        // A fresh drag never starts with a latched wheel override
        // (`pollInputPlayer` L762-768 clears it on every frame between drags).
        self.controller.state.override_line_rotation = false;
        self.drag = Some(Drag {
            kind: DragKind::Place,
            start: (x, y),
            last: (x, y),
            moved: false,
        });
        self.update_place_line(bindings, (x, y), (x, y));
    }

    fn left_release(&mut self) {
        // `DesktopInput.java:507-509`: releasing select stops shooting; the
        // release also resolves a pending tap-unit gesture (`tryDropItems`).
        self.shooting = false;
        self.try_drop_items();
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag.kind {
            DragKind::Place => self.commit_place(drag),
            DragKind::SelectRect => self.commit_select(drag),
            DragKind::Break => {}
        }
    }

    /// `InputHandler.canTapPlayer` (`InputHandler.java:2032-2043`): no block
    /// selected, the cursor is within `playerSelectRange` (11) of the player
    /// unit and the unit carries items.
    fn can_tap_player(&self, wx: f32, wy: f32) -> bool {
        if self.controller.state.block.is_some() {
            return false;
        }
        let Some(host) = self.host.clone() else {
            return false;
        };
        let guard = host.bind();
        let Some((px, py)) = guard.player_unit_position() else {
            return false;
        };
        let dx = px - wx;
        let dy = py - wy;
        if dx * dx + dy * dy > PLAYER_SELECT_RANGE * PLAYER_SELECT_RANGE {
            return false;
        }
        guard
            .player_unit_stack()
            .is_some_and(|(_, amount)| amount > 0)
    }

    /// `InputHandler.tryDropItems` (`InputHandler.java:2286-2308`): a pending
    /// tap-unit gesture on release deposits into an accepting building or drops
    /// the stack (`Call.dropItem`).
    fn try_drop_items(&mut self) {
        if !self.dropping_item {
            return;
        }
        self.dropping_item = false;
        let Some(host) = self.host.clone() else {
            return;
        };
        if host.bind().is_paused() {
            return;
        }
        let Some((item_name, amount)) = host.bind().player_unit_stack() else {
            return;
        };
        if amount <= 0 {
            return;
        }
        let Some((wx, wy)) = self.aim_world() else {
            return;
        };
        // Released back on the unit: `canTapPlayer` -> no action.
        if self.can_tap_player(wx, wy) {
            return;
        }
        let (tx, ty) = (WorldGrid::to_tile(wx), WorldGrid::to_tile(wy));
        let now_ms = Time::singleton().get_ticks_msec();
        // Bind the probe to a local: a `match host.bind()...` scrutinee keeps
        // the shared guard alive through the arms, and `transfer_item` needs
        // `bind_mut`.
        let accepts = host.bind().player_can_deposit_at(tx, ty);
        match accepts {
            Some(true) => {
                if now_ms.saturating_sub(self.last_deposit_ms) >= ITEM_DEPOSIT_COOLDOWN_MS {
                    self.transfer_item(tx, ty, &item_name, amount, true);
                    self.last_deposit_ms = now_ms;
                }
            }
            // Accepting building on cooldown: Java neither deposits nor drops.
            Some(_) => {}
            None => {
                let (Ok(x), Ok(y)) = (i16::try_from(tx), i16::try_from(ty)) else {
                    return;
                };
                self.emit_action(RemoteAction::Inventory {
                    kind: InventoryKind::DropItem,
                    x,
                    y,
                    item: None,
                    amount: 0,
                    angle: 0.0,
                });
            }
        }
    }

    /// `InputHandler.tryStopMine`/`tryBeginMine`: toggles the player unit's
    /// mine order for a no-selection left press. Returns `true` when consumed.
    fn try_mine_at(&self, x: i32, y: i32) -> bool {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let Some(mut host) = self.host.clone() else {
            return false;
        };
        let mut guard = host.bind_mut();
        if !guard.player_can_mine(tx, ty) {
            return false;
        }
        if guard.player_mine_target() == Some((tx, ty)) {
            guard.set_player_mine(None);
        } else {
            guard.set_player_mine(Some((tx, ty)));
        }
        true
    }

    /// `build.interactable(player.team())` (`Tile.interactable`,
    /// `Tile.java:576-578`): own team or derelict.
    fn tile_interactable(&self, x: i32, y: i32) -> bool {
        let team = self.player_team();
        self.host.as_ref().is_some_and(|host| {
            host.bind()
                .building_team(x, y)
                .is_some_and(|tile_team| tile_team == team || tile_team == 0)
        })
    }

    /// `build.block.configurable` for the tap dispatch (H02 T6): whether the
    /// panel may switch to the tapped building.
    fn block_configurable_at(&self, x: i32, y: i32) -> bool {
        let Some(host) = self.host.clone() else {
            return false;
        };
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return false;
        };
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        guard
            .grid()
            .block_at(TilePos::new(tx, ty))
            .and_then(|block| content.block(block))
            .is_some_and(|def| def.configurable)
    }

    /// `build.block.synthetic()` (`Block.synthetic()` = `update || destructible`).
    fn block_synthetic_at(&self, x: i32, y: i32) -> bool {
        let Some(host) = self.host.clone() else {
            return false;
        };
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return false;
        };
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        guard
            .grid()
            .block_at(TilePos::new(tx, ty))
            .and_then(|block| content.block(block))
            .is_some_and(mind_core::maps::filters::block_info::synthetic)
    }

    /// `tileTapped` (`InputHandler.java:1973-2030`): a left tap on a placed
    /// building opens its config UI, or its inventory when it stores items.
    /// Returns `true` when the tap was consumed (`InputHandler.tileTapped`
    /// returns `consumed`; `DesktopInput.java:701` gates shooting on it).
    fn tile_tapped(&mut self, x: i32, y: i32) -> bool {
        // `if(build == null){ inv.hide(); config.hideConfig(); return false; }`
        if !self
            .host
            .as_ref()
            .is_some_and(|host| host.bind().building_team(x, y).is_some())
        {
            self.effects.push(Effect::HidePanels);
            self.config_selected = None;
            return false;
        }
        // `config.isShown() && config.getSelected().onConfigureBuildTapped(build)`
        // (`InputHandler.java:1993-2005`; H02 T6, PAR-0220): the building whose
        // panel is open reacts to the second tap (bridge/node/driver link). Java
        // runs this for configurable and non-configurable buildings alike, before
        // any `tapped()` handling.
        if let Some((sx, sy)) = self.config_selected {
            let (Ok(sx16), Ok(sy16), Ok(other_x), Ok(other_y)) = (
                i16::try_from(sx),
                i16::try_from(sy),
                i16::try_from(x),
                i16::try_from(y),
            ) else {
                self.config_selected = None;
                return false;
            };
            let Some(mut host) = self.host.clone() else {
                return false;
            };
            // Read-only preview (the guard drops at the end of the statement).
            let preview = host
                .bind()
                .preview_config_tap(i32::from(sx16), i32::from(sy16), x, y);
            if preview < 0 {
                // The selected building is gone: forget the stale selection.
                self.config_selected = None;
            } else {
                host.bind_mut().enqueue_sim_command(SimCommand::ConfigTap {
                    x: sx16,
                    y: sy16,
                    other_x,
                    other_y,
                });
                match preview {
                    0 => {
                        // Java `return false`: consumed, the panel stays on the
                        // selected building (`inv.hide()` at the end).
                        self.effects.push(Effect::HideInventory);
                        return true;
                    }
                    1 => {
                        // Java `deselect(); return false`.
                        self.config_selected = None;
                        self.effects.push(Effect::HidePanels);
                        return true;
                    }
                    _ => {
                        // Java `return true`: the panel agrees to switch. A
                        // configurable, interactable target opens its own panel
                        // below; every other building consumes the tap and
                        // closes the panel (`config.hideConfig()`).
                        self.config_selected = None;
                        if !(self.block_configurable_at(x, y) && self.tile_interactable(x, y)) {
                            self.effects.push(Effect::HidePanels);
                            return true;
                        }
                    }
                }
            }
        }
        // `build.interactable(player.team())`: enemy buildings never open a
        // panel or inventory (IH:1980-1990).
        if !self.tile_interactable(x, y) {
            self.effects.push(Effect::HideInventory);
            return false;
        }
        // `build.block.configurable && build.interactable(team)`.
        // `SwitchBlock.SwitchBuild.configTapped`: a tap toggles `enabled` and
        // returns false, so no panel opens (PAR-0108).
        if self.block_kind_at(x, y) == Some(BlockKind::SwitchBlock) {
            let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
                return false;
            };
            let Some(host) = self.host.clone() else {
                return false;
            };
            let enabled = match host.bind().building_enabled(x, y).try_to::<bool>() {
                Ok(enabled) => enabled,
                Err(_) => return false,
            };
            // `SwitchBlock.SwitchBuild.clickSound.at(this)`
            // (`SwitchBlock.java:58-62`; T9): the UI click goes through the
            // `MindUi` signal like every other UI sound (plan 18).
            if let Some(mut ui) = host.try_get_node_as::<godot::classes::Node>("/root/MindUi") {
                let name = GString::from("click");
                ui.call("play_ui_sound", &[name.to_variant()]);
            }
            self.emit_action(RemoteAction::Configure {
                x: tx,
                y: ty,
                value: mind_core::world::config::ConfigValue::Bool(!enabled),
            });
            // `if(!showedInventory) inv.hide();`
            self.effects.push(Effect::HideInventory);
            return true;
        }
        // `Door.tapped` (`Door.java:148-154`, T13): a tap toggles unless a unit
        // stands in an open door; `Door.configurable` is false upstream, so no
        // panel opens. The 60-tick throttle lives in `DoorBehavior::configured`.
        if matches!(
            self.block_kind_at(x, y),
            Some(BlockKind::Door | BlockKind::AutoDoor)
        ) {
            let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
                return false;
            };
            let Some(host) = self.host.clone() else {
                return false;
            };
            let open = host.bind().building_config_json(x, y) == "true";
            if open && host.bind().building_has_unit_overlap(x, y) {
                // `Door.tapped` returns without toggling: the tap is not consumed.
                self.effects.push(Effect::HideInventory);
                return false;
            }
            self.emit_action(RemoteAction::Configure {
                x: tx,
                y: ty,
                value: mind_core::world::config::ConfigValue::Bool(!open),
            });
            self.effects.push(Effect::HideInventory);
            return true;
        }
        if let Some(spec) = self.config_spec(x, y) {
            self.config_selected = Some((x, y));
            self.effects.push(Effect::OpenBlockConfig {
                x,
                y,
                screen: self.mouse,
                spec,
            });
            self.effects.push(Effect::HideInventory);
            return true;
        }
        // `build.interactable(team) && build.block.synthetic() &&
        // build.block.hasItems && build.items.total() > 0` (IH:1998-2004).
        if self.block_synthetic_at(x, y) {
            let Some(host) = self.host.clone() else {
                return false;
            };
            let items = host.bind().block_items_json(x, y);
            if let Some(items) = items {
                self.effects.push(Effect::OpenBlockInventory {
                    x,
                    y,
                    screen: self.mouse,
                    items,
                });
                return true;
            }
        }
        // `if(!showedInventory) inv.hide();`
        self.effects.push(Effect::HideInventory);
        false
    }

    fn right_press(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        // `DesktopInput.java:608-610` runs before the select/deselect taps: the
        // right tap clears `mineTile` when no placement mode, unit plans or
        // command mode owns the tap. Do not return: the break drag still
        // starts (`DI:740-748`).
        let plans = self
            .host
            .as_ref()
            .map(|host| host.bind().player_plan_count())
            .unwrap_or(0);
        if mind_core::input::desktop::deselect_stops_mining(
            self.controller.state.block.is_some(),
            self.controller.state.command_mode,
            plans,
        ) && let Some(mut host) = self.host.clone()
        {
            host.bind_mut().set_player_mine(None);
        }
        if self.controller.state.command_mode {
            self.command_tap(bindings);
            return;
        }
        // `DesktopInput.java:737` (`Binding.deselect` shares the right button
        // with breaking): while placing, the press clears the block selection
        // instead of entering breaking mode - otherwise every left press stays
        // trapped in the placement branch and ore clicks can never mine
        // (PAR-0045).
        if self.controller.state.block.is_some() {
            self.controller.state.select_block(None);
            self.controller.state.override_line_rotation = false;
            if let Some(mut host) = self.host.clone() {
                host.bind_mut().select_block(GString::from("air"));
            }
            return;
        }
        // `DesktopInput.java:741-744`: the deselect tap clears copied plans
        // (before entering breaking mode).
        if !self.controller.state.select_plans.is_empty() {
            self.controller.state.select_plans.clear();
            self.refresh_select_preview();
            return;
        }
        // `Binding.breakBlock` (`DesktopInput.pollInputPlayer`): the right
        // button always enters breaking mode; config/inventory open on the
        // left tap.
        self.controller.state.begin_break();
        // `DesktopInput.java:746-748`: the break drag latches the raw cursor for
        // a potential F-cut.
        self.schem_start = self.raw_cursor_tile();
        self.drag = Some(Drag {
            kind: DragKind::Break,
            start: (x, y),
            last: (x, y),
            moved: false,
        });
    }

    fn right_release(&mut self, bindings: &BindingState) {
        // `DesktopInput.java:799`: the break release also resolves a pending
        // tap-unit gesture (`tryDropItems`).
        self.try_drop_items();
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        // Cut: `keyRelease(breakBlock) && keyDown(schematicSelect)` creates the
        // schematic BEFORE `removeSelection` and pastes it after
        // (`DesktopInput.java:765-790`).
        let cut = if self.key_down(bindings, ids::SCHEMATIC_SELECT) {
            match (self.schem_start, self.raw_cursor_tile()) {
                (Some(start), Some(end)) => {
                    Some((end, host.bind().copy_schematic_plans(start, end, end)))
                }
                _ => None,
            }
        } else {
            None
        };
        let mut commands: Vec<SimCommand> = Vec::new();
        // `InputHandler.removeSelection` clamps the rect to `maxLength` (100;
        // 64 with F held for the cut, `DesktopInput.java:789`).
        let max_length = if self.key_down(bindings, ids::SCHEMATIC_SELECT) {
            64
        } else {
            100
        };
        let r = mind_core::input::placement::normalize_area(
            drag.start.0,
            drag.start.1,
            drag.last.0,
            drag.last.1,
            0,
            false,
            max_length,
        );
        {
            let guard = host.bind();
            let world = HostWorld {
                grid: guard.grid(),
                content: guard.content_registry(),
                host: &guard,
            };
            'outer: for y in r.y..=r.y2 {
                for x in r.x..=r.x2 {
                    if commands.len() >= DRAG_TILE_CAP {
                        break 'outer;
                    }
                    if world.block_at(x, y) == BlockId::AIR {
                        continue;
                    }
                    let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
                        continue;
                    };
                    // `player: Some` routes to the player unit's build queue
                    // (`InputHandler.removeSelection`; PAR-0098).
                    commands.push(SimCommand::Break {
                        x,
                        y,
                        player: Some(0),
                    });
                }
            }
        }
        self.enqueue_commands(&commands);
        self.controller.state.place_mode = PlaceMode::None;
        // `removeSelection` plan half (`InputHandler.java:1905-1935`): drop the
        // player's queued plans and the team ghosts inside the rect.
        if let Some(mut host) = self.host.clone() {
            host.bind_mut().remove_plans_in(
                drag.start.0,
                drag.start.1,
                drag.last.0,
                drag.last.1,
                max_length,
            );
        }
        if let Some((end, plans)) = cut {
            self.adopt_select_plans(plans, end);
        }
    }

    fn mouse_move(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        // `pollInputPlayer` L630: copied plans follow the raw cursor.
        if !self.controller.state.select_plans.is_empty() {
            let (sx, sy) = self.schematic_origin;
            let (dx, dy) = (x - sx, y - sy);
            if dx != 0 || dy != 0 {
                for plan in &mut self.controller.state.select_plans {
                    plan.x += dx;
                    plan.y += dy;
                }
                self.schematic_origin = (x, y);
                self.refresh_select_preview();
            }
        }
        // `tileX/tileY`: an active placement drag tracks the offset cursor tile.
        let position = if self
            .drag
            .as_ref()
            .is_some_and(|drag| drag.kind == DragKind::Place)
        {
            self.placement_cursor_tile().unwrap_or((x, y))
        } else {
            (x, y)
        };
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if drag.last != position || position != drag.start {
            drag.last = position;
            drag.moved = true;
        }
        let drag = *drag;
        if drag.kind == DragKind::Place {
            self.update_place_line(bindings, drag.start, position);
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
        let block = self.controller.state.block;
        let block_replace = self.block_replace;
        {
            let guard = host.bind();
            let world = HostWorld {
                grid: guard.grid(),
                content: guard.content_registry(),
                host: &guard,
            };
            let block_meta = self.line_block_for(&world);
            let rules = block.and_then(|block| {
                world
                    .content
                    .map(|content| mind_core::input::line_rules::line_rules_for(content, block))
            });
            // `changePlacementPath` runs inside `iterateLine` regardless of
            // `blockreplace`; the rules half (`getReplacement` /
            // `handlePlacementLine`) is the gated part (IH:1952-1961).
            let active = rules.as_ref().filter(|_| block_replace);
            match rules.as_ref().and_then(|rules| rules.node) {
                Some(rule) => {
                    let hooks = mind_core::input::line_rules::NodeHooks {
                        rule,
                        world: &world,
                        block: block.unwrap_or(BlockId::AIR),
                    };
                    self.controller.state.update_line_with(
                        &world,
                        block_meta.as_ref(),
                        TilePos::new(start.0 as i16, start.1 as i16),
                        TilePos::new(end.0 as i16, end.1 as i16),
                        &params,
                        &hooks,
                        active,
                    );
                }
                None => {
                    self.controller.state.update_line_with(
                        &world,
                        block_meta.as_ref(),
                        TilePos::new(start.0 as i16, start.1 as i16),
                        TilePos::new(end.0 as i16, end.1 as i16),
                        &params,
                        &mind_core::input::line::NoLineHooks,
                        active,
                    );
                }
            }
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

    /// Block metadata for `iterateLine` (flags from the Java block class;
    /// `None` when unknown).
    fn line_block_for(&self, world: &HostWorld<'_>) -> Option<LineBlock> {
        let block = self.controller.state.block?;
        let content = world.content?;
        let def = content.block(block)?;
        Some(mind_core::input::line::line_block_from_def(block, def))
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
        // Java `InputHandler.placeBlock` places on `player.team()` — the
        // campaign's default team (`Team.sharded`), not the derelict id.
        let team = self
            .host
            .clone()
            .map(|host| host.bind().player_team())
            .unwrap_or(0);
        // `Block.nextConfig` (`Block.java:827-832`): a block with a remembered
        // config (`saveConfig`) queues through the config-carrying path; line
        // plans may carry their own config (T6b/T8).
        let remembered = self
            .last_config
            .get(&block.raw())
            .cloned()
            .filter(|_| self.block_save_config(block));
        let mut commands: Vec<SimCommand> = Vec::new();
        let mut configured: Vec<(i16, i16, BlockId, u8, mind_core::world::config::ConfigValue)> =
            Vec::new();
        for plan in plans.iter().take(DRAG_TILE_CAP) {
            if plan.breaking || plan.block == BlockId::AIR {
                continue;
            }
            let config = if !matches!(plan.config, mind_core::world::config::ConfigValue::None) {
                plan.config.clone()
            } else if plan.block == block {
                remembered
                    .clone()
                    .unwrap_or(mind_core::world::config::ConfigValue::None)
            } else {
                mind_core::world::config::ConfigValue::None
            };
            if matches!(config, mind_core::world::config::ConfigValue::None) {
                commands.push(SimCommand::Place {
                    x: plan.x as i16,
                    y: plan.y as i16,
                    block: plan.block.raw(),
                    rotation: plan.rotation as i8,
                    team,
                    // Player input queues on the player unit (`flushPlans` ->
                    // `BuilderComp.addBuild`); the builder constructs it (PAR-0098).
                    player: Some(0),
                });
            } else {
                configured.push((
                    plan.x as i16,
                    plan.y as i16,
                    plan.block,
                    plan.rotation,
                    config,
                ));
            }
        }
        if let Some(mut host) = self.host.clone() {
            for (x, y, plan_block, rotation, config) in configured {
                let _ = host
                    .bind_mut()
                    .queue_player_place_config(x, y, plan_block, rotation, config);
            }
        }
        self.enqueue_commands(&commands);
        self.controller.state.line_plans.clear();
        self.controller.state.line.clear();
        self.controller.state.place_mode = PlaceMode::None;
        // `pollInputPlayer` L762-768: the latch clears once the drag is over.
        self.controller.state.override_line_rotation = false;
        self.controller.state.preview.clear();
    }

    /// Commits a command-mode selection drag (`selectUnitsRect`).
    fn commit_select(&mut self, drag: Drag) {
        let unit = mind_core::config::TILESIZE as f32;
        let (x1, x2) = (drag.start.0.min(drag.last.0), drag.start.0.max(drag.last.0));
        let (y1, y2) = (drag.start.1.min(drag.last.1), drag.start.1.max(drag.last.1));
        if !drag.moved {
            if let Some((x, y)) = self.cursor_world() {
                match select_unit_tap(&self.selectable, self.player_team(), x, y, UNIT_TAP_RADIUS) {
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
                            select_typed_units(
                                &self.selectable,
                                self.player_team(),
                                type_id.unwrap_or(0),
                                &mut out,
                            );
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
            select_units_rect(&self.selectable, self.player_team(), rect, &mut out);
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
        if !rotate_on_scroll && !zoom_on_scroll {
            return;
        }
        let diagonal_held = self.key_down(bindings, ids::DIAGONAL_PLACEMENT);
        let block = self.controller.state.block;
        let placing = self.controller.state.is_building && block.is_some();
        let ctx = mind_core::input::desktop::ScrollCtx {
            diagonal_held,
            rotate_placed_held: self.key_down(bindings, ids::ROTATE_PLACED),
            zoom_is_rotate: bindings.value(ids::ROTATE) == bindings.value(ids::ZOOM),
            placing,
            block_rotates: block.is_some_and(|block| self.block_rotates(block)),
            select_plans: !self.controller.state.select_plans.is_empty(),
            cursor_build_quick_rotates: self.cursor_build_quick_rotates(),
        };
        let route = mind_core::input::desktop::route_scroll(&ctx);
        let step = if delta > 0.0 { 1 } else { -1 };
        if route.rotate_selection {
            // `pollInputPlayer` L762-768 (snap to the drag angle) runs BEFORE
            // L836-849 (`rotation = mod(rotation + axisTap, 4)`): a wheel during
            // a fresh drag snaps to the drag angle, adds the step and latches
            // `overrideLineRotation`, so every line step gets `input.rotation`.
            let drag = self.drag.filter(|drag| drag.kind == DragKind::Place);
            if placing
                && let Some(drag) = drag
                && !self.controller.state.override_line_rotation
                && !diagonal_held
                && drag.start != drag.last
            {
                self.controller.state.rotation = mind_core::input::placement::line_angle_rotation(
                    drag.start.0,
                    drag.start.1,
                    drag.last.0,
                    drag.last.1,
                );
                self.controller.state.override_line_rotation = true;
            }
            self.controller.state.rotation =
                (self.controller.state.rotation as i32 + step).rem_euclid(4) as u8;
            if placing && let Some(drag) = drag {
                self.update_place_line(bindings, drag.start, drag.last);
            } else if !placing && !self.controller.state.select_plans.is_empty() {
                self.rotate_select_plans(step);
            }
        }
        if route.rotate_placed
            && let Some((x, y)) = self.cursor_tile()
        {
            // `Call.rotateBlock(player, cursor.build, axisTap(rotate) > 0)`.
            self.emit_action(RemoteAction::Rotate {
                x: x as i16,
                y: y as i16,
                direction: delta > 0.0,
            });
        }
        if route.zoom {
            self.zoom(delta);
        }
    }

    /// `Block.rotate` for a block id.
    fn block_rotates(&self, block: BlockId) -> bool {
        self.host
            .clone()
            .is_some_and(|host| host.bind().block_rotates(block))
    }

    /// The building under the cursor is interactable and
    /// `block.rotate && block.quickRotate` (`DesktopInput.java:924-926`).
    fn cursor_build_quick_rotates(&self) -> bool {
        let Some((x, y)) = self.cursor_tile() else {
            return false;
        };
        let Some(host) = self.host.clone() else {
            return false;
        };
        let guard = host.bind();
        let team = self.player_team();
        let interactive = guard
            .building_team(x, y)
            .is_some_and(|building_team| building_team == team || building_team == 0);
        if !interactive {
            return false;
        }
        let Some(content) = guard.content_registry() else {
            return false;
        };
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let Some(block) = guard.grid().block_at(TilePos::new(tx, ty)) else {
            return false;
        };
        content.block(block).is_some_and(|def| def.quick_rotate) && guard.block_rotates(block)
    }

    // ---- world queries ----

    /// Block kind at a tile (`None` out of bounds/unloaded).
    pub fn block_kind_at(&self, x: i32, y: i32) -> Option<BlockKind> {
        let host = self.host.clone()?;
        let guard = host.bind();
        let content = guard.content_registry()?;
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let block = guard.grid().block_at(TilePos::new(tx, ty))?;
        content.block(block).map(|def| def.kind)
    }

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
        // `SwitchBlock.configTapped` toggles on tap; it never opens a panel.
        if def.kind == BlockKind::SwitchBlock {
            return None;
        }
        // H02 T6: link blocks have no option list — the panel shows only the
        // title, the current link and the close button. The link itself is made
        // by tapping another block while this panel is open.
        if matches!(
            def.kind,
            BlockKind::ItemBridge
                | BlockKind::BufferedItemBridge
                | BlockKind::LiquidBridge
                | BlockKind::MassDriver
                | BlockKind::PayloadMassDriver
                | BlockKind::PowerNode
                | BlockKind::LongPowerNode
        ) {
            let mut spec = serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "text": false,
                "multiline": false,
                "color": false,
                "options": [],
                "link": true,
            });
            let current = serde_json::from_str::<serde_json::Value>(
                guard.building_config_json(x, y).to_string().as_str(),
            )
            .ok()
            .filter(|value| value.get("type").and_then(|kind| kind.as_str()) == Some("point"));
            if let Some(current) = current {
                spec["current"] = current;
            }
            return Some(spec.to_string());
        }
        // `Reconstructor.buildConfiguration` (`Reconstructor.java:215-240`):
        // with `showAllCommands = true` it lists every unit command (the held
        // payload's command subset needs a payload-model this panel cannot see).
        let reconstructor = def.kind == BlockKind::Reconstructor;
        let mut options: Vec<serde_json::Value> = Vec::new();
        if reconstructor {
            for entry in content.entries(ContentType::UnitCommand) {
                if let Some(name) = entry.name {
                    options.push(serde_json::json!({
                        "name": name,
                        "label": format!("command.{name}"),
                    }));
                }
            }
        }
        // A behavior that declares its config kinds (`Block.config(...)`) owns
        // the picker; undeclared blocks keep the item/liquid-container fallback.
        let declared = guard.block_config_kinds(block);
        // `UnitFactory.buildConfiguration` / `PayloadSource`/`PayloadRouter`:
        // factory plans (or every unit), plus sortable blocks for payloads.
        if declared.contains(&ConfigKind::Unit) {
            if def.unit_plans.is_empty() {
                for entry in content.entries(ContentType::Unit) {
                    if let Some(name) = entry.name {
                        options.push(serde_json::json!({
                            "name": name,
                            "label": format!("unit.{name}.name"),
                        }));
                    }
                }
            } else {
                for plan in &def.unit_plans {
                    if let Some(unit) = content.unit(plan.unit) {
                        let name = unit.name.clone();
                        options.push(serde_json::json!({
                            "name": name,
                            "label": format!("unit.{name}.name"),
                        }));
                    }
                }
            }
        }
        if declared.contains(&ConfigKind::Block) {
            // `Constructor.canProduce` size window (`Init` min/max block size);
            // payload sources/routers sort any producible block.
            let (min_size, max_size) = match def.name.as_str() {
                "constructor" => (1, 2),
                "large-constructor" => (3, 4),
                _ => (1, i32::MAX),
            };
            for entry in content.entries(ContentType::Block) {
                let Some(name) = entry.name else {
                    continue;
                };
                let Some(candidate) = content.block_by_name(name) else {
                    continue;
                };
                if candidate.removed
                    || candidate.build_visibility == mind_core::content::BuildVisibility::Hidden
                    || candidate.kind == BlockKind::CoreBlock
                    || candidate.size < min_size
                    || candidate.size > max_size
                {
                    continue;
                }
                options.push(serde_json::json!({
                    "name": name,
                    "label": format!("block.{name}.name"),
                }));
            }
        }
        let item_options = options.is_empty()
            && (declared.contains(&ConfigKind::Item) || (declared.is_empty() && def.has_items));
        if item_options {
            for entry in content.entries(ContentType::Item) {
                if let Some(name) = entry.name {
                    options.push(serde_json::json!({
                        "name": name,
                        "label": format!("item.{name}.name"),
                    }));
                }
            }
        }
        let liquid_options =
            declared.contains(&ConfigKind::Liquid) || (declared.is_empty() && def.has_liquids);
        if options.is_empty() && liquid_options {
            for entry in content.entries(ContentType::Liquid) {
                if let Some(name) = entry.name {
                    options.push(serde_json::json!({
                        "name": name,
                        "label": format!("liquid.{name}.name"),
                    }));
                }
            }
        }
        let text = matches!(def.kind, BlockKind::LogicBlock | BlockKind::MessageBlock);
        let multiline = def.kind == BlockKind::MessageBlock;
        let color = def.kind == BlockKind::LightBlock;
        // Current value readbacks (`MessageBlock.message` / `LightBuild.color`).
        let current_text = if text {
            guard
                .building_config_string(x, y)
                .try_to::<String>()
                .ok()
                .filter(|text| !text.is_empty())
        } else {
            None
        };
        let current_color = if color {
            Some(guard.light_color(x, y).try_to::<i64>().unwrap_or_default())
        } else {
            None
        };
        // Reconstructor current command (`command == item` checked state,
        // `Reconstructor.java:237-240`): the content config id resolved to a name.
        let current_command = if reconstructor {
            serde_json::from_str::<serde_json::Value>(&guard.building_config_json(x, y).to_string())
                .ok()
                .filter(|value| value.get("type").and_then(|kind| kind.as_str()) == Some("content"))
                .and_then(|value| value.get("id").and_then(|id| id.as_u64()))
                .and_then(|id| {
                    content
                        .entries(ContentType::UnitCommand)
                        .into_iter()
                        .find(|entry| u64::from(entry.id) == id)
                        .and_then(|entry| entry.name)
                        .map(str::to_owned)
                })
        } else {
            None
        };
        let mut spec = serde_json::json!({
            "title": format!("block.{}.name", def.name),
            "block": def.name,
            "x": x,
            "y": y,
            "text": text,
            "multiline": multiline,
            "color": color,
            "options": options,
        });
        if let Some(current) = current_text {
            spec["current"] = serde_json::Value::from(current);
        }
        if let Some(current) = current_command {
            spec["current"] = serde_json::Value::from(current);
        }
        if let Some(current) = current_color {
            // The color picker replaces the option grid entirely.
            spec["current"] = serde_json::Value::from(current);
            spec["options"] = serde_json::Value::Array(Vec::new());
        }
        Some(spec.to_string())
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
}

/// Camera world coordinate for a viewport point (screen→tile center;
/// Java `Tile.worldx()` convention: the centre is `x * tilesize`).
fn camera_world_corner(camera: &Gd<MindCamera2D>, x: f32, y: f32) -> (f32, f32) {
    let tile = camera.bind().screen_to_tile(x as f64, y as f64);
    let unit = mind_core::config::TILESIZE as f32;
    (tile.x as f32 * unit, tile.y as f32 * unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mode_hold_tracks_the_shared_default_key() {
        // `DesktopInput.update` + `commandmodehold` default true: command mode
        // follows the held command key (`boost` shares `shiftLeft`).
        let mut bridge = DesktopBridge::new();
        // The whole `update` runs only in game (`Control.java:690-760`).
        bridge.in_game = true;
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
    fn mouse_back_sets_pressed() {
        // T9: `pan` (mouseForward) and `mouseMove` (mouseBack) are binding keys;
        // the raw mouse press latches their key name in `pressed`, so
        // `MindInput.pan_pressed`/`mouse_move_pressed` and the player pull see
        // the held state.
        let mut bridge = DesktopBridge::new();
        bridge.in_game = true;
        let bindings = BindingState::new();
        assert_eq!(bindings.name(ids::MOUSE_MOVE), Some("mouseBack"));
        assert_eq!(bindings.name(ids::PAN), Some("mouseForward"));
        bridge.handle(
            &bindings,
            RawEvent::MouseButton {
                button: "back".to_owned(),
                down: true,
                x: 10.0,
                y: 10.0,
            },
        );
        assert!(bridge.key_down(&bindings, ids::MOUSE_MOVE));
        assert!(!bridge.key_down(&bindings, ids::PAN));
        bridge.handle(
            &bindings,
            RawEvent::MouseButton {
                button: "back".to_owned(),
                down: false,
                x: 10.0,
                y: 10.0,
            },
        );
        assert!(!bridge.key_down(&bindings, ids::MOUSE_MOVE));

        bridge.handle(
            &bindings,
            RawEvent::MouseButton {
                button: "forward".to_owned(),
                down: true,
                x: 10.0,
                y: 10.0,
            },
        );
        assert!(bridge.key_down(&bindings, ids::PAN));
    }

    #[test]
    fn command_mode_off_on_menu() {
        // `Control.java:690-760`: `DesktopInput.update` only runs in game, so
        // the command key is inert on the menu.
        let mut bridge = DesktopBridge::new();
        let bindings = BindingState::new();
        assert!(!bridge.in_game);
        bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
        assert!(!bridge.controller.state.command_mode);
        assert!(!bridge.command_mode_allowed(&bindings));
    }

    #[test]
    fn command_mode_tap_toggles_when_hold_disabled() {
        // `commandmodehold=false`: `input.keyTap` toggles on the press edge.
        let mut bridge = DesktopBridge::new();
        bridge.set_command_mode_hold(false);
        // The tap path runs through the gameplay binding gate (`state.isGame()`).
        bridge.in_game = true;
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
        bridge.in_game = true;
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
        bridge.in_game = true;
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
        bridge.in_game = true;
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

    #[test]
    fn bindings_do_not_fire_outside_game() {
        // `Control.java:690-760`: gameplay binds are gated on `state.isGame()`;
        // the menu renders no world and the HUD fragments stay closed.
        let bindings = BindingState::new();
        assert_eq!(bindings.name(ids::SCHEMATIC_MENU), Some("t"));
        let mut bridge = DesktopBridge::new();
        bridge.handle(&bindings, RawEvent::key_down("t"));
        assert!(bridge.take_effects().is_empty(), "menu T is inert");
        bridge.handle(&bindings, RawEvent::key_up("t"));

        bridge.in_game = true;
        bridge.handle(&bindings, RawEvent::key_down("t"));
        let effects = bridge.take_effects();
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::OpenDialog { name, .. } if name == "schematics"
            )),
            "in game T opens schematics"
        );
    }

    #[test]
    fn campaign_keys_need_an_active_campaign() {
        // `DesktopInput.java:446-461`: research/planet open only in a campaign.
        let bindings = BindingState::new();
        let mut bridge = DesktopBridge::new();
        bridge.in_game = true;
        bridge.campaign_active = false;
        bridge.handle(&bindings, RawEvent::key_down("j"));
        assert!(
            !bridge
                .take_effects()
                .iter()
                .any(|effect| matches!(effect, Effect::OpenDialog { .. })),
            "J outside a campaign opens nothing"
        );
        bridge.campaign_active = true;
        bridge.handle(&bindings, RawEvent::key_down("n"));
        assert!(
            bridge.take_effects().iter().any(|effect| matches!(
                effect,
                Effect::OpenDialog { name, .. } if name == "planet"
            )),
            "N in a campaign opens the planet map"
        );
    }

    #[test]
    fn override_line_rotation_resets_when_the_drag_ends() {
        // Java DI:762-768: the wheel snap latch clears whenever the frame is
        // not a placement line, so a second drag after a wheel step gets
        // per-step rotations again.
        let mut bridge = DesktopBridge::new();
        bridge
            .controller
            .state
            .select_block(Some(BlockId::STONE_WALL));
        bridge.controller.state.override_line_rotation = true;
        bridge.controller.state.place_mode = PlaceMode::Placing;
        bridge.commit_place(Drag {
            kind: DragKind::Place,
            start: (1, 1),
            last: (1, 2),
            moved: true,
        });
        assert!(!bridge.controller.state.override_line_rotation);

        // The per-frame mirror also clears it outside a drag.
        bridge.controller.state.override_line_rotation = true;
        bridge.sync_line_override();
        assert!(!bridge.controller.state.override_line_rotation);

        // While a placement line is active the latch survives.
        bridge.controller.state.override_line_rotation = true;
        bridge.controller.state.place_mode = PlaceMode::Placing;
        bridge.sync_line_override();
        assert!(bridge.controller.state.override_line_rotation);
    }

    #[test]
    fn rotate_step_snaps_to_the_drag_angle_before_stepping() {
        // `pollInputPlayer` L762-768 runs before L836-849: a rotate tap during
        // a fresh drag snaps to the drag angle (upward drag = 1), then adds
        // the step and latches the override.
        let mut bridge = DesktopBridge::new();
        let bindings = BindingState::new();
        bridge
            .controller
            .state
            .select_block(Some(BlockId::STONE_WALL));
        bridge.drag = Some(Drag {
            kind: DragKind::Place,
            start: (5, 5),
            last: (5, 8),
            moved: true,
        });
        bridge.controller.state.override_line_rotation = false;
        bridge.rotate_placement_step(&bindings, 1);
        assert_eq!(bridge.controller.state.rotation, 2);
        assert!(bridge.controller.state.override_line_rotation);

        // A latched override skips the snap and only steps.
        bridge.rotate_placement_step(&bindings, 1);
        assert_eq!(bridge.controller.state.rotation, 3);
    }

    #[test]
    fn placement_view_reports_break_drag() {
        // G03 T2: the break rectangle is exposed while a right-drag is held.
        let mut bridge = DesktopBridge::new();
        bridge.controller.state.place_mode = PlaceMode::Breaking;
        bridge.drag = Some(Drag {
            kind: DragKind::Break,
            start: (1, 1),
            last: (3, 4),
            moved: true,
        });
        let view = bridge.placement_view();
        assert_eq!(view.break_rect, Some(((1, 1), (3, 4))));
        assert_eq!(view.mode, PlaceMode::Breaking);
        assert!(view.line_plans.is_empty());
    }
}
