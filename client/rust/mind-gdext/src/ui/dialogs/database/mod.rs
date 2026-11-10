// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Database dialogs: effects, the content database and content info. Filled by S06 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod content_info;
pub mod database_dialog;
pub mod effects;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    effects::dispatch(id)
        .or_else(|| database_dialog::dispatch(id))
        .or_else(|| content_info::dispatch(id))
}
