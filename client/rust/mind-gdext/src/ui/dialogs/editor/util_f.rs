// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the old `batch_f` for the editor dialogs.
//! Upstream: none (port-only; shared helpers of the old batch_f)

use crate::ui::dialog::MindDialog;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel};
use godot::builtin::Side;
use godot::classes::{Button, Json, RichTextLabel, StyleBoxFlat};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// A queued one-shot action (`Callable.call_deferred` payload).
pub(in crate::ui::dialogs) type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase (see the module docs).
pub(in crate::ui::dialogs) fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
        Variant::nil()
    });
    callable.call_deferred(&[]);
}

/// Calls a `MindUi` facade method (`None` when the facade/method is absent).
pub(in crate::ui::dialogs) fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Calls a `MindEditor` facade method (`None` when the facade/method is absent).
pub(in crate::ui::dialogs) fn editor_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut editor = kit::editor()?;
    if !editor.has_method(method) {
        return None;
    }
    Some(editor.call(method, args))
}

/// `Vars.mobile`-independent portrait test (`Core.graphics.isPortrait()`).
pub(in crate::ui::dialogs) fn is_portrait(dialog: &MindDialog) -> bool {
    let size = dialog.base().get_viewport_rect().size;
    size.y > size.x
}

/// The viewport width (`get_viewport_rect().size.x`).
pub(in crate::ui::dialogs) fn viewport_width(dialog: &MindDialog) -> f32 {
    dialog.base().get_viewport_rect().size.x
}

/// A rich label's engine handle (text/color/alignment/autowrap).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub(in crate::ui::dialogs) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// `MindWidgets.markup(text)` for a raw markup string.
pub(in crate::ui::dialogs) fn markup(raw: &str) -> String {
    kit::markup_text(raw).to_string()
}

/// `UiRoot.dialog(name)` on the `UiRoot` registry (falls back to the tree).
pub(in crate::ui::dialogs) fn dialog_node(name: &str) -> Option<Gd<Node>> {
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("dialog")
        && let Ok(node) = root
            .call("dialog", &[GString::from(name).to_variant()])
            .try_to::<Gd<Node>>()
    {
        return Some(node);
    }
    kit::tree_root()?.find_child(name)
}

/// Connects a `MindUi` prompt signal once.
pub(in crate::ui::dialogs) fn connect_prompt_once(
    dialog: &mut MindDialog,
    key: &str,
    signal: &str,
    callable: &Callable,
) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    let meta = format!("_batch_f_{key}");
    if dialog.base().has_meta(&meta) || !ui.has_signal(signal) {
        return;
    }
    let _ = ui.connect(signal, callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// Re-runs the active controller's `shown` hook on a live dialog handle.
pub(in crate::ui::dialogs) fn dialog_refresh(dialog: &Gd<MindDialog>) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    node.bind_mut().refresh_controller();
}

/// Removes and frees every child of `node`.
pub(in crate::ui::dialogs) fn clear_children<T: Inherits<Node>>(node: &mut Gd<T>) {
    let mut parent: Gd<Node> = node.clone().upcast();
    for child in parent.get_children().iter_shared() {
        let mut child = child.clone();
        parent.remove_child(&child);
        child.queue_free();
    }
}

/// `str(value)` for one variant (empty when unconvertible).
pub(in crate::ui::dialogs) fn variant_string(value: &Variant) -> String {
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string();
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return flag.to_string();
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number.to_string();
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number.to_string();
    }
    String::new()
}

/// `int(value)` for one variant (JSON integers and floats both convert).
pub(in crate::ui::dialogs) fn variant_i64(value: &Variant) -> i64 {
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return i64::from(flag);
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0);
    }
    0
}

/// `float(value)` for one variant.
pub(in crate::ui::dialogs) fn variant_f64(value: &Variant) -> f64 {
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number as f64;
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0.0);
    }
    0.0
}

/// `bool(value)` for one variant.
pub(in crate::ui::dialogs) fn variant_bool(value: &Variant) -> bool {
    if let Ok(flag) = value.try_to::<bool>() {
        return flag;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number != 0;
    }
    if let Ok(text) = value.try_to::<GString>() {
        let text = text.to_string();
        return text == "true" || text == "1";
    }
    false
}

/// A dictionary value coerced to text (`str(dict.get(key, ""))`).
pub(in crate::ui::dialogs) fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_default()
}

/// A dictionary value coerced to an integer.
pub(in crate::ui::dialogs) fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to a float.
pub(in crate::ui::dialogs) fn dict_f64(dict: &VarDictionary, key: &str, default: f64) -> f64 {
    dict.get(key)
        .map(|value| variant_f64(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to a boolean.
pub(in crate::ui::dialogs) fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// `JSON.parse_string(text)` as an array (`[]` on any other shape).
pub(in crate::ui::dialogs) fn parse_json_array(text: &str) -> VarArray {
    if text.is_empty() {
        return VarArray::new();
    }
    Json::parse_string(text)
        .try_to::<VarArray>()
        .unwrap_or_default()
}

/// `JSON.parse_string(text)` as a dictionary (`{}` on any other shape).
pub(in crate::ui::dialogs) fn parse_json_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `JSON.stringify(value)` for one variant.
pub(in crate::ui::dialogs) fn stringify(value: &Variant) -> String {
    Json::stringify(value).to_string()
}

/// An array of strings as a Godot array (`Array[String]` writes).
pub(in crate::ui::dialogs) fn string_array_variant(values: &[String]) -> Variant {
    let mut array = VarArray::new();
    for value in values {
        array.push(&GString::from(value.as_str()).to_variant());
    }
    array.to_variant()
}

/// `String.is_valid_int()`: an optional sign followed by digits only.
pub(in crate::ui::dialogs) fn is_valid_int(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty() && body.chars().all(|character| character.is_ascii_digit())
}

/// A flat `StyleBoxFlat` with a solid fill (`MindThemeBuilder.flat`).
pub(in crate::ui::dialogs) fn flat_box(fill: Color) -> Gd<StyleBoxFlat> {
    // code-instantiated: the theme builder's `flat` helper is private; the
    // gray data cards need the same look at runtime.
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(fill);
    box_.set_corner_radius_all(3);
    box_.set_content_margin(Side::LEFT, 8.0);
    box_.set_content_margin(Side::RIGHT, 8.0);
    box_.set_content_margin(Side::TOP, 4.0);
    box_.set_content_margin(Side::BOTTOM, 4.0);
    box_
}

/// A flat 50x50 `graySquarei` action cell (`_square`).
pub(in crate::ui::dialogs) fn square_cell(
    icon_name: &str,
    tooltip: &str,
    handler: Callable,
) -> Gd<Button> {
    // code-instantiated: per-row action cells are data-driven.
    let mut button = factory::icon_button(
        icon_name,
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    button.set_tooltip_text(tooltip);
    button.connect("pressed", &handler);
    button
}

/// Shows a transient error toast (`ui.toast(text, "error")`).
pub(in crate::ui::dialogs) fn toast_error(text: &str) {
    let _ = ui_call(
        "toast",
        &[
            GString::from(text).to_variant(),
            GString::from("error").to_variant(),
        ],
    );
}

/// `_notify(key)`: the bundle lookup as an error toast.
pub(in crate::ui::dialogs) fn notify_key(key: &str) {
    toast_error(&kit::t(key).to_string());
}

/// `_show_text(title, body)`: the shared text prompt with a toast fallback.
pub(in crate::ui::dialogs) fn show_text(title: &str, body: &str) {
    if ui_call(
        "show_text",
        &[
            GString::from(title).to_variant(),
            GString::from(body).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    show_toast(body);
}

/// `MindDialog.show_toast`: a transient info prompt through `MindUi`.
pub(in crate::ui::dialogs) fn show_toast(text: &str) {
    if text.is_empty() {
        return;
    }
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `false` under a plain `cargo test`: the GDExtension entry point never
    /// runs, so Godot builtins (`VarDictionary`/`VarArray`/`Json`) and the
    /// asset-host lookup behind `kit::markup_text` panic with "Godot engine
    /// not available" (PAR-0586). Engine-backed assertions in this module are
    /// skipped there; the in-engine `dialog_sweep_check` gate builds and shows
    /// the real dialogs, so the production path stays covered.
    pub(crate) fn engine_available() -> bool {
        godot::sys::is_godot_initialized()
    }
    #[test]
    fn integer_validation_matches_godot() {
        assert!(is_valid_int("123"));
        assert!(is_valid_int("-42"));
        assert!(!is_valid_int(""));
        assert!(!is_valid_int("-"));
        assert!(!is_valid_int("1.5"));
        assert!(!is_valid_int("x1"));
    }
}
