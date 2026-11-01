// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/BlockInventoryFragment.java
//         (plan 14 §3.5, M4; G03 T13 presentation pass).
//
// `Tex.inventory` panel at the building's top-right: a 3-column grid of 40x40
// item icons with the live amount bottom-left, tapping takes the whole stack,
// holding takes one every 20 ticks, the panel follows the building and closes
// when it is gone, empty for 120 ticks, or tapped again.

//! `MindBlockInventoryFragment` — the building inventory overlay.
//!
//! 1:1 port of `client/ui/fragments/block_inventory_fragment.gd`. The item grid
//! is built from the `MindSimHost.building_items_json` read model (one control
//! per item, count driven by the item module), the panel tracks the building's
//! top-right corner through the camera read model, and every withdraw goes
//! through `MindInput.transfer_item` (a sim command) and the `take_requested`
//! signal. Nothing here touches sim state.

use std::collections::HashMap;

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Control, GridContainer, IControl, InputEvent, InputEventMouseButton, Json, Label,
    PanelContainer, StyleBoxTexture, TextureRect,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::kit;

/// `BlockInventoryFragment.holdWithdraw` (ticks per held withdraw).
const HOLD_WITHDRAW: f64 = 20.0;
/// `BlockInventoryFragment.holdShrink` (ticks before an absent item's cell goes).
const HOLD_SHRINK: f64 = 120.0;
/// Cell size (`size(8 * 5)`), grid columns (`cols = 3`).
const CELL: f32 = 40.0;
const COLUMNS: i32 = 3;
/// Live re-read cadence (seconds).
const REFRESH_INTERVAL: f64 = 0.1;
/// `Tile.SIZE` (`BlockInventoryFragment` anchors on raw tile coordinates).
const TILESIZE: f64 = 8.0;

/// One inventory cell: the amount label and the last read amount
/// (`_cells[name] = [control, label, amount]`).
///
/// The GDScript also parked the cell `Control` in that array but never read it
/// back; the grid owns it, so only the two live fields are kept here.
struct InventoryCell {
    label: Gd<Label>,
    amount: i64,
}

/// `MindBlockInventoryFragment` — the item panel of one building.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindBlockInventoryFragment {
    base: Base<Control>,

    /// The building the panel follows (`Vector2i(-1, -1)` when closed).
    tile: Vector2i,
    /// Item name -> cell (insertion order mirrored in [`Self::order`]).
    cells: HashMap<String, InventoryCell>,
    /// Item names in the current display order (`_order`).
    order: Vec<String>,
    /// Ticks the inventory has been empty (`emptyTime`).
    empty_time: f64,
    /// Item name -> ticks absent (`holdShrink` tracking).
    absent: HashMap<String, f64>,
    /// Seconds since the last live re-read.
    accum: f64,
    /// Held cell state (`_holding`, `_hold_time`, `_held`).
    holding: String,
    hold_time: f64,
    held: bool,
}

#[godot_api]
impl IControl for MindBlockInventoryFragment {
    fn ready(&mut self) {
        self.base_mut().set_visible(false);
        // The GDScript member initializer; `#[class(init)]` would zero it.
        self.tile = Vector2i::new(-1, -1);
        self.setup_style();
        if let Some(mut grid) = self.grid() {
            grid.set_columns(COLUMNS);
            grid.add_theme_constant_override("h_separation", 4);
            grid.add_theme_constant_override("v_separation", 4);
        }
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        if !self.base().is_visible() {
            return;
        }
        self.position_panel();
        if !self.holding.is_empty() {
            self.hold_time += delta * 60.0;
        }
        if !self.holding.is_empty() && self.hold_time >= HOLD_WITHDRAW {
            let item = self.holding.clone();
            self.take(&item, 1);
            self.hold_time = 0.0;
            self.held = true;
        }
        self.accum += delta;
        if self.accum < REFRESH_INTERVAL {
            return;
        }
        self.accum = 0.0;
        self.refresh();
    }
}

#[godot_api]
impl MindBlockInventoryFragment {
    /// `take_requested(item, amount)`: a withdraw the owning HUD applies.
    #[signal]
    fn take_requested(item: GString, amount: i64);

    /// Opens the panel for a tile with a `{item_name: amount}` JSON read model;
    /// tapping the same building again closes it (`showFor`).
    #[func]
    pub fn open_at(&mut self, tile: Vector2i, items_json: GString) {
        if self.base().is_visible() && tile == self.tile {
            self.hide_panel();
            return;
        }
        self.tile = tile;
        self.empty_time = 0.0;
        self.absent.clear();
        let parsed = Json::parse_string(&items_json);
        let items = parsed.try_to::<VarDictionary>().unwrap_or_default();
        if items.is_empty() {
            self.hide_panel();
            return;
        }
        self.set_items(items);
        self.base_mut().set_visible(true);
    }

    /// Rebuilds the item grid from a `{item_name: amount}` read model.
    #[func]
    pub fn set_items(&mut self, items: VarDictionary) {
        let Some(grid) = self.grid() else {
            return;
        };
        for child in grid.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        self.cells.clear();
        self.order.clear();
        // code-instantiated: one cell per inventory item (count driven by the
        // item module read model; G03 T13).
        for (item_name, amount) in item_entries(&items) {
            self.add_cell(&item_name, amount);
        }
        let visible = !items.is_empty();
        self.base_mut().set_visible(visible);
    }

    /// Hides the panel (tapped again, empty for 120 ticks, or the building is
    /// gone).
    #[func]
    pub fn hide_panel(&mut self) {
        self.base_mut().set_visible(false);
        self.tile = Vector2i::new(-1, -1);
        self.cells.clear();
        self.order.clear();
        self.holding.clear();
        self.hold_time = 0.0;
        self.held = false;
    }

    /// Cell `gui_input`: a press holds the cell, a release without a hold in
    /// between takes the whole stack.
    #[func(rename = "_on_cell_input")]
    pub fn on_cell_input(&mut self, event: Gd<InputEvent>, item_name: GString) {
        let Ok(button) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if button.get_button_index() != godot::global::MouseButton::LEFT {
            return;
        }
        let item_name = item_name.to_string();
        if button.is_pressed() {
            self.holding = item_name;
            self.hold_time = 0.0;
            self.held = false;
            return;
        }
        if !self.held {
            let amount = self
                .cells
                .get(&item_name)
                .map(|cell| cell.amount)
                .unwrap_or(0);
            self.take(&item_name, amount);
        }
        self.holding.clear();
        self.hold_time = 0.0;
    }
}

impl MindBlockInventoryFragment {
    /// `Panel` (`@onready var panel`).
    fn panel(&self) -> Option<Gd<PanelContainer>> {
        self.base()
            .get_node_or_null("Panel")?
            .try_cast::<PanelContainer>()
            .ok()
    }

    /// `Panel/Grid`.
    fn grid(&self) -> Option<Gd<GridContainer>> {
        self.base()
            .get_node_or_null("Panel/Grid")?
            .try_cast::<GridContainer>()
            .ok()
    }

    /// `Tex.inventory` ninepatch (margins 4) behind the grid.
    fn setup_style(&mut self) {
        let Some(mut panel) = self.panel() else {
            return;
        };
        let Some(texture) = kit::icon_texture("inventory") else {
            return;
        };
        let mut style = StyleBoxTexture::new_gd();
        style.set_texture(&texture);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            style.set_texture_margin(side, 4.0);
            style.set_content_margin(side, 4.0);
        }
        panel.add_theme_stylebox_override("panel", &style);
    }

    /// `BlockInventoryFragment.update`: follows the building's top-right corner.
    /// The panel hugs its `Tex.inventory` table (`table.pack()`), not the
    /// scene's 260x200 stub — Java draws a compact box around the 3-column
    /// 40 px grid.
    fn position_panel(&mut self) {
        let Some(mut camera) = kit::node("/root/Spine/World/Camera2D") else {
            return;
        };
        if !camera.has_method("world_to_screen") {
            return;
        }
        let mut block_size = 1i64;
        if let Some(mut host) = kit::sim_host()
            && host.has_method("block_size")
        {
            block_size = host
                .call(
                    "block_size",
                    &[self.tile.x.to_variant(), self.tile.y.to_variant()],
                )
                .try_to_relaxed::<i64>()
                .unwrap_or(1)
                .max(1);
        }
        let corner_x = f64::from(self.tile.x) * TILESIZE + block_size as f64 * 4.0;
        let corner_y = f64::from(self.tile.y) * TILESIZE + block_size as f64 * 4.0;
        let screen = camera.call(
            "world_to_screen",
            &[corner_x.to_variant(), corner_y.to_variant()],
        );
        if let Ok(point) = screen.try_to::<Vector2>() {
            self.base_mut().set_position(point);
        }
        if let Some(panel) = self.panel() {
            // `table.pack()`: the fragment is exactly the inventory table (the
            // scene stub is 260x200, which stretched the ninepatch).
            let size = panel.get_combined_minimum_size();
            self.base_mut().set_size(size);
            self.base_mut().set_pivot_offset(size / 2.0);
        }
    }

    /// Live amounts every 100 ms; hide when the building is gone, the inventory
    /// has been empty for 120 ticks, or rebuild when an item appears/disappears.
    fn refresh(&mut self) {
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("building_items_json") {
            return;
        }
        let tile = host.call(
            "tile_debug",
            &[self.tile.x.to_variant(), self.tile.y.to_variant()],
        );
        if let Ok(tile) = tile.try_to::<VarDictionary>()
            && !dict_bool(&tile, "has_build", false)
        {
            self.hide_panel();
            return;
        }
        let payload = host
            .call(
                "building_items_json",
                &[self.tile.x.to_variant(), self.tile.y.to_variant()],
            )
            .try_to_relaxed::<GString>()
            .unwrap_or_default();
        let parsed = Json::parse_string(&payload);
        let items = parsed.try_to::<VarDictionary>().unwrap_or_default();
        let entries = item_entries(&items);
        let mut changed = false;
        // Cells that are still present: re-read the amount in place and drop the
        // absence timer (`_absent.erase(item_key)`).
        for (item_name, amount) in &entries {
            if !self.cells.contains_key(item_name) {
                continue;
            }
            let text = self.format_amount(*amount as f64);
            if let Some(cell) = self.cells.get_mut(item_name) {
                cell.amount = *amount;
                cell.label.set_text(&text);
            }
            self.absent.remove(item_name);
        }
        // Items the panel has not seen yet take a new cell.
        for (item_name, amount) in &entries {
            if self.cells.contains_key(item_name) {
                continue;
            }
            self.add_cell(item_name, *amount);
            changed = true;
        }
        // Items that vanished are only dropped once `holdShrink` ticks passed
        // without them (`BlockInventoryFragment.update`).
        for item_key in self.order.clone() {
            if items.contains_key(item_key.as_str()) {
                continue;
            }
            let elapsed =
                self.absent.get(&item_key).copied().unwrap_or(0.0) + REFRESH_INTERVAL * 60.0;
            self.absent.insert(item_key, elapsed);
            if elapsed >= HOLD_SHRINK {
                changed = true;
            }
        }
        if changed {
            self.set_items(items.clone());
        }
        if items.is_empty() {
            self.empty_time += REFRESH_INTERVAL * 60.0;
            if self.empty_time >= HOLD_SHRINK {
                self.hide_panel();
            }
        } else {
            self.empty_time = 0.0;
        }
    }

    /// `size(8 * 5)` cell: item icon (`item-<name>-ui`) with the amount
    /// bottom-left.
    fn add_cell(&mut self, item_name: &str, amount: i64) {
        let Some(mut grid) = self.grid() else {
            return;
        };
        let mut cell = Control::new_alloc();
        cell.set_custom_minimum_size(Vector2::new(CELL, CELL));
        cell.set_mouse_filter(MouseFilter::STOP);
        let mut arguments = Array::<Variant>::new();
        arguments.push(&GString::from(item_name).to_variant());
        let callable = self.base().callable("_on_cell_input").bindv(&arguments);
        cell.connect("gui_input", &callable);
        let mut icon = TextureRect::new_alloc();
        icon.set_anchors_preset(LayoutPreset::FULL_RECT);
        icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        let texture = kit::icon_texture(&format!("item-{item_name}-ui"))
            .or_else(|| kit::icon_texture(&format!("item-{item_name}")));
        if let Some(texture) = texture {
            icon.set_texture(&texture);
        }
        cell.add_child(&icon);
        let mut label = Label::new_alloc();
        label.set_anchors_preset(LayoutPreset::BOTTOM_LEFT);
        label.set_text(&self.format_amount(amount as f64));
        cell.add_child(&label);
        grid.add_child(&cell);
        self.cells
            .insert(item_name.to_string(), InventoryCell { label, amount });
        self.order.push(item_name.to_string());
    }

    fn take(&mut self, item: &str, amount: i64) {
        if let Some(mut input) = kit::node("/root/Spine/Input")
            && input.has_method("transfer_item")
        {
            let _ = input.call(
                "transfer_item",
                &[
                    self.tile.x.to_variant(),
                    self.tile.y.to_variant(),
                    GString::from(item).to_variant(),
                    amount.to_variant(),
                    false.to_variant(),
                ],
            );
        }
        self.base_mut().emit_signal(
            "take_requested",
            &[GString::from(item).to_variant(), amount.to_variant()],
        );
    }

    /// `UI.formatAmount`-style short count: `1.2k` / `3mil` (`unit.thousands` /
    /// `unit.millions` bundle values).
    fn format_amount(&self, amount: f64) -> GString {
        match amount_scale(amount) {
            Some((divisor, key, fallback)) => {
                let count = (amount / divisor) as i64;
                let suffix = bundle_or(key, fallback);
                GString::from(format!("{count}{suffix}").as_str())
            }
            None => GString::from((amount as i64).to_string().as_str()),
        }
    }
}

/// The `_round` threshold for `amount`: `(divisor, bundle key, fallback)`.
fn amount_scale(amount: f64) -> Option<(f64, &'static str, &'static str)> {
    if amount >= 1_000_000.0 {
        Some((1_000_000.0, "unit.millions", "mil"))
    } else if amount >= 1_000.0 {
        Some((1_000.0, "unit.thousands", "k"))
    } else {
        None
    }
}

/// `_bundle(key, fallback)`: the bundle value, or `fallback` when the key is
/// missing (or unresolved).
fn bundle_or(key: &str, fallback: &str) -> GString {
    let Some(assets) = kit::assets() else {
        return GString::from(fallback);
    };
    let value = assets.bind().bundle_get(GString::from(key));
    if value.is_empty() || value == key {
        return GString::from(fallback);
    }
    value
}

/// `{item_name: amount}` read-model rows as `(name, amount)` pairs.
fn item_entries(items: &VarDictionary) -> Vec<(String, i64)> {
    items
        .iter_shared()
        .map(|(key, value)| {
            (
                key.try_to_relaxed::<GString>()
                    .unwrap_or_default()
                    .to_string(),
                value.try_to_relaxed::<i64>().unwrap_or(0),
            )
        })
        .collect()
}

/// Reads a `bool` from a read model with GDScript's `bool(...)` coercion.
fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<bool>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_thresholds_match_the_fragment() {
        assert_eq!(amount_scale(999.0), None);
        assert_eq!(
            amount_scale(1_000.0),
            Some((1_000.0, "unit.thousands", "k"))
        );
        assert_eq!(
            amount_scale(1_000_000.0),
            Some((1_000_000.0, "unit.millions", "mil"))
        );
        // `int(amount / divisor)` truncates like GDScript.
        assert_eq!((1_999.0f64 / 1_000.0) as i64, 1);
        assert_eq!((2_500_000.0f64 / 1_000_000.0) as i64, 2);
    }

    #[test]
    fn timings_match_the_fragment() {
        assert!((HOLD_WITHDRAW - 20.0).abs() < f64::EPSILON);
        assert!((HOLD_SHRINK - 120.0).abs() < f64::EPSILON);
        assert!((REFRESH_INTERVAL - 0.1).abs() < f64::EPSILON);
        assert_eq!(COLUMNS, 3);
        assert_eq!(CELL, 40.0);
    }
}
