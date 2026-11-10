// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Join/host/load/save plumbing shared by the `net` and `game` dialogs. Filled by S07 (docs/PLANS).
//! Upstream: none (port-only; join/host/load/save shared plumbing)

use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::game::util_b::{variant_bool, variant_i64, variant_string};
use crate::ui::kit;

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

/// Calls one `MindUi` method (`None` when the facade or method is absent).
pub(in crate::ui::dialogs) fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

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
    .map(|value| variant_i64(&value))
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
        .map(|value| variant_bool(&value))
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

/// A dictionary value coerced to text, with a fallback (`str(dict.get(key, d))`).
pub(in crate::ui::dialogs) fn dict_string_or(
    dict: &VarDictionary,
    key: &str,
    default: &str,
) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_else(|| default.to_string())
}
