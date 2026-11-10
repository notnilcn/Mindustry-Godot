// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DesktopBridge` — live desktop interaction for `MindInput` (plan 15 §3.1/§3.5).
//!
//! Translates captured [`RawEvent`]s through the pure `mind_core::input`
//! controller into [`SimCommand`]s enqueued on `MindSimHost` (they apply at the
//! next tick start, like the relay path). All branch logic lives in `mind-core`;
//! this file is Godot node plumbing only. UI-affecting actions are reported as
//! [`Effect`]s so `MindInput` can resolve the scene nodes.

mod actions;
mod command_mode;
mod config;
mod content;
mod effect;
mod helpers;
mod hotkeys;
mod keys;
mod mouse;
mod placement;
mod selection;
#[cfg(test)]
mod tests;

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
    line::rotate_plans_about, placement::SelectionShow, placement::selection_show,
    select_typed_units, select_unit_tap, select_units_rect,
};
use mind_core::world::build::can_replace;
use mind_core::world::{ConfigKind, TilePos, WorldGrid};

use crate::camera::MindCamera2D;
use crate::campaign::MindCampaign;
use crate::gdutil::coords::tile_i16;
use crate::sim_host::MindSimHost;

use content::{
    camera_world_corner, command_options_json, config_content_name, content_option_json,
    item_selection_shape,
};
pub use effect::Effect;
use helpers::{
    console_setting_enabled, parse_selectable_units, plan_config_spec_json, plan_drag_tile,
};

/// Maximum tiles one drag action emits (defensive; previews cap at `MAX_LENGTH`).
const DRAG_TILE_CAP: usize = 256;
/// `Build.validPlace` hit radius for tap selection (world pixels).
///
/// Same value as [`PLAYER_SELECT_RANGE`], but deliberately **not** the same
/// constant: that one is upstream's `InputHandler.playerSelectRange`
/// (`mobile ? 17f : 11f`, `InputHandler.java:55`) for the drop-on-player
/// gesture, so the mobile port will move it to 17 while this stays at 11.
const UNIT_TAP_RADIUS: f32 = 11.0;
/// Double-tap window for `selectTypedUnits` (`Time.timeSinceMillis < 300`).
const UNIT_TAP_INTERVAL_MS: u64 = 300;

/// `Vars.maxPingTextLength` (`Vars.java:111`): the `ping.text` prompt limit
/// and the `InputHandler.pingLocation` truncation length.
const MAX_PING_TEXT: usize = 40;

/// `Units.closest` search radius for `selectedUnit()` (`InputHandler.java:2152`).
const CONTROL_RADIUS: f32 = 40.0;
/// `selectedUnit()`'s hitbox grow per side (`InputHandler.java:2156`).
const CONTROL_HITBOX_GROW: f32 = 6.0;

/// Bindings whose world gesture runs directly from a pointer edge (T9); every
/// other binding sharing the same mouse key keeps its normal key-down edge.
const POINTER_BINDS: [&str; 5] = ["select", "deselect", "break_block", "pick", "command_queue"];

/// How close a cursor must be to the carrying player unit to start a drop
/// gesture (`Vars.playerSelectRange`, `InputHandler.java:2032-2043`).
///
/// Desktop half of `InputHandler.playerSelectRange` (`mobile ? 17f : 11f`,
/// `InputHandler.java:55`); see [`UNIT_TAP_RADIUS`] for why the shared value is
/// not a shared constant.
const PLAYER_SELECT_RANGE: f32 = 11.0;

/// `rules.itemDepositCooldown` default 0.5 s (`Rules.java`), the minimum gap
/// between two tap-unit deposits.
const ITEM_DEPOSIT_COOLDOWN_MS: u64 = 500;

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

/// `DesktopInput.splan` (`DesktopInput.java:43/657-665/710-717/800-813`): the
/// player's queued, still-unstarted build plan being dragged to a new tile.
#[derive(Debug, Clone)]
struct PlanDrag {
    /// Snapshot of the queued plan (`splan`; `x/y/rotation/config` mutate
    /// during the drag).
    plan: ClientPlan,
    /// Grab tile (`splan.x/splan.y` at the press).
    origin: (i32, i32),
    /// Mouse world point at the press (`buildPlanMouseOffsetX/Y` baseline).
    grab_world: (f32, f32),
    /// Live drag tile (`splan.x/splan.y`).
    tile: (i32, i32),
    /// `movedPlan`: the pointer changed the tile.
    moved: bool,
}

/// Read-only view for the placement overlay (`DesktopInput.drawBottom` /
/// `InputHandler.drawBreakSelection`; G03 T2) and the command drag box
/// (`InputHandler.drawUnitSelection`; GAPS #47).
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
    /// Live command-mode drag rect (`InputHandler.commandRect`), world pixels:
    /// `(commandRectX, commandRectY, mouseWorldX - commandRectX, mouseWorldY -
    /// commandRectY)` — the size is signed. `None` outside a held select drag;
    /// the committed tile rect the state keeps after release is not this.
    pub command_rect: Option<(f32, f32, f32, f32)>,
    /// Held-key selection (`DesktopInput.drawTop`, `DesktopInput.java:133-145`):
    /// the `SelectionShow` kind plus the latched start tile (`schemX/schemY`).
    /// The rectangle's end is [`Self::cursor`] (Java `cursorX/cursorY`).
    /// `None` while nothing draws (breaking, keyboard focus, no latch).
    pub selection: Option<(SelectionShow, (i32, i32))>,
    /// Copied plans pasted under the cursor (`DesktopInput.drawBottom`,
    /// `DesktopInput.java:175-185` `selectPlans`).
    pub select_plans: Vec<ClientPlan>,
}

/// Read-only [`PlacementWorld`] over the live sim grid/content.
///
/// `valid_place` is the real `Build.validPlace` for the player team; build
/// rotation and the `ChainedBuilding` walk read the live buildings. The host
/// is borrowed immutably (`HostWorld` never mutates the sim).
pub(crate) struct HostWorld<'a> {
    grid: &'a mind_core::world::WorldGrid,
    content: Option<&'a ContentRegistry>,
    host: &'a MindSimHost,
}

impl<'a> HostWorld<'a> {
    /// Builds the world view over a bound sim host.
    pub(crate) fn new(host: &'a MindSimHost) -> Self {
        Self {
            grid: host.grid(),
            content: host.content_registry(),
            host,
        }
    }

    /// Block kind at a tile (`None` out of bounds/unloaded).
    fn kind_at(&self, x: i32, y: i32) -> Option<mind_core::content::BlockKind> {
        let content = self.content?;
        let (x, y) = tile_i16(x, y)?;
        let block = self.grid.block_at(TilePos::new(x, y))?;
        content.block(block).map(|def| def.kind)
    }
}

impl PlacementWorld for HostWorld<'_> {
    fn in_bounds(&self, x: i32, y: i32) -> bool {
        self.grid.tiles.in_bounds(x, y)
    }

    fn block_at(&self, x: i32, y: i32) -> BlockId {
        let Some((x, y)) = tile_i16(x, y) else {
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

    fn block_size(&self, block: BlockId) -> i32 {
        // `BuildPlan.hitbox` / `Block.size` (`MobileInput.getPlan`,
        // `checkOverlapPlacement`, `InputHandler.normalizeRectangle`).
        self.content
            .and_then(|content| content.block(block))
            .map(|def| def.size)
            .unwrap_or(1)
    }

    fn block_offset(&self, block: BlockId) -> f32 {
        // `Block.offset` (`BuildPlan.drawx/drawy`, `4` for even footprints).
        self.content
            .and_then(|content| content.block(block))
            .map(|def| def.offset)
            .unwrap_or(0.0)
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

/// `HudFragment.canSkipWave` inputs (`HudFragment.java:1234-1236`):
/// `state.rules.waves && state.rules.waveSending && ((net.server() ||
/// player.admin) || !net.active()) && state.enemies == 0 &&
/// !spawner.isSpawning()`. The single-player client satisfies the net clause
/// (`!net.active()`), so only the wave/enemy/spawn half is left; the skip
/// button also requires a live core (`_refresh_skip_button`). `MindInput`
/// snapshots the values from `/root/MindHud` each frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaveGate {
    /// `state.rules.waves`.
    pub waves: bool,
    /// `state.rules.waveSending`.
    pub wave_sending: bool,
    /// `spawner.isSpawning()`.
    pub is_spawning: bool,
    /// `state.enemies`.
    pub enemies: i64,
    /// `state.teams.playerCores().size > 0` (`has_core`).
    pub has_core: bool,
}

impl WaveGate {
    /// `canSkipWave()` as the skip-wave button applies it
    /// (`_refresh_skip_button`, `ui/fragments/hud.rs`): waves on, wave sending
    /// on, not mid-spawn, no enemies left, a live core.
    pub fn can_skip_wave(self) -> bool {
        self.waves && self.wave_sending && !self.is_spawning && self.enemies <= 0 && self.has_core
    }
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
    /// Scene campaign facade (`content_unlocked` for the config spec).
    pub campaign: Option<Gd<MindCampaign>>,
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
    /// `keyDown(Binding.schematicSelect)` mirror for the F selection rect
    /// (`DesktopInput.java:140`). The draw pass has no binding table, so the
    /// flag is recomputed from the held key set on every raw key event.
    schematic_held: bool,
    /// `keyDown(Binding.rebuildSelect)` mirror (`DesktopInput.java:142`), same
    /// cadence as [`Self::schematic_held`].
    rebuild_held: bool,
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
    /// Per-block remembered config scroll offset (`Block.selectScroll`,
    /// `ItemSelection.java:103-108`), keyed by block content name.
    config_scroll: HashMap<String, f32>,
    /// `canSkipWave()` snapshot for the `skip_wave` binding
    /// (`HudFragment.java:461-467/1234-1236`), refreshed by `MindInput` from
    /// `/root/MindHud`.
    pub wave_gate: WaveGate,
    /// `splan` (`DesktopInput.java:43`): the queued plan under a select drag.
    splan: Option<PlanDrag>,
    /// `planConfig.selected` (`PlanConfigFragment.selected`): the plan whose
    /// point-config popup is open, waiting for `plan_config_applied`.
    plan_config_plan: Option<ClientPlan>,
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
            campaign: None,
            selectable: Vec::new(),
            last_config: HashMap::new(),
            config_selected: None,
            schem_start: None,
            schematic_held: false,
            rebuild_held: false,
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
            config_scroll: HashMap::new(),
            wave_gate: WaveGate::default(),
            splan: None,
            plan_config_plan: None,
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
        self.schematic_held = false;
        self.rebuild_held = false;
    }

    /// Mirrors `keyDown(schematicSelect/rebuildSelect)` into the two fields the
    /// draw pass reads (`DesktopInput.drawTop`, `DesktopInput.java:133-145`).
    /// Called on every raw key edge, including key-ups that arrive while a text
    /// field owns the keyboard (`handle` still removes them from `pressed`), so
    /// a focus change cannot leave a stale selection rectangle.
    fn refresh_selection_held(&mut self, bindings: &BindingState) {
        self.schematic_held = self.key_down(bindings, ids::SCHEMATIC_SELECT);
        self.rebuild_held = self.key_down(bindings, ids::REBUILD_SELECT);
    }

    /// Removes and returns the pending effects.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    /// Runs `f` with the scene sim host bound, or returns `None` when the
    /// scene has none (the same `self.host.clone()` + `bind()` the call sites
    /// wrote out). The `Gd` is cloned first, so `f` may still read the other
    /// bridge fields while the guard is alive.
    ///
    /// `f` must not call back into Godot (a `call("…")`, a signal or a UI node
    /// access) or re-bind the host: the guard must never be held across a
    /// re-entry.
    pub(super) fn with_host<R>(&self, f: impl FnOnce(&MindSimHost) -> R) -> Option<R> {
        let host = self.host.clone()?;
        let guard = host.bind();
        Some(f(&guard))
    }

    /// [`Self::with_host`] for a mutable sim host (`bind_mut`).
    pub(super) fn with_host_mut<R>(&self, f: impl FnOnce(&mut MindSimHost) -> R) -> Option<R> {
        let mut host = self.host.clone()?;
        let mut guard = host.bind_mut();
        Some(f(&mut guard))
    }
}
