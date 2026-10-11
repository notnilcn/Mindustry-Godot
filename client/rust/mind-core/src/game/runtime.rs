// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live campaign runtime: the `CampaignRuntime` ECS resource plus the
//! `TickSet::Campaign` / `TickSet::Objectives` / `TickSet::GameStateCheck`
//! systems.
//!
//! Ported from the campaign half of `core/src/mindustry/core/Logic.java`
//! (`update`/`runWave`/`checkGameState`), `core/src/mindustry/game/Universe.java`
//! (`update` clock, `updateGlobal`) and `mindustry/game/MapObjectives.java`
//! (`state.objectives.update()`). The resource hosts the live [`PlaySession`]
//! and [`Campaign`] while a sector is loaded; the systems are no-ops when it is
//! absent, so P0 `Sim` checksums and goldens are unchanged.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use std::sync::{Arc, Mutex};

use super::map_objectives::{ObjectiveEnv, ObjectiveRunParams};
use super::play::{PlayEvent, PlaySession, apply_sector_loss, check_game_state, run_wave_campaign};
use super::rules::Rules;
use super::stats::GameStats;
use super::teams::Teams;
use super::universe::Campaign;
use crate::ai::controller::{AiKind, ControllerSlot};
use crate::ai::rts_ai::{self, EnemyStat, MAX_TARGETS_CHECKED, RtsBuilding, RtsLiveUnit};
use crate::ai::{BaseBuildInput, BaseRegistry, Cost};
use crate::content::registries::blocks::BlockFlag;
use crate::content::{BlockId, BlockKind, ContentRegistry, ItemId};
use crate::ecs::{BuildingComp, EntitySeq, TeamId};
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::game::State;
use crate::math::ArcRand;
use crate::render::g3d::grid::PlanetGrid;
use crate::sim::runtime::ContentIndex;
use crate::world::TilePos;
use crate::world::block::BlockTable;
use crate::world::blocks::storage::core_block::CoreBuild;
use crate::world::limits::{BlockCounter, BuildRules};
use crate::world::modules::ItemModule;

/// Sim ticks per second (fixed rate).
const TICKS_PER_SECOND: f32 = 60.0;
/// `Universe.update` global-position refresh period in ticks (one second).
const GLOBAL_UPDATE_TICKS: u64 = 60;

/// Result of syncing the session against the live ECS world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionSync {
    /// Player/default-team cores found in the world.
    pub player_cores: usize,
    /// Wave-team cores found in the world.
    pub wave_cores: usize,
    /// Enemy spawn-point count carried into `session.spawn_count`.
    pub spawn_count: i32,
}

/// Shared campaign hook state: the live session snapshot plus the deferred
/// `SectorInfo.handleCoreItem`/`handleProduction` deltas queued by the core and
/// miner paths (consumed by [`campaign_tick_system`]).
#[derive(Debug, Default)]
pub struct CampaignHookQueues {
    /// `state.isCampaign()` snapshot (refreshed by the campaign tick).
    pub is_campaign: bool,
    /// `state.rules.defaultTeam` snapshot.
    pub default_team: u8,
    /// Pending `handleCoreItem` deltas `(item, amount)`.
    pub core_deltas: Vec<(ItemId, i32)>,
    /// Pending `handleProduction` deltas `(item, amount)`.
    pub production_deltas: Vec<(ItemId, i32)>,
}

/// `CoreCampaignHooks` impl for a live campaign session: queues the deltas the
/// campaign tick folds into the played sector's `SectorInfo` (plan 12 half of
/// `CoreBlock.handleItem` -> `SectorInfo.handleCoreItem`).
#[derive(Debug, Clone)]
pub struct CampaignCoreHooks {
    queues: Arc<Mutex<CampaignHookQueues>>,
}

impl crate::world::blocks::storage::CoreCampaignHooks for CampaignCoreHooks {
    fn is_campaign(&self) -> bool {
        self.queues
            .lock()
            .map(|queues| queues.is_campaign)
            .unwrap_or(false)
    }

    fn default_team(&self) -> u8 {
        self.queues
            .lock()
            .map(|queues| queues.default_team)
            .unwrap_or(0)
    }

    fn handle_core_item(&self, item: ItemId, amount: i32) {
        if let Ok(mut queues) = self.queues.lock() {
            queues.core_deltas.push((item, amount));
        }
    }
}

/// The shared queue handle installed beside the campaign core hooks. The
/// campaign tick, the core acceptance path and the miner all reach the same
/// `Arc` through this resource / the hooks impl.
#[derive(Resource, Clone)]
pub struct CampaignCoreQueues(pub Arc<Mutex<CampaignHookQueues>>);

/// Installs the campaign core hooks + queue handle (idempotent: the first
/// install owns the shared handle; later calls keep it).
pub fn install_campaign_core_hooks(world: &mut World) -> Arc<Mutex<CampaignHookQueues>> {
    if let Some(existing) = world.get_resource::<CampaignCoreQueues>() {
        return existing.0.clone();
    }
    let queues = Arc::new(Mutex::new(CampaignHookQueues::default()));
    world.insert_resource(crate::world::blocks::storage::CoreHooks(Arc::new(
        CampaignCoreHooks {
            queues: queues.clone(),
        },
    )));
    world.insert_resource(CampaignCoreQueues(queues.clone()));
    queues
}

/// Clears the shared session snapshot and any pending deltas (menu / runtime
/// removal): the hooks then answer `is_campaign: false` and drop new deltas.
pub fn reset_campaign_core_hooks(world: &mut World) {
    if let Some(queues) = world.get_resource::<CampaignCoreQueues>()
        && let Ok(mut state) = queues.0.lock()
    {
        state.is_campaign = false;
        state.core_deltas.clear();
        state.production_deltas.clear();
    }
}

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

/// The campaign match runtime installed into the sim ECS world while a sector
/// is loaded.
#[derive(Resource)]
pub struct CampaignRuntime {
    /// Live match/session state (`GameState` plan-12 half).
    pub session: PlaySession,
    /// Live campaign container (planets/sectors/stats).
    pub campaign: Campaign,
    /// Ordered events produced by the tick systems for the host to consume.
    pub events: Vec<PlayEvent>,
    /// Completed fixed ticks since the sector loaded.
    pub ticks: u64,
    /// The campaign clock reached a turn boundary (`Universe.update`).
    pub turn_due: bool,
    /// Running as a network client (`net.client()`): server-only systems skip.
    pub is_client: bool,
    /// Unlocked content names (map objective `Research`/`Produce` predicates).
    pub unlocked_content: std::collections::HashSet<String>,
    /// Deterministic team-AI randomness (`BaseBuilderAI` placement scatter).
    ///
    /// Upstream draws from the global `Mathf.random`; the live port keeps a
    /// dedicated stream so a scenario replays byte-for-byte (PAR-0139).
    pub ai_rng: ArcRand,
}

impl CampaignRuntime {
    /// Wraps the launched session/campaign.
    pub fn new(session: PlaySession, campaign: Campaign) -> Self {
        Self {
            session,
            campaign,
            events: Vec::new(),
            ticks: 0,
            turn_due: false,
            is_client: false,
            unlocked_content: std::collections::HashSet::new(),
            ai_rng: ArcRand::new(0x5EED_0139),
        }
    }

    /// Replaces the unlocked-content cache (host-side registry projection).
    pub fn set_unlocked_content(&mut self, names: impl IntoIterator<Item = String>) {
        self.unlocked_content = names.into_iter().collect();
    }

    /// Current match rules.
    pub fn rules(&self) -> &Rules {
        &self.session.rules
    }

    /// Drains the queued events (host-side bookkeeping).
    pub fn take_events(&mut self) -> Vec<PlayEvent> {
        std::mem::take(&mut self.events)
    }
}

/// Registers the live world's core buildings into the session team registry and
/// reports the spawn accounting. `map_spawns` is the loaded map's enemy
/// spawn-point count (0 when the map has none).
///
/// This is the GAP-9 bridge: it makes `PlaySession::player_core_count` reflect
/// the sim before `check_game_state` can run.
pub fn sync_session_with_sim(
    session: &mut PlaySession,
    world: &mut World,
    content: &ContentRegistry,
    map_spawns: i32,
    apply_loadout: bool,
) -> SessionSync {
    let mut cores: Vec<(Entity, u8, i32)> = Vec::new();
    for entity in world.iter_entities() {
        let Some(comp) = entity.get::<BuildingComp>() else {
            continue;
        };
        let Some(def) = content.block(comp.block) else {
            continue;
        };
        if def.kind == BlockKind::CoreBlock {
            cores.push((entity.id(), comp.team.0, def.item_capacity));
        }
    }

    // A world load rebuilds the `Teams` registry to register the live cores,
    // but upstream keeps every `TeamData` across `World.loadMap` (`Logic`
    // only recreates teams on reset), so the carried save build plans must
    // survive the rebuild (PAR-0139 round 2: the resumed sector's
    // `apply_team_plans` plans were wiped here before the AI could drain
    // them).
    let carried_plans: Vec<(
        u8,
        std::collections::VecDeque<crate::game::teams::BlockPlan>,
    )> = (0..=255u8)
        .filter_map(|team| {
            session
                .teams
                .get_or_null(TeamId(team))
                .filter(|data| !data.plans.is_empty())
                .map(|data| (team, data.plans.clone()))
        })
        .collect();
    session.teams = Teams::new();
    let default_team = session.default_team();
    let wave_team = session.wave_team();
    // `World.setRules`: player placements carry `state.rules.defaultTeam`
    // (`Team.sharded` = 1 for the campaign); the build guard resource starts at
    // the derelict id, so sync it with the loaded session.
    let env = session.rules.env;
    if let Some(mut rules) = world.get_resource_mut::<crate::world::limits::BuildRules>() {
        rules.default_team = default_team;
        // `Block.isPlaceable` -> `supportsEnv(state.rules.env)`.
        rules.env = env;
    }
    let mut sync = SessionSync::default();
    for (entity, team, _) in &cores {
        session
            .teams
            .register_core(*entity, TeamId(*team), &session.rules);
        if *team == default_team {
            sync.player_cores += 1;
        }
        if *team == wave_team {
            sync.wave_cores += 1;
        }
    }
    for (team, plans) in carried_plans {
        session.teams.get(TeamId(team)).plans = plans;
    }
    sync.spawn_count = map_spawns.max(sync.wave_cores as i32);
    session.spawn_count = sync.spawn_count;
    session.enemies = 0;
    // `Logic.play` starting-items half: with the cores registered, clear the
    // default team's inventory and add the launch loadout (GAP-10) on a fresh
    // launch; a resumed save keeps its own core inventory.
    if apply_loadout {
        super::play::apply_launch_loadout(session, content);
        // Upstream clears each core building's own items and adds the loadout
        // there (`entity.items.clear()` + `items.add`); the live core module is
        // the authoritative store (`Team.items()`/`Sector.items()` while
        // playing), so mirror the loadout into it as well.
        if !session.is_campaign() || !session.allow_launch_loadout || session.add_starting_items {
            for (entity, team, capacity) in &cores {
                if *team != default_team {
                    continue;
                }
                let Some(mut module) = world.get_mut::<ItemModule>(*entity) else {
                    continue;
                };
                module.clear();
                for stack in &session.rules.loadout {
                    if let Some(name) = &stack.item
                        && let Some(item) = content.item_id(name)
                    {
                        module.add(item, stack.amount.max(0), *capacity);
                    }
                }
            }
        }
    }
    sync
}

/// `TickSet::Campaign`: universe clock, global positions, sector lighting and
/// the campaign wave timer (`Universe.update` + `Logic.update`).
pub fn campaign_tick_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if runtime.session.phase == State::Playing && !runtime.is_client {
        runtime.ticks = runtime.ticks.wrapping_add(1);
        let due = runtime.campaign.advance_time(1);
        if due {
            runtime.turn_due = true;
        }
        if runtime.ticks % GLOBAL_UPDATE_TICKS == 0 {
            let seconds = runtime.campaign.universe.seconds(false) as f32;
            runtime.campaign.update_global(seconds);
            apply_sector_lighting(&mut runtime, seconds);
        }

        // `Logic.update` wave timer (`!net.client()`, waves enabled).
        if runtime.session.rules.waves
            && runtime.session.rules.wave_timer
            && !runtime.session.game_over
        {
            runtime.session.wavetime = (runtime.session.wavetime - 1.0 / TICKS_PER_SECOND).max(0.0);
            if runtime.session.wavetime <= 0.0 {
                runtime.events.push(run_wave_campaign(&mut runtime.session));
                // `Logic.runWave` -> `WaveSpawner.spawnEnemies`: the live unit
                // runtime spawns the pre-increment wave in `TickSet::RunWave`.
                if let Some(mut unit_runtime) =
                    world.get_non_send_mut::<crate::sim::unit_runtime::UnitRuntime>()
                {
                    unit_runtime.pending_wave = Some(runtime.session.wave - 1);
                }
                // `WaveEvent` listener: `stats.wavesLasted++`.
                if let Some((planet, _)) = runtime.session.sector
                    && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                {
                    stats.waves_lasted += 1;
                }
            }
        }

        // `Teams.unregisterCore` on destroyed cores: the port's break/combat
        // paths despawn the building entity without touching the session's team
        // data (upstream unregisters at `CoreBuild.destroy`), so
        // `player_core_count`/`wave_core_count` would keep counting dead cores
        // and the loss/attack-win checks never fire. A liveness sweep keeps the
        // counts honest (PAR-0084).
        let rules = runtime.session.rules.clone();
        for team_raw in [rules.default_team, rules.wave_team] {
            let team = TeamId(team_raw);
            let dead: Vec<Entity> = runtime
                .session
                .teams
                .get_or_null(team)
                .map(|data| {
                    data.cores
                        .iter()
                        .copied()
                        .filter(|entity| world.get_entity(*entity).is_err())
                        .collect()
                })
                .unwrap_or_default();
            for entity in dead {
                runtime.session.teams.unregister_core(entity, team, &rules);
            }
        }

        // `SectorInfo.handleCoreItem`/`handleProduction` deltas queued by the
        // core/miner paths land in the played sector's info, then
        // `SectorInfo.update` refreshes the statistic windows (Logic.java:538).
        if let Some(queues) = world.get_resource::<CampaignCoreQueues>().cloned()
            && let Ok(mut state) = queues.0.lock()
        {
            state.is_campaign = runtime.session.is_campaign();
            state.default_team = runtime.session.rules.default_team;
            let core_deltas = std::mem::take(&mut state.core_deltas);
            let production_deltas = std::mem::take(&mut state.production_deltas);
            drop(state);
            if let Some((planet, sector_id)) = runtime.session.sector
                && let Some(sector) = runtime.campaign.sector_mut(planet, sector_id)
            {
                for (item, amount) in core_deltas {
                    sector.info.handle_core_item(item.index(), amount);
                }
                for (item, amount) in production_deltas {
                    sector.info.handle_production(item.index(), amount);
                }
                let index_of = |name: &str| {
                    world
                        .get_resource::<ContentIndex>()
                        .and_then(|index| index.item_ids.get(name).copied())
                        .map(|id| id as usize)
                };
                let name_of = |index: usize| {
                    world
                        .get_resource::<ContentIndex>()
                        .and_then(|content| content.item_names.get(index).cloned())
                };
                let item_count = world
                    .get_resource::<ContentIndex>()
                    .map(|content| content.item_names.len())
                    .unwrap_or(0);
                sector
                    .info
                    .update(1.0 / TICKS_PER_SECOND, &index_of, &name_of, item_count);
            }
        }
    }
    world.insert_resource(runtime);
}

/// `Universe.update` lighting half: day/night ambient from the sector normal and
/// the solar-system direction (`Sector.getLight`).
fn apply_sector_lighting(runtime: &mut CampaignRuntime, seconds: f32) {
    let Some((planet_id, sector_id)) = runtime.session.sector else {
        return;
    };
    let light = {
        let Some(planet) = runtime.campaign.planet(planet_id) else {
            return;
        };
        if !planet.update_lighting {
            return;
        }
        let no_lighting = runtime
            .campaign
            .sector(planet_id, sector_id)
            .is_some_and(|sector| sector.preset_no_lighting);
        if no_lighting {
            return;
        }
        sector_light(runtime, planet_id, sector_id, seconds)
    };
    let Some(planet) = runtime.campaign.planet(planet_id).cloned() else {
        return;
    };
    runtime
        .campaign
        .universe
        .apply_lighting(&planet, light, &mut runtime.session.rules);
}

/// `(normal · lightDir + 1) / 2` for the sector, mirroring `Sector.getLight`.
fn sector_light(
    runtime: &CampaignRuntime,
    planet_id: crate::content::PlanetId,
    sector_id: u16,
    seconds: f32,
) -> f32 {
    let Some(planet) = runtime.campaign.planet(planet_id) else {
        return 0.5;
    };
    let grid = PlanetGrid::create(planet.sector_tiles as usize);
    let Some(tile) = grid.tiles.get(sector_id as usize) else {
        return 0.5;
    };
    let rotation = -planet.get_rotation(seconds).to_radians();
    let (sin, cos) = rotation.sin_cos();
    // Rotate the unit normal around Y (`Vec3.rotate(Vec3.Y, angle)`).
    let normal = [
        tile.v[0] * cos + tile.v[2] * sin,
        tile.v[1],
        -tile.v[0] * sin + tile.v[2] * cos,
    ];
    let solar = planet
        .parent
        .and_then(|parent| runtime.campaign.planet(parent))
        .map(|parent| parent.position)
        .unwrap_or(planet.position);
    let dir = [
        solar.x - planet.position.x,
        solar.y - planet.position.y,
        solar.z - planet.position.z,
    ];
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len < 1e-6 {
        return 0.5;
    }
    let dot = (normal[0] * dir[0] + normal[1] * dir[1] + normal[2] * dir[2]) / len;
    (dot + 1.0) / 2.0
}

/// `TickSet::Objectives`: `state.objectives.update()` over the live session.
pub fn objectives_tick_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if runtime.session.phase != State::Playing || runtime.is_client {
        world.insert_resource(runtime);
        return;
    }
    let params = ObjectiveRunParams {
        default_team: runtime.session.default_team(),
        wave_team: runtime.session.wave_team(),
        timer_multiplier: runtime.session.rules.objective_timer_multiplier,
    };
    let rules_snapshot = runtime.session.rules.clone();
    let completed = {
        let env = RuntimeObjectiveEnv {
            rules: &rules_snapshot,
            teams: &runtime.session.teams,
            unlocked: &runtime.unlocked_content,
            stats: &runtime.session.stats,
            world,
            index: world.get_resource::<ContentIndex>(),
        };
        runtime
            .session
            .objectives
            .update(&env, &params, 1.0 / TICKS_PER_SECOND)
    };
    for index in completed {
        runtime
            .session
            .objectives
            .complete(index, &mut runtime.session.rules);
    }
    world.insert_resource(runtime);
}

/// Live-world objective environment: rules flags/`researched`, the session
/// team registry, the live match stats and the ECS-backed item/block counts.
struct RuntimeObjectiveEnv<'a> {
    rules: &'a Rules,
    teams: &'a Teams,
    unlocked: &'a std::collections::HashSet<String>,
    stats: &'a GameStats,
    world: &'a World,
    index: Option<&'a ContentIndex>,
}

impl ObjectiveEnv for RuntimeObjectiveEnv<'_> {
    fn is_content_unlocked(&self, content: &str) -> bool {
        self.rules.researched.iter().any(|name| name == content) || self.unlocked.contains(content)
    }

    /// `Team.items()`: the sum of the team's live cores (`ItemObjective`).
    fn team_has_item(&self, team: u8, item: &str, amount: i32) -> bool {
        if amount <= 0 {
            return true;
        }
        let Some(&item_id) = self.index.and_then(|index| index.item_ids.get(item)) else {
            return false;
        };
        let item = ItemId::new(item_id);
        let mut count = 0;
        for entity_ref in self.world.iter_entities() {
            if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
                continue;
            }
            if entity_ref.get::<CoreBuild>().is_none() {
                continue;
            }
            if let Some(module) = entity_ref.get::<ItemModule>() {
                count += module.get(item);
            }
        }
        count >= amount
    }

    fn core_item_count(&self, item: &str) -> i32 {
        self.stats.core_item_count.get(item).copied().unwrap_or(0)
    }

    /// `state.stats.placedBlockCount.get(block, 0)` (`BuildCount`): bumped by
    /// the placement-event listener on [`super::super::sim::Sim`].
    fn placed_block_count(&self, block: &str) -> i32 {
        self.stats
            .placed_block_count
            .get(block)
            .copied()
            .unwrap_or(0)
    }

    fn unit_count(&self, _team: u8, _unit: &str) -> i32 {
        0
    }

    fn enemy_units_destroyed(&self) -> i32 {
        0
    }

    fn objective_flag(&self, flag: &str) -> bool {
        self.rules.objective_flags.contains(flag)
    }

    fn core_count(&self, team: u8) -> usize {
        self.teams
            .get_or_null(TeamId(team))
            .map_or(0, |data| data.cores.len())
    }

    fn block_at(&self, _x: i32, _y: i32) -> Option<(&str, u8)> {
        None
    }

    fn headless(&self) -> bool {
        false
    }

    fn command_mode_satisfied(&self) -> bool {
        false
    }
}

/// `TickSet::GameStateCheck`: `Logic.checkGameState` + campaign loss
/// bookkeeping.
pub fn game_state_tick_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if runtime.session.phase != State::Playing || runtime.is_client {
        world.insert_resource(runtime);
        return;
    }
    let events = check_game_state(&mut runtime.session, &mut runtime.campaign);
    for event in events {
        if matches!(event, PlayEvent::GameOver { .. }) && runtime.session.is_campaign() {
            let winner = match event {
                PlayEvent::GameOver { winner } => winner,
                _ => 0,
            };
            runtime.events.extend(apply_sector_loss(
                &mut runtime.session,
                &mut runtime.campaign,
                winner,
            ));
        } else {
            runtime.events.push(event);
        }
    }
    world.insert_resource(runtime);
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
struct TeamAiBuildingRow {
    entity: Entity,
    seq: u64,
    team: u8,
    pos: TilePos,
    x: f32,
    y: f32,
    in_core: bool,
    flag_rank: Option<usize>,
    damaged: bool,
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
            let rts_ready = rule.rts_ai
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
                let enemy_stats = team_ai_enemy_stats(&units, team.0);
                let Some(rts) = runtime.session.teams.get(team).rts_ai.as_mut() else {
                    continue;
                };
                let orders = rts.plan_live_orders(&rts_ai::RtsLiveInput {
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
        }
    }
    world.insert_resource(runtime);
}

/// Runs one `BaseBuilderAI.update` for `team` and enqueues the queued
/// `BlockPlan`s as [`AiPlaceRequest`]s for the host pump (`Logic.update`'s
/// `rules.buildAi` half; PAR-0139 round 2).
fn run_team_base_builder(
    world: &mut World,
    runtime: &mut CampaignRuntime,
    team: TeamId,
    rule: &super::rules::TeamRule,
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
    {
        let table = world.get_resource::<BlockTable>();
        let build_rules = world.get_resource::<BuildRules>();
        let counter = world.get_resource::<BlockCounter>();
        if let (Some(table), Some(build_rules), Some(counter)) = (table, build_rules, counter) {
            let crate::sim::unit_runtime::UnitRuntime {
                grid,
                content,
                pathfinder,
                ..
            } = &mut unit_runtime;
            let field = pathfinder.get_field(Cost::Ground, team.0, &enemy_cores);
            let CampaignRuntime {
                session, ai_rng, ..
            } = runtime;
            let data = session.teams.get(team);
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
                // `CoreBlock.unitType` content is not on `BlockDef` yet, so the
                // core-unit cap stays at zero (plan note).
                core_unit_count: 0,
                bases: catalog.0.as_ref(),
                rng: ai_rng,
            };
            let _ = crate::ai::update_base_build(data, &mut input);
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

/// `estimateStats` enemy rows: every live enemy unit.
///
/// Upstream also folds enemy turret buildings (health + `estimateDps`) into the
/// score; the live port has no turret DPS estimate yet, so only units
/// contribute (tracked in the fix plan).
fn team_ai_enemy_stats(units: &[TeamAiUnitRow], team: u8) -> Vec<EnemyStat> {
    units
        .iter()
        .filter(|row| row.team != team)
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
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    fn content() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).expect("content")
    }

    fn campaign(registry: &ContentRegistry) -> Campaign {
        Campaign::from_registry(registry, &super::super::planet::EmptyNeighborhood)
    }

    #[test]
    fn systems_are_noop_without_the_resource() {
        let mut world = World::new();
        campaign_tick_system(&mut world);
        objectives_tick_system(&mut world);
        game_state_tick_system(&mut world);
        team_ai_system(&mut world);
        assert!(world.get_resource::<CampaignRuntime>().is_none());
    }

    #[test]
    fn team_ai_orders_idle_wave_units_at_an_enemy_core() {
        use crate::ai::ControllerSlot;
        use crate::sim::unit_runtime::{UnitRuntime, unit_update_system};
        use crate::world::WorldGrid;

        let registry = {
            let mut registry = content();
            registry.init().expect("init");
            registry
        };
        let table = crate::world::block::BlockTable::build_default(&registry).expect("table");
        let mut world = World::new();

        // Live wave-team units near the map corner.
        let grid = WorldGrid::new(32, 32);
        let runtime_content = {
            let mut content = content();
            content.init().expect("init");
            content
        };
        let mut unit_runtime = UnitRuntime::new(runtime_content, &grid, 2);
        let a = unit_runtime
            .spawn(&mut world, "dagger", 2, 8.0, 8.0, 0.0)
            .expect("unit a");
        let b = unit_runtime
            .spawn(&mut world, "dagger", 2, 24.0, 8.0, 0.0)
            .expect("unit b");
        world.insert_non_send(unit_runtime);

        // The player core is the enemy target (BuildingComp mirrors the live
        // `SimHooks::new_building` insert).
        let core_inst = table.get_named("core-shard").expect("core-shard").clone();
        let core = core_inst.spawn(
            &mut world,
            1,
            TilePos::new(20, 20),
            1,
            0,
            registry.items().len(),
            registry.liquids().len(),
        );
        world.entity_mut(core).insert(BuildingComp {
            pos: TilePos::new(20, 20),
            block: registry.block_id("core-shard").expect("core-shard id"),
            team: TeamId(1),
            rot: 0,
        });
        world.insert_resource(table);

        let campaign = campaign(&registry);
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.phase = State::Playing;
        {
            let rule = session.rules.team_rule_mut(2);
            rule.rts_ai = true;
            rule.rts_min_squad = 1;
        }
        world.insert_resource(CampaignRuntime::new(session, campaign));

        team_ai_system(&mut world);

        let slot_a = world.get::<ControllerSlot>(a).expect("slot a").target;
        let slot_b = world.get::<ControllerSlot>(b).expect("slot b").target;
        assert_eq!(
            slot_a,
            Some(TilePos::new(20, 20)),
            "the squad attacks the enemy core ({slot_a:?})"
        );
        assert_eq!(slot_b, slot_a, "both clustered units share the order");

        // The order drives the live movement system.
        let before = world.get::<Pos>(a).expect("pos a").x;
        unit_update_system(&mut world);
        let after = world.get::<Pos>(a).expect("pos a").x;
        assert!(
            after > before,
            "ordered unit moves toward the core ({before} -> {after})"
        );
    }

    #[test]
    fn sync_session_preserves_carried_team_plans() {
        use crate::ecs::BuildingComp;
        use crate::sim::unit_runtime::UnitRuntime;
        use crate::world::WorldGrid;
        use crate::world::limits::{BlockCounter, BuildRules};

        let registry = {
            let mut registry = content();
            registry.init().expect("init");
            registry
        };
        let table = BlockTable::build_default(&registry).expect("table");
        let mut world = World::new();
        let grid = WorldGrid::new(16, 16);
        let runtime_content = {
            let mut content = content();
            content.init().expect("init");
            content
        };
        world.insert_non_send(UnitRuntime::new(runtime_content, &grid, 2));
        let core_inst = table.get_named("core-shard").expect("core-shard").clone();
        let core = core_inst.spawn(
            &mut world,
            7,
            TilePos::new(3, 3),
            1,
            0,
            registry.items().len(),
            registry.liquids().len(),
        );
        world.entity_mut(core).insert(BuildingComp {
            pos: TilePos::new(3, 3),
            block: registry.block_id("core-shard").expect("core id"),
            team: TeamId(1),
            rot: 0,
        });
        world.insert_resource(table);
        world.insert_resource(BuildRules::default());
        world.insert_resource(BlockCounter::new());

        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session
            .teams
            .get(TeamId(2))
            .plans
            .push_back(crate::game::teams::BlockPlan {
                x: 9,
                y: 9,
                rotation: 1,
                block: registry.block_id("copper-wall").expect("wall"),
                config: None,
                removed: false,
            });
        let sync = sync_session_with_sim(&mut session, &mut world, &registry, 0, false);
        assert_eq!(sync.player_cores, 1);
        let plans = &session.teams.get_or_null(TeamId(2)).expect("team 2").plans;
        assert_eq!(plans.len(), 1, "carried plans survive the teams rebuild");
        assert_eq!((plans[0].x, plans[0].y), (9, 9));
    }

    #[test]
    fn team_ai_base_builder_queues_plans_for_the_host() {
        use crate::ecs::BuildingComp;
        use crate::sim::unit_runtime::UnitRuntime;
        use crate::world::WorldGrid;
        use crate::world::limits::{BlockCounter, BuildRules};

        let registry = {
            let mut registry = content();
            registry.init().expect("init");
            registry
        };
        let table = BlockTable::build_default(&registry).expect("table");
        let mut world = World::new();
        let grid = WorldGrid::new(32, 32);
        let runtime_content = {
            let mut content = content();
            content.init().expect("init");
            content
        };
        world.insert_non_send(UnitRuntime::new(runtime_content, &grid, 2));
        let core_inst = table.get_named("core-shard").expect("core-shard").clone();
        let core = core_inst.spawn(
            &mut world,
            3,
            TilePos::new(16, 16),
            2,
            0,
            registry.items().len(),
            registry.liquids().len(),
        );
        world.entity_mut(core).insert(BuildingComp {
            pos: TilePos::new(16, 16),
            block: registry.block_id("core-shard").expect("core id"),
            team: TeamId(2),
            rot: 0,
        });
        world.insert_resource(table);
        world.insert_resource(BuildRules::default());
        world.insert_resource(BlockCounter::new());
        world.insert_resource(BaseCatalog::default());
        let queues = install_campaign_ai_queues(&mut world);

        let campaign = campaign(&registry);
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.phase = State::Playing;
        session.rules.team_rule_mut(2).build_ai = true;
        session.teams.register_core(core, TeamId(2), &session.rules);
        // A pending plan from the base builder (its `data.plans` queue).
        session
            .teams
            .get(TeamId(2))
            .plans
            .push_back(crate::game::teams::BlockPlan {
                x: 5,
                y: 5,
                rotation: 0,
                block: registry.block_id("copper-wall").expect("wall"),
                config: None,
                removed: false,
            });
        world.insert_resource(CampaignRuntime::new(session, campaign));

        team_ai_system(&mut world);

        let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
        let data = runtime
            .session
            .teams
            .get_or_null(TeamId(2))
            .expect("team data");
        assert!(data.build_ai.is_some(), "the base builder is created");
        assert!(data.plans.is_empty(), "queued plans drained");
        let queued = queues.lock().expect("queue").clone();
        assert_eq!(queued.len(), 1, "one placement request queued");
        assert_eq!(queued[0].team, 2);
        assert_eq!((queued[0].x, queued[0].y), (5, 5));
        world.insert_resource(runtime);
    }

    #[test]
    fn wave_timer_reaches_win_wave_and_captures() {
        let registry = content();
        let campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules {
            waves: true,
            win_wave: 1,
            wave_spacing: 1.0,
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.sector = Some((planet, sector_id));
        session.phase = State::Playing;
        session.wavetime = 0.01;
        session.spawn_count = 1;
        let core = Entity::from_raw_u32(1).expect("entity");
        session
            .teams
            .register_core(core, TeamId(session.default_team()), &session.rules);
        let mut world = World::new();
        world.insert_resource(CampaignRuntime::new(session, campaign));

        // One tick fires the due wave; the next game-state pass captures.
        campaign_tick_system(&mut world);
        game_state_tick_system(&mut world);
        let runtime = world
            .remove_resource::<CampaignRuntime>()
            .expect("runtime installed");
        assert!(runtime.session.wave >= 1);
        assert!(
            runtime
                .events
                .iter()
                .any(|event| matches!(event, PlayEvent::SectorCapture { .. })),
            "capture fired: {:?}",
            runtime.events
        );
        assert!(
            runtime
                .campaign
                .sector(planet, sector_id)
                .is_some_and(|sector| sector.info.info.was_captured)
        );
    }

    #[test]
    fn core_and_production_deltas_fold_into_the_sector_info() {
        let registry = content();
        let campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.sector = Some((planet, sector_id));
        session.phase = State::Playing;
        let mut world = World::new();
        install_campaign_core_hooks(&mut world);
        world.insert_resource(CampaignRuntime::new(session, campaign));

        // A core accept queues through the installed hooks; a miner tick queues
        // a production delta directly (Java `CoreBlock.handleItem` /
        // `MinerComp.update` -> `SectorInfo.handleCoreItem`/`handleProduction`).
        let hooks = world
            .get_resource::<crate::world::blocks::storage::CoreHooks>()
            .expect("hooks installed")
            .0
            .clone();
        let copper = registry.item_id("copper").unwrap();
        hooks.handle_core_item(copper, 3);
        if let Some(queues) = world.get_resource::<CampaignCoreQueues>()
            && let Ok(mut state) = queues.0.lock()
        {
            state.production_deltas.push((copper, 5));
        }

        campaign_tick_system(&mut world);
        let runtime = world
            .remove_resource::<CampaignRuntime>()
            .expect("runtime installed");
        let sector = runtime.campaign.sector(planet, sector_id).expect("sector");
        assert_eq!(sector.info.windows.core_deltas[copper.index()], 3);
        assert_eq!(sector.info.windows.production_deltas[copper.index()], 5);
    }

    #[test]
    fn destroyed_cores_unregister_on_the_campaign_tick() {
        let registry = content();
        let campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.sector = Some((planet, sector_id));
        session.phase = State::Playing;
        let mut world = World::new();
        install_campaign_core_hooks(&mut world);
        // A world entity standing in for the core building.
        let core = world.spawn_empty().id();
        session
            .teams
            .register_core(core, TeamId(session.default_team()), &session.rules);
        world.insert_resource(CampaignRuntime::new(session, campaign));

        campaign_tick_system(&mut world);
        assert_eq!(
            world
                .get_resource::<CampaignRuntime>()
                .expect("runtime")
                .session
                .player_core_count(),
            1,
            "a live core stays registered"
        );

        world.despawn(core);
        campaign_tick_system(&mut world);
        let runtime = world
            .remove_resource::<CampaignRuntime>()
            .expect("runtime installed");
        assert_eq!(
            runtime.session.player_core_count(),
            0,
            "a destroyed core unregisters (the loss check can fire)"
        );
    }

    #[test]
    fn sync_registers_cores_and_spawns() {
        use crate::ecs::BuildingComp as Comp;
        use crate::world::TilePos;

        let registry = content();
        let mut session = PlaySession::new(Rules {
            waves: true,
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        let mut world = World::new();
        let core = registry
            .block_id("core-shard")
            .expect("core-shard in base content");
        world.spawn(Comp {
            pos: TilePos::new(8, 8),
            block: core,
            team: TeamId(1),
            rot: 0,
        });
        world.spawn(Comp {
            pos: TilePos::new(40, 40),
            block: core,
            team: TeamId(2),
            rot: 0,
        });
        let sync = sync_session_with_sim(&mut session, &mut world, &registry, 1, true);
        assert_eq!(sync.player_cores, 1);
        assert_eq!(sync.wave_cores, 1);
        assert_eq!(sync.spawn_count, 1);
        assert_eq!(session.player_core_count(), 1);
        assert_eq!(session.wave_core_count(), 1);
    }

    #[test]
    fn sync_applies_the_launch_loadout() {
        use crate::ecs::BuildingComp as Comp;
        use crate::io::json::content_serde::JsonItemStack;
        use crate::world::TilePos;

        let registry = content();
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.add_starting_items = true;
        session.rules.loadout = vec![JsonItemStack {
            item: Some("copper".to_owned()),
            amount: 500,
        }];
        let mut world = World::new();
        let core = registry.block_id("core-shard").expect("core");
        let entity = world
            .spawn(Comp {
                pos: TilePos::new(8, 8),
                block: core,
                team: TeamId(1),
                rot: 0,
            })
            .id();
        let lead = registry.item_id("lead").expect("lead");
        {
            let mut module = ItemModule::with_items(registry.items().len());
            module.add(lead, 9, 100);
            world.entity_mut(entity).insert(module);
        }
        let _ = sync_session_with_sim(&mut session, &mut world, &registry, 0, true);
        let copper = registry.item_id("copper").expect("copper");
        let inventory = session
            .teams
            .inventory_ref(TeamId(1))
            .expect("team inventory");
        assert_eq!(inventory.get(copper), 500);
        // Upstream also clears/loads the core building's own storage.
        let module = world.get::<ItemModule>(entity).expect("core module");
        assert_eq!(module.get(copper), 500);
        assert_eq!(module.get(lead), 0, "fresh launch clears core storage");
    }
}
