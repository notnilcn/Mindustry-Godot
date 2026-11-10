// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Binding to the live `MindPlanetView` node.
//! Upstream: none (port-only; the planet view binding seam).

use godot::classes::Node;
use godot::prelude::*;

use super::util::*;
use super::*;

// ---- planet view binding --------------------------------------------------

/// The live planet view (`MindPlanetView`, `ui/planet.rs`).
pub(super) fn planet_view() -> Option<Gd<Node>> {
    kit::node(PLANET_VIEW_PATH)
}

/// Calls a planet-view method when it exists.
pub(super) fn planet_view_call(
    view: &mut Gd<Node>,
    method: &str,
    args: &[Variant],
) -> Option<Variant> {
    if !view.has_method(method) {
        return None;
    }
    Some(view.call(method, args))
}

/// `PlanetView.view_tile(sector)`: the campaign row's remapped grid tile.
pub(super) fn planet_view_tile(view: &mut Gd<Node>, sector: &VarDictionary) -> i64 {
    planet_view_call(view, "view_tile", &[sector.to_variant()])
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or(-1)
}

/// `PlanetView.pick(viewport_position)` (`-1` when no tile is under the cursor).
pub(super) fn planet_view_pick(view: &mut Gd<Node>, point: Vector2) -> i64 {
    planet_view_call(view, "pick", &[point.to_variant()])
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or(-1)
}

/// `PlanetView.data` (the `planet_view` grid topology dictionary).
pub(super) fn planet_view_data(view: &mut Gd<Node>) -> VarDictionary {
    view.get("data")
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `PlanetView.hover_tile`.
pub(super) fn planet_view_hover(view: &mut Gd<Node>) -> i64 {
    view.get("hover_tile").try_to::<i64>().unwrap_or(-1)
}
