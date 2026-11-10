// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/ColorPicker.java,
//         core/src/mindustry/ui/dialogs/ModsDialog.java,
//         core/src/mindustry/ui/dialogs/ModBrowserDialog.java.

//! Moved to `dialogs/pickers/color_picker.rs` and `dialogs/mods/`. Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

/// The controllers in this batch (moved; see the destination leaves).
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
