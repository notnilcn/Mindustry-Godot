// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Reusable simulation test fixtures (plan 09 §7a).
//!
//! [`power`] ports `tests/src/test/java/power/PowerTestFixture.java`: fake
//! producer/battery/direct-consumer blocks and a graph harness. Plans 10/23 and
//! `mind-headless` scenarios reuse it.

pub mod power;
