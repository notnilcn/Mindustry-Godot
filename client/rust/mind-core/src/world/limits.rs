// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Placement limits, bans and rule knobs (`Rules` boundary fields).
//!
//! Ported from the block-relevant subset of `core/src/mindustry/game/Rules.java`
//! and `BlockCounter`/`TeamData` block counting. Plan 12 owns the full `Rules`;
//! this is plan 07's boundary stub (plan 07 §8 R6/R7) with the fields the
//! placement/construct paths read.

use bevy_ecs::prelude::Resource;
use indexmap::IndexMap;

use crate::content::BlockId;

/// Block-relevant rule fields (`Rules` boundary stub).
#[derive(Debug, Clone, Resource)]
pub struct BuildRules {
    /// `Rules.cheat` — infinite resources, instant build, no consumer limits.
    pub cheat: bool,
    /// `Rules.infiniteResources`.
    pub infinite_resources: bool,
    /// `Rules.instantBuild` (editor/debug).
    pub instant_build: bool,
    /// `Rules.deconstructRefundMultiplier`.
    pub deconstruct_refund_multiplier: f32,
    /// `Rules.allowEnvironmentDeconstruct`.
    pub allow_environment_deconstruct: bool,
    /// `Rules.placeRangeCheck`.
    pub place_range_check: bool,
    /// `Rules.allowDerelictRepair`.
    pub allow_derelict_repair: bool,
    /// `Rules.buildCostMultiplier`.
    pub build_cost_multiplier: f32,
    /// `Rules.enemyCoreBuildRadius` (world pixels).
    pub enemy_core_build_radius: f32,
    /// `Rules.polygonCoreProtection`.
    pub polygon_core_protection: bool,
    /// Team id treated as the local/player team (`sharded`).
    pub default_team: u8,
    /// Banned blocks (`Rules.bannedBlocks`).
    pub banned_blocks: Vec<BlockId>,
    /// Per-team block limits (`Rules.blockLimits`).
    pub block_limits: IndexMap<(u8, BlockId), u32>,
}

impl Default for BuildRules {
    fn default() -> Self {
        Self {
            cheat: false,
            infinite_resources: false,
            instant_build: false,
            deconstruct_refund_multiplier: 0.5,
            allow_environment_deconstruct: false,
            place_range_check: false,
            allow_derelict_repair: true,
            build_cost_multiplier: 1.0,
            enemy_core_build_radius: 400.0,
            polygon_core_protection: false,
            default_team: 0,
            banned_blocks: Vec::new(),
            block_limits: IndexMap::new(),
        }
    }
}

impl BuildRules {
    /// Whether `block` is banned for the current team.
    pub fn is_banned(&self, block: BlockId) -> bool {
        self.banned_blocks.contains(&block)
    }

    /// Placement limit for `(team, block)`, if any.
    pub fn limit(&self, team: u8, block: BlockId) -> Option<u32> {
        self.block_limits.get(&(team, block)).copied()
    }
}

/// Live block counter (`BlockCounter`/`TeamData` fallback).
///
/// Maintained on place/construct-finish/remove; plan 12 overrides with the
/// `TeamData` provider (plan 07 §3.6).
#[derive(Debug, Clone, Default, Resource)]
pub struct BlockCounter {
    /// Count per `(team, block)`.
    pub counts: IndexMap<(u8, BlockId), u32>,
}

impl BlockCounter {
    /// Creates an empty counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current count for `(team, block)`.
    pub fn count(&self, team: u8, block: BlockId) -> u32 {
        self.counts.get(&(team, block)).copied().unwrap_or(0)
    }

    /// Adds `amount` (saturating).
    pub fn add(&mut self, team: u8, block: BlockId, amount: u32) {
        let entry = self.counts.entry((team, block)).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// Removes up to `amount`, dropping zero entries.
    pub fn remove(&mut self, team: u8, block: BlockId, amount: u32) {
        if let Some(entry) = self.counts.get_mut(&(team, block)) {
            *entry = entry.saturating_sub(amount);
            if *entry == 0 {
                self.counts.shift_remove(&(team, block));
            }
        }
    }

    /// Whether placing `(team, block)` would exceed `rules.blockLimits`.
    pub fn is_over_limit(&self, rules: &BuildRules, team: u8, block: BlockId) -> bool {
        match rules.limit(team, block) {
            Some(limit) => self.count(team, block) >= limit,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_tracks_and_prunes() {
        let mut counter = BlockCounter::new();
        let block = BlockId::STONE_WALL;
        counter.add(0, block, 2);
        assert_eq!(counter.count(0, block), 2);
        counter.remove(0, block, 1);
        assert_eq!(counter.count(0, block), 1);
        counter.remove(0, block, 5);
        assert_eq!(counter.count(0, block), 0);
        assert!(counter.counts.is_empty());
    }

    #[test]
    fn limits_are_per_team() {
        let mut rules = BuildRules::default();
        rules.block_limits.insert((0, BlockId::STONE_WALL), 1);
        let mut counter = BlockCounter::new();
        counter.add(0, BlockId::STONE_WALL, 1);
        assert!(counter.is_over_limit(&rules, 0, BlockId::STONE_WALL));
        assert!(!counter.is_over_limit(&rules, 1, BlockId::STONE_WALL));
    }
}
