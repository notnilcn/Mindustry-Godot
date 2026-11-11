// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Helpers shared by the dialog controllers (deferred actions, facade calls,
//! prompt wiring).
//! Upstream: none (port-only; helpers duplicated in the old batch files)

pub mod defer;
pub mod facade;
pub mod prompt;
