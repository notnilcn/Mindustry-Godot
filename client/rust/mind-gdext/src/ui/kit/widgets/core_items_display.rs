// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/CoreItemsDisplay.java (plan 14 §3.2 scope).
//
//! Core item table: per item an `iconSmall` image and a `UI.formatAmount`
//! label (`minWidth(52)`, `padRight(3)` each), four items per row, in content
//! order. The cells are rebuilt only when the shown item set changes; the
//! amounts refresh every update (Java binds the label to the live core).
//!
//! Deviation: Java keeps every item ever seen (`usedItems`) until `resetUsed`
//! on `ResetEvent`; the port has no reset hook here, so it shows the items the
//! core currently holds (`core_items_json` omits zero amounts).

use godot::builtin::{VarDictionary, VariantType};
use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{GridContainer, IGridContainer, RichTextLabel, TextureRect};
use godot::obj::NewAlloc;
use godot::prelude::*;
use std::collections::HashMap;

use crate::ui::kit;

/// Four items per row, two cells (image + label) per item (`++i % 4 == 0`).
const ITEMS_PER_ROW: i32 = 4;
/// `Vars.iconSmall` (`8 * 3`).
const ICON_SIZE: f32 = 24.0;
/// `label(...).minWidth(52f)`.
const LABEL_MIN_WIDTH: f32 = 52.0;
/// `padRight(3)` on both cells.
const PAD_RIGHT: i32 = 3;

/// `CoreItemsDisplay` — the core inventory table (GridContainer).
#[derive(GodotClass)]
#[class(base=GridContainer, init)]
pub struct CoreItemsDisplay {
    base: Base<GridContainer>,
    /// Shown item names in content order (the `core_items_json` key order).
    order: Vec<String>,
    /// Item name -> amount label.
    labels: HashMap<String, Gd<RichTextLabel>>,
}

#[godot_api]
impl IGridContainer for CoreItemsDisplay {
    fn ready(&mut self) {
        self.base_mut().set_columns(ITEMS_PER_ROW * 2);
        self.base_mut()
            .add_theme_constant_override("h_separation", PAD_RIGHT);
        self.base_mut()
            .add_theme_constant_override("v_separation", 0);
    }
}

#[godot_api]
impl CoreItemsDisplay {
    /// Whether any item is shown (`usedItems.size > 0` gates the background).
    #[func]
    pub fn has_items(&self) -> bool {
        !self.order.is_empty()
    }

    /// Updates the table for `items` (item name -> amount, content order).
    #[func]
    pub fn update_items(&mut self, items: VarDictionary) {
        let mut names: Vec<String> = Vec::new();
        for key in items.keys_array().iter_shared() {
            names.push(variant_text(&key));
        }
        if names != self.order {
            self.order = names;
            self.rebuild();
        }
        let order = self.order.clone();
        for item_name in order {
            let Some(mut label) = self.labels.get(&item_name).cloned() else {
                continue;
            };
            let amount = items
                .get(item_name.as_str())
                .map(|value| variant_i64(&value))
                .unwrap_or(0);
            label.set_text(&self.format_amount(amount));
        }
    }

    /// Alias for [`Self::update_items`] (the `set_items` call name).
    #[func]
    pub fn set_items(&mut self, items: VarDictionary) {
        self.update_items(items);
    }

    /// Alias for [`Self::update_items`] (the `refresh` call name).
    #[func]
    pub fn refresh(&mut self, items: VarDictionary) {
        self.update_items(items);
    }

    fn rebuild(&mut self) {
        self.labels.clear();
        for child in self.base().get_children().iter_shared() {
            let mut child = child;
            self.base_mut().remove_child(&child);
            child.queue_free();
        }
        let order = self.order.clone();
        for item_name in order {
            // code-instantiated: one image + label pair per shown item; the
            // count is driven by the live core inventory.
            let mut icon = TextureRect::new_alloc();
            if let Some(texture) = kit::icon_texture(&format!("item-{item_name}-ui")) {
                icon.set_texture(&texture);
            }
            icon.set_custom_minimum_size(Vector2::new(ICON_SIZE, ICON_SIZE));
            icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            let tooltip = self.bundle(&format!("item.{item_name}.name"), &item_name);
            icon.set_tooltip_text(&tooltip);
            self.base_mut().add_child(&icon);
            // code-instantiated: amount label bound to the live core inventory.
            let mut amount = RichTextLabel::new_alloc();
            amount.set_use_bbcode(true);
            amount.set_fit_content(true);
            amount.set_scroll_active(false);
            amount.set_autowrap_mode(AutowrapMode::OFF);
            amount.set_custom_minimum_size(Vector2::new(LABEL_MIN_WIDTH, 0.0));
            amount.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            amount.set_mouse_filter(MouseFilter::IGNORE);
            amount.add_theme_color_override("default_color", Color::WHITE);
            self.base_mut().add_child(&amount);
            self.labels.insert(item_name, amount);
        }
    }

    /// `UI.formatAmount(long)` (`UI.java:716-734`) with `[gray]` unit suffixes
    /// (`unit.billions`/`unit.millions`/`unit.thousands`).
    fn format_amount(&self, number: i64) -> String {
        let magnitude = number.unsigned_abs();
        let prefix = if number < 0 { "-" } else { "" };
        if magnitude >= 1_000_000_000 {
            return format!(
                "{prefix}{:.1}{}",
                magnitude as f64 / 1_000_000_000.0,
                self.unit("unit.billions", "B")
            );
        }
        if magnitude >= 1_000_000 {
            return format!(
                "{prefix}{:.1}{}",
                magnitude as f64 / 1_000_000.0,
                self.unit("unit.millions", "M")
            );
        }
        if magnitude >= 10_000 {
            // Java `number / 1000` (integer division, sign kept by the quotient).
            let thousands = (number as f64 / 1000.0) as i64;
            return format!("{thousands}{}", self.unit("unit.thousands", "k"));
        }
        if magnitude >= 1000 {
            return format!(
                "{prefix}{:.1}{}",
                magnitude as f64 / 1000.0,
                self.unit("unit.thousands", "k")
            );
        }
        number.to_string()
    }

    fn unit(&self, key: &str, fallback: &str) -> String {
        format!("[color=#7f7f7f]{}[/color]", self.bundle(key, fallback))
    }

    /// `MindAssets.bundle_get` with the GDScript fallback rule: an absent or
    /// empty entry (`value == key`) resolves to `fallback`.
    fn bundle(&self, key: &str, fallback: &str) -> String {
        let value = kit::t(key).to_string();
        if value.is_empty() || value == key {
            fallback.to_string()
        } else {
            value
        }
    }
}

/// GDScript `str(value)` for dictionary keys (item names).
fn variant_text(value: &Variant) -> String {
    match value.get_type() {
        VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::STRING_NAME => value
            .try_to::<StringName>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        _ => format!("{value}"),
    }
}

/// GDScript `int(value)`: JSON numbers (`float`) and numeric strings convert.
fn variant_i64(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        VariantType::BOOL => {
            if value.try_to::<bool>().unwrap_or(false) {
                1
            } else {
                0
            }
        }
        VariantType::STRING | VariantType::STRING_NAME => {
            variant_text(value).trim().parse().unwrap_or_default()
        }
        _ => 0,
    }
}
