// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Map-list and custom-game dialogs. Filled by S03 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod custom_game;
pub mod map_list;
pub mod map_play;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    custom_game::dispatch(id).or_else(|| map_play::dispatch(id))
}
