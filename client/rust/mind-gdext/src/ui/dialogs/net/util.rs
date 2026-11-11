// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the net dialogs: join/host/load/save plumbing (old
//! `batch_b.rs`), the host/load/save helper section (old `batch_h.rs`) and the
//! misc coercion helpers (old `misc.rs`).
//! Upstream: none (port-only; join/host/load/save shared plumbing)

use godot::prelude::*;

use crate::gdutil::variant::{to_bool_lenient, to_i64_lenient, to_text_lenient};
use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::common::facade::ui_call;
use crate::ui::dialogs::game::util::campaign_call;
use crate::ui::kit::{self, MindTable};

// ---------------------------------------------------------------------------
// Join / host / load / save shared plumbing (I08)
// ---------------------------------------------------------------------------

/// `Core.settings` `name` key (`JoinDialog.java:341`, `HostDialog.java:24`).
pub(in crate::ui::dialogs) const PLAYER_NAME_SETTING: &str = "name";
/// `Core.settings` `color-0` key (`JoinDialog.java:349`).
pub(in crate::ui::dialogs) const PLAYER_COLOR_SETTING: &str = "color-0";
/// `maxLength(40)` on the player-name fields (`JoinDialog.java:341`).
pub(in crate::ui::dialogs) const PLAYER_NAME_MAX_LENGTH: i32 = 40;
/// Java `Core.settings.getJson("servers")` key (`JoinDialog.java:691`).
pub(in crate::ui::dialogs) const SERVERS_SETTING: &str = "servers";
/// The add panel's remembered address (`JoinDialog.java:83`).
pub(in crate::ui::dialogs) const IP_SETTING: &str = "ip";
/// The local relay address (`tools/mp-local.sh up`); Java's LAN discovery has
/// no STDB equivalent, so "local" is the default server (`JoinDialog.java:418-424`).
pub(in crate::ui::dialogs) const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:3000/mindustry";
pub(in crate::ui::dialogs) const DEFAULT_DB_NAME: &str = "mindustry";
pub(in crate::ui::dialogs) const DEFAULT_SERVER_PORT: i64 = 3000;
/// Probe rows re-poll every 10 frames at 60 Hz (the transport is async).
pub(in crate::ui::dialogs) const PROBE_POLL_SECONDS: f64 = 10.0 / 60.0;

/// Per-controller generation keys.
///
/// A lasting handler connected to `MindUi` (or to the shared picker/file
/// chooser) captures the controller state. `ready()` runs again on an extension
/// reload and installs a new controller, so every handler also checks the
/// generation stamped on its node: the previous generation's handlers retire
/// instead of mutating stale state.
pub(in crate::ui::dialogs) const JOIN_GENERATION_META: &str = "_mind_join_generation";

/// Whether `MindUi` exposes `method`.
pub(in crate::ui::dialogs) fn ui_has_method(method: &str) -> bool {
    kit::ui().map(|ui| ui.has_method(method)).unwrap_or(false)
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
    .map(|value| to_i64_lenient(&value))
}

/// `Core.settings.put(key, value)` for an integer.
pub(in crate::ui::dialogs) fn settings_put_int(key: &str, value: i64) {
    let _ = ui_call(
        "settings_put_int",
        &[GString::from(key).to_variant(), value.to_variant()],
    );
}

/// `Core.settings.getBool(key)` (`None` without `MindUi`).
pub(in crate::ui::dialogs) fn settings_bool(key: &str) -> Option<bool> {
    ui_call("settings_get_bool", &[GString::from(key).to_variant()])
        .map(|value| to_bool_lenient(&value))
}

/// `Core.settings.getString("name")` (`JoinDialog.java:341`).
pub(in crate::ui::dialogs) fn stored_player_name() -> String {
    settings_text(PLAYER_NAME_SETTING, "").unwrap_or_default()
}

/// `Core.settings.getInt("color-0")` as `Color.rgba8888()`; unset (0) keeps the
/// palette default (`JoinDialog.java:349`).
pub(in crate::ui::dialogs) fn stored_player_color() -> Color {
    let Some(rgba) = settings_int(PLAYER_COLOR_SETTING, 0) else {
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

/// `Color.to_u32(godot::builtin::ColorChannelOrder::RGBA)`: the `0xRRGGBBAA` word Godot's method returns (gdext
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

/// A campaign facade method's boolean result (`false` when absent).
pub(in crate::ui::dialogs) fn campaign_bool(method: &str, args: &[Variant]) -> bool {
    campaign_call(method, args)
        .map(|value| to_bool_lenient(&value))
        .unwrap_or(false)
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
        to_i64_lenient(&node.get_meta(key))
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
        && to_i64_lenient(&dialog.get_meta(key)) == generation
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
    kit::clear_children(table);
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

/// A dictionary value coerced to text, with a fallback (`str(dict.get(key, d))`).
pub(in crate::ui::dialogs) fn dict_string_or(
    dict: &VarDictionary,
    key: &str,
    default: &str,
) -> String {
    dict.get(key)
        .map(|value| to_text_lenient(&value))
        .unwrap_or_else(|| default.to_string())
}

/// `Planet.localizedName` (`planet.<name>.name`) for the completion banner.
pub(in crate::ui::dialogs) fn planet_localized(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    let key = format!("planet.{name}.name");
    let localized = kit::t(&format!("@{key}")).to_string();
    if localized.is_empty() || localized == key {
        name.to_string()
    } else {
        localized
    }
}

/// The `Variant` array coerced to a `String` list (`String[]` context rows).
pub(in crate::ui::dialogs) fn string_array(value: Option<Variant>) -> Vec<String> {
    value
        .map(|value| value.to::<VarArray>())
        .map(|array| {
            array
                .iter_shared()
                .map(|entry| entry.to::<GString>().to_string())
                .collect()
        })
        .unwrap_or_default()
}
