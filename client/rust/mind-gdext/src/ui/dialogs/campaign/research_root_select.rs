// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The research root-select dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/ResearchDialog.java (inline root-select BaseDialog, ~:86)

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use godot::builtin::{VarArray, VarDictionary};
use godot::classes::{Button, Control, HBoxContainer, Label};
use godot::prelude::*;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "research_root_select" => Some(Box::new(ResearchRootSelectDialog)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ResearchRootSelectDialog
// ---------------------------------------------------------------------------

/// One 300x60 `flatTogglet` icon+name row per selectable tech-tree root.
pub struct ResearchRootSelectDialog;

/// `iconMed` (`Scl.scl(32f)`).
const ICON_SIZE: f32 = 32.0;

impl DialogController for ResearchRootSelectDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@techtree.select");
        dialog.should_pause = false;
        dialog.add_default_close_button();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // A reused dialog node: drop the previous open's body before rebuilding.
        dialog.clear_content();
        let mut table = dialog.content_table();
        {
            let mut control: Gd<Control> = table.clone().upcast();
            control.add_theme_constant_override("separation", 0);
            control.set_custom_minimum_size(Vector2::new(300.0, 0.0));
            control.set_h_size_flags(godot::classes::control::SizeFlags::SHRINK_CENTER);
        }
        let selected = dialog.context_string("selected").to_string();
        let roots = dialog
            .context_value("roots")
            .map(|value| value.to::<VarArray>())
            .unwrap_or_default();
        for root in roots.iter_shared() {
            let Ok(entry) = root.try_to::<VarDictionary>() else {
                continue;
            };
            // code-instantiated: root rows come from the tree JSON roots list.
            let name = entry
                .get("name")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let mut row_button = Button::new_alloc();
            row_button.set_theme_type_variation("flatTogglet");
            row_button.set_toggle_mode(true);
            row_button.set_pressed(
                entry
                    .get("selected")
                    .map(|value| value.to::<bool>())
                    .unwrap_or(false)
                    || name.to_string() == selected,
            );
            row_button.set_custom_minimum_size(Vector2::new(300.0, 60.0));
            // code-instantiated: row content (icon + name) is data-driven.
            let mut content = HBoxContainer::new_alloc();
            content.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
            content
                .set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
            content.add_theme_constant_override("separation", 8);
            content.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
            row_button.add_child(&content);
            // Java `.marginLeft(12f)`.
            let mut margin = Control::new_alloc();
            margin.set_custom_minimum_size(Vector2::new(12.0, 0.0));
            margin.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
            content.add_child(&margin);
            let icon_name = entry
                .get("icon")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let type_name = entry
                .get("type")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let mut icon =
                factory::content_icon(&icon_name.to_string(), &type_name.to_string(), ICON_SIZE)
                    .unwrap_or_else(|| {
                        let mut spacer = Control::new_alloc();
                        spacer.set_custom_minimum_size(Vector2::new(ICON_SIZE, ICON_SIZE));
                        spacer
                    });
            icon.set_custom_minimum_size(Vector2::new(ICON_SIZE, ICON_SIZE));
            icon.set_v_size_flags(godot::classes::control::SizeFlags::SHRINK_CENTER);
            icon.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
            content.add_child(&icon);
            let title = entry
                .get("title")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let mut label = Label::new_alloc();
            label.set_text(&title);
            label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
            label.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
            content.add_child(&label);
            let mut target = dialog.to_gd();
            let name_for_pick = name.clone();
            let callable = Callable::from_fn("choose_root", move |_args: &[&Variant]| {
                if name_for_pick.is_empty() {
                    return;
                }
                let _ = target.emit_signal("root_selected", &[name_for_pick.to_variant()]);
            });
            row_button.connect("pressed", &callable);
            table.bind_mut().add(&row_button).width(300.0).height(60.0);
            table.bind_mut().row();
        }
    }
}
