// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Globe input (drag/wheel/tap), hover and escape handling.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::prelude::*;
use mind_core::ui::campaign::SectorView;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::planet::MindPlanetView;

use super::reads::*;
use super::util::*;
use super::view_binding::*;
use super::*;

// ---- globe input ----------------------------------------------------------

/// Left-drag orbits, the wheel zooms, a tap selects (`PlanetDialog.java:114`).
pub(super) fn planet_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    event: Gd<godot::classes::InputEvent>,
) {
    if !dialog.base().is_visible() || !dialog.is_top() {
        return;
    }
    let mut st = state.borrow_mut();
    if let Ok(key) = event.clone().try_cast::<godot::classes::InputEventKey>() {
        planet_handle_reveal_escape(dialog, &mut st, &key);
        return;
    }
    if let Ok(motion) = event
        .clone()
        .try_cast::<godot::classes::InputEventMouseMotion>()
    {
        let Some(mut view) = planet_view() else {
            return;
        };
        if !view.is_visible() {
            return;
        }
        let mask = motion.get_button_mask();
        if mask.ord() & godot::global::MouseButtonMask::LEFT.ord() != 0 {
            if !st.press_on_globe {
                return;
            }
            if !st.new_presets.is_empty()
                && st.new_presets.last().copied() != Some(st.last_sector_id)
            {
                return;
            }
            st.new_presets.clear();
            st.dragged = true;
            view.bind_mut().drag(motion.get_relative());
            if let Some(mut viewport) = dialog.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            return;
        }
        planet_update_hover(dialog, &mut st, &view, motion.get_position());
        return;
    }
    let Ok(click) = event.try_cast::<godot::classes::InputEventMouseButton>() else {
        return;
    };
    let button = click.get_button_index();
    if button == godot::global::MouseButton::WHEEL_UP
        || button == godot::global::MouseButton::WHEEL_DOWN
    {
        let Some(mut view) = planet_view() else {
            return;
        };
        if !view.is_visible() {
            return;
        }
        if planet_over_ui(dialog, &st, click.get_position()) {
            return;
        }
        if !click.is_pressed() {
            if let Some(mut viewport) = dialog.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            return;
        }
        let amount = if button == godot::global::MouseButton::WHEEL_UP {
            -1.0f32
        } else {
            1.0f32
        };
        view.bind_mut().scroll(amount);
        if let Some(mut viewport) = dialog.base().get_viewport() {
            viewport.set_input_as_handled();
        }
        return;
    }
    if button != godot::global::MouseButton::LEFT {
        return;
    }
    let Some(view) = planet_view() else {
        return;
    };
    if !view.is_visible() {
        return;
    }
    if click.is_pressed() {
        st.pressed_position = click.get_position();
        st.dragged = false;
        st.press_on_globe = !planet_over_ui(dialog, &st, click.get_position());
        return;
    }
    if st.dragged {
        st.dragged = false;
        st.press_on_globe = false;
        return;
    }
    if !st.press_on_globe {
        return;
    }
    st.press_on_globe = false;
    if click.get_position().distance_to(st.pressed_position) > 4.0 {
        return;
    }
    if planet_over_ui(dialog, &st, click.get_position()) {
        return;
    }
    let tile = planet_view_pick(&view, click.get_position());
    if tile < 0 {
        return;
    }
    planet_select_view_tile(dialog, state, &mut st, tile);
    planet_update_hover(dialog, &mut st, &view, click.get_position());
    if let Some(mut viewport) = dialog.base().get_viewport() {
        viewport.set_input_as_handled();
    }
}

/// One Escape press on the planet page (`PlanetDialog.java:91-102`): while the
/// reveal runs it keeps only the first entry (the last sector); then a selected
/// sector is cleared — `selectSector(null)` hides the detail/action panel and
/// the frame tick rebuilds the page without a selection; only when nothing is
/// selected does the press fall through to hiding the dialog. Returns whether
/// the press was consumed (the port's `blocks_escape` contract).
///
/// The first-run chooser keeps its Escape blocked (the campaign choice is
/// mandatory; `_show_select` has no back action), unchanged from before.
pub(super) fn planet_escape_step(st: &mut PlanetState) -> bool {
    if st.chooser_active {
        return true;
    }
    if st.new_presets.len() > 1 {
        st.new_presets.truncate(1);
        st.preset_show = 0.0;
        st.reveal_showed = false;
        return true;
    }
    if st.selected_sector >= 0 {
        st.selected_sector = -1;
        st.selection_cleared_pending = true;
        return true;
    }
    false
}

/// Escape during the planet page (`PlanetDialog.java:91-102`), reached when the
/// dialog root itself gets the key (`gui_input`).
pub(super) fn planet_handle_reveal_escape(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    key: &Gd<godot::classes::InputEventKey>,
) {
    if !key.is_pressed() || key.is_echo() || key.get_keycode() != godot::global::Key::ESCAPE {
        return;
    }
    if !planet_escape_step(st) {
        return;
    }
    if let Some(mut viewport) = dialog.base().get_viewport() {
        viewport.set_input_as_handled();
    }
}

/// `_select_view_tile`: selects the campaign row whose grid tile matches.
pub(super) fn planet_select_view_tile(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    tile: i64,
) {
    let planet = st.selected_planet.clone();
    let sectors = planet_sectors(dialog, st, &planet);
    let launch_mode = st.launch_mode.clone();
    for sector in sectors {
        let Some(view) = planet_view() else {
            return;
        };
        if planet_view_tile(&view, &sector) != tile {
            continue;
        }
        let selectable = if launch_mode == "planetLaunch" {
            sector.accelerator_selectable
        } else {
            sector.selectable
        };
        if !selectable {
            return;
        }
        st.selected_sector = i64::from(sector.id);
        planet_show_planet(dialog, state, st, &planet, true);
        return;
    }
}

/// `_update_hover`: fades the grid in around the sector under the cursor.
pub(super) fn planet_update_hover(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    view: &Gd<MindPlanetView>,
    point: Vector2,
) {
    let mut tile = -1i64;
    if !planet_over_ui(dialog, st, point) {
        tile = planet_view_pick(view, point);
    }
    if tile == planet_view_hover(view) {
        return;
    }
    let mut name_text = String::new();
    if tile >= 0 {
        let planet = st.selected_planet.clone();
        for sector in planet_sectors(dialog, st, &planet) {
            let sector_tile = planet_view_tile(view, &sector);
            if sector_tile != tile {
                continue;
            }
            // `PlanetDialog.draw` hover label (`PlanetDialog.java:956-977`):
            // selectable cells show `[accent][[ [white]name[accent] ]`; a
            // locked numbered/submission cell next to a base explains the
            // reason from the generator (`SerpuloPlanetGenerator
            // .getLockedText`); `planetLaunch` shows the cancel glyph.
            let selectable = if st.launch_mode == "planetLaunch" {
                sector.accelerator_selectable
            } else {
                sector.selectable
            };
            if selectable {
                name_text = format!(
                    "[color=#ffd37fff][lb] [color=white]{}[color=#ffd37fff] ]",
                    planet_localized_sector(&sector)
                );
            } else if !sector.hover_allowed {
                name_text = String::new();
            } else if st.launch_mode == "planetLaunch" {
                name_text = format!("[color=#7f7f7fff]{}[/color]", kit::icons::glyph("cancel"));
            } else if st.launch_mode != "select" {
                name_text = planet_hover_lock_text(&sector);
            }
            break;
        }
    }
    let mut view = view.clone();
    view.bind_mut()
        .set_sector_label_markup(gs(name_text.as_str()));
    view.bind_mut().set_hover(tile as i32);
}

/// `PlanetGenerator.getLockedText` (`SerpuloPlanetGenerator.java:114-123`):
/// the reason a non-selectable cell shows under the cursor. The reason kind
/// comes from the sector row (`hover_lock`, computed host-side).
pub(super) fn planet_hover_lock_text(sector: &SectorView) -> String {
    match sector.hover_lock.unwrap_or("") {
        "shielded" => format!(
            "[color=#ff0000ff]{}[/color]{}",
            kit::icons::glyph("defense"),
            kit::tm("@sector.shielded")
        ),
        "foundation" => {
            let emoji = kit::assets()
                .map(|assets| {
                    assets
                        .bind()
                        .unicode_str(GString::from("core-foundation"))
                        .to_string()
                })
                .unwrap_or_default();
            format!(
                "[color=#ff0000ff]{}[/color]{}{}",
                kit::icons::glyph("cancel"),
                emoji,
                kit::tm("@sector.foundationrequired")
            )
        }
        "locked" => format!(
            "[color=#7f7f7fff]{} {}[/color]",
            kit::icons::glyph("lock"),
            kit::t("@locked")
        ),
        _ => String::new(),
    }
}

/// `_over_ui`: the rail, the sector list and the button row are not the globe.
pub(super) fn planet_over_ui(dialog: &mut MindDialog, st: &PlanetState, point: Vector2) -> bool {
    if let Some(root) = st.root.clone()
        && root.get_global_rect().contains_point(point)
    {
        return true;
    }
    if let Some(list) = st.sector_list.clone()
        && list.get_global_rect().contains_point(point)
    {
        return true;
    }
    if let Some(buttons) = dialog.buttons_row()
        && buttons.get_global_rect().contains_point(point)
    {
        return true;
    }
    false
}
