// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Defense-block behaviors owned by plan 10 (`world/blocks/defense/*.java`).
//!
//! Plan 10 §3.1 lays out `base_shield`/`force_projector`/`mend_projector`/
//! `shield_wall`/`shock_mine`/`target_dummy` plus the `turrets/` subtree.
//! Turrets are the first family landed (M5); the projector/shield family (M7)
//! lives in [`projectors`]/[`behaviors`].

pub mod behaviors;
pub mod projectors;
pub mod shields;
pub mod turrets;

pub use behaviors::register;
