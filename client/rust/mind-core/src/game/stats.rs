// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign statistics (plan 12 M3).
//!
//! Ported from `core/src/mindustry/game/CampaignStats.java` and the runtime
//! helpers of `core/src/mindustry/game/GameStats.java`. Plan 04 already owns
//! the persisted [`GameStats`] JSON shape (`io::json::rules::GameStats`); this
//! module adds the mutation helpers used by block/unit/objective consumers and
//! defines [`CampaignStats`], the per-planet lifetime aggregate.
//!
//! Content keys are stored by **name** (the parity/mod ABI) rather than by
//! Arc's `ObjectIntMap<Block>`; the on-disk map body is a JSON object keyed by
//! name. Java's field names are preserved as camelCase.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub use crate::io::json::rules::GameStats;

/// Lifetime statistics for a planet's campaign (`CampaignStats`).
///
/// Java uses `ObjectIntMap<UnitType>/<Block>`; this Rust shape keys by content
/// name (HLP §9, content IDs are the ABI). `playtime` is milliseconds.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CampaignStats {
    /// Enemy units destroyed by type name.
    pub enemy_units_destroyed: IndexMap<String, i32>,
    /// Enemy blocks destroyed by name (from any source).
    pub enemy_buildings_destroyed: IndexMap<String, i32>,
    /// Player team units produced by type name.
    pub units_produced: IndexMap<String, i32>,
    /// Player team units destroyed by type name.
    pub units_destroyed: IndexMap<String, i32>,
    /// Blocks placed by name.
    pub buildings_built: IndexMap<String, i32>,
    /// Blocks deconstructed by name.
    pub buildings_deconstructed: IndexMap<String, i32>,
    /// Blocks destroyed by name.
    pub buildings_destroyed: IndexMap<String, i32>,
    /// Total campaign playtime in milliseconds.
    pub playtime: i64,
    /// Total game-overs.
    pub sectors_lost: i32,
    /// Total sector captures (recaptures count again).
    pub sectors_captured: i32,
    /// Total waves lasted.
    pub waves_lasted: i32,
}

impl CampaignStats {
    /// Empty aggregate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments a named counter in `map` by `amount` (creating it at 0).
    pub fn add(map: &mut IndexMap<String, i32>, name: &str, amount: i32) {
        *map.entry(name.to_owned()).or_insert(0) += amount;
    }

    /// `CampaignStats.enemyUnitsDestroyed`.
    pub fn add_enemy_unit_destroyed(&mut self, name: &str, amount: i32) {
        Self::add(&mut self.enemy_units_destroyed, name, amount);
    }

    /// `CampaignStats.enemyBuildingsDestroyed`.
    pub fn add_enemy_building_destroyed(&mut self, name: &str, amount: i32) {
        Self::add(&mut self.enemy_buildings_destroyed, name, amount);
    }

    /// `CampaignStats.unitsProduced`.
    pub fn add_unit_produced(&mut self, name: &str, amount: i32) {
        Self::add(&mut self.units_produced, name, amount);
    }

    /// `CampaignStats.unitsDestroyed`.
    pub fn add_unit_destroyed(&mut self, name: &str, amount: i32) {
        Self::add(&mut self.units_destroyed, name, amount);
    }

    /// `CampaignStats.buildingsBuilt`.
    pub fn add_building_built(&mut self, name: &str, amount: i32) {
        Self::add(&mut self.buildings_built, name, amount);
    }

    /// `CampaignStats.buildingsDeconstructed`.
    pub fn add_building_deconstructed(&mut self, name: &str, amount: i32) {
        Self::add(&mut self.buildings_deconstructed, name, amount);
    }

    /// `CampaignStats.buildingsDestroyed`.
    pub fn add_building_destroyed(&mut self, name: &str, amount: i32) {
        Self::add(&mut self.buildings_destroyed, name, amount);
    }

    /// Human-facing total counts (audit/scenario convenience).
    pub fn total(&self, map: &IndexMap<String, i32>) -> i32 {
        map.values().copied().sum()
    }
}

impl GameStats {
    /// `GameStats.getPlaced`-parity increment (`placedBlockCount`).
    pub fn add_placed(&mut self, block: &str, amount: i32) {
        *self.placed_block_count.entry(block.to_owned()).or_insert(0) += amount;
    }

    /// Increments `destroyedBlockCount`.
    pub fn add_destroyed(&mut self, block: &str, amount: i32) {
        *self
            .destroyed_block_count
            .entry(block.to_owned())
            .or_insert(0) += amount;
    }

    /// Increments `coreItemCount` (`handleCoreItem`).
    pub fn add_core_item(&mut self, item: &str, amount: i32) {
        *self.core_item_count.entry(item.to_owned()).or_insert(0) += amount;
    }

    /// Zeros every counter and clears the maps (`Logic.reset`).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_stats_accumulate_and_roundtrip() {
        let mut stats = CampaignStats::new();
        stats.add_enemy_unit_destroyed("dagger", 3);
        stats.add_enemy_unit_destroyed("dagger", 2);
        stats.add_building_built("router", 4);
        stats.playtime = 123_456;
        stats.sectors_captured = 2;

        assert_eq!(stats.enemy_units_destroyed.get("dagger"), Some(&5));
        assert_eq!(stats.total(&stats.buildings_built), 4);

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"enemyUnitsDestroyed\":{\"dagger\":5}"));
        assert!(json.contains("\"buildingsBuilt\":{\"router\":4}"));
        let back: CampaignStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stats);
        let empty: CampaignStats = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, CampaignStats::new());
    }

    #[test]
    fn game_stats_helpers_match_io_shape() {
        let mut stats = GameStats::default();
        stats.add_placed("conveyor", 2);
        stats.add_placed("conveyor", 3);
        stats.add_destroyed("wall", 1);
        stats.add_core_item("copper", 99);
        assert_eq!(stats.get_placed("conveyor"), 5);
        assert_eq!(stats.get_destroyed("wall"), 1);
        assert_eq!(stats.core_item_count.get("copper"), Some(&99));
        stats.reset();
        assert_eq!(stats.get_placed("conveyor"), 0);
        assert!(stats.core_item_count.is_empty());
    }
}
