// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The schematic info dialog controller. Filled by S09 (docs/PLANS).
//! Upstream: ui/dialogs/SchematicsDialog.java (SchematicInfoDialog, ~:774-834)

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
