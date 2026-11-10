// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign session sync: registers the live world's cores into the session
//! team registry and runs `runWave` on demand.
//! Upstream: core/src/mindustry/core/Logic.java (`runWave`),
//! core/src/mindustry/game/Universe.java (invasions)

use super::*;
use crate::content::{BlockKind, ContentRegistry};
use crate::ecs::{BuildingComp, TeamId};
use crate::entities::comp::Health;
use crate::game::play::run_wave_campaign;
use crate::game::teams::Teams;
use crate::world::modules::ItemModule;
use bevy_ecs::entity::Entity;

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

/// `Universe.runTurn` played-sector invasion (`Universe.java:550-555`): restart
/// the live waves (`winWave`/`waves`/`attackMode=false`) and re-derive the
/// planet campaign rules on the live session, marking the world for the
/// `Sim::apply_rules_to_world` drain.
pub fn apply_played_invasions(
    runtime: &mut CampaignRuntime,
    events: &[crate::game::universe::CampaignEvent],
) {
    for event in events {
        if let crate::game::universe::CampaignEvent::SectorInvasion {
            planet,
            played: true,
            win_wave,
            ..
        } = event
            && let Some(planet_record) = runtime.campaign.planet(*planet)
        {
            runtime.session.rules.win_wave = *win_wave;
            runtime.session.rules.waves = true;
            runtime.session.rules.attack_mode = false;
            planet_record.campaign_rules.apply_planet(
                planet_record,
                &mut runtime.session.rules,
                true,
            );
            runtime.rules_world_dirty = true;
        }
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
    // `World.setRules`: copy every placement-relevant rule into the
    // `BuildRules` resource (H02 T2 `sync_build_rules`); H04/H06 call the same
    // entry point after any rules swap.
    crate::world::limits::sync_build_rules(world, &session.rules, session.is_campaign());
    let mut sync = SessionSync::default();
    for (entity, team, _) in &cores {
        session
            .teams
            .register_core(*entity, TeamId(*team), &session.rules);
        // `Logic.play` heals every active team's cores on a fresh play
        // (`Logic.java:286-291` `entity.heal()`); a resume keeps saved health.
        if apply_loadout && let Some(mut health) = world.get_mut::<Health>(*entity) {
            health.health = health.max_health;
        }
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
    // `WaveSpawner.countSpawns()` (`WaveSpawner.java:49-51`) counts ground spawn
    // tiles only; enemy cores are added by `Logic.checkGameState`, never here.
    sync.spawn_count = map_spawns;
    session.spawn_count = sync.spawn_count;
    session.enemies = 0;
    // `Logic.play` starting-items half: with the cores registered, clear the
    // default team's inventory and add the launch loadout (GAP-10) on a fresh
    // launch; a resumed save keeps its own core inventory.
    if apply_loadout {
        crate::game::play::apply_launch_loadout(session, content);
        // Upstream clears each core building's own items and adds the loadout
        // there (`entity.items.clear()` + `items.add`); the live core module is
        // the authoritative store (`Team.items()`/`Sector.items()` while
        // playing), so mirror the loadout into it as well.
        if crate::game::play::launch_loadout_applies(session) {
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
/// `Logic.runWave()` (`Logic.java:319-325`) on the live session: advances the
/// wave, resets the countdown, counts `wavesLasted`, and queues
/// `spawnEnemies()` for the pre-increment wave on the unit runtime.
///
/// `run_wave_campaign` returns the `SpawnGroup.getSpawned` index (the
/// pre-increment `wave - 1`, `WaveSpawner.java:74`); callers must use it for
/// `pending_wave`, never `session.wave - 1` after the increment (H00 T2b).
pub fn run_wave_now(
    runtime: &mut CampaignRuntime,
    unit_runtime: Option<&mut crate::sim::unit_runtime::UnitRuntime>,
) {
    let (event, spawn_index) = run_wave_campaign(&mut runtime.session);
    runtime.events.push(event);
    if let Some(ur) = unit_runtime {
        ur.pending_wave = Some(spawn_index);
    }
    // `WaveEvent` listener: `stats.wavesLasted++`.
    if let Some((planet, _)) = runtime.session.sector
        && let Some(stats) = runtime.campaign.stats_for_mut(planet)
    {
        stats.waves_lasted += 1;
    }
    // `GameService`'s `WaveEvent` listener (`maxWavesSurvived` +
    // `survive10WavesNoBlocks`; GAPS #68).
    runtime.service.on_wave(&runtime.session, &runtime.campaign);
}
