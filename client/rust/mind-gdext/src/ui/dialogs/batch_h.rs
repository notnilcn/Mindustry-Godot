// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to `net/` (`util_h`, `host`) and `game/load`, `campaign/campaign_rules`.
//! Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

/// The dialog controllers in this batch (all moved out).
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
