// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Math helpers additive to plan 05's scalar utilities (plan 06 §3.10, R6).
//!
//! Plan 05 did not land a `math/` module, so `noise` is hosted here (the plan's
//! additive-file intent); generators import `crate::math::noise`.

pub mod noise;
pub mod rand_arc;
pub mod ridged;
pub mod windowed_mean;

pub use rand_arc::ArcRand;
pub use windowed_mean::WindowedMean;
