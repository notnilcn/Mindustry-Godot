// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The launch loadout dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/LaunchLoadoutDialog.java

use super::util::*;
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
    plan: VarDictionary,
}

impl LaunchLoadoutState {
    fn new() -> Self {
        Self {
            selected: -1,
            ..Self::default()
        }
    }
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
            "loadout_changed",
            &loadout,
            "loadout_changed",
            &callable,
        );
    }
}

/// The `launch_dialog_json` plan for the current context/selection.
fn launch_plan(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) -> VarDictionary {
    let planet = dialog.context_string("planet").to_string();
    let launcher = dialog
        .context_value("launcher")
        .map(|value| int_of(&value))
        .unwrap_or(-1);
    let sector = dialog
        .context_value("sector")
        .map(|value| int_of(&value))
        .unwrap_or(-1);
    let selected = {
        let st = state.borrow();
        st.selected
    };
    dialog.campaign_json(
        "launch_dialog_json",
        &[
            gs(planet).to_variant(),
            launcher.to_variant(),
            sector.to_variant(),
            selected.to_variant(),
        ],
    )
}

/// `schematics.getLoadouts()` filtered by the launcher core.
fn launch_rows(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LaunchLoadoutState>>,
) -> Vec<VarDictionary> {
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    // A live plan's row list is authoritative: with `allowLaunchSchematics`
    // false Java renders no grid at all (`LaunchLoadoutDialog.java:167-196`),
    // so an empty list must stay empty instead of falling back to the planet
    // default core's loadouts (PAR-0509).
    if !plan.is_empty() {
        return row_rows(&plan, "loadouts");
    }
    dialog
        .campaign_section("loadouts")
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// The picker's core (`PlanetDialog.java:1405`), read from the live plan.
fn launch_selected_core(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LaunchLoadoutState>>,
) -> String {
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    let core = row_str(&plan, "core");
    if !core.is_empty() {
        return core;
    }
    for loadout in launch_rows(dialog, state) {
        let core = row_str(&loadout, "core");
        if !core.is_empty() {
            return core;
        }
    }
    String::new()
}

/// The `SchematicView` row for a schematic index.
fn launch_schematic_row(dialog: &mut MindDialog, index: i64) -> VarDictionary {
    for row_variant in dialog.campaign_section("schematics").iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_i64_or(&row, "index", -1) == index {
            return row;
        }
    }
    VarDictionary::new()
}

/// The launcher's localized name (`Sector.name()` + the `sector.<preset>.name`
/// rule).
fn launch_localized_from(plan: &VarDictionary) -> String {
    let from_name = row_str(plan, "from");
    let preset = row_str(plan, "from_preset");
    if !preset.is_empty() {
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
    // Select a row the launcher's plan offers (default row preferred); the plan
    // carries the requirements/capacity for the *selected* schematic, so a
    // selection change rebuilds it.
    let offered: Vec<i64> = launch_rows(dialog, state)
        .iter()
        .map(|loadout| row_i64_or(loadout, "index", -1))
        .collect();
    let mut selected = {
        let st = state.borrow();
        st.selected
    };
    if !offered.contains(&selected) {
        let plan = {
            let st = state.borrow();
            st.plan.clone()
        };
        selected = row_i64_or(&plan, "selected", -1);
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
    let loadouts = launch_rows(dialog, state);
    // `launch.from`: the launcher sector's name (`LaunchLoadoutDialog.java:165`).
    if !plan.is_empty() {
        let text = kit::tm_format("@launch.from", &[launch_localized_from(&plan).as_str()]);
        let label = factory::label(&text.to_string());
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    if loadouts.is_empty() {
        // `LaunchLoadoutDialog.java:167-196,204-210`: with schematics forbidden
        // the dialog has no grid at all; Java then shows the destination
        // preset's description pane (nothing when the preset has none).
        let description = row_str(&plan, "preset_description");
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
            let schematic_index = row_i64_or(loadout, "index", -1);
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
            let schematic = launch_schematic_row(dialog, schematic_index);
            image.bind_mut().setup(schematic);
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
    if plan.is_empty() {
        let resource_totals = launch_resource_stacks(dialog);
        if !resource_totals.is_empty() {
            let mut parts: Vec<String> = Vec::new();
            for item in resource_totals.keys_array().iter_shared() {
                let name = text_of(&item);
                let amount = resource_totals
                    .get(&gs(name.as_str()))
                    .map(|value| int_of(&value))
                    .unwrap_or(0);
                parts.push(format!("{name} x{amount}"));
            }
            let label = factory::label(&format!("{} {}", kit::t("@resources"), parts.join(", ")));
            list.bind_mut().add(&label).grow_x().pad(2.0);
            list.bind_mut().row();
        }
        return;
    }
    let capacity = row_i64_or(&plan, "capacity", 0);
    if capacity > 0 {
        let text = kit::tm_format("@launch.capacity", &[capacity.to_string().as_str()]).to_string();
        let label = factory::label(&text);
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    let allow = row_bool(&plan, "allow_launch_loadout");
    let total = row_dict(&plan, "total");
    let requirements = row_dict(&plan, "requirements");
    let resources = row_dict(&plan, "resources");
    let stored = row_dict(&plan, "stored");
    let mut count = 0i64;
    // code-instantiated: item rows are the item-id-ordered `total` map.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 4);
    for item in total.keys_array().iter_shared() {
        let name = text_of(&item);
        let amount = total
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        if amount == 0 {
            continue;
        }
        let icon = factory::image(&format!("item-{name}-ui"));
        row.add_child(&icon);
        let as_amount = requirements
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        let al_amount = resources
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        let mut amount_text = amount.to_string();
        if allow {
            amount_text = format!("{amount_text}[gray] ({al_amount} + {as_amount})");
        }
        let stored_amount = stored
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        if stored_amount < amount {
            amount_text = format!(
                "[scarlet]{}[lightgray]/{}",
                stored_amount.min(amount),
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
    if !row_bool(&plan, "valid") {
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
    let allow = row_bool_or(&plan, "allow_launch_loadout", true);
    let max_on = row_bool_or(&plan, "max", true);
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
    launch.set_disabled(!plan.is_empty() && !row_bool(&plan, "valid"));
}

/// `setMax`: writes the `maxresources` flag through the live settings store.
fn launch_toggle_max(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let max_on = {
        let st = state.borrow();
        row_bool_or(&st.plan, "max", true)
    };
    setting_put_flag("maxresources", !max_on);
    state.borrow_mut().plan = VarDictionary::new();
    launch_rebuild(dialog, state);
}

/// Live `universe.getLaunchResources` as `{item: amount}`.
fn launch_resource_stacks(dialog: &mut MindDialog) -> VarDictionary {
    let Some(mut campaign) = dialog.campaign_node() else {
        return VarDictionary::new();
    };
    if !campaign.has_method("get_launch_resource_stacks") {
        return VarDictionary::new();
    }
    let text = campaign
        .call("get_launch_resource_stacks", &[])
        .to::<GString>();
    parse_object(&text.to_string())
}

/// The selected loadout row, or `{}`.
fn launch_find_selected_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LaunchLoadoutState>>,
) -> VarDictionary {
    let selected = {
        let st = state.borrow();
        st.selected
    };
    for loadout in launch_rows(dialog, state) {
        if row_i64_or(&loadout, "index", -1) == selected {
            return loadout;
        }
    }
    VarDictionary::new()
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
    let row = launch_find_selected_row(dialog, state);
    let mut core_name = row_str(&row, "core");
    if core_name.is_empty() {
        core_name = row_str(&plan, "core");
    }
    let mut file_name = row_str(&row, "file");
    if file_name.is_empty() {
        file_name = row_str(&plan, "selected_file");
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
        .map(|value| int_of(&value))
        .unwrap_or(-1);
    let capacity = {
        let st = state.borrow();
        row_i64_or(&st.plan, "capacity", 0)
    };
    let selected = {
        let st = state.borrow();
        st.selected
    };
    let core = launch_selected_core(dialog, state);
    let mut context = VarDictionary::new();
    context.set("planet", &gs(planet));
    context.set("sector", &sector.to_variant());
    context.set("capacity", &capacity.to_variant());
    context.set("index", &selected.to_variant());
    context.set("core", &gs(core));
    open_dialog("loadout", &context);
}
