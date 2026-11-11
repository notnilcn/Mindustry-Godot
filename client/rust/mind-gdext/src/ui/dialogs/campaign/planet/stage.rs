// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Stage cover/restore while the planet page is shown.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use godot::classes::CanvasItem;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;

use super::util::*;
use super::view_binding::*;
use super::*;

// ---- stage cover/restore --------------------------------------------------

/// `_set_planet_view_active`: globe layer + the dialog's own backdrop/dim.
pub(super) fn planet_set_planet_view_active(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    active: bool,
) {
    if let Some(mut view) = planet_view() {
        view.bind_mut().set_active(active);
    }
    if let Some(backdrop) = dialog.base().get_node_or_null("SpaceBackdrop")
        && let Ok(mut backdrop) = backdrop.try_cast::<CanvasItem>()
    {
        backdrop.set_visible(!active);
    }
    if let Some(dim) = dialog.base().get_node_or_null("Dim")
        && let Ok(mut dim) = dim.try_cast::<CanvasItem>()
    {
        dim.set_visible(!active);
    }
    if !dialog.base().is_visible() {
        return;
    }
    if active || !st.saved_layers.is_empty() || planet_hud_visible() {
        planet_cover_stage(st);
    } else {
        planet_set_visible_at(MENU_GROUP_PATH, true);
        planet_set_visible_at(MENU_BACKGROUND_PATH, true);
    }
}

pub(super) fn planet_hud_visible() -> bool {
    let Some(hud) = kit::node(HUD_GROUP_PATH) else {
        return false;
    };
    hud.clone()
        .try_cast::<CanvasItem>()
        .map(|hud| hud.is_visible())
        .unwrap_or(false)
}

/// Captures (once) and hides every stage layer under the dialog.
pub(super) fn planet_cover_stage(st: &mut PlanetState) {
    if st.saved_layers.is_empty() {
        let mut paths: Vec<String> = vec![
            String::from(MENU_GROUP_PATH),
            String::from(MENU_BACKGROUND_PATH),
            String::from(HUD_GROUP_PATH),
            String::from(WORLD_PATH),
        ];
        if let Some(world) = kit::node(WORLD_PATH) {
            let layers = world
                .find_children_ex("*")
                .type_("CanvasLayer")
                .recursive(true)
                .owned(false)
                .done();
            for layer in layers.iter_shared() {
                paths.push(layer.clone().get_path().to_string());
            }
        }
        for path in paths {
            let Some(node) = kit::node(&path) else {
                continue;
            };
            if let Ok(canvas) = node.clone().try_cast::<CanvasItem>() {
                st.saved_layers.push((path, canvas.is_visible()));
            } else if let Ok(layer) = node.clone().try_cast::<godot::classes::CanvasLayer>() {
                st.saved_layers.push((path, layer.is_visible()));
            }
        }
    }
    let saved: Vec<String> = st
        .saved_layers
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    for path in saved {
        planet_set_visible_at(&path, false);
    }
}

/// Restores exactly the captured visibility (no-op when nothing was covered).
pub(super) fn planet_restore_stage(st: &mut PlanetState) {
    let saved = st.saved_layers.clone();
    for (path, visible) in saved {
        planet_set_visible_at(&path, visible);
    }
    st.saved_layers.clear();
}

pub(super) fn planet_set_visible_at(path: &str, value: bool) {
    let Some(node) = kit::node(path) else {
        return;
    };
    if let Ok(mut canvas) = node.clone().try_cast::<CanvasItem>() {
        canvas.set_visible(value);
    } else if let Ok(mut layer) = node.clone().try_cast::<godot::classes::CanvasLayer>() {
        layer.set_visible(value);
    }
}
