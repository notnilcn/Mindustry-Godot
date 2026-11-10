// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers for the standalone-menu dialogs (moved from the old simple.rs).
//! Upstream: none (port-only; shared helper of the old simple)

use godot::classes::RichTextLabel;
use godot::prelude::*;

use crate::ui::kit::MindLabel;

/// A rich label's engine handle (for text/color/alignment).
pub(super) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}
