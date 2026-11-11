// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The loadout dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/LoadoutDialog.java

use super::util::*;
use crate::gdutil::variant::{to_i64_typed, to_text_godot};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable};
use godot::builtin::VarDictionary;
use godot::classes::control::MouseFilter;
use godot::classes::{Control, HBoxContainer};
use godot::prelude::*;
use mind_core::ui::campaign::CampaignViews;
use std::cell::RefCell;
use std::rc::Rc;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "loadout" => Some(Box::new(LoadoutDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// LoadoutDialog
// ---------------------------------------------------------------------------

/// Controller state for the core loadout capacity editor.
#[derive(Default)]
struct LoadoutState {
    summary: Option<Gd<MindTable>>,
    capacity: i64,
    index: i64,
    amounts: VarDictionary,
    seed_amounts: VarDictionary,
    pending_edit: String,
}

impl LoadoutState {
    fn new() -> Self {
        Self {
            index: -1,
            ..Self::default()
        }
    }
}

/// Core loadout capacity editor (`LoadoutDialog`).
pub struct LoadoutDialog {
    state: Rc<RefCell<LoadoutState>>,
}

impl Default for LoadoutDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(LoadoutState::new())),
        }
    }
}

impl DialogController for LoadoutDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@configure");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        let table = dialog.content_table();
        self.state.borrow_mut().summary = Some(table);
        if let Some(ui) = kit::ui() {
            let target = dialog.to_gd();
            let state_for_input = self.state.clone();
            let callable = Callable::from_fn("loadout_text_input", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| (*value).to::<GString>().to_string())
                    .unwrap_or_default();
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                loadout_on_text_input(&mut bound, &state_for_input, &text);
            });
            connect_once(
                dialog,
                "loadout_text_input",
                &ui,
                "text_input_result",
                &callable,
            );
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let planet = dialog.context_string("planet").to_string();
        // The typed per-planet projection seeds the editor's item rows. The old
        // `launch_capacity` dictionary read was always 0: `CampaignViews` has no
        // such field, so the capacity comes from the context only.
        let views = campaign_views_of(dialog, &planet);
        let capacity = dialog
            .context_value("capacity")
            .map(|value| to_i64_typed(&value))
            .unwrap_or(0);
        let index = dialog
            .context_value("index")
            .map(|value| to_i64_typed(&value))
            .unwrap_or(-1);
        {
            let mut st = self.state.borrow_mut();
            st.capacity = capacity;
            st.index = index;
        }
        loadout_seed(dialog, &self.state, views.as_ref());
        loadout_rebuild(dialog, &self.state);
    }
}

/// Seeds the editor from the live launch resources, unioned with every loadout
/// requirement item at zero so all relevant items can be stepped at least once.
///
/// `LoadoutDialog.reseed` (`LoadoutDialog.java:125-129`) copies the live
/// launch-resource stacks and appends every remaining planet item at 0 — never
/// the schematic requirement amounts (PAR-0488).
fn loadout_seed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LoadoutState>>,
    views: Option<&CampaignViews>,
) {
    let mut seed_amounts = VarDictionary::new();
    // `Universe.getLaunchResources()`: the typed `{item: amount}` stacks.
    for (name, amount) in campaign_launch_stacks(dialog) {
        seed_amounts.set(&gs(name.as_str()), &i64::from(amount).to_variant());
    }
    if let Some(views) = views {
        for loadout in &views.loadouts {
            for (item, _) in &loadout.requirements {
                if !seed_amounts.contains_key(&gs(item.as_str())) {
                    seed_amounts.set(&gs(item.as_str()), &0i64.to_variant());
                }
            }
        }
    }
    let mut st = state.borrow_mut();
    st.amounts = seed_amounts.duplicate_shallow();
    st.seed_amounts = seed_amounts;
}

fn loadout_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    let Some(mut summary) = state.borrow().summary.clone() else {
        return;
    };
    summary.bind_mut().clear();
    let (capacity, amounts) = {
        let st = state.borrow();
        (st.capacity, st.amounts.clone())
    };
    if capacity > 0 {
        let text = kit::tm_format("@launch.capacity", &[capacity.to_string().as_str()]).to_string();
        let label = factory::label(&text);
        summary.bind_mut().add(&label).grow_x().pad(4.0);
        summary.bind_mut().row();
    }
    let mut keys: Vec<String> = amounts
        .keys_array()
        .iter_shared()
        .map(|value| to_text_godot(&value))
        .collect();
    keys.sort();
    let mut pending: Option<Gd<HBoxContainer>> = None;
    for (position, item) in keys.iter().enumerate() {
        if position % 2 == 0 {
            // code-instantiated: item rows are the live launch-resource ItemSeq.
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 6);
            summary.bind_mut().add(&row).grow_x().pad(2.0);
            summary.bind_mut().row();
            pending = Some(row);
        }
        let Some(mut row) = pending.clone() else {
            continue;
        };
        let item_row = loadout_item_row(dialog, state, item);
        row.add_child(&item_row);
    }
    dialog.clear_buttons();
    let target = dialog.to_gd();
    let state_for_max = state.clone();
    let callable = Callable::from_fn("loadout_max", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_max_items(&mut bound, &state_for_max);
    });
    let max_text = kit::t("@max").to_string();
    dialog.add_button(&max_text, Some(callable), "add", 200.0);
    let target = dialog.to_gd();
    let state_for_reset = state.clone();
    let callable = Callable::from_fn("loadout_reset", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_reset(&mut bound, &state_for_reset);
    });
    let reset_text = kit::t("@settings.reset").to_string();
    dialog.add_button(&reset_text, Some(callable), "refresh", 200.0);
    dialog.add_default_close_button();
}

fn loadout_item_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LoadoutState>>,
    item: &str,
) -> Gd<Control> {
    // code-instantiated: item rows are the live launch-resource ItemSeq.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 2);
    let mut minus = factory::button("-");
    minus.set_custom_minimum_size(Vector2::new(36.0, 36.0));
    let target = dialog.to_gd();
    let state_for_minus = state.clone();
    let item_for_minus = item.to_string();
    let callable = Callable::from_fn("loadout_minus", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_step(&mut bound, &state_for_minus, &item_for_minus, -1);
    });
    minus.connect("pressed", &callable);
    row.add_child(&minus);
    let mut plus = factory::button("+");
    plus.set_custom_minimum_size(Vector2::new(36.0, 36.0));
    let target = dialog.to_gd();
    let state_for_plus = state.clone();
    let item_for_plus = item.to_string();
    let callable = Callable::from_fn("loadout_plus", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_step(&mut bound, &state_for_plus, &item_for_plus, 1);
    });
    plus.connect("pressed", &callable);
    row.add_child(&plus);
    let mut pencil = factory::image_button("pencil", "flati");
    pencil.set_custom_minimum_size(Vector2::new(36.0, 36.0));
    let target = dialog.to_gd();
    let state_for_edit = state.clone();
    let item_for_edit = item.to_string();
    let callable = Callable::from_fn("loadout_edit", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_edit(&mut bound, &state_for_edit, &item_for_edit);
    });
    pencil.connect("pressed", &callable);
    row.add_child(&pencil);
    // `res.image(item.uiIcon)`: the packed item region (the GDScript passed the
    // bare content name, which has no atlas region).
    let icon = match content_texture(item) {
        Some(texture) => icon_rect(&texture, 28.0),
        None => factory::image(&format!("item-{item}-ui")),
    };
    let mut icon_control: Gd<Control> = icon.upcast();
    icon_control.set_custom_minimum_size(Vector2::new(28.0, 28.0));
    icon_control.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon_control);
    let amount = {
        let st = state.borrow();
        st.amounts
            .get(&gs(item))
            .map(|value| to_i64_typed(&value))
            .unwrap_or(0)
    };
    let label = factory::label(&format!("{}  {}", loadout_localized_item(item), amount));
    row.add_child(&label);
    let control: Gd<Control> = row.upcast();
    control
}

/// Bundle label for an item (`item.<name>.name`), falling back to the id.
fn loadout_localized_item(item: &str) -> String {
    let key = format!("item.{item}.name");
    bundle_or(&key, item)
}

/// `LoadoutDialog.step`: 100/200/500/1000 depending on the current amount.
fn loadout_step_size(amount: i64) -> i64 {
    if amount < 1000 {
        100
    } else if amount < 2000 {
        200
    } else if amount < 5000 {
        500
    } else {
        1000
    }
}

fn loadout_step(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LoadoutState>>,
    item: &str,
    direction: i64,
) {
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    {
        let mut st = state.borrow_mut();
        let current = st
            .amounts
            .get(&gs(item))
            .map(|value| to_i64_typed(&value))
            .unwrap_or(0);
        let mut amount = (current + direction * loadout_step_size(current)).max(0);
        if capacity > 0 {
            amount = amount.min(capacity);
        }
        st.amounts.set(&gs(item), &amount.to_variant());
    }
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// `LoadoutDialog.maxItems`: every stack to the launch capacity.
fn loadout_max_items(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    if capacity <= 0 {
        return;
    }
    {
        let mut st = state.borrow_mut();
        let keys: Vec<String> = st
            .amounts
            .keys_array()
            .iter_shared()
            .map(|value| to_text_godot(&value))
            .collect();
        for item in keys {
            st.amounts.set(&gs(item.as_str()), &capacity.to_variant());
        }
    }
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// `LoadoutDialog.@settings.reset` (`LoadoutDialog.java:56-60`):
/// `resetter.run()` clears the launch resources
/// (`LaunchLoadoutDialog.java:138` passes `out::clear`), then `reseed()`
/// rebuilds the rows at zero.
fn loadout_reset(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    {
        let mut st = state.borrow_mut();
        let keys: Vec<String> = st
            .amounts
            .keys_array()
            .iter_shared()
            .map(|value| to_text_godot(&value))
            .collect();
        for item in keys {
            st.amounts.set(&gs(item.as_str()), &0i64.to_variant());
        }
    }
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// Inline amount entry through the shared `showTextInput` prompt pipeline.
fn loadout_edit(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>, item: &str) {
    state.borrow_mut().pending_edit = item.to_string();
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    let current = {
        let st = state.borrow();
        st.amounts
            .get(&gs(item))
            .map(|value| to_i64_typed(&value))
            .unwrap_or(0)
    };
    let limit = if capacity > 0 { capacity } else { 9999 };
    let message = kit::tm_format("@configure.invalid", &[limit.to_string().as_str()]).to_string();
    let _ = dialog;
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@configure").to_variant(),
            gs(message.as_str()).to_variant(),
            10i64.to_variant(),
            gs(current.to_string().as_str()).to_variant(),
            true.to_variant(),
            true.to_variant(),
        ],
    );
}

fn loadout_on_text_input(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>, text: &str) {
    let pending = {
        let st = state.borrow();
        st.pending_edit.clone()
    };
    if pending.is_empty() || !is_valid_int(text) {
        state.borrow_mut().pending_edit = String::new();
        return;
    }
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    let mut amount = text.trim().parse::<i64>().unwrap_or(0).max(0);
    if capacity > 0 {
        amount = amount.min(capacity);
    }
    {
        let mut st = state.borrow_mut();
        st.amounts.set(&gs(pending.as_str()), &amount.to_variant());
        st.pending_edit = String::new();
    }
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// GDScript `String.is_valid_int()`: an optional sign followed by digits.
fn is_valid_int(text: &str) -> bool {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix(['-', '+']).unwrap_or(trimmed);
    !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit())
}

/// Persists the edited amounts through the campaign facade and emits the change.
fn loadout_apply(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    let (amounts, index) = {
        let st = state.borrow();
        (st.amounts.clone(), st.index)
    };
    if let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("set_launch_resources")
    {
        let _ = campaign.call("set_launch_resources", &[amounts.to_variant()]);
    }
    if index >= 0 {
        dialog
            .base_mut()
            .emit_signal("loadout_changed", &[index.to_variant()]);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn step_sizes_match_loadout_dialog() {
        assert_eq!(loadout_step_size(0), 100);
        assert_eq!(loadout_step_size(999), 100);
        assert_eq!(loadout_step_size(1000), 200);
        assert_eq!(loadout_step_size(2000), 500);
        assert_eq!(loadout_step_size(5000), 1000);
    }

    #[test]
    fn valid_int_matches_gdscript_is_valid_int() {
        assert!(is_valid_int("42"));
        assert!(is_valid_int("-7"));
        assert!(is_valid_int(" 12 "));
        assert!(!is_valid_int(""));
        assert!(!is_valid_int("4.2"));
        assert!(!is_valid_int("abc"));
    }
}
