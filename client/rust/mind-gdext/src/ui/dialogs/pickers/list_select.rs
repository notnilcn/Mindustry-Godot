// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The generic list-select picker controller: a scrollable list of titled rows
//! with an optional reset row and a `selected(name)` signal.
//! Upstream: none (port-only; generic select picker)

use godot::classes::{HBoxContainer, VBoxContainer};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::net::util::string_array;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "list_select" => Some(Box::new(ListSelectDialog::default())),
        _ => None,
    }
}

/// Generic one-column picker: a scrollable list of titled rows, an optional
/// reset row, and a `selected(name)` signal (empty name = reset/closed).
#[derive(Default)]
pub struct ListSelectDialog {
    items: Vec<String>,
    reset_key: String,
    /// Set while the dialog is hiding after a pick, so `hidden` does not emit.
    chosen: bool,
}

impl DialogController for ListSelectDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        dialog.add_default_close_button();
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let title = dialog.context_string("title");
        dialog.title_text = if title.is_empty() {
            GString::new()
        } else {
            kit::t(&title.to_string())
        };
        dialog.refresh_title();
        self.reset_key = dialog.context_string("reset").to_string();
        self.items = string_array(dialog.context_value("items"));
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.chosen = false;
        self.build(dialog);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        let mut chosen = self.chosen;
        if dialog.base().has_meta("_picker_chosen") {
            chosen = dialog.base().get_meta("_picker_chosen").to::<bool>();
            dialog.base_mut().remove_meta("_picker_chosen");
        }
        if !chosen {
            dialog
                .to_gd()
                .emit_signal("selected", &[GString::new().to_variant()]);
        }
        self.chosen = false;
    }
}

impl ListSelectDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        let mut scroll = MindScroll::new_alloc();
        // The dialog shell sizes `cont` from its children's minimums; without
        // this the picker renders collapsed for large item lists.
        scroll.set_custom_minimum_size(Vector2::new(520.0, 320.0));
        scroll.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.bind_mut().set_scrolling_disabled(true, false);
        root.bind_mut().add(&scroll).grow_x().grow_y();
        // code-instantiated: the row list is caller-supplied picker data.
        let mut list = VBoxContainer::new_alloc();
        list.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.add_child(&list);
        if !self.reset_key.is_empty() {
            let reset_text = kit::t(&self.reset_key).to_string();
            let mut reset = factory::icon_button(
                "cancel",
                &reset_text,
                "defaultt",
                godot::classes::box_container::AlignmentMode::BEGIN,
            );
            let mut target = dialog.to_gd();
            let callable = Callable::from_fn("pick_reset", move |_args: &[&Variant]| {
                let _ = target.call("pick_selected", &[GString::new().to_variant()]);
            });
            reset.connect("pressed", &callable);
            list.add_child(&reset);
        }
        let columns = 3usize;
        let mut row: Option<Gd<HBoxContainer>> = None;
        for (index, item) in self.items.clone().into_iter().enumerate() {
            // code-instantiated: picker rows are data-driven (units/effects/spawns).
            let mut button = factory::button(&item);
            button.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
            let mut target = dialog.to_gd();
            let item_for_pick = item.clone();
            let callable = Callable::from_fn("pick_item", move |_args: &[&Variant]| {
                let _ = target.call(
                    "pick_selected",
                    &[GString::from(item_for_pick.as_str()).to_variant()],
                );
            });
            button.connect("pressed", &callable);
            if index % columns == 0 {
                let mut new_row = HBoxContainer::new_alloc();
                new_row.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
                list.add_child(&new_row);
                row = Some(new_row);
            }
            if let Some(mut row) = row.clone() {
                row.add_child(&button);
            }
        }
        if self.items.is_empty() {
            let label = factory::label(&kit::t("@none.found").to_string());
            list.add_child(&label);
        }
        let _ = &mut root;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn string_array_handles_missing_context() {
        assert!(string_array(None).is_empty());
    }
}
