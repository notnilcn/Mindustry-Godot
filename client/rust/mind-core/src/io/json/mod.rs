// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `JsonIO` equivalent (plan 04 §3.6): serde-based JSON for rules, stats,
//! settings and map tags.
//!
//! Ported from `core/src/mindustry/io/JsonIO.java`. M2 ships only the
//! `TypeIO.writeRules` surface (minimal [`Rules`]); M6 completes the class-tag
//! registry, content serializers with upstream fallbacks, and the full
//! `Rules`/`GameStats`/`MapLocales`/`SectorInfo` field sets.

pub mod rules;

pub use rules::Rules;
