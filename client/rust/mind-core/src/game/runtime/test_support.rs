// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared fixtures for the campaign runtime tests (the `game::test_support`
//! registry and campaign, re-exported under the runtime tests' names).
//! Upstream: none (port-only test support).

pub(super) use crate::game::test_support::{campaign, raw_registry as content};
