// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/CampaignRulesDialog.java.

//! The campaign rules controller: the difficulty options and the per-planet
//! campaign rule toggles.
//! Upstream: ui/dialogs/CampaignRulesDialog.java

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use godot::builtin::{Corner, Side};
use godot::classes::control::SizeFlags;
use godot::classes::{
    Button, CenterContainer, PanelContainer, StyleBoxEmpty, StyleBoxFlat, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use super::util::{campaign_views_live, campaign_views_of};
use crate::gdutil::variant::{to_bool_lenient, to_text_lenient};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::game::util::campaign_call;
use crate::ui::dialogs::net::util::{clear_table_nodes, emit_dialog_signal};
use crate::ui::kit::widgets::check::MindCheck;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, Align, MindTable, styles};
use mind_core::ui::campaign::{CampaignRulesView, CampaignViews, PlanetView};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "campaign_rules" => Some(Box::new(CampaignRulesDialogController::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// CampaignRulesDialogController (manifest id `campaign_rules`)
// ---------------------------------------------------------------------------

/// Vanilla toggle keys in `CampaignRulesDialog.check` order.
const RULE_KEYS: [&str; 7] = [
    "sector_invasion",
    "fog",
    "hide_spawns",
    "random_wave_ai",
    "pause_disabled",
    "rts_ai",
    "clear_sector_on_lose",
];
/// The `Difficulty.all` order offered by the options box.
const DIFFICULTIES: [&str; 5] = ["casual", "easy", "normal", "hard", "eradication"];

/// Rule key -> bundle label; unknown keys fall back to the key itself.
fn rule_label(key: &str) -> &str {
    match key {
        "fog" => "@rules.fog",
        "hide_spawns" => "@rules.hidespawns",
        "sector_invasion" => "@rules.invasions",
        "random_wave_ai" => "@rules.randomwaveai",
        "rts_ai" => "@rules.rtsai.campaign",
        "pause_disabled" => "@rules.pauseDisabled",
        "clear_sector_on_lose" => "@rules.clearsectoronloss",
        other => other,
    }
}

/// `_difficulty_tooltip`'s multiplier table (`Difficulty.info`).
const DIFFICULTY_MODIFIERS: [(&str, [f64; 3]); 5] = [
    ("casual", [0.5, 0.5, 2.0]),
    ("easy", [1.0, 0.75, 1.5]),
    ("normal", [1.0, 1.0, 1.0]),
    ("hard", [1.25, 1.5, 0.8]),
    ("eradication", [1.5, 2.0, 0.6]),
];

/// The campaign-rules dialog state: the cached typed projection plus the
/// edited rule values.
struct CampaignRulesState {
    table: Option<Gd<MindTable>>,
    planet: String,
    difficulty: String,
    toggles: HashMap<String, bool>,
    /// The `campaign_views(planet)` projection this open reads from.
    views: Option<CampaignViews>,
}

impl Default for CampaignRulesState {
    fn default() -> Self {
        Self {
            table: None,
            planet: String::new(),
            difficulty: String::from("normal"),
            toggles: HashMap::new(),
            views: None,
        }
    }
}

/// `@campaign.difficulty` — the full-screen difficulty + campaign rule page.
///
/// Values come from the live `MindCampaign.get_campaign_rules(planet)` (WS2
/// contract) with the `campaign_views_json` rules section as fallback; every
/// change is emitted as `rule_changed` and written through
/// `set_campaign_rule(planet, key, value)`.
pub struct CampaignRulesDialogController {
    state: Rc<RefCell<CampaignRulesState>>,
}

impl Default for CampaignRulesDialogController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(CampaignRulesState::default())),
        }
    }
}

impl DialogController for CampaignRulesDialogController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `cont.top()` (`CampaignRulesDialog.java:42`): the body sticks to the
        // top (L01 T2).
        dialog.apply_cont_top(true);
        dialog.set_title_key("@campaign.difficulty");
        dialog.should_pause = false;
        // The Java BaseDialog is full-screen: title at the top, content from
        // the top, Back at the bottom centre (`CampaignRulesDialog` sits over
        // the planet page).
        dialog.enable_full_dialog(true);
        dialog.clear_content();
        dialog.clear_buttons();
        let table = dialog.content_table();
        self.state.borrow_mut().table = Some(table);
        let mut back = dialog.add_close_button(200.0);
        let mut size = back.get_custom_minimum_size();
        size.y = 60.0;
        back.set_custom_minimum_size(size);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        campaign_rules_shown(dialog, &self.state);
    }
}

/// `shown`: re-read the planet's rules and rebuild the page.
fn campaign_rules_shown(dialog: &mut MindDialog, state: &Rc<RefCell<CampaignRulesState>>) {
    let planet = match dialog.context_value("planet") {
        Some(value) => to_text_lenient(&value),
        None => {
            let cached = state.borrow().views.clone();
            match cached {
                Some(views) => views.planet,
                None => campaign_views_live(dialog)
                    .map(|views| views.planet)
                    .unwrap_or_default(),
            }
        }
    };
    // `refresh_campaign_views(_planet)`: the per-planet projection; keep the
    // existing cache when the facade is absent.
    if let Some(live_views) = campaign_views_of(dialog, &planet) {
        state.borrow_mut().views = Some(live_views);
    } else if state.borrow().views.is_none()
        && let Some(fallback) = campaign_views_live(dialog)
    {
        state.borrow_mut().views = Some(fallback);
    }
    let views = state.borrow().views.clone();
    // Live per-planet rules win over the read model's fixture values. The old
    // `MindCampaign.get_campaign_rules` dynamic call never resolved (no such
    // `#[func]`), so the typed projection is the only source now.
    let rules = views
        .as_ref()
        .map(|views| views.rules.clone())
        .unwrap_or(CampaignRulesView {
            planet: planet.clone(),
            difficulty: "normal",
            difficulties: Vec::new(),
            toggles: Vec::new(),
        });
    let difficulty = rules.difficulty.to_string();
    let mut toggles = HashMap::new();
    for toggle in &rules.toggles {
        toggles.insert(toggle.key.clone(), toggle.value);
    }
    if toggles.is_empty() {
        for key in RULE_KEYS {
            toggles.insert(key.to_string(), false);
        }
    }
    {
        let mut st = state.borrow_mut();
        st.planet = planet;
        st.difficulty = difficulty;
        st.toggles = toggles;
    }
    campaign_rules_rebuild(dialog, state);
}

/// `_planet_view_row`: the `campaign_views().planets` card for `planet_name`.
fn campaign_rules_planet_row(
    views: &Option<CampaignViews>,
    planet_name: &str,
) -> Option<PlanetView> {
    let views = views.as_ref()?;
    views
        .planets
        .iter()
        .find(|planet| planet.name == planet_name)
        .cloned()
}

/// `CampaignRulesDialog.rebuild` visibility gates: `sectorInvasion` only when
/// `allowSectorInvasion`, `rtsAI` only when `showRtsAIRule`,
/// `clearSectorOnLose` only when the flag is OFF (Erekir shows 4 rows, Serpulo
/// 7).
fn campaign_rules_rule_visible(key: &str, planet_row: &Option<PlanetView>) -> bool {
    let Some(planet_row) = planet_row.as_ref() else {
        return true;
    };
    match key {
        "sector_invasion" => planet_row.allow_sector_invasion,
        "rts_ai" => planet_row.show_rts_ai_rule,
        "clear_sector_on_lose" => !planet_row.clear_sector_on_lose,
        _ => true,
    }
}

/// The `Tex.button`-styled options box (black fill, 4 px #454545 outline).
fn campaign_rules_options_box() -> Gd<StyleBoxFlat> {
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::BLACK);
    style.set_border_width(Side::LEFT, 4);
    style.set_border_width(Side::TOP, 4);
    style.set_border_width(Side::RIGHT, 4);
    style.set_border_width(Side::BOTTOM, 4);
    style.set_border_color(styles::PANE_OUTLINE_GRAY);
    style.set_corner_radius(Corner::TOP_LEFT, 4);
    style.set_corner_radius(Corner::TOP_RIGHT, 4);
    style.set_corner_radius(Corner::BOTTOM_LEFT, 4);
    style.set_corner_radius(Corner::BOTTOM_RIGHT, 4);
    style.set_content_margin_all(8.0);
    style
}

/// The checked option's accent 4 px outline (reference frame: Normal selected).
fn campaign_rules_accent_box() -> Gd<StyleBoxFlat> {
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::BLACK);
    style.set_border_width(Side::LEFT, 4);
    style.set_border_width(Side::TOP, 4);
    style.set_border_width(Side::RIGHT, 4);
    style.set_border_width(Side::BOTTOM, 4);
    style.set_border_color(styles::ACCENT);
    style.set_corner_radius(Corner::TOP_LEFT, 4);
    style.set_corner_radius(Corner::TOP_RIGHT, 4);
    style.set_corner_radius(Corner::BOTTOM_LEFT, 4);
    style.set_corner_radius(Corner::BOTTOM_RIGHT, 4);
    style
}

/// `flatTogglet`: unselected options are bare labels; the checked one gets the
/// accent outline.
fn campaign_rules_apply_option_style(button: &mut Gd<Button>) {
    let empty = StyleBoxEmpty::new_gd();
    button.add_theme_stylebox_override("normal", &empty);
    button.add_theme_stylebox_override("hover", &empty);
    let accent = campaign_rules_accent_box();
    button.add_theme_stylebox_override("pressed", &accent);
    button.add_theme_stylebox_override("hover_pressed", &accent);
}

/// `Difficulty.percentStat`/`percentStatNeg` coloured delta ("" at 100%).
fn campaign_rules_percent_stat(value: f64, negated: bool) -> String {
    let delta = (value * 100.0 - 100.0) as i64;
    if delta == 0 {
        return String::new();
    }
    let tag = if (delta > 0) == negated {
        "[stat]"
    } else {
        "[negstat]"
    };
    format!("{tag}{delta:+}%[]")
}

/// Localized `Difficulty.info()` tooltip from the multiplier table.
fn campaign_rules_difficulty_tooltip(difficulty_name: &str) -> String {
    let values = DIFFICULTY_MODIFIERS
        .iter()
        .find(|(name, _)| *name == difficulty_name)
        .map(|(_, values)| *values)
        .unwrap_or([1.0, 1.0, 1.0]);
    let health = campaign_rules_percent_stat(values[0], false);
    let spawn = campaign_rules_percent_stat(values[1], false);
    let wave = campaign_rules_percent_stat(values[2], true);
    let mut lines: Vec<String> = Vec::new();
    if !health.is_empty() {
        lines.push(
            kit::tm_format("@difficulty.enemyHealthMultiplier", &[health.as_str()]).to_string(),
        );
    }
    if !spawn.is_empty() {
        lines.push(
            kit::tm_format("@difficulty.enemySpawnMultiplier", &[spawn.as_str()]).to_string(),
        );
    }
    if !wave.is_empty() {
        lines.push(kit::tm_format("@difficulty.waveTimeMultiplier", &[wave.as_str()]).to_string());
    }
    if lines.is_empty() {
        kit::t("@difficulty.nomodifiers").to_string()
    } else {
        lines.join("\n")
    }
}

/// `_rebuild`: the options box + the visible rule checks, centred under the
/// title.
fn campaign_rules_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<CampaignRulesState>>) {
    let table = state.borrow().table.clone();
    let Some(mut table) = table else {
        return;
    };
    clear_table_nodes(&mut table);
    let (planet, difficulty, toggles, views) = {
        let st = state.borrow();
        (
            st.planet.clone(),
            st.difficulty.clone(),
            st.toggles.clone(),
            st.views.clone(),
        )
    };
    let dialog_gd = dialog.to_gd();
    // code-instantiated: the centered option/rule column is data-driven.
    let mut column = VBoxContainer::new_alloc();
    column.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    column.add_theme_constant_override("separation", 8);
    // Java `current.table(Tex.button, ...)`: the five options live in a black,
    // 4 px #454545-bordered box, 140x50 each, centered (`CampaignRulesDialog`).
    let mut box_ = PanelContainer::new_alloc();
    box_.add_theme_stylebox_override("panel", &campaign_rules_options_box());
    let mut diff_row = factory::hbox(0);
    // code-instantiated: difficulty buttons are the data-driven Difficulty list.
    for difficulty_name in DIFFICULTIES {
        let mut button = Button::new_alloc();
        button.set_text(&kit::t(&format!("@difficulty.{difficulty_name}")));
        button.set_theme_type_variation("flatTogglet");
        button.set_toggle_mode(true);
        button.set_custom_minimum_size(Vector2::new(140.0, 50.0));
        button.set_pressed(difficulty_name == difficulty);
        let tooltip = campaign_rules_difficulty_tooltip(difficulty_name);
        button.set_tooltip_text(&GString::from(tooltip.as_str()));
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let name = difficulty_name.to_string();
        let callable =
            Callable::from_fn("campaign_rules_difficulty", move |_args: &[&Variant]| {
                campaign_rules_set_difficulty(&dialog_for, &state_for, &name);
                Variant::nil()
            });
        button.connect("pressed", &callable);
        campaign_rules_apply_option_style(&mut button);
        diff_row.add_child(&button);
    }
    box_.add_child(&diff_row);
    column.add_child(&box_);
    let planet_row = campaign_rules_planet_row(&views, &planet);
    for key in RULE_KEYS {
        // `CampaignRulesDialog.rebuild` gates three checks by planet flags.
        if !campaign_rules_rule_visible(key, &planet_row) {
            continue;
        }
        // code-instantiated: rule toggles come from the plan-12 CampaignRules set.
        let label = kit::t(rule_label(key)).to_string();
        let checked = toggles.get(key).copied().unwrap_or(false);
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let key_string = key.to_string();
        let callable = Callable::from_fn("campaign_rules_toggle", move |args: &[&Variant]| {
            let value = args
                .first()
                .map(|value| to_bool_lenient(value))
                .unwrap_or(false);
            campaign_rules_toggle(&dialog_for, &state_for, &key_string, value);
            Variant::nil()
        });
        let mut check = MindCheck::new_alloc();
        check
            .bind_mut()
            .setup(GString::from(label.as_str()), checked, callable);
        // Java `Elems.check` rows are content-sized (grayPanel strip behind the
        // indicator + label only), left-aligned under the options box.
        check.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
        column.add_child(&check);
    }
    // No `grow_x_axis` on the column itself: a CenterContainer fills the cell
    // and centers the content column (Java's table centers the box).
    let mut center = CenterContainer::new_alloc();
    center.add_child(&column);
    table
        .bind_mut()
        .add(&center)
        .grow_x()
        .align(Align::Center)
        .pad_top(14.0);
    table.bind_mut().row();
}

/// `_set_difficulty`: store, write through the facade and rebuild the page.
fn campaign_rules_set_difficulty(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<CampaignRulesState>>,
    difficulty_name: &str,
) {
    state.borrow_mut().difficulty = difficulty_name.to_string();
    campaign_rules_apply(
        dialog,
        state,
        "difficulty",
        GString::from(difficulty_name).to_variant(),
    );
    let mut node = dialog.clone();
    if !node.is_instance_valid() {
        return;
    }
    let mut bound = node.bind_mut();
    campaign_rules_rebuild(&mut bound, state);
}

/// `_toggle`: store one rule value and write it through the facade.
fn campaign_rules_toggle(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<CampaignRulesState>>,
    key: &str,
    value: bool,
) {
    state.borrow_mut().toggles.insert(key.to_string(), value);
    campaign_rules_apply(dialog, state, key, value.to_variant());
}

/// `_apply`: emits the change for listeners and writes it through the live
/// facade (`set_campaign_rule(planet, key, value)`).
fn campaign_rules_apply(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<CampaignRulesState>>,
    key: &str,
    value: Variant,
) {
    let planet = state.borrow().planet.clone();
    emit_dialog_signal(
        dialog,
        "rule_changed",
        &[GString::from(key).to_variant(), value.clone()],
    );
    campaign_call(
        "set_campaign_rule",
        &[
            GString::from(planet.as_str()).to_variant(),
            GString::from(key).to_variant(),
            value,
        ],
    );
}
