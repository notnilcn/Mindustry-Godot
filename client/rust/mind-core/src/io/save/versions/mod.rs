// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Native save format versions (plan 04 §3.3). Append-only: one module per
//! version, wired into [`super::version::version_array`].

pub mod v1;
