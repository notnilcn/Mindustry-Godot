// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-facing UI layer (plan 14 §3.1).
//!
//! `ui_host` owns the `MindUi` autoload (dialog registry, pause governor,
//! prompt/toast signals). `hud` (the `MindHud` read-only HUD surface) and the
//! MSUI `dsl_factory` bridge land with plan 14 M2/M4/M6.

pub mod ui_host;

pub use ui_host::MindUi;
