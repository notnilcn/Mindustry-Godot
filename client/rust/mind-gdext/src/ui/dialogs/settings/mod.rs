// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Settings dialogs: the menu, language and keybinds. Filled by S06 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod keybind;
pub mod language;
pub mod settings_menu;
pub mod util;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    settings_menu::dispatch(id)
        .or_else(|| language::dispatch(id))
        .or_else(|| keybind::dispatch(id))
}
