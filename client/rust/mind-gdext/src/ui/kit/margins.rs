// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/core/UI.java (`updateMargins`/`uiEdgePadding`).

//! `MindUiMargins` — safe-area/edge-padding helper for HUD gutters.
//!
//! Reads the authoritative insets from the `MindUi` autoload (Rust) so
//! fragments never compute margins themselves.

use super::ui;

fn margin(accessor: &str) -> f32 {
    let Some(mut ui) = ui() else {
        return 0.0;
    };
    if !ui.has_method(accessor) {
        return 0.0;
    }
    ui.call(accessor, &[]).to::<i32>() as f32
}

pub fn left() -> f32 {
    margin("margin_left")
}

#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn right() -> f32 {
    margin("margin_right")
}

#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn top() -> f32 {
    margin("margin_top")
}

#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn bottom() -> f32 {
    margin("margin_bottom")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ui_reports_zero() {
        // No `MindUi` in a unit-test host: every gutter collapses to zero.
        assert_eq!(left(), 0.0);
        assert_eq!(right(), 0.0);
        assert_eq!(top(), 0.0);
        assert_eq!(bottom(), 0.0);
    }
}
