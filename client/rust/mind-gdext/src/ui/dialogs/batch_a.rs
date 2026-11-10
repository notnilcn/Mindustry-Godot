// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to `dialogs/settings` and `dialogs/database`. Deleted in W5 (C01).

pub use super::settings::util::open_content_info;

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
