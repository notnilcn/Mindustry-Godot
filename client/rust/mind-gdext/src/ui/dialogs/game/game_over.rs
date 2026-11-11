// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/GameOverDialog.java (43-184).

//! The game-over summary controller (`restart`): the animated stat block, the
//! campaign continue/disconnect actions and the one-time difficulty notice.
//! Upstream: ui/dialogs/GameOverDialog.java

use godot::builtin::Side;
use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{
    CanvasItem, Control, HBoxContainer, Json, PanelContainer, RichTextLabel, StyleBoxFlat,
    VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::gdutil::variant::{
    dict_bool_or, dict_i64_or, dict_text_or, to_bool_lenient, to_f64_lenient, to_i64_lenient,
};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::common::defer::defer_action;
use crate::ui::dialogs::common::facade::ui_call;
use crate::ui::dialogs::game::util::{
    STAT_ROWS, campaign_call, dialog_press, format_millis, is_mobile, net_active,
    open_named_dialog, padded, sector_state,
};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel, MindTable, styles};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        // The manifest name of the game-over scene (`GameOverDialog`).
        "restart" => Some(Box::new(GameOverDialog::default())),
        _ => None,
    }
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
            if field == "wave" && !dict_bool_or(&stats, "waves", false) {
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
                .map(|value| to_bool_lenient(&value))
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
            return to_bool_lenient(&value);
        }
        dict_bool_or(&sector_state(), "campaign", false)
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
            .map(|value| to_f64_lenient(&value))
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
            let weight = args
                .first()
                .map(|value| to_f64_lenient(value))
                .unwrap_or(0.0);
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
            let value = args
                .first()
                .map(|value| to_f64_lenient(value))
                .unwrap_or(0.0);
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
    let rows = factory::vbox(0);
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
        .map(|value| to_i64_lenient(&value))
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
    if !dict_bool_or(&state, "campaign", false) {
        return None;
    }
    let planet = dict_text_or(&state, "planet");
    let sector = dict_i64_or(&state, "sector", -1);
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
        .find(|row| dict_text_or(row, "name") == planet)
        .map(|row| dict_bool_or(row, "allow_campaign_rules", false))
        .unwrap_or(false);
    let attempts = view_rows(&views, "sectors")
        .iter()
        .find(|row| dict_i64_or(row, "id", -1) == sector)
        .map(|row| {
            row.get("stats")
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .map(|stats| dict_i64_or(&stats, "attempts", 0))
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let difficulty = views
        .get("rules")
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .map(|rules| dict_text_or(&rules, "difficulty"))
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

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
