// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to `game/` (`util_b`, `paused`, `game_over`), `net/util` and
//! `net/join/` (`mod`, `version_gate`). Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

/// The dialog controllers in this batch (all moved out).
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
