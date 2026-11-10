// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The sector stats dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/PlanetDialog.java (showStats)

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
