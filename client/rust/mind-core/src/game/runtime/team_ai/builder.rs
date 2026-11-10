// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BaseBuilderAI` base-building half: one builder update per active team and
//! the `prebuildAi` defenders.
//! Upstream: core/src/mindustry/ai/BaseBuilderAI.java,
//! core/src/mindustry/ai/types/PrebuildAI.java

use super::super::*;
use super::{enemy_core_tiles, team_core_tiles};
use crate::ai::{BaseBuildInput, Cost};
use crate::content::{ContentRegistry, ItemId, UnitTypeId};
use crate::ecs::{BuildingComp, TeamId};
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Pos, TeamComp};
use crate::game::teams::Teams;
use crate::world::TilePos;
use crate::world::block::BlockTable;
use crate::world::limits::{BlockCounter, BuildRules};
use crate::world::modules::ItemModule;
use bevy_ecs::entity::Entity;

/// Runs one `BaseBuilderAI.update` for `team` and enqueues the queued
/// `BlockPlan`s as [`AiPlaceRequest`]s for the host pump (`Logic.update`'s
/// `rules.buildAi` half; PAR-0139 round 2).
pub(in crate::game::runtime) fn run_team_base_builder(
    world: &mut World,
    runtime: &mut CampaignRuntime,
    team: TeamId,
    rule: &crate::game::rules::TeamRule,
    rules: &Rules,
) {
    let Some(catalog) = world.get_resource::<BaseCatalog>().cloned() else {
        return;
    };
    let core_tiles = team_core_tiles(world, &runtime.session.teams, team.0);
    if core_tiles.is_empty() {
        return;
    }
    let enemy_cores = enemy_core_tiles(world, &runtime.session.teams, team.0);
    let mut spawn_tiles: Vec<TilePos> = world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .map(|unit_runtime| unit_runtime.wave_spawner.spawns.clone())
        .unwrap_or_default();
    if spawn_tiles.is_empty() {
        // `WaveSpawner.eachGroundSpawn`'s attack branch: without spawn-overlay
        // tiles the wave-team cores seed the trace/placement positions.
        spawn_tiles = core_tiles.clone();
    }

    let mut queued: Vec<AiPlaceRequest> = Vec::new();
    let Some(mut unit_runtime) = world.remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
    else {
        return;
    };
    let mut actions = crate::ai::BaseBuildActions::default();
    let mut core_unit: Option<(&'static str, UnitTypeId)> = None;
    {
        let table = world.get_resource::<BlockTable>();
        let build_rules = world.get_resource::<BuildRules>();
        let counter = world.get_resource::<BlockCounter>();
        if let (Some(table), Some(build_rules), Some(counter)) = (table, build_rules, counter) {
            let crate::sim::unit_runtime::UnitRuntime {
                grid,
                content,
                pathfinder,
                units,
                ..
            } = &mut unit_runtime;
            let field = pathfinder.get_field(Cost::Ground, team.0, &enemy_cores);
            let CampaignRuntime {
                session, ai_rng, ..
            } = runtime;
            let data = session.teams.get(team);
            // `data.core().block`'s `unitType` (`BaseBuilderAI.java:62`): the
            // core-unit content of the team's first core.
            core_unit = data
                .core()
                .and_then(|core| core_unit_of(world, content, table, core));
            // `data.countType(block.unitType)` (`BaseBuilderAI.java:63`).
            let core_unit_count = core_unit.map_or(0, |(_, type_id)| {
                units
                    .iter()
                    .filter(|&&entity| {
                        world.get_entity(entity).is_ok_and(|entity_ref| {
                            entity_ref
                                .get::<UnitTypeComp>()
                                .is_some_and(|comp| comp.type_id == type_id)
                                && entity_ref
                                    .get::<TeamComp>()
                                    .is_some_and(|comp| comp.team == team.0)
                                && entity_ref.get::<UnitCore>().is_some_and(|core| !core.dead)
                        })
                    })
                    .count() as i32
            });
            if crate::ai::ensure_build_ai(data) {
                log::info!("team ai: base builder enabled for team {}", team.0);
            }
            let mut input = BaseBuildInput {
                content,
                grid,
                table,
                build_rules,
                counter,
                team_rule: rule,
                rules,
                team: team.0,
                cores: &core_tiles,
                spawns: &spawn_tiles,
                enemy_core_field: Some(field),
                enemy_cores: &enemy_cores,
                core_unit_count,
                bases: catalog.0.as_ref(),
                rng: ai_rng,
            };
            actions = crate::ai::update_base_build(data, &mut input);
            while let Some(plan) = data.plans.pop_front() {
                if plan.removed {
                    continue;
                }
                queued.push(AiPlaceRequest {
                    team: team.0,
                    x: plan.x,
                    y: plan.y,
                    block: plan.block,
                    rot: plan.rotation.max(0) as u8,
                });
            }
        }
    }
    // `BaseBuilderAI.update` host side effects (`BaseBuilderAI.java:53-70`),
    // applied before the unit runtime goes back into the world.
    if actions.fill_core_items {
        fill_first_core_items(world, runtime, &unit_runtime.content, team.0);
    }
    if actions.spawn_core_unit
        && let Some((name, _)) = core_unit
    {
        spawn_core_unit_at_random_core(&mut unit_runtime, world, runtime, team.0, name);
    }
    world.insert_non_send(unit_runtime);
    if queued.is_empty() {
        return;
    }
    if let Some(queues) = world.get_resource::<CampaignAiQueues>()
        && let Ok(mut queue) = queues.0.lock()
    {
        queue.extend(queued.iter().copied());
    }
    log::info!(
        "team ai: team {} base builder queued {} placement(s)",
        team.0,
        queued.len()
    );
}

/// `data.core().block`'s unit type (`CoreBlock.unitType`): the unit name from
/// the block table and its content id (`BaseBuilderAI.java:62`,
/// `RtsAI.java:90`, `Logic.java:576`).
fn core_unit_of(
    world: &World,
    content: &ContentRegistry,
    table: &BlockTable,
    core: Entity,
) -> Option<(&'static str, UnitTypeId)> {
    let block = world.get::<BuildingComp>(core)?.block;
    let name = table.get(block)?.def.core_unit?;
    let id = content.unit_by_name(name)?.id;
    Some((name, id))
}

/// The team's first core's unit type (`data.core()`), resolved live.
pub(super) fn core_unit_type(
    world: &World,
    teams: &Teams,
    team: u8,
) -> Option<(&'static str, UnitTypeId)> {
    let core = teams.get_or_null(TeamId(team))?.core()?;
    let unit_runtime = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>()?;
    let table = world.get_resource::<BlockTable>()?;
    core_unit_of(world, &unit_runtime.content, table, core)
}

/// Live count of `type_id` units owned by `team` (`TeamData.countType`).
pub(in crate::game::runtime) fn live_type_count(
    world: &World,
    team: u8,
    type_id: UnitTypeId,
) -> i32 {
    let Some(unit_runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() else {
        return 0;
    };
    unit_runtime
        .units
        .iter()
        .filter(|&&entity| {
            world.get_entity(entity).is_ok_and(|entity_ref| {
                entity_ref
                    .get::<UnitTypeComp>()
                    .is_some_and(|comp| comp.type_id == type_id)
                    && entity_ref
                        .get::<TeamComp>()
                        .is_some_and(|comp| comp.team == team)
                    && entity_ref.get::<UnitCore>().is_some_and(|core| !core.dead)
            })
        })
        .count() as i32
}

/// `BaseBuilderAI.java:53-58`: every tick, fill the team's first core with
/// every item to `getMaximumAccepted` (unlike `Logic.fillItems` there is no
/// planet/hidden filter upstream).
fn fill_first_core_items(
    world: &mut World,
    runtime: &CampaignRuntime,
    content: &ContentRegistry,
    team: u8,
) {
    let Some(core) = runtime
        .session
        .teams
        .get_or_null(TeamId(team))
        .and_then(|data| data.core())
    else {
        return;
    };
    let Some(block) = world.get::<BuildingComp>(core).map(|comp| comp.block) else {
        return;
    };
    let amounts: Vec<(ItemId, i32)> = {
        let Some(table) = world.get_resource::<BlockTable>() else {
            return;
        };
        let Some(instance) = table.instance(block) else {
            return;
        };
        content
            .items()
            .iter()
            .map(|item| {
                (
                    item.id,
                    instance.behavior.get_maximum_accepted(world, core, item.id),
                )
            })
            .collect()
    };
    if let Some(mut module) = world.get_mut::<ItemModule>(core) {
        for (item, amount) in amounts {
            module.set(item, amount);
        }
    }
}

/// `BaseBuilderAI.java:61-70` / `RtsAI.java:90-98`: spawn the core block's unit
/// type at a random live core (`unit.set(data.cores.random())`). Needs the
/// already-removed [`crate::sim::unit_runtime::UnitRuntime`] to track the unit.
fn spawn_core_unit_at_random_core(
    unit_runtime: &mut crate::sim::unit_runtime::UnitRuntime,
    world: &mut World,
    runtime: &mut CampaignRuntime,
    team: u8,
    name: &str,
) -> Option<Entity> {
    let cores = runtime
        .session
        .teams
        .get_or_null(TeamId(team))
        .map_or(Vec::new(), |data| data.cores.clone());
    if cores.is_empty() {
        return None;
    }
    let index = runtime.ai_rng.next_int_bound(cores.len() as i32) as usize;
    let core = cores[index.min(cores.len().saturating_sub(1))];
    let pos = world.get::<Pos>(core).map(|pos| (pos.x, pos.y))?;
    unit_runtime.spawn(world, name, team, pos.0, pos.1, 0.0)
}

/// [`spawn_core_unit_at_random_core`] taking the unit runtime out of the world
/// (the `team_ai_system` path, where it is installed).
pub(super) fn spawn_random_core_unit(
    world: &mut World,
    runtime: &mut CampaignRuntime,
    team: u8,
    name: &str,
) -> Option<Entity> {
    let mut unit_runtime = world.remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()?;
    let spawned = spawn_core_unit_at_random_core(&mut unit_runtime, world, runtime, team, name);
    world.insert_non_send(unit_runtime);
    spawned
}

/// Spawns a unit by content name at a fixed position through the live
/// [`crate::sim::unit_runtime::UnitRuntime`].
fn spawn_unit_at(world: &mut World, team: u8, name: &str, x: f32, y: f32) -> Option<Entity> {
    let mut unit_runtime = world.remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()?;
    let spawned = unit_runtime.spawn(world, name, team, x, y, 0.0);
    world.insert_non_send(unit_runtime);
    spawned
}

/// `Logic.java:574-584` (`prebuildAi`): one core-unit defender per core,
/// flagged with the core's packed tile position (`unit.flag = core.pos()`) so
/// it is not respawned.
pub(super) fn run_prebuild_ai_defenders(
    world: &mut World,
    runtime: &mut CampaignRuntime,
    team: u8,
) {
    let mut pending: Vec<(Entity, &'static str)> = Vec::new();
    {
        let Some(data) = runtime.session.teams.get_or_null(TeamId(team)) else {
            return;
        };
        for &core in &data.cores {
            let Some(comp) = world.get::<BuildingComp>(core) else {
                continue;
            };
            let packed = comp.pos.pack() as f64;
            let Some(unit_runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            else {
                return;
            };
            let Some(table) = world.get_resource::<BlockTable>() else {
                return;
            };
            let Some((name, type_id)) = core_unit_of(world, &unit_runtime.content, table, core)
            else {
                continue;
            };
            // `units.contains(u -> u.flag == core.pos())` (`Logic.java:577`).
            let present = world.iter_entities().any(|entity_ref| {
                entity_ref
                    .get::<UnitTypeComp>()
                    .is_some_and(|comp| comp.type_id == type_id)
                    && entity_ref
                        .get::<TeamComp>()
                        .is_some_and(|comp| comp.team == team)
                    && entity_ref
                        .get::<UnitCore>()
                        .is_some_and(|unit| !unit.dead && unit.flag == packed)
            });
            if !present {
                pending.push((core, name));
            }
        }
    }
    for (core, name) in pending {
        let Some(pos) = world.get::<Pos>(core).map(|pos| (pos.x, pos.y)) else {
            continue;
        };
        if let Some(unit) = spawn_unit_at(world, team, name, pos.0, pos.1) {
            // `unit.flag = core.pos()` (`Logic.java:579`).
            let packed = world
                .get::<BuildingComp>(core)
                .map_or(0.0, |comp| comp.pos.pack() as f64);
            if let Some(mut unit_core) = world.get_mut::<UnitCore>(unit) {
                unit_core.flag = packed;
            }
        }
    }
}
