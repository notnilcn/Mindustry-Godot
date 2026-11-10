// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The database dialog controller. Filled by S06 (docs/PLANS).
//! Upstream: ui/dialogs/DatabaseDialog.java

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
