// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Facade accessors shared by the dialog controllers (`MindUi` calls, rich labels).
//! Upstream: none (port-only; helpers duplicated in the old batch files)

use godot::classes::RichTextLabel;
use godot::prelude::*;

use crate::ui::kit;
use crate::ui::kit::MindLabel;

/// Calls one `MindUi` method (`None` when the facade or method is absent).
#[allow(dead_code)] // adopted in W2 (docs/PLANS)
pub(crate) fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// One `MindLabel`'s engine handle (text/colour/wrap on a rich label).
#[allow(dead_code)] // adopted in W2 (docs/PLANS)
pub(crate) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}
