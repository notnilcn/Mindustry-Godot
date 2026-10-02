// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map-generation parameters (`world/WorldParams.java`, plan 06 §3.10).
//!
//! Rust adaptation: `SaveInfo` (a plan-12 campaign type) is a plain optional
//! value behind a bool until plan 12 lands (`save_info` here just records
//! whether a save context was supplied).

use serde::{Deserialize, Serialize};

/// Parameters for one generation/load (`WorldParams`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldParams {
    /// Seed offset added to the planet base seed (`seedOffset`).
    pub seed_offset: u64,
    /// Whether a save-info/campaign context was supplied (`SaveInfo`).
    pub save_info: bool,
    /// Forced core position (`corePositionOverride`; `-1` = none).
    pub core_position_override: i32,
    /// Requested map width in tiles (`0` = generator default).
    pub width: i32,
    /// Requested map height in tiles.
    pub height: i32,
}

impl Default for WorldParams {
    fn default() -> Self {
        Self {
            seed_offset: 0,
            save_info: false,
            core_position_override: -1,
            width: 0,
            height: 0,
        }
    }
}

impl WorldParams {
    /// Default parameters for a planet sector (`PlanetGenerator` defaults).
    pub fn new(seed_offset: u64) -> Self {
        Self {
            seed_offset,
            ..Self::default()
        }
    }
}
