// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the old `batch_h.rs` (host, load/save and campaign rules). Filled by S07 (docs/PLANS).
//! Upstream: none (port-only; shared helpers of the old batch_h)

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::Node;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::net::host::{HOST_COLOR_SETTING, HOST_NAME_SETTING};
use crate::ui::kit::{self, MindTable};

/// A queued one-shot action (`Callable::call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase.
///
/// A prompt (`confirm_result`/`text_input_result`) and the file chooser's
/// `file_chosen` are emitted from inside a method of the emitting dialog, so
/// the answering handler must not re-bind that object synchronously.
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

/// `Core.settings.getString(key, default)` (`None` without `MindUi`).
pub(in crate::ui::dialogs) fn settings_text(key: &str, default: &str) -> Option<String> {
    ui_call(
        "settings_get_text",
        &[
            GString::from(key).to_variant(),
            GString::from(default).to_variant(),
        ],
    )
    .map(|value| value.to::<GString>().to_string())
}

/// `Core.settings.put(key, value)` for a string.
pub(in crate::ui::dialogs) fn settings_put_text(key: &str, value: &str) {
    let _ = ui_call(
        "settings_put_text",
        &[
            GString::from(key).to_variant(),
            GString::from(value).to_variant(),
        ],
    );
}

/// `Core.settings.getInt(key, default)` (`None` without `MindUi`).
pub(in crate::ui::dialogs) fn settings_int(key: &str, default: i64) -> Option<i64> {
    ui_call(
        "settings_get_int",
        &[GString::from(key).to_variant(), default.to_variant()],
    )
    .map(|value| variant_i64(&value))
}

/// `Core.settings.put(key, value)` for an integer.
pub(in crate::ui::dialogs) fn settings_put_int(key: &str, value: i64) {
    let _ = ui_call(
        "settings_put_int",
        &[GString::from(key).to_variant(), value.to_variant()],
    );
}

/// `Core.settings.getString("name")` (`HostDialog.java:24`).
pub(in crate::ui::dialogs) fn stored_player_name() -> String {
    settings_text(HOST_NAME_SETTING, "").unwrap_or_default()
}

/// `Core.settings.getInt("color-0")` as `Color.rgba8888()`; unset (0) keeps the
/// palette default (`HostDialog.java:31`).
pub(in crate::ui::dialogs) fn stored_player_color() -> Color {
    let Some(rgba) = settings_int(HOST_COLOR_SETTING, 0) else {
        return Color::WHITE;
    };
    if rgba == 0 {
        return Color::WHITE;
    }
    // The store keeps the ARGB word as a signed 32-bit value; the low 32 bits
    // are the `0xRRGGBBAA` packing (`Color.to_rgba32`).
    let packed = rgba as u32;
    Color::from_rgba8(
        ((packed >> 24) & 0xFF) as u8,
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    )
}

/// `Color.to_rgba32()`: the `0xRRGGBBAA` word Godot's method returns (gdext
/// only exposes it on the internal by-value `InnerColor`, so the packing is
/// reproduced here with Godot's truncating channel conversion).
pub(in crate::ui::dialogs) fn color_rgba32(color: Color) -> i64 {
    let channel = |value: f32| -> i64 { (value.clamp(0.0, 1.0) * 255.0) as i64 };
    (channel(color.r) << 24) | (channel(color.g) << 16) | (channel(color.b) << 8) | channel(color.a)
}

/// Pushes `player.name`/`player.color` to the relay identity reducers
/// (`MindNet.set_player_identity`; the facade queues them on connect).
pub(in crate::ui::dialogs) fn push_player_identity(name: &str, color: Color) {
    let Some(mut net) = kit::net() else {
        return;
    };
    if !net.has_method("set_player_identity") {
        return;
    }
    let _ = net.call(
        "set_player_identity",
        &[
            GString::from(name).to_variant(),
            color_rgba32(color).to_variant(),
        ],
    );
}

/// Calls a campaign facade method (`None` when the facade or method is absent).
pub(in crate::ui::dialogs) fn campaign_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut campaign = kit::campaign()?;
    if !campaign.has_method(method) {
        return None;
    }
    Some(campaign.call(method, args))
}

/// A campaign facade method's boolean result (`false` when absent).
pub(in crate::ui::dialogs) fn campaign_bool(method: &str, args: &[Variant]) -> bool {
    campaign_call(method, args)
        .map(|value| variant_bool(&value))
        .unwrap_or(false)
}

/// A dictionary value coerced to text (`str(value)`; empty when absent).
pub(in crate::ui::dialogs) fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_default()
}

/// A dictionary value coerced to text, with a fallback.
pub(in crate::ui::dialogs) fn dict_string_or(
    dict: &VarDictionary,
    key: &str,
    default: &str,
) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_else(|| default.to_string())
}

/// A dictionary value coerced to an integer.
pub(in crate::ui::dialogs) fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to a boolean.
pub(in crate::ui::dialogs) fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// `str(value)` for one variant.
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

/// Shows a transient info prompt (`ui.showInfo`).
pub(in crate::ui::dialogs) fn ui_show_info(text: &str) {
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

/// Shows a confirm prompt (`ui.showConfirm`); answers on `confirm_result`.
pub(in crate::ui::dialogs) fn ui_show_confirm(text: &str) {
    let _ = ui_call("show_confirm", &[GString::from(text).to_variant()]);
}

/// `UI.showTextInput(title, message, maxLength, defaultText, numeric,
/// allowEmpty)`.
pub(in crate::ui::dialogs) fn ui_show_text_input(
    title: &str,
    message: &str,
    max_length: i64,
    default_text: &str,
) {
    let _ = ui_call(
        "show_text_input",
        &[
            GString::from(title).to_variant(),
            GString::from(message).to_variant(),
            max_length.to_variant(),
            GString::from(default_text).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `ui.showErrorMessage` via the shared text prompt (`@error.title`).
pub(in crate::ui::dialogs) fn ui_show_error(text: &str) {
    let title = kit::t("@error.title").to_string();
    let title = kit::markup_text(&title);
    let body = kit::markup_text(text);
    let _ = ui_call("show_text", &[title.to_variant(), body.to_variant()]);
}

/// A dialog node by manifest name (the `UiRoot` registry, else the tree).
pub(in crate::ui::dialogs) fn dialog_node(name: &str) -> Option<Gd<Node>> {
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("dialog")
        && let Ok(node) = root
            .call("dialog", &[GString::from(name).to_variant()])
            .try_to::<Gd<Node>>()
    {
        return Some(node);
    }
    let root = kit::tree_root()?;
    root.find_child(name)
}

/// Opens a registered dialog through `MindUi` (`ui.<name>.show()`).
pub(in crate::ui::dialogs) fn open_named_dialog(name: &str, context_json: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    let _ = ui.call(
        "open_dialog",
        &[
            GString::from(name).to_variant(),
            GString::from(context_json).to_variant(),
        ],
    );
}

/// `ui.close_dialog(id)`: whether `MindUi` owned the close.
pub(in crate::ui::dialogs) fn close_named_dialog(id: &str) -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    if !ui.has_method("close_dialog") {
        return false;
    }
    ui.call("close_dialog", &[GString::from(id).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false)
}

/// Hides a dialog node directly (`hide_dialog` without the `MindUi` stack).
pub(in crate::ui::dialogs) fn hide_dialog_node(dialog: &Gd<MindDialog>) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let _ = node.call("hide_dialog", &[]);
}

/// Emits a `MindDialog` signal from a controller callback.
pub(in crate::ui::dialogs) fn emit_dialog_signal(
    dialog: &Gd<MindDialog>,
    signal: &str,
    args: &[Variant],
) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let _ = node.emit_signal(signal, args);
}

/// Stamps and returns the next controller generation on `dialog`.
pub(in crate::ui::dialogs) fn next_generation(dialog: &Gd<MindDialog>, key: &str) -> i64 {
    let mut node = dialog.clone();
    let current = if node.has_meta(key) {
        variant_i64(&node.get_meta(key))
    } else {
        0
    };
    let next = current + 1;
    node.set_meta(key, &next.to_variant());
    next
}

/// Whether `generation` is still the controller installed on `dialog`.
pub(in crate::ui::dialogs) fn generation_live(
    dialog: &Gd<MindDialog>,
    key: &str,
    generation: i64,
) -> bool {
    dialog.is_instance_valid()
        && dialog.has_meta(key)
        && variant_i64(&dialog.get_meta(key)) == generation
}

/// Replaces (or installs) one lasting `MindUi` signal handler, remembering the
/// callable on the dialog so an extension reload does not stack duplicates.
pub(in crate::ui::dialogs) fn install_ui_signal(
    dialog: &Gd<MindDialog>,
    signal: &str,
    meta: &str,
    callable: &Callable,
) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal(signal) {
        return;
    }
    let mut node = dialog.clone();
    if node.has_meta(meta) {
        if let Ok(previous) = node.get_meta(meta).try_to::<Callable>()
            && ui.is_connected(signal, &previous)
        {
            ui.disconnect(signal, &previous);
        }
        node.remove_meta(meta);
    }
    if !ui.is_connected(signal, callable) {
        ui.connect(signal, callable);
    }
    node.set_meta(meta, &callable.to_variant());
}

/// Removes and frees every child of a `MindTable` (and drops its cell entries).
pub(in crate::ui::dialogs) fn clear_table_nodes(table: &mut Gd<MindTable>) {
    for child in table.get_children().iter_shared() {
        let mut child = child.clone();
        table.remove_child(&child);
        child.queue_free();
    }
    table.bind_mut().clear();
}

/// `String.get_file()` + `String.get_basename()`: the file stem of a path.
pub(in crate::ui::dialogs) fn file_basename(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match file.rfind('.') {
        Some(index) if index > 0 => file[..index].to_string(),
        _ => file.to_string(),
    }
}

/// `Strings.formatMillis`: `H:MM:SS`.
pub(in crate::ui::dialogs) fn format_millis(millis: i64) -> String {
    let sign = if millis < 0 { "-" } else { "" };
    let value = millis.abs();
    format!(
        "{sign}{}:{:02}:{:02}",
        value / 3_600_000,
        (value % 3_600_000) / 60_000,
        (value % 60_000) / 1000
    )
}
