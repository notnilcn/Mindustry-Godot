// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the in-game menu dialogs (the old `batch_b.rs` helper section).
//! Upstream: none (port-only; shared helpers of the old batch_b)

use godot::classes::{Button, Control, HBoxContainer, Json, Label, MarginContainer, Node};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::gdutil::variant::to_bool_lenient;
use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::common::defer::defer_action;
use crate::ui::dialogs::common::facade::ui_call;
use crate::ui::kit;

/// Queues a dialog action for the idle phase (see [`defer_action`]).
pub(in crate::ui::dialogs) fn defer_dialog(
    name: &'static str,
    dialog: &Gd<MindDialog>,
    action: fn(&mut MindDialog),
) {
    let dialog = dialog.clone();
    defer_action(name, move || {
        if !dialog.is_instance_valid() {
            return;
        }
        let mut node = dialog.clone();
        let mut bound = node.bind_mut();
        action(&mut bound);
    });
}

/// Runs a Rust-side dialog action from an engine callback (button press, timer).
///
/// Only valid when the dialog is not already executing; UI signal handlers must
/// go through [`defer_dialog`] instead.
pub(in crate::ui::dialogs) fn with_dialog(dialog: &Gd<MindDialog>, action: fn(&mut MindDialog)) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let mut bound = node.bind_mut();
    action(&mut bound);
}

/// A `pressed` listener running a dialog action.
pub(in crate::ui::dialogs) fn dialog_press(
    name: &'static str,
    dialog: &Gd<MindDialog>,
    action: fn(&mut MindDialog),
) -> Callable {
    let dialog = dialog.clone();
    Callable::from_fn(name, move |_args: &[&Variant]| {
        with_dialog(&dialog, action);
    })
}

/// A `pressed` listener running a plain action.
pub(in crate::ui::dialogs) fn press(name: &'static str, action: fn()) -> Callable {
    Callable::from_fn(name, move |_args: &[&Variant]| action())
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

/// Shows a transient info prompt (`ui.showInfo`).
pub(in crate::ui::dialogs) fn ui_show_info(text: &str) {
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

/// Shows a confirm prompt (`ui.showConfirm`); answers on `confirm_result`.
pub(in crate::ui::dialogs) fn ui_show_confirm(text: &str) {
    let _ = ui_call("show_confirm", &[GString::from(text).to_variant()]);
}

/// Calls a campaign facade method (`None` when the facade or method is absent).
pub(in crate::ui::dialogs) fn campaign_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut campaign = kit::campaign()?;
    if !campaign.has_method(method) {
        return None;
    }
    Some(campaign.call(method, args))
}

/// A campaign facade method returning a dictionary (`{}` when absent).
pub(in crate::ui::dialogs) fn campaign_dict(method: &str) -> VarDictionary {
    campaign_call(method, &[])
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// `MindCampaign.get_sector_state` (the pause dialog's campaign branch).
pub(in crate::ui::dialogs) fn sector_state() -> VarDictionary {
    campaign_dict("get_sector_state")
}

/// `MindNet.session_state() != "offline"` (`net.active()`).
pub(in crate::ui::dialogs) fn net_active() -> bool {
    let Some(mut net) = kit::net() else {
        return false;
    };
    if !net.has_method("session_state") {
        return false;
    }
    net.call("session_state", &[])
        .try_to::<GString>()
        .unwrap_or_default()
        != "offline"
}

/// `state.gameOver` from the live HUD state.
pub(in crate::ui::dialogs) fn game_over_flag() -> bool {
    let Some(hud) = kit::hud() else {
        return false;
    };
    hud.get("game_over").try_to::<bool>().unwrap_or(false)
}

/// `Vars.mobile` (`MindUi.is_mobile`).
pub(in crate::ui::dialogs) fn is_mobile() -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    if !ui.has_method("is_mobile") {
        return false;
    }
    ui.call("is_mobile", &[]).try_to::<bool>().unwrap_or(false)
}

/// `control.saves.getCurrent().isAutosave()` through the campaign facade.
pub(in crate::ui::dialogs) fn current_slot_autosave() -> bool {
    campaign_call("current_save_autosave", &[])
        .map(|value| to_bool_lenient(&value))
        .unwrap_or(false)
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

/// `JSON.stringify(value)` for a dictionary.
pub(in crate::ui::dialogs) fn stringify_object(value: &VarDictionary) -> String {
    Json::stringify(&value.to_variant()).to_string()
}

/// Wraps `child` in a uniform margin (`Cell.pad`).
pub(in crate::ui::dialogs) fn padded<T: Inherits<Control> + Inherits<Node>>(
    child: &Gd<T>,
    margin: f32,
) -> Gd<MarginContainer> {
    // code-instantiated: padding wrapper for a data-built cell.
    let mut holder = MarginContainer::new_alloc();
    let margin = margin as i32;
    holder.add_theme_constant_override("margin_left", margin);
    holder.add_theme_constant_override("margin_right", margin);
    holder.add_theme_constant_override("margin_top", margin);
    holder.add_theme_constant_override("margin_bottom", margin);
    holder.add_child(child);
    holder
}

/// Sets the visible label of an `icon_button` (its text lives in the row).
pub(in crate::ui::dialogs) fn set_button_text(button: &Gd<Button>, value: &str) {
    let mut labels: Vec<Gd<Label>> = Vec::new();
    for child in button.get_children().iter_shared() {
        let Ok(row) = child.clone().try_cast::<HBoxContainer>() else {
            continue;
        };
        for inner in row.get_children().iter_shared() {
            if let Ok(label) = inner.clone().try_cast::<Label>() {
                labels.push(label);
            }
        }
    }
    match labels.last() {
        Some(label) => {
            let mut label = label.clone();
            label.set_text(value);
        }
        None => {
            let mut button = button.clone();
            button.set_text(value);
        }
    }
}

/// `Strings.formatMillis` (`H:MM:SS`, hours unpadded).
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

/// One stat row's label key and `GameStats` field (`GameOverDialog.STAT_ROWS`).
pub(in crate::ui::dialogs) const STAT_ROWS: [(&str, &str); 6] = [
    ("stats.wave", "wave"),
    ("stats.unitsCreated", "unitsCreated"),
    ("stats.enemiesDestroyed", "enemiesDestroyed"),
    ("stats.built", "built"),
    ("stats.destroyed", "destroyed"),
    ("stats.deconstructed", "deconstructed"),
];
