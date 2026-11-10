// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to dialogs/campaign/planet/. Deleted in W5 (C01).
//! Upstream: none (port-only; the planet dialog lives in `dialogs/campaign/planet`).

use super::super::dialog::DialogController;

/// The dialog controllers of the old batch (all moved out).
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
