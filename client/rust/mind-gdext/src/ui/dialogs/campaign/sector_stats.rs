// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The sector stats dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/PlanetDialog.java (showStats)

use super::util::*;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable};
use godot::builtin::{VarArray, VarDictionary, VariantType};
use godot::classes::control::SizeFlags;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{Control, HBoxContainer};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "sector_stats" => Some(Box::new(SectorStatsDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// SectorStatsDialog
// ---------------------------------------------------------------------------

/// `Pal.accent` hex used in the `name [accent]value` stat lines.
const STATS_ACCENT: &str = "#ffd37f";

/// Controller state for the reused sector stats dialog node.
#[derive(Default)]
struct SectorStatsState {
    planet: String,
    sector_id: i64,
    row: VarDictionary,
    root: Option<Gd<MindTable>>,
    pending_abandon: bool,
}

impl SectorStatsState {
    fn new() -> Self {
        Self {
            sector_id: -1,
            ..Self::default()
        }
    }
}

/// The planet page's Stats window (`PlanetDialog.java:1026-1171`).
pub struct SectorStatsDialog {
    state: Rc<RefCell<SectorStatsState>>,
}

impl Default for SectorStatsDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(SectorStatsState::new())),
        }
    }
}

impl DialogController for SectorStatsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        // `new BaseDialog(sector.name())` fills the viewport.
        dialog.fill_panel();
        if let Some(ui) = kit::ui() {
            let target = dialog.to_gd();
            let state_for_confirm = self.state.clone();
            let callable = Callable::from_fn("sector_stats_confirm", move |args: &[&Variant]| {
                let confirmed = args
                    .first()
                    .map(|value| (*value).to::<bool>())
                    .unwrap_or(false);
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                sector_stats_on_confirm(&mut bound, &state_for_confirm, confirmed);
            });
            connect_once(
                dialog,
                "sector_stats_confirm",
                &ui,
                "confirm_result",
                &callable,
            );
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let planet = dialog.context_string("planet").to_string();
        let sector_id = dialog
            .context_value("sector")
            .map(|value| int_of(&value))
            .unwrap_or(-1);
        let row = sector_stats_find_row(dialog, &planet, sector_id);
        {
            let mut st = self.state.borrow_mut();
            st.planet = planet;
            st.sector_id = sector_id;
            st.row = row;
            st.pending_abandon = false;
        }
        sector_stats_build(dialog, &self.state);
    }
}

/// `PlanetDialog.showStats`: description, time/attempts/wave/threat, resources,
/// lockdown, rate sections and the stored pool, in this order.
fn sector_stats_build(dialog: &mut MindDialog, state: &Rc<RefCell<SectorStatsState>>) {
    let mut root = {
        let st = state.borrow();
        st.root.clone()
    };
    let mut root = match root.take() {
        Some(table) => table,
        None => {
            // `dialog.cont.pane(c -> ...)`: the dialog body already scrolls, so
            // the stats table sits directly in it, centred at its natural width.
            let table = dialog.content_table();
            {
                let mut control: Gd<Control> = table.clone().upcast();
                control.set_h_size_flags(SizeFlags::SHRINK_CENTER);
            }
            state.borrow_mut().root = Some(table.clone());
            table
        }
    };
    root.bind_mut().clear();
    dialog.clear_buttons();
    let title = sector_stats_localized_name(state);
    dialog.set_title_literal(&title);
    let (data, planet, sector_id) = {
        let st = state.borrow();
        (st.row.clone(), st.planet.clone(), st.sector_id)
    };
    let _ = (planet, sector_id);
    let stats = row_dict(&data, "stats");
    let description_key = row_opt_str(&stats, "preset_description_key").unwrap_or_default();
    if !description_key.is_empty() {
        let description = factory::label(&kit::tm(&format!("@{description_key}")).to_string());
        let mut description_rich = rich(&description);
        description_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        let mut minimum = description_rich.get_custom_minimum_size();
        minimum.x = 420.0;
        description_rich.set_custom_minimum_size(minimum);
        root.bind_mut().add(&description).grow_x().pad_bottom(5.0);
        root.bind_mut().row();
    }
    let playtime = row_i64_or(&stats, "playtime_ms", 0);
    sector_stats_value_line(&mut root, "@sectors.time", &format_millis(playtime));
    let attempts = row_i64_or(&stats, "attempts", 0);
    if attempts > 0 {
        sector_stats_value_line(&mut root, "@sectors.attempts", &attempts.to_string());
    }
    let wave = row_i64_or(&stats, "wave", 0);
    if wave > 0 {
        sector_stats_value_line(&mut root, "@sectors.wave", &wave.to_string());
    }
    if row_bool(&data, "attacked") || !row_bool(&data, "has_base") {
        let band = row_opt_str(&data, "threat_band").unwrap_or_else(|| String::from("low"));
        let text = kit::t(&format!("@threat.{band}")).to_string();
        sector_stats_value_line(&mut root, "@sectors.threat", &text);
    }
    let resources = row_arr(&stats, "resources");
    if row_bool(&data, "has_save") && !resources.is_empty() {
        let label = factory::label(&kit::t("@sectors.resources").to_string());
        root.bind_mut().add(&label).left();
        root.bind_mut().row();
        let mut resource_row = HBoxContainer::new_alloc();
        resource_row.add_theme_constant_override("separation", 3);
        for resource in resources.iter_shared() {
            let name = text_of(&resource);
            let Some(texture) = content_texture(&name) else {
                continue;
            };
            // code-instantiated: one 24 px icon per known resource.
            resource_row.add_child(&icon_rect(&texture, 24.0));
        }
        root.bind_mut().add(&resource_row).pad_left(10.0).left();
        root.bind_mut().row();
    }
    if row_bool(&stats, "lockdown") {
        // `c.add(UI.formatIcons(bundle.get("sector.lockdown"))).wrap().fillX()`.
        let lockdown = factory::label(&icon_glyph_markup(&kit::tm("@sector.lockdown").to_string()));
        let mut lockdown_rich = rich(&lockdown);
        lockdown_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        lockdown_rich.set_h_size_flags(SizeFlags::EXPAND_FILL);
        root.bind_mut().add(&lockdown).grow_x().pad_bottom(10.0);
        root.bind_mut().row();
        apply_icon_fallback(&lockdown_rich);
    }
    sector_stats_stat_section(
        &mut root,
        "@sectors.production",
        &row_arr(&stats, "production"),
        &VarArray::new(),
    );
    let mut export_trailing = VarArray::new();
    if let Some(destination) = stats.get("destination")
        && destination.get_type() == VariantType::DICTIONARY
    {
        export_trailing.push(&destination);
    }
    sector_stats_stat_section(
        &mut root,
        "@sectors.export",
        &row_arr(&stats, "export"),
        &export_trailing,
    );
    if row_bool(&data, "has_base") {
        sector_stats_stat_section(
            &mut root,
            "@sectors.import",
            &row_arr(&stats, "import"),
            &row_arr(&stats, "import_sources"),
        );
    }
    let stored = row_arr(&stats, "stored");
    if row_bool(&data, "has_base") && !stored.is_empty() {
        let label = factory::label(&kit::t("@sectors.stored").to_string());
        root.bind_mut().add(&label).left();
        root.bind_mut().row();
        // code-instantiated: the stored pool is a runtime-sized 4-column grid.
        let mut stored_grid = MindTable::new_alloc();
        let mut index = 0i64;
        for entry in stored.iter_shared() {
            let Ok(entry) = entry.try_to::<VarArray>() else {
                continue;
            };
            if entry.len() < 2 {
                continue;
            }
            let mut values = entry.iter_shared();
            let content_name = values
                .next()
                .map(|value| text_of(&value))
                .unwrap_or_default();
            let amount = values.next().map(|value| int_of(&value)).unwrap_or(0);
            let cell = sector_stats_stored_cell(&content_name, amount);
            stored_grid.bind_mut().add(&cell);
            index += 1;
            if index % 4 == 0 {
                stored_grid.bind_mut().row();
            }
        }
        root.bind_mut().add(&stored_grid).pad_left(10.0).left();
        root.bind_mut().row();
    }
    dialog.add_close_button(210.0);
    // `abandonSectorConfirm` button, shown for sectors with a base.
    if row_bool(&data, "has_base") {
        let mut abandon = factory::icon_button(
            "cancel",
            &kit::t("@sector.abandon").to_string(),
            "cleart",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        let target = dialog.to_gd();
        let state_for_abandon = state.clone();
        let callable = Callable::from_fn("sector_stats_abandon", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            sector_stats_confirm_abandon(&mut bound, &state_for_abandon);
        });
        abandon.connect("pressed", &callable);
        if let Some(mut row) = dialog.buttons_row() {
            row.add_child(&abandon);
        }
    }
}

/// `name [accent]value` line (`c.add(Core.bundle.get(...) + " [accent]" + ...)`).
fn sector_stats_value_line(root: &mut Gd<MindTable>, label_key: &str, value: &str) {
    let label = factory::label(&format!(
        "{} [color={STATS_ACCENT}]{}[/color]",
        kit::t(label_key),
        value
    ));
    root.bind_mut().add(&label).left().pad_bottom(5.0);
    root.bind_mut().row();
}

/// `displayItems(c, stats, name, builder)`: 3 `icon total unit.perminute` cells
/// per row behind a `lightGray` label, plus the trailing destination/source
/// lines (`Iconc.rightOpen` + sector name).
fn sector_stats_stat_section(
    root: &mut Gd<MindTable>,
    label_key: &str,
    rows: &VarArray,
    trailing: &VarArray,
) {
    let mut cells: Vec<VarArray> = Vec::new();
    for entry in rows.iter_shared() {
        let Ok(entry) = entry.try_to::<VarArray>() else {
            continue;
        };
        if entry.len() >= 2 {
            cells.push(entry);
        }
    }
    if cells.is_empty() && trailing.is_empty() {
        return;
    }
    let label = factory::label(&kit::t(label_key).to_string());
    root.bind_mut().add(&label).left();
    root.bind_mut().row();
    if !cells.is_empty() {
        // code-instantiated: runtime-sized 3-column rate grid.
        let mut grid = MindTable::new_alloc();
        let mut index = 0i64;
        for cell in &cells {
            let mut cell_row = HBoxContainer::new_alloc();
            cell_row.add_theme_constant_override("separation", 3);
            let content_name = cell
                .iter_shared()
                .next()
                .map(|value| text_of(&value))
                .unwrap_or_default();
            let amount = cell
                .iter_shared()
                .nth(1)
                .map(|value| int_of(&value))
                .unwrap_or(0);
            if let Some(texture) = content_texture(&content_name) {
                // code-instantiated: one icon per rate row.
                cell_row.add_child(&icon_rect(&texture, 24.0));
            }
            let label = factory::label(&format!("{} {}", amount, kit::t("@unit.perminute")));
            rich(&label).add_theme_color_override("default_color", LIGHT_GRAY);
            cell_row.add_child(&label);
            grid.bind_mut().add(&cell_row);
            index += 1;
            if index % 3 == 0 {
                grid.bind_mut().row();
            }
        }
        root.bind_mut().add(&grid).pad_left(10.0).left();
        root.bind_mut().row();
    }
    for source_variant in trailing.iter_shared() {
        let Ok(source) = source_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let prefix = kit::glyph("right-open");
        let content_icon = row_opt_str(&source, "content_icon").unwrap_or_default();
        let icon_name = row_opt_str(&source, "icon").unwrap_or_default();
        let icon_text = if !content_icon.is_empty() {
            String::new()
        } else if !icon_name.is_empty() {
            kit::glyph(&icon_name).to_string()
        } else {
            String::new()
        };
        let spacing = if icon_text.is_empty() { "" } else { " " };
        let line = format!(
            "{} {}{}{}",
            prefix,
            icon_text,
            spacing,
            row_str(&source, "name")
        );
        let label = factory::label(&line);
        root.bind_mut().add(&label).pad_left(10.0).left();
        root.bind_mut().row();
    }
}

/// One stored cell (`res.image(uiIcon).padRight(3); res.add(formatAmount)`).
fn sector_stats_stored_cell(content_name: &str, amount: i64) -> Gd<Control> {
    // code-instantiated: one per stored item row.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 3);
    if let Some(texture) = content_texture(content_name) {
        row.add_child(&icon_rect(&texture, 24.0));
    }
    let label = factory::label(&amount.max(0).to_string());
    rich(&label).add_theme_color_override("default_color", LIGHT_GRAY);
    row.add_child(&label);
    let control: Gd<Control> = row.upcast();
    control
}

/// `BaseDialog(sector.name())`: custom name wins, then the localized preset.
fn sector_stats_localized_name(state: &Rc<RefCell<SectorStatsState>>) -> String {
    let st = state.borrow();
    if row_bool(&st.row, "named") {
        return row_str(&st.row, "name");
    }
    let preset = row_str(&st.row, "preset");
    if !preset.is_empty() {
        let key = format!("sector.{preset}.name");
        let localized = kit::t(&format!("@{key}")).to_string();
        if !localized.is_empty() && localized != key {
            return localized;
        }
    }
    row_str(&st.row, "name")
}

/// The live `campaign_views` sector row this dialog was opened for.
fn sector_stats_find_row(dialog: &mut MindDialog, planet: &str, sector_id: i64) -> VarDictionary {
    let views = dialog.campaign_json("campaign_views_json", &[gs(planet).to_variant()]);
    for row_variant in row_arr(&views, "sectors").iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_i64_or(&row, "id", -1) == sector_id && row_str(&row, "planet") == planet {
            return row;
        }
    }
    VarDictionary::new()
}

/// `abandonSectorConfirm`: shared confirm prompt, then the off-play record
/// clear (`MindCampaign.abandon_sector_at`) and the planet page refresh.
fn sector_stats_confirm_abandon(dialog: &mut MindDialog, state: &Rc<RefCell<SectorStatsState>>) {
    let _ = dialog;
    state.borrow_mut().pending_abandon = true;
    let _ = ui_call(
        "show_confirm",
        &[kit::t("@sector.abandon.confirm").to_variant()],
    );
}

fn sector_stats_on_confirm(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SectorStatsState>>,
    confirmed: bool,
) {
    let pending = {
        let mut st = state.borrow_mut();
        let pending = st.pending_abandon;
        st.pending_abandon = false;
        pending
    };
    if !pending {
        return;
    }
    if !confirmed {
        return;
    }
    let (planet, sector_id) = {
        let st = state.borrow();
        (st.planet.clone(), st.sector_id)
    };
    let played = sector_stats_is_played(dialog, state);
    if let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("abandon_sector_at")
    {
        let _ = campaign.call(
            "abandon_sector_at",
            &[gs(planet).to_variant(), sector_id.to_variant()],
        );
    }
    if kit::ui().is_some() {
        close_dialog("sector_stats");
        // `sector.isBeingPlayed()` -> `hide()` (PlanetDialog.java:1153-1154):
        // the cores die 7 ticks later and the game-over path plays.
        if played {
            close_dialog("planet");
            return;
        }
    }
    if let Some(mut planet_dialog) = dialog_node("planet")
        && planet_dialog.has_method("refresh_page")
    {
        planet_dialog.call("refresh_page", &[]);
    }
}

/// `Sector.isBeingPlayed` for this dialog's sector (`get_sector_state`).
fn sector_stats_is_played(dialog: &mut MindDialog, state: &Rc<RefCell<SectorStatsState>>) -> bool {
    let (planet, sector_id) = {
        let st = state.borrow();
        (st.planet.clone(), st.sector_id)
    };
    let Some(mut campaign) = dialog.campaign_node() else {
        return false;
    };
    if !campaign.has_method("get_sector_state") {
        return false;
    }
    let value = campaign.call("get_sector_state", &[]);
    let Ok(sector_state) = value.try_to::<VarDictionary>() else {
        return false;
    };
    row_bool(&sector_state, "campaign")
        && row_str(&sector_state, "planet") == planet
        && row_i64_or(&sector_state, "sector", -1) == sector_id
}
