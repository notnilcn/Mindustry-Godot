// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/logic/LogicDialog.java, LCanvas.java,
//         LStatement.java (plan 13 M4, I03 T7-T9).

//! Moved to `dialogs/logic/editor/`. Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

/// The controller in this batch (moved; see `logic/editor`).
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
