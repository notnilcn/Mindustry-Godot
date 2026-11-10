// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::WeatherAndAi` team half: the per-active-team `RtsAI` update, the
//! live unit/building rows it reads and the AI placement queues.
//! Upstream: core/src/mindustry/ai/RtsAI.java, core/src/mindustry/ai/BaseBuilderAI.java

use super::*;
use crate::ai::BaseRegistry;
use crate::ai::controller::{AiKind, ControllerSlot};
use crate::ai::rts_ai::{self, EnemyStat, MAX_TARGETS_CHECKED, RtsBuilding, RtsLiveUnit};
use crate::content::BlockId;
use crate::content::registries::blocks::BlockFlag;
use crate::ecs::{BuildingComp, EntitySeq, TeamId};
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::game::State;
use crate::game::teams::Teams;
use crate::world::TilePos;
use crate::world::block::BlockTable;
use bevy_ecs::entity::Entity;
use builder::{
    core_unit_type, live_type_count, run_prebuild_ai_defenders, run_team_base_builder,
    spawn_random_core_unit,
};
use stats::enemy_stats_for;
use std::sync::{Arc, Mutex};
pub(super) mod builder;
pub(super) mod stats;

/// The loaded base-part catalogue (`Vars.bases`), supplied by the host from
/// `assets/baseparts/*.msch`. The base-builder half of the team AI is skipped
/// while this resource is absent (headless P0 worlds never pay for it).
#[derive(Resource, Clone, Default)]
pub struct BaseCatalog(pub Arc<BaseRegistry>);

/// One queued AI placement (`team_ai_system` -> host pump).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiPlaceRequest {
    /// Owning team.
    pub team: u8,
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Block content id.
    pub block: BlockId,
    /// Rotation.
    pub rot: u8,
}

/// Shared queue handle for [`AiPlaceRequest`]s (`CampaignCoreQueues` sibling);
/// the host pump drains it at tick boundaries through `Sim::place_for_team`.
#[derive(Resource, Clone, Default)]
pub struct CampaignAiQueues(pub Arc<Mutex<Vec<AiPlaceRequest>>>);

/// Installs the AI placement queue resource (idempotent).
pub fn install_campaign_ai_queues(world: &mut World) -> Arc<Mutex<Vec<AiPlaceRequest>>> {
    if let Some(existing) = world.get_resource::<CampaignAiQueues>() {
        return existing.0.clone();
    }
    let queues = Arc::new(Mutex::new(Vec::new()));
    world.insert_resource(CampaignAiQueues(queues.clone()));
    queues
}

/// `RtsAI.flags`: candidate target priority order.
const TARGET_FLAGS: [BlockFlag; 5] = [
    BlockFlag::Generator,
    BlockFlag::Factory,
    BlockFlag::Core,
    BlockFlag::Battery,
    BlockFlag::Drill,
];

/// One live unit row for the per-team AI snapshot.
#[derive(Debug, Clone, Copy)]
struct TeamAiUnitRow {
    entity: Entity,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
    hit_size: f32,
    health: f32,
    dps: f32,
    target_air: bool,
    target_ground: bool,
    flying: bool,
    naval: bool,
    idle: bool,
}

impl TeamAiUnitRow {
    /// The [`RtsLiveUnit`] projection (`unit.flag` is 0: the live port has no
    /// flag field yet).
    fn to_live(self) -> RtsLiveUnit {
        RtsLiveUnit {
            entity: self.entity,
            x: self.x,
            y: self.y,
            flag: 0,
            idle: self.idle,
            hit_size: self.hit_size,
            health: self.health,
            dps: self.dps,
            target_air: self.target_air,
            target_ground: self.target_ground,
            naval: self.naval,
            flying: self.flying,
        }
    }
}

/// One live building row for the per-team AI snapshot.
#[derive(Debug, Clone, Copy)]
pub(super) struct TeamAiBuildingRow {
    pub(super) entity: Entity,
    pub(super) seq: u64,
    pub(super) team: u8,
    pub(super) pos: TilePos,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) in_core: bool,
    pub(super) flag_rank: Option<usize>,
    pub(super) damaged: bool,
}

/// `TickSet::WeatherAndAi`: the per-active-team `RtsAI` update
/// (`Logic.update`: `for(TeamData data : state.teams.getActive())`, PAR-0139).
///
/// Round 1 wires the RTS half (`TeamRule.rtsAi`): squads are grouped from the
/// live idle units, weighted targets are picked from the live enemy buildings
/// (flag priority: generator/factory/core/battery/drill) and every order lands
/// on the member's live move target (`ControllerSlot.target`; upstream issues
/// `CommandAI.commandTarget`/`commandPosition`). The `BaseBuilderAI`/
/// `prebuildAi` halves stay unwired (no live `BaseRegistry` load / core
/// `unitType` content yet) and are tracked in the PAR-0139 fix plan.
pub fn team_ai_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if runtime.session.phase != State::Playing || runtime.is_client {
        world.insert_resource(runtime);
        return;
    }
    let rules = runtime.session.rules.clone();
    // `Logic.java:552-561` (before the buildAi/rtsAi block): every active team
    // with `fillItems` refills `getMaximumAccepted` of each on-planet, visible
    // item into its first core. Not PvP-gated; skipped in the editor.
    if !rules.editor {
        let mut fill_plan: Vec<(Entity, Vec<(crate::content::ItemId, i32)>)> = Vec::new();
        if let Some(unit_runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() {
            let content = &unit_runtime.content;
            let table = world.get_resource::<crate::world::BlockTable>();
            let planet = runtime.session.sector.map(|(planet, _)| planet);
            fill_plan = runtime
                .session
                .teams
                .active
                .iter()
                .filter_map(|team| {
                    // `Logic.java:554` `if(rules.fillItems && data.cores.size > 0)`:
                    // only teams whose rule sets `fillItems` are filled — the
                    // campaign sets it on the wave team alone
                    // (`apply_world_load_campaign_rules`), so the player core is
                    // never refilled (its `getMaximumAccepted` is `i32::MAX / 2`
                    // under `coreIncinerates`).
                    if !rules.team_rule(team.0).fill_items {
                        return None;
                    }
                    let data = runtime.session.teams.get_or_null(*team)?;
                    let core = data.core()?;
                    let building = world.get::<crate::entities::comp::Building>(core)?;
                    let instance = table.and_then(|table| table.instance(building.block))?;
                    // `UnlockableContent.isOnPlanet` (`UnlockableContent.java:132-134`).
                    let amounts: Vec<(crate::content::ItemId, i32)> = content
                        .items()
                        .iter()
                        .filter(|item| {
                            !item.is_hidden()
                                && (planet.is_none()
                                    || item.unlock.shown_planets.is_empty()
                                    || planet.is_some_and(|planet| {
                                        item.unlock.shown_planets.contains(&planet)
                                    }))
                        })
                        .map(|item| {
                            (
                                item.id,
                                instance.behavior.get_maximum_accepted(world, core, item.id),
                            )
                        })
                        .collect();
                    Some((core, amounts))
                })
                .collect();
        }
        for (core, amounts) in fill_plan {
            if let Some(mut module) = world.get_mut::<crate::world::modules::ItemModule>(core) {
                for (item, amount) in amounts {
                    module.set(item, amount);
                }
            }
        }
    }
    // `if(rules.buildAi && !state.rules.pvp)` and the RTS guard share the
    // PvP exclusion upstream; the whole per-team block is host-only.
    if !rules.pvp {
        let active = runtime.session.teams.active.clone();
        let (units, buildings) = collect_team_ai_rows(world);
        for team in active {
            let rule = rules.team_rule(team.0).clone();
            if rule.rts_ai {
                rts_ai::ensure_rts_ai(runtime.session.teams.get(team));
            }
            // `RtsAI.update` (`RtsAI.java:72-77`): the 120-tick `timeUpdate`
            // gate runs `assignSquads()` and `checkBuilding()`. Java calls
            // `update` for every active rtsAi team every tick, so the gate is
            // ticked here; the squad half below only runs when idle units exist.
            let rts_fired = rule.rts_ai
                && runtime
                    .session
                    .teams
                    .get(team)
                    .rts_ai
                    .as_mut()
                    .is_some_and(|rts| rts.update_timer(rts_ai::UPDATE_INTERVAL));
            if rts_fired {
                // `checkBuilding` (`RtsAI.java:86-99`): on its 420-tick
                // `timerSpawn` the team's first core block spawns one core unit
                // up to `data.cores.size` units.
                if let Some((name, type_id)) = core_unit_type(world, &runtime.session.teams, team.0)
                {
                    let core_units = live_type_count(world, team.0, type_id);
                    let cores = runtime
                        .session
                        .teams
                        .get_or_null(team)
                        .map_or(0, |data| data.cores.len() as i32);
                    let spawn =
                        runtime
                            .session
                            .teams
                            .get(team)
                            .rts_ai
                            .as_mut()
                            .is_some_and(|rts| {
                                rts.check_building_spawn(rule.ai_core_spawn, core_units, cores)
                            });
                    if spawn {
                        spawn_random_core_unit(world, &mut runtime, team.0, name);
                    }
                }
            }
            let rts_ready = rts_fired
                && units.iter().any(|row| row.team == team.0 && row.idle)
                && runtime
                    .session
                    .teams
                    .get_or_null(team)
                    .is_some_and(|data| data.rts_ai.is_some());
            if rts_ready {
                let team_units: Vec<RtsLiveUnit> = units
                    .iter()
                    .filter(|row| row.team == team.0)
                    .map(|row| row.to_live())
                    .collect();
                let targets = team_ai_targets(&buildings, team.0);
                let damaged = team_ai_damaged(&buildings, team.0);
                let enemy_stats = enemy_stats_for(world, &units, &buildings, team.0);
                let Some(rts) = runtime.session.teams.get(team).rts_ai.as_mut() else {
                    continue;
                };
                let orders = rts.plan_live_orders_now(&rts_ai::RtsLiveInput {
                    min_squad: rule.rts_min_squad,
                    max_squad: rule.rts_max_squad,
                    min_weight: rule.rts_min_weight,
                    unit_cap: i32::MAX,
                    units: &team_units,
                    targets: &targets,
                    damaged: &damaged,
                    enemy_stats: &enemy_stats,
                });
                for order in &orders {
                    for &entity in &order.units {
                        if let Some(mut slot) = world.get_mut::<ControllerSlot>(entity) {
                            slot.target = Some(order.target);
                        }
                    }
                    log::info!(
                        "team ai: team {} squad of {} -> tile ({}, {}) ({})",
                        team.0,
                        order.units.len(),
                        order.target.x(),
                        order.target.y(),
                        if order.defend { "defend" } else { "attack" }
                    );
                }
            }
            if rule.build_ai {
                run_team_base_builder(world, &mut runtime, team, &rule, &rules);
            }
            // `Logic.java:574-584`: `prebuildAi` defenders, one core unit per
            // core, flagged with the core's packed position.
            if rule.prebuild_ai && !rules.editor {
                run_prebuild_ai_defenders(world, &mut runtime, team.0);
            }
        }
    }
    world.insert_resource(runtime);
}

/// Tile positions of `team`'s live cores (`TeamData.cores` -> `BuildingComp`).
fn team_core_tiles(world: &World, teams: &Teams, team: u8) -> Vec<TilePos> {
    teams
        .get_or_null(TeamId(team))
        .map(|data| {
            data.cores
                .iter()
                .filter_map(|entity| world.get::<BuildingComp>(*entity).map(|comp| comp.pos))
                .collect()
        })
        .unwrap_or_default()
}

/// Tile positions of every other active team's cores (`Pathfinder.fieldCore`
/// target set for the base builder's enemy path).
fn enemy_core_tiles(world: &World, teams: &Teams, team: u8) -> Vec<TilePos> {
    let mut out = Vec::new();
    for other in &teams.active {
        if other.0 == team {
            continue;
        }
        out.extend(team_core_tiles(world, teams, other.0));
    }
    out
}

/// Collects the live unit/building rows the per-team AI reads this tick.
fn collect_team_ai_rows(world: &World) -> (Vec<TeamAiUnitRow>, Vec<TeamAiBuildingRow>) {
    let mut units = Vec::new();
    if let Some(unit_runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() {
        for &entity in &unit_runtime.units {
            let Ok(entity_ref) = world.get_entity(entity) else {
                continue;
            };
            let Some(team) = entity_ref.get::<TeamComp>() else {
                continue;
            };
            let Some(pos) = entity_ref.get::<Pos>() else {
                continue;
            };
            let Some(core) = entity_ref.get::<UnitCore>() else {
                continue;
            };
            if core.dead {
                continue;
            }
            let Some(def) = entity_ref
                .get::<UnitTypeComp>()
                .and_then(|comp| unit_runtime.content.unit(comp.type_id))
            else {
                continue;
            };
            // `CommandAI` idle: no order and not attacking (the live controller
            // stores the order in `ControllerSlot.target`).
            let idle = entity_ref
                .get::<ControllerSlot>()
                .is_some_and(|slot| slot.kind != AiKind::Player && slot.target.is_none());
            let health = entity_ref
                .get::<Health>()
                .map_or(def.health, |health| health.health);
            units.push(TeamAiUnitRow {
                entity,
                team: team.team,
                x: pos.x,
                y: pos.y,
                range: def.range.max(def.max_range),
                hit_size: def.hit_size,
                health,
                dps: def.dps_estimate,
                target_air: def.target_air,
                target_ground: def.target_ground,
                flying: def.flying,
                naval: def.naval,
                idle,
            });
        }
    }

    let mut buildings = Vec::new();
    let table = world.get_resource::<BlockTable>();
    for entity_ref in world.iter_entities() {
        let Some(comp) = entity_ref.get::<BuildingComp>() else {
            continue;
        };
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let Some(def) = table
            .and_then(|table| table.get(comp.block))
            .map(|instance| instance.def.clone())
        else {
            continue;
        };
        let seq = entity_ref.get::<EntitySeq>().map_or(u64::MAX, |seq| seq.0);
        // Damaged snapshot: the live port has no `BuildDamageEvent` listener
        // yet, so `damagedSet` is approximated by live health below max.
        let damaged = entity_ref
            .get::<Health>()
            .is_some_and(|health| health.health > 0.0 && health.health < health.max_health);
        buildings.push(TeamAiBuildingRow {
            entity: entity_ref.id(),
            seq,
            team: comp.team.0,
            pos: comp.pos,
            x: pos.x,
            y: pos.y,
            in_core: def.flags.contains(&BlockFlag::Core),
            flag_rank: TARGET_FLAGS
                .iter()
                .position(|flag| def.flags.contains(flag)),
            damaged,
        });
    }
    (units, buildings)
}

/// `findTarget` candidate pool: enemy buildings carrying one of [`TARGET_FLAGS`],
/// in flag-priority then spawn order, truncated to [`MAX_TARGETS_CHECKED`].
///
/// Upstream shuffles the pool before truncating (`Mathf.random`); the port keeps
/// stable order so a scenario replays deterministically.
fn team_ai_targets(buildings: &[TeamAiBuildingRow], team: u8) -> Vec<RtsBuilding> {
    let mut rows: Vec<(usize, u64, RtsBuilding)> = buildings
        .iter()
        .filter(|row| row.team != team)
        .filter_map(|row| {
            Some((
                row.flag_rank?,
                row.seq,
                RtsBuilding {
                    entity: row.entity,
                    pos: row.pos,
                    x: row.x,
                    y: row.y,
                    in_core: row.in_core,
                },
            ))
        })
        .collect();
    rows.sort_by_key(|(rank, seq, _)| (*rank, *seq));
    rows.truncate(MAX_TARGETS_CHECKED);
    rows.into_iter().map(|(_, _, row)| row).collect()
}

/// `damagedSet` snapshot: the team's damaged buildings.
fn team_ai_damaged(buildings: &[TeamAiBuildingRow], team: u8) -> Vec<RtsBuilding> {
    buildings
        .iter()
        .filter(|row| row.team == team && row.damaged)
        .map(|row| RtsBuilding {
            entity: row.entity,
            pos: row.pos,
            x: row.x,
            y: row.y,
            in_core: row.in_core,
        })
        .collect()
}

/// `estimateStats` enemy unit rows (`Units.nearbyEnemies` excludes
/// `Team.derelict`); turret rows are appended by [`turret_enemy_stats`].
fn team_ai_enemy_stats(units: &[TeamAiUnitRow], team: u8) -> Vec<EnemyStat> {
    units
        .iter()
        .filter(|row| row.team != team && row.team != 0)
        .map(|row| EnemyStat {
            x: row.x,
            y: row.y,
            health: row.health,
            dps: row.dps,
            range: row.range,
            target_air: row.target_air,
            target_ground: row.target_ground,
            // The enemy's own mobility state (used by the defense aggressor
            // filter; `estimateStats` ignores it).
            air: row.flying,
            is_turret: false,
        })
        .collect()
}
