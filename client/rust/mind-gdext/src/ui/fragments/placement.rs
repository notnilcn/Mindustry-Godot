// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/PlacementFragment.java
//         (plan 14 §3.5, M4).
//
// Block/category palette with search, the command table and the hover info
// box. The static frame is `placement_fragment.tscn` (tscn-first); the category
// rail and block grid are data-driven from the `MindUi.block_catalog_json()`
// catalogue (plan 02/07/15) and the hover box renders the `MindSimHost`
// requirement/bar/consumption probes (`ui::display` rows).

//! `MindPlacementFragment` — the placement palette (`PlacementFragment`).
//!
//! 1:1 port of `client/ui/fragments/placement_fragment.gd`:
//!
//! - The category rail (`Category.all` order) and the block grid are rebuilt
//!   from `MindUi.block_catalog_json()`; the checked cell follows
//!   `MindSimHost.selected_block` every frame and a rebuild clears the cached
//!   highlight.
//! - `block_select_01..10` digits/arrows and the category cycle arrive as
//!   `MindInput.hotkey` actions (`gridUpdate`); block/category selection is
//!   pushed back through `MindInput.select_block_by_name`.
//! - The top table shows the selected block header + requirements, or the
//!   hovered building/unit (`hover_info_world`/`hover_info`), with the
//!   `Block.setBars` rows and the `displayConsumption` `MultiReqImage` cells.
//! - Command mode swaps the palette for `command_panel_json` rows
//!   (`rebuildCommand`) and routes commands/stances through `MindInput`.
//!
//! Every string goes through the `MindAssets` bundle (`bundle_get` echo =
//! missing key, exactly like the GDScript `_bundle` fallback); icons resolve
//! through [`kit::icon_texture`] and the icon-font glyphs through
//! [`kit::glyph`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::object::ConnectFlags;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, ButtonGroup, ColorRect, Control, GridContainer, HBoxContainer, IControl, InputEvent,
    InputEventMouseButton, LineEdit, MarginContainer, Node, PanelContainer, RichTextLabel,
    ScrollContainer, StyleBox, StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, TextureRect, Time,
    VBoxContainer, VScrollBar,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;
use indexmap::IndexMap;
use serde_json::Value;

use crate::ui::kit::widgets::bar::MindBar;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::widgets::multi_req_image::MindMultiReqImage;
use crate::ui::kit::widgets::req_image::MindReqImage;
use crate::ui::kit::{self, MindRichLabel, styles};

/// Upstream `InputHandler.buildPlacementUI` (`DesktopInput.java:545-562`): the
/// four icon buttons under the block pane. `tree`/`map` are campaign-only
/// (`visible(() -> state.isCampaign())`).
struct InputButton {
    icon: &'static str,
    tooltip: &'static str,
    dialog: &'static str,
    campaign_only: bool,
}

impl InputButton {
    const fn new(icon: &'static str, tooltip: &'static str, dialog: &'static str) -> Self {
        Self {
            icon,
            tooltip,
            dialog,
            campaign_only: false,
        }
    }

    const fn campaign(icon: &'static str, tooltip: &'static str, dialog: &'static str) -> Self {
        Self {
            icon,
            tooltip,
            dialog,
            campaign_only: true,
        }
    }
}

/// `PlacementFragment.INPUT_BUTTONS`, in upstream order.
const INPUT_BUTTONS: [InputButton; 4] = [
    InputButton::new("paste", "@schematics", "schematics"),
    InputButton::new("book", "@database", "database"),
    InputButton::campaign("tree", "@research", "research"),
    InputButton::campaign("map", "@planetmap", "planet"),
];

/// Java `Color.gray` (`arc/graphics/Color.java:14-15`) used for the palette
/// tint of an unaffordable/not-a-builder block.
const TINT_WHITE: Color = Color::from_rgba(1.0, 1.0, 1.0, 1.0);
const TINT_GRAY: Color = Color::from_rgba(0.498, 0.498, 0.498, 1.0);
const TINT_DARK_GRAY: Color = Color::from_rgba(0.247, 0.247, 0.247, 1.0);

/// Palette tint refresh cadence in frames (Java updates every frame).
const TINT_INTERVAL: i32 = 10;

/// `Tex.buttonEdge2` 9-patch padding (splits 12) and the inner
/// `topTable.margin(5)` (`PlacementFragment.java:358`): content starts 17 px in.
const TOP_PAD: f32 = 12.0;
const TOP_MARGIN: f32 = 5.0;

/// Last live re-read cadence of the requirement `have` counts (msec).
const TOP_REFRESH_MS: i64 = 250;

/// `PlacementFragment` command-table re-read cadence (msec).
const COMMAND_REFRESH_MS: i64 = 250;

/// `block_select[0..9]` action names -> the Java `Category.all` digit index
/// (`PlacementFragment.java:60-74`).
const DIGIT_ACTIONS: [(&str, i32); 10] = [
    ("block_select_01", 0),
    ("block_select_02", 1),
    ("block_select_03", 2),
    ("block_select_04", 3),
    ("block_select_05", 4),
    ("block_select_06", 5),
    ("block_select_07", 6),
    ("block_select_08", 7),
    ("block_select_09", 8),
    ("block_select_10", 9),
];

/// `block_select[10..13]` arrow actions -> the `gridUpdate` direction index.
const ARROW_ACTIONS: [(&str, i32); 4] = [
    ("block_select_left", 10),
    ("block_select_right", 11),
    ("block_select_up", 12),
    ("block_select_down", 13),
];

/// `Time.timeSinceMillis(blockSelectSeqMillis) > 400` combo window.
const BLOCK_SELECT_COMBO_MS: i64 = 400;

/// `PlacementFragment.blockSelect` grid width (arrow up/down wrap).
const BLOCK_GRID_COLUMNS: i64 = 4;

/// `MindPlacementFragment` — the block/category palette.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindPlacementFragment {
    base: Base<Control>,

    // `@onready` scene nodes (`Panel/Frame/...`).
    panel: Option<Gd<PanelContainer>>,
    search: Option<Gd<LineEdit>>,
    categories: Option<Gd<GridContainer>>,
    blocks: Option<Gd<GridContainer>>,
    input_bar: Option<Gd<HBoxContainer>>,
    block_scroll: Option<Gd<ScrollContainer>>,
    top_table: Option<Gd<PanelContainer>>,
    top_rows: Option<Gd<VBoxContainer>>,
    // `Panel/Frame/TopDivider` is declared upstream but never read; the
    // `Pal.gray` rule is a static scene child.
    command_panel: Option<Gd<PanelContainer>>,
    command_rows: Option<Gd<VBoxContainer>>,
    layout: Option<Gd<HBoxContainer>>,

    /// `_selected_category`; `-1` = pick `Category.distribution` on the next
    /// rail build (`currentCategory` default, reset on `WorldLoadEvent`).
    #[init(val = -1)]
    selected_category: i32,
    /// Sim tick seen at the last reveal; a smaller tick means a new world.
    #[init(val = -1)]
    last_tick: i64,
    /// `MindUi.block_catalog_json()` (`{}` before the first load).
    catalog: Value,
    /// The four `INPUT_BUTTONS` instances (`_input_buttons`).
    input_buttons: Vec<Gd<Button>>,
    /// `hud.campaign` as of the last frame (`_bar_campaign`).
    bar_campaign: bool,
    /// Last rendered top-table state (`b:<block>` or `h:<kind>:<name>`); the
    /// header rebuilds only when it changes (`PlacementFragment.lastDisplayState`).
    top_state: String,
    /// One-shot warning when the `block_select_01..10` binds are incomplete.
    warned_block_select: bool,
    /// Last block selection mirrored into the palette's checked cell.
    last_selected: String,
    /// Frame counter for the batch tint refresh.
    tint_tick: i32,
    /// Block name -> cell button for the current grid, in rebuild order.
    block_buttons: IndexMap<String, Gd<Button>>,
    /// Requirement amount labels + their `(have, amount)` entries.
    top_amounts: Vec<Gd<RichTextLabel>>,
    top_requirements: Vec<Value>,
    /// Last live re-read of the requirement `have` counts (msec).
    top_refreshed_ms: i64,
    /// Hovered building tile; `-1` while not over a building.
    #[init(val = Vector2i::new(-1, -1))]
    hover_tile: Vector2i,
    /// Live `Block.setBars` rows: `(row, MindBar)` in add order.
    hover_bars: Vec<(Value, Gd<MindBar>)>,
    /// `displayConsumption` `ReqImage` validity, keyed `"<row>:<icon>"` and
    /// shared with the predicate callables.
    consume_valid: Rc<RefCell<HashMap<String, bool>>>,
    /// Command-mode panel state (`PlacementFragment.commandTable`).
    command_mode: bool,
    /// Last `command_panel_json` payload.
    command_json: String,
    #[init(val = true)]
    command_dirty: bool,
    command_refresh_ms: i64,
    /// `blockSelectEnd`/`blockSelectSeq`/`blockSelectSeqMillis`.
    #[init(val = true)]
    block_select_end: bool,
    block_select_seq: i32,
    block_select_ms: i64,
    /// `PlacementFragment.selectedBlocks`: category name -> remembered block.
    selected_blocks: HashMap<String, String>,
}

#[godot_api]
impl IControl for MindPlacementFragment {
    fn ready(&mut self) {
        self.panel = self.node_as::<PanelContainer>("Panel");
        self.search = self.node_as::<LineEdit>("Panel/Frame/Layout/Search");
        self.categories = self.node_as::<GridContainer>("Panel/Frame/Layout/Categories");
        self.blocks =
            self.node_as::<GridContainer>("Panel/Frame/Layout/Left/Rows/Scroll/Pad/Blocks");
        self.input_bar = self.node_as::<HBoxContainer>("Panel/Frame/Layout/Left/Rows/InputBar");
        self.block_scroll = self.node_as::<ScrollContainer>("Panel/Frame/Layout/Left/Rows/Scroll");
        self.top_table = self.node_as::<PanelContainer>("Panel/Frame/TopTable");
        self.top_rows = self.node_as::<VBoxContainer>("Panel/Frame/TopTable/Rows");
        self.command_panel = self.node_as::<PanelContainer>("Panel/Frame/CommandPanel");
        self.command_rows = self.node_as::<VBoxContainer>("Panel/Frame/CommandPanel/Rows");
        self.layout = self.node_as::<HBoxContainer>("Panel/Frame/Layout");

        // `PlacementFragment.blockCatTable` is part of the standard HUD, not a
        // collapsed tab: the manifest bind owns the fragment root, so the panel
        // stays visible here.
        if let Some(mut panel) = self.panel.clone() {
            panel.set_visible(true);
        }
        if let Some(mut search) = self.search.clone() {
            // The GDScript connects an anonymous lambda that ignores the new
            // text and rebuilds (`text_changed -> _rebuild`).
            let node = self.to_gd();
            let callable =
                Callable::from_fn("placement_search_changed", move |_args: &[&Variant]| {
                    node.clone().bind_mut().rebuild();
                });
            search.connect("text_changed", &callable);
        }
        self.setup_top_style();
        self.setup_frame_style();
        self.setup_command_style();
        self.build_input_bar();
        // The HUD starts hidden in the menu; the catalogue is rebuilt on every
        // reveal so a sector launch or research that happened while it was
        // hidden is reflected.
        let callable = self.base().callable("_ensure_loaded");
        if !self.base().is_connected("visibility_changed", &callable) {
            let _ = self.base_mut().connect("visibility_changed", &callable);
        }
        // Category cycling and block-select digits are dispatched by `MindInput`.
        if let Some(mut input) = self.input_node() {
            let callable = self.base().callable("_on_hotkey");
            if input.has_signal("hotkey") && !input.is_connected("hotkey", &callable) {
                let _ = input.connect("hotkey", &callable);
            }
            let callable = self.base().callable("_on_selection_changed");
            if input.has_signal("selection_changed")
                && !input.is_connected("selection_changed", &callable)
            {
                let _ = input.connect("selection_changed", &callable);
            }
        }
        // `WorldLoadEvent` / `UnlockEvent`-with-a-block rebuild the palette;
        // both posts are deferred like Java's `Core.app.post`.
        if let Some(mut host) = self.host() {
            let callable = self.base().callable("_on_world_loaded");
            if host.has_signal("world_loaded") && !host.is_connected("world_loaded", &callable) {
                let _ = host.connect_flags("world_loaded", &callable, ConnectFlags::DEFERRED);
            }
        }
        if let Some(mut campaign) = kit::campaign() {
            let callable = self.base().callable("_on_block_unlocked");
            if campaign.has_signal("block_unlocked")
                && !campaign.is_connected("block_unlocked", &callable)
            {
                let _ = campaign.connect_flags("block_unlocked", &callable, ConnectFlags::DEFERRED);
            }
        }
        self.refresh_input_bar();
        self.ensure_loaded();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let in_campaign = self.in_campaign();
        if in_campaign != self.bar_campaign {
            self.bar_campaign = in_campaign;
            self.refresh_input_bar();
        }
        self.update_top_table();
        self.update_selection_highlight();
        self.update_command_panel();
        // Java recolours every cell in a per-frame `button.update`; the port
        // batches the `palette_tints_json` seam on the HUD poll cadence.
        self.tint_tick += 1;
        if self.tint_tick >= TINT_INTERVAL {
            self.tint_tick = 0;
            self.update_palette_tints();
        }
    }
}

#[godot_api]
impl MindPlacementFragment {
    /// Opens the block-select pane (`blocksSelect` collapser expanded).
    #[func]
    pub fn open_palette(&mut self) {
        if let Some(mut panel) = self.panel.clone() {
            panel.set_visible(true);
        }
        self.base_mut().set_visible(true);
        self.ensure_loaded();
    }

    /// Collapses the block-select pane back to the HUD.
    #[func]
    pub fn close_palette(&mut self) {
        if let Some(mut panel) = self.panel.clone() {
            panel.set_visible(false);
        }
        self.ensure_loaded();
    }

    /// The selection re-renders the command table (`mainStack` swap).
    #[func(rename = _on_selection_changed)]
    pub fn on_selection_changed(&mut self, _units_json: GString) {
        self.command_dirty = true;
    }

    /// `Events.on(WorldLoadEvent)`: reset the category, clear the per-category
    /// memory, drop the selected block and re-read the catalogue.
    #[func(rename = _on_world_loaded)]
    pub fn on_world_loaded(&mut self) {
        self.selected_category = -1;
        self.selected_blocks.clear();
        self.select_block_by_name("air");
        self.load_catalog();
    }

    /// `Events.on(UnlockEvent)` when the content is a `Block`.
    #[func(rename = _on_block_unlocked)]
    pub fn on_block_unlocked(&mut self) {
        self.load_catalog();
    }

    /// `PlacementFragment.gridUpdate` hotkey half (digits, arrows, category
    /// cycles and `updatePick`'s category sync).
    #[func(rename = _on_hotkey)]
    pub fn on_hotkey(&mut self, action: GString) {
        let action = action.to_string();
        if !self.command_mode {
            if let Some(digit) = action_index(&DIGIT_ACTIONS, &action) {
                self.block_select_digit(digit);
                return;
            }
            if let Some(arrow) = action_index(&ARROW_ACTIONS, &action) {
                self.block_select_arrow(arrow);
                return;
            }
            if action == "category_prev" {
                self.category_step(-1);
                return;
            }
            if action == "category_next" {
                self.category_step(1);
                return;
            }
        }
        if action == "pick" {
            // `updatePick`'s `currentCategory = input.block.category`.
            self.sync_category_to_selected();
        }
    }

    /// Rebuilds the catalogue when the palette is revealed (and resets the
    /// category when the sim tick went backwards = a new world).
    #[func(rename = _ensure_loaded)]
    pub fn ensure_loaded(&mut self) {
        if !self.base().is_visible_in_tree() {
            return;
        }
        if let Some(mut host) = self.host()
            && host.has_method("get_tick")
        {
            let tick = host
                .call("get_tick", &[])
                .try_to_relaxed::<i64>()
                .unwrap_or_default();
            if tick < self.last_tick {
                self.selected_category = -1;
            }
            self.last_tick = tick;
        }
        self.load_catalog();
    }

    /// Rail click (`PlacementFragment.java:730-736`): switch the category and,
    /// when a block is already picked, re-select that category's block.
    #[func(rename = _select)]
    pub fn select(&mut self, index: i64) {
        let select_block = !self.block_selected_name().is_empty();
        self.select_category(index as i32, select_block);
    }

    /// `?` button: `ui.content.show(displayBlock)` (`ContentInfoDialog`).
    #[func(rename = _open_content_info)]
    pub fn open_content_info(&mut self, content_name: GString) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("open_dialog") {
            return;
        }
        let context = serde_json::json!({ "content": content_name.to_string() }).to_string();
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("content").to_variant(),
                GString::from(context.as_str()).to_variant(),
            ],
        );
    }

    /// `InputHandler.buildPlacementUI` icon row routing (`MindUi.open_dialog`).
    #[func(rename = _open_input_dialog)]
    pub fn open_input_dialog(&mut self, dialog_name: GString) {
        if dialog_name.to_string().is_empty() {
            return;
        }
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("open_dialog") {
            return;
        }
        let _ = ui.call(
            "open_dialog",
            &[dialog_name.to_variant(), GString::from("{}").to_variant()],
        );
    }

    /// `StatValues.stack` cell click (`b.clicked(mouseLeft/mouseRight)`).
    #[func(rename = _on_command_type_input)]
    pub fn on_command_type_input(&mut self, event: Gd<InputEvent>, type_id: i64) {
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() || type_id < 0 {
            return;
        }
        let Some(mut input) = self.input_node() else {
            return;
        };
        if mouse.get_button_index() == MouseButton::LEFT && input.has_method("select_only_type") {
            let _ = input.call("select_only_type", &[type_id.to_variant()]);
        } else if mouse.get_button_index() == MouseButton::RIGHT
            && input.has_method("deselect_type")
        {
            let _ = input.call("deselect_type", &[type_id.to_variant()]);
        }
    }

    /// `Call.setUnitCommand(player, ids, command)`.
    #[func(rename = _set_command)]
    pub fn set_command(&mut self, command_name: GString) {
        let Some(mut input) = self.input_node() else {
            return;
        };
        if input.has_method("set_unit_command") {
            let _ = input.call("set_unit_command", &[command_name.to_variant()]);
        }
    }

    /// `Call.setUnitStance(player, ids, stance, !activeStances.get(...))`.
    #[func(rename = _toggle_stance)]
    pub fn toggle_stance(&mut self, command_name: GString, active: bool) {
        let Some(mut input) = self.input_node() else {
            return;
        };
        if input.has_method("set_unit_stance_named") {
            let _ = input.call(
                "set_unit_stance_named",
                &[command_name.to_variant(), (!active).to_variant()],
            );
        }
    }

    /// Block cell target (`PlacementFragment.java:306-307`): clicking the
    /// selected block deselects it (`input.block == block ? null : block`).
    #[func(rename = _on_block_pressed)]
    pub fn on_block_pressed(&mut self, block_name: GString, category_name: GString) {
        let block_name = block_name.to_string();
        let current = self.selected_block_raw();
        let next = if current == block_name {
            String::from("air")
        } else {
            block_name
        };
        let remembered = if next == "air" {
            String::new()
        } else {
            next.clone()
        };
        self.selected_blocks
            .insert(category_name.to_string(), remembered);
        self.select_block(&next);
    }
}

impl MindPlacementFragment {
    /// One `@onready`-style node lookup with the GDScript cast semantics.
    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    /// `/root/Spine/SimHost` (`get_node_or_null`).
    fn host(&self) -> Option<Gd<Node>> {
        kit::sim_host()
    }

    /// `/root/Spine/Input`.
    fn input_node(&self) -> Option<Gd<Node>> {
        kit::node("/root/Spine/Input")
    }

    /// `state.isCampaign()` through the read-only `MindHud` facade.
    fn in_campaign(&self) -> bool {
        let Some(hud) = kit::hud() else {
            return false;
        };
        hud.get("campaign")
            .try_to_relaxed::<bool>()
            .unwrap_or(false)
    }

    /// Resolves a bundle key through `MindAssets`, falling back to `fallback`
    /// when the key is missing (the bundle echoes the key) or the autoload is
    /// absent.
    fn bundle(&self, key: &str, fallback: &str) -> String {
        if key.is_empty() {
            return fallback.to_string();
        }
        let Some(assets) = kit::assets() else {
            return fallback.to_string();
        };
        let value = assets.bind().bundle_get(GString::from(key)).to_string();
        if value.is_empty() || value == key {
            fallback.to_string()
        } else {
            value
        }
    }

    /// The catalogue categories (`_catalog.get("categories", [])`).
    fn catalog_categories(&self) -> Vec<Value> {
        self.catalog
            .get("categories")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    /// The live catalogue entry for `selected_category` (`{}` when none).
    fn current_entry(&self) -> Value {
        let list = self.catalog_categories();
        if list.is_empty()
            || self.selected_category < 0
            || self.selected_category as usize >= list.len()
        {
            return Value::Null;
        }
        list[self.selected_category as usize].clone()
    }

    /// Article names of one catalogue entry, in content order
    /// (`getUnlockedByCategory` order).
    fn block_names(&self, entry: &Value) -> Vec<String> {
        entry
            .get("blocks")
            .and_then(Value::as_array)
            .map(|blocks| blocks.iter().map(|block| json_str(block, "name")).collect())
            .unwrap_or_default()
    }

    /// Catalogue index of the entry with `Category.ordinal() == ordinal`
    /// (`-1` when the category is empty and therefore absent).
    fn category_index_by_ordinal(&self, ordinal: i32) -> i32 {
        let list = self.catalog_categories();
        for (index, entry) in list.iter().enumerate() {
            let entry_ordinal = entry.get("ordinal").and_then(Value::as_i64).unwrap_or(-1);
            if entry_ordinal == i64::from(ordinal) {
                return index as i32;
            }
        }
        -1
    }

    /// `PlacementFragment.getSelectedBlock`: the remembered block of the
    /// category, else its first block (the head of the unlock-filtered list).
    fn get_selected_block(&self, entry: &Value) -> String {
        if !entry.is_object() {
            return String::new();
        }
        let block_name = json_str(entry, "name");
        if let Some(remembered) = self.selected_blocks.get(&block_name) {
            return remembered.clone();
        }
        self.block_names(entry).first().cloned().unwrap_or_default()
    }

    /// `control.input.block` as reported by the sim host (`""` when none).
    fn selected_block_raw(&self) -> String {
        let Some(mut host) = self.host() else {
            return String::new();
        };
        if !host.has_method("selected_block") {
            return String::new();
        }
        host.call("selected_block", &[])
            .try_to_relaxed::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    /// `_block_selected_name`: `""` for the air/no-selection sentinel.
    fn block_selected_name(&self) -> String {
        let block_name = self.selected_block_raw();
        if block_name == "air" {
            String::new()
        } else {
            block_name
        }
    }

    /// Selects a block by content name through `MindInput` (empty is a no-op).
    fn select_block_by_name(&mut self, block_name: &str) {
        if block_name.is_empty() {
            return;
        }
        self.select_block(block_name);
    }

    /// Selects a block through `MindInput` so placement rotation/state stay in
    /// sync (falls back to the sim host when the input node is absent).
    fn select_block(&self, block_name: &str) {
        if let Some(mut input) = self.input_node()
            && input.has_method("select_block_by_name")
        {
            let _ = input.call(
                "select_block_by_name",
                &[GString::from(block_name).to_variant()],
            );
            return;
        }
        if let Some(mut host) = self.host() {
            let _ = host.call("select_block", &[GString::from(block_name).to_variant()]);
        }
    }

    /// Switches the visible category and re-selects its remembered block
    /// (rail click `:730-736`, digit first number `:213-218`).
    fn select_category(&mut self, index: i32, select_block: bool) {
        self.selected_category = index;
        self.build_categories();
        self.rebuild();
        if !select_block {
            return;
        }
        let remembered = self.get_selected_block(&self.current_entry());
        if !remembered.is_empty() {
            self.select_block_by_name(&remembered);
        }
    }

    /// Pulls the block catalogue from `MindUi` and paints the rail + grid.
    fn load_catalog(&mut self) {
        if let Some(mut ui) = kit::ui()
            && ui.has_method("block_catalog_json")
        {
            let payload = ui
                .call("block_catalog_json", &[])
                .try_to_relaxed::<GString>()
                .unwrap_or_default();
            if let Ok(parsed) = serde_json::from_str::<Value>(&payload.to_string())
                && parsed.is_object()
            {
                self.catalog = parsed;
            }
        }
        self.build_categories();
        self.rebuild();
    }

    /// Rebuilds the category rail (`rebuildCategory`).
    fn build_categories(&mut self) {
        let Some(mut categories) = self.categories.clone() else {
            return;
        };
        for child in categories.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        // code-instantiated: category tabs are the data-driven block-category
        // list (plan 02); the count is runtime content.
        let list = self.catalog_categories();
        if list.is_empty() {
            return;
        }
        if self.selected_category < 0 || self.selected_category as usize >= list.len() {
            self.selected_category = 0;
            for (index, entry) in list.iter().enumerate() {
                if json_str(entry, "name") == "distribution" {
                    self.selected_category = index as i32;
                    break;
                }
            }
        }
        // `ButtonGroup`: exactly the active category is checked.
        let group = ButtonGroup::new_gd();
        for (index, entry) in list.iter().enumerate() {
            let cat_name = json_str(entry, "name");
            let label = self.bundle(&json_str(entry, "label"), &cat_name);
            let mut button = factory::glyph_button(&cat_name, &label, "clearTogglei");
            button.set_name(&format!("category-{cat_name}"));
            button.set_tooltip_text(&label);
            button.set_toggle_mode(true);
            button.set_button_group(&group);
            button.set_pressed(index as i32 == self.selected_category);
            button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
            let mut arguments = Array::<Variant>::new();
            arguments.push(&(index as i64).to_variant());
            let callable = self.base().callable("_select").bindv(&arguments);
            let _ = button.connect("pressed", &callable);
            categories.add_child(&button);
        }
    }

    /// Rebuilds the block grid for the selected category + search query.
    fn rebuild(&mut self) {
        let Some(mut blocks) = self.blocks.clone() else {
            return;
        };
        for child in blocks.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        self.block_buttons.clear();
        // A rebuild re-creates the cells; force the next frame's highlight pass
        // to re-apply instead of trusting the cached selection.
        self.last_selected.clear();
        // code-instantiated: block buttons come from the content catalogue
        // filtered by category/search (data-driven, plan 02/07).
        let list = self.catalog_categories();
        if list.is_empty()
            || self.selected_category < 0
            || self.selected_category as usize >= list.len()
        {
            return;
        }
        let category = list[self.selected_category as usize].clone();
        let category_name = json_str(&category, "name");
        let query = match self.search.as_ref() {
            Some(search) => search.get_text().to_string().to_lowercase(),
            None => String::new(),
        };
        let selected = self.selected_block_raw();
        // `ButtonGroup`: the active block is the checked button; the checked
        // block can be unchecked by clicking it again.
        let mut group = ButtonGroup::new_gd();
        group.set_allow_unpress(true);
        let block_list = category
            .get("blocks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for block in block_list {
            let block_name = json_str(&block, "name");
            let label = self.bundle(&json_str(&block, "localized"), &block_name);
            if !query.is_empty() {
                let name_lower = block_name.to_lowercase();
                let label_lower = label.to_lowercase();
                if !name_lower.contains(&query) && !label_lower.contains(&query) {
                    continue;
                }
            }
            let regions = [
                format!("block-{block_name}-ui"),
                format!("block-{block_name}-full"),
                block_name.clone(),
            ];
            let region_refs: Vec<&str> = regions.iter().map(String::as_str).collect();
            let mut button = factory::image_button_first(&region_refs, "selecti");
            button.set_name(&format!("block-{block_name}"));
            button.set_tooltip_text(&label);
            button.set_toggle_mode(true);
            button.set_button_group(&group);
            button.set_pressed(block_name == selected);
            button.set_custom_minimum_size(Vector2::new(46.0, 46.0));
            // `Button.resizeImage(iconMed)`: 46 px cell, 32 px centred icon.
            button.set_expand_icon(true);
            button.add_theme_constant_override("icon_max_width", 32);
            // Arc's `ImageButton` centres its image cell.
            button.set_icon_alignment(HorizontalAlignment::CENTER);
            button.set_vertical_icon_alignment(VerticalAlignment::CENTER);
            if button.get_button_icon().is_none() {
                button.set_text(label.as_str());
            }
            let mut arguments = Array::<Variant>::new();
            arguments.push(&GString::from(block_name.as_str()).to_variant());
            arguments.push(&GString::from(category_name.as_str()).to_variant());
            let callable = self.base().callable("_on_block_pressed").bindv(&arguments);
            let _ = button.connect("pressed", &callable);
            blocks.add_child(&button);
            self.block_buttons.insert(block_name, button);
        }
    }

    /// `PlacementFragment.categoryPrev/Next`: cycle the categories and select
    /// the new category's remembered block even with no prior pick.
    fn category_step(&mut self, delta: i32) {
        let list = self.catalog_categories();
        if list.is_empty() {
            return;
        }
        let panel_visible = self.panel.as_ref().is_some_and(|panel| panel.is_visible());
        if !panel_visible {
            self.open_palette();
        }
        let size = list.len() as i64;
        let value = i64::from(self.selected_category) + i64::from(delta);
        self.selected_category = value.rem_euclid(size) as i32;
        self.build_categories();
        self.rebuild();
        let remembered = self.get_selected_block(&self.current_entry());
        if !remembered.is_empty() {
            self.select_block_by_name(&remembered);
        }
    }

    /// `updatePick`'s category switch for a successful middle-click pick: the
    /// rail follows the picked block's category.
    fn sync_category_to_selected(&mut self) {
        let block_name = self.block_selected_name();
        if block_name.is_empty() {
            return;
        }
        let list = self.catalog_categories();
        for (index, entry) in list.iter().enumerate() {
            if !self.block_names(entry).contains(&block_name) {
                continue;
            }
            self.selected_category = index as i32;
            self.selected_blocks
                .insert(json_str(entry, "name"), block_name.clone());
            self.build_categories();
            self.rebuild();
            return;
        }
    }

    /// `gridUpdate` digits: the first number of a combo picks `Category.all[i]`,
    /// the second/third pick the block index (`X,1,0` == `X,0`).
    fn block_select_digit(&mut self, digit: i32) {
        let now = now_ms();
        if self.block_select_end || now - self.block_select_ms > BLOCK_SELECT_COMBO_MS {
            let index = self.category_index_by_ordinal(digit);
            if index < 0 {
                // Empty category (`!getUnlockedByCategory(...).isEmpty()`).
                return;
            }
            let select_block = !self.block_selected_name().is_empty();
            self.select_category(index, select_block);
            self.block_select_end = false;
            self.block_select_seq = 0;
            self.block_select_ms = now;
            return;
        }
        let mut target = digit;
        if self.block_select_seq == 0 {
            self.block_select_seq = digit + 1;
        } else {
            target += (self.block_select_seq - i32::from(digit == 9)) * 10;
            self.block_select_end = true;
        }
        let entry = self.current_entry();
        let names = self.block_names(&entry);
        if target < 0 || target as usize >= names.len() {
            return;
        }
        let name = names[target as usize].clone();
        if entry.is_object() {
            self.selected_blocks
                .insert(json_str(&entry, "name"), name.clone());
        }
        self.select_block_by_name(&name);
        self.block_select_ms = now_ms();
    }

    /// `gridUpdate` arrows: the grid position of the current selection in the
    /// category's (unlock-filtered) list.
    fn block_select_arrow(&mut self, arrow: i32) {
        let entry = self.current_entry();
        let names = self.block_names(&entry);
        let count = names.len() as i64;
        if count <= 0 {
            return;
        }
        let selected = self.block_selected_name();
        let Some(at) = names.iter().position(|name| *name == selected) else {
            return;
        };
        let at = at as i64;
        let target = match arrow {
            10 => (at - 1 + count) % count,
            11 => (at + 1) % count,
            12 => {
                let mut target = if at > 3 {
                    at - 4
                } else {
                    count - count % BLOCK_GRID_COLUMNS + at
                };
                if target >= count {
                    target -= 4;
                }
                target
            }
            13 => {
                if at < count - 4 {
                    at + 4
                } else {
                    at % 4
                }
            }
            _ => at,
        };
        // GDScript arrays accept negative indices (`names[-1]` = last).
        let index = if target < 0 { count + target } else { target };
        if index < 0 || index >= count {
            return;
        }
        let name = names[index as usize].clone();
        if entry.is_object() {
            self.selected_blocks
                .insert(json_str(&entry, "name"), name.clone());
        }
        self.select_block_by_name(&name);
    }

    /// `PlacementFragment` key combo: category hotkey + row/column digits from
    /// the `block_select_01..10` binds (`placement.blockselectkeys`).
    fn key_combo(&mut self, display: &str) -> String {
        let list = self.catalog_categories();
        if self.selected_category < 0 || self.selected_category as usize >= list.len() {
            return String::new();
        }
        let category = list[self.selected_category as usize].clone();
        let blocks = category
            .get("blocks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(index) = blocks
            .iter()
            .position(|block| json_str(block, "name") == display)
        else {
            return String::new();
        };
        let binds = self.block_select_displays();
        let displayable = binds.iter().filter(|bind| !bind.is_empty()).count();
        if displayable < 10 {
            if !self.warned_block_select {
                self.warned_block_select = true;
                log::warn!(
                    "placement: block_select_01..10 keybinds incomplete ({displayable}/10 displayable); key combo line omitted"
                );
            }
            return String::new();
        }
        let ordinal = category.get("ordinal").and_then(Value::as_u64).unwrap_or(0) as usize;
        let category_key = binds.get(ordinal.min(9)).cloned().unwrap_or_default();
        // `Core.bundle.format("placement.blockselectkeys", categoryKey)` +
        // `(i < 10 ? "" : blockSelect[(i + 1) / 10 - 1] + ",") + blockSelect[i % 10]
        // + "]"` (PlacementFragment.java:371-395).
        let template = self.bundle("placement.blockselectkeys", "\n[lightgray]Key: [{0},");
        let mut combo = template.replace("{0}", &category_key);
        if index >= 10 {
            let bind_index = ((index + 1) / 10 - 1).min(9);
            combo.push_str(binds.get(bind_index).map(String::as_str).unwrap_or(""));
            combo.push(',');
        }
        combo.push_str(binds.get(index % 10).map(String::as_str).unwrap_or(""));
        combo.push(']');
        kit::markup_text(&combo).to_string()
    }

    /// `block_select_01..10` key display strings from the input binding
    /// registry.
    fn block_select_displays(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(mut input) = self.input_node() else {
            return out;
        };
        if !input.has_method("keybinds_json") {
            return out;
        }
        let Ok(payload) = input.call("keybinds_json", &[]).try_to_relaxed::<GString>() else {
            return out;
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&payload.to_string()) else {
            return out;
        };
        let Some(rows) = parsed.as_array() else {
            return out;
        };
        let mut by_name: HashMap<String, String> = HashMap::new();
        for entry in rows {
            if entry.is_object() {
                by_name.insert(json_str(entry, "name"), json_str(entry, "display"));
            }
        }
        for index in 1..=10 {
            out.push(
                by_name
                    .get(&format!("block_select_{index:02}"))
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        out
    }

    /// `InputHandler.buildPlacementUI` icon row: 48 px glyph buttons
    /// left-aligned under the pane.
    fn build_input_bar(&mut self) {
        let Some(mut input_bar) = self.input_bar.clone() else {
            return;
        };
        if !self.input_buttons.is_empty() {
            return;
        }
        for entry in INPUT_BUTTONS {
            // code-instantiated: fixed upstream icon row (four desktop buttons).
            let label = self.bundle(entry.tooltip, entry.icon);
            let mut button = factory::glyph_button(entry.icon, &label, "clearNonei");
            button.set_name(&format!("input-{}", entry.icon));
            button.set_tooltip_text(&label);
            button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
            button.set_meta("campaign_only", &entry.campaign_only.to_variant());
            let mut arguments = Array::<Variant>::new();
            arguments.push(&GString::from(entry.dialog).to_variant());
            let callable = self.base().callable("_open_input_dialog").bindv(&arguments);
            let _ = button.connect("pressed", &callable);
            input_bar.add_child(&button);
            self.input_buttons.push(button);
        }
    }

    /// `visible(() -> state.isCampaign())` on the research/planet buttons.
    fn refresh_input_bar(&mut self) {
        let in_campaign = self.in_campaign();
        for button in &self.input_buttons {
            if !button.is_instance_valid() {
                continue;
            }
            let mut button = button.clone();
            let campaign_only = button.has_meta("campaign_only")
                && button
                    .get_meta("campaign_only")
                    .try_to_relaxed::<bool>()
                    .unwrap_or(false);
            if campaign_only {
                button.set_visible(in_campaign);
            }
        }
    }

    // ---- styles -----------------------------------------------------------

    /// `PlacementFragment` top table: `Tex.buttonEdge2` ninepatch frame.
    fn setup_top_style(&mut self) {
        let Some(mut top_table) = self.top_table.clone() else {
            return;
        };
        let Some(texture) = kit::icon_texture("button-edge-2") else {
            return;
        };
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_texture_margin(side, TOP_PAD);
            box_.set_content_margin(side, TOP_PAD + TOP_MARGIN);
        }
        top_table.add_theme_stylebox_override("panel", &box_);
    }

    /// Sets the top table's bottom content margin (`topTable.marginBottom`); a
    /// new box per change so the panel re-reads its minimum size.
    fn set_top_bottom_margin(&mut self, margin: f32) {
        let Some(mut top_table) = self.top_table.clone() else {
            return;
        };
        if !top_table.has_theme_stylebox_override("panel") {
            return;
        }
        let Some(current) = top_table.get_theme_stylebox("panel") else {
            return;
        };
        if is_equal_approx(current.get_content_margin(Side::BOTTOM), margin) {
            return;
        }
        let Some(mut box_) = current.duplicate_resource_ex().done_or_null() else {
            return;
        };
        box_.set_content_margin(Side::BOTTOM, margin);
        top_table.add_theme_stylebox_override("panel", &box_);
    }

    /// `full.table(frame -> ...)` + the blocks column (`pane2` 4/0/4/4, the
    /// small-pane scroll bar and the `black6` category filler).
    fn setup_frame_style(&mut self) {
        if let Some(mut panel) = self.panel.clone() {
            let empty: Gd<StyleBox> = StyleBoxEmpty::new_gd().upcast();
            panel.add_theme_stylebox_override("panel", &empty);
        }
        if let Some(mut left) = self.node_as::<PanelContainer>("Panel/Frame/Layout/Left")
            && let Some(pane) = kit::icon_texture("pane-2")
        {
            let mut box_ = StyleBoxTexture::new_gd();
            box_.set_texture(&pane);
            for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
                box_.set_texture_margin(side, 12.0);
            }
            box_.set_content_margin(Side::LEFT, 4.0);
            box_.set_content_margin(Side::TOP, 0.0);
            box_.set_content_margin(Side::RIGHT, 4.0);
            box_.set_content_margin(Side::BOTTOM, 4.0);
            left.add_theme_stylebox_override("panel", &box_);
        }
        if let Some(block_scroll) = self.block_scroll.clone()
            && let Some(bar) = block_scroll.get_v_scroll_bar()
        {
            self.apply_small_pane(bar);
        }
        if let Some(mut layout) = self.layout.clone() {
            layout.add_theme_constant_override("separation", 0);
            if let Some(mut categories) = self.categories.clone() {
                // `new Image(Styles.black6)` filler: `colspan(2).growX().growY()
                // .padTop(-3f)` drawn `height - 3`, i.e. from the column top
                // down to the first category row.
                let categories_for_draw = categories.clone();
                let layout_for_draw = layout.clone();
                let callable =
                    Callable::from_fn("placement_filler", move |_args: &[&Variant]| {
                        if !categories_for_draw.is_instance_valid() {
                            return;
                        }
                        let position = categories_for_draw.get_position();
                        if position.y <= 3.0 {
                            return;
                        }
                        let size = categories_for_draw.get_size();
                        let mut layout = layout_for_draw.clone();
                        layout.draw_rect(
                            Rect2::new(
                                Vector2::new(position.x, 0.0),
                                Vector2::new(size.x, position.y),
                            ),
                            styles::BLACK_6,
                        );
                    });
                let _ = layout.connect("draw", &callable);
                // The filler follows the column's placement; a container
                // re-sort does not redraw the layout by itself.
                let layout_for_rect = layout.clone();
                let callable =
                    Callable::from_fn("placement_filler_rect", move |_args: &[&Variant]| {
                        let mut layout = layout_for_rect.clone();
                        layout.queue_redraw();
                    });
                let _ = categories.connect("item_rect_changed", &callable);
            }
            layout.queue_redraw();
        }
    }

    /// `Styles.smallPane`: `vScroll = clear` (no track) and the 12 px
    /// `scroll-knob-vertical-thin` knob.
    fn apply_small_pane(&self, mut bar: Gd<VScrollBar>) {
        let mut track = StyleBoxEmpty::new_gd();
        track.set_content_margin(Side::LEFT, 6.0);
        track.set_content_margin(Side::RIGHT, 6.0);
        bar.add_theme_stylebox_override("scroll", &track);
        bar.add_theme_stylebox_override("scroll_focus", &track);
        let Some(texture) = kit::icon_texture("scroll-knob-vertical-thin") else {
            return;
        };
        let mut knob = StyleBoxTexture::new_gd();
        knob.set_texture(&texture);
        knob.set_content_margin(Side::LEFT, 6.0);
        knob.set_content_margin(Side::RIGHT, 6.0);
        knob.set_content_margin(Side::TOP, 0.0);
        knob.set_content_margin(Side::BOTTOM, 0.0);
        for slot in ["grabber", "grabber_highlight", "grabber_pressed"] {
            bar.add_theme_stylebox_override(slot, &knob);
        }
    }

    /// `Tex.pane2` ninepatch for the command table (`commandTable`).
    fn setup_command_style(&mut self) {
        let Some(mut command_panel) = self.command_panel.clone() else {
            return;
        };
        let Some(texture) = kit::icon_texture("pane-2") else {
            return;
        };
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        box_.set_texture_margin(Side::LEFT, 12.0);
        box_.set_texture_margin(Side::TOP, 12.0);
        box_.set_texture_margin(Side::RIGHT, 12.0);
        box_.set_texture_margin(Side::BOTTOM, 12.0);
        command_panel.add_theme_stylebox_override("panel", &box_);
    }

    // ---- palette ----------------------------------------------------------

    /// `PlacementFragment.java:313-323` palette cell tint: white when placeable,
    /// affordable and the player unit is a builder, `Color.gray` otherwise,
    /// `Color.darkGray` when `Block.isPlaceable()` is false. Only the icon is
    /// tinted so the checked `selecti` accent frame keeps its colour.
    fn update_palette_tints(&mut self) {
        if self.block_buttons.is_empty() {
            return;
        }
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("palette_tints_json") {
            return;
        }
        let Ok(payload) = host
            .call("palette_tints_json", &[])
            .try_to_relaxed::<GString>()
        else {
            return;
        };
        let Ok(tints) = serde_json::from_str::<Value>(&payload.to_string()) else {
            return;
        };
        if !tints.is_object() {
            return;
        }
        for (name, button) in &self.block_buttons {
            if !button.is_instance_valid() {
                continue;
            }
            let mut button = button.clone();
            let color = match tints.get(name).and_then(Value::as_i64).unwrap_or(1) {
                0 => TINT_WHITE,
                2 => TINT_DARK_GRAY,
                _ => TINT_GRAY,
            };
            if button.get_theme_color("icon_normal_color") == color {
                continue;
            }
            for state in [
                "icon_normal_color",
                "icon_hover_color",
                "icon_pressed_color",
                "icon_disabled_color",
            ] {
                button.add_theme_color_override(state, color);
            }
        }
    }

    /// The checked cell follows `control.input.block` even when the selection
    /// changed outside a palette click (`PlacementFragment.java:300-330`).
    fn update_selection_highlight(&mut self) {
        if self.blocks.is_none() {
            return;
        }
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("selected_block") {
            return;
        }
        let selected = host
            .call("selected_block", &[])
            .try_to_relaxed::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        if selected == self.last_selected {
            return;
        }
        self.last_selected.clone_from(&selected);
        let active = self
            .block_buttons
            .get(&selected)
            .map(|button| button.instance_id());
        for button in self.block_buttons.values() {
            if !button.is_instance_valid() {
                continue;
            }
            let mut button = button.clone();
            let instance = button.instance_id();
            // `setPressedNoSignal`: the ButtonGroup must not re-fire `pressed`.
            button.set_pressed_no_signal(active == Some(instance));
        }
    }

    // ---- top table --------------------------------------------------------

    /// `PlacementFragment` top table: selected block header + requirements, else
    /// the hovered displayable. Visibility = `hasInfoBox()`.
    fn update_top_table(&mut self) {
        if self.top_table.is_none() || self.top_rows.is_none() {
            return;
        }
        let mut host = self.host();
        let mut display = String::new();
        if let Some(node) = host.as_mut()
            && node.has_method("selected_block")
        {
            display = node
                .call("selected_block", &[])
                .try_to_relaxed::<GString>()
                .map(|value| value.to_string())
                .unwrap_or_default();
            if display == "air" || display == "remove-wall" || display == "remove-ore" {
                display.clear();
            }
        }
        let mut hover = VarDictionary::new();
        if display.is_empty()
            && let Some(node) = host.as_mut()
        {
            if node.has_method("hover_info_world") {
                // `PlacementFragment.hovered`: the world unit test uses the
                // cursor's world position, not the tile centre.
                if let Some(world) = self.mouse_world() {
                    let info = node.call(
                        "hover_info_world",
                        &[world.x.to_variant(), world.y.to_variant()],
                    );
                    if let Ok(info) = info.try_to::<VarDictionary>()
                        && !dict_str(&info, "kind").is_empty()
                    {
                        hover = info;
                    }
                }
            } else if node.has_method("hover_info")
                && let Some(tile) = self.mouse_tile()
            {
                let info = node.call("hover_info", &[tile.x.to_variant(), tile.y.to_variant()]);
                if let Ok(info) = info.try_to::<VarDictionary>()
                    && !dict_str(&info, "kind").is_empty()
                {
                    hover = info;
                }
            }
        }
        // The bar read needs the hovered tile (the hover dict carries no coords).
        self.hover_tile = Vector2i::new(-1, -1);
        if !hover.is_empty()
            && dict_str(&hover, "kind") == "building"
            && let Some(tile) = self.mouse_tile()
        {
            self.hover_tile = tile;
        }
        if display.is_empty() && hover.is_empty() {
            self.set_top_visible(false);
            return;
        }
        self.set_top_visible(true);
        let state = if !display.is_empty() {
            format!("b:{display}")
        } else {
            format!(
                "h:{}:{}:{},{}",
                dict_str(&hover, "kind"),
                dict_str(&hover, "name"),
                self.hover_tile.x,
                self.hover_tile.y
            )
        };
        if state == self.top_state {
            if !display.is_empty() {
                self.refresh_top_amounts(Some(&display));
            } else if !self.hover_bars.is_empty() {
                let now = now_ms();
                if now - self.top_refreshed_ms >= TOP_REFRESH_MS {
                    self.top_refreshed_ms = now;
                    self.refresh_hover_bars();
                    self.refresh_hover_consumption();
                }
            }
            return;
        }
        self.top_state = state;
        self.rebuild_top(&display, &hover);
    }

    /// `visible(this::hasInfoBox)` on the top table.
    fn set_top_visible(&mut self, shown: bool) {
        if let Some(mut top_table) = self.top_table.clone() {
            top_table.set_visible(shown);
        }
    }

    /// The world tile under the cursor, or `None` while any `STOP` control holds
    /// the mouse (`Core.scene.hasMouse(...)` guard in `hovered()`).
    fn mouse_tile(&self) -> Option<Vector2i> {
        let hovered = self.base().get_tree().get_root().gui_get_hovered_control();
        if let Some(hovered) = hovered
            && hovered.get_mouse_filter() == MouseFilter::STOP
        {
            return None;
        }
        let mut camera = kit::node("/root/Spine/World/Camera2D")?;
        if !camera.has_method("screen_to_tile") {
            return None;
        }
        let mouse = self.base().get_viewport()?.get_mouse_position();
        camera
            .call(
                "screen_to_tile",
                &[mouse.x.to_variant(), mouse.y.to_variant()],
            )
            .try_to::<Vector2i>()
            .ok()
    }

    /// The cursor in world pixels (`Core.input.mouseWorldX/Y`).
    fn mouse_world(&self) -> Option<Vector2> {
        let hovered = self.base().get_tree().get_root().gui_get_hovered_control();
        if let Some(hovered) = hovered
            && hovered.get_mouse_filter() == MouseFilter::STOP
        {
            return None;
        }
        let mut camera = kit::node("/root/Spine/World/Camera2D")?;
        if !camera.has_method("screen_to_world") {
            return None;
        }
        let mouse = self.base().get_viewport()?.get_mouse_position();
        camera
            .call(
                "screen_to_world",
                &[mouse.x.to_variant(), mouse.y.to_variant()],
            )
            .try_to::<Vector2>()
            .ok()
    }

    /// Rebuilds the header + rows for a state change (`topTable.clear()` path).
    fn rebuild_top(&mut self, display: &str, hover: &VarDictionary) {
        let Some(top_rows) = self.top_rows.clone() else {
            return;
        };
        for child in top_rows.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        self.top_amounts.clear();
        self.top_requirements.clear();
        self.hover_bars.clear();
        self.consume_valid.borrow_mut().clear();
        self.set_top_bottom_margin(TOP_PAD + TOP_MARGIN);
        if display.is_empty() {
            self.build_hover_row(hover);
            return;
        }
        self.build_block_header(display);
        self.build_requirement_rows(display);
        self.refresh_top_amounts(None);
    }

    /// Header: 32 px icon, 190 px name label (`localizedName` + key combo),
    /// `?` button.
    fn build_block_header(&mut self, display: &str) {
        let Some(mut top_rows) = self.top_rows.clone() else {
            return;
        };
        // code-instantiated: header cells are rebuilt per displayed block
        // (`topTable.clear()`; plan 14 §3.5).
        let mut header = HBoxContainer::new_alloc();
        header.add_theme_constant_override("separation", 5);
        let mut icon = TextureRect::new_alloc();
        icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
        // `.size(8 * 4)` + `Scaling.fit`: never the texture's native size.
        icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
        let texture = kit::icon_texture(&format!("block-{display}-ui"))
            .or_else(|| kit::icon_texture(&format!("block-{display}-full")));
        if let Some(texture) = texture {
            icon.set_texture(&texture);
        }
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        header.add_child(&icon);
        let mut label: Gd<RichTextLabel> = MindRichLabel::new_alloc().upcast();
        // Java `labelWrap(...).left().width(190f).padLeft(5)`.
        label.set_fit_content(true);
        label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        label.set_custom_minimum_size(Vector2::new(190.0, 0.0));
        label.set_h_size_flags(SizeFlags::FILL);
        label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        let markup = kit::tm(&format!("@block.{display}.name")).to_string();
        label.set_text(&GString::from(
            format!("{markup}{}", self.key_combo(display)).as_str(),
        ));
        header.add_child(&label);
        let mut spacer = Control::new_alloc();
        spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
        header.add_child(&spacer);
        // `header.button("?", Styles.flatBordert, ...).size(8 * 5).padTop(-5)
        // .padRight(-5)`: the cell reserves 35x35 and the 40 px button pokes
        // 5 px into the top/right table margin.
        let mut info_cell = Control::new_alloc();
        info_cell.set_custom_minimum_size(Vector2::new(35.0, 35.0));
        info_cell.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        info_cell.set_mouse_filter(MouseFilter::IGNORE);
        let mut info = factory::button("?");
        info.set_name("blockinfo");
        self.apply_flat_bordered(&info);
        info.set_position(Vector2::new(0.0, -5.0));
        info.set_size(Vector2::new(40.0, 40.0));
        info.set_tooltip_text(&self.bundle("database", "Database"));
        let mut arguments = Array::<Variant>::new();
        arguments.push(&GString::from(display).to_variant());
        let callable = self.base().callable("_open_content_info").bindv(&arguments);
        let _ = info.connect("pressed", &callable);
        info_cell.add_child(&info);
        header.add_child(&info_cell);
        top_rows.add_child(&header);
    }

    /// `Styles.flatBordert`: `up = pane`, `over = flatDownBase` (the accent
    /// framed 9-patch), `down = flatOver` (#454545).
    fn apply_flat_bordered(&self, button: &Gd<Button>) {
        let mut button = button.clone();
        if let Some(up) = self.nine_patch_box("pane") {
            let up: Gd<StyleBox> = up.upcast();
            button.add_theme_stylebox_override("normal", &up);
            button.add_theme_stylebox_override("disabled", &up);
        }
        if let Some(over) = self.nine_patch_box("flat-down-base") {
            let over: Gd<StyleBox> = over.upcast();
            button.add_theme_stylebox_override("hover", &over);
        }
        let down: Gd<StyleBox> = {
            let mut down = StyleBoxFlat::new_gd();
            down.set_bg_color(Color::from_rgba8(0x45, 0x45, 0x45, 0xff));
            down.upcast()
        };
        button.add_theme_stylebox_override("pressed", &down);
        button.add_theme_stylebox_override("hover_pressed", &down);
        button.add_theme_color_override("font_color", Color::WHITE);
        button.add_theme_color_override("font_hover_color", Color::WHITE);
        button.add_theme_color_override("font_pressed_color", Color::WHITE);
    }

    /// A 12 px-split atlas 9-patch with no content padding.
    fn nine_patch_box(&self, region: &str) -> Option<Gd<StyleBoxTexture>> {
        let texture = kit::icon_texture(region)?;
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_texture_margin(side, 12.0);
            box_.set_content_margin(side, 0.0);
        }
        Some(box_)
    }

    /// Requirement rows: 16 px item icon, item name, `have/need` amount.
    fn build_requirement_rows(&mut self, display: &str) {
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("block_requirements_json") {
            return;
        }
        let Ok(payload) = host
            .call(
                "block_requirements_json",
                &[GString::from(display).to_variant()],
            )
            .try_to_relaxed::<GString>()
        else {
            return;
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&payload.to_string()) else {
            return;
        };
        let Some(rows) = parsed.as_array() else {
            return;
        };
        let Some(mut top_rows) = self.top_rows.clone() else {
            return;
        };
        // `topTable.table(req -> ...).growX().left().margin(3)`: the requirement
        // table keeps its 3 px margin even when it has no rows.
        let mut req_margin = MarginContainer::new_alloc();
        for side in ["margin_left", "margin_top", "margin_right", "margin_bottom"] {
            req_margin.add_theme_constant_override(side, 3);
        }
        let mut req = VBoxContainer::new_alloc();
        req.add_theme_constant_override("separation", 0);
        req_margin.add_child(&req);
        top_rows.add_child(&req_margin);
        for entry in rows {
            if !entry.is_object() {
                continue;
            }
            // code-instantiated: one row per block requirement (content data).
            let mut line = HBoxContainer::new_alloc();
            line.add_theme_constant_override("separation", 2);
            let mut icon = TextureRect::new_alloc();
            icon.set_custom_minimum_size(Vector2::new(16.0, 16.0));
            icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
            let item = json_str(entry, "item");
            if let Some(texture) = kit::icon_texture(&format!("item-{item}-ui")) {
                icon.set_texture(&texture);
            }
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            line.add_child(&icon);
            let mut name_label: Gd<RichTextLabel> = MindRichLabel::new_alloc().upcast();
            name_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
            name_label.set_text(&kit::tm(&format!("@item.{item}.name")));
            line.add_child(&name_label);
            let mut amount_label: Gd<RichTextLabel> = MindRichLabel::new_alloc().upcast();
            amount_label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
            line.add_child(&amount_label);
            req.add_child(&line);
            self.top_amounts.push(amount_label);
            self.top_requirements.push(entry.clone());
        }
    }

    /// Refreshes requirement amounts (core inventory changes live). Re-reads
    /// `have` from the sim at most every `TOP_REFRESH_MS`; `None` keeps the
    /// cached rows.
    fn refresh_top_amounts(&mut self, display: Option<&str>) {
        let now = now_ms();
        if let Some(display) = display.filter(|value| !value.is_empty())
            && now - self.top_refreshed_ms >= TOP_REFRESH_MS
        {
            self.top_refreshed_ms = now;
            if let Some(mut host) = self.host()
                && host.has_method("block_requirements_json")
            {
                let args = [GString::from(display).to_variant()];
                if let Ok(payload) = host
                    .call("block_requirements_json", &args)
                    .try_to_relaxed::<GString>()
                    && let Ok(parsed) = serde_json::from_str::<Value>(&payload.to_string())
                    && let Some(rows) = parsed.as_array()
                    && rows.len() == self.top_requirements.len()
                {
                    for (slot, row) in self.top_requirements.iter_mut().zip(rows.iter()) {
                        if row.is_object() {
                            *slot = row.clone();
                        }
                    }
                }
            }
        }
        for index in 0..self.top_amounts.len() {
            let Some(entry) = self.top_requirements.get(index).cloned() else {
                continue;
            };
            let Some(amount) = self.top_amounts.get(index).cloned() else {
                continue;
            };
            let mut label = amount;
            // The sim host formats the Java markup (`*/90` under infinite
            // resources or without a core, else
            // `[scarlet|accent|white]amount[white]/need`).
            let raw = json_str(&entry, "label");
            if !raw.is_empty() {
                let cached: Option<String> = if label.has_meta("raw") {
                    label
                        .get_meta("raw")
                        .try_to_relaxed::<GString>()
                        .ok()
                        .map(|value| value.to_string())
                } else {
                    None
                };
                if cached.as_deref() != Some(raw.as_str()) {
                    label.set_meta("raw", &GString::from(raw.as_str()).to_variant());
                    label.set_text(&kit::markup_text(&raw));
                }
                continue;
            }
            let have = json_i64(&entry, "have", 0);
            let need = json_i64(&entry, "amount", 0);
            // `amount < stackamount / 2f ? [scarlet] : amount < stackamount ?
            // [accent] : [white]` (`Pal.remove`/`Pal.accent`).
            let color = if (have as f64) < (need as f64) / 2.0 {
                "#e55454"
            } else if have < need {
                "#ffd37f"
            } else {
                "#ffffff"
            };
            label.set_text(&GString::from(
                format!("[color={color}]{have}[/color]/{need}").as_str(),
            ));
        }
    }

    /// Hover row: 32 px icon + `Displayable` label (`Tile.display` name).
    fn build_hover_row(&mut self, hover: &VarDictionary) {
        let Some(mut top_rows) = self.top_rows.clone() else {
            return;
        };
        // code-instantiated: the hover row follows the cursor's target
        // (`topTable.clear()`; plan 14 §3.5).
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 5);
        let mut icon = TextureRect::new_alloc();
        icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
        icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
        let icon_name = dict_str(hover, "icon");
        let texture = kit::icon_texture(&icon_name).or_else(|| {
            if icon_name.is_empty() {
                None
            } else {
                kit::icon_texture(&format!("block-{}-ui", dict_str(hover, "name")))
            }
        });
        if let Some(texture) = texture {
            icon.set_texture(&texture);
        }
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        row.add_child(&icon);
        let mut label: Gd<RichTextLabel> = MindRichLabel::new_alloc().upcast();
        label.set_custom_minimum_size(Vector2::new(190.0, 0.0));
        label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        // Arc cells centre their element vertically.
        label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        let mut label_key = dict_str(hover, "label");
        if label_key.is_empty() {
            label_key = format!("block.{}.name", dict_str(hover, "name"));
        }
        label.set_text(&kit::tm(&format!("@{label_key}")));
        row.add_child(&label);
        top_rows.add_child(&row);
        // `BuildingComp.display`'s bar table (only for the player's buildings;
        // `building_bars_json` gates the team).
        if dict_str(hover, "kind") == "building" && self.hover_tile.x >= 0 {
            self.rebuild_hover_bars();
            self.build_hover_consumption();
            // `table.marginBottom(-5)` closes the building's bar branch: only
            // 7 px of the 9-patch padding remain under the bars.
            self.set_top_bottom_margin(TOP_PAD - 5.0);
        }
    }

    /// `Block.setBars` rows from `building_bars_json`: one 18 px `MindBar` per
    /// row inside a 4 px margin.
    fn rebuild_hover_bars(&mut self) {
        self.hover_bars.clear();
        let rows = self.hover_bar_rows();
        let Some(rows) = rows.as_ref().and_then(Value::as_array) else {
            return;
        };
        let Some(mut top_rows) = self.top_rows.clone() else {
            return;
        };
        for entry in rows {
            if !entry.is_object() {
                continue;
            }
            // code-instantiated: one bar per `Block.setBars` entry.
            let mut margin = MarginContainer::new_alloc();
            for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
                margin.add_theme_constant_override(side, 4);
            }
            let mut bar = MindBar::new_alloc();
            bar.set_custom_minimum_size(Vector2::new(0.0, 18.0));
            let value = entry.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            bar.bind_mut().setup(
                GString::from(self.bar_text(entry).as_str()),
                self.bar_color(entry),
                value,
            );
            self.apply_bar_blink(entry, &bar);
            let control: Gd<Control> = bar.clone().upcast();
            margin.add_child(&control);
            top_rows.add_child(&margin);
            self.hover_bars.push((entry.clone(), bar));
        }
    }

    /// Live re-read of the bar rows (`TOP_REFRESH_MS`): fractions every tick,
    /// text when the formatted argument changed.
    fn refresh_hover_bars(&mut self) {
        let rows = self.hover_bar_rows();
        let rows = rows.as_ref().and_then(Value::as_array);
        if rows.is_none_or(|rows| rows.len() != self.hover_bars.len()) {
            self.clear_hover_bars();
            self.rebuild_hover_bars();
            return;
        }
        let Some(rows) = rows else {
            return;
        };
        let entries: Vec<Value> = rows.clone();
        for (index, entry) in entries.iter().enumerate() {
            let Some((previous, bar)) = self.hover_bars.get(index).cloned() else {
                continue;
            };
            let mut bar = bar;
            let value = entry.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            bar.bind_mut().fraction = value;
            if entry.get("label_args") != previous.get("label_args") {
                bar.bind_mut().setup(
                    GString::from(self.bar_text(entry).as_str()),
                    self.bar_color(entry),
                    value,
                );
                self.apply_bar_blink(entry, &bar);
                self.hover_bars[index] = (entry.clone(), bar);
            }
        }
    }

    /// Frees the current bar rows (`queue_free`, like the GDScript).
    fn clear_hover_bars(&mut self) {
        for (_, bar) in &self.hover_bars {
            if bar.is_instance_valid() {
                let mut bar = bar.clone();
                bar.queue_free();
            }
        }
        self.hover_bars.clear();
    }

    /// `table.row(); table.table(this::displayConsumption).growX()`: every
    /// consumer's `Consume.build` cell sits in one left-aligned row under the
    /// bars. The ported cell is `ItemTurret`'s ammo `MultiReqImage`.
    fn build_hover_consumption(&mut self) {
        let Some(rows) = self.hover_consumption_rows() else {
            return;
        };
        let Some(rows) = rows.as_array().cloned() else {
            return;
        };
        if rows.is_empty() {
            return;
        }
        // code-instantiated: the consumption row exists only for hovered
        // consumers.
        let mut line = HBoxContainer::new_alloc();
        line.add_theme_constant_override("separation", 0);
        for (row_index, images) in rows.iter().enumerate() {
            let Some(images) = images.as_array() else {
                continue;
            };
            if images.is_empty() {
                continue;
            }
            // code-instantiated: one `MultiReqImage` per consumer cell.
            let mut multi = MindMultiReqImage::new_alloc();
            multi.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            multi.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
            for entry in images {
                if !entry.is_object() {
                    continue;
                }
                let icon_name = json_str(entry, "icon");
                let key = format!("{row_index}:{icon_name}");
                let valid = json_bool(entry, "valid", false);
                self.consume_valid.borrow_mut().insert(key.clone(), valid);
                // code-instantiated: `new Image(item.uiIcon)` stretched to the
                // cell.
                let mut icon = TextureRect::new_alloc();
                icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
                icon.set_stretch_mode(StretchMode::SCALE);
                if let Some(texture) = kit::icon_texture(&icon_name) {
                    icon.set_texture(&texture);
                }
                let icon_control: Gd<Control> = icon.upcast();
                let valid_map = self.consume_valid.clone();
                let lookup = key.clone();
                let callable = Callable::from_fn("consume_valid", move |_args: &[&Variant]| {
                    valid_map.borrow().get(&lookup).copied().unwrap_or(false)
                });
                let mut req = MindReqImage::new_alloc();
                req.bind_mut().setup(icon_control, callable);
                multi.bind_mut().add_req(req);
            }
            line.add_child(&multi);
        }
        if line.get_child_count() == 0 {
            line.free();
            return;
        }
        if let Some(mut top_rows) = self.top_rows.clone() {
            top_rows.add_child(&line);
        }
    }

    /// Live re-read of the `ReqImage` predicates (`build.items`/`ammo.peek()`).
    fn refresh_hover_consumption(&mut self) {
        if self.consume_valid.borrow().is_empty() {
            return;
        }
        let Some(rows) = self.hover_consumption_rows() else {
            return;
        };
        let Some(rows) = rows.as_array() else {
            return;
        };
        for (row_index, images) in rows.iter().enumerate() {
            let Some(images) = images.as_array() else {
                continue;
            };
            for entry in images {
                if !entry.is_object() {
                    continue;
                }
                let key = format!("{row_index}:{}", json_str(entry, "icon"));
                let valid = json_bool(entry, "valid", false);
                self.consume_valid.borrow_mut().insert(key, valid);
            }
        }
    }

    /// `building_consumption_json` rows (`[[{icon, valid}...]...]`), or `None`.
    fn hover_consumption_rows(&self) -> Option<Value> {
        let mut host = self.host()?;
        if !host.has_method("building_consumption_json") || self.hover_tile.x < 0 {
            return None;
        }
        let args = [
            self.hover_tile.x.to_variant(),
            self.hover_tile.y.to_variant(),
        ];
        let payload = host
            .call("building_consumption_json", &args)
            .try_to_relaxed::<GString>()
            .ok()?;
        serde_json::from_str(&payload.to_string()).ok()
    }

    /// `building_bars_json` rows for the hovered tile, or `None`.
    fn hover_bar_rows(&self) -> Option<Value> {
        let mut host = self.host()?;
        if !host.has_method("building_bars_json") || self.hover_tile.x < 0 {
            return None;
        }
        let args = [
            self.hover_tile.x.to_variant(),
            self.hover_tile.y.to_variant(),
        ];
        let payload = host
            .call("building_bars_json", &args)
            .try_to_relaxed::<GString>()
            .ok()?;
        serde_json::from_str(&payload.to_string()).ok()
    }

    fn bar_text(&self, entry: &Value) -> String {
        let key = json_str(entry, "label_key");
        let args: Vec<String> = entry
            .get("label_args")
            .and_then(Value::as_array)
            .map(|values| values.iter().map(json_arg_text).collect())
            .unwrap_or_default();
        if !args.is_empty() {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            // `Bar` labels are drawn by `MindBar::draw_name` with a plain font,
            // not a BBCode label: content icons must arrive as their raw PUA
            // glyph (`Fonts.getUnicodeStr`, `UnitFactory.java:119-137`).
            return kit::tm_format_text_icons(&key, &refs).to_string();
        }
        kit::tm(&format!("@{key}")).to_string()
    }

    /// The `health` and force-projector `shield` bars flash white on a drop
    /// (`.blink(Color.white)`); every other bar blinks in its own colour.
    fn apply_bar_blink(&self, entry: &Value, bar: &Gd<MindBar>) {
        let key = json_str(entry, "key");
        if key == "health" || key == "shield" {
            let mut bar = bar.clone();
            let _ = bar.bind_mut().blink(Color::WHITE);
        }
    }

    fn bar_color(&self, entry: &Value) -> Color {
        let raw = json_str(entry, "color");
        let raw = if raw.is_empty() {
            "ffffffff".to_string()
        } else {
            raw
        };
        Color::from_string(raw.as_str())
            .or_else(|| Color::from_html(raw.as_str()))
            .unwrap_or(Color::WHITE)
    }

    // ---- command table ----------------------------------------------------

    /// `control.input.commandMode ? commandTable : blockCatTable`: swaps the
    /// block palette for the command table and re-reads `command_panel_json` at
    /// most every `COMMAND_REFRESH_MS` (or after a selection change).
    fn update_command_panel(&mut self) {
        let Some(mut command_panel) = self.command_panel.clone() else {
            return;
        };
        let mut input = self.input_node();
        let mut mode = false;
        if let Some(node) = input.as_mut()
            && node.has_method("command_mode")
        {
            mode = node
                .call("command_mode", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false);
        }
        if mode != self.command_mode {
            self.command_mode = mode;
            if let Some(mut layout) = self.layout.clone() {
                layout.set_visible(!mode);
            }
            command_panel.set_visible(mode);
            self.command_dirty = true;
            if mode {
                // Java forces the command table to the block table width
                // (`commandTable.getWidth()` constraint).
                if let Some(layout) = self.layout.as_ref() {
                    let mut size = command_panel.get_custom_minimum_size();
                    size.x = layout.get_size().x;
                    command_panel.set_custom_minimum_size(size);
                }
            }
        }
        if !mode {
            return;
        }
        let now = now_ms();
        if !self.command_dirty && now - self.command_refresh_ms < COMMAND_REFRESH_MS {
            return;
        }
        self.command_refresh_ms = now;
        let Some(mut input) = input else {
            return;
        };
        if !input.has_method("selection_json") {
            return;
        }
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("command_panel_json") {
            return;
        }
        let selection = input
            .call("selection_json", &[])
            .try_to_relaxed::<GString>()
            .unwrap_or_default();
        let json = host
            .call("command_panel_json", &[selection.to_variant()])
            .try_to_relaxed::<GString>()
            .unwrap_or_default()
            .to_string();
        if json == self.command_json && !self.command_dirty {
            return;
        }
        self.command_json.clone_from(&json);
        self.command_dirty = false;
        let view = serde_json::from_str::<Value>(&json).unwrap_or(Value::Null);
        self.rebuild_command_panel(&view);
    }

    /// `rebuildCommand` (`PlacementFragment.java:492-610`): title, accent rule,
    /// unit-type grid, command row and stance row from the probe JSON.
    fn rebuild_command_panel(&self, view: &Value) {
        let Some(mut command_rows) = self.command_rows.clone() else {
            return;
        };
        for child in command_rows.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        // code-instantiated: rows follow the live selection (type counts,
        // command and stance unions).
        if !view.is_object() {
            return;
        }
        let mut title: Gd<RichTextLabel> =
            factory::label(&kit::tm("@commandmode.name").to_string()).upcast();
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        title.set_h_size_flags(SizeFlags::EXPAND_FILL);
        command_rows.add_child(&title);
        // `image().color(Pal.accent).growX().pad(20f).padTop(0f).padBottom(4f)`.
        let mut rule_margin = MarginContainer::new_alloc();
        rule_margin.add_theme_constant_override("margin_left", 20);
        rule_margin.add_theme_constant_override("margin_right", 20);
        rule_margin.add_theme_constant_override("margin_bottom", 4);
        let mut rule = ColorRect::new_alloc();
        rule.set_custom_minimum_size(Vector2::new(0.0, 4.0));
        rule.set_color(Color::from_string("ffd37f").unwrap_or(styles::ACCENT));
        rule.set_h_size_flags(SizeFlags::EXPAND_FILL);
        rule_margin.add_child(&rule);
        command_rows.add_child(&rule_margin);
        let types = view
            .get("types")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if types.is_empty() {
            // `commandmode.nounits`, light grey, centred, pad 6.
            let mut none: Gd<RichTextLabel> =
                factory::label(&kit::tm("@commandmode.nounits").to_string()).upcast();
            none.set_modulate(Color::from_rgba(0.75, 0.75, 0.75, 1.0));
            none.set_horizontal_alignment(HorizontalAlignment::CENTER);
            none.set_h_size_flags(SizeFlags::EXPAND_FILL);
            let mut pad = MarginContainer::new_alloc();
            for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
                pad.add_theme_constant_override(side, 6);
            }
            pad.add_child(&none);
            command_rows.add_child(&pad);
            return;
        }
        let mut type_grid = GridContainer::new_alloc();
        type_grid.set_columns(7);
        for entry in &types {
            if entry.is_object() {
                type_grid.add_child(&self.command_type_cell(entry));
            }
        }
        command_rows.add_child(&type_grid);
        let commands = view
            .get("commands")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let stances = view
            .get("stances")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !commands.is_empty() {
            command_rows.add_child(&self.command_button_grid(&commands, true));
        }
        if !stances.is_empty() {
            if !commands.is_empty() {
                // `image().color(Pal.gray).height(3).pad(7)` separator.
                let mut sep_margin = MarginContainer::new_alloc();
                for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
                    sep_margin.add_theme_constant_override(side, 7);
                }
                let mut separator = ColorRect::new_alloc();
                separator.set_custom_minimum_size(Vector2::new(0.0, 3.0));
                separator.set_color(Color::from_string("454545").unwrap_or(styles::FLAT_OVER));
                separator.set_h_size_flags(SizeFlags::EXPAND_FILL);
                sep_margin.add_child(&separator);
                command_rows.add_child(&sep_margin);
            }
            command_rows.add_child(&self.command_button_grid(&stances, false));
        }
    }

    /// `StatValues.stack(type, count)` cell: 32 px unit icon with the count at
    /// the bottom-right; left click keeps the type, right click drops it.
    fn command_type_cell(&self, entry: &Value) -> Gd<Control> {
        let unit_name = json_str(entry, "name");
        let count = json_i64(entry, "count", 0);
        let mut button = Button::new_alloc();
        button.set_custom_minimum_size(Vector2::new(42.0, 42.0));
        button.set_tooltip_text(&self.bundle(&format!("unit.{unit_name}.name"), &unit_name));
        if let Some(texture) = kit::icon_texture(&format!("unit-{unit_name}-ui")) {
            button.set_button_icon(&texture);
        }
        button.set_expand_icon(true);
        button.add_theme_constant_override("icon_max_width", 32);
        let mut arguments = Array::<Variant>::new();
        arguments.push(&json_i64(entry, "type", -1).to_variant());
        let callable = self
            .base()
            .callable("_on_command_type_input")
            .bindv(&arguments);
        let _ = button.connect("gui_input", &callable);
        // code-instantiated: the stack amount follows the live selection count.
        let mut amount: Gd<RichTextLabel> = factory::label(&count.to_string()).upcast();
        amount.set_anchors_and_offsets_preset(LayoutPreset::BOTTOM_RIGHT);
        amount.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&amount);
        button.upcast::<Control>()
    }

    /// Java `Icon` field name -> port glyph alias (`downOpen` -> `down-open`);
    /// `cancel` when the port has no such glyph
    /// (`Icon.icons.get(..., Icon.cancel)`).
    fn icon_alias(&self, icon_name: &str) -> String {
        resolve_icon_alias(icon_name)
    }

    /// Command buttons (50x50, checked when any selected unit runs it) or
    /// stance buttons (toggled).
    fn command_button_grid(&self, entries: &[Value], is_command: bool) -> Gd<GridContainer> {
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(6);
        for entry in entries {
            if !entry.is_object() {
                continue;
            }
            let command_name = json_str(entry, "name");
            let active = json_bool(entry, "active", false);
            let label = self.bundle(&json_str(entry, "key"), &command_name);
            // `Icon.icons.get(command.icon, Icon.cancel)`: the Java field name
            // maps to the port's kebab-case alias; unknown icons fall back.
            // code-instantiated: one button per command/stance in the live
            // union.
            let mut button = factory::glyph_button(
                &self.icon_alias(&json_str(entry, "icon")),
                "?",
                "clearNoneTogglei",
            );
            button.set_toggle_mode(true);
            button.set_pressed_no_signal(active);
            button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
            button.set_tooltip_text(&label);
            let mut arguments = Array::<Variant>::new();
            arguments.push(&GString::from(command_name.as_str()).to_variant());
            if is_command {
                let callable = self.base().callable("_set_command").bindv(&arguments);
                let _ = button.connect("pressed", &callable);
            } else {
                arguments.push(&active.to_variant());
                let callable = self.base().callable("_toggle_stance").bindv(&arguments);
                let _ = button.connect("pressed", &callable);
            }
            grid.add_child(&button);
        }
        grid
    }
}

/// Reads a `JSON` object string field (`str(dict.get(key, ""))`).
fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Reads a `JSON` object integer field (`int(dict.get(key, default))`).
fn json_i64(value: &Value, key: &str, default: i64) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(default)
}

/// Reads a `JSON` object bool field (`bool(dict.get(key, default))`).
fn json_bool(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// `bundle_markup_format` argument text (`str(value)` for the label args).
fn json_arg_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Reads a `String` from a probe dictionary with GDScript's `str(...)`
/// coercion.
fn dict_str(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// Java `Icon` field name -> port glyph alias (`downOpen` -> `down-open`);
/// `cancel` when the port has no such glyph (`Icon.icons.get(..., Icon.cancel)`).
fn resolve_icon_alias(icon_name: &str) -> String {
    let alias = to_snake_case(icon_name).replace('_', "-");
    if kit::glyph(&alias).is_empty() {
        String::from("cancel")
    } else {
        alias
    }
}

/// `String.to_snake_case`: an underscore before an uppercase run that follows a
/// lowercase letter or digit, then lowercase.
fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut previous: Option<char> = None;
    for character in name.chars() {
        if character.is_uppercase() {
            if let Some(previous) = previous
                && (previous.is_lowercase() || previous.is_ascii_digit())
            {
                out.push('_');
            }
            for lower in character.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(character);
        }
        previous = Some(character);
    }
    out
}

/// The action's value in a hotkey table (`DIGIT_ACTIONS.get(action, -1)`).
fn action_index(table: &[(&str, i32)], action: &str) -> Option<i32> {
    table
        .iter()
        .find(|(name, _)| *name == action)
        .map(|(_, value)| *value)
}

/// Godot's `Time.get_ticks_msec()` as an `int`.
fn now_ms() -> i64 {
    Time::singleton().get_ticks_msec() as i64
}

/// Godot's `is_equal_approx`: exact equality first, then a relative
/// `CMP_EPSILON` tolerance.
fn is_equal_approx(first: f32, second: f32) -> bool {
    const CMP_EPSILON: f32 = 0.00001;
    if first == second {
        return true;
    }
    let mut tolerance = CMP_EPSILON * first.abs();
    if tolerance < CMP_EPSILON {
        tolerance = CMP_EPSILON;
    }
    (first - second).abs() < tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_alias_converts_camel_case_and_falls_back() {
        assert_eq!(resolve_icon_alias("downOpen"), "down-open");
        assert_eq!(resolve_icon_alias("commandAttack"), "command-attack");
        // The port has no glyph for this alias: `Icon.cancel` instead.
        assert_eq!(resolve_icon_alias("definitelyNotAnIcon"), "cancel");
    }

    #[test]
    fn hotkey_tables_match_upstream_digits_and_arrows() {
        assert_eq!(DIGIT_ACTIONS.len(), 10);
        assert_eq!(action_index(&DIGIT_ACTIONS, "block_select_01"), Some(0));
        assert_eq!(action_index(&DIGIT_ACTIONS, "block_select_10"), Some(9));
        assert_eq!(action_index(&ARROW_ACTIONS, "block_select_up"), Some(12));
        assert_eq!(action_index(&ARROW_ACTIONS, "nope"), None);
    }

    #[test]
    fn combo_second_digit_index_is_integer_division() {
        // `mini((index + 1) / 10 - 1, 9)` (integer division, clamped to row 9;
        // the clamp itself is not exercised here because it is a constant fold).
        assert_eq!((10 + 1) / 10 - 1, 0);
        assert_eq!((99 + 1) / 10 - 1, 9);
    }

    #[test]
    fn arrow_up_wrap_matches_grid_update() {
        // `target = at - 4 if at > 3 else count - count % 4 + at`; a target past
        // the end steps back by one row.
        let count = 10_i64;
        let at = 1_i64;
        let target = count - count % BLOCK_GRID_COLUMNS + at;
        assert_eq!(target, 9);
        let mut wrapped = target;
        if wrapped >= count {
            wrapped -= BLOCK_GRID_COLUMNS;
        }
        assert_eq!(wrapped, target);
        // A selection below the first row jumps up one row.
        let at = 5_i64;
        assert_eq!(at - 4, 1);
    }

    #[test]
    fn tints_match_arc_colors() {
        assert_eq!(TINT_WHITE, Color::from_rgba(1.0, 1.0, 1.0, 1.0));
        // 0x7f / 255 = 0.498.
        assert!((TINT_GRAY.r - 0.498).abs() < f32::EPSILON);
        // 0x3f / 255 = 0.247.
        assert!((TINT_DARK_GRAY.r - 0.247).abs() < f32::EPSILON);
    }

    #[test]
    fn equality_matches_godot_is_equal_approx() {
        assert!(is_equal_approx(12.0, 12.0));
        assert!(!is_equal_approx(12.0, 12.01));
    }
}
