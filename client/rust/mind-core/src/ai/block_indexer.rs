// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BlockIndexer` (plan 11 §3.7/§4.2).
//!
//! Ported from `core/src/mindustry/ai/BlockIndexer.java`. Maintains per-team
//! building buckets keyed by [`BlockFlag`], a damaged list, a flat building
//! list, the flagged-floor map, the map's present-ore list and the
//! `blocksPresent` set. The block-flag data is plan 02's
//! ([`BlockDef::flags`]). Java's turret/building quadtrees are replaced by the
//! flat `all` list (entity order); all scans stay deterministic (buildings
//! sorted by entity index).
//!
//! Incremental maintenance mirrors Java's `TilePreChangeEvent`/
//! `TileChangeEvent`/`TileFloorChangeEvent` hooks: [`BlockIndexer::add_index`]/
//! [`BlockIndexer::remove_index`] for buildings and
//! [`BlockIndexer::add_floor_index`]/[`BlockIndexer::remove_floor_index`] for
//! floor flags + ore counts. [`BlockIndexer::rebuild`] remains the
//! load/wholesale path (buildings only: the live sim's grid is `Sim::grid`, not
//! a `World` resource); [`BlockIndexer::rebuild_static`] rebuilds the
//! tile-derived half from a [`WorldGrid`].
//!
//! [`BlockDef::flags`]: crate::content::registries::blocks::BlockDef

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::registries::blocks::BlockFlag;
use crate::content::{BlockId, ContentRegistry, ItemId};
use crate::entities::comp::building::Building;
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::world::{TilePos, WorldGrid};

/// Number of [`BlockFlag`] variants (declaration order).
pub const FLAG_COUNT: usize = 18;

/// `Team.all.length` — teams indexed `0..=255`.
const TEAM_SLOTS: usize = 256;
/// Flags per team block (the bucket vector is `TEAM_SLOTS * FLAG_COUNT`).
const FLAGS_PER_TEAM: usize = FLAG_COUNT;

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
    /// `(team * FLAGS_PER_TEAM + flag_index) -> entities`.
    flags: Vec<Vec<Entity>>,
    /// Buildings with `health.damaged()`.
    damaged: Vec<Entity>,
    /// All indexed buildings.
    all: Vec<Entity>,
    /// `BlockId -> Block.underBullets`, captured at rebuild
    /// (`AIController.target`'s building predicate, AIController.java:282).
    under_bullets: Vec<bool>,
    /// `BlockFlag -> floor tiles` (`BlockIndexer.floorMap`).
    floor_map: Vec<Vec<TilePos>>,
    /// `allOres` (floor/overlay ore counts by item id).
    ore_counts: Vec<i32>,
    /// `allWallOres` (wall ore counts by item id).
    wall_ore_counts: Vec<i32>,
    /// `allPresentOres` (content order).
    present_ores: Vec<ItemId>,
    /// `blocksPresent` (block content id -> present anywhere on the map).
    blocks_present: Vec<bool>,
}

impl BlockIndexer {
    /// Creates an empty index.
    pub fn new() -> Self {
        Self {
            flags: vec![Vec::new(); TEAM_SLOTS * FLAGS_PER_TEAM],
            damaged: Vec::new(),
            all: Vec::new(),
            under_bullets: Vec::new(),
            floor_map: vec![Vec::new(); FLAG_COUNT],
            ore_counts: Vec::new(),
            wall_ore_counts: Vec::new(),
            present_ores: Vec::new(),
            blocks_present: Vec::new(),
        }
    }

    /// Rebuilds the index from scratch (`WorldLoadEvent`/`TileChangeEvent`).
    pub fn rebuild(&mut self, world: &mut World, content: &ContentRegistry) {
        self.clear();
        self.under_bullets = content
            .blocks()
            .iter()
            .map(|block| block.under_bullets)
            .collect();
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
                self.flags[team as usize * FLAGS_PER_TEAM + flag_index(flag)].push(entity);
            }
            if damaged {
                self.damaged.push(entity);
            }
        }
    }

    /// Clears the building index.
    pub fn clear(&mut self) {
        for bucket in &mut self.flags {
            bucket.clear();
        }
        self.damaged.clear();
        self.all.clear();
    }

    /// Clears the tile-derived half (floors/ores/`blocksPresent`) and rebuilds
    /// it from `grid` (`WorldLoadEvent`'s per-tile `process`+`addFloorIndex`
    /// loop). Does not touch the building halves.
    pub fn rebuild_static(&mut self, grid: &WorldGrid, content: &ContentRegistry) {
        for bucket in &mut self.floor_map {
            bucket.clear();
        }
        self.ore_counts = vec![0; content.items().len()];
        self.wall_ore_counts = vec![0; content.items().len()];
        self.present_ores.clear();
        self.blocks_present = vec![false; content.blocks().len()];
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                self.index_tile_static(grid, content, x, y);
            }
        }
        self.update_present_ores(content);
    }

    /// Adds `entity` to the index (`BlockIndexer.addIndex` building half).
    ///
    /// Keeps the `all`/flag/damaged vectors sorted by entity index so
    /// incremental maintenance matches a full [`Self::rebuild`].
    pub fn add_index(&mut self, world: &World, content: &ContentRegistry, entity: Entity) -> bool {
        let Some(building) = world.get::<Building>(entity) else {
            return false;
        };
        let Some(team) = world.get::<TeamComp>(entity).map(|team| team.team) else {
            return false;
        };
        if self
            .all
            .binary_search_by_key(&entity.index(), |existing| existing.index())
            .is_ok()
        {
            return false;
        }
        let flags = content
            .block(building.block)
            .map(|def| def.flags.clone())
            .unwrap_or_default();
        self.refresh_under_bullets(content);
        insert_sorted(&mut self.all, entity);
        for flag in flags {
            insert_sorted(
                &mut self.flags[team as usize * FLAGS_PER_TEAM + flag_index(flag)],
                entity,
            );
        }
        self.notify_health_changed(world, entity);
        true
    }

    /// Removes `entity` from the index (`BlockIndexer.removeIndex`).
    pub fn remove_index(&mut self, _world: &World, entity: Entity) -> bool {
        let present = self
            .all
            .binary_search_by_key(&entity.index(), |existing| existing.index())
            .map(|index| self.all.remove(index))
            .is_ok();
        if !present {
            return false;
        }
        self.damaged.retain(|&existing| existing != entity);
        // Java removes the entity from the flag buckets of the tile's own team.
        // The port's callers only hold the entity, so scan every bucket (the
        // entity is present in at most one team's bucket per flag).
        for bucket in &mut self.flags {
            bucket.retain(|&existing| existing != entity);
        }
        true
    }

    /// Adds a floor tile to the flag map and its ore counts
    /// (`BlockIndexer.addFloorIndex` + the `addIndex` tile half).
    ///
    /// Call *after* the grid stores the new floor/block.
    pub fn add_floor_index(&mut self, grid: &WorldGrid, content: &ContentRegistry, x: i32, y: i32) {
        if !grid.tiles.in_bounds(x, y) {
            return;
        }
        self.index_tile_static(grid, content, x, y);
        self.update_present_ores(content);
    }

    /// Removes a floor tile from the flag map and its ore counts
    /// (`BlockIndexer.removeFloorIndex` + the `removeIndex` tile half).
    ///
    /// Call *before* the grid changes the tile; the ore counts are decremented
    /// from the current tile contents.
    pub fn remove_floor_index(
        &mut self,
        grid: &WorldGrid,
        content: &ContentRegistry,
        x: i32,
        y: i32,
    ) {
        if !grid.tiles.in_bounds(x, y) {
            return;
        }
        let tile = grid.tiles.get(x, y).clone();
        self.remove_floor_flag(content, &tile);
        // `BlockIndexer.addIndex`: a floor ore counts only on an air tile, a
        // wall ore only on a blocked tile (`tile.drop()`/`tile.wallDrop()`).
        if tile.block == BlockId::AIR {
            if let Some(drop) = floor_drop(content, &tile) {
                self.bump_ore(content, drop, -1);
            }
        } else if let Some(drop) = wall_drop(content, &tile) {
            self.bump_wall_ore(content, drop, -1);
        }
        self.update_present_ores(content);
    }

    /// `BlockIndexer.getFlaggedFloors(flag)`: floor tiles carrying `flag`.
    pub fn get_flagged_floors(&self, flag: BlockFlag) -> &[TilePos] {
        &self.floor_map[flag_index(flag)]
    }

    /// `BlockIndexer.getAllPresentOres()`: items present as floor or wall ore,
    /// in content order.
    pub fn all_present_ores(&self) -> &[ItemId] {
        &self.present_ores
    }

    /// `BlockIndexer.hasOre(item)`.
    pub fn has_ore(&self, item: ItemId) -> bool {
        self.ore_counts
            .get(item.raw() as usize)
            .is_some_and(|count| *count > 0)
    }

    /// `BlockIndexer.hasWallOre(item)`.
    pub fn has_wall_ore(&self, item: ItemId) -> bool {
        self.wall_ore_counts
            .get(item.raw() as usize)
            .is_some_and(|count| *count > 0)
    }

    /// `BlockIndexer.isBlockPresent(block)` (needs [`Self::rebuild_static`]).
    pub fn is_block_present(&self, block: BlockId) -> bool {
        self.blocks_present
            .get(block.raw() as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Indexes one tile's floor flags/ores/blocks-present entries.
    fn index_tile_static(&mut self, grid: &WorldGrid, content: &ContentRegistry, x: i32, y: i32) {
        let Some(tile) = grid.tiles.getn(x, y).cloned() else {
            return;
        };
        self.add_floor_flag(grid, content, &tile);
        // A floor ore counts only on an air tile; a wall ore only on a blocked
        // tile (`BlockIndexer.addIndex`, Tile.java:580-589).
        if tile.block == BlockId::AIR {
            if let Some(drop) = floor_drop(content, &tile) {
                self.bump_ore(content, drop, 1);
            }
        } else if let Some(drop) = wall_drop(content, &tile) {
            self.bump_wall_ore(content, drop, 1);
        }
        let block = content.block(tile.block);
        // `Tile.isStatic`: `cacheLayer == CacheLayer.walls` (Block.java:1121).
        if block.is_none_or(|def| def.cache_layer != crate::render::layer::CacheLayerId::Walls) {
            mark_present(&mut self.blocks_present, tile.floor);
            mark_present(&mut self.blocks_present, tile.overlay);
        }
        mark_present(&mut self.blocks_present, tile.block);
    }

    /// `BlockIndexer.addFloorIndex`: floors with flags that pass
    /// `shouldIndex`.
    fn add_floor_flag(
        &mut self,
        grid: &WorldGrid,
        content: &ContentRegistry,
        tile: &crate::world::Tile,
    ) {
        let Some(def) = content.block(tile.floor) else {
            return;
        };
        for flag in &def.flags {
            if flag_index(*flag) >= self.floor_map.len() {
                continue;
            }
            if !floor_should_index(*flag, grid, tile) {
                continue;
            }
            let pos = TilePos::new(tile.x, tile.y);
            let bucket = &mut self.floor_map[flag_index(*flag)];
            if let Err(index) = bucket.binary_search_by_key(&(tile.y, tile.x), |p| (p.y(), p.x())) {
                bucket.insert(index, pos);
            }
        }
    }

    /// `BlockIndexer.removeFloorIndex`.
    fn remove_floor_flag(&mut self, content: &ContentRegistry, tile: &crate::world::Tile) {
        let Some(def) = content.block(tile.floor) else {
            return;
        };
        for flag in &def.flags {
            if flag_index(*flag) >= self.floor_map.len() {
                continue;
            }
            self.floor_map[flag_index(*flag)].retain(|pos| pos.x() != tile.x || pos.y() != tile.y);
        }
    }

    fn bump_ore(&mut self, content: &ContentRegistry, item: ItemId, delta: i32) {
        self.ensure_item_counts(content);
        if let Some(count) = self.ore_counts.get_mut(item.raw() as usize) {
            *count = (*count + delta).max(0);
        }
    }

    fn bump_wall_ore(&mut self, content: &ContentRegistry, item: ItemId, delta: i32) {
        self.ensure_item_counts(content);
        if let Some(count) = self.wall_ore_counts.get_mut(item.raw() as usize) {
            *count = (*count + delta).max(0);
        }
    }

    fn ensure_item_counts(&mut self, content: &ContentRegistry) {
        if self.ore_counts.len() < content.items().len() {
            self.ore_counts.resize(content.items().len(), 0);
            self.wall_ore_counts.resize(content.items().len(), 0);
        }
    }

    /// `BlockIndexer.updatePresentOres`.
    fn update_present_ores(&mut self, content: &ContentRegistry) {
        self.ensure_item_counts(content);
        self.present_ores.clear();
        for item in content.items() {
            if self.has_ore(item.id) || self.has_wall_ore(item.id) {
                self.present_ores.push(item.id);
            }
        }
    }

    fn refresh_under_bullets(&mut self, content: &ContentRegistry) {
        if self.under_bullets.len() != content.blocks().len() {
            self.under_bullets = content
                .blocks()
                .iter()
                .map(|block| block.under_bullets)
                .collect();
        }
    }

    /// All indexed buildings (stable entity order).
    pub fn all(&self) -> &[Entity] {
        &self.all
    }

    /// Damaged buildings (stable order).
    pub fn damaged(&self) -> &[Entity] {
        &self.damaged
    }

    /// Whether `team` has any building carrying `flag` (non-empty
    /// `BlockIndexer.getFlagged(team, flag)`).
    pub fn has_flagged(&self, team: u8, flag: BlockFlag) -> bool {
        !self.flags[team as usize * FLAGS_PER_TEAM + flag_index(flag)].is_empty()
    }

    /// Iterates the buildings of `team` carrying `flag`.
    pub fn each_block(&self, team: u8, flag: BlockFlag, mut visit: impl FnMut(Entity)) {
        for &entity in &self.flags[team as usize * FLAGS_PER_TEAM + flag_index(flag)] {
            visit(entity);
        }
    }

    /// `BlockIndexer.eachBlock(team, wx, wy, range, pred, cons)`: visits the
    /// buildings of `team` (or every team when `None`) whose centre is within
    /// `range + hitSize/2` and that satisfy `pred`, skipping `privileged`
    /// blocks. Returns whether anything was visited.
    ///
    /// Java walks the team quadtree (box intersect, then the `within` test);
    /// the port walks the flat `all` list in entity order and applies the
    /// `within` test directly (same set, deterministic order).
    #[allow(clippy::too_many_arguments)]
    pub fn each_block_range(
        &self,
        world: &World,
        content: &ContentRegistry,
        team: Option<u8>,
        wx: f32,
        wy: f32,
        range: f32,
        mut pred: impl FnMut(Entity) -> bool,
        mut cons: impl FnMut(Entity),
    ) -> bool {
        let mut found = false;
        for &entity in &self.all {
            if let Some(team) = team
                && world.get::<TeamComp>(entity).map(|t| t.team) != Some(team)
            {
                continue;
            }
            if !building_within(world, content, entity, wx, wy, range) {
                continue;
            }
            if !pred(entity) || building_privileged(world, content, entity) {
                continue;
            }
            found = true;
            cons(entity);
        }
        found
    }

    /// `BlockIndexer.allBuildings(x, y, range, cons)`: visits every indexed
    /// building of every team within `range + hitSize/2`.
    pub fn all_buildings(
        &self,
        world: &World,
        content: &ContentRegistry,
        x: f32,
        y: f32,
        range: f32,
        mut cons: impl FnMut(Entity),
    ) {
        for &entity in &self.all {
            if building_within(world, content, entity, x, y, range) {
                cons(entity);
            }
        }
    }

    /// `BlockIndexer.findTile(team, x, y, range, pred, usePriority, source)`:
    /// the closest building of `team` for which `pred` holds, ordered by
    /// `dst - hitSize/2`, optionally preferring higher `Block.priority`.
    ///
    /// Java's `block.targetable` term is side-only (`next.team != source &&
    /// !next.block.targetable`): the port has no `Block.targetable` field yet,
    /// so only the team term is applied (documented deviation).
    #[allow(clippy::too_many_arguments)]
    pub fn find_tile(
        &self,
        world: &World,
        content: &ContentRegistry,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
        use_priority: bool,
        source: Option<u8>,
        mut pred: impl FnMut(Entity) -> bool,
    ) -> Option<Entity> {
        let mut closest: Option<(f32, f32, Entity)> = None;
        for &entity in &self.all {
            if world.get::<TeamComp>(entity).map(|t| t.team) != Some(team) {
                continue;
            }
            let Some(def) = building_def(world, content, entity) else {
                continue;
            };
            if source
                .is_some_and(|source| world.get::<TeamComp>(entity).map(|t| t.team) != Some(source))
            {
                continue;
            }
            if !pred(entity) {
                continue;
            }
            let Some(pos) = world.get::<Pos>(entity) else {
                continue;
            };
            let bdst = ((pos.x - x).powi(2) + (pos.y - y).powi(2)).sqrt()
                - (def.size as f32 * crate::config::TILESIZE as f32) / 2.0;
            if bdst >= range {
                continue;
            }
            let better = match closest {
                None => true,
                Some((best_dst, best_priority, _)) => {
                    (bdst < best_dst && (!use_priority || best_priority <= def.priority))
                        || (use_priority && best_priority < def.priority)
                }
            };
            if better {
                closest = Some((bdst, def.priority, entity));
            }
        }
        closest.map(|(_, _, entity)| entity)
    }

    /// `BlockIndexer.getEnemy(team, flag)`: every building carrying `flag` that
    /// does not belong to `team` (derelict skipped unless `coreCapture`), in
    /// team-id then entity order (`BlockIndexer.java:getEnemy`).
    pub fn enemy_flagged(&self, world: &World, team: u8, flag: BlockFlag) -> Vec<Entity> {
        let mut out = Vec::new();
        // The bucket vec holds one `FLAGS_PER_TEAM` block per team; never
        // iterate past the allocated teams.
        let team_slots = self.flags.len() / FLAGS_PER_TEAM;
        for other in 0..team_slots {
            if other == team as usize || (other == 0 && is_derelict(world, 0)) {
                continue;
            }
            out.extend_from_slice(&self.flags[other * FLAGS_PER_TEAM + flag_index(flag)]);
        }
        out
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
            // `BlockIndexer.findTile(... b -> pred.get(b) && b.isDiscovered(team)
            // ...)` (BlockIndexer.java:468).
            if !crate::game::fog_gate::building_discovered(world, content, None, entity, team) {
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

    /// Nearest hostile building within `range` (no flag requirement).
    ///
    /// Unlike [`Self::find_enemy_tile`] (which restricts to priority flags),
    /// this considers every indexed building, matching `Units.bestTarget`'s
    /// building half for turret targeting (plan 10 §3.7). Ties break by entity
    /// index for determinism.
    pub fn find_enemy_building(
        &self,
        world: &World,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
        allow_under_blocks: bool,
    ) -> Option<Entity> {
        let range2 = range * range;
        let mut best: Option<(f32, Entity)> = None;
        for &entity in &self.all {
            // `findEnemyTile`: skip own team and `Team.derelict` (id 0) unless
            // `rules.coreCapture` (BlockIndexer.java:466).
            let other = world.get::<TeamComp>(entity).map(|t| t.team);
            if other == Some(team) || other.is_some_and(|other| is_derelict(world, other)) {
                continue;
            }
            // `BlockIndexer.findTile(...)` discovery gate (BlockIndexer.java:468);
            // no content borrow here, so the size comes from the `BlockTable`.
            if !crate::game::fog_gate::building_discovered_table(world, None, entity, team) {
                continue;
            }
            // `AIController.target`: `targetUnderBlocks || !t.block.underBullets`
            // (AIController.java:282).
            if !allow_under_blocks {
                let under = world
                    .get::<crate::ecs::BuildingComp>(entity)
                    .and_then(|comp| self.under_bullets.get(comp.block.raw() as usize).copied())
                    .unwrap_or(false);
                if under {
                    continue;
                }
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
        best.map(|(_, entity)| entity)
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

/// Inserts `entity` keeping the vector sorted by entity index.
fn insert_sorted(entities: &mut Vec<Entity>, entity: Entity) {
    // Insert by ECS entity index, the same key `rebuild` sorts on, so the
    // incremental and wholesale paths are byte-identical.
    match entities.binary_search_by_key(&entity.index(), |existing| existing.index()) {
        Ok(_) => {}
        Err(index) => entities.insert(index, entity),
    }
}

/// Marks a content id as present in the `blocksPresent` table.
fn mark_present(present: &mut [bool], block: BlockId) {
    if let Some(slot) = present.get_mut(block.raw() as usize) {
        *slot = true;
    }
}

/// `BlockIndexer.updatePresentOres` cannot see the floor half of an ore tile
/// through `Tile::drop` (which reads the *block* drop), so the ore helpers
/// mirror `Tile.drop()`/`Tile.wallDrop()` directly (`Tile.java:580-589`).
fn floor_drop(content: &ContentRegistry, tile: &crate::world::Tile) -> Option<ItemId> {
    content
        .block(tile.overlay)
        .and_then(|def| def.item_drop)
        .or_else(|| content.block(tile.floor).and_then(|def| def.item_drop))
}

fn wall_drop(content: &ContentRegistry, tile: &crate::world::Tile) -> Option<ItemId> {
    let block = content.block(tile.block)?;
    if !block.solid {
        return None;
    }
    // `block.solid ? block.itemDrop != null ? block.itemDrop :
    // overlay.wallOre && !block.synthetic() ? overlay.itemDrop : null : null`
    // (`Tile.java:584-589`; the port has no `Block.synthetic` flag).
    block.item_drop.or_else(|| {
        content
            .block(tile.overlay)
            .filter(|def| def.wall_ore)
            .and_then(|def| def.item_drop)
    })
}

/// `Floor.shouldIndex(tile)`: true for every floor except `SteamVent`, which
/// indexes only its top-right tile (`SteamVent.isCenterVent`: the 3×3 around
/// `tile + (1, 1)` is all the same vent floor; `SteamVent.java:66-87`).
fn floor_should_index(flag: BlockFlag, grid: &WorldGrid, tile: &crate::world::Tile) -> bool {
    if flag != BlockFlag::SteamVent {
        return true;
    }
    let (cx, cy) = (tile.x as i32 + 1, tile.y as i32 + 1);
    for dx in -1..=1 {
        for dy in -1..=1 {
            let Some(other) = grid.tiles.getn(cx + dx, cy + dy) else {
                return false;
            };
            // `checkAdjacent` compares floor blocks only (SteamVent.java:83-92).
            if other.floor != tile.floor {
                return false;
            }
        }
    }
    true
}

/// `Building.hitSize()` (`block.size * tilesize`).
fn building_hit_size(world: &World, content: &ContentRegistry, entity: Entity) -> Option<f32> {
    let building = world.get::<Building>(entity)?;
    let def = content.block(building.block)?;
    Some(def.size as f32 * crate::config::TILESIZE as f32)
}

fn building_def<'a>(
    world: &World,
    content: &'a ContentRegistry,
    entity: Entity,
) -> Option<&'a crate::content::registries::blocks::BlockDef> {
    let building = world.get::<Building>(entity)?;
    content.block(building.block)
}

fn building_privileged(world: &World, content: &ContentRegistry, entity: Entity) -> bool {
    building_def(world, content, entity).is_some_and(|def| def.privileged)
}

/// `b.within(x, y, range + b.hitSize()/2f)`.
fn building_within(
    world: &World,
    content: &ContentRegistry,
    entity: Entity,
    x: f32,
    y: f32,
    range: f32,
) -> bool {
    let Some(pos) = world.get::<Pos>(entity) else {
        return false;
    };
    let radius = range + building_hit_size(world, content, entity).unwrap_or(0.0) / 2.0;
    let dx = pos.x - x;
    let dy = pos.y - y;
    dx * dx + dy * dy <= radius * radius
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

/// Whether `team` is `Team.derelict` in this world. Java reserves id 0 for
/// derelict ruins (never targeted unless `rules.coreCapture`); the P0 spine and
/// the plan harnesses use 0 as the player team, so 0 only counts as derelict
/// when the live rules put the player elsewhere (campaign `defaultTeam = 1`).
pub fn is_derelict(world: &World, team: u8) -> bool {
    if team != 0 {
        return false;
    }
    let campaign = world.get_resource::<crate::game::runtime::CampaignRuntime>();
    // `findEnemyTile`: `enemy == Team.derelict && !state.rules.coreCapture`
    // skips the derelict team; with `coreCapture` its blocks are capturable
    // targets (BlockIndexer.java:466).
    if campaign.is_some_and(|runtime| runtime.session.rules.core_capture) {
        return false;
    }
    let campaign_default = campaign.map(|runtime| runtime.session.rules.default_team);
    let build_default = world
        .get_resource::<crate::world::limits::BuildRules>()
        .map(|rules| rules.default_team);
    campaign_default.is_some_and(|t| t != 0) || build_default.is_some_and(|t| t != 0)
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
        assert!(indexer.has_flagged(0, BlockFlag::Core));
        assert!(indexer.has_flagged(0, BlockFlag::Factory));
        assert!(!indexer.has_flagged(0, BlockFlag::Turret));
        let mut count = 0;
        indexer.each_block(0, BlockFlag::Core, |_| count += 1);
        assert_eq!(count, 1);
    }

    /// #63: Java's `TilePreChangeEvent`/`TileChangeEvent` hooks maintain the
    /// index incrementally; the port now has the matching
    /// `remove_index`/`add_index` pair and both keep a full `rebuild` result.
    #[test]
    fn incremental_add_and_remove_index_match_rebuild() {
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
        let core_entity = harness.build.build_at(4, 4).expect("core entity");
        let factory_entity = harness.build.build_at(12, 12).expect("factory entity");
        let content = &harness.build.content;

        // Start from an empty index and add both buildings incrementally.
        let mut indexer = BlockIndexer::new();
        assert!(indexer.add_index(&harness.build.world, content, core_entity));
        assert!(!indexer.add_index(&harness.build.world, content, core_entity));
        assert!(indexer.add_index(&harness.build.world, content, factory_entity));
        // `all` is entity-index ordered (the same key `rebuild` sorts on), not
        // placement ordered.
        let mut expected = vec![core_entity, factory_entity];
        expected.sort_by_key(|entity| entity.index());
        assert_eq!(indexer.all(), expected.as_slice());
        assert!(indexer.has_flagged(0, BlockFlag::Core));
        assert!(indexer.has_flagged(0, BlockFlag::Factory));

        // Removing a building drops its flag buckets and the flat list entry.
        assert!(indexer.remove_index(&harness.build.world, core_entity));
        assert!(!indexer.remove_index(&harness.build.world, core_entity));
        assert_eq!(indexer.all(), &[factory_entity]);
        assert!(!indexer.has_flagged(0, BlockFlag::Core));

        // Same state as a full rebuild over the untouched world, minus the
        // building the incremental index removed (the world still holds it).
        let mut rebuilt = BlockIndexer::new();
        rebuilt.rebuild(&mut harness.build.world, content);
        let mut expected: Vec<Entity> = rebuilt
            .all()
            .iter()
            .copied()
            .filter(|entity| *entity != core_entity)
            .collect();
        expected.sort_by_key(|entity| entity.index());
        assert_eq!(expected, indexer.all());
        assert_eq!(rebuilt.damaged(), indexer.damaged());
    }

    /// #63: the per-team flag buckets cover all 256 teams
    /// (`BlockIndexer.flagMap = new Seq[Team.all.length][BlockFlag.all.length]`);
    /// the old 256-slot stride only reached teams 0..17 and panicked past them.
    #[test]
    fn high_team_ids_use_their_own_flag_bucket() {
        let mut harness = UnitHarness::new(16, 16, 1);
        harness.build.rules.default_team = 200;
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        assert!(harness.build.place(4, 4, core, 0, true));
        let mut indexer = BlockIndexer::new();
        indexer.rebuild(&mut harness.build.world, &harness.build.content);
        assert!(indexer.has_flagged(200, BlockFlag::Core));
        assert!(!indexer.has_flagged(201, BlockFlag::Core));
        let mut count = 0;
        indexer.each_block(200, BlockFlag::Core, |_| count += 1);
        assert_eq!(count, 1);
    }

    /// #63: `getFlaggedFloors` mirrors `SteamVent.isCenterVent` (only the tile
    /// whose top-right neighbour heads a full 3×3 vent is indexed).
    #[test]
    fn flagged_floors_index_steam_vent_top_right() {
        let harness = UnitHarness::new(16, 16, 1);
        let content = &harness.build.content;
        let vent = content.block_id("rhyolite-vent").expect("rhyolite-vent");
        let stone = content.block_id("stone").expect("stone");
        let mut grid = WorldGrid::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                grid.tiles.get_mut(x, y).floor = stone;
            }
        }
        for y in 4..=6 {
            for x in 4..=6 {
                grid.tiles.get_mut(x, y).floor = vent;
            }
        }
        let mut indexer = BlockIndexer::new();
        indexer.rebuild_static(&grid, content);
        assert_eq!(
            indexer.get_flagged_floors(BlockFlag::SteamVent),
            &[TilePos::new(4, 4)]
        );
        assert!(indexer.get_flagged_floors(BlockFlag::Core).is_empty());
    }

    /// #63: `getAllPresentOres`/`hasOre`/`hasWallOre` + incremental tile
    /// updates (`addFloorIndex`/`removeFloorIndex`).
    #[test]
    fn present_ores_track_floor_and_wall_ore() {
        let harness = UnitHarness::new(16, 16, 1);
        let content = &harness.build.content;
        let copper = content.item_by_name("copper").expect("copper").id;
        let thorium = content.item_by_name("thorium").expect("thorium").id;
        let ore_copper = content.block_id("ore-copper").expect("ore-copper");
        let ore_wall_thorium = content
            .block_id("ore-wall-thorium")
            .expect("ore-wall-thorium");
        let stone = content.block_id("stone").expect("stone");
        let stone_wall = content.block_id("stone-wall").expect("stone-wall");
        let mut grid = WorldGrid::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                grid.tiles.get_mut(x, y).floor = stone;
            }
        }
        grid.tiles.get_mut(2, 2).floor = ore_copper;
        grid.tiles.get_mut(3, 3).block = stone_wall;
        grid.tiles.get_mut(3, 3).overlay = ore_wall_thorium;

        let mut indexer = BlockIndexer::new();
        indexer.rebuild_static(&grid, content);
        assert!(indexer.has_ore(copper));
        assert!(indexer.has_wall_ore(thorium));
        assert!(indexer.all_present_ores().contains(&copper));
        assert!(indexer.all_present_ores().contains(&thorium));
        assert!(indexer.is_block_present(stone_wall));

        // Remove the floor ore (caller invokes before mutating the tile).
        indexer.remove_floor_index(&grid, content, 2, 2);
        assert!(!indexer.has_ore(copper));
        assert!(!indexer.all_present_ores().contains(&copper));
        grid.tiles.get_mut(2, 2).floor = stone;
        indexer.add_floor_index(&grid, content, 2, 2);
        assert!(!indexer.has_ore(copper));

        // Re-add the ore through the incremental path.
        grid.tiles.get_mut(2, 2).floor = ore_copper;
        indexer.add_floor_index(&grid, content, 2, 2);
        assert!(indexer.has_ore(copper));
    }

    /// #63: range `eachBlock`/`allBuildings`/`findTile`.
    #[test]
    fn range_scans_and_find_tile_use_the_flat_index() {
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
        // Keep the core at (4,4)'s centre; the factory is far away.
        let core_entity = harness.build.build_at(4, 4).expect("core");
        let factory_entity = harness.build.build_at(12, 12).expect("factory");
        let mut indexer = BlockIndexer::new();
        indexer.rebuild(&mut harness.build.world, &harness.build.content);
        let (cx, cy) = {
            let pos = harness.build.world.get::<Pos>(core_entity).expect("pos");
            (pos.x, pos.y)
        };

        let mut near = Vec::new();
        let found = indexer.each_block_range(
            &harness.build.world,
            &harness.build.content,
            Some(harness.build.rules.default_team),
            cx,
            cy,
            4.0,
            |_| true,
            |entity| near.push(entity),
        );
        assert!(found);
        assert_eq!(near, vec![core_entity], "only the core is in range");

        let mut every = Vec::new();
        indexer.all_buildings(
            &harness.build.world,
            &harness.build.content,
            cx,
            cy,
            10_000.0,
            |entity| every.push(entity),
        );
        assert_eq!(every, vec![core_entity, factory_entity]);

        let closest = indexer.find_tile(
            &harness.build.world,
            &harness.build.content,
            harness.build.rules.default_team,
            cx,
            cy,
            4.0,
            false,
            None,
            |_| true,
        );
        assert_eq!(closest, Some(core_entity));
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
