// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mindustry.game.Teams` — the per-team registry, `TeamData` caches and the
//! shared team inventory (plan 12 M1).
//!
//! Ported from `core/src/mindustry/game/Teams.java`. The per-tick caches
//! (`present`, `bosses`, unit/building type maps, quadtrees, clustered counts)
//! are rebuilt from deterministic snapshot records ([`BuildingRecord`] /
//! [`UnitRecord`]) instead of iterating the ECS `Groups` directly: the adapter
//! that turns `Groups.build`/`Groups.unit` into records is the thin ECS wiring
//! step (joint with plan 11), which keeps this module testable without a world
//! and keeps iteration order explicit and stable.
//!
//! `TeamInventory` (plan 08 R2 default) lives on [`Teams`] as
//! `IndexMap<TeamId, ItemModule>`; `Team::items()` routes here.

use bevy_ecs::entity::Entity;
use indexmap::IndexMap;

use crate::content::BlockId;
use crate::ecs::TeamId;
use crate::math::ArcRand;
use crate::world::config::ConfigValue;
use crate::world::modules::ItemModule;

use super::quad_tree::QuadTree;
use super::rules::{CLUSTER_CHUNK_SIZE, Rules};
use super::team::NEOPLASTIC;

/// Plan-11 `BaseBuilderAI` (plan 11 M5). The per-`TeamData` field is the
/// integration seam owned here (plan 12) but implemented by plan 11.
pub use crate::ai::base_builder_ai::BaseBuilderAi;

/// One live building, as seen by [`Teams::update_team_stats`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingRecord {
    /// Entity handle.
    pub entity: Entity,
    /// Owning team.
    pub team: u8,
    /// Block type.
    pub block: BlockId,
    /// World-pixel center x.
    pub x: f32,
    /// World-pixel center y.
    pub y: f32,
    /// Whether this is a core (`CoreBuild`).
    pub is_core: bool,
    /// Whether this is a turret (enters `turretTree`).
    pub is_turret: bool,
    /// Turret range in world pixels (0 for non-turrets).
    pub turret_range: f32,
    /// `Block.privileged` (never converted to derelict).
    pub privileged: bool,
}

/// One live unit, as seen by [`Teams::update_team_stats`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnitRecord {
    /// Entity handle.
    pub entity: Entity,
    /// Owning team.
    pub team: u8,
    /// Unit type id (`UnitType.id`).
    pub type_id: u16,
    /// Unit type name (debug/golden).
    pub type_name: String,
    /// World-pixel x.
    pub x: f32,
    /// World-pixel y.
    pub y: f32,
    /// Whether the unit is a boss (`Unit.isBoss`).
    pub is_boss: bool,
    /// Whether the unit flies (excluded from ground clustering).
    pub is_flying: bool,
    /// Payload unit type ids carried by this unit (`Payloadc.payloads`).
    pub payload_types: Vec<u16>,
}

/// A broken block queued for drone rebuild (`Teams.BlockPlan`).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockPlan {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Rotation.
    pub rotation: i8,
    /// Block type.
    pub block: BlockId,
    /// Placement config.
    pub config: Option<ConfigValue>,
    /// Whether the plan was consumed (`removed`).
    pub removed: bool,
}

/// Result of [`Teams::destroy_to_derelict`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DerelictReport {
    /// Buildings that are killed outright (cores + random 20%).
    pub killed: Vec<Entity>,
    /// Non-core buildings that remain as derelicts.
    pub derelict: Vec<Entity>,
    /// Derelict entities in upstream `chunked(1000)` order.
    pub chunks: Vec<Vec<Entity>>,
    /// Repair plans queued before conversion.
    pub plans: Vec<BlockPlan>,
}

/// Per-team caches (`Teams.TeamData`).
#[derive(Debug, Clone, PartialEq)]
pub struct TeamData {
    /// Owning team.
    pub team: TeamId,
    /// Base-builder AI handle (plan 11 M5).
    pub build_ai: Option<BaseBuilderAi>,
    /// RTS AI handle (plan 11).
    pub rts_ai: Option<super::super::ai::rts_ai::RtsAi>,
    /// Whether the team has any block/unit presence (`presentFlag`).
    pub present_flag: bool,
    /// Ground-unit cluster counts keyed by packed cluster cell.
    pub clustered_counts: IndexMap<i32, i32>,
    /// Last cluster rebuild time in ticks (`lastClusterUpdateTimer`).
    pub last_cluster_update_timer: f32,
    /// Enemies with cores or spawn points.
    pub core_enemies: Vec<TeamId>,
    /// Repair plans for drones.
    pub plans: std::collections::VecDeque<BlockPlan>,
    /// Live core entities.
    pub cores: Vec<Entity>,
    /// Last known live core.
    pub last_core: Option<Entity>,
    /// All-building spatial index (`buildingTree`).
    pub building_tree: Option<QuadTree<Entity>>,
    /// Turret spatial index (`turretTree`).
    pub turret_tree: Option<QuadTree<Entity>>,
    /// Unit spatial index (`unitTree`).
    pub unit_tree: Option<QuadTree<Entity>>,
    /// Current unit cap.
    pub unit_cap: i32,
    /// Total unit count.
    pub unit_count: i32,
    /// Per-type unit counts, indexed by `UnitType.id`.
    pub type_counts: Vec<i32>,
    /// Buildings by block type (`buildingTypes`).
    pub building_types: IndexMap<BlockId, Vec<Entity>>,
    /// Live units (`units`).
    pub units: Vec<Entity>,
    /// Live players (`players`).
    pub players: Vec<Entity>,
    /// All buildings (`buildings`).
    pub buildings: Vec<Entity>,
    /// Units by type (`unitsByType`), indexed by `UnitType.id`.
    pub units_by_type: Vec<Vec<Entity>>,
    /// Building snapshot records for the current frame (superset of `buildings`).
    pub building_records: Vec<BuildingRecord>,
    /// Unit snapshot records for the current frame (superset of `units`).
    pub unit_records: Vec<UnitRecord>,
}

impl TeamData {
    /// Creates empty data for `team`.
    pub fn new(team: TeamId) -> Self {
        Self {
            team,
            build_ai: None,
            rts_ai: None,
            present_flag: false,
            clustered_counts: IndexMap::new(),
            last_cluster_update_timer: -100.0,
            core_enemies: Vec::new(),
            plans: std::collections::VecDeque::new(),
            cores: Vec::new(),
            last_core: None,
            building_tree: None,
            turret_tree: None,
            unit_tree: None,
            unit_cap: 0,
            unit_count: 0,
            type_counts: Vec::new(),
            building_types: IndexMap::new(),
            units: Vec::new(),
            players: Vec::new(),
            buildings: Vec::new(),
            units_by_type: Vec::new(),
            building_records: Vec::new(),
            unit_records: Vec::new(),
        }
    }

    /// Buildings of a block type (`getBuildings`).
    pub fn get_buildings(&self, block: BlockId) -> &[Entity] {
        self.building_types
            .get(&block)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Count of buildings of a block type (`getCount`).
    pub fn get_count(&self, block: BlockId) -> usize {
        self.building_types.get(&block).map_or(0, Vec::len)
    }

    /// Units of a type (`unitCache`/`getUnits`), if the cache exists.
    pub fn unit_cache(&self, type_id: u16) -> Option<&[Entity]> {
        self.units_by_type
            .get(type_id as usize)
            .filter(|list| !list.is_empty())
            .map(Vec::as_slice)
    }

    /// `countType`.
    pub fn count_type(&self, type_id: u16) -> i32 {
        self.type_counts.get(type_id as usize).copied().unwrap_or(0)
    }

    /// `TeamData.active(rules)`.
    pub fn active(&self, rules: &Rules) -> bool {
        (self.team.0 == rules.wave_team && rules.waves)
            || !self.cores.is_empty()
            || !self.buildings.is_empty()
            || (self.team == NEOPLASTIC && !self.units.is_empty())
    }

    /// `TeamData.hasCore`.
    pub fn has_core(&self) -> bool {
        !self.cores.is_empty()
    }

    /// `TeamData.isAlive`.
    pub fn is_alive(&self) -> bool {
        self.has_core()
    }

    /// `TeamData.noCores`.
    pub fn no_cores(&self) -> bool {
        self.cores.is_empty()
    }

    /// `TeamData.core`.
    pub fn core(&self) -> Option<Entity> {
        self.cores.first().copied()
    }

    /// `TeamData.hasAI` (reads the team's `TeamRule`).
    pub fn has_ai(&self, rules: &Rules) -> bool {
        let rule = rules.team_rule(self.team.0);
        rule.rts_ai || rule.build_ai
    }

    /// Approximate clustered ground units near `(x, y)` (`getClustered`).
    ///
    /// Rebuilds the cluster map when `time` advanced more than 10 ticks since
    /// the last rebuild (upstream `Time.time`).
    pub fn get_clustered(&mut self, x: f32, y: f32, time: f32) -> i32 {
        if time > self.last_cluster_update_timer + 10.0 {
            self.last_cluster_update_timer = time;
            self.clustered_counts.clear();
            for record in &self.unit_records {
                if record.is_flying {
                    continue;
                }
                let key = cluster_key(record.x, record.y);
                *self.clustered_counts.entry(key).or_insert(0) += 1;
            }
        }
        self.clustered_counts
            .get(&cluster_key(x, y))
            .copied()
            .unwrap_or(0)
    }
}

/// `TeamData.clusterKey` (`(floor(x/70) & 0xFFF) << 12 | (floor(y/70) & 0xFFF)`).
pub fn cluster_key(x: f32, y: f32) -> i32 {
    let cx = (x / CLUSTER_CHUNK_SIZE).floor() as i32 & 0xFFF;
    let cy = (y / CLUSTER_CHUNK_SIZE).floor() as i32 & 0xFFF;
    (cx << 12) | cy
}

/// The 256-team registry (`Teams`).
#[derive(Debug, Clone)]
pub struct Teams {
    map: Vec<Option<TeamData>>,
    /// Active teams (`Teams.active`).
    pub active: Vec<TeamId>,
    /// Teams with block/unit presence (`Teams.present`).
    pub present: Vec<TeamId>,
    /// Current boss units (`Teams.bosses`).
    pub bosses: Vec<Entity>,
    inventories: IndexMap<u8, ItemModule>,
}

impl Default for Teams {
    fn default() -> Self {
        Self::new()
    }
}

impl Teams {
    /// Creates the registry with the wave team pre-activated (`Teams()` adds
    /// `Team.crux`).
    pub fn new() -> Self {
        let mut teams = Self {
            map: (0..256).map(|_| None).collect(),
            active: vec![TeamId(2)],
            present: Vec::new(),
            bosses: Vec::new(),
            inventories: IndexMap::new(),
        };
        // Upstream `active.add(get(Team.crux))` creates the wave team's data.
        teams.get(TeamId(2));
        teams
    }

    /// `Teams.get(team)`, lazily creating the data.
    pub fn get(&mut self, team: TeamId) -> &mut TeamData {
        self.map[team.0 as usize].get_or_insert_with(|| TeamData::new(team))
    }

    /// `Teams.getOrNull(team)`.
    pub fn get_or_null(&self, team: TeamId) -> Option<&TeamData> {
        self.map[team.0 as usize].as_ref()
    }

    /// Immutable team data, if it exists.
    pub fn data(&self, team: TeamId) -> Option<&TeamData> {
        self.get_or_null(team)
    }

    /// `Teams.cores(team)`.
    pub fn cores(&mut self, team: TeamId) -> &[Entity] {
        &self.get(team).cores
    }

    /// `Teams.playerCores()`.
    pub fn player_cores(&mut self, rules: &Rules) -> &[Entity] {
        self.cores(TeamId(rules.default_team))
    }

    /// `Teams.isActive(team)`.
    pub fn is_active(&mut self, team: TeamId, rules: &Rules) -> bool {
        self.get(team).active(rules)
    }

    /// `Teams.canInteract(team, other)`.
    pub fn can_interact(&self, team: TeamId, other: TeamId) -> bool {
        team == other || other == TeamId(0)
    }

    /// `Teams.getActive()`: prunes inactive entries and returns the live list.
    pub fn get_active(&mut self, rules: &Rules) -> &[TeamId] {
        self.active.retain(|team| {
            self.map[team.0 as usize]
                .as_ref()
                .is_some_and(|data| data.active(rules))
        });
        &self.active
    }

    /// `Teams.updateActive(team)`.
    pub fn update_active(&mut self, team: TeamId, rules: &Rules) {
        let is_active = self.map[team.0 as usize]
            .as_ref()
            .is_some_and(|data| data.active(rules));
        if is_active && !self.active.contains(&team) {
            self.active.push(team);
            self.update_enemies(rules);
        }
    }

    /// `Teams.registerCore(core)`.
    pub fn register_core(&mut self, entity: Entity, team: TeamId, rules: &Rules) {
        {
            let data = self.get(team);
            if !data.cores.contains(&entity) {
                data.cores.push(entity);
            }
        }
        self.update_active(team, rules);
    }

    /// `Teams.unregisterCore(entity)`.
    pub fn unregister_core(&mut self, entity: Entity, team: TeamId, rules: &Rules) {
        let core_count;
        {
            let data = self.get(team);
            data.cores.retain(|existing| *existing != entity);
            core_count = data.cores.len();
        }
        if core_count == 0 {
            self.active.retain(|active| *active != team);
            self.update_enemies(rules);
        }
    }

    /// `TeamInventory` accessor (plan 08 R2): the team's item module, lazily
    /// created with `items` slots.
    pub fn inventory(&mut self, team: TeamId, items: usize) -> &mut ItemModule {
        self.inventories
            .entry(team.0)
            .or_insert_with(|| ItemModule::with_items(items))
    }

    /// Read-only inventory.
    pub fn inventory_ref(&self, team: TeamId) -> Option<&ItemModule> {
        self.inventories.get(&team.0)
    }

    /// `Team.items()`: the core inventory for a team, or `None` when the team
    /// has no core (upstream returns `ItemModule.empty`).
    pub fn team_items(&self, team: TeamId) -> Option<&ItemModule> {
        self.inventory_ref(team)
    }

    /// Rebuilds `present`, `bosses` and all per-team caches from snapshots
    /// (`Teams.updateTeamStats`).
    ///
    /// Building caches are re-derived from `buildings` here (upstream updates
    /// them on add/remove); this is the behavior-equivalent deterministic form
    /// until the ECS adapter registers incremental changes.
    pub fn update_team_stats(
        &mut self,
        buildings: &[BuildingRecord],
        units: &[UnitRecord],
        players: &[(Entity, u8)],
        rules: &Rules,
    ) {
        self.present.clear();
        self.bosses.clear();

        // Reset per-team caches. `lastCore` is intentionally retained.
        for data in self.map.iter_mut().flatten() {
            data.present_flag = false;
            data.unit_count = 0;
            data.units.clear();
            data.unit_records.clear();
            data.players.clear();
            data.building_records.clear();
            data.buildings.clear();
            data.building_types.clear();
            data.type_counts.clear();
            data.units_by_type.clear();
            data.cores.clear();
            if let Some(tree) = data.building_tree.as_mut() {
                tree.clear();
            }
            if let Some(tree) = data.turret_tree.as_mut() {
                tree.clear();
            }
            if let Some(tree) = data.unit_tree.as_mut() {
                tree.clear();
            }
        }

        for record in buildings {
            // Ensure the slot exists, then borrow only `self.map`.
            let data = self.map[record.team as usize]
                .get_or_insert_with(|| TeamData::new(TeamId(record.team)));
            data.building_records.push(*record);
            data.buildings.push(record.entity);
            data.building_types
                .entry(record.block)
                .or_default()
                .push(record.entity);
            data.building_tree.get_or_insert_with(QuadTree::new).insert(
                record.x,
                record.y,
                record.entity,
            );
            if record.is_turret {
                data.turret_tree.get_or_insert_with(QuadTree::new).insert(
                    record.x,
                    record.y,
                    record.entity,
                );
            }
            if record.is_core && !data.cores.contains(&record.entity) {
                data.cores.push(record.entity);
            }
        }

        let mut bosses = Vec::new();
        for record in units {
            let data = self.map[record.team as usize]
                .get_or_insert_with(|| TeamData::new(TeamId(record.team)));
            data.unit_records.push(record.clone());
            data.units.push(record.entity);
            data.present_flag = true;
            if record.team == rules.wave_team && record.is_boss {
                bosses.push(record.entity);
            }
            if data.type_counts.len() <= record.type_id as usize {
                data.type_counts.resize(record.type_id as usize + 1, 0);
            }
            data.type_counts[record.type_id as usize] += 1;
            if data.units_by_type.len() <= record.type_id as usize {
                data.units_by_type
                    .resize_with(record.type_id as usize + 1, Vec::new);
            }
            data.units_by_type[record.type_id as usize].push(record.entity);
            data.unit_count += 1;
            // Recursive payload unit counting (`Teams.count`).
            for payload in &record.payload_types {
                data.unit_count += 1;
                if data.type_counts.len() <= *payload as usize {
                    data.type_counts.resize(*payload as usize + 1, 0);
                }
                data.type_counts[*payload as usize] += 1;
            }
            data.unit_tree.get_or_insert_with(QuadTree::new).insert(
                record.x,
                record.y,
                record.entity,
            );
        }
        self.bosses = bosses;

        for (entity, team) in players {
            self.get(TeamId(*team)).players.push(*entity);
        }

        // Mark block presence and build `present`.
        let active = self.active.clone();
        for index in 0..256 {
            let team = TeamId(index as u8);
            if let Some(data) = self.map[index].as_mut() {
                data.present_flag = data.present_flag || !data.buildings.is_empty();
                let is_active = data.active(rules) || active.contains(&team);
                if data.present_flag || is_active {
                    self.present.push(team);
                }
            }
        }
    }

    /// Rebuilds `coreEnemies` for the active teams (`Teams.updateEnemies`).
    pub fn update_enemies(&mut self, rules: &Rules) {
        let wave_team = TeamId(rules.wave_team);
        if rules.waves && !self.active.contains(&wave_team) {
            // Ensure the data slot exists before it enters the active list.
            self.get(wave_team);
            self.active.push(wave_team);
        }
        let active = self.active.clone();
        for team in &active {
            let enemies: Vec<TeamId> = active
                .iter()
                .copied()
                .filter(|other| other != team)
                .collect();
            if let Some(data) = self.map[team.0 as usize].as_mut() {
                data.core_enemies = enemies;
            }
        }
    }

    /// `TeamData.destroyToDerelict()`: converts a dead team's presence to
    /// derelict.
    ///
    /// The scheduling side effects are returned as a [`DerelictReport`] for the
    /// ECS adapter to apply (`Time.run` kills become deterministic same-tick
    /// kills plus the 20% coin flip).
    pub fn destroy_to_derelict(&mut self, team: TeamId, rng: &mut ArcRand) -> DerelictReport {
        let records = {
            let data = self.get(team);
            data.plans.clear();
            std::mem::take(&mut data.building_records)
        };
        let mut report = DerelictReport::default();

        for record in &records {
            if record.privileged {
                continue;
            }
            if record.is_core {
                report.killed.push(record.entity);
                continue;
            }
            report.plans.push(BlockPlan {
                x: (record.x / crate::config::TILESIZE as f32) as i16,
                y: (record.y / crate::config::TILESIZE as f32) as i16,
                rotation: 0,
                block: record.block,
                config: None,
                removed: false,
            });
            if rng.chance(0.2) {
                report.killed.push(record.entity);
            } else {
                report.derelict.push(record.entity);
            }
        }
        report.chunks = report
            .derelict
            .chunks(1000)
            .map(<[Entity]>::to_vec)
            .collect();

        {
            let data = self.get(team);
            for plan in &report.plans {
                data.plans.push_back(plan.clone());
            }
            data.units.clear();
            data.unit_records.clear();
            data.buildings.clear();
            data.building_records.clear();
            data.building_types.clear();
            data.cores.clear();
            data.type_counts.clear();
            data.units_by_type.clear();
            if let Some(tree) = data.building_tree.as_mut() {
                tree.clear();
            }
            if let Some(tree) = data.turret_tree.as_mut() {
                tree.clear();
            }
            if let Some(tree) = data.unit_tree.as_mut() {
                tree.clear();
            }
        }
        self.inventories.shift_remove(&team.0);
        self.active.retain(|active| *active != team);
        report
    }

    /// `Teams.closestCore(x, y, team)`.
    pub fn closest_core(&self, x: f32, y: f32, team: TeamId) -> Option<Entity> {
        let data = self.get_or_null(team)?;
        let mut best: Option<(f32, Entity)> = None;
        for entity in &data.cores {
            // No positions on `cores`; use the building record lookup.
            if let Some(record) = data
                .building_records
                .iter()
                .find(|record| record.entity == *entity)
            {
                let dx = record.x - x;
                let dy = record.y - y;
                let dist2 = dx * dx + dy * dy;
                if best.is_none_or(|(best_dist, _)| dist2 < best_dist) {
                    best = Some((dist2, *entity));
                }
            }
        }
        best.map(|(_, entity)| entity)
    }

    /// `Teams.closestEnemyCore(x, y, team)`.
    pub fn closest_enemy_core(&self, x: f32, y: f32, team: TeamId) -> Option<Entity> {
        let data = self.get_or_null(team)?;
        let enemies = data.core_enemies.clone();
        let mut best: Option<(f32, Entity)> = None;
        for enemy in enemies {
            if let Some(core) = self.closest_core(x, y, enemy)
                && let Some(enemy_data) = self.get_or_null(enemy)
                && let Some(record) = enemy_data
                    .building_records
                    .iter()
                    .find(|record| record.entity == core)
            {
                let dx = record.x - x;
                let dy = record.y - y;
                let dist2 = dx * dx + dy * dy;
                if best.is_none_or(|(best_dist, _)| dist2 < best_dist) {
                    best = Some((dist2, core));
                }
            }
        }
        best.map(|(_, entity)| entity)
    }

    /// Whether any enemy core lies within `radius` of `(x, y)` (`anyEnemyCoresWithin`).
    pub fn any_enemy_cores_within(&self, team: TeamId, x: f32, y: f32, radius: f32) -> bool {
        let Some(data) = self.get_or_null(team) else {
            return false;
        };
        let radius2 = radius * radius;
        for enemy in &data.core_enemies {
            if let Some(enemy_data) = self.get_or_null(*enemy) {
                for record in &enemy_data.building_records {
                    if record.is_core {
                        let dx = record.x - x;
                        let dy = record.y - y;
                        if dx * dx + dy * dy <= radius2 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::io::json::rules::{Rules, TeamRule};

    fn entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).expect("valid test entity index")
    }

    fn building(team: u8, block: u16, x: f32, y: f32, is_core: bool) -> BuildingRecord {
        BuildingRecord {
            entity: entity(1000 + x as u32),
            team,
            block: BlockId::new(block),
            x,
            y,
            is_core,
            is_turret: false,
            turret_range: 0.0,
            privileged: false,
        }
    }

    fn unit(team: u8, type_id: u16, x: f32, y: f32) -> UnitRecord {
        UnitRecord {
            entity: entity(2000 + type_id as u32),
            team,
            type_id,
            type_name: format!("unit-{type_id}"),
            x,
            y,
            is_boss: false,
            is_flying: false,
            payload_types: Vec::new(),
        }
    }

    #[test]
    fn new_pre_activates_wave_team() {
        let teams = Teams::new();
        assert_eq!(teams.active, vec![TeamId(2)]);
    }

    #[test]
    fn update_stats_counts_caches_and_present() {
        let mut teams = Teams::new();
        let mut rules = Rules::default();
        rules.waves = false;
        let buildings = vec![
            building(1, 1, 100.0, 100.0, true),
            building(1, 2, 200.0, 100.0, false),
            building(2, 1, 300.0, 100.0, true),
        ];
        let units = vec![unit(1, 7, 110.0, 110.0), unit(1, 7, 120.0, 110.0)];
        teams.update_team_stats(&buildings, &units, &[], &rules);

        let blue = teams.get_or_null(TeamId(1)).unwrap();
        assert!(blue.has_core());
        assert_eq!(blue.buildings.len(), 2);
        assert_eq!(blue.cores.len(), 1);
        assert_eq!(blue.count_type(7), 2);
        assert_eq!(blue.unit_count, 2);
        assert_eq!(blue.get_count(BlockId::new(2)), 1);
        assert!(blue.present_flag);
        assert!(teams.present.contains(&TeamId(1)));
        assert!(teams.present.contains(&TeamId(2)));
    }

    #[test]
    fn register_core_and_inventory() {
        let mut teams = Teams::new();
        let rules = Rules::default();
        let core = entity(77);
        teams.register_core(core, TeamId(1), &rules);
        assert!(teams.cores(TeamId(1)).contains(&core));
        assert!(teams.active.contains(&TeamId(1)), "active updated");

        let inventory = teams.inventory(TeamId(1), 22);
        inventory.add(crate::content::ItemId::COPPER, 10, 100);
        assert_eq!(teams.team_items(TeamId(1)).unwrap().total(), 10);

        teams.unregister_core(core, TeamId(1), &rules);
        assert!(teams.cores(TeamId(1)).is_empty());
        assert!(!teams.active.contains(&TeamId(1)));
    }

    #[test]
    fn update_enemies_tracks_wave_team_when_waves_enabled() {
        let mut teams = Teams::new();
        let mut rules = Rules::default();
        rules.waves = true;
        rules.wave_team = 2;
        rules.default_team = 1;
        teams.register_core(entity(1), TeamId(1), &rules);
        teams.update_enemies(&rules);

        let player = teams.get_or_null(TeamId(1)).unwrap();
        assert!(player.core_enemies.contains(&TeamId(2)));
        let enemy = teams.get_or_null(TeamId(2)).unwrap();
        assert!(enemy.core_enemies.contains(&TeamId(1)));
    }

    #[test]
    fn destroy_to_derelict_chunks_and_clears_inventory() {
        let mut teams = Teams::new();
        let rules = Rules::default();
        // 2500 non-core derelict buildings -> 3 chunks of <=1000.
        let mut buildings = Vec::new();
        for index in 0..2500u32 {
            let x = (index % 50) as f32 * 8.0;
            let y = (index / 50) as f32 * 8.0;
            buildings.push(BuildingRecord {
                entity: entity(index),
                team: 2,
                block: BlockId::new(1),
                x,
                y,
                is_core: false,
                is_turret: false,
                turret_range: 0.0,
                privileged: false,
            });
        }
        // One privileged building (skipped) and one core (killed).
        buildings.push(BuildingRecord {
            entity: entity(9000),
            team: 2,
            block: BlockId::new(1),
            x: 0.0,
            y: 0.0,
            is_core: false,
            is_turret: false,
            turret_range: 0.0,
            privileged: true,
        });
        buildings.push(BuildingRecord {
            entity: entity(9001),
            team: 2,
            block: BlockId::new(1),
            x: 0.0,
            y: 0.0,
            is_core: true,
            is_turret: false,
            turret_range: 0.0,
            privileged: false,
        });
        teams.update_team_stats(&buildings, &[], &[], &rules);
        teams.inventory(TeamId(2), 22);

        let mut rng = ArcRand::new(1);
        let report = teams.destroy_to_derelict(TeamId(2), &mut rng);
        assert!(report.killed.contains(&entity(9001)), "core is killed");
        assert!(
            !report.killed.contains(&entity(9000)),
            "privileged building is never killed"
        );
        assert_eq!(
            report.plans.len(),
            2500,
            "one plan per non-core, non-privileged"
        );
        let chunked_total: usize = report.chunks.iter().map(Vec::len).sum();
        assert_eq!(
            chunked_total + report.killed.len(),
            2501,
            "every non-privileged building is killed or derelict"
        );
        assert!(report.derelict.len() <= 2500);
        assert!(report.chunks.iter().all(|chunk| chunk.len() <= 1000));
        assert_eq!(report.chunks.len(), report.derelict.len().div_ceil(1000));
        assert!(
            teams.inventory_ref(TeamId(2)).is_none(),
            "inventory cleared"
        );
        assert!(teams.get_or_null(TeamId(2)).unwrap().buildings.is_empty());
    }

    #[test]
    fn get_clustered_rebuilds_every_10_ticks() {
        let mut teams = Teams::new();
        let rules = Rules::default();
        let units: Vec<UnitRecord> = (0..5)
            .map(|index| unit(1, 1, 10.0 + index as f32, 10.0))
            .collect();
        teams.update_team_stats(&[], &units, &[(entity(50), 1)], &rules);
        let data = teams.get(TeamId(1));
        assert_eq!(data.get_clustered(10.0, 10.0, 0.0), 5);
        assert_eq!(data.get_clustered(5000.0, 5000.0, 5.0), 0);
        // Rebuild with all units moved (simulate via new stats) still clustered.
        assert!(data.players.contains(&entity(50)));

        // team rule drives has_ai.
        let mut rules2 = Rules::default();
        rules2.team_rule_mut(1).rts_ai = true;
        assert!(data.has_ai(&rules2));
        assert!(!data.has_ai(&rules));
    }

    #[test]
    fn block_plan_and_team_rule_defaults() {
        let plan = BlockPlan {
            x: 1,
            y: 2,
            rotation: 3,
            block: BlockId::new(5),
            config: None,
            removed: false,
        };
        assert_eq!(plan.rotation, 3);
        // Teams.new pre-adds crux, whose default rule is standard.
        let mut rules = Rules::default();
        rules.teams.0.insert(2, TeamRule::default());
        assert!(rules.team_rule(2).protect_cores);
    }
}
