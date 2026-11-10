// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Custom-rules dialogs. Filled by S09 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod custom_rules;
pub mod util;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    custom_rules::dispatch(id)
}
