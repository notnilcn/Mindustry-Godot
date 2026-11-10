// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Mod dialogs: the mods list and the mod browser. Filled by S10 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod mod_browser;
pub mod mods_dialog;
pub mod util;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    mods_dialog::dispatch(id).or_else(|| mod_browser::dispatch(id))
}
