// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::Campaign` universe clock and wave timer tests.
//! Upstream: none (port-only tests).

use crate::ecs::TeamId;
use crate::game::State;
use crate::game::achievements::{Achievement, SStat};
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;
use bevy_ecs::entity::Entity;

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
    // GAPS #68: the wave fired through `run_wave_now` reached the service,
    // and the capture's no-loss check ran in the game-state pass.
    let service = runtime.service.service();
    assert_eq!(service.get(SStat::MaxWavesSurvived), runtime.session.wave);
    assert!(Achievement::CaptureNoBlocksBroken.unlocked(service));
}

/// GAPS #68: `campaign_tick_system` folds the `GameStats`/`CampaignStats`
/// aggregation points into the local service (`GameService.poll`).
#[test]
fn campaign_tick_polls_stats_into_the_service() {
    let registry = content();
    let campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
    let mut session = PlaySession::new(Rules {
        waves: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.sector = Some((planet, sector_id));
    session.phase = State::Playing;
    session.stats.buildings_built = 2;
    session.stats.placed_block_count.insert("router".into(), 1);
    session.stats.units_created = 1;
    session.stats.enemy_units_destroyed = 3;
    session
        .stats
        .destroyed_block_count
        .insert("conveyor".into(), 1);
    let mut world = World::new();
    world.insert_resource(CampaignRuntime::new(session, campaign));

    campaign_tick_system(&mut world);
    let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    let service = runtime.service.service();
    assert_eq!(service.get(SStat::BlocksBuilt), 2);
    assert_eq!(service.get(SStat::BlocksDestroyed), 1);
    assert_eq!(service.get(SStat::UnitsBuilt), 1);
    assert_eq!(service.get(SStat::UnitsDestroyed), 3);
    assert!(runtime.service.blocks_built().contains("router"));
}

#[test]
fn wave_timer_counts_ticks() {
    let registry = content();
    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        waves: true,
        wave_timer: true,
        wave_spacing: 7200.0,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    // No sector: the non-campaign multiplier is 1.0.
    session.phase = State::Playing;
    session.wavetime = 120.0;
    session.spawn_count = 1;
    let mut world = World::new();
    world.insert_resource(CampaignRuntime::new(session, campaign));

    for _ in 0..119 {
        campaign_tick_system(&mut world);
    }
    {
        let runtime = world.get_resource::<CampaignRuntime>().expect("runtime");
        assert_eq!(runtime.session.wave, 1, "wave not due yet");
        assert_eq!(runtime.session.wavetime, 1.0, "one tick per tick");
    }
    campaign_tick_system(&mut world);
    let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    assert_eq!(runtime.session.wave, 2, "wave fired at zero");
    assert_eq!(runtime.session.wavetime, 7200.0, "spacing re-armed");
}

#[test]
fn waiting_wave_holds_timer() {
    let registry = content();
    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        waves: true,
        wave_timer: true,
        wave_spacing: 7200.0,
        wait_enemies: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session.wavetime = 50.0;
    session.enemies = 3;
    session.spawn_count = 1;
    let mut world = World::new();
    world.insert_resource(CampaignRuntime::new(session, campaign));

    for _ in 0..10 {
        campaign_tick_system(&mut world);
    }
    {
        let runtime = world.get_resource::<CampaignRuntime>().expect("runtime");
        assert_eq!(runtime.session.wavetime, 50.0, "held while enemies alive");
    }
    world
        .get_resource_mut::<CampaignRuntime>()
        .expect("runtime")
        .session
        .enemies = 0;
    for _ in 0..10 {
        campaign_tick_system(&mut world);
    }
    let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    assert_eq!(runtime.session.wavetime, 40.0, "countdown resumes");
}

#[test]
fn wave_runs_without_timer_when_due() {
    let registry = content();
    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        waves: true,
        wave_timer: false,
        wave_spacing: 7200.0,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session.wavetime = 0.0;
    session.spawn_count = 1;
    let mut world = World::new();
    world.insert_resource(CampaignRuntime::new(session, campaign));

    campaign_tick_system(&mut world);
    let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    assert_eq!(
        runtime.session.wave, 2,
        "Java runs the wave regardless of timer (wave 1 -> 2)"
    );
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
