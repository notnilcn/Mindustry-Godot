// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla planet generators (plan 06 §3.11).
//!
//! Ported from `maps/planet/{SerpuloPlanetGenerator,ErekirPlanetGenerator,
//! TantrosPlanetGenerator,AsteroidGenerator}.java`. Structural parity only
//! (OD6-B): goldens are Rust-recorded. Serpulo/Erekir/Asteroid depend on plan
//! 11/12 data (bases, schematics, rules, attributes) and land incrementally.

pub mod tantros;

pub use tantros::TantrosPlanetGenerator;
