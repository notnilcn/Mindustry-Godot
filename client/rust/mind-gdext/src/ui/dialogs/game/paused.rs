// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/PausedDialog.java (14-217).

//! The pause menu controller: the fixed desktop/mobile entry rows, the campaign
//! actions and the quit/abandon confirms.
//! Upstream: ui/dialogs/PausedDialog.java

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::SizeFlags;
use godot::classes::{Button, CenterContainer, Control, HBoxContainer, Json, Node, VBoxContainer};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::game::util_b::{
    campaign_call, current_slot_autosave, defer_dialog, dialog_node, dialog_press, dict_bool,
    dict_string, game_over_flag, is_mobile, net_active, open_named_dialog, parse_json_object,
    press, sector_state, set_button_text, stringify_object, ui_show_confirm, variant_bool,
    variant_string,
};
use crate::ui::kit;
use crate::ui::kit::widgets::factory;

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "paused" => Some(Box::new(PausedDialog::default())),
        _ => None,
    }
}

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
