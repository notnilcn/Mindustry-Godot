// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the old `batch_g` for the mods dialogs. Filled by S10 (docs/PLANS).
//! Upstream: none (port-only; shared helpers of the old batch_g)

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::widgets::factory;
use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{Button, Control, HBoxContainer, Node, StyleBoxFlat};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
#[cfg(test)]
use mind_core::util::strip_colors;
use std::{cell::RefCell, rc::Rc};

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// A queued one-shot action (`Callable.call_deferred` payload).
pub(in crate::ui::dialogs) type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase.
///
/// Every controller in this batch keeps its state in an `Rc<RefCell<..>>` that
/// its own buttons and lasting signal handlers share. A `MindMods`/`MindUi`
/// call made while that state is borrowed can re-enter a handler synchronously
/// (`mods_changed`, `mod_import_progress`, `confirm_result`), so the handlers
/// queue their work here instead of borrowing recursively.
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

/// Calls one `MindUi` method (`None` when the facade or method is absent).
pub(in crate::ui::dialogs) fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Opens a registered dialog through `MindUi`.
pub(in crate::ui::dialogs) fn open_dialog(name: &str, context_json: &str) {
    let _ = ui_call(
        "open_dialog",
        &[
            GString::from(name).to_variant(),
            GString::from(context_json).to_variant(),
        ],
    );
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

/// The `MindMods` autoload.
pub(in crate::ui::dialogs) fn mods_node() -> Option<Gd<Node>> {
    kit::node("/root/MindMods")
}

/// Re-runs a dialog controller after the current callback unwinds
/// (`set_updates`/`install_repos` replacement; see the module docs).
pub(in crate::ui::dialogs) fn refresh_dialog_deferred(node: &Gd<Node>) {
    let node = node.clone();
    defer_action("batch_g_refresh_dialog", move || {
        if !node.is_instance_valid() {
            return;
        }
        let mut node = node;
        let _ = node.call("refresh_controller", &[]);
    });
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

/// `int(value)` for one variant.
pub(in crate::ui::dialogs) fn variant_i64(value: &Variant) -> i64 {
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0);
    }
    0
}

/// `bool(value)` for one variant.
pub(in crate::ui::dialogs) fn variant_bool(value: &Variant) -> bool {
    if let Ok(flag) = value.try_to::<bool>() {
        return flag;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number != 0;
    }
    false
}

/// A dictionary value coerced to text (`str(dict.get(key, ""))`).
pub(in crate::ui::dialogs) fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_default()
}

/// A dictionary value coerced to a boolean.
pub(in crate::ui::dialogs) fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to an integer.
pub(in crate::ui::dialogs) fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// `PackedStringArray(value)` for a JSON array (`[]` on any other shape).
pub(in crate::ui::dialogs) fn string_list(value: Option<&Variant>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .map(|item| variant_string(&item))
        .collect()
}

/// `Core.bundle.get(key)` as plain text.
pub(in crate::ui::dialogs) fn bt(key: &str) -> String {
    kit::t(key).to_string()
}

/// `MindWidgets.markup(key)`: the bundle string translated to BBCode.
pub(in crate::ui::dialogs) fn bm(key: &str) -> String {
    kit::tm(key).to_string()
}

/// `Bundle.format` with `{0}`-style substitution (`_fmt`).
pub(in crate::ui::dialogs) fn fmt_key(key: &str, args: &[&str]) -> String {
    let mut result = bt(key);
    for (index, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{index}}}"), arg);
    }
    result
}

/// A solid `StyleBoxFlat` (`_flat`).
pub(in crate::ui::dialogs) fn flat_style(color: Color) -> Gd<StyleBoxFlat> {
    // code-instantiated: the gray/flat card looks are built per data row.
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(color);
    box_
}

/// Local copy of `MindDialog::build_button_content` (the shell helper is a
/// `&mut self` method; the data-row rebuild paths hold no dialog borrow).
pub(in crate::ui::dialogs) fn build_button_content(
    button: &mut Gd<Button>,
    icon: Option<Gd<Control>>,
    text: &str,
) {
    // code-instantiated: button content rows are data-driven per dialog.
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 13.0);
    row.set_offset(Side::RIGHT, -8.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    if let Some(icon) = icon {
        row.add_child(&icon);
    }
    let mut label = factory::label(text);
    label.set_autowrap_mode(AutowrapMode::OFF);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&label);
}

/// Local copy of `MindDialog::apply_pane_button` (`Styles.flatTogglet`).
pub(in crate::ui::dialogs) fn apply_pane_button(
    button: &mut Gd<Button>,
    border: Color,
    bottom_border: f32,
) {
    let normal = MindDialog::pane_box(Color::BLACK, border, 4.0, 2.0, bottom_border);
    button.add_theme_stylebox_override("normal", &normal);
    button.add_theme_stylebox_override("hover", &normal);
    button.add_theme_stylebox_override("disabled", &normal);
    let pressed = MindDialog::pane_box(Color::BLACK, border, 4.0, 2.0, -1.0);
    button.add_theme_stylebox_override("pressed", &pressed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_colors_drops_arc_tags() {
        // `ModsDialog.java:256` / the state-details tooltip use plain
        // `Strings.stripColors`: only registered names and 1..=8 hex digits.
        assert_eq!(strip_colors("[accent]Copper[lightgray] ore"), "Copper ore");
        assert_eq!(strip_colors("[#ff0000]red[]"), "red");
        assert_eq!(strip_colors("[LIGHT_GRAY]grey[#f]x"), "greyx");
        assert_eq!(strip_colors("no [tags] here"), "no [tags] here");
        assert_eq!(strip_colors("keep [1] digits"), "keep [1] digits");
        assert_eq!(strip_colors("[NotATag]"), "[NotATag]");
    }
}
