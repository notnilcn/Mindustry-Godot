// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! In-game dialogs: pause, game over, load and save. Filled by S07 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod game_over;
pub mod load;
pub mod paused;
pub mod util_b;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    paused::dispatch(id)
        .or_else(|| game_over::dispatch(id))
        .or_else(|| load::dispatch(id))
}
