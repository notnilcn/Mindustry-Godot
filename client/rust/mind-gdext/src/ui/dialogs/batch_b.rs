// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/PausedDialog.java (14-217),
//         core/src/mindustry/ui/dialogs/GameOverDialog.java (43-184),
//         core/src/mindustry/ui/dialogs/JoinDialog.java (77-775),
//         core/src/mindustry/ui/dialogs/HostDialog.java (22-99),
//         core/src/mindustry/ui/dialogs/LoadDialog.java (47-292),
//         core/src/mindustry/ui/dialogs/SaveDialog.java (10-39).

//! In-game menu dialogs: the pause rows, the game-over summary, the server
//! browser and host setup, and the save/load slot grids.
//!
//! Every controller mirrors one `client/ui/dialogs/*.gd` script and keeps its
//! data flow: prompts and the dialog stack go through `MindUi`, saves/rules/
//! sector state through the campaign facade, and server probes/identity through
//! `MindNet`. Signals (`host_requested`, `connect_requested`,
//! `join_requested`, `save_requested`, `slot_selected`, …) are declared on
//! [`MindDialog`] and emitted with `emit_signal`, so `UiRoot`'s wiring keeps
//! working unchanged. A signal emitted by `MindUi` runs while `MindUi` is
//! bound; those handlers queue their work through [`defer_action`] because
//! re-entering a bound GDExtension object panics (`PAR-0066`).

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{
    Button, CanvasItem, CenterContainer, ColorRect, Control, HBoxContainer, Json, Label, LineEdit,
    MarginContainer, Node, PanelContainer, RichTextLabel, StyleBoxFlat, Timer, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindTable, styles};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "paused" => Some(Box::new(PausedDialog::default())),
        // The manifest name of the game-over scene (`GameOverDialog`).
        "restart" => Some(Box::new(GameOverDialog::default())),
        "join" => Some(Box::new(JoinDialogController::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// A queued one-shot action (`Callable::call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase.
///
/// A `MindUi` prompt signal (`confirm_result`/`text_input_result`) is emitted
/// from inside a `MindUi` method, so the answering handler must not call back
/// into the facade synchronously: `Gd::bind_mut` panics on a re-entrant borrow
/// (`PAR-0066`).
fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
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

/// Queues a dialog action for the idle phase (see [`defer_action`]).
fn defer_dialog(name: &'static str, dialog: &Gd<MindDialog>, action: fn(&mut MindDialog)) {
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
fn with_dialog(dialog: &Gd<MindDialog>, action: fn(&mut MindDialog)) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let mut bound = node.bind_mut();
    action(&mut bound);
}

/// A `pressed` listener running a dialog action.
fn dialog_press(
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
fn press(name: &'static str, action: fn()) -> Callable {
    Callable::from_fn(name, move |_args: &[&Variant]| action())
}

/// Opens a registered dialog through `MindUi` (`ui.<name>.show()`).
fn open_named_dialog(name: &str, context_json: &str) {
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
fn ui_show_info(text: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_info") {
        return;
    }
    let _ = ui.call("show_info", &[GString::from(text).to_variant()]);
}

/// Shows a confirm prompt (`ui.showConfirm`); answers on `confirm_result`.
fn ui_show_confirm(text: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_confirm") {
        return;
    }
    let _ = ui.call("show_confirm", &[GString::from(text).to_variant()]);
}

/// Calls a campaign facade method (`None` when the facade or method is absent).
fn campaign_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut campaign = kit::campaign()?;
    if !campaign.has_method(method) {
        return None;
    }
    Some(campaign.call(method, args))
}

/// A campaign facade method returning a dictionary (`{}` when absent).
fn campaign_dict(method: &str) -> VarDictionary {
    campaign_call(method, &[])
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// `MindCampaign.get_sector_state` (the pause dialog's campaign branch).
fn sector_state() -> VarDictionary {
    campaign_dict("get_sector_state")
}

/// `MindNet.session_state() != "offline"` (`net.active()`).
fn net_active() -> bool {
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
fn game_over_flag() -> bool {
    let Some(hud) = kit::hud() else {
        return false;
    };
    hud.get("game_over").try_to::<bool>().unwrap_or(false)
}

/// `Vars.mobile` (`MindUi.is_mobile`).
fn is_mobile() -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    if !ui.has_method("is_mobile") {
        return false;
    }
    ui.call("is_mobile", &[]).try_to::<bool>().unwrap_or(false)
}

/// `control.saves.getCurrent().isAutosave()` through the campaign facade.
fn current_slot_autosave() -> bool {
    campaign_call("current_save_autosave", &[])
        .map(|value| variant_bool(&value))
        .unwrap_or(false)
}

/// A dialog node by manifest name (the `UiRoot` registry, else the tree).
fn dialog_node(name: &str) -> Option<Gd<Node>> {
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

/// A dictionary value coerced to text (`str(value)`; empty when absent).
fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_default()
}

/// A dictionary value coerced to an integer.
fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to a boolean.
fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// `str(value)` for one variant.
fn variant_string(value: &Variant) -> String {
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
fn variant_i64(value: &Variant) -> i64 {
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
fn variant_f64(value: &Variant) -> f64 {
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
fn variant_bool(value: &Variant) -> bool {
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

/// `JSON.parse_string(text)` as an object (`{}` on any other shape).
fn parse_json_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `JSON.parse_string(text)` as an array (`[]` on any other shape).
fn parse_json_array(text: &str) -> VarArray {
    if text.is_empty() {
        return VarArray::new();
    }
    Json::parse_string(text)
        .try_to::<VarArray>()
        .unwrap_or_default()
}

/// `JSON.stringify(value)` for a dictionary.
fn stringify_object(value: &VarDictionary) -> String {
    Json::stringify(&value.to_variant()).to_string()
}

/// Wraps `child` in a uniform margin (`Cell.pad`).
fn padded<T: Inherits<Control> + Inherits<Node>>(
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
fn set_button_text(button: &Gd<Button>, value: &str) {
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
fn format_millis(millis: i64) -> String {
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
const STAT_ROWS: [(&str, &str); 6] = [
    ("stats.wave", "wave"),
    ("stats.unitsCreated", "unitsCreated"),
    ("stats.enemiesDestroyed", "enemiesDestroyed"),
    ("stats.built", "built"),
    ("stats.destroyed", "destroyed"),
    ("stats.deconstructed", "deconstructed"),
];

// ---------------------------------------------------------------------------
// PausedDialog
// ---------------------------------------------------------------------------

/// A pending `MindUi.show_confirm` answer (`_pending_quit`).
const PENDING_QUIT: u8 = 1;
/// A pending `MindUi.show_confirm` answer (`_pending_abandon`).
const PENDING_ABANDON: u8 = 2;
/// The pending-answer slot on the dialog node.
const PENDING_META: &str = "_pending_pause";

/// `PausedDialog` — the in-game pause menu (`@menu`).
///
/// `should_pause = true` (manifest `pause`) so showing it through `MindUi`
/// pauses the sim via the reference-counted governor. Every exit routes back
/// through `MindUi` (settings/planet) or the `UiRoot` menu presentation; the
/// dialog never mutates sim state.
#[derive(Default)]
pub struct PausedDialog {
    /// The `@objective`/`@abandon` row; hidden (no VBox separation) unless one
    /// of its buttons shows.
    campaign_row: Option<Gd<HBoxContainer>>,
    /// The `@savegame`/`@loadgame` row (desktop, non-campaign, non-editor).
    save_row: Option<Gd<HBoxContainer>>,
    objective_button: Option<Gd<Button>>,
    abandon_button: Option<Gd<Button>>,
    save_button: Option<Gd<Button>>,
    load_button: Option<Gd<Button>>,
    planet_button: Option<Gd<Button>>,
    host_button: Option<Gd<Button>>,
    quit_button: Option<Gd<Button>>,
    /// `PausedDialog`'s bottom-left `Icon.book` rules button.
    rules_button: Option<Gd<Button>>,
}

impl DialogController for PausedDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@menu");
        dialog.should_pause = true;
        // `clearChildren(); add(titleTable)` (`PausedDialog.java:24-25`): the
        // padded title cell.
        dialog.apply_button_overlay(true);
        dialog.enable_full_dialog(true);
        self.build(dialog);
        self.connect_confirm(dialog);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.refresh_campaign_buttons(dialog);
        self.refresh_host_button();
        self.refresh_rules_button(dialog);
    }
}

impl PausedDialog {
    /// `PausedDialog.rebuild`'s fixed desktop/mobile entry layout.
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        dialog.clear_buttons();
        let dialog_gd = dialog.to_gd();
        // code-instantiated: the pause dialog's fixed upstream row layout; the
        // shared `buttons` HBox is a single row for every dialog.
        let mut rows = VBoxContainer::new_alloc();
        rows.set_name("Rows");
        rows.add_theme_constant_override("separation", 10);
        // `cont.grow()` with the row table centred: the Java rows sit around
        // mid-screen, so host them in a centred filler between the title and
        // the (empty) bottom button row instead of the scrollable body.
        if let Some(layout) = dialog.base().get_node_or_null("Center/Panel/Layout") {
            let mut layout = layout;
            if let Some(mut old) = layout.get_node_or_null("RowsCenter") {
                layout.remove_child(&old);
                old.queue_free();
            }
            if let Some(mut body) = dialog.body() {
                body.set_visible(false);
            }
            let mut host = CenterContainer::new_alloc();
            host.set_name("RowsCenter");
            host.set_h_size_flags(SizeFlags::EXPAND_FILL);
            host.set_v_size_flags(SizeFlags::EXPAND_FILL);
            let host_control: Gd<Control> = host.clone().upcast();
            layout.add_child(&host_control);
            if let Some(buttons) = dialog.buttons_row() {
                layout.move_child(&host_control, buttons.get_index());
            }
            host.add_child(&rows);
        }

        let mut campaign_row = Self::row(&mut rows);
        campaign_row.set_visible(false);
        let mut objective = Self::add_to(
            &mut campaign_row,
            &kit::t("@objective").to_string(),
            "info",
            220.0,
            press("pause_objective", open_objective),
        );
        objective.set_name("objective");
        objective.set_visible(false);
        self.objective_button = Some(objective);
        let mut abandon = Self::add_to(
            &mut campaign_row,
            &kit::t("@abandon").to_string(),
            "cancel",
            220.0,
            dialog_press("pause_abandon", &dialog_gd, confirm_abandon),
        );
        abandon.set_name("abandon");
        abandon.set_visible(false);
        self.abandon_button = Some(abandon);
        self.campaign_row = Some(campaign_row);

        let mut navigation_row = Self::row(&mut rows);
        let mut back = Self::add_to(
            &mut navigation_row,
            &kit::t("@back").to_string(),
            "left",
            220.0,
            dialog_press("pause_back", &dialog_gd, |target| target.close()),
        );
        back.set_name("back");
        let mut settings_button = Self::add_to(
            &mut navigation_row,
            &kit::t("@settings").to_string(),
            "settings",
            220.0,
            press("pause_settings", || open_named_dialog("settings", "")),
        );
        settings_button.set_name("settings");

        // Desktop non-campaign, non-editor entry set (`PausedDialog.java:89-93`);
        // the campaign/mobile branch replaces it with `@research`/`@planetmap`.
        let mut save_row = Self::row(&mut rows);
        let mut save_button = Self::add_to(
            &mut save_row,
            &kit::t("@savegame").to_string(),
            "save",
            220.0,
            press("pause_save", || open_named_dialog("save", "")),
        );
        save_button.set_name("savegame");
        self.save_button = Some(save_button);
        let mut load_button = Self::add_to(
            &mut save_row,
            &kit::t("@loadgame").to_string(),
            "upload",
            220.0,
            press("pause_load", || open_named_dialog("load", "")),
        );
        load_button.set_name("loadgame");
        self.load_button = Some(load_button);
        self.save_row = Some(save_row);

        // Mobile campaign branch entry (`PausedDialog.java:140-149`); desktop
        // keeps it hidden and takes no row space.
        let mut planet_button = Self::add_to(
            &mut rows,
            &kit::t("@planetmap").to_string(),
            "map",
            220.0,
            press("pause_planet", || open_named_dialog("planet", "")),
        );
        planet_button.set_name("planetmap");
        planet_button.set_visible(false);
        self.planet_button = Some(planet_button);

        let mut host_button = Self::add_to(
            &mut rows,
            &kit::t("@hostserver").to_string(),
            "host",
            450.0,
            press("pause_host", || open_named_dialog("host", "")),
        );
        host_button.set_name("hostserver");
        host_button.set_disabled(net_active());
        self.host_button = Some(host_button);

        // `PausedDialog.java:116`: the quit entry spans both columns but keeps
        // the `dw + 10` (230) width, so it centres inside its cell.
        let mut quit_button = Self::add_to(
            &mut rows,
            &kit::t("@quit").to_string(),
            "exit",
            230.0,
            dialog_press("pause_quit", &dialog_gd, ask_quit_confirm),
        );
        quit_button.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        quit_button.set_name("quit");
        self.quit_button = Some(quit_button);

        self.build_rules_button(dialog);
    }

    /// `stack(cont, new Table(t -> t.bottom().left().button(Icon.book, ...)))`
    /// (`PausedDialog.java:20-37`): the rules entry in the bottom-left corner.
    fn build_rules_button(&mut self, dialog: &mut MindDialog) {
        // code-instantiated: the corner entry has no static scene (visibility is
        // rule- and net-dependent).
        let mut button = factory::icon_button("book", "", "defaultt", AlignmentMode::BEGIN);
        button.set_name("customize");
        button.set_custom_minimum_size(Vector2::new(70.0, 70.0));
        button.set_tooltip_text(&kit::t("@customize"));
        button.set_anchor(Side::LEFT, 0.0);
        button.set_anchor(Side::TOP, 1.0);
        button.set_anchor(Side::RIGHT, 0.0);
        button.set_anchor(Side::BOTTOM, 1.0);
        button.set_offset(Side::LEFT, 12.0);
        button.set_offset(Side::TOP, -82.0);
        button.set_offset(Side::RIGHT, 82.0);
        button.set_offset(Side::BOTTOM, -12.0);
        button.connect("pressed", &press("pause_rules", open_rules));
        let control: Gd<Control> = button.clone().upcast();
        dialog.base_mut().add_child(&control);
        button.set_visible(false);
        self.rules_button = Some(button);
    }

    /// A centered two-entry row of the dialog's own button column.
    fn row(parent: &mut Gd<VBoxContainer>) -> Gd<HBoxContainer> {
        // code-instantiated: fixed upstream row grouping (campaign entries pair
        // up); the shared button row is a single HBox.
        let mut row = HBoxContainer::new_alloc();
        row.set_alignment(AlignmentMode::CENTER);
        row.add_theme_constant_override("separation", 10);
        parent.add_child(&row);
        row
    }

    /// `MindDialog.add_button` variant that targets one row of this dialog.
    fn add_to<P: Inherits<Node>>(
        parent: &mut Gd<P>,
        text_value: &str,
        icon_name: &str,
        width: f32,
        listener: Callable,
    ) -> Gd<Button> {
        // code-instantiated: the entry set is fixed per the upstream rebuild
        // branch.
        let mut button =
            factory::icon_button(icon_name, text_value, "defaultt", AlignmentMode::BEGIN);
        // `cont.defaults().width(dw).height(55)` (`PausedDialog.java:75`).
        button.set_custom_minimum_size(Vector2::new(width, 55.0));
        if listener.is_valid() {
            button.connect("pressed", &listener);
        }
        let mut parent_node: Gd<Node> = parent.clone().upcast();
        parent_node.add_child(&button);
        button
    }

    /// `PausedDialog.rebuild`: `@objective` needs a preset description;
    /// `@abandon` appears for a campaign sector; `@planetmap` only on mobile.
    fn refresh_campaign_buttons(&mut self, dialog: &mut MindDialog) {
        let state = sector_state();
        let campaign = dict_bool(&state, "campaign", false);
        let editor = dict_bool(&state, "editor", false);
        let can_save = !campaign && !editor;
        // `PausedDialog.java:89-93`: the campaign branch builds no
        // `@savegame`/`@loadgame` row at all, so hide the whole row container.
        if let Some(mut row) = self.save_row.clone() {
            row.set_visible(can_save);
        }
        if let Some(mut button) = self.save_button.clone() {
            button.set_visible(can_save);
        }
        let net_active = net_active();
        if let Some(mut button) = self.load_button.clone() {
            button.set_visible(can_save);
            // `PausedDialog.java:92`: load is disabled while a match is active.
            button.set_disabled(net_active);
        }
        if let Some(mut button) = self.objective_button.clone() {
            button.set_visible(!dict_string(&state, "presetDescription").is_empty());
        }
        let game_over = game_over_flag();
        if let Some(mut button) = self.abandon_button.clone() {
            button.set_visible(campaign);
            button.set_disabled(net_active || game_over);
        }
        if let Some(mut row) = self.campaign_row.clone() {
            let objective = self
                .objective_button
                .as_ref()
                .map(|button| button.is_visible())
                .unwrap_or(false);
            let abandon = self
                .abandon_button
                .as_ref()
                .map(|button| button.is_visible())
                .unwrap_or(false);
            row.set_visible(objective || abandon);
        }
        if let Some(mut button) = self.planet_button.clone() {
            // The Java mobile branch (`PausedDialog.java:140-149`); the GDScript
            // deferred this out of `MindUi`'s bind, which the Rust `shown()`
            // already runs outside of (it is called from a deferred
            // `show_dialog`).
            button.set_visible(campaign && is_mobile());
        }
        if let Some(button) = self.quit_button.clone() {
            // `PausedDialog.java:116,154-155`: the entry reads `@save.quit` while
            // the current save is an autosave.
            let key = if current_slot_autosave() {
                "@save.quit"
            } else {
                "@quit"
            };
            set_button_text(&button, &kit::t(key).to_string());
        }
        let _ = dialog;
    }

    /// `PausedDialog.java:98-105`: the host entry is disabled while a match is
    /// active (`net.active()`).
    fn refresh_host_button(&mut self) {
        if let Some(mut button) = self.host_button.clone() {
            button.set_disabled(net_active());
        }
    }

    /// `visible(() -> state.rules.allowEditRules && (net.server() || !net.active()))`.
    fn refresh_rules_button(&mut self, dialog: &mut MindDialog) {
        let Some(mut button) = self.rules_button.clone() else {
            return;
        };
        let mut enabled = false;
        if let Some(mut campaign) = dialog.campaign_node()
            && campaign.has_method("get_rules_json")
        {
            let text = campaign
                .call("get_rules_json", &[])
                .try_to::<GString>()
                .unwrap_or_default();
            let rules = parse_json_object(&text.to_string());
            enabled = dict_bool(&rules, "allowEditRules", false);
        }
        if enabled
            && let Some(mut net) = kit::net()
            && net.has_method("session_state")
        {
            let state = net
                .call("session_state", &[])
                .try_to::<GString>()
                .unwrap_or_default()
                .to_string();
            // `net.server()` hosts; an inactive client is offline play.
            enabled = state == "offline" || state == "hosting" || state == "host";
        }
        button.set_visible(enabled);
    }

    /// `PausedDialog.showQuitConfirm`/`abandonSectorConfirm` answers.
    fn connect_confirm(&mut self, dialog: &mut MindDialog) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_signal("confirm_result") {
            return;
        }
        let dialog_gd = dialog.to_gd();
        let callable = Callable::from_fn("pause_confirm_result", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| variant_bool(value))
                .unwrap_or(false);
            let flags = take_pending(&dialog_gd);
            if flags & PENDING_ABANDON != 0 {
                if confirmed {
                    defer_dialog("pause_abandon_confirmed", &dialog_gd, abandon_sector);
                }
                return Variant::nil();
            }
            if flags & PENDING_QUIT == 0 {
                return Variant::nil();
            }
            if confirmed {
                defer_dialog("pause_quit_confirmed", &dialog_gd, quit_with_exit_save);
            }
            Variant::nil()
        });
        ui.connect("confirm_result", &callable);
    }
}

/// Records a pending confirm answer on the dialog node.
fn set_pending(dialog: &Gd<MindDialog>, flag: u8) {
    let mut node = dialog.clone();
    node.set_meta(PENDING_META, &i64::from(flag).to_variant());
}

/// Takes (and clears) a pending confirm answer from the dialog node.
fn take_pending(dialog: &Gd<MindDialog>) -> u8 {
    let mut node = dialog.clone();
    if !node.has_meta(PENDING_META) {
        return 0;
    }
    let flag = node.get_meta(PENDING_META).try_to::<i64>().unwrap_or(0);
    node.remove_meta(PENDING_META);
    flag as u8
}

/// `PausedDialog.showQuitConfirm` (`@quit.confirm`); quitting runs the exit
/// save before returning to the standalone menu.
fn ask_quit_confirm(dialog: &mut MindDialog) {
    if kit::ui().is_some() {
        set_pending(&dialog.to_gd(), PENDING_QUIT);
        ui_show_confirm(&kit::t("@quit.confirm").to_string());
        return;
    }
    quit_with_exit_save(dialog);
}

/// `PausedDialog.java:82`: `ui.planet.abandonSectorConfirm(sector, hide)`.
fn confirm_abandon(dialog: &mut MindDialog) {
    if kit::ui().is_some() {
        set_pending(&dialog.to_gd(), PENDING_ABANDON);
        ui_show_confirm(&kit::t("@sector.abandon.confirm").to_string());
        return;
    }
    abandon_sector(dialog);
}

/// `PlanetDialog.abandonSectorConfirm` for the played sector.
///
/// The listener hides this dialog and the planet page; 7 ticks later the sim
/// kills the player cores, so the match ends through the campaign game-over
/// path (no direct return to the menu).
fn abandon_sector(dialog: &mut MindDialog) {
    let abandoned = campaign_call("abandon_sector", &[])
        .map(|value| variant_bool(&value))
        .unwrap_or(false);
    if !abandoned {
        return;
    }
    dialog.close();
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call("close_dialog", &[GString::from("planet").to_variant()]);
    }
}

/// `PausedDialog.showQuitConfirm`'s quit runnable: `runExitSave()` writes the
/// current autosave slot (and resets the match), then the menu shows.
fn quit_with_exit_save(dialog: &mut MindDialog) {
    // `runExitSave` -> `checkPlaytest()` (`PausedDialog.java:189-197`): a
    // playtest skips the exit save and returns to the editor. Close first:
    // closing releases the pause governor, then `check_playtest` re-parks the
    // sim behind the editor.
    let playtesting = campaign_call("playtesting_map", &[])
        .and_then(|value| value.try_to::<GString>().ok())
        .unwrap_or_default();
    if !playtesting.is_empty() {
        dialog.close();
        if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
            && root.has_method("check_playtest")
            && root
                .call("check_playtest", &[])
                .try_to::<bool>()
                .unwrap_or(false)
        {
            return;
        }
    }
    let _ = campaign_call("run_exit_save", &[]);
    dialog.close();
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("return_to_menu")
    {
        let _ = root.call("return_to_menu", &[]);
    }
}

/// `ui.fullText.show("@objective", preset.description)`.
fn open_objective() {
    let description = dict_string(&sector_state(), "presetDescription");
    if description.is_empty() {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("title", &kit::t("@objective").to_variant());
    context.set("text", &GString::from(description.as_str()).to_variant());
    open_named_dialog("full_text", &stringify_object(&context));
}

/// `stack(cont, ... button(Icon.book, ...))`: the rules editor edits a copy of
/// the live rules, written back on `rules_edited`.
fn open_rules() {
    let (Some(mut ui), Some(mut campaign)) = (kit::ui(), kit::campaign()) else {
        return;
    };
    if !campaign.has_method("get_rules_json") {
        return;
    }
    let text = campaign
        .call("get_rules_json", &[])
        .try_to::<GString>()
        .unwrap_or_default();
    let parsed = Json::parse_string(&text);
    let Ok(rules) = parsed.try_to::<VarDictionary>() else {
        return;
    };
    let mut context = VarDictionary::new();
    context.set("rules", &rules.to_variant());
    context.set("reset", &rules.to_variant());
    context.set("show_rule_edit", &false.to_variant());
    let _ = ui.call(
        "open_dialog",
        &[
            GString::from("custom_rules").to_variant(),
            GString::from(stringify_object(&context).as_str()).to_variant(),
        ],
    );
    let Some(mut rules_dialog) = dialog_node("custom_rules") else {
        return;
    };
    if !rules_dialog.has_signal("rules_edited") {
        return;
    }
    // One lasting connection (the edited JSON goes to the campaign facade).
    if rules_dialog.has_meta("_mind_rules_hook") {
        return;
    }
    let callable = Callable::from_fn("rules_edited", |args: &[&Variant]| {
        let json = args
            .first()
            .map(|value| variant_string(value))
            .unwrap_or_default();
        if let Some(mut campaign) = kit::campaign()
            && campaign.has_method("set_rules_json")
        {
            let _ = campaign.call(
                "set_rules_json",
                &[GString::from(json.as_str()).to_variant()],
            );
        }
    });
    rules_dialog.set_meta("_mind_rules_hook", &true.to_variant());
    rules_dialog.connect("rules_edited", &callable);
}

// ---------------------------------------------------------------------------
// GameOverDialog (registry id `restart`)
// ---------------------------------------------------------------------------

/// `cont.table(...).center().minWidth(370).maxSize(600f, 550f).grow()`
/// (`GameOverDialog.java:102`): the stats block is a centred 600 px column that
/// grows to at most 550 px tall.
const BLOCK_WIDTH: f32 = 600.0;
const BLOCK_MAX_HEIGHT: f32 = 550.0;
/// `Pal.darkestGray` (`graphics/Pal.java:61`): the colour every row animates to.
const DARKEST_GRAY: Color = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
/// `@gameover.waiting` (`bundle.properties:26`): the client-only campaign
/// "waiting for next map" line (`GameOverDialog.java:130-133`).
const GAMEOVER_WAITING_KEY: &str = "@gameover.waiting";
/// `@gameover.disconnect` (`bundle.properties:24`): the client-only campaign
/// button that drops the match and returns to the menu
/// (`GameOverDialog.java:105-111`).
const GAMEOVER_DISCONNECT_KEY: &str = "@gameover.disconnect";
/// `Core.settings` key for the one-time difficulty notice
/// (`GameOverDialog.java:115`, `Core.settings.getBoolOnce("difficultyguide")`).
const DIFFICULTY_GUIDE_SETTING: &str = "difficultyguide";
/// `@difficulty.guide.title` / `@difficulty.guide` / `@difficulty.guide.confirm`
/// (`bundle.properties:967-969`).
const DIFFICULTY_GUIDE_TITLE_KEY: &str = "@difficulty.guide.title";
const DIFFICULTY_GUIDE_KEY: &str = "@difficulty.guide";
const DIFFICULTY_GUIDE_CONFIRM_KEY: &str = "@difficulty.guide.confirm";
/// The shared `BaseDialog` shell for popups with no manifest entry (the
/// difficulty notice); it is instantiated on demand like Java's `new BaseDialog`.
const DIALOG_BASE_SCENE: &str = "res://scenes/ui/dialogs/mind_dialog_base.tscn";

/// `GameOverDialog` — the game-over summary (manifest dialog `restart`).
///
/// Shown by the in-game HUD watcher when the campaign reports `gameOver` (loss)
/// or a sector capture (win). Stat rows and the headline come from the dialog
/// context; campaign continues to the planet map, other modes return to the
/// standalone menu.
#[derive(Default)]
pub struct GameOverDialog {
    table: Option<Gd<MindTable>>,
    /// HUD fragments hidden while the dialog is up (restored in `hidden`).
    hidden_hud: Vec<Gd<CanvasItem>>,
}

impl DialogController for GameOverDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `GameOverDialog` (`GameOverDialog.java:25-30`): `super("@gameover")`
        // then `titleTable.remove()` — the content headline *is* the dialog's
        // only title, so no title row (and no accent rule) is built.
        dialog.should_pause = false;
        dialog.clear_content();
        let table = dialog.content_table();
        {
            let mut control: Gd<Control> = table.clone().upcast();
            // The block below `grow()`s, so the content table takes the whole
            // `cont` height (Arc `cont` is `expand().fill()`).
            control.set_v_size_flags(SizeFlags::EXPAND_FILL);
        }
        self.table = Some(table);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // `ui.hudfrag.shown = false` (GameOverDialog shown): the HUD tables
        // gated on `shown()` disappear while the dialog is up. The GDScript
        // deferred this out of `MindUi.open_dialog`'s bind; the Rust `shown()`
        // already runs from a deferred `show_dialog`, so it is called directly.
        self.hud_set(false);
        self.rebuild(dialog);
    }

    fn hidden(&mut self, _dialog: &mut MindDialog) {
        self.hud_set(true);
    }
}

impl GameOverDialog {
    /// `GameOverDialog.rebuild`: headline, then the `Styles.black3` stats pane
    /// with one 50 px accent row per stat in Java order, each sliding in after
    /// `0.05 * i` s with the value counting up (`StatLabel`).
    fn rebuild(&mut self, dialog: &mut MindDialog) {
        let Some(mut table) = self.table.clone() else {
            return;
        };
        table.bind_mut().clear();
        // code-instantiated: the headline and stat rows are data-driven
        // (campaign sector / PvP winner) and come from the `stats` context.
        let mut block = VBoxContainer::new_alloc();
        block.set_custom_minimum_size(Vector2::new(BLOCK_WIDTH, 0.0));
        block.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        block.add_theme_constant_override("separation", 0);
        // No `grow_x` on the cell: the block keeps its own 600 px width and the
        // table's default align centres it (Java's `.center()`). Vertically it
        // grows up to `maxSize(.., 550)` and centres in `cont`.
        table
            .bind_mut()
            .add(&block)
            .grow_y()
            .max_height(BLOCK_MAX_HEIGHT);
        table.bind_mut().row();
        let headline = Self::headline(dialog);
        block.add_child(&padded(&headline_label(&headline), 6.0));

        let stats = dialog
            .context_value("stats")
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        // `t.pane(...).grow().pad(12).top()`: the pane takes the block's
        // remaining height.
        let mut pane_holder = padded(&stats_pane(), 12.0);
        pane_holder.set_v_size_flags(SizeFlags::EXPAND_FILL);
        block.add_child(&pane_holder);
        let Some(pane) = pane_holder
            .get_child(0)
            .and_then(|node| node.try_cast::<PanelContainer>().ok())
        else {
            return;
        };
        let Some(rows) = pane
            .get_child(0)
            .and_then(|node| node.try_cast::<VBoxContainer>().ok())
        else {
            return;
        };
        let mut rows = rows;
        rows.add_child(&filler());
        // `if(state.rules.waves) addStat(...)` (`GameOverDialog.java:79`): only
        // the wave row is gated on waves; every other row always shows.
        for (index, entry) in STAT_ROWS.iter().enumerate() {
            let (key, field) = *entry;
            if field == "wave" && !dict_bool(&stats, "waves", false) {
                continue;
            }
            Self::add_stat_row(&mut rows, &stats, key, field, index);
        }
        rows.add_child(&filler());
        // `if(control.saves.getCurrent() != null)` (`GameOverDialog.java:92-96`):
        // the Time Played row only exists while a save slot is current.
        let playtime = current_playtime_ms();
        if playtime >= 0 {
            rows.add_child(&playtime_row(playtime));
        }
        // `if(state.isCampaign() && net.client())` (`GameOverDialog.java:130-133`):
        // a multiplayer campaign client gets the "waiting for next map" line
        // under the stats block, 20 px below it.
        if Self::is_campaign(dialog) && net_client() {
            table.bind_mut().row();
            // code-instantiated: the waiting line is a bundle string.
            let waiting =
                factory::styled_label(&kit::tm(GAMEOVER_WAITING_KEY).to_string(), "outlineLabel");
            table
                .bind_mut()
                .add(&waiting)
                .pad_top(20.0)
                .align(kit::Align::Center);
            table.bind_mut().row();
        }
        self.build_buttons(dialog);
    }

    /// `GameOverDialog.rebuild` headline: campaign capture/loss, PvP winner,
    /// else the plain game-over title.
    fn headline(dialog: &MindDialog) -> String {
        let sector = dialog.context_string("sector").to_string();
        if Self::is_campaign(dialog) {
            if dialog
                .context_value("captured")
                .map(|value| variant_bool(&value))
                .unwrap_or(false)
            {
                return kit::tm("@sector.curcapture").to_string();
            }
            if !sector.is_empty() {
                return kit::tm_format("@sector.lost", &[sector.as_str()]).to_string();
            }
            return kit::tm("@sector.lost").to_string();
        }
        let winner = dialog.context_string("winner").to_string();
        if !winner.is_empty() {
            return kit::tm_format("@gameover.pvp", &[winner.as_str()]).to_string();
        }
        kit::tm("@gameover").to_string()
    }

    /// `state.isCampaign()`: the dialog context, else the live sector state.
    fn is_campaign(dialog: &MindDialog) -> bool {
        if let Some(value) = dialog.context_value("campaign") {
            return variant_bool(&value);
        }
        dict_bool(&sector_state(), "campaign", false)
    }

    /// `GameOverDialog.java:101-140`: campaign gets `@continue` (planet map) or
    /// the client's `@gameover.disconnect`; other modes get `@menu` (the
    /// standalone menu); all are 60 px tall.
    fn build_buttons(&mut self, dialog: &mut MindDialog) {
        dialog.clear_buttons();
        let campaign = Self::is_campaign(dialog);
        let dialog_gd = dialog.to_gd();
        let mut button = if campaign && net_client() {
            // `if(net.client()) buttons.button("@gameover.disconnect", …)`
            // (`GameOverDialog.java:105-111`): a multiplayer campaign client
            // cannot continue the map alone, so the only campaign action is
            // leaving the match back to the menu.
            dialog.add_button(
                &kit::t(GAMEOVER_DISCONNECT_KEY).to_string(),
                Some(dialog_press(
                    "gameover_disconnect",
                    &dialog_gd,
                    disconnect_pressed,
                )),
                "",
                170.0,
            )
        } else if campaign {
            dialog.add_button(
                &kit::t("@continue").to_string(),
                Some(dialog_press(
                    "gameover_continue",
                    &dialog_gd,
                    continue_pressed,
                )),
                "map",
                170.0,
            )
        } else {
            dialog.add_button(
                &kit::t("@menu").to_string(),
                Some(dialog_press("gameover_menu", &dialog_gd, menu_pressed)),
                "",
                140.0,
            )
        };
        let mut min = button.get_custom_minimum_size();
        min.y = 60.0;
        button.set_custom_minimum_size(min);
        button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        // `buttons.margin(10)` (`GameOverDialog.java:47`): a 10 px margin around
        // the 60 px button row.
        if let Some(mut buttons) = dialog.buttons_row() {
            let mut size = buttons.get_custom_minimum_size();
            size.y = 80.0;
            buttons.set_custom_minimum_size(size);
        }
    }

    /// One `StatLabel` (`GameOverDialog.java:143-184`): a 50 px `Tex.whiteui`
    /// bar tinted `Pal.accent`, name left / value right, scaled in from x=0 after
    /// `0.05 * i` s while the tint animates to `Pal.darkestGray` over 0.3 s
    /// (`pow3Out`); the value then counts up from 0 over 60 frames — linearly
    /// below 10, `Interp.slowFast` (= `pow2In` = `a * a`) above.
    fn add_stat_row(
        parent: &mut Gd<VBoxContainer>,
        stats: &VarDictionary,
        key: &str,
        field: &str,
        index: usize,
    ) {
        // code-instantiated: one row per GameStats entry (T16).
        let mut panel = PanelContainer::new_alloc();
        panel.set_custom_minimum_size(Vector2::new(0.0, 50.0));
        let mut bar = StyleBoxFlat::new_gd();
        bar.set_bg_color(styles::ACCENT);
        bar.set_content_margin(Side::LEFT, 5.0);
        bar.set_content_margin(Side::RIGHT, 5.0);
        panel.add_theme_stylebox_override("panel", &bar);
        panel.set_pivot_offset(Vector2::ZERO);
        panel.set_scale(Vector2::new(0.0, 1.0));
        let mut row = HBoxContainer::new_alloc();
        let mut name_label =
            factory::styled_label(&kit::t(&format!("@{key}")).to_string(), "outlineLabel");
        name_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        // `StatLabel` cells centre their labels in the 50 px bar; a filled HBox
        // child top-aligned the text.
        name_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        row.add_child(&name_label);
        let mut value_label = factory::styled_label("0", "outlineLabel");
        value_label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        value_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        row.add_child(&value_label);
        panel.add_child(&row);
        parent.add_child(&padded(&panel, 5.0));
        let target = stats
            .get(field)
            .map(|value| variant_f64(&value))
            .unwrap_or(0.0);
        let delay = 0.05 * index as f64;
        // Scale + colour: `parallel(scaleTo(1, 1, 0.3f, pow3Out), color(...))`.
        let mut intro = panel.create_tween();
        intro.tween_interval(delay);
        intro.set_trans(TransitionType::CUBIC);
        intro.set_ease(EaseType::OUT);
        intro.tween_property(&panel, "scale:x", &1.0f32.to_variant(), 0.3);
        let tint_target = bar.clone();
        let tint = Callable::from_fn("gameover_row_tint", move |args: &[&Variant]| {
            let weight = args.first().map(|value| variant_f64(value)).unwrap_or(0.0);
            let mut bar = tint_target.clone();
            bar.set_bg_color(styles::ACCENT.lerp(DARKEST_GRAY, weight));
            Variant::nil()
        });
        let mut parallel = intro.parallel();
        parallel.tween_method(&tint, &0.0f64.to_variant(), &1.0f64.to_variant(), 0.3);
        // Count-up: `sequence(delay(0.3f), run(...))` after the same initial
        // delay.
        let mut counter = panel.create_tween();
        counter.tween_interval(delay + 0.3);
        if target >= 10.0 {
            counter.set_trans(TransitionType::QUAD);
            counter.set_ease(EaseType::IN);
        } else {
            counter.set_trans(TransitionType::LINEAR);
            counter.set_ease(EaseType::IN_OUT);
        }
        let value_target = value_label.clone();
        let count = Callable::from_fn("gameover_row_count", move |args: &[&Variant]| {
            let value = args.first().map(|value| variant_f64(value)).unwrap_or(0.0);
            let mut label = value_target.clone();
            label.set_text(&format!("{}", value as i64));
            Variant::nil()
        });
        counter.tween_method(&count, &0.0f64.to_variant(), &target.to_variant(), 1.0);
    }

    /// `ui.hudfrag.shown = false/true`: hide the HUD fragments that are visible
    /// now and restore exactly those afterwards (the menu path hides the HUD
    /// group itself, so nothing is re-shown over the menu).
    fn hud_set(&mut self, hud_visible: bool) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("hud_set_visible", &[hud_visible.to_variant()]);
        }
        let Some(group) = kit::node("/root/Spine/Ui/UiRoot/HudGroup") else {
            return;
        };
        if !hud_visible {
            self.hidden_hud.clear();
            for child in group.get_children().iter_shared() {
                let Ok(mut canvas) = child.clone().try_cast::<CanvasItem>() else {
                    continue;
                };
                if canvas.is_visible() {
                    canvas.set_visible(false);
                    self.hidden_hud.push(canvas);
                }
            }
            return;
        }
        for canvas in std::mem::take(&mut self.hidden_hud) {
            if !canvas.is_instance_valid() {
                continue;
            }
            let mut canvas = canvas;
            canvas.set_visible(true);
        }
    }
}

/// `cont.pane(p -> ... p.margin(13f).setBackground(Styles.black3))`: the stats
/// pane, `Tex.black3` over the dialog backdrop with a 13 px inner margin.
fn stats_pane() -> Gd<PanelContainer> {
    // code-instantiated: the pane's rows come from the stats context.
    let mut pane = PanelContainer::new_alloc();
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(styles::BLACK_3);
    box_.set_content_margin(Side::LEFT, 13.0);
    box_.set_content_margin(Side::RIGHT, 13.0);
    box_.set_content_margin(Side::TOP, 13.0);
    box_.set_content_margin(Side::BOTTOM, 13.0);
    pane.add_theme_stylebox_override("panel", &box_);
    let mut rows = VBoxContainer::new_alloc();
    rows.add_theme_constant_override("separation", 0);
    pane.add_child(&rows);
    pane
}

/// An expanding spacer (the grown stats table's free space, centring its rows).
fn filler() -> Gd<Control> {
    // code-instantiated: layout filler around the data-built stat rows.
    let mut filler = Control::new_alloc();
    filler.set_v_size_flags(SizeFlags::EXPAND_FILL);
    filler.set_mouse_filter(MouseFilter::IGNORE);
    filler
}

/// The headline label (`MindWidgets.styled_label(_headline(), "techLabel")`).
fn headline_label(text: &str) -> Gd<MindLabel> {
    let mut label = factory::styled_label(text, "techLabel");
    label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    label
}

/// `pt.add(new FLabel(...)).left().pad(5).growX()` + `"[accent]" + playtime`.
fn playtime_row(millis: i64) -> Gd<Control> {
    // code-instantiated: the row follows the current save slot's playtime.
    let mut row = HBoxContainer::new_alloc();
    let mut name_label =
        factory::styled_label(&kit::t("@stats.playtime").to_string(), "outlineLabel");
    name_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    row.add_child(&name_label);
    let markup = kit::markup_text(&format!("[accent]{}", format_millis(millis)));
    let mut value_label = factory::styled_label(&markup.to_string(), "outlineLabel");
    value_label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    row.add_child(&value_label);
    padded(&row, 5.0).upcast()
}

/// `SaveSlot.getPlayTime()` through the campaign facade; `-1` when no slot is
/// current (Java hides the row then).
fn current_playtime_ms() -> i64 {
    campaign_call("current_save_playtime_ms", &[])
        .map(|value| variant_i64(&value))
        .unwrap_or(-1)
}

/// `GameOverDialog.java:112-129`: campaign continues to the planet map, then
/// (non-client only) shows the one-time difficulty notice when the sector has
/// already been attempted twice on `normal`.
fn continue_pressed(dialog: &mut MindDialog) {
    dialog.close();
    open_named_dialog("planet", "");
    if let Some(planet) = difficulty_guide_planet() {
        // `Time.runTask(10f, …)` after `ui.planet.show()`: build the notice on
        // the next idle so the planet page is laid out first.
        defer_action("gameover_difficulty_guide", move || {
            show_difficulty_guide(&planet);
        });
    }
}

/// `GameOverDialog.java:105-111`: the campaign client leaves the match and
/// returns to the menu (`logic.reset(); net.reset(); hide(); state.set(menu)`).
fn disconnect_pressed(dialog: &mut MindDialog) {
    dialog.close();
    // `net.reset()`: the relay session/leaders are scoped to the match.
    if let Some(mut net) = kit::net()
        && net.has_method("leave_match")
    {
        let _ = net.call("leave_match", &[]);
    }
    // `logic.reset()` + menu through the single return-to-menu path.
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("return_to_menu")
    {
        let _ = root.call("return_to_menu", &[]);
    }
}

/// `net.client()`: an active match that this node does not host. The port has
/// no `net.server()`; `MindNet.local_admin()` is the host flag
/// (`ModsDialog`/`Call` gating uses it).
fn net_client() -> bool {
    if !net_active() {
        return false;
    }
    let Some(mut net) = kit::net() else {
        return false;
    };
    if !net.has_method("local_admin") {
        return true;
    }
    !net.call("local_admin", &[])
        .try_to::<bool>()
        .unwrap_or(false)
}

/// `Core.settings.getBoolOnce(key)` (`arc Settings.getBoolOnce`): read the flag,
/// write `true`, and report whether this was the first read.
fn settings_flag_once(key: &str) -> bool {
    let seen = ui_call("settings_get_flag", &[GString::from(key).to_variant()])
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false);
    if !seen {
        let _ = ui_call(
            "settings_put_flag",
            &[GString::from(key).to_variant(), true.to_variant()],
        );
    }
    !seen
}

/// `GameOverDialog.java:115` gate for the one-time difficulty notice:
/// `allowCampaignRules && attempts >= 2 && difficulty == normal &&
/// Core.settings.getBoolOnce("difficultyguide")`.
fn difficulty_guide_should_show(
    allow_campaign_rules: bool,
    attempts: i64,
    difficulty: &str,
    first_read: bool,
) -> bool {
    allow_campaign_rules && attempts >= 2 && difficulty == "normal" && first_read
}

/// `campaign_views_json(planet)` through the campaign facade (`{}` when absent).
fn campaign_views_for(planet: &str) -> VarDictionary {
    campaign_call("campaign_views_json", &[GString::from(planet).to_variant()])
        .and_then(|value| value.try_to::<GString>().ok())
        .and_then(|text| Json::parse_string(&text).try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// The object rows of one `campaign_views_json` array field.
fn view_rows(views: &VarDictionary, key: &str) -> Vec<VarDictionary> {
    views
        .get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .map(|rows| {
            rows.iter_shared()
                .filter_map(|row| row.try_to::<VarDictionary>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Resolves the one-time difficulty notice for the sector that just ended.
///
/// Java reads `prev.planet.allowCampaignRules`, `prev.info.attempts`,
/// `prev.planet.campaignRules.difficulty` and the `difficultyguide` setting;
/// the port has them in the `campaign_views_json` projection (`planets`,
/// `sectors[].stats.attempts`, `rules.difficulty`) plus `get_sector_state` for
/// the active planet/sector. Returns the planet name to pass to
/// `CampaignRulesDialog` (`ui.campaignRules.show(prev.planet)`).
fn difficulty_guide_planet() -> Option<String> {
    let state = sector_state();
    if !dict_bool(&state, "campaign", false) {
        return None;
    }
    let planet = dict_string(&state, "planet");
    let sector = dict_i64(&state, "sector", -1);
    if planet.is_empty() || sector < 0 {
        return None;
    }
    // A campaign client never runs the `@continue` arm (`GameOverDialog.java:105`).
    if net_client() {
        return None;
    }
    let views = campaign_views_for(&planet);
    let allow_campaign_rules = view_rows(&views, "planets")
        .iter()
        .find(|row| dict_string(row, "name") == planet)
        .map(|row| dict_bool(row, "allow_campaign_rules", false))
        .unwrap_or(false);
    let attempts = view_rows(&views, "sectors")
        .iter()
        .find(|row| dict_i64(row, "id", -1) == sector)
        .map(|row| {
            row.get("stats")
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .map(|stats| dict_i64(&stats, "attempts", 0))
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let difficulty = views
        .get("rules")
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .map(|rules| dict_string(&rules, "difficulty"))
        .unwrap_or_default();
    // Java's `&&` evaluates left to right: getBoolOnce only writes when the
    // other three gates pass.
    if !allow_campaign_rules || attempts < 2 || difficulty != "normal" {
        return None;
    }
    let first_read = settings_flag_once(DIFFICULTY_GUIDE_SETTING);
    difficulty_guide_should_show(allow_campaign_rules, attempts, &difficulty, first_read)
        .then_some(planet)
}

/// `GameOverDialog.java:117-128`: `new BaseDialog("@difficulty.guide.title")`
/// with a 500 px (400 mobile) wrapped statement and two 200x60 buttons —
/// `@hide` and `@difficulty.guide.confirm` (opens `CampaignRulesDialog`).
///
/// The port registers no dialog scene for this one-time notice, so the shared
/// `mind_dialog_base.tscn` shell is instantiated directly (the same node every
/// manifest dialog inherits) and lives under `UiRoot/DialogLayer` until it is
/// replaced on a later game over.
fn show_difficulty_guide(planet: &str) {
    let Some(mut layer) = kit::node("/root/Spine/Ui/UiRoot/DialogLayer") else {
        return;
    };
    if let Some(mut previous) = layer.get_node_or_null("difficulty_guide") {
        previous.queue_free();
    }
    let Ok(scene) =
        godot::tools::try_load::<godot::classes::PackedScene>(&GString::from(DIALOG_BASE_SCENE))
    else {
        return;
    };
    let Some(mut node) = scene.instantiate() else {
        return;
    };
    let Ok(dialog) = node.clone().try_cast::<MindDialog>() else {
        return;
    };
    node.set_name("difficulty_guide");
    layer.add_child(&node);
    let width = if is_mobile() { 400.0 } else { 500.0 };
    let dialog_gd = dialog.clone();
    let mut dialog_node = dialog_gd.clone();
    {
        let mut bound = dialog_node.bind_mut();
        // `BaseDialog("@difficulty.guide.title")` + `setFillParent(false)`.
        bound.set_title_key(DIFFICULTY_GUIDE_TITLE_KEY);
        bound.should_pause = false;
        bound.clear_content();
        bound.clear_buttons();
        if let Some(mut panel) = bound.panel() {
            panel.set_custom_minimum_size(Vector2::new(width + 40.0, 0.0));
        }
        let mut table = bound.content_table();
        // code-instantiated: the notice body is a bundle string.
        let label = factory::label(&kit::tm(DIFFICULTY_GUIDE_KEY).to_string());
        {
            let mut rich: Gd<RichTextLabel> = label.clone().upcast();
            rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
            rich.set_horizontal_alignment(HorizontalAlignment::CENTER);
            rich.set_custom_minimum_size(Vector2::new(width, 0.0));
        }
        // `dialog.cont.add("@difficulty.guide").width(...).wrap().pad(10f)`
        table.bind_mut().add(&label).pad(10.0);
        // `buttons.defaults().size(200f, 60f).pad(2f)`
        let hide_target = dialog_gd.clone();
        let hide = Callable::from_fn("difficulty_guide_hide", move |_args: &[&Variant]| {
            if hide_target.is_instance_valid() {
                let mut target = hide_target.clone();
                target.bind_mut().hide_dialog();
            }
            Variant::nil()
        });
        let mut hide_button =
            bound.add_button(&kit::t("@hide").to_string(), Some(hide), "cancel", 200.0);
        hide_button.set_custom_minimum_size(Vector2::new(200.0, 60.0));
        let confirm_target = dialog_gd.clone();
        let planet = planet.to_string();
        let confirm = Callable::from_fn("difficulty_guide_confirm", move |_args: &[&Variant]| {
            if confirm_target.is_instance_valid() {
                let mut target = confirm_target.clone();
                target.bind_mut().hide_dialog();
            }
            // `Vars.ui.campaignRules.show(prev.planet)`.
            let mut context = VarDictionary::new();
            context.set("planet", &GString::from(planet.as_str()).to_variant());
            if let Some(mut ui) = kit::ui() {
                let _ = ui.call(
                    "open_dialog",
                    &[
                        GString::from("campaign_rules").to_variant(),
                        Json::stringify(&context.to_variant()).to_variant(),
                    ],
                );
            }
            Variant::nil()
        });
        let mut confirm_button = bound.add_button(
            &kit::t(DIFFICULTY_GUIDE_CONFIRM_KEY).to_string(),
            Some(confirm),
            "book",
            200.0,
        );
        confirm_button.set_custom_minimum_size(Vector2::new(200.0, 60.0));
    }
    dialog_node.bind_mut().show_dialog();
}

/// Non-campaign `@menu` (`GameOverDialog.java:134-138`):
/// `if(!ui.paused.checkPlaytest()) logic.reset();` and the standalone menu.
fn menu_pressed(dialog: &mut MindDialog) {
    dialog.close();
    // A playtest that ended in game over returns to the editor.
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("check_playtest")
        && root
            .call("check_playtest", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    {
        return;
    }
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("return_to_menu")
    {
        let _ = root.call("return_to_menu", &[]);
    }
}

// ---------------------------------------------------------------------------
// Join / host / load / save shared plumbing (I08)
// ---------------------------------------------------------------------------

/// `Core.settings` `name` key (`JoinDialog.java:341`, `HostDialog.java:24`).
const PLAYER_NAME_SETTING: &str = "name";
/// `Core.settings` `color-0` key (`JoinDialog.java:349`).
const PLAYER_COLOR_SETTING: &str = "color-0";
/// `maxLength(40)` on the player-name fields (`JoinDialog.java:341`).
const PLAYER_NAME_MAX_LENGTH: i32 = 40;
/// Java `Core.settings.getJson("servers")` key (`JoinDialog.java:691`).
const SERVERS_SETTING: &str = "servers";
/// The add panel's remembered address (`JoinDialog.java:83`).
const IP_SETTING: &str = "ip";
/// The local relay address (`tools/mp-local.sh up`); Java's LAN discovery has
/// no STDB equivalent, so "local" is the default server (`JoinDialog.java:418-424`).
const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:3000/mindustry";
const DEFAULT_DB_NAME: &str = "mindustry";
const DEFAULT_SERVER_PORT: i64 = 3000;
/// Probe rows re-poll every 10 frames at 60 Hz (the transport is async).
const PROBE_POLL_SECONDS: f64 = 10.0 / 60.0;

/// Per-controller generation keys.
///
/// A lasting handler connected to `MindUi` (or to the shared picker/file
/// chooser) captures the controller state. `ready()` runs again on an extension
/// reload and installs a new controller, so every handler also checks the
/// generation stamped on its node: the previous generation's handlers retire
/// instead of mutating stale state.
const JOIN_GENERATION_META: &str = "_mind_join_generation";

/// Calls one `MindUi` method (`None` when the facade or method is absent).
fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Whether `MindUi` exposes `method`.
fn ui_has_method(method: &str) -> bool {
    kit::ui().map(|ui| ui.has_method(method)).unwrap_or(false)
}

/// `Core.settings.getString(key, default)` (`None` without `MindUi`).
fn settings_text(key: &str, default: &str) -> Option<String> {
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
fn settings_put_text(key: &str, value: &str) {
    let _ = ui_call(
        "settings_put_text",
        &[
            GString::from(key).to_variant(),
            GString::from(value).to_variant(),
        ],
    );
}

/// `Core.settings.getInt(key, default)` (`None` without `MindUi`).
fn settings_int(key: &str, default: i64) -> Option<i64> {
    ui_call(
        "settings_get_int",
        &[GString::from(key).to_variant(), default.to_variant()],
    )
    .map(|value| variant_i64(&value))
}

/// `Core.settings.put(key, value)` for an integer.
fn settings_put_int(key: &str, value: i64) {
    let _ = ui_call(
        "settings_put_int",
        &[GString::from(key).to_variant(), value.to_variant()],
    );
}

/// `Core.settings.getBool(key)` (`None` without `MindUi`).
fn settings_bool(key: &str) -> Option<bool> {
    ui_call("settings_get_bool", &[GString::from(key).to_variant()])
        .map(|value| variant_bool(&value))
}

/// `Core.settings.getString("name")` (`JoinDialog.java:341`).
fn stored_player_name() -> String {
    settings_text(PLAYER_NAME_SETTING, "").unwrap_or_default()
}

/// `Core.settings.getInt("color-0")` as `Color.rgba8888()`; unset (0) keeps the
/// palette default (`JoinDialog.java:349`).
fn stored_player_color() -> Color {
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
fn color_rgba32(color: Color) -> i64 {
    let channel = |value: f32| -> i64 { (value.clamp(0.0, 1.0) * 255.0) as i64 };
    (channel(color.r) << 24) | (channel(color.g) << 16) | (channel(color.b) << 8) | channel(color.a)
}

/// Pushes `player.name`/`player.color` to the relay identity reducers
/// (`MindNet.set_player_identity`; the facade queues them on connect).
fn push_player_identity(name: &str, color: Color) {
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
fn close_named_dialog(id: &str) -> bool {
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
fn hide_dialog_node(dialog: &Gd<MindDialog>) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let _ = node.call("hide_dialog", &[]);
}

/// Emits a `MindDialog` signal from a controller callback.
fn emit_dialog_signal(dialog: &Gd<MindDialog>, signal: &str, args: &[Variant]) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let _ = node.emit_signal(signal, args);
}

/// Stamps and returns the next controller generation on `dialog`.
fn next_generation(dialog: &Gd<MindDialog>, key: &str) -> i64 {
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
fn generation_live(dialog: &Gd<MindDialog>, key: &str, generation: i64) -> bool {
    dialog.is_instance_valid()
        && dialog.has_meta(key)
        && variant_i64(&dialog.get_meta(key)) == generation
}

/// A dictionary value coerced to text, with a fallback (`str(dict.get(key, d))`).
fn dict_string_or(dict: &VarDictionary, key: &str, default: &str) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_else(|| default.to_string())
}

// ---------------------------------------------------------------------------
// JoinDialogController (manifest id `join`)
// ---------------------------------------------------------------------------

/// One persisted server (`Java JoinDialog.Server` shape, `{"ip","port"}`).
#[derive(Clone, Default)]
struct JoinServer {
    ip: String,
    port: i64,
}

/// `JoinDialog` state: the persisted servers, the header/search handles and the
/// last rendered probe signature (`_servers`/`_server_signature`/…).
#[derive(Default)]
struct JoinState {
    /// Player colour shown in the header swatch (`player.color()`); seeded from
    /// `color-0` while the header row is built.
    color: Color,
    table: Option<Gd<MindTable>>,
    name_field: Option<Gd<LineEdit>>,
    swatch_tint: Option<Gd<ColorRect>>,
    add_panel: Option<Gd<VBoxContainer>>,
    ip_field: Option<Gd<LineEdit>>,
    local_body: Option<Gd<VBoxContainer>>,
    remote_body: Option<Gd<VBoxContainer>>,
    global_list: Option<Gd<MindTable>>,
    /// Header/search/body controls of the community section (`_global_nodes`).
    global_nodes: Vec<Gd<Control>>,
    /// Persisted servers (`[{"ip": String, "port": int}]`).
    servers: Vec<JoinServer>,
    /// Signature of the last rendered probe state (`_server_signature`).
    signature: String,
    /// Add-panel target: `-1` = append, else the server index being edited.
    editing_index: i64,
    /// Pending delete confirmation target (`_pending_delete`).
    pending_delete: i64,
    server_search: String,
    generation: i64,
}

impl Default for JoinDialogController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(JoinState::default())),
        }
    }
}

/// `JoinDialog` — server browser over SpacetimeDB addresses.
///
/// Saved servers persist under the Java `servers` settings JSON as
/// `[{"ip": ..., "port": N}]`; each row probes its address through
/// `MindNet.server_probe_json` and lists the public `all_matches` rows (Java's
/// `net.pingHost` + `setupServer`). "Community" is the active server's match
/// list: the port never fetches the Java community URLs because they serve Java
/// servers the relay cannot talk to (I08 deviation).
pub struct JoinDialogController {
    state: Rc<RefCell<JoinState>>,
}

impl DialogController for JoinDialogController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@joingame");
        dialog.should_pause = false;
        // `makeButtonOverlay()` (`JoinDialog.java:59`): the padded title cell.
        dialog.apply_button_overlay(true);
        dialog.enable_full_dialog(true);
        let dialog_gd = dialog.to_gd();
        self.state.borrow_mut().generation = next_generation(&dialog_gd, JOIN_GENERATION_META);
        join_load_servers(&self.state);
        join_build(dialog, &self.state);
        join_connect_confirm(&dialog_gd, &self.state);
        join_hook_picker(&dialog_gd, &self.state);
        join_install_poll(&dialog_gd, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let dialog_gd = dialog.to_gd();
        join_shown(&dialog_gd, &self.state);
    }
}

/// `JoinDialog._build`.
fn join_build(dialog: &mut MindDialog, state: &Rc<RefCell<JoinState>>) {
    dialog.clear_content();
    dialog.clear_buttons();
    let dialog_gd = dialog.to_gd();
    let mut table = dialog.content_table();
    table.add_theme_constant_override("separation", 8);
    state.borrow_mut().table = Some(table);
    join_build_header(&dialog_gd, state);
    join_build_add_panel(&dialog_gd, state);
    join_build_sections(&dialog_gd, state);

    let dialog_for_back = dialog_gd.clone();
    let back = Callable::from_fn("join_close", move |_args: &[&Variant]| {
        join_close(&dialog_for_back);
        Variant::nil()
    });
    dialog.add_button(&kit::t("@back").to_string(), Some(back), "left", 210.0);

    // `add_button(_t("@server.add"), _toggle_add, "add")` passes no width, so
    // the entry keeps its natural size (`MindWidgets.icon_button`).
    // code-instantiated: the add entry toggles the inline form at runtime.
    let mut add = factory::icon_button(
        "add",
        &kit::t("@server.add").to_string(),
        "defaultt",
        AlignmentMode::BEGIN,
    );
    {
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("join_toggle_add", move |_args: &[&Variant]| {
            join_toggle_add(&dialog_for, &state_for);
            Variant::nil()
        });
        add.connect("pressed", &callable);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&add);
    }

    let dialog_for_info = dialog_gd.clone();
    let info = Callable::from_fn("join_info", move |_args: &[&Variant]| {
        let _ = &dialog_for_info;
        join_show_info();
        Variant::nil()
    });
    dialog.add_button("?", Some(info), "", 60.0);
}

/// The `@name` header row: label, remembered name and the square colour swatch
/// (`JoinDialog.setup`).
fn join_build_header(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let name_hint = kit::t("@name").to_string();
    // code-instantiated: the name/colour header row is data-driven and has no
    // static scene counterpart.
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 6);
    header.add_child(&factory::label(&name_hint));
    let mut name_field = factory::field(&name_hint);
    name_field.set_max_length(PLAYER_NAME_MAX_LENGTH);
    // `JoinDialog.java:341`: the field opens with the remembered name.
    name_field.set_text(&GString::from(stored_player_name().as_str()));
    name_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("join_store_name", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| variant_string(value))
                .unwrap_or_default();
            join_store_name(&state_for, &text);
            Variant::nil()
        });
        name_field.connect("text_changed", &callable);
    }
    header.add_child(&name_field);

    // code-instantiated: the square colour swatch is tinted at runtime by the
    // colour chosen in the `picker` dialog.
    let mut swatch = Button::new_alloc();
    swatch.set_theme_type_variation("squarei");
    swatch.set_custom_minimum_size(Vector2::new(54.0, 54.0));
    let color = stored_player_color();
    let mut tint = ColorRect::new_alloc();
    tint.set_color(color);
    tint.set_mouse_filter(MouseFilter::IGNORE);
    let tint_control: Gd<Control> = tint.clone().upcast();
    swatch.add_child(&tint_control);
    tint.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    {
        let dialog_for = dialog.clone();
        let callable = Callable::from_fn("join_open_picker", move |_args: &[&Variant]| {
            open_named_dialog("picker", "{}");
            let _ = &dialog_for;
            Variant::nil()
        });
        swatch.connect("pressed", &callable);
    }
    header.add_child(&swatch);

    let mut st = state.borrow_mut();
    st.color = color;
    st.name_field = Some(name_field);
    st.swatch_tint = Some(tint);
    if let Some(mut table) = st.table.clone() {
        table.bind_mut().add(&header).grow_x().pad(4.0);
        table.bind_mut().row();
    }
}

/// The inline add/edit-server form (`JoinDialog.add`, `JoinDialog.java:77-112`).
fn join_build_add_panel(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let ip_hint = kit::t("@joingame.ip").to_string();
    // code-instantiated: the inline add-server form is revealed by `@server.add`
    // and toggled at runtime.
    let mut panel = VBoxContainer::new_alloc();
    panel.set_visible(false);
    panel.add_theme_constant_override("separation", 4);
    let mut ip_row = HBoxContainer::new_alloc();
    ip_row.add_theme_constant_override("separation", 6);
    ip_row.add_child(&factory::label(&ip_hint));
    let mut ip_field = factory::field(&ip_hint);
    ip_field.set_max_length(100);
    ip_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        // `JoinDialog.java:81-83`: the field remembers the last address typed.
        let callable = Callable::from_fn("join_store_ip", |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| variant_string(value))
                .unwrap_or_default();
            settings_put_text(IP_SETTING, &text);
            Variant::nil()
        });
        ip_field.connect("text_changed", &callable);
    }
    ip_row.add_child(&ip_field);
    panel.add_child(&ip_row);
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 6);
    let mut ok = factory::button(&kit::t("@ok").to_string());
    {
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("join_add_ok", move |_args: &[&Variant]| {
            join_submit_add(&dialog_for, &state_for);
            Variant::nil()
        });
        ok.connect("pressed", &callable);
    }
    actions.add_child(&ok);
    let mut cancel = factory::button(&kit::t("@cancel").to_string());
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("join_add_cancel", move |_args: &[&Variant]| {
            join_hide_add(&state_for);
            Variant::nil()
        });
        cancel.connect("pressed", &callable);
    }
    actions.add_child(&cancel);
    panel.add_child(&actions);
    let mut st = state.borrow_mut();
    st.add_panel = Some(panel.clone());
    st.ip_field = Some(ip_field);
    if let Some(mut table) = st.table.clone() {
        table.bind_mut().add(&panel).grow_x().pad(4.0);
        table.bind_mut().row();
    }
}

/// The three server groups (`JoinDialog.section`): local, remote, global.
fn join_build_sections(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    if let Some(local) = join_add_section(dialog, state, "@servers.local", false) {
        state.borrow_mut().local_body = Some(local);
    }
    if let Some(remote) = join_add_section(dialog, state, "@servers.remote", false) {
        state.borrow_mut().remote_body = Some(remote);
    }
    join_rebuild_servers(state, dialog);

    let first_global = state
        .borrow()
        .table
        .as_ref()
        .map(|table| table.get_child_count())
        .unwrap_or(0);
    if let Some(mut global) = join_add_section(dialog, state, "@servers.global", true) {
        // The community section's header/search/body nodes, hidden while
        // `communityservers` is off (`JoinDialog.java:329-331`).
        let mut nodes = Vec::new();
        if let Some(table) = state.borrow().table.clone() {
            let count = table.get_child_count();
            for index in first_global..count {
                if let Some(child) = table.get_child(index)
                    && let Ok(control) = child.try_cast::<Control>()
                {
                    nodes.push(control);
                }
            }
        }
        state.borrow_mut().global_nodes = nodes;
        // code-instantiated: the community body lists the active server's public
        // `all_matches` rows; it is Java's community list minus the remote fetch.
        let mut list = MindTable::new_alloc();
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        global.add_child(&list);
        state.borrow_mut().global_list = Some(list);
    }
    join_rebuild_public(dialog, state);
}

/// One section header (accent title + collapse glyph, optional search row) plus
/// its collapsible body container.
fn join_add_section(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    label_key: &str,
    with_search: bool,
) -> Option<Gd<VBoxContainer>> {
    // code-instantiated: section header + collapsible body repeat for the three
    // server groups and are toggled at runtime.
    let mut section_body = VBoxContainer::new_alloc();
    section_body.add_theme_constant_override("separation", 4);
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 4);
    let mut title = factory::styled_label(&kit::tm(label_key).to_string(), "outlineLabel");
    title.set_h_size_flags(SizeFlags::EXPAND_FILL);
    title.set_horizontal_alignment(HorizontalAlignment::LEFT);
    title.add_theme_color_override("default_color", styles::ACCENT);
    header.add_child(&title);
    let mut toggle = Button::new_alloc();
    toggle.set_theme_type_variation("emptyi");
    toggle.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    // code-instantiated: the collapse glyph swaps between up-open/down-open.
    let toggle_glyph = factory::glyph_label("up-open", 22, styles::ACCENT);
    toggle.add_child(&toggle_glyph);
    {
        let body_for = section_body.clone();
        let glyph_for = toggle_glyph.clone();
        let callable = Callable::from_fn("join_toggle_section", move |_args: &[&Variant]| {
            let mut body = body_for.clone();
            let visible = !body.is_visible();
            body.set_visible(visible);
            let mut glyph = glyph_for.clone();
            glyph.set_text(&kit::glyph(if visible { "up-open" } else { "down-open" }));
            Variant::nil()
        });
        toggle.connect("pressed", &callable);
    }
    header.add_child(&toggle);

    let mut table = state.borrow().table.clone()?;
    table.bind_mut().add(&header).grow_x().pad(4.0);
    table.bind_mut().row();

    if with_search {
        // code-instantiated: the community search/refresh row is data-driven
        // (`JoinDialog.java:417-424`).
        let search_hint = kit::t("@search").to_string();
        let mut search_row = HBoxContainer::new_alloc();
        search_row.add_theme_constant_override("separation", 6);
        search_row.add_child(&factory::label(&search_hint));
        let mut search = factory::field(&search_hint);
        search.set_max_length(50);
        search.set_h_size_flags(SizeFlags::EXPAND_FILL);
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_search", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| variant_string(value))
                    .unwrap_or_default();
                join_on_search_changed(&dialog_for, &state_for, &text);
                Variant::nil()
            });
            search.connect("text_changed", &callable);
        }
        search_row.add_child(&search);
        let mut zoom = factory::icon_button("zoom", "", "emptyi", AlignmentMode::CENTER);
        zoom.set_custom_minimum_size(Vector2::new(54.0, 54.0));
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_public_refresh", move |_args: &[&Variant]| {
                join_rebuild_public(&dialog_for, &state_for);
                Variant::nil()
            });
            zoom.connect("pressed", &callable);
        }
        search_row.add_child(&zoom);
        table.bind_mut().add(&search_row).grow_x().pad(4.0);
        table.bind_mut().row();
    }

    table.bind_mut().add(&section_body).grow_x().pad(4.0);
    table.bind_mut().row();
    Some(section_body)
}

/// Rebuilds both server sections from the live probe data.
fn join_rebuild_servers(state: &Rc<RefCell<JoinState>>, dialog: &Gd<MindDialog>) {
    let (local, remote, servers) = {
        let st = state.borrow();
        (
            st.local_body.clone(),
            st.remote_body.clone(),
            st.servers.clone(),
        )
    };
    if let Some(mut body) = local
        && body.is_instance_valid()
    {
        clear_container(&mut body);
        join_build_server_row(state, dialog, &mut body, DEFAULT_SERVER_ADDRESS, -1);
    }
    if let Some(mut body) = remote
        && body.is_instance_valid()
    {
        clear_container(&mut body);
        for (index, entry) in servers.iter().enumerate() {
            join_build_server_row(state, dialog, &mut body, &entry.ip, index as i64);
        }
    }
}

/// Removes + frees every child of a container (immediate rebuilds must not stack
/// deferred frees).
fn clear_container(container: &mut Gd<VBoxContainer>) {
    for child in container.get_children().iter_shared() {
        let mut child = child.clone();
        container.remove_child(&child);
        child.queue_free();
    }
}

/// One server row: address header + Java's action buttons
/// (`JoinDialog.java:150-204`) followed by the probe content.
fn join_build_server_row(
    state: &Rc<RefCell<JoinState>>,
    dialog: &Gd<MindDialog>,
    parent: &mut Gd<VBoxContainer>,
    address: &str,
    index: i64,
) {
    // code-instantiated: one server row per saved server plus the local default;
    // rows rebuild whenever a probe's state changes.
    let mut row = VBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 2);
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 4);
    let mut label = factory::styled_label(address, "outlineLabel");
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    label.add_theme_color_override("default_color", styles::ACCENT);
    header.add_child(&label);
    if index >= 0 {
        header.add_child(&join_row_action(
            "up-open",
            join_move_action(dialog, state, index, -1),
        ));
        header.add_child(&join_row_action(
            "down-open",
            join_move_action(dialog, state, index, 1),
        ));
    }
    header.add_child(&join_row_action(
        "refresh",
        join_refresh_action(dialog, state, address),
    ));
    if index >= 0 {
        header.add_child(&join_row_action(
            "pencil",
            join_edit_action(dialog, state, index),
        ));
        header.add_child(&join_row_action(
            "trash",
            join_remove_action(dialog, state, index),
        ));
    }
    row.add_child(&header);
    join_build_server_content(state, dialog, &mut row, address);
    parent.add_child(&row);
}

/// One row action button (`Styles.emptyi` + icon glyph).
fn join_row_action(icon: &str, listener: Callable) -> Gd<Button> {
    // code-instantiated: per-server row action buttons (reorder/refresh/edit/remove).
    let mut button = factory::icon_button(icon, "", "emptyi", AlignmentMode::CENTER);
    button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    button.connect("pressed", &listener);
    button
}

fn join_move_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
    direction: i64,
) -> Callable {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    Callable::from_fn("join_move_server", move |_args: &[&Variant]| {
        join_move_server(&dialog_for, &state_for, index, direction);
        Variant::nil()
    })
}

fn join_refresh_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    address: &str,
) -> Callable {
    let state_for = state.clone();
    let address_for = address.to_string();
    let _ = dialog;
    Callable::from_fn("join_refresh_server", move |_args: &[&Variant]| {
        join_refresh_server(&state_for, &address_for);
        Variant::nil()
    })
}

fn join_edit_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
) -> Callable {
    let state_for = state.clone();
    let _ = dialog;
    Callable::from_fn("join_edit_server", move |_args: &[&Variant]| {
        join_edit_server(&state_for, index);
        Variant::nil()
    })
}

fn join_remove_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
) -> Callable {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    Callable::from_fn("join_remove_server", move |_args: &[&Variant]| {
        join_remove_server(&dialog_for, &state_for, index);
        Variant::nil()
    })
}

/// Appends one server's probe content to its row.
fn join_build_server_content(
    state: &Rc<RefCell<JoinState>>,
    dialog: &Gd<MindDialog>,
    parent: &mut Gd<VBoxContainer>,
    address: &str,
) {
    let probe = parse_json_object(&join_probe_json(address));
    let state_name = dict_string_or(&probe, "state", "none");
    let ping = dict_i64(&probe, "ping_ms", -1);
    let matches = probe
        .get("matches")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    match state_name.as_str() {
        "ready" => {
            // `buildServer` shows the host's `getVersionString` (version and
            // the `@server.outdated` badge when the build differs; GAPS #45).
            if let Some(first) = matches
                .iter_shared()
                .find_map(|entry| entry.try_to::<VarDictionary>().ok())
            {
                let (build, protocol) = join_match_version(&first);
                let version = join_version_string(&build, protocol);
                if !version.is_empty() {
                    parent.add_child(&factory::styled_label(&version, "outlineLabel"));
                }
            }
            if ping > 0 {
                // `buildServer` shows `host.ping + "ms"` under the host info.
                parent.add_child(&factory::styled_label(&format!("{ping}ms"), "outlineLabel"));
            }
            if matches.is_empty() {
                parent.add_child(&factory::styled_label(
                    &kit::tm("@hosts.none").to_string(),
                    "outlineLabel",
                ));
            } else {
                for entry in matches.iter_shared() {
                    let Ok(row) = entry.try_to::<VarDictionary>() else {
                        continue;
                    };
                    let match_id = dict_i64(&row, "match_id", 0);
                    if match_id <= 0 {
                        continue;
                    }
                    let label = format!(
                        "{}  {}  {}/{}",
                        dict_string(&row, "map_id"),
                        dict_string(&row, "mode_name"),
                        dict_i64(&row, "player_count", 0),
                        dict_i64(&row, "max_players", 0),
                    );
                    let mut button = factory::button(&label);
                    let dialog_for = dialog.clone();
                    let state_for = state.clone();
                    let address_for = address.to_string();
                    let callable =
                        Callable::from_fn("join_server_match", move |_args: &[&Variant]| {
                            join_on_server(&dialog_for, &state_for, &address_for, match_id);
                            Variant::nil()
                        });
                    button.connect("pressed", &callable);
                    parent.add_child(&button);
                }
            }
        }
        "connecting" => {
            // `refreshServer`: `@server.refreshing` while the ping is in flight.
            parent.add_child(&factory::styled_label(
                &kit::tm("@server.refreshing").to_string(),
                "outlineLabel",
            ));
        }
        _ => {
            // `refreshServer` failure: `@host.invalid`.
            parent.add_child(&factory::styled_label(
                &kit::tm("@host.invalid").to_string(),
                "outlineLabel",
            ));
        }
    }
}

/// Global group body: the active server's public `all_matches` rows, filtered by
/// the community search field.
fn join_rebuild_public(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let list = state.borrow().global_list.clone();
    let Some(mut list) = list else {
        return;
    };
    if !list.is_instance_valid() {
        return;
    }
    clear_table_nodes(&mut list);
    let matches = join_public_matches();
    let search = state.borrow().server_search.clone();
    for row_variant in matches.iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let match_id = dict_i64(&row, "match_id", 0);
        if match_id <= 0 {
            continue;
        }
        let label = format!(
            "{}  {}  {}/{}",
            dict_string(&row, "map_id"),
            dict_string(&row, "mode_name"),
            dict_i64(&row, "player_count", 0),
            dict_i64(&row, "max_players", 0),
        );
        if !search.is_empty() && !label.to_lowercase().contains(&search) {
            continue;
        }
        // code-instantiated: public match rows come from the server view.
        let mut button = factory::button(&label);
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_public_match", move |_args: &[&Variant]| {
                join_selected(&dialog_for, &state_for, match_id);
                Variant::nil()
            });
            button.connect("pressed", &callable);
        }
        list.bind_mut().add(&button).grow_x().pad(3.0);
        list.bind_mut().row();
    }
    if list.get_child_count() == 0 {
        // code-instantiated: empty-state row carries the refresh control.
        let mut empty = HBoxContainer::new_alloc();
        empty.add_theme_constant_override("separation", 6);
        let mut empty_label =
            factory::styled_label(&kit::tm("@hosts.none").to_string(), "outlineLabel");
        empty_label.set_autowrap_mode(AutowrapMode::OFF);
        empty_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        empty.add_child(&empty_label);
        let mut refresh = factory::icon_button("refresh", "", "emptyi", AlignmentMode::CENTER);
        refresh.set_custom_minimum_size(Vector2::new(70.0, 70.0));
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_public_refresh", move |_args: &[&Variant]| {
                join_rebuild_public(&dialog_for, &state_for);
                Variant::nil()
            });
            refresh.connect("pressed", &callable);
        }
        empty.add_child(&refresh);
        list.bind_mut().add(&empty).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    list.bind_mut().sort_now();
    let _ = list.call_deferred("sort_now", &[]);
}

/// Removes + frees a table's children before a rebuild (the immediate
/// `MindTable.clear_children` the GDScript relied on for its empty-state test).
fn clear_table_nodes(table: &mut Gd<MindTable>) {
    for child in table.get_children().iter_shared() {
        let mut child = child.clone();
        table.remove_child(&child);
        child.queue_free();
    }
    table.bind_mut().clear();
}

/// Live `all_matches` rows from `MindNet` (empty when absent or offline).
fn join_public_matches() -> VarArray {
    let Some(mut net) = kit::net() else {
        return VarArray::new();
    };
    if !net.has_method("get_public_matches_json") {
        return VarArray::new();
    }
    let text = net
        .call("get_public_matches_json", &[])
        .try_to::<GString>()
        .unwrap_or_default();
    parse_json_array(&text.to_string())
}

/// Polls every row's probe and returns one signature over their rendered state.
fn join_poll_once(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    if !dialog.is_instance_valid() || !dialog.is_visible() {
        return;
    }
    let signature = {
        let st = state.borrow();
        join_signature(&st)
    };
    let changed = {
        let mut st = state.borrow_mut();
        if st.signature == signature {
            false
        } else {
            st.signature = signature;
            true
        }
    };
    if changed {
        join_rebuild_servers(state, dialog);
    }
}

/// The rendered probe signature (`_poll_server_signature`).
fn join_signature(st: &JoinState) -> String {
    let mut signature = format!(
        "{DEFAULT_SERVER_ADDRESS}={};",
        join_probe_json(DEFAULT_SERVER_ADDRESS)
    );
    for entry in &st.servers {
        signature.push_str(&format!("{}={};", entry.ip, join_probe_json(&entry.ip)));
    }
    signature
}

/// One address's probe state as compact JSON (empty when `MindNet` is absent).
fn join_probe_json(address: &str) -> String {
    let Some(mut net) = kit::net() else {
        return "{}".to_string();
    };
    if !net.has_method("server_probe_json") {
        return "{}".to_string();
    }
    net.call("server_probe_json", &[GString::from(address).to_variant()])
        .try_to::<GString>()
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// Server version gate (`JoinDialog.getVersionString`/`safeConnect`, GAPS #45)
// ---------------------------------------------------------------------------

/// `JoinDialog.getVersionString` classification (`JoinDialog.java:778-791`).
///
/// The relay exposes the numeric gate as `RelayMatch.protocol_version` and the
/// human version as `RelayMatch.build_id`; this client's `Version.build`
/// analogue is [`mind_stdb::PROTOCOL_VERSION`] plus [`join_client_build`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JoinVersion {
    /// Same build and protocol (`host.version == Version.build`, official).
    Same,
    /// Older host (`host.version < Version.build`).
    Older,
    /// Newer host (`host.version > Version.build`).
    Newer,
    /// No build advertised (`host.version == 0`).
    Unknown,
}

/// The rendered pieces of Java's `getVersionString` return value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct JoinVersionLine {
    /// `@server.version` arguments `(build, type)`; `None` when Java returns
    /// `""` (same build) or lists only the outdated badge.
    version: Option<(String, String)>,
    /// `@server.outdated` / `@server.outdated.client` bundle key.
    badge: Option<&'static str>,
}

/// The client build identity sent with every join (`MindNet.join_match` sends
/// `mind_stdb::MIND_VERSION` as the relay `build_id`).
fn join_client_build() -> &'static str {
    mind_stdb::MIND_VERSION
}

/// Numeric per-component version compare (`0.1.10 > 0.1.9`), like
/// `Version.build` ordering; non-numeric suffixes are ignored.
fn join_build_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let parts = |value: &str| -> Vec<i64> {
        value
            .trim()
            .split('.')
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| {
                let digits: String = chunk.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse().unwrap_or(0)
            })
            .collect()
    };
    let left_parts = parts(left);
    let right_parts = parts(right);
    for index in 0..left_parts.len().max(right_parts.len()) {
        let a = left_parts.get(index).copied().unwrap_or(0);
        let b = right_parts.get(index).copied().unwrap_or(0);
        match a.cmp(&b) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// Classifies one probe match against this client (`getVersionString`'s
/// branch order: `0`/older/newer/equal).
fn join_version(server_build: &str, server_protocol: i64) -> JoinVersion {
    let build = server_build.trim();
    if build.is_empty() || server_protocol == 0 {
        // `host.version == 0` (or an unversioned relay row): `@server.outdated`.
        return JoinVersion::Unknown;
    }
    let client_protocol = i64::from(mind_stdb::PROTOCOL_VERSION);
    if server_protocol != client_protocol {
        return if server_protocol < client_protocol {
            JoinVersion::Older
        } else {
            JoinVersion::Newer
        };
    }
    match join_build_cmp(build, join_client_build()) {
        std::cmp::Ordering::Equal => JoinVersion::Same,
        std::cmp::Ordering::Less => JoinVersion::Older,
        std::cmp::Ordering::Greater => JoinVersion::Newer,
    }
}

/// `JoinDialog.getVersionString` (`:778-791`) split into its display pieces.
fn join_version_line(server_build: &str, server_protocol: i64) -> JoinVersionLine {
    let build = server_build.trim().to_string();
    match join_version(server_build, server_protocol) {
        // Same build and official type: Java returns "" (nothing shown).
        JoinVersion::Same => JoinVersionLine::default(),
        JoinVersion::Unknown => JoinVersionLine {
            version: None,
            badge: Some("@server.outdated"),
        },
        // `@server.version` + `@server.outdated` (Java prepends a newline; the
        // caller lays the line out).
        JoinVersion::Older => JoinVersionLine {
            version: Some((build, String::new())),
            badge: Some("@server.outdated"),
        },
        JoinVersion::Newer => JoinVersionLine {
            version: Some((build, String::new())),
            badge: Some("@server.outdated.client"),
        },
    }
}

/// The `safeConnect` gate result (`JoinDialog.java:675-681`), `None` when the
/// versions match and the connect may proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct JoinSafeConnect {
    /// `server.kicked.clientOutdated`/`server.kicked.serverOutdated`.
    reason_key: &'static str,
    /// This client's build (`Version.build`).
    client_version: &'static str,
    /// The host's advertised build id (`host.version`).
    server_version: String,
}

/// `JoinDialog.safeConnect` version comparison: differing builds show the
/// scarlet kick reason plus `@server.versions` and never connect.
fn join_safe_connect(server_build: &str, server_protocol: i64) -> Option<JoinSafeConnect> {
    let state = join_version(server_build, server_protocol);
    if state == JoinVersion::Same {
        return None;
    }
    let reason_key = if state == JoinVersion::Newer {
        "@server.kicked.clientOutdated"
    } else {
        "@server.kicked.serverOutdated"
    };
    let server = server_build.trim();
    Some(JoinSafeConnect {
        reason_key,
        client_version: join_client_build(),
        server_version: if server.is_empty() {
            String::from("?")
        } else {
            server.to_string()
        },
    })
}

/// `getVersionString` rendered with bundle markup for a server row; empty when
/// the versions match (Java returns `""`).
fn join_version_string(server_build: &str, server_protocol: i64) -> String {
    let line = join_version_line(server_build, server_protocol);
    let mut out = String::new();
    if let Some((build, version_type)) = &line.version {
        out.push_str(
            &kit::tm_format("@server.version", &[build.as_str(), version_type.as_str()])
                .to_string(),
        );
    }
    if let Some(key) = line.badge {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&kit::tm(key).to_string());
    }
    out
}

/// Shows the `safeConnect` version modal (`ui.showInfo`) and returns whether
/// the connect must be aborted.
fn join_abort_on_version(server_build: &str, server_protocol: i64) -> bool {
    let Some(gate) = join_safe_connect(server_build, server_protocol) else {
        return false;
    };
    let text = format!(
        "[scarlet]{}[]\n{}",
        kit::t(gate.reason_key),
        kit::tm_format(
            "@server.versions",
            &[gate.client_version, gate.server_version.as_str()]
        )
    );
    if let Some(mut ui) = kit::ui()
        && ui.has_method("show_text")
    {
        let _ = ui.call(
            "show_text",
            &[
                GString::new().to_variant(),
                GString::from(text.as_str()).to_variant(),
            ],
        );
    }
    true
}

/// The first `RelayMatch` of an address's probe (`None` before the probe is
/// ready); carries `build_id`/`protocol_version` for the version gate.
fn join_probe_first_match(address: &str) -> Option<VarDictionary> {
    let probe = parse_json_object(&join_probe_json(address));
    probe
        .get("matches")
        .and_then(|value| value.try_to::<VarArray>().ok())?
        .iter_shared()
        .find_map(|entry| entry.try_to::<VarDictionary>().ok())
}

/// The `all_matches` row with `match_id` (public/browse connect path).
fn join_public_match(match_id: i64) -> Option<VarDictionary> {
    join_public_matches()
        .iter_shared()
        .filter_map(|entry| entry.try_to::<VarDictionary>().ok())
        .find(|row| dict_i64(row, "match_id", 0) == match_id)
}

/// `RelayMatch.build_id`/`protocol_version` of one row.
fn join_match_version(row: &VarDictionary) -> (String, i64) {
    (
        dict_string(row, "build_id"),
        dict_i64(row, "protocol_version", 0),
    )
}

/// Installs the probe poll (`_process`: every 10 frames while visible).
fn join_install_poll(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    // code-instantiated: the async server probes need a periodic poll; the
    // former GDScript used `_process` (the dialog class has no process hook).
    if let Some(mut old) = dialog.get_node_or_null("ProbePoll") {
        let mut node = dialog.clone();
        node.remove_child(&old);
        old.queue_free();
    }
    let mut timer = Timer::new_alloc();
    timer.set_name("ProbePoll");
    timer.set_wait_time(PROBE_POLL_SECONDS);
    timer.set_one_shot(false);
    timer.set_autostart(true);
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("join_probe_poll", move |_args: &[&Variant]| {
        join_poll_once(&dialog_for, &state_for);
        Variant::nil()
    });
    timer.connect("timeout", &callable);
    let mut node = dialog.clone();
    node.add_child(&timer);
}

/// `JoinDialog.shown`: rebuild rows and re-apply the community visibility.
fn join_shown(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    join_rebuild_servers(state, dialog);
    let community = join_community_servers();
    let nodes = state.borrow().global_nodes.clone();
    for node in nodes {
        if !node.is_instance_valid() {
            continue;
        }
        let mut node = node;
        node.set_visible(community);
    }
    if community {
        join_rebuild_public(dialog, state);
    }
}

/// `Core.settings.getBool("communityservers", true)` (Game settings row).
fn join_community_servers() -> bool {
    settings_bool("communityservers").unwrap_or(true)
}

/// `_on_search_changed`: trim, collapse spaces, lowercase (`JoinDialog.java:419-421`).
fn join_on_search_changed(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, text: &str) {
    let normalized = text
        .trim()
        .to_lowercase()
        .split(' ')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    state.borrow_mut().server_search = normalized;
    join_rebuild_public(dialog, state);
}

/// `JoinDialog.safeConnect` intent: join the picked public match.
fn join_selected(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, match_id: i64) {
    // `safeConnect(host.address, host.port, host.version)`: a differing build
    // shows the scarlet reason + `@server.versions` instead of joining.
    if let Some(row) = join_public_match(match_id) {
        let (build, protocol) = join_match_version(&row);
        if join_abort_on_version(&build, protocol) {
            return;
        }
    }
    join_push_identity(state);
    emit_dialog_signal(dialog, "join_requested", &[match_id.to_variant()]);
    join_close(dialog);
}

/// Clicking a match under a server row: connect first unless the connector
/// already points there (`JoinDialog.java:150-158`).
fn join_on_server(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    address: &str,
    match_id: i64,
) {
    if match_id <= 0 {
        return;
    }
    // `safeConnect(host.address, host.port, host.version)` (GAPS #45).
    if let Some(row) = join_probe_first_match(address) {
        let (build, protocol) = join_match_version(&row);
        if join_abort_on_version(&build, protocol) {
            return;
        }
    }
    join_push_identity(state);
    let mut current = String::new();
    if let Some(mut net) = kit::net()
        && net.has_method("current_server_address")
    {
        current = net
            .call("current_server_address", &[])
            .try_to::<GString>()
            .unwrap_or_default()
            .to_string();
    }
    if current != join_normalized_address(address) {
        emit_dialog_signal(
            dialog,
            "connect_requested",
            &[GString::from(address).to_variant()],
        );
    }
    emit_dialog_signal(dialog, "join_requested", &[match_id.to_variant()]);
    join_close(dialog);
}

/// Normalizes an address to `host[:port]/db` (no scheme), matching
/// `MindNet.current_server_address` and the Rust parser defaults.
fn join_normalized_address(address: &str) -> String {
    let value = address.trim();
    let value = value.strip_prefix("http://").unwrap_or(value);
    let value = value.strip_prefix("https://").unwrap_or(value);
    let (authority_part, database_part) = match value.split_once('/') {
        Some((authority, rest)) => (authority, rest.split('/').next().unwrap_or("")),
        None => (value, ""),
    };
    let mut authority = authority_part.to_string();
    let database = if database_part.is_empty() {
        DEFAULT_DB_NAME
    } else {
        database_part
    };
    if !authority.contains(':') {
        authority = format!("{authority}:{DEFAULT_SERVER_PORT}");
    }
    format!("{authority}/{database}")
}

/// `JoinDialog.java:83`: the add panel remembers the address field.
fn join_push_identity(state: &Rc<RefCell<JoinState>>) {
    let st = state.borrow();
    let Some(field) = st.name_field.clone() else {
        return;
    };
    let name = field.get_text().to_string();
    push_player_identity(&name, st.color);
}

/// `JoinDialog.java:342-346`: every name edit updates `player.name` and the
/// `name` setting immediately.
fn join_store_name(state: &Rc<RefCell<JoinState>>, text: &str) {
    settings_put_text(PLAYER_NAME_SETTING, text);
    join_push_identity(state);
}

/// `JoinDialog.java:348-354`: picking a colour updates `player.color()` and the
/// `color-0` setting immediately.
fn join_store_color(state: &Rc<RefCell<JoinState>>, color: Color) {
    {
        let mut st = state.borrow_mut();
        st.color = color;
        if let Some(mut tint) = st.swatch_tint.clone() {
            tint.set_color(color);
        }
    }
    settings_put_int(PLAYER_COLOR_SETTING, color_rgba32(color));
    join_push_identity(state);
}

/// Hooks the `picker` dialog's `color_selected` (once per controller generation).
fn join_hook_picker(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let Some(mut picker) = dialog_node("picker") else {
        return;
    };
    if !picker.has_signal("color_selected") {
        return;
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("join_color_selected", move |args: &[&Variant]| {
        let generation = state_for.borrow().generation;
        if !generation_live(&dialog_for, JOIN_GENERATION_META, generation) {
            return Variant::nil();
        }
        let color = args
            .first()
            .and_then(|value| value.try_to::<Color>().ok())
            .unwrap_or(Color::WHITE);
        join_store_color(&state_for, color);
        Variant::nil()
    });
    picker.connect("color_selected", &callable);
}

/// `JoinDialog._on_confirm_result`: answers the `@server.delete` confirm.
fn join_connect_confirm(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") {
        return;
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("join_confirm_result", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| variant_bool(value))
            .unwrap_or(false);
        let generation = state_for.borrow().generation;
        if !generation_live(&dialog_for, JOIN_GENERATION_META, generation) {
            return Variant::nil();
        }
        let index = {
            let mut st = state_for.borrow_mut();
            let index = st.pending_delete;
            st.pending_delete = -1;
            index
        };
        if confirmed {
            join_delete_server(&dialog_for, &state_for, index);
        }
        Variant::nil()
    });
    ui.connect("confirm_result", &callable);
}

/// `_toggle_add`: reveal the inline form (or hide it when already open).
fn join_toggle_add(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let (visible, panel, ip_field) = {
        let st = state.borrow();
        (
            st.add_panel
                .as_ref()
                .map(|panel| panel.is_visible())
                .unwrap_or(false),
            st.add_panel.clone(),
            st.ip_field.clone(),
        )
    };
    let Some(mut panel) = panel else {
        return;
    };
    if visible {
        join_hide_add(state);
        return;
    }
    state.borrow_mut().editing_index = -1;
    if let Some(text) = settings_text(IP_SETTING, "")
        && let Some(mut field) = ip_field.clone()
    {
        field.set_text(&GString::from(text.as_str()));
    }
    panel.set_visible(true);
    if let Some(mut field) = ip_field {
        field.grab_focus();
    }
    let _ = dialog;
}

/// `_edit_server`: open the form on an existing entry.
fn join_edit_server(state: &Rc<RefCell<JoinState>>, index: i64) {
    let (panel, field, ip) = {
        let st = state.borrow();
        if index < 0 || index >= st.servers.len() as i64 {
            return;
        }
        (
            st.add_panel.clone(),
            st.ip_field.clone(),
            st.servers[index as usize].ip.clone(),
        )
    };
    let Some(mut panel) = panel else {
        return;
    };
    state.borrow_mut().editing_index = index;
    if let Some(mut field) = field {
        field.set_text(&GString::from(ip.as_str()));
        field.grab_focus();
    }
    panel.set_visible(true);
}

/// `_hide_add`: collapse the inline form.
fn join_hide_add(state: &Rc<RefCell<JoinState>>) {
    let mut st = state.borrow_mut();
    if let Some(mut panel) = st.add_panel.clone() {
        panel.set_visible(false);
    }
    st.editing_index = -1;
}

/// `JoinDialog.java:92-101` OK handler: append or replace, save, rebuild rows.
fn join_submit_add(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let (address, editing) = {
        let st = state.borrow();
        (
            st.ip_field
                .as_ref()
                .map(|field| field.get_text().to_string().trim().to_string())
                .unwrap_or_default(),
            st.editing_index,
        )
    };
    if address.is_empty() {
        return;
    }
    {
        let mut st = state.borrow_mut();
        if editing >= 0 && editing < st.servers.len() as i64 {
            st.servers[editing as usize].ip = address;
        } else {
            st.servers.push(JoinServer {
                ip: address,
                port: 0,
            });
        }
    }
    join_save_servers(state);
    join_rebuild_servers(state, dialog);
    join_hide_add(state);
}

/// Java `moveRemote` (`JoinDialog.java:206-222`): reorder and persist.
fn join_move_server(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
    direction: i64,
) {
    {
        let mut st = state.borrow_mut();
        let target = index + direction;
        if index < 0 || target < 0 || target >= st.servers.len() as i64 {
            return;
        }
        let entry = st.servers.remove(index as usize);
        st.servers.insert(target as usize, entry);
    }
    join_save_servers(state);
    join_rebuild_servers(state, dialog);
}

/// Java confirm flow (`JoinDialog.java:194-202`) then remove + persist.
fn join_remove_server(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, index: i64) {
    if !ui_has_method("show_confirm") {
        join_delete_server(dialog, state, index);
        return;
    }
    state.borrow_mut().pending_delete = index;
    ui_show_confirm(&kit::t("@server.delete").to_string());
}

/// Removes the server at `index` and persists (`_delete_server`).
fn join_delete_server(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, index: i64) {
    {
        let mut st = state.borrow_mut();
        if index < 0 || index >= st.servers.len() as i64 {
            return;
        }
        st.servers.remove(index as usize);
    }
    join_save_servers(state);
    join_rebuild_servers(state, dialog);
}

/// `JoinDialog.refresh` row action: drop the probe so the next poll re-pings.
fn join_refresh_server(state: &Rc<RefCell<JoinState>>, address: &str) {
    if let Some(mut net) = kit::net()
        && net.has_method("refresh_server")
    {
        let _ = net.call("refresh_server", &[GString::from(address).to_variant()]);
    }
    // force the next poll to rebuild the rows
    state.borrow_mut().signature.clear();
}

/// `loadServers` (`JoinDialog.java:689-694`): `Core.settings.getJson("servers")`.
fn join_load_servers(state: &Rc<RefCell<JoinState>>) {
    let mut servers = Vec::new();
    if let Some(text) = settings_text(SERVERS_SETTING, "")
        && !text.is_empty()
    {
        for entry in parse_json_array(&text).iter_shared() {
            let Ok(row) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let ip = dict_string(&row, "ip");
            if ip.is_empty() {
                continue;
            }
            servers.push(JoinServer {
                ip,
                port: dict_i64(&row, "port", 0),
            });
        }
    }
    state.borrow_mut().servers = servers;
}

/// `saveServers` (`JoinDialog.java:773-775`): `Core.settings.putJson`.
fn join_save_servers(state: &Rc<RefCell<JoinState>>) {
    let servers = {
        let st = state.borrow();
        let mut array = VarArray::new();
        for entry in &st.servers {
            let mut row = VarDictionary::new();
            row.set("ip", &GString::from(entry.ip.as_str()).to_variant());
            row.set("port", &entry.port.to_variant());
            array.push(&row.to_variant());
        }
        Json::stringify(&array.to_variant()).to_string()
    };
    settings_put_text(SERVERS_SETTING, &servers);
}

/// `_show_info`: the `@join.info` help prompt.
fn join_show_info() {
    ui_show_info(&kit::t("@join.info").to_string());
}

/// Closes through `MindUi` so the dialog stack and pause governor stay in sync.
fn join_close(dialog: &Gd<MindDialog>) {
    if kit::ui().is_some() {
        let _ = close_named_dialog("join");
        return;
    }
    hide_dialog_node(dialog);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn build_compare_is_numeric_per_component() {
        assert_eq!(
            join_build_cmp("0.1.10", "0.1.9"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(join_build_cmp("0.1", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(join_build_cmp("1.2", "1.10"), std::cmp::Ordering::Less);
        // Non-numeric suffixes are ignored (`0.1.0-beta` == `0.1.0`).
        assert_eq!(
            join_build_cmp("0.1.0-beta", "0.1.0"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            join_build_cmp("", join_client_build()),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn version_gate_matches_get_version_string_branches() {
        // Same build + protocol: nothing shown and safeConnect proceeds.
        assert_eq!(
            join_version(join_client_build(), i64::from(mind_stdb::PROTOCOL_VERSION)),
            JoinVersion::Same
        );
        assert_eq!(
            join_safe_connect(join_client_build(), i64::from(mind_stdb::PROTOCOL_VERSION)),
            None
        );
        // No build / version 0: `@server.outdated` without a version line.
        assert_eq!(join_version("", 2), JoinVersion::Unknown);
        assert_eq!(join_version_line("", 2).badge, Some("@server.outdated"));
        assert!(join_version_line("", 2).version.is_none());
        assert_eq!(join_version("0.1.0", 0), JoinVersion::Unknown);
        // Older/newer host builds map to the two outdated badges.
        let older = join_version_line("0.0.9", i64::from(mind_stdb::PROTOCOL_VERSION));
        assert_eq!(
            older.version.as_ref().map(|(build, _)| build.as_str()),
            Some("0.0.9")
        );
        assert_eq!(older.badge, Some("@server.outdated"));
        let newer = join_version_line("9.9.9", i64::from(mind_stdb::PROTOCOL_VERSION));
        assert_eq!(newer.badge, Some("@server.outdated.client"));
        // The protocol is the numeric gate even when the build string matches.
        assert_eq!(
            join_version(
                join_client_build(),
                i64::from(mind_stdb::PROTOCOL_VERSION) - 1
            ),
            JoinVersion::Older
        );
        assert_eq!(
            join_version(
                join_client_build(),
                i64::from(mind_stdb::PROTOCOL_VERSION) + 1
            ),
            JoinVersion::Newer
        );
    }

    #[test]
    fn safe_connect_reports_the_kick_reason_and_both_versions() {
        let protocol = i64::from(mind_stdb::PROTOCOL_VERSION);
        let older = join_safe_connect("0.0.9", protocol).expect("older host must gate");
        assert_eq!(older.reason_key, "@server.kicked.serverOutdated");
        assert_eq!(older.client_version, join_client_build());
        assert_eq!(older.server_version, "0.0.9");
        let newer = join_safe_connect("9.9.9", protocol).expect("newer host must gate");
        assert_eq!(newer.reason_key, "@server.kicked.clientOutdated");
        // An unversioned host still fills `{1}` rather than leaving it blank.
        let unknown = join_safe_connect("", protocol).expect("unversioned host must gate");
        assert_eq!(unknown.reason_key, "@server.kicked.serverOutdated");
        assert_eq!(unknown.server_version, "?");
    }

    /// `GameOverDialog.java:115`: every clause of the difficulty-guide gate is
    /// required, and the `getBoolOnce` flag folds into `first_read`.
    #[test]
    fn difficulty_guide_gate_requires_all_java_clauses() {
        assert!(difficulty_guide_should_show(true, 2, "normal", true));
        // `allowCampaignRules`.
        assert!(!difficulty_guide_should_show(false, 2, "normal", true));
        // `info.attempts >= 2`.
        assert!(!difficulty_guide_should_show(true, 1, "normal", true));
        // `campaignRules.difficulty == Difficulty.normal`.
        assert!(!difficulty_guide_should_show(true, 5, "hard", true));
        assert!(!difficulty_guide_should_show(true, 5, "casual", true));
        // `Core.settings.getBoolOnce("difficultyguide")`: already read once.
        assert!(!difficulty_guide_should_show(true, 2, "normal", false));
    }
}
