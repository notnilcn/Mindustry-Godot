// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The sector select dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/SectorSelectDialog.java

use super::util::campaign_views_of;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindRowTreeLayout};
use godot::builtin::VarArray;
use godot::classes::{HBoxContainer, LineEdit};
use godot::prelude::*;
use mind_core::ui::campaign::SectorView;

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
        if let Some(views) = campaign_views_of(dialog, &self.planet) {
            for sector in &views.sectors {
                if sector.planet != self.planet {
                    continue;
                }
                let Some(preset) = sector.preset.as_deref() else {
                    continue;
                };
                if preset.is_empty() {
                    continue;
                }
                if !needle.is_empty() && !sector.name.to_lowercase().contains(&needle) {
                    continue;
                }
                entries.push(&sector_entry_row(sector).to_variant());
            }
        }
        layout.bind_mut().set_entries(entries);
    }
}

/// The four fields `MindRowTreeLayout::set_entries` reads (`name`, `locked`,
/// `threat_band`, `id`).
///
/// The layout is outside T02's owned paths and its `#[func]` surface takes
/// untyped rows, so the dialog hands it a minimal dictionary built from the
/// typed view instead of the whole `campaign_views_json` row. Follow-up for
/// C01: a typed `set_sector_views(&[SectorView])` on the layout would drop this
/// adapter.
fn sector_entry_row(sector: &SectorView) -> VarDictionary {
    let mut row = VarDictionary::new();
    row.set("id", &i64::from(sector.id).to_variant());
    row.set("name", &GString::from(sector.name.as_str()));
    row.set("locked", &sector.locked.to_variant());
    row.set("threat_band", &GString::from(sector.threat_band));
    row
}
