// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor dialog controller; this directory holds its submodules. Filled by S09 and S10 (docs/PLANS).
//! Upstream: logic/LogicDialog.java, LCanvas.java, LStatement.java

use crate::ui::dialog::DialogController;

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
