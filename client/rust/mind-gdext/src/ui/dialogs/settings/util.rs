// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared request/meta, variant/dict/JSON and page helpers of the old batch_a.
//! Upstream: none (port-only; shared helpers of the old batch_a)

use godot::classes::{Json, StyleBoxFlat};
use godot::obj::NewGd;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;

/// Meta key carrying the request `shown` dispatches on.
pub(in crate::ui::dialogs) const REQUEST_META: &str = "mind_dialog_request";
/// Meta key carrying the `confirm_result` answer until the deferred `shown`.
pub(in crate::ui::dialogs) const CONFIRM_META: &str = "mind_dialog_confirm";

/// Request re-running `shown` synchronously with `request` recorded.
pub(in crate::ui::dialogs) fn request_refresh(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call("refresh_controller", &[]);
}

/// Deferred [`request_refresh`] for callbacks that run inside a `MindUi` borrow.
pub(in crate::ui::dialogs) fn request_refresh_deferred(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call_deferred("refresh_controller", &[]);
}

/// Reads and clears the pending request (`""` when none is queued).
pub(in crate::ui::dialogs) fn take_request(dialog: &mut MindDialog) -> String {
    let mut node = dialog.to_gd();
    if !node.has_meta(REQUEST_META) {
        return String::new();
    }
    let request = variant_text(&node.get_meta(REQUEST_META));
    node.remove_meta(REQUEST_META);
    request
}

pub(in crate::ui::dialogs) fn meta_text(node: &Gd<MindDialog>, key: &str) -> String {
    if !node.has_meta(key) {
        return String::new();
    }
    variant_text(&node.get_meta(key))
}

pub(in crate::ui::dialogs) fn set_meta_text(node: &mut Gd<MindDialog>, key: &str, value: &str) {
    node.set_meta(key, &GString::from(value).to_variant());
}

pub(in crate::ui::dialogs) fn meta_int(node: &Gd<MindDialog>, key: &str, fallback: i64) -> i64 {
    if !node.has_meta(key) {
        return fallback;
    }
    as_int(Some(node.get_meta(key)))
}

pub(in crate::ui::dialogs) fn set_meta_int(node: &mut Gd<MindDialog>, key: &str, value: i64) {
    node.set_meta(key, &value.to_variant());
}

pub(in crate::ui::dialogs) fn meta_bool(node: &Gd<MindDialog>, key: &str) -> bool {
    node.has_meta(key) && as_bool(Some(node.get_meta(key)))
}

/// GDScript `str(value)` for a variant (`<null>` for null, like Godot).
pub(in crate::ui::dialogs) fn variant_text(value: &Variant) -> String {
    value
        .try_to::<GString>()
        .map(|text| text.to_string())
        .unwrap_or_else(|_| value.stringify().to_string())
}

/// GDScript `int(value)` for a variant.
pub(in crate::ui::dialogs) fn as_int(value: Option<Variant>) -> i64 {
    let Some(value) = value else {
        return 0;
    };
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return i64::from(flag);
    }
    0
}

/// GDScript `float(value)` for a variant.
pub(in crate::ui::dialogs) fn as_float(value: Option<Variant>) -> f64 {
    let Some(value) = value else {
        return 0.0;
    };
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number as f64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return if flag { 1.0 } else { 0.0 };
    }
    0.0
}

/// GDScript `bool(value)` for a variant.
pub(in crate::ui::dialogs) fn as_bool(value: Option<Variant>) -> bool {
    let Some(value) = value else {
        return false;
    };
    value.try_to::<bool>().unwrap_or(false)
}

pub(in crate::ui::dialogs) fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

pub(in crate::ui::dialogs) fn dict_int(dict: &VarDictionary, key: &str) -> i64 {
    as_int(dict.get(key))
}

pub(in crate::ui::dialogs) fn dict_float(dict: &VarDictionary, key: &str, fallback: f32) -> f32 {
    match dict.get(key) {
        Some(value) => as_float(Some(value)) as f32,
        None => fallback,
    }
}

pub(in crate::ui::dialogs) fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    as_bool(dict.get(key))
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

/// `JSON.parse_string` of a UI JSON payload (`[]` when it is not an array).
pub(in crate::ui::dialogs) fn json_array(text: &GString) -> VarArray {
    if text.is_empty() {
        return VarArray::new();
    }
    Json::parse_string(text)
        .try_to::<VarArray>()
        .unwrap_or_default()
}

/// `JSON.parse_string` of a UI JSON payload (`{}` when it is not an object).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub(in crate::ui::dialogs) fn json_object(text: &GString) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `MindUi.settings_categories_json` rows.
pub(in crate::ui::dialogs) fn load_categories() -> Vec<VarDictionary> {
    let Some(mut ui) = kit::ui() else {
        return Vec::new();
    };
    if !ui.has_method("settings_categories_json") {
        return Vec::new();
    }
    let text = ui.call("settings_categories_json", &[]).to::<GString>();
    json_array(&text)
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `MindUi.settings_rows_json(category)` rows.
pub(in crate::ui::dialogs) fn load_rows(category_id: &str) -> Vec<VarDictionary> {
    let Some(mut ui) = kit::ui() else {
        return Vec::new();
    };
    if !ui.has_method("settings_rows_json") {
        return Vec::new();
    }
    let text = ui
        .call(
            "settings_rows_json",
            &[GString::from(category_id).to_variant()],
        )
        .to::<GString>();
    json_array(&text)
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
