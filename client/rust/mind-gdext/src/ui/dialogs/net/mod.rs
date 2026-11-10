// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Multiplayer dialogs: host, join, bans, admins and trace. Filled by S07 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod admins;
pub mod bans;
pub mod host;
pub mod join;
pub mod roster;
pub mod trace;
pub mod util;
pub mod util_h;
pub mod util_misc;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    host::dispatch(id)
        .or_else(|| join::dispatch(id))
        .or_else(|| bans::dispatch(id))
        .or_else(|| admins::dispatch(id))
        .or_else(|| trace::dispatch(id))
}
