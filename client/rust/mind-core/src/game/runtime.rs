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

use super::map_objectives::{ObjectiveEnv, ObjectiveRunParams};
use super::play::{PlayEvent, PlaySession, apply_sector_loss, check_game_state, run_wave_campaign};
use super::rules::Rules;
use super::teams::Teams;
use super::universe::Campaign;
use crate::content::{BlockKind, ContentRegistry};
use crate::ecs::{BuildingComp, TeamId};
use crate::game::State;
use crate::render::g3d::grid::PlanetGrid;

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
) -> SessionSync {
    let mut cores: Vec<(Entity, u8)> = Vec::new();
    for entity in world.iter_entities() {
        let Some(comp) = entity.get::<BuildingComp>() else {
            continue;
        };
        let Some(def) = content.block(comp.block) else {
            continue;
        };
        if def.kind == BlockKind::CoreBlock {
            cores.push((entity.id(), comp.team.0));
        }
    }

    session.teams = Teams::new();
    let default_team = session.default_team();
    let wave_team = session.wave_team();
    let mut sync = SessionSync::default();
    for (entity, team) in cores {
        session
            .teams
            .register_core(entity, TeamId(team), &session.rules);
        if team == default_team {
            sync.player_cores += 1;
        }
        if team == wave_team {
            sync.wave_cores += 1;
        }
    }
    sync.spawn_count = map_spawns.max(sync.wave_cores as i32);
    session.spawn_count = sync.spawn_count;
    session.enemies = 0;
    // `Logic.play` starting-items half: with the cores registered, clear the
    // default team's inventory and add the launch loadout (GAP-10).
    super::play::apply_launch_loadout(session, content);
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
                // `WaveEvent` listener: `stats.wavesLasted++`.
                if let Some((planet, _)) = runtime.session.sector
                    && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                {
                    stats.waves_lasted += 1;
                }
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

/// Minimal live-world objective environment: rules flags/`researched` and the
/// session team registry. Sim-backed counts land with the plan-07/08 ECS
/// adapters.
struct RuntimeObjectiveEnv<'a> {
    rules: &'a Rules,
    teams: &'a Teams,
    unlocked: &'a std::collections::HashSet<String>,
}

impl ObjectiveEnv for RuntimeObjectiveEnv<'_> {
    fn is_content_unlocked(&self, content: &str) -> bool {
        self.rules.researched.iter().any(|name| name == content) || self.unlocked.contains(content)
    }

    fn team_has_item(&self, _team: u8, _item: &str, amount: i32) -> bool {
        amount <= 0
    }

    fn core_item_count(&self, _item: &str) -> i32 {
        0
    }

    fn placed_block_count(&self, _block: &str) -> i32 {
        0
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
        assert!(world.get_resource::<CampaignRuntime>().is_none());
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
        let sync = sync_session_with_sim(&mut session, &mut world, &registry, 1);
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
        world.spawn(Comp {
            pos: TilePos::new(8, 8),
            block: core,
            team: TeamId(1),
            rot: 0,
        });
        let _ = sync_session_with_sim(&mut session, &mut world, &registry, 0);
        let copper = registry.item_id("copper").expect("copper");
        let inventory = session
            .teams
            .inventory_ref(TeamId(1))
            .expect("team inventory");
        assert_eq!(inventory.get(copper), 500);
    }
}
