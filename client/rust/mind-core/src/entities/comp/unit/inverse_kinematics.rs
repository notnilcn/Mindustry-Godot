// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan-11 IK re-export (plan 11 §3.1/§4.1).
//!
//! The two-segment solver itself lives in
//! [`crate::render::math::inverse_kinematics`] (plan 16 M7 ported upstream
//! `graphics/InverseKinematics.java`). Plan 11 consumes it for `LegsComp` joint
//! placement; re-exporting avoids a second copy drifting from the source.

pub use crate::render::math::inverse_kinematics::{rotate, set_length, solve, solve_side};
