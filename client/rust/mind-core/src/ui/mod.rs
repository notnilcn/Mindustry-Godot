// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Godot-free UI logic (plan 14).
//!
//! Everything here is pure data/format/parse code with no Godot, tokio or
//! network dependency (HLP §2.2, plan 14 §3.10). The Godot-facing widget layer
//! lives in `mind-gdext::ui` plus GDScript under `client/ui` and
//! `client/scenes/ui`.

pub mod builder;
pub mod hud_text;
pub mod manifest;
pub mod text;
