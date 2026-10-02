// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BlockIndexer` (plan 11 §3.7/§4.2).
//!
//! Ported from `core/src/mindustry/ai/BlockIndexer.java`. Maintains per-team
//! building buckets keyed by [`BlockFlag`], a damaged list and a flat building
//! list. The block-flag data is plan 02's ([`BlockDef::flags`]). The turret
//! quadtree and ore/floor-flag lists are stubbed with explicit TODO seams; all
//! scans are deterministic (buildings sorted by entity index).
//!
//! [`BlockDef::flags`]: crate::content::registries::blocks::BlockDef

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::content::registries::blocks::BlockFlag;
use crate::entities::comp::building::Building;
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::world::{TilePos, WorldGrid};

/// Number of [`BlockFlag`] variants (declaration order).
pub const FLAG_COUNT: usize = 18;

/// Teams indexed `0..=255`.
const TEAM_STRIDE: usize = 256;

/// `BlockIndexer.BuildingPriority`: ordered search classes for target finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockPriority {
    /// Enemy core (primary target).
    Core,
    /// Stored resources.
    Storage,
    /// Power generation.
    Generator,
    /// Resource transformation.
    Factory,
    /// Repair points.
    Repair,
    /// Power resupply.
    Battery,
    /// Reactors.
    Reactor,
    /// Drills.
    Drill,
    /// Force projectors.
    Shield,
    /// Turrets.
    Turret,
    /// Launch pads.
    LaunchPad,
    /// Unit assemblers.
    UnitAssembler,
}

impl BlockPriority {
    /// Flags matched by this priority, in search order.
    pub const fn flags(self) -> &'static [BlockFlag] {
        match self {
            BlockPriority::Core => &[BlockFlag::Core],
            BlockPriority::Storage => &[BlockFlag::Storage],
            BlockPriority::Generator => &[BlockFlag::Generator],
            BlockPriority::Factory => &[BlockFlag::Factory],
            BlockPriority::Repair => &[BlockFlag::Repair],
            BlockPriority::Battery => &[BlockFlag::Battery],
            BlockPriority::Reactor => &[BlockFlag::Reactor],
            BlockPriority::Drill => &[BlockFlag::Drill],
            BlockPriority::Shield => &[BlockFlag::Shield],
            BlockPriority::Turret => &[BlockFlag::Turret],
            BlockPriority::LaunchPad => &[BlockFlag::LaunchPad],
            BlockPriority::UnitAssembler => &[BlockFlag::UnitAssembler],
        }
    }
}

/// Deterministic index of buildings (plan 11 §3.7).
#[derive(Debug, Default, Clone)]
pub struct BlockIndexer {
    /// `(team * TEAM_STRIDE + flag_index) -> entities`.
    flags: Vec<Vec<Entity>>,
    /// Buildings with `health.damaged()`.
    damaged: Vec<Entity>,
    /// All indexed buildings.
    all: Vec<Entity>,
}

impl BlockIndexer {
    /// Creates an empty index.
    pub fn new() -> Self {
        Self {
            flags: vec![Vec::new(); TEAM_STRIDE * FLAG_COUNT],
            damaged: Vec::new(),
            all: Vec::new(),
        }
    }

    /// Rebuilds the index from scratch (`WorldLoadEvent`/`TileChangeEvent`).
    pub fn rebuild(&mut self, world: &mut World, content: &ContentRegistry) {
        self.clear();
        let mut rows: Vec<(Entity, u8, Vec<BlockFlag>, bool)> = {
            let mut query = world.query::<(Entity, &Building, &TeamComp, &Health)>();
            query
                .iter(world)
                .map(|(entity, building, team, health)| {
                    let flags = content
                        .block(building.block)
                        .map(|def| def.flags.clone())
                        .unwrap_or_default();
                    (entity, team.team, flags, health.damaged())
                })
                .collect()
        };
        rows.sort_by_key(|(entity, _, _, _)| entity.index());
        for (entity, team, flags, damaged) in rows {
            self.all.push(entity);
            for flag in flags {
                self.flags[team as usize * TEAM_STRIDE + flag_index(flag)].push(entity);
            }
            if damaged {
                self.damaged.push(entity);
            }
        }
    }

    /// Clears the index.
    pub fn clear(&mut self) {
        for bucket in &mut self.flags {
            bucket.clear();
        }
        self.damaged.clear();
        self.all.clear();
    }

    /// All indexed buildings (stable entity order).
    pub fn all(&self) -> &[Entity] {
        &self.all
    }

    /// Damaged buildings (stable order).
    pub fn damaged(&self) -> &[Entity] {
        &self.damaged
    }

    /// Whether `team` has any building carrying `flag`.
    pub fn is_block_present(&self, team: u8, flag: BlockFlag) -> bool {
        !self.flags[team as usize * TEAM_STRIDE + flag_index(flag)].is_empty()
    }

    /// Iterates the buildings of `team` carrying `flag`.
    pub fn each_block(&self, team: u8, flag: BlockFlag, mut visit: impl FnMut(Entity)) {
        for &entity in &self.flags[team as usize * TEAM_STRIDE + flag_index(flag)] {
            visit(entity);
        }
    }

    /// Marks/unmarks a building as damaged (`notifyHealthChanged`).
    pub fn notify_health_changed(&mut self, world: &World, entity: Entity) {
        let damaged = world
            .get::<Health>(entity)
            .map(|health| health.damaged())
            .unwrap_or(false);
        if damaged {
            if let Err(index) = self.damaged.binary_search(&entity) {
                self.damaged.insert(index, entity);
            }
        } else {
            self.damaged.retain(|&existing| existing != entity);
        }
    }

    /// Nearest hostile building matching one of `priorities` within `range`.
    ///
    /// Priorities are searched in order; ties break by entity index.
    #[allow(clippy::too_many_arguments)] // mirrors `BlockIndexer.findEnemyTile`
    pub fn find_enemy_tile(
        &self,
        world: &World,
        content: &ContentRegistry,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
        priorities: &[BlockPriority],
    ) -> Option<TilePos> {
        let range2 = range * range;
        let mut best: Option<(f32, Entity)> = None;
        for &entity in &self.all {
            let Some(other_team) = world.get::<TeamComp>(entity).map(|t| t.team) else {
                continue;
            };
            if other_team == team {
                continue;
            }
            let Some(building) = world.get::<Building>(entity) else {
                continue;
            };
            let matches = content.block(building.block).is_some_and(|def| {
                priorities
                    .iter()
                    .any(|priority| priority.flags().iter().any(|flag| def.flags.contains(flag)))
            });
            if !matches {
                continue;
            }
            let Some(pos) = world.get::<Pos>(entity) else {
                continue;
            };
            let dx = pos.x - x;
            let dy = pos.y - y;
            let dist2 = dx * dx + dy * dy;
            if dist2 > range2 {
                continue;
            }
            match best {
                Some((best_dist, best_entity))
                    if best_dist < dist2
                        || (best_dist == dist2 && best_entity.index() <= entity.index()) => {}
                _ => best = Some((dist2, entity)),
            }
        }
        best.and_then(|(_, entity)| world.get::<Building>(entity).map(|building| building.tile))
    }

    /// Closest ore tile to `(x, y)` (delegates to the miner scan).
    pub fn find_closest_ore(
        &self,
        grid: &WorldGrid,
        content: &ContentRegistry,
        x: f32,
        y: f32,
        range: f32,
    ) -> Option<TilePos> {
        super::types::miner::find_ore_tile(grid, content, x, y, range)
    }
}

/// Index of a [`BlockFlag`] in declaration order.
pub const fn flag_index(flag: BlockFlag) -> usize {
    match flag {
        BlockFlag::Core => 0,
        BlockFlag::Storage => 1,
        BlockFlag::Generator => 2,
        BlockFlag::Turret => 3,
        BlockFlag::Factory => 4,
        BlockFlag::Repair => 5,
        BlockFlag::Battery => 6,
        BlockFlag::Reactor => 7,
        BlockFlag::Extinguisher => 8,
        BlockFlag::Drill => 9,
        BlockFlag::Shield => 10,
        BlockFlag::LaunchPad => 11,
        BlockFlag::UnitCargoUnloadPoint => 12,
        BlockFlag::UnitAssembler => 13,
        BlockFlag::HasFogRadius => 14,
        BlockFlag::SteamVent => 15,
        BlockFlag::BlockRepair => 16,
        BlockFlag::Synced => 17,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;

    #[test]
    fn indexes_core_and_factory_flags() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        let factory = harness
            .content()
            .block_id("graphite-press")
            .expect("graphite-press");
        assert!(harness.build.place(4, 4, core, 0, true));
        assert!(harness.build.place(12, 12, factory, 0, true));
        let mut indexer = BlockIndexer::new();
        indexer.rebuild(&mut harness.build.world, &harness.build.content);
        assert!(indexer.is_block_present(0, BlockFlag::Core));
        assert!(indexer.is_block_present(0, BlockFlag::Factory));
        assert!(!indexer.is_block_present(0, BlockFlag::Turret));
        let mut count = 0;
        indexer.each_block(0, BlockFlag::Core, |_| count += 1);
        assert_eq!(count, 1);
    }

    #[test]
    fn finds_enemy_core_tile() {
        let mut harness = UnitHarness::new(64, 64, 1);
        // Team 1 core at (2,2); team 0 unit at (40,40).
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        harness.build.rules.default_team = 1;
        assert!(harness.build.place(2, 2, core, 0, true));
        let mut indexer = BlockIndexer::new();
        indexer.rebuild(&mut harness.build.world, &harness.build.content);
        let tile = indexer
            .find_enemy_tile(
                &harness.build.world,
                &harness.build.content,
                0,
                320.0,
                320.0,
                2000.0,
                &[BlockPriority::Core],
            )
            .expect("enemy core");
        assert_eq!(tile, TilePos::new(2, 2));
    }
}
