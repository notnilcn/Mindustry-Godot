// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic canvas edit dialog controller. Filled by S09 (docs/PLANS).
//! Upstream: ui/dialogs/CanvasEditDialog.java, world/blocks/logic/CanvasBlock.java

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
