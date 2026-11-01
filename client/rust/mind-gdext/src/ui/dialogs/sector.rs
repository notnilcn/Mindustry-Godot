// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Sector select, research-root select and the player-color palette.

use godot::classes::{Button, Control, HBoxContainer, Label, LineEdit};
use godot::obj::NewAlloc;
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindRowTreeLayout};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "sector_select" => Some(Box::new(SectorSelectDialog::default())),
        "research_root_select" => Some(Box::new(ResearchRootSelectDialog)),
        "palette" => Some(Box::new(PaletteDialog)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// SectorSelectDialog
// ---------------------------------------------------------------------------

/// Sector picker for campaign launch/sector-select mode.
#[derive(Default)]
pub struct SectorSelectDialog {
    layout: Option<Gd<MindRowTreeLayout>>,
    search: Option<Gd<LineEdit>>,
    planet: String,
}

impl DialogController for SectorSelectDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `cont.top()` (`SectorSelectDialog.java:27`).
        dialog.apply_cont_top(true);
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        self.build(dialog);
        dialog.add_close_button(0.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.planet = dialog.context_string("planet").to_string();
        self.rebuild(dialog);
    }
}

impl SectorSelectDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let mut root = dialog.content_table();
        let title =
            factory::styled_label(&kit::t("@sectors.launchselect").to_string(), "techLabel");
        root.bind_mut().add(&title).grow_x().pad(6.0);
        root.bind_mut().row();
        // code-instantiated: the search row is a single data-driven filter field.
        let mut search_row = HBoxContainer::new_alloc();
        let mut search = factory::field(&kit::t("@search").to_string());
        let mut target = dialog.to_gd();
        let callable = Callable::from_fn("sector_search", move |_args: &[&Variant]| {
            let _ = target.call("refresh_controller", &[]);
        });
        search.connect("text_changed", &callable);
        search_row.add_child(&search);
        root.bind_mut().add(&search_row).grow_x().pad(4.0);
        root.bind_mut().row();
        // code-instantiated: sector placement is computed from the plan-12 grid
        // via the shared row layout; cells are data-driven.
        let mut layout = MindRowTreeLayout::new_alloc();
        layout.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        let mut target = dialog.to_gd();
        let callable = Callable::from_fn("sector_activated", move |args: &[&Variant]| {
            if let Some(id) = args.first() {
                let value = (*id).clone();
                let _ = target.call("pick_sector", &[value]);
            }
        });
        layout.connect("sector_activated", &callable);
        root.bind_mut().add(&layout).grow_x().grow_y();
        self.search = Some(search);
        self.layout = Some(layout);
    }

    /// `SectorSelectDialog.matches`: the current planet's unlockable (authored)
    /// sectors matching the search text.
    fn rebuild(&mut self, dialog: &mut MindDialog) {
        let Some(mut layout) = self.layout.clone() else {
            return;
        };
        let needle = self
            .search
            .clone()
            .map(|search| search.get_text().to_string().to_lowercase())
            .unwrap_or_default();
        let mut entries = VarArray::new();
        for sector in dialog.campaign_section("sectors").iter_shared() {
            let Ok(sector) = sector.try_to::<VarDictionary>() else {
                continue;
            };
            let planet = sector
                .get("planet")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            if planet.to_string() != self.planet {
                continue;
            }
            let preset = sector
                .get("preset")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            if preset.is_empty() {
                continue;
            }
            let name = sector
                .get("name")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            if !needle.is_empty() && !name.to_string().to_lowercase().contains(&needle) {
                continue;
            }
            entries.push(&sector.to_variant());
        }
        layout.bind_mut().set_entries(entries);
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
        dialog.add_close_button(210.0);
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

// ---------------------------------------------------------------------------
// PaletteDialog
// ---------------------------------------------------------------------------

/// `Vars.playerColors` — names/values are the upstream ABI.
const PLAYER_COLORS: [&str; 16] = [
    "82759a", "c0c1c5", "ffffff", "7d2953", "ff074e", "ff072a", "ff76a6", "a95238", "ffa108",
    "feeb2c", "ffcaa8", "008551", "00e339", "423c7b", "4b5ef1", "2cabfe",
];

/// A 4-column grid of the 16 vanilla player colors.
pub struct PaletteDialog;

impl DialogController for PaletteDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@color");
        dialog.should_pause = false;
        let mut grid = dialog.content_table();
        {
            let mut control: Gd<Control> = grid.clone().upcast();
            control.add_theme_constant_override("separation", 4);
        }
        let mut column = 0usize;
        for hex in PLAYER_COLORS {
            let color = Color::from_string(hex).unwrap_or(Color::WHITE);
            // code-instantiated: palette entries come from the playerColors table.
            let mut swatch = Button::new_alloc();
            swatch.set_theme_type_variation("squareTogglei");
            swatch.set_custom_minimum_size(Vector2::new(48.0, 48.0));
            swatch.set_modulate(color);
            let mut target = dialog.to_gd();
            let callable = Callable::from_fn("choose_color", move |_args: &[&Variant]| {
                let _ = target.emit_signal("color_selected", &[color.to_variant()]);
                let _ = target.call("hide_dialog", &[]);
            });
            swatch.connect("pressed", &callable);
            grid.bind_mut().add(&swatch).size(48.0);
            column += 1;
            if column.is_multiple_of(4) {
                grid.bind_mut().row();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_colors_are_the_upstream_table() {
        assert_eq!(PLAYER_COLORS.len(), 16);
        assert_eq!(PLAYER_COLORS[0], "82759a");
        assert_eq!(PLAYER_COLORS[15], "2cabfe");
    }
}
