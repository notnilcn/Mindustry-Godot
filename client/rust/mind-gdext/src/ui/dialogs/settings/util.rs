// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared request/meta, variant/dict/JSON and page helpers of the old batch_a.
//! Upstream: none (port-only; shared helpers of the old batch_a)

use godot::classes::{Json, StyleBoxFlat};
use godot::obj::NewGd;
use godot::prelude::*;

use crate::gdutil::variant::{
    parse_array, to_bool_strict, to_f64_boolish, to_i64_no_text, to_text_stringify,
};
use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::common::facade::ui_call;
use crate::ui::kit;

/// Meta key carrying the `confirm_result` answer until the deferred `shown`.
pub(in crate::ui::dialogs) const CONFIRM_META: &str = "mind_dialog_confirm";

pub(in crate::ui::dialogs) fn meta_text(node: &Gd<MindDialog>, key: &str) -> String {
    if !node.has_meta(key) {
        return String::new();
    }
    to_text_stringify(&node.get_meta(key))
}

pub(in crate::ui::dialogs) fn set_meta_text(node: &mut Gd<MindDialog>, key: &str, value: &str) {
    node.set_meta(key, &GString::from(value).to_variant());
}

pub(in crate::ui::dialogs) fn meta_int(node: &Gd<MindDialog>, key: &str, fallback: i64) -> i64 {
    if !node.has_meta(key) {
        return fallback;
    }
    to_i64_no_text(Some(node.get_meta(key)))
}

pub(in crate::ui::dialogs) fn set_meta_int(node: &mut Gd<MindDialog>, key: &str, value: i64) {
    node.set_meta(key, &value.to_variant());
}

pub(in crate::ui::dialogs) fn meta_bool(node: &Gd<MindDialog>, key: &str) -> bool {
    node.has_meta(key) && to_bool_strict(Some(node.get_meta(key)))
}

/// `Dictionary.get(key, "")` with GDScript `str()` coercion.
///
/// Unlike [`crate::gdutil::variant::dict_text_or`] this keeps the stringify
/// fallback (a JSON null reads `<null>`, like Godot), so it stays a local copy.
pub(in crate::ui::dialogs) fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| to_text_stringify(&value))
        .unwrap_or_default()
}

/// `Dictionary.get(key, 0)` with GDScript `int()` (no string parse).
pub(in crate::ui::dialogs) fn dict_int(dict: &VarDictionary, key: &str) -> i64 {
    to_i64_no_text(dict.get(key))
}

/// `Dictionary.get(key, fallback)` with GDScript `float()` (no string parse).
pub(in crate::ui::dialogs) fn dict_float(dict: &VarDictionary, key: &str, fallback: f32) -> f32 {
    match dict.get(key) {
        Some(value) => to_f64_boolish(Some(value)) as f32,
        None => fallback,
    }
}

pub(in crate::ui::dialogs) fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    match dict.get(key) {
        Some(value) => value.try_to::<VarArray>().unwrap_or_default(),
        None => VarArray::new(),
    }
}

#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub(in crate::ui::dialogs) fn dict_dict(dict: &VarDictionary, key: &str) -> VarDictionary {
    match dict.get(key) {
        Some(value) => value.try_to::<VarDictionary>().unwrap_or_default(),
        None => VarDictionary::new(),
    }
}

/// `MindUi.settings_categories_json` rows.
pub(in crate::ui::dialogs) fn load_categories() -> Vec<VarDictionary> {
    let Some(text) = ui_call("settings_categories_json", &[]) else {
        return Vec::new();
    };
    parse_array(&text.to::<GString>().to_string())
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `MindUi.settings_rows_json(category)` rows.
pub(in crate::ui::dialogs) fn load_rows(category_id: &str) -> Vec<VarDictionary> {
    let Some(text) = ui_call(
        "settings_rows_json",
        &[GString::from(category_id).to_variant()],
    ) else {
        return Vec::new();
    };
    parse_array(&text.to::<GString>().to_string())
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `Bundle.get(key)` with the raw-key fallback of the GDScript `_bundle_or`.
pub(in crate::ui::dialogs) fn bundle_or(key: &str, fallback: &str) -> String {
    let value = kit::t(key).to_string();
    if value == key || value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

/// `ContentInfoDialog.show(content)`: opens the shared `content` dialog for a
/// content name. Java's dialog takes the content alone (there is no caller
/// dialog argument), so every content link routes through this one helper.
pub fn open_content_info(content: &str) {
    if content.is_empty() {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("content", &GString::from(content).to_variant());
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("content").to_variant(),
                Json::stringify(&context.to_variant()).to_variant(),
            ],
        );
    }
}

/// The dialog's viewport width (`get_viewport_rect().size.x`).
pub(in crate::ui::dialogs) fn viewport_width(dialog: &MindDialog) -> f32 {
    dialog.base().get_viewport_rect().size.x
}

/// A flat `StyleBoxFlat` with a solid fill (`_flat_box`).
pub(in crate::ui::dialogs) fn flat_box(fill: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(fill);
    box_
}
