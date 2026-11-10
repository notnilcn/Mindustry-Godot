// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::Campaign`: universe clock, global positions, sector lighting and
//! the campaign wave timer.
//! Upstream: core/src/mindustry/core/Logic.java (`update`),
//! core/src/mindustry/game/Universe.java (`update`)

use super::hooks::{event_content, unlock_sort_key};
use super::*;
use crate::ecs::TeamId;
use crate::event::{EventRow, GameEvent};
use crate::game::State;
use crate::game::play::is_waiting_wave;
use crate::render::g3d::grid::PlanetGrid;
use crate::sim::runtime::ContentIndex;
use bevy_ecs::entity::Entity;

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

        // `Logic.update` wave timer (`!net.client()`, `Logic.java:560-569`):
        // `state.wavetime = Math.max(state.wavetime - Time.delta, 0)` while not
        // waiting for enemies (Time.delta is 1 per 60 Hz tick), then an
        // unconditional `runWave` gate on `state.wavetime <= 0 && rules.waves`.
        {
            let s = &mut runtime.session;
            if s.rules.waves && s.rules.wave_timer && !s.game_over && !is_waiting_wave(s) {
                s.wavetime = (s.wavetime - 1.0).max(0.0);
            }
        }
        if runtime.session.wavetime <= 0.0 && runtime.session.rules.waves {
            // `Logic.runWave` -> `WaveSpawner.spawnEnemies`: the live unit
            // runtime spawns the pre-increment wave in `TickSet::RunWave`.
            let mut unit_runtime =
                world.get_non_send_mut::<crate::sim::unit_runtime::UnitRuntime>();
            run_wave_now(&mut runtime, unit_runtime.as_deref_mut());
        }

        // `GameService` stat tracking (GAPS #68). The per-event listeners Java
        // registers are replaced by counter diffs over the existing aggregation
        // points: `poll` reads `GameStats`/`CampaignStats`, which
        // `Sim::flush_events` and `notify_unit_created` filled on the previous
        // tick. The `Timer.schedule(checkUpdate, 2f, 2f)` half runs every two
        // seconds (120 ticks).
        runtime.service.poll(&runtime.session, &runtime.campaign);
        if runtime.ticks % SERVICE_CHECK_TICKS == 0 {
            let active = crate::entities::comp::unit::queries::count(
                world,
                Some(runtime.session.default_team()),
            ) as i32;
            runtime
                .service
                .check_update(&runtime.session, &runtime.campaign, active);
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
            state.core_incinerates = runtime.session.rules.core_incinerates;
            state.allow_core_unloaders = runtime.session.rules.allow_core_unloaders;
            let core_deltas = std::mem::take(&mut state.core_deltas);
            let production_deltas = std::mem::take(&mut state.production_deltas);
            let unlocks = std::mem::take(&mut state.unlocks);
            let imports = std::mem::take(&mut state.imports);
            drop(state);
            // Produced/dumped content unlocks (`UnlockableContent.unlock()`
            // -> `UnlockEvent`): dedupe into the runtime queue for the host.
            let mut seen: std::collections::BTreeSet<(u8, u16)> = runtime
                .pending_unlocks
                .iter()
                .map(unlock_sort_key)
                .collect();
            for unlock in unlocks {
                if seen.insert(unlock_sort_key(&unlock)) {
                    runtime.pending_unlocks.push(unlock);
                    // `UnlockableContent.unlock()` fires `UnlockEvent`
                    // (`Control.java:160-174`); queue the typed row for the
                    // host `Events` bus (GAPS #37).
                    runtime
                        .session
                        .push_event_row(EventRow::Game(GameEvent::UnlockEvent(
                            crate::event::UnlockEvent {
                                content: event_content(unlock),
                            },
                        )));
                }
            }
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
                // Landing-pad arrivals record `handleItemImport` (H04 T7).
                for (item, amount) in imports {
                    if let Some(name) = name_of(item.index()) {
                        sector.info.handle_item_import(&name, amount);
                    }
                }
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
