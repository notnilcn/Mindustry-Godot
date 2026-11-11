// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The preset reveal loop and the per-frame process hook.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::obj::Singleton;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;

use super::reads::*;
use super::util::*;
use super::view_binding::*;
use super::*;

// ---- reveal loop ----------------------------------------------------------

/// `_start_preset_reveal`: zoom reset, camera to the last sector, new presets.
pub(super) fn planet_start_preset_reveal(dialog: &mut MindDialog, st: &mut PlanetState) {
    st.new_presets.clear();
    st.preset_show = 0.0;
    st.reveal_showed = false;
    let planet = st.selected_planet.clone();
    if planet.is_empty() {
        return;
    }
    let Some(mut view) = planet_view() else {
        return;
    };
    view.bind_mut().zoom = 1.0;
    view.bind_mut().zoom_target = 1.0;
    let data = planet_view_data(&view);
    st.last_sector_id = row_i64_or(&data, "last_sector", -1);
    if let Some(last_sector) = planet_find_sector(dialog, st, &planet, st.last_sector_id) {
        let tile = planet_view_tile(&view, &last_sector);
        view.bind_mut().look_at_tile(tile as i32, 1.0);
    }
    let Some(mut campaign) = dialog.campaign_node() else {
        return;
    };
    if !campaign.has_method("take_new_presets") {
        return;
    }
    let collected = campaign
        .call(
            "take_new_presets",
            &[GString::from(planet.as_str()).to_variant()],
        )
        .to::<PackedInt32Array>();
    for id in collected.as_slice() {
        st.new_presets.push(*id as i64);
    }
    if !st.new_presets.is_empty() {
        st.new_presets.push(st.last_sector_id);
        st.new_presets.reverse();
    }
}

/// `_process`: the `newPresets` reveal loop (`PlanetDialog.java:986-1003`) plus
/// the deferred deselect rebuild (`PlanetDialog.java:94` `selectSector(null)`).
pub(super) fn planet_process(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    if !dialog.base().is_visible() {
        return;
    }
    if st.selection_cleared_pending {
        // The Escape press ran inside `blocks_escape(&self)`, so it cannot
        // rebuild the page itself; do it here with the dialog bound.
        st.selection_cleared_pending = false;
        let planet = st.selected_planet.clone();
        if !planet.is_empty() {
            planet_show_planet(dialog, state, &mut st, &planet, false);
        }
        return;
    }
    if st.new_presets.is_empty() {
        return;
    }
    let Some(mut view) = planet_view() else {
        return;
    };
    let now = godot::classes::Time::singleton().get_ticks_msec();
    let delta = if st.last_tick_ms == 0 {
        1.0 / 60.0
    } else {
        ((now.saturating_sub(st.last_tick_ms)) as f32 / 1000.0).clamp(0.0, 0.25)
    };
    st.last_tick_ms = now;
    st.preset_show += delta * 60.0;
    let to_id = st.new_presets.last().copied().unwrap_or(-1);
    let planet = st.selected_planet.clone();
    let sector = planet_find_sector(dialog, &mut st, &planet, to_id);
    if let Some(sector) = sector.as_ref() {
        let tile = planet_view_tile(&view, sector);
        view.bind_mut().look_at_tile(tile as i32, 0.11);
    }
    view.bind_mut().zoom_target = 0.75;
    if st.preset_show >= 20.0 && !st.reveal_showed && st.new_presets.len() > 1 {
        st.reveal_showed = true;
        let text = format!(
            "{} [accent]{}",
            kit::glyph("lock-open"),
            sector
                .as_ref()
                .map(planet_localized_sector)
                .unwrap_or_default()
        );
        let _ = ui_call(
            "announce",
            &[
                GString::from(text.as_str()).to_variant(),
                2.0f32.to_variant(),
            ],
        );
    }
    if st.preset_show > 144.0 {
        st.new_presets.pop();
        st.reveal_showed = false;
        st.preset_show = 0.0;
    }
}
