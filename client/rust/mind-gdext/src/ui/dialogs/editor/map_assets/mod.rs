// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map assets dialog controller; this directory holds its submodules. Filled by S08 and S09 (docs/PLANS).
//! Upstream: editor/data/MapAssetsDialog.java

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
