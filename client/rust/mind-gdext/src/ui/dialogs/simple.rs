// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to `dialogs/menu`. Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
