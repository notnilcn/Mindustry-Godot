// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared pickers: list select, file chooser, icon select, color picker and palette. Filled by S03, S07, S08 and S10 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod color_picker;
pub mod file_chooser;
pub mod icon_select;
pub mod list_select;
pub mod palette;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    list_select::dispatch(id)
        .or_else(|| file_chooser::dispatch(id))
        .or_else(|| icon_select::dispatch(id))
        .or_else(|| color_picker::dispatch(id))
        .or_else(|| palette::dispatch(id))
}
