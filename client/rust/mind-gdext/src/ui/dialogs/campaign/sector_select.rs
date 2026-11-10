// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The sector select dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/SectorSelectDialog.java

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindRowTreeLayout};
use godot::builtin::{VarArray, VarDictionary};
use godot::classes::{HBoxContainer, LineEdit};
use godot::prelude::*;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "sector_select" => Some(Box::new(SectorSelectDialog::default())),
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
        for sector in dialog
            .campaign_section_for(&self.planet, "sectors")
            .iter_shared()
        {
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
