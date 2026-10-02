// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-kind bullet behaviors (`entities/bullet/*.java`).
//!
//! Each module mirrors one Java `BulletType` subclass. Kinds not yet given a
//! dedicated implementation fall back to the base behavior (plan 10 §3.4);
//! subsequent milestones add the remaining kind modules here.

pub mod basic;
