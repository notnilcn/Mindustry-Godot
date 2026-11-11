// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/PlanetDialog.java (plan 14 M5).
//
//! Campaign entry: the planet dialog controller.
//!
//! The controller binds the data-driven part of the planet page to the shared
//! [`MindDialog`] shell: the first-run campaign chooser, the planet/difficulty
//! rail, the globe input (drag / wheel / tap), the collapsible sector list and
//! the selected-sector readout with its in-panel action button. Static
//! structure lives in the dialog scene; every row here is rebuilt from the
//! `MindCampaign` read models (`campaign_views_json`, `launch_dialog_json`)
//! and the `MindPreview` map registry, so the controller stays UI-only.

mod chooser;
mod globe_input;
mod launch;
mod page;
mod reads;
mod reveal;
mod sector_panel;
mod stage;
mod util;
mod view_binding;

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::control::MouseFilter;
use godot::classes::{Button, Control, PanelContainer, VBoxContainer};
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::{self, MindTable};
use mind_core::ui::campaign::CampaignViews;

use self::chooser::{
    planet_add_space_backdrop, planet_apply_full_dialog_margins, planet_maybe_show_remap_notice,
    planet_show_select,
};
use self::globe_input::{planet_escape_step, planet_input};
use self::page::{
    planet_adopt_current_sector, planet_boot_planet, planet_planets_with_base,
    planet_remove_sector_list, planet_show_planet,
};
use self::reveal::{planet_process, planet_start_preset_reveal};
use self::sector_panel::planet_on_text_input;
use self::stage::{planet_restore_stage, planet_set_planet_view_active};
use self::util::{Deferred, connect_once, install_frame_tick, setting_flag};
use self::view_binding::planet_view;

#[cfg(test)]
use self::launch::planet_launch_clears_loadout;
#[cfg(test)]
use self::sector_panel::{SECTOR_MAP_LINKS, SECTOR_THREADS, sector_submission_link};

/// The planet dialog controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "planet" => Some(Box::new(PlanetDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// PlanetDialog
// ---------------------------------------------------------------------------

/// `PlanetDialog` chooser card size (`CARD_SIZE`).
const CARD_SIZE: Vector2 = Vector2::new(320.0, 320.0);
/// Chooser disc fallback diameter (`DISC_SIZE`).
const DISC_SIZE: f32 = 170.0;
/// Java card outline (`Styles.togglet`).
const CARD_OUTLINE: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `PlanetDialog` footer buttons (`(200, 54)`).
const FOOTER_BUTTON: Vector2 = Vector2::new(200.0, 54.0);

/// Controller state for the reused planet dialog node.
#[derive(Default)]
struct PlanetState {
    root: Option<Gd<MindTable>>,
    selected_planet: String,
    selected_sector: i64,
    ok_button: Option<Gd<Button>>,
    last_planet_written: String,
    chooser_active: bool,
    footer_entries: Vec<Gd<Button>>,
    complete_announced: String,
    sectors_shown: bool,
    sector_search: String,
    sector_list: Option<Gd<VBoxContainer>>,
    sector_list_backdrop: Option<Gd<PanelContainer>>,
    /// node path -> visibility captured when the page first covered the stage.
    saved_layers: Vec<(String, bool)>,
    dragged: bool,
    pressed_position: Vector2,
    press_on_globe: bool,
    rename_target: i64,
    icon_target: i64,
    last_sector_id: i64,
    new_presets: Vec<i64>,
    preset_show: f32,
    reveal_showed: bool,
    launch_mode: String,
    accelerator_x: i64,
    accelerator_y: i64,
    /// Last `Time.get_ticks_msec()` the reveal loop ran (frame delta source).
    last_tick_ms: u64,
    /// Escape cleared the sector selection; the frame tick rebuilds the page
    /// (`selectSector(null)`) once the dialog can be bound again.
    selection_cleared_pending: bool,
    /// `(planet, typed campaign_views)` cache of the dialog's campaign reads.
    views: Option<CampaignViews>,
    views_planet: String,
    /// Pending `call_deferred` work drained on the next frame.
    deferred: Deferred,
}

impl PlanetState {
    fn new() -> Self {
        Self {
            rename_target: -1,
            icon_target: -1,
            last_sector_id: -1,
            launch_mode: String::from("look"),
            accelerator_x: -1,
            accelerator_y: -1,
            ..Self::default()
        }
    }
}

/// Campaign entry: the first-run chooser and the planet/sector page.
pub struct PlanetDialog {
    state: Rc<RefCell<PlanetState>>,
}

impl Default for PlanetDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(PlanetState::new())),
        }
    }
}

impl DialogController for PlanetDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        planet_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        planet_shown(dialog, &self.state);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        planet_hidden(dialog, &self.state);
    }

    fn blocks_escape(&self) -> bool {
        // `PlanetDialog.java:91-102` handles Escape inside the dialog's own
        // key listener; the port's escape entry point is `MindUiRoot`'s
        // `on_escape`, which asks the top dialog before closing it. A consumed
        // press keeps the page (the deselect rebuild runs next frame in
        // `planet_process`); otherwise the page hides.
        planet_escape_step(&mut self.state.borrow_mut())
    }
}

fn planet_ready(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    dialog.set_title_literal("");
    dialog.should_pause = true;
    dialog.enable_full_dialog(true);
    planet_apply_full_dialog_margins(dialog);
    planet_add_space_backdrop(dialog);
    st.root = Some(dialog.content_table());
    planet_show_select(dialog, state, &mut st);

    // `ui.showTextInput` result for the sector rename pencil
    // (`PlanetDialog.java:1200-1206`).
    let target = dialog.to_gd();
    let state_for_input = state.clone();
    let callable = Callable::from_fn("planet_text_input", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_on_text_input(&mut bound, &state_for_input, &text);
    });
    if let Some(ui) = kit::ui() {
        connect_once(dialog, "ui_text_input", &ui, "text_input_result", &callable);
    }

    // Globe drag/wheel/tap: the dialog root Control owns the empty-screen
    // `gui_input` once `Dim` stops swallowing it (the page's screen input).
    if let Some(dim) = dialog.base().get_node_or_null("Dim")
        && let Ok(mut dim) = dim.try_cast::<Control>()
    {
        dim.set_mouse_filter(MouseFilter::IGNORE);
    }
    // `PanelContainer` defaults to STOP, so the bubble from the empty page area
    // (the globe is behind `Center/Panel/Layout/Body`) died at `Panel` before it
    // reached the dialog root — real mouse motion/click never ran `planet_input`
    // (PAR-0018). The planet page's panel is a transparent full-screen layout
    // shell, so let the pointer pass through to the root; the rail, sector list
    // and footer buttons still stop their own events, and `planet_over_ui`
    // additionally rejects their rects.
    if let Some(mut panel) = dialog.panel() {
        panel.set_mouse_filter(MouseFilter::PASS);
    }
    let target = dialog.to_gd();
    let state_for_click = state.clone();
    let callable = Callable::from_fn("planet_gui_input", move |args: &[&Variant]| {
        let Some(event) = args
            .first()
            .map(|value| (*value).to::<Gd<godot::classes::InputEvent>>())
        else {
            return;
        };
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_input(&mut bound, &state_for_click, event);
    });
    dialog.base_mut().connect("gui_input", &callable);

    // The reveal loop and the deferred rebuild queue share one frame hook.
    let target = dialog.to_gd();
    let state_for_frame = state.clone();
    let deferred = st.deferred.clone();
    let tick = move || {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_process(&mut bound, &state_for_frame);
    };
    install_frame_tick(dialog, "planet", deferred, tick);
}

fn planet_shown(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    // `PlanetDialog.show`: no sector selected when the page opens.
    st.selected_sector = -1;
    // An Escape pressed in the same frame the page closed must not fire a
    // stale deselect rebuild on the next open (it would clear `newPresets`
    // before the reveal starts).
    st.selection_cleared_pending = false;
    st.launch_mode = dialog.context_string("mode").to_string();
    if st.launch_mode.is_empty() {
        st.launch_mode = String::from("look");
    }
    st.accelerator_x = dialog.context_int("x");
    st.accelerator_y = dialog.context_int("y");
    planet_adopt_current_sector(dialog, &mut st);

    let mut chooser_shown = false;
    if st.selected_planet.is_empty() {
        let based = planet_planets_with_base(dialog, &mut st);
        if !setting_flag("campaignselect") && based.is_empty() {
            planet_show_select(dialog, state, &mut st);
            chooser_shown = true;
        } else {
            let planet = planet_boot_planet(dialog, &mut st, &based);
            planet_show_planet(dialog, state, &mut st, &planet, false);
        }
    } else {
        let planet = st.selected_planet.clone();
        planet_show_planet(dialog, state, &mut st, &planet, false);
    }

    if !chooser_shown {
        planet_start_preset_reveal(dialog, &mut st);
    }
    if st.launch_mode == "planetLaunch"
        && let Some(mut view) = planet_view()
    {
        view.bind_mut().zoom = 1.0;
        view.bind_mut().zoom_target = 1.0;
        view.bind_mut().ui_alpha = 0.0;
    }
    if !chooser_shown {
        st.deferred.push({
            let target = dialog.to_gd();
            move || {
                let mut bound = target.clone();
                let mut bound = bound.bind_mut();
                planet_maybe_show_remap_notice(&mut bound);
            }
        });
    }
}

fn planet_hidden(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    st.launch_mode = String::from("look");
    st.selection_cleared_pending = false;
    planet_remove_sector_list(&mut st);
    planet_set_planet_view_active(dialog, &mut st, false);
    planet_restore_stage(&mut st);
    if let Some(mut view) = planet_view() {
        view.bind_mut().set_hover(-1);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// PAR-0507: `PlanetDialog.playSelected` clears the stored loadout and
    /// launch resources only on the free-launch branch (`PlanetDialog.java:1396-1403`);
    /// a resume of a base (`:1449`) and a launcher launch keep them. The old
    /// code cleared on every plain `start_sector`, so resuming a base wiped
    /// `launch-resources-seq` and `lastloadout-<core>`.
    #[test]
    fn only_a_free_launch_clears_the_stored_loadout() {
        // Free launch: no launcher sector, no loadout, not a resume.
        assert!(planet_launch_clears_loadout(false, -1, -1));
        // Resume / go of a sector with a base.
        assert!(!planet_launch_clears_loadout(true, -1, -1));
        // Launcher launch with a picker selection.
        assert!(!planet_launch_clears_loadout(false, 42, 7));
        // `sectors.go` on a base while the launcher lookup returned nothing:
        // still a resume branch (Java `sector.hasBase()` -> playSector).
        assert!(!planet_launch_clears_loadout(true, 0, 7));
    }

    /// `SectorSubmissions.getSectorThread`: a map-file submission wins over the
    /// generic discussion thread, unknown cells are null, and every id is unique
    /// (Java's `threadMap` is keyed by sector).
    #[test]
    fn sector_submission_links_prefer_map_files_and_are_unique() {
        assert_eq!(SECTOR_THREADS.len(), 51);
        assert_eq!(SECTOR_MAP_LINKS.len(), 46);
        let mut ids: Vec<i64> = SECTOR_THREADS.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SECTOR_THREADS.len());
        let mut map_ids: Vec<i64> = SECTOR_MAP_LINKS.iter().map(|(id, _)| *id).collect();
        map_ids.sort_unstable();
        map_ids.dedup();
        assert_eq!(map_ids.len(), SECTOR_MAP_LINKS.len());
        // 76 has both a generic thread and the Tyrant-classZzz map submission.
        let tyrant = sector_submission_link(76).expect("sector 76 submission");
        assert!(tyrant.ends_with("/1479890094486716499"), "{tyrant}");
        // 103 only has the autogenerated discussion thread.
        let thread = sector_submission_link(103).expect("sector 103 thread");
        assert_eq!(
            thread,
            "https://discord.com/channels/391020510269669376/1379926839559979030"
        );
        // A sector with no entry returns null (`threadMap.get`).
        assert_eq!(sector_submission_link(999), None);
        assert_eq!(sector_submission_link(-1), None);
    }
}
