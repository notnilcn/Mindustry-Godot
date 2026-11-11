// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The launch loadout dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/LaunchLoadoutDialog.java

use super::util::*;
use crate::gdutil::variant::to_i64_typed;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::schematics::schematics_dialog::SCHEMATIC_COLUMN_WIDTH;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::widgets::schematic_image::MindSchematicImage;
use crate::ui::kit::{self, MindTable};
use godot::builtin::VarDictionary;
use godot::classes::control::LayoutPreset;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{Button, Control, HBoxContainer};
use godot::prelude::*;
use mind_core::ui::campaign::{CampaignViews, LaunchPlanView, SchematicView};
use std::cell::RefCell;
use std::rc::Rc;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "launch_loadout" => Some(Box::new(LaunchLoadoutDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// LaunchLoadoutDialog
// ---------------------------------------------------------------------------

/// Controller state for the launch-loadout picker.
#[derive(Default)]
struct LaunchLoadoutState {
    list: Option<Gd<MindTable>>,
    selected: i64,
    /// The typed `launch_plan(...)` payload for the current selection.
    plan: Option<LaunchPlanView>,
}

impl LaunchLoadoutState {
    fn new() -> Self {
        Self {
            selected: -1,
            ..Self::default()
        }
    }
}

/// One grid row: the picker's own loadouts (`LaunchPlanView.loadouts`) or the
/// planet's projected loadouts when no plan is available.
struct LaunchRow {
    index: i64,
    core: String,
    file: String,
}

/// Launch-loadout picker (`LaunchLoadoutDialog`).
pub struct LaunchLoadoutDialog {
    state: Rc<RefCell<LaunchLoadoutState>>,
}

impl Default for LaunchLoadoutDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(LaunchLoadoutState::new())),
        }
    }
}

impl DialogController for LaunchLoadoutDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@configure");
        dialog.should_pause = false;
        let list = dialog.content_table();
        self.state.borrow_mut().list = Some(list);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        launch_rebuild(dialog, &self.state);
        // `LaunchLoadoutDialog.@resources` gives the editor a hider callback
        // (`LaunchLoadoutDialog.java:138-141`): committing an amount writes the
        // live launch resources back and rebuilds the parent's total/rows. The
        // editor signals `loadout_changed` on every committed edit (PAR-0488).
        let Some(loadout) = dialog_node("loadout") else {
            return;
        };
        let target = dialog.to_gd();
        let callable = Callable::from_fn("launch_loadout_changed", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            bound.refresh_controller();
        });
        connect_once(
            dialog,
            "launch_loadout_changed",
            &loadout,
            "loadout_changed",
            &callable,
        );
    }
}

/// The typed `launch_plan` payload for the current context/selection.
fn launch_plan(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LaunchLoadoutState>>,
) -> Option<LaunchPlanView> {
    let planet = dialog.context_string("planet").to_string();
    let launcher = dialog
        .context_value("launcher")
        .map(|value| to_i64_typed(&value))
        .unwrap_or(-1);
    let sector = dialog
        .context_value("sector")
        .map(|value| to_i64_typed(&value))
        .unwrap_or(-1);
    let selected = {
        let st = state.borrow();
        st.selected
    };
    let mut campaign = dialog.campaign()?;
    campaign.bind_mut().launch_plan(
        &planet,
        i32::try_from(launcher).unwrap_or(-1),
        i32::try_from(sector).unwrap_or(-1),
        i32::try_from(selected).unwrap_or(-1),
    )
}

/// `schematics.getLoadouts()` filtered by the launcher core.
fn launch_rows(plan: Option<&LaunchPlanView>, views: Option<&CampaignViews>) -> Vec<LaunchRow> {
    // A live plan's row list is authoritative: with `allowLaunchSchematics`
    // false Java renders no grid at all (`LaunchLoadoutDialog.java:167-196`),
    // so an empty list must stay empty instead of falling back to the planet
    // default core's loadouts (PAR-0509).
    if let Some(plan) = plan {
        return plan
            .loadouts
            .iter()
            .map(|row| LaunchRow {
                index: row.index as i64,
                core: row.core.clone(),
                file: row.file.clone().unwrap_or_default(),
            })
            .collect();
    }
    views
        .map(|views| {
            views
                .loadouts
                .iter()
                .map(|loadout| LaunchRow {
                    index: loadout.index as i64,
                    core: loadout.core.clone(),
                    file: loadout.file.clone().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The picker's core (`PlanetDialog.java:1405`), read from the live plan.
fn launch_selected_core(plan: Option<&LaunchPlanView>, rows: &[LaunchRow]) -> String {
    let core = plan.map(|plan| plan.core.clone()).unwrap_or_default();
    if !core.is_empty() {
        return core;
    }
    for loadout in rows {
        if !loadout.core.is_empty() {
            return loadout.core.clone();
        }
    }
    String::new()
}

/// The `SchematicView` for a schematic index.
fn launch_schematic_row(views: Option<&CampaignViews>, index: i64) -> Option<SchematicView> {
    let views = views?;
    views
        .schematics
        .iter()
        .find(|row| i64::try_from(row.index).unwrap_or(-1) == index)
        .cloned()
}

/// The launcher's localized name (`Sector.name()` + the `sector.<preset>.name`
/// rule).
fn launch_localized_from(plan: &LaunchPlanView) -> String {
    let from_name = plan.from.clone();
    if let Some(preset) = plan.from_preset.as_deref()
        && !preset.is_empty()
    {
        let key = format!("sector.{preset}.name");
        let localized = kit::t(&format!("@{key}")).to_string();
        if !localized.is_empty() && localized != key {
            return localized;
        }
    }
    from_name
}

fn launch_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let Some(mut list) = state.borrow().list.clone() else {
        return;
    };
    list.bind_mut().clear();
    let plan = launch_plan(dialog, state);
    state.borrow_mut().plan = plan;
    // The planet's projected loadouts (the no-plan fallback rows) and the
    // schematic previews come from one per-show projection.
    let views = campaign_views_of(dialog, "");
    // Select a row the launcher's plan offers (default row preferred); the plan
    // carries the requirements/capacity for the *selected* schematic, so a
    // selection change rebuilds it.
    let offered: Vec<i64> = {
        let st = state.borrow();
        launch_rows(st.plan.as_ref(), views.as_ref())
            .iter()
            .map(|loadout| loadout.index)
            .collect()
    };
    let mut selected = {
        let st = state.borrow();
        st.selected
    };
    if !offered.contains(&selected) {
        selected = {
            let st = state.borrow();
            st.plan
                .as_ref()
                .map(|plan| plan.selected as i64)
                .unwrap_or(-1)
        };
        if !offered.contains(&selected) && !offered.is_empty() {
            selected = offered[0];
        }
        state.borrow_mut().selected = selected;
        let plan = launch_plan(dialog, state);
        state.borrow_mut().plan = plan;
    }
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    let loadouts = launch_rows(plan.as_ref(), views.as_ref());
    // `launch.from`: the launcher sector's name (`LaunchLoadoutDialog.java:165`).
    if let Some(plan) = plan.as_ref() {
        let text = kit::tm_format("@launch.from", &[launch_localized_from(plan).as_str()]);
        let label = factory::label(&text.to_string());
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    if loadouts.is_empty() {
        // `LaunchLoadoutDialog.java:167-196,204-210`: with schematics forbidden
        // the dialog has no grid at all; Java then shows the destination
        // preset's description pane (nothing when the preset has none).
        let description = plan
            .as_ref()
            .map(|plan| plan.preset_description.clone())
            .unwrap_or_default();
        if !description.is_empty() {
            let label = factory::label(&kit::markup_text(&description).to_string());
            let mut description_rich = rich(&label);
            description_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
            let mut minimum = description_rich.get_custom_minimum_size();
            minimum.x = 420.0;
            description_rich.set_custom_minimum_size(minimum);
            list.bind_mut().add(&label).grow_x().pad(10.0);
            list.bind_mut().row();
        }
    } else {
        // `LaunchLoadoutDialog.java:160-190`: `cols = max(screenW / 230, 1)`
        // image toggles (`Styles.togglet` + `SchematicImage`, 200 px, pad 4).
        let cols =
            ((dialog.base().get_viewport_rect().size.x / SCHEMATIC_COLUMN_WIDTH) as i64).max(1);
        for (position, loadout) in loadouts.iter().enumerate() {
            let schematic_index = loadout.index;
            // code-instantiated: one SchematicImage toggle per allowed loadout.
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("togglet");
            button.set_toggle_mode(true);
            button.set_pressed(schematic_index == selected);
            button.set_custom_minimum_size(Vector2::new(200.0, 200.0));
            let target = dialog.to_gd();
            let state_for_select = state.clone();
            let callable = Callable::from_fn("launch_select", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                state_for_select.borrow_mut().selected = schematic_index;
                launch_rebuild(&mut bound, &state_for_select);
            });
            button.connect("pressed", &callable);
            let mut image = MindSchematicImage::new_alloc();
            {
                let mut image_control: Gd<Control> = image.clone().upcast();
                image_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            }
            if let Some(schematic) = launch_schematic_row(views.as_ref(), schematic_index) {
                image.bind_mut().setup(schematic_image_row(&schematic));
            }
            button.add_child(&image);
            {
                let image_for_enter = image.clone();
                let callable = Callable::from_fn("launch_enter", move |_args: &[&Variant]| {
                    let mut image = image_for_enter.clone();
                    image.bind_mut().set_hovered(true);
                });
                button.connect("mouse_entered", &callable);
                let image_for_exit = image.clone();
                let callable = Callable::from_fn("launch_exit", move |_args: &[&Variant]| {
                    let mut image = image_for_exit.clone();
                    image.bind_mut().set_hovered(false);
                });
                button.connect("mouse_exited", &callable);
            }
            list.bind_mut().add(&button).pad(4.0);
            if (position as i64 + 1) % cols == 0 {
                list.bind_mut().row();
            }
        }
    }
    launch_rebuild_items(dialog, state);
    launch_rebuild_buttons(dialog, state);
}

/// Capacity label + item rows (`LaunchLoadoutDialog.rebuild`).
fn launch_rebuild_items(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let Some(mut list) = state.borrow().list.clone() else {
        return;
    };
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    let Some(plan) = plan else {
        let resource_totals = campaign_launch_stacks(dialog);
        if !resource_totals.is_empty() {
            let mut parts: Vec<String> = Vec::new();
            for (name, amount) in &resource_totals {
                parts.push(format!("{name} x{amount}"));
            }
            let label = factory::label(&format!("{} {}", kit::t("@resources"), parts.join(", ")));
            list.bind_mut().add(&label).grow_x().pad(2.0);
            list.bind_mut().row();
        }
        return;
    };
    let capacity = plan.capacity;
    if capacity > 0 {
        let text = kit::tm_format("@launch.capacity", &[capacity.to_string().as_str()]).to_string();
        let label = factory::label(&text);
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    let allow = plan.allow_launch_loadout;
    let mut count = 0i64;
    // code-instantiated: item rows are the item-id-ordered `total` map.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 4);
    for (name, amount) in &plan.total {
        if *amount == 0 {
            continue;
        }
        let icon = factory::image(&format!("item-{name}-ui"));
        row.add_child(&icon);
        let as_amount = plan.requirements.get(name.as_str()).copied().unwrap_or(0);
        let al_amount = plan.resources.get(name.as_str()).copied().unwrap_or(0);
        let mut amount_text = amount.to_string();
        if allow {
            amount_text = format!("{amount_text}[gray] ({al_amount} + {as_amount})");
        }
        let stored_amount = plan.stored.get(name.as_str()).copied().unwrap_or(0);
        if stored_amount < *amount {
            amount_text = format!(
                "[scarlet]{}[lightgray]/{}",
                stored_amount.min(*amount),
                amount_text
            );
        }
        let label = factory::label(&kit::markup_text(&amount_text).to_string());
        row.add_child(&label);
        count += 1;
        if count % 4 == 0 {
            list.bind_mut().add(&row).grow_x().pad(2.0);
            list.bind_mut().row();
            // code-instantiated: grid row continuation for the item list.
            row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 4);
        }
    }
    if count % 4 != 0 {
        list.bind_mut().add(&row).grow_x().pad(2.0);
        list.bind_mut().row();
    }
    if !plan.valid {
        let label = factory::label(&kit::tm("@sector.missingresources").to_string());
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    let _ = state;
}

/// `buttons.defaults().size(160f, 64f)`; Java order: `@back`,
/// (`@resources.max` toggle), `@resources`, `@launch.text`.
fn launch_rebuild_buttons(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    dialog.clear_buttons();
    let back = dialog.base().callable("hide_dialog");
    let back_text = kit::t("@back").to_string();
    dialog.add_button(&back_text, Some(back), "left", 160.0);
    // An absent plan reads as the old empty dictionary: both flags default true.
    let allow = plan
        .as_ref()
        .map(|plan| plan.allow_launch_loadout)
        .unwrap_or(true);
    let max_on = plan.as_ref().map(|plan| plan.max).unwrap_or(true);
    if allow {
        let target = dialog.to_gd();
        let state_for_max = state.clone();
        let callable = Callable::from_fn("launch_toggle_max", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            launch_toggle_max(&mut bound, &state_for_max);
        });
        let max_text = kit::t("@resources.max").to_string();
        let mut max_button = dialog.add_button(&max_text, Some(callable), "add", 160.0);
        max_button.set_toggle_mode(true);
        max_button.set_pressed(max_on);
        let target = dialog.to_gd();
        let state_for_open = state.clone();
        let callable = Callable::from_fn("launch_open_loadout", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            launch_open_loadout(&mut bound, &state_for_open);
        });
        let resources_text = kit::t("@resources").to_string();
        let mut resources = dialog.add_button(&resources_text, Some(callable), "edit", 160.0);
        resources.set_disabled(max_on);
    }
    let target = dialog.to_gd();
    let state_for_confirm = state.clone();
    let callable = Callable::from_fn("launch_confirm", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        launch_confirm(&mut bound, &state_for_confirm);
    });
    let launch_text = kit::t("@launch.text").to_string();
    let mut launch = dialog.add_button(&launch_text, Some(callable), "ok", 160.0);
    launch.set_disabled(plan.as_ref().is_some_and(|plan| !plan.valid));
}

/// `setMax`: writes the `maxresources` flag through the live settings store.
fn launch_toggle_max(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let max_on = {
        let st = state.borrow();
        st.plan.as_ref().map(|plan| plan.max).unwrap_or(true)
    };
    setting_put_flag("maxresources", !max_on);
    state.borrow_mut().plan = None;
    launch_rebuild(dialog, state);
}

/// The selected loadout row.
fn launch_find_selected_row(
    state: &Rc<RefCell<LaunchLoadoutState>>,
    views: Option<&CampaignViews>,
    selected: i64,
) -> Option<LaunchRow> {
    let st = state.borrow();
    launch_rows(st.plan.as_ref(), views)
        .into_iter()
        .find(|loadout| loadout.index == selected)
}

/// `LaunchLoadoutDialog.@launch.text`: `universe.updateLoadout(core, selected);
/// confirm.run(); hide()`.
fn launch_confirm(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let selected = {
        let st = state.borrow();
        st.selected
    };
    if selected < 0 {
        return;
    }
    // `Universe.updateLoadout(core, selected)` (`LaunchLoadoutDialog.java:150-152`):
    // record the chosen schematic's file for the core before the planet dialog
    // reads it back for the launch. A file-less schematic records the empty
    // string (Java `schem.file == null ? ""`), and Java stores
    // `schem.file.nameWithoutExtension()` — never the full path — so the
    // seeded `lastloadout-<core>` resolves again on the next launch
    // (PAR-0508). The plan carries the picker's core and selected file even
    // when the destination forbids schematics (no grid rows).
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    let views = if plan.is_none() {
        campaign_views_of(dialog, "")
    } else {
        None
    };
    let row = launch_find_selected_row(state, views.as_ref(), selected);
    let mut core_name = row.as_ref().map(|row| row.core.clone()).unwrap_or_default();
    if core_name.is_empty() {
        core_name = plan
            .as_ref()
            .map(|plan| plan.core.clone())
            .unwrap_or_default();
    }
    let mut file_name = row.as_ref().map(|row| row.file.clone()).unwrap_or_default();
    if file_name.is_empty() {
        file_name = plan
            .as_ref()
            .and_then(|plan| plan.selected_file.clone())
            .unwrap_or_default();
    }
    let file_name = mind_core::game::universe::file_stem(&file_name).to_owned();
    if let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("update_loadout")
    {
        let _ = campaign.call(
            "update_loadout",
            &[gs(core_name).to_variant(), gs(file_name).to_variant()],
        );
    }
    dialog
        .base_mut()
        .emit_signal("loadout_chosen", &[selected.to_variant()]);
}

/// `LaunchLoadoutDialog.@resources`: opens the capacity editor seeded with the
/// live launch resources.
fn launch_open_loadout(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let planet = dialog.context_string("planet").to_string();
    let sector = dialog
        .context_value("sector")
        .map(|value| to_i64_typed(&value))
        .unwrap_or(-1);
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    let capacity = plan.as_ref().map(|plan| plan.capacity).unwrap_or(0);
    let selected = {
        let st = state.borrow();
        st.selected
    };
    let views = if plan.is_none() {
        campaign_views_of(dialog, "")
    } else {
        None
    };
    let rows = {
        let st = state.borrow();
        launch_rows(st.plan.as_ref(), views.as_ref())
    };
    let core = launch_selected_core(plan.as_ref(), &rows);
    let mut context = VarDictionary::new();
    context.set("planet", &gs(planet));
    context.set("sector", &sector.to_variant());
    context.set("capacity", &i64::from(capacity).to_variant());
    context.set("index", &selected.to_variant());
    context.set("core", &gs(core));
    open_dialog("loadout", &context);
}
