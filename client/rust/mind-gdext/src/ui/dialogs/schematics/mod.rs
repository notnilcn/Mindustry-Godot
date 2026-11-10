// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Schematic dialogs: the list, info, import/export and tag/edit sub-dialogs. Filled by S03 and S09 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod files;
pub mod import_export;
pub mod schematic_info;
pub mod schematics_dialog;
pub mod subdialogs;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    schematics_dialog::dispatch(id).or_else(|| schematic_info::dispatch(id))
}
