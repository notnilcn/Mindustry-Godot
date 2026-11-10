// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Teams` team transitions: team change, destroy-to-derelict and time-destroy.
//!
//! Upstream: core/src/mindustry/game/Teams.java (destroyToDerelict, timeDestroy).

use super::*;

use crate::sim::query::sorted_by_seq;

impl Sim {
    /// `BuildingComp.changeTeam` for `Call.setTeams` (`BuildingComp.java`):
    /// sets the ECS team comps, moves the block in the team counter and
    /// re-merges the power graph.
    pub fn set_building_team(&mut self, entity: Entity, team: u8) {
        let Some(block) = self
            .ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .map(|building| building.block)
        else {
            return;
        };
        let old_team = self
            .ecs
            .0
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|comp| comp.team)
            .unwrap_or(0);
        if old_team == team {
            return;
        }
        if let Some(mut comp) = self
            .ecs
            .0
            .get_mut::<crate::entities::comp::TeamComp>(entity)
        {
            comp.team = team;
        }
        if let Some(mut comp) = self.ecs.0.get_mut::<BuildingComp>(entity) {
            comp.team = TeamId(team);
        }
        if let Some(mut counter) = self.ecs.0.get_resource_mut::<BlockCounter>() {
            counter.remove(old_team, block, 1);
            counter.add(team, block, 1);
        }
        let Sim {
            ecs,
            grid,
            block_runtime,
            ..
        } = self;
        if let Some(runtime) = block_runtime.as_mut() {
            runtime.remove_power_graph(&mut ecs.0, grid, entity);
            runtime.merge_power_graph(&mut ecs.0, grid, entity);
        }
    }

    /// `TeamData.destroyToDerelict` (`Teams.java:331-362`): a captured team's
    /// cores die, its other buildings convert to derelict (team 0) except a
    /// 20% `rng` roll that is destroyed, its units die and its plans clear.
    pub fn destroy_team_to_derelict(&mut self, team: u8, rng: &mut crate::determinism::SimRng) {
        let team_id = crate::ecs::TeamId(team);
        // Plans clear first (`TeamData.destroyToDerelict` `plans.clear()`).
        if let Some(mut campaign) = self.ecs.0.get_resource_mut::<CampaignRuntime>() {
            campaign.session.teams.get(team_id).plans.clear();
        }
        for entity in self.live_buildings_by_seq() {
            let entity_team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            if entity_team != team {
                continue;
            }
            let privileged = self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .and_then(|building| {
                    self.ecs
                        .0
                        .get_resource::<BlockTable>()
                        .and_then(|table| table.instance(building.block))
                        .map(|inst| inst.def.privileged)
                })
                .unwrap_or(false);
            if privileged {
                continue;
            }
            if self
                .ecs
                .0
                .get::<crate::world::blocks::storage::core_block::CoreBuild>(entity)
                .is_some()
            {
                self.kill_building(entity);
                continue;
            }
            if rng.chance(crate::determinism::RngStream::Sim, 0.2) {
                self.kill_building(entity);
            } else {
                self.set_building_team(entity, 0);
            }
        }
        // Units of the team die (Java runs the kill after a random delay).
        let doomed_units: Vec<Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity_ref| {
                entity_ref
                    .get::<crate::entities::comp::unit::comp::UnitTypeComp>()
                    .is_some()
                    && entity_ref
                        .get::<crate::entities::comp::TeamComp>()
                        .map(|comp| comp.team)
                        == Some(team)
            })
            .map(|entity_ref| entity_ref.id())
            .collect();
        for entity in doomed_units {
            self.kill_building(entity);
        }
    }

    /// `TeamData.timeDestroy` (`Teams.java:365-397`): every non-privileged
    /// building of `team` within `range` of `(x, y)` that is not within
    /// `range` of another remaining core converts to derelict (20% die).
    pub fn time_destroy(
        &mut self,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
        rng: &mut crate::determinism::SimRng,
    ) {
        let cores = self.team_core_positions(team);
        for entity in self.live_buildings_by_seq() {
            let entity_team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            if entity_team != team {
                continue;
            }
            let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(entity) else {
                continue;
            };
            let privileged = self
                .ecs
                .0
                .get_resource::<BlockTable>()
                .and_then(|table| table.instance(building.block))
                .map(|inst| inst.def.privileged)
                .unwrap_or(false);
            if privileged {
                continue;
            }
            let (px, py) = crate::world::BuildHarness::tile_center(
                i32::from(building.tile.x()),
                i32::from(building.tile.y()),
            );
            // `build.within(x, y, range)`: circle test in world pixels.
            if (px - x).powi(2) + (py - y).powi(2) > range * range {
                continue;
            }
            // `!cores.contains(c -> c.within(build, range))`.
            let near_core = cores
                .iter()
                .any(|(cx, cy)| (px - cx).powi(2) + (py - cy).powi(2) <= range * range);
            if near_core {
                continue;
            }
            if rng.chance(crate::determinism::RngStream::Sim, 0.2) {
                self.kill_building(entity);
            } else {
                self.set_building_team(entity, 0);
            }
        }
    }

    /// World-pixel centers of every live core of `team`.
    pub(super) fn team_core_positions(&self, team: u8) -> Vec<(f32, f32)> {
        let mut cores = Vec::new();
        for entity_ref in self.ecs.0.iter_entities() {
            if entity_ref
                .get::<crate::world::blocks::storage::core_block::CoreBuild>()
                .is_none()
            {
                continue;
            }
            if entity_ref
                .get::<crate::entities::comp::TeamComp>()
                .map(|comp| comp.team)
                != Some(team)
            {
                continue;
            }
            let Some(building) = entity_ref.get::<crate::entities::comp::Building>() else {
                continue;
            };
            cores.push(crate::world::BuildHarness::tile_center(
                i32::from(building.tile.x()),
                i32::from(building.tile.y()),
            ));
        }
        cores
    }

    /// Live plan-07 buildings in `(EntitySeq, entity.index())` order.
    pub(super) fn live_buildings_by_seq(&self) -> Vec<Entity> {
        sorted_by_seq(self.ecs.0.iter_entities().filter_map(|entity_ref| {
            entity_ref.get::<crate::entities::comp::Building>()?;
            let seq = entity_ref
                .get::<crate::ecs::EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            Some((seq, entity_ref.id()))
        }))
    }

    /// Marks a building or unit dead; the next `resolve_deaths` pass destroys
    /// it (Java `Building.kill()`/`Unit.kill()`).
    pub(super) fn kill_building(&mut self, entity: Entity) {
        if let Some(mut health) = self.ecs.0.get_mut::<crate::entities::comp::Health>(entity) {
            health.dead = true;
            health.health = 0.0;
        }
    }
}
