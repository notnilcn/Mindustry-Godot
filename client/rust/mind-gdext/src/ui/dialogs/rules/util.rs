// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the old `batch_i` for the rules dialogs.
//! Upstream: none (port-only; shared helpers of the old batch_i)

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use godot::builtin::VarDictionary;
use godot::classes::{Json, LineEdit, Node};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

// Coercions and the facade/deferred helpers are shared (adopted from F1/F2);
// the old `variant_*`/`rich` names stay as re-exports because the rules
// children and `editor/editor_maps.rs` (H05c) import them through
// `rules::util::*`.
pub(in crate::ui::dialogs) use crate::gdutil::variant::{
    to_bool_typed as variant_bool, to_f64_typed as variant_f64, to_i64_typed as variant_i64,
    to_text_typed as variant_text,
};
pub(in crate::ui::dialogs) use crate::ui::dialogs::common::defer::defer_action;
pub(in crate::ui::dialogs) use crate::ui::dialogs::common::facade::{rich, ui_call};

/// One dictionary value as text (`str(dict.get(key, ""))`).
pub(in crate::ui::dialogs) fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

/// One dictionary value as an integer.
pub(in crate::ui::dialogs) fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// `JSON.stringify(dictionary)`.
pub(in crate::ui::dialogs) fn json_stringify(dict: &VarDictionary) -> String {
    Json::stringify(&dict.to_variant()).to_string()
}

/// `dict.get(key, default)` on a captured dictionary.
pub(in crate::ui::dialogs) fn dict_at(
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    default: bool,
) -> bool {
    dict.borrow()
        .get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// `dict.get(key)` on a captured dictionary, coerced to a float.
pub(in crate::ui::dialogs) fn dict_at_f64(
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    default: f64,
) -> f64 {
    dict.borrow()
        .get(key)
        .map(|value| variant_f64(&value))
        .unwrap_or(default)
}

/// `dict.get(key)` on a captured dictionary, coerced to an integer.
pub(in crate::ui::dialogs) fn dict_at_i64(
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    default: i64,
) -> i64 {
    dict.borrow()
        .get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// `dict.get(key)` on a captured dictionary, coerced to text.
pub(in crate::ui::dialogs) fn dict_at_text(dict: &Rc<RefCell<VarDictionary>>, key: &str) -> String {
    dict.borrow()
        .get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

/// `dict[key] = value` on a captured dictionary.
pub(in crate::ui::dialogs) fn set_at(dict: &Rc<RefCell<VarDictionary>>, key: &str, value: Variant) {
    dict.borrow_mut().set(key, &value);
}

/// Calls a `MindCampaign` facade method (`None` when the facade/method is absent).
pub(in crate::ui::dialogs) fn campaign_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut campaign = kit::campaign()?;
    if !campaign.has_method(method) {
        return None;
    }
    Some(campaign.call(method, args))
}

/// The `UiRoot.dialog(name)` registry node (falls back to a tree search).
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

/// Connects a `MindUi` prompt signal once, keeping the rules batch's key.
pub(in crate::ui::dialogs) fn connect_prompt_once(
    dialog: &mut MindDialog,
    key: &str,
    signal: &str,
    callable: &Callable,
) {
    crate::ui::dialogs::common::prompt::connect_prompt_once(
        dialog, "_batch_i_", key, signal, callable,
    );
}

/// Runs `action` on a freshly bound dialog during the idle phase.
///
/// The custom-rules overlays are built *from signals* (the `@edit`/`@configure`/
/// `@banned*`/`@rules.weather` buttons, the search field's `text_changed`, and
/// the overlay buttons that rebuild the rules table), and every one of them
/// needs a mutable `MindDialog` (`open_*`/`setup_main`). A signal can be emitted
/// while that dialog is already bound — a programmatic press from a `shown()`/
/// `#[func]` handler, a field signal fired from a rebuild, or a press made from
/// a bound host method — and the nested `Gd::bind_mut()` then panics
/// `bind_mut() failed, already bound`; gdext catches it, the overlay never
/// opens, and (with the MCP debugger's break-on-error) Play's frame loop stays
/// wedged until restart (`PAR-0470`). Queue the bind instead: it lands after the
/// emitting call stack returns, exactly like the `editor_maps` prompt seams
/// (`PAR-0066`).
pub(in crate::ui::dialogs) fn defer_dialog_action(
    name: &'static str,
    target: &Gd<MindDialog>,
    action: impl FnOnce(&mut MindDialog) + 'static,
) {
    let target = target.clone();
    defer_action(name, move || {
        if !target.is_instance_valid() {
            return;
        }
        let mut node = target;
        let mut bound = node.bind_mut();
        action(&mut bound);
    });
}

/// `String.is_valid_int()`: an optional sign followed by digits only.
pub(in crate::ui::dialogs) fn is_valid_int(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty() && body.chars().all(|character| character.is_ascii_digit())
}

/// `String.is_valid_float()`.
pub(in crate::ui::dialogs) fn is_valid_float(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (body, None),
    };
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.chars().all(|value| value.is_ascii_digit()) {
            return false;
        }
    }
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };
    if integer.is_empty() && fraction.is_none() {
        return false;
    }
    if !integer.chars().all(|value| value.is_ascii_digit()) {
        return false;
    }
    match fraction {
        Some(fraction) => {
            if !fraction.chars().all(|value| value.is_ascii_digit()) {
                return false;
            }
            !(integer.is_empty() && fraction.is_empty())
        }
        None => true,
    }
}

/// `String.is_valid_identifier()`.
pub(in crate::ui::dialogs) fn is_valid_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    characters.all(|value| value.is_ascii_alphanumeric() || value == '_')
}

/// `Strings.canParsePositiveFloat` (`number`) / `Strings.parseInt` (`numberi`).
pub(in crate::ui::dialogs) fn parse_number(text: &str, integer: bool) -> bool {
    if text.is_empty() {
        return false;
    }
    if integer {
        return is_valid_int(text);
    }
    if !is_valid_float(text) {
        return false;
    }
    text.parse::<f64>()
        .map(|value| value >= 0.0)
        .unwrap_or(false)
}

/// `_number_text`: `String.num(value, 6)` with a forced `.0` for whole floats.
pub(in crate::ui::dialogs) fn number_text(value: f64, integer: bool) -> String {
    if integer {
        return format!("{}", value as i64);
    }
    if !value.is_finite() {
        return format!("{value}.0");
    }
    let mut text = format!("{value:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    if !text.contains('.') {
        text.push_str(".0");
    }
    text
}

/// `_invalid_field`: tints the field red while its text does not parse.
pub(in crate::ui::dialogs) fn invalid_field(field: &Gd<LineEdit>, invalid: bool) {
    let mut field = field.clone();
    field.add_theme_color_override(
        "font_color",
        if invalid {
            Color::from_rgba(1.0, 0.35, 0.35, 1.0)
        } else {
            Color::WHITE
        },
    );
}

/// `_in_game()` (`GameState.isGame`: playing/paused, not menu).
pub(in crate::ui::dialogs) fn in_game() -> bool {
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("get_state") {
        return false;
    }
    host.call("get_state", &[]).to::<GString>() != "menu"
}

/// `Pixmap`/`Color8` RGBA8888 -> `Color`.
pub(in crate::ui::dialogs) fn rgba8_color(value: i64) -> Color {
    let packed = value as u32;
    Color::from_rgba8(
        ((packed >> 24) & 0xff) as u8,
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

/// `_notify(key)`: the bundle lookup as an info prompt.
pub(in crate::ui::dialogs) fn notify_key(key: &str) {
    let _ = ui_call("show_info", &[kit::t(key).to_variant()]);
}

/// `Bundle.format(key, args)`: `{0}`-style substitution (`_fmt`).
pub(in crate::ui::dialogs) fn fmt_key(key: &str, args: &[&str]) -> String {
    let mut result = kit::t(key).to_string();
    for (index, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{index}}}"), arg);
    }
    result
}
