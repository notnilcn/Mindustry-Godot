// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot touch → core gesture bridge (plan 15 §3.1/§3.5).
//!
//! The pure Arc `GestureDetector` port lives in `mind_core::input::mobile` so it
//! is replayable headlessly (deviation I1); this module is the thin Godot-facing
//! surface that constructs the `InputHandler.add()` detector and re-exports the
//! gesture event type for the touch glue in [`super::mobile`].

pub use mind_core::input::mobile::{GestureDetector, GestureEvent};

/// The `GestureDetector` construction used by `InputHandler.add()`.
///
/// `new GestureDetector(20, 0.5f, 0.3f, 0.15f, this)` with the Arc signature
/// `(halfTapSquareSize, tapCountInterval, longPressDuration, maxFlingDelay)`, so
/// `tapCountInterval = 0.5 s` and `longPressDuration = 0.3 s`.
pub fn input_handler_detector() -> GestureDetector {
    GestureDetector::new()
}
