// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/ItemsDisplay.java (plan 14 §3.2 scope).
//
//! Launched-item row: icon + amount chips rebuilt when the item set changes.
//!
//! The host pushes the live item counts (`{item name: amount}`) through
//! [`ItemsDisplay::update_items`]; the chips are rebuilt only when the sorted
//! `name=amount` signature changes.

use godot::builtin::{VarDictionary, VariantType};
use godot::classes::texture_rect::StretchMode;
use godot::classes::{HBoxContainer, Label, TextureRect};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::kit;

/// `ItemsDisplay` — the launched-items chip row (`ItemsDisplay.java`).
#[derive(GodotClass)]
#[class(rename = RustItemsDisplay, base=HBoxContainer, init)]
pub struct ItemsDisplay {
    base: Base<HBoxContainer>,
    /// `_amounts`: the `"<name>=<amount>"` signature of the last rebuild.
    amounts: Vec<String>,
}

#[godot_api]
impl ItemsDisplay {
    /// Rebuilds the chips for `items` (item name -> amount).
    #[func]
    pub fn update_items(&mut self, items: VarDictionary) {
        let mut keys: Vec<String> = Vec::new();
        for key in items.keys_array().iter_shared() {
            keys.push(variant_text(&key));
        }
        keys.sort();
        let mut signature: Vec<String> = Vec::with_capacity(keys.len());
        for key in &keys {
            let value = items
                .get(key.as_str())
                .map(|value| variant_text(&value))
                .unwrap_or_default();
            signature.push(format!("{key}={value}"));
        }
        if signature == self.amounts {
            return;
        }
        self.amounts = signature;
        self.rebuild(&items, &keys);
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

    fn rebuild(&mut self, items: &VarDictionary, keys: &[String]) {
        for child in self.base().get_children().iter_shared() {
            let mut child = child;
            child.queue_free();
        }
        for key in keys {
            // code-instantiated: one chip per used item; count is data-driven.
            let mut chip = HBoxContainer::new_alloc();
            let mut icon = TextureRect::new_alloc();
            if let Some(texture) = kit::icon_texture(&format!("item-{key}-ui")) {
                icon.set_texture(&texture);
            }
            icon.set_custom_minimum_size(Vector2::new(24.0, 24.0));
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            chip.add_child(&icon);
            let amount = items
                .get(key.as_str())
                .map(|value| variant_i64(&value))
                .unwrap_or(0);
            let mut amount_label = Label::new_alloc();
            amount_label.set_text(&amount.to_string());
            chip.add_child(&amount_label);
            self.base_mut().add_child(&chip);
        }
    }
}

/// GDScript `str(value)` for the chip amounts (JSON numbers arrive as floats).
fn variant_text(value: &Variant) -> String {
    match value.get_type() {
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::BOOL => value
            .try_to::<bool>()
            .map(|flag| flag.to_string())
            .unwrap_or_default(),
        VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::STRING_NAME => value
            .try_to::<StringName>()
            .map(|text| text.to_string())
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
