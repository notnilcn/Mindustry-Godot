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

/// `Rules.TeamRule` placement subset (`Rules.java` `TeamRule`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeamPlaceRule {
    /// Team id.
    pub team: u8,
    /// `TeamRule.protectCores` (core no-build radius applies).
    pub protect_cores: bool,
    /// `TeamRule.checkPlacement` (`getEnemyOverlap` scans this team).
    pub check_placement: bool,
    /// `TeamRule.extraCoreBuildRadius`.
    pub extra_core_build_radius: f32,
}

impl Default for TeamPlaceRule {
    fn default() -> Self {
        Self {
            team: 0,
            protect_cores: true,
            check_placement: true,
            extra_core_build_radius: 0.0,
        }
    }
}

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
    /// `Rules.env` (Java `Env` bits; `Vars.defaultEnv` = 233).
    pub env: i32,
    /// `Rules.blockWhitelist` (`isBanned` inverts the list).
    pub block_whitelist: bool,
    /// `Rules.editor`.
    pub editor: bool,
    /// `Rules.hideBannedBlocks`.
    pub hide_banned_blocks: bool,
    /// `Rules.revealedBlocks` (`Block.isHidden` escape).
    pub revealed_blocks: Vec<BlockId>,
    /// `Rules.coreBuildAndConfig` (`CoreBlock.canBreak`/`canPlaceOn`).
    pub core_build_and_config: bool,
    /// `Rules.waves`.
    pub waves: bool,
    /// `Rules.waveTeam`.
    pub wave_team: u8,
    /// `Rules.fog`.
    pub fog: bool,
    /// `Rules.staticFog`.
    pub static_fog: bool,
    /// `Rules.lighting`.
    pub lighting: bool,
    /// `Rules.allowEditWorldProcessors`.
    pub allow_edit_world_processors: bool,
    /// `state.isCampaign()` (not on `Rules` itself).
    pub is_campaign: bool,
    /// Per-team placement rules (`Rules.teams`), ordered by team id.
    pub team_rules: Vec<TeamPlaceRule>,
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
            env: crate::io::json::rules::DEFAULT_ENV,
            block_whitelist: false,
            editor: false,
            hide_banned_blocks: false,
            revealed_blocks: Vec::new(),
            core_build_and_config: false,
            waves: true,
            wave_team: 2,
            fog: false,
            static_fog: true,
            lighting: false,
            allow_edit_world_processors: false,
            is_campaign: false,
            team_rules: Vec::new(),
        }
    }
}

impl BuildRules {
    /// `Rules.isBanned(block)` (`blockWhitelist != bannedBlocks.contains(block)`,
    /// `Rules.java:225-227`).
    pub fn is_banned(&self, block: BlockId) -> bool {
        self.block_whitelist != self.banned_blocks.contains(&block)
    }

    /// `rules.teams.get(team)` with upstream defaults for absent entries.
    pub fn team_rule(&self, team: u8) -> TeamPlaceRule {
        self.team_rules
            .iter()
            .copied()
            .find(|rule| rule.team == team)
            .unwrap_or(TeamPlaceRule {
                team,
                ..TeamPlaceRule::default()
            })
    }

    /// `Rules.buildRadius(team)` (`Rules.java:294-296`).
    pub fn build_radius(&self, team: u8) -> f32 {
        let rule = self.team_rule(team);
        if !rule.protect_cores {
            0.0
        } else {
            self.enemy_core_build_radius + rule.extra_core_build_radius
        }
    }

    /// Placement limit for `(team, block)`, if any.
    pub fn limit(&self, team: u8, block: BlockId) -> Option<u32> {
        self.block_limits.get(&(team, block)).copied()
    }
}

/// Copies the placement/break rule subset of `Rules` into the [`BuildRules`]
/// resource (`World.setRules` + every `Build.validPlace` read of
/// `state.rules`). No-op when `BuildRules` is absent. Block names resolve
/// through [`crate::sim::runtime::ContentIndex`].
///
/// `is_campaign` is explicit because `Rules` alone does not know it; callers
/// pass `session.is_campaign()`.
pub fn sync_build_rules(
    world: &mut bevy_ecs::world::World,
    rules: &crate::game::rules::Rules,
    is_campaign: bool,
) {
    // Resolve names before taking the resource borrow.
    let (banned, revealed, limits) = {
        let index = world.get_resource::<crate::sim::runtime::ContentIndex>();
        let map_id = |name: &str| -> Option<BlockId> {
            index
                .and_then(|index| index.block_ids.get(name).copied())
                .map(BlockId::new)
        };
        // `IndexSet` iteration order (bans are compared as a set only).
        let banned: Vec<BlockId> = rules
            .banned_blocks
            .iter()
            .filter_map(|n| map_id(n))
            .collect();
        let revealed: Vec<BlockId> = rules
            .revealed_blocks
            .iter()
            .filter_map(|n| map_id(n))
            .collect();
        // `Rules.blockLimits` is per-block upstream (`Rules.java:187`); the
        // port table is (team, block)-keyed, so the limit is filed under the
        // default team. Only positive limits apply (`Block.java:1044-1048`).
        let mut limits: IndexMap<(u8, BlockId), u32> = IndexMap::new();
        for (name, limit) in &rules.block_limits {
            if *limit > 0
                && let Some(id) = map_id(name)
            {
                limits.insert((rules.default_team, id), *limit as u32);
            }
        }
        (banned, revealed, limits)
    };
    let team_rules: Vec<TeamPlaceRule> = rules
        .teams
        .0
        .iter()
        .map(|(&team, team_rule)| TeamPlaceRule {
            team,
            protect_cores: team_rule.protect_cores,
            check_placement: team_rule.check_placement,
            extra_core_build_radius: team_rule.extra_core_build_radius,
        })
        .collect();
    let Some(mut build) = world.get_resource_mut::<BuildRules>() else {
        return;
    };
    build.block_whitelist = rules.block_whitelist;
    build.banned_blocks = banned;
    build.revealed_blocks = revealed;
    build.block_limits = limits;
    build.editor = rules.editor;
    build.hide_banned_blocks = rules.hide_banned_blocks;
    build.core_build_and_config = rules.core_build_and_config;
    build.waves = rules.waves;
    build.wave_team = rules.wave_team;
    build.fog = rules.fog;
    build.static_fog = rules.static_fog;
    build.lighting = rules.lighting;
    build.allow_edit_world_processors = rules.allow_edit_world_processors;
    build.is_campaign = is_campaign;
    build.team_rules = team_rules;
    build.infinite_resources = rules.infinite_resources;
    build.instant_build = rules.instant_build;
    build.deconstruct_refund_multiplier = rules.deconstruct_refund_multiplier;
    build.build_cost_multiplier = rules.build_cost_multiplier;
    build.enemy_core_build_radius = rules.enemy_core_build_radius;
    build.polygon_core_protection = rules.polygon_core_protection;
    build.place_range_check = rules.place_range_check;
    build.allow_derelict_repair = rules.derelict_repair;
    build.allow_environment_deconstruct = rules.allow_environment_deconstruct;
    build.env = rules.env;
    build.default_team = rules.default_team;
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
