// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to `editor/`. Deleted in W5 (C01).

use super::super::dialog::DialogController;

/// The controllers moved to the editor leaves; nothing is dispatched here.
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
