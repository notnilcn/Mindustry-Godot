// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Binding to the live `MindPlanetView` node.
//! Upstream: none (port-only; the planet view binding seam).

use godot::builtin::VarDictionary;
use godot::prelude::*;
use mind_core::ui::campaign::SectorView;

use crate::ui::kit;
use crate::ui::planet::MindPlanetView;

use super::util::PLANET_VIEW_PATH;

// ---- planet view binding --------------------------------------------------

/// The live planet view (`MindPlanetView`, `ui/planet.rs`).
pub(super) fn planet_view() -> Option<Gd<MindPlanetView>> {
    kit::node(PLANET_VIEW_PATH)?
        .try_cast::<MindPlanetView>()
        .ok()
}

/// `PlanetView.view_tile(sector)`: the sector view's remapped grid tile.
pub(super) fn planet_view_tile(view: &Gd<MindPlanetView>, sector: &SectorView) -> i64 {
    let tile = view
        .bind()
        .view_tile_index(i64::from(sector.id), sector.preset.as_deref().unwrap_or(""));
    i64::from(tile)
}

/// `PlanetView.pick(viewport_position)` (`-1` when no tile is under the cursor).
pub(super) fn planet_view_pick(view: &Gd<MindPlanetView>, point: Vector2) -> i64 {
    i64::from(view.bind().pick(point))
}

/// `PlanetView.data` (the `planet_view` grid topology dictionary).
pub(super) fn planet_view_data(view: &Gd<MindPlanetView>) -> VarDictionary {
    view.bind().data.clone()
}

/// `PlanetView.hover_tile`.
pub(super) fn planet_view_hover(view: &Gd<MindPlanetView>) -> i64 {
    i64::from(view.bind().hover_tile)
}
