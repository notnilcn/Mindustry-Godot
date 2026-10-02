// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic fixture helpers shared by unit tests and `mind-headless`
//! scenarios (plan 08 §3.1 `fixtures/logistics.rs`).

pub mod logistics;

pub use logistics::{LOGISTICS_FIXTURE_SEED, SourceBehavior, logistics_checksum, source_behavior};
