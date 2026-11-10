// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Developer-only dialogs. Filled by S08 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod widget_gallery;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    widget_gallery::dispatch(id)
}
