// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Logic-editor dialogs. Filled by S09 and S10 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod canvas_edit;
pub mod editor;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    canvas_edit::dispatch(id).or_else(|| editor::dispatch(id))
}
