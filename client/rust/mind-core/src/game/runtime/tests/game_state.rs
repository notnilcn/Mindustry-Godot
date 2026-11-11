// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::GameStateCheck` and the sector-capture bookkeeping tests.
//! Upstream: none (port-only tests).

use crate::ecs::TeamId;
use crate::game::State;
use crate::game::achievements::{Achievement, SStat};
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;
use bevy_ecs::entity::Entity;

#[test]
fn systems_are_noop_without_the_resource() {
    let mut world = World::new();
    campaign_tick_system(&mut world);
    objectives_tick_system(&mut world);
    game_state_tick_system(&mut world);
    team_ai_system(&mut world);
    assert!(world.get_resource::<CampaignRuntime>().is_none());
}

/// T7: `Logic.update` runs `checkGameState` only when
/// `state.rules.canGameOver` (`Logic.java:521`).
#[test]
fn game_state_check_respects_can_game_over() {
    let registry = content();
    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        can_game_over: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    // No player core: `checkGameState` would emit GameOver if it ran.
    let mut world = World::new();
    world.insert_resource(CampaignRuntime::new(session, campaign));
    game_state_tick_system(&mut world);
    let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    assert!(
        !runtime
            .events
            .iter()
            .any(|event| matches!(event, PlayEvent::GameOver { .. })),
        "canGameOver=false suppresses the check: {:?}",
        runtime.events
    );
}

/// Finding #2: the shared capture hook runs the `GameService` listener with
/// the pre-capture values and queues the derelict wave team — the seam both
/// `game_state_tick_system` and `MindCampaign::capture_sector` use.
#[test]
fn captured_sector_hook_tracks_achievements_and_derelict_team() {
    let registry = content();
    let campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").expect("serpulo");
    let sector_id = registry
        .sector_by_name("groundZero")
        .expect("groundZero")
        .sector;
    let mut session = PlaySession::new(Rules {
        attack_mode: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session.sector = Some((planet, sector_id));
    session.wave = 3;
    let mut runtime = CampaignRuntime::new(session, campaign);

    on_captured_sector(&mut runtime, planet, true, 0);

    assert_eq!(runtime.service.service().get(SStat::AttacksWon), 1);
    assert!(Achievement::DefeatAttack5Waves.unlocked(runtime.service.service()));
    assert!(Achievement::CaptureNoBlocksBroken.unlocked(runtime.service.service()));
    assert_eq!(runtime.pending_derelict_team, Some(2));
}

/// T7: `current_sector_captured` follows `Sector.isCaptured`
/// (`Sector.java:181-184`): a played campaign sector with waves and attack
/// mode off; no runtime means not captured.
#[test]
fn captured_flag_follows_rules() {
    let registry = content();
    let campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
    let mut session = PlaySession::new(Rules {
        waves: false,
        attack_mode: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.sector = Some((planet, sector_id));
    let mut world = World::new();
    world.insert_resource(CampaignRuntime::new(session, campaign));
    assert!(current_sector_captured(&world));
    world
        .get_resource_mut::<CampaignRuntime>()
        .expect("runtime")
        .session
        .rules
        .waves = true;
    assert!(!current_sector_captured(&world));
    assert!(!current_sector_captured(&World::new()), "no runtime");
}

/// GAPS #68: a played attack-mode capture feeds `attacksWon`,
/// `defeatAttack5Waves`, `captureNoBlocksBroken` and `sectorsControlled`
/// through the `SectorCaptureEvent` listener path.
#[test]
fn game_state_capture_tracks_attack_stats_and_achievements() {
    let registry = content();
    let mut campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
    if let Some(sector) = campaign.sector_mut(planet, sector_id) {
        sector.save = Some("sector-save".into());
        sector.info.info.has_core = true;
    }
    let mut session = PlaySession::new(Rules {
        waves: false,
        attack_mode: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.sector = Some((planet, sector_id));
    session.phase = State::Playing;
    session.wave = 3;
    session.spawn_count = 1;
    let core = Entity::from_raw_u32(1).expect("entity");
    session
        .teams
        .register_core(core, TeamId(session.default_team()), &session.rules);
    let mut world = World::new();
    world.insert_resource(CampaignRuntime::new(session, campaign));

    // Attack mode with no wave-team core left captures the sector.
    game_state_tick_system(&mut world);
    let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    assert!(
        runtime
            .events
            .iter()
            .any(|event| matches!(event, PlayEvent::SectorCapture { .. })),
        "capture fired: {:?}",
        runtime.events
    );
    let service = runtime.service.service();
    assert_eq!(service.get(SStat::AttacksWon), 1);
    assert_eq!(service.get(SStat::SectorsControlled), 1);
    assert!(Achievement::DefeatAttack5Waves.unlocked(service));
    assert!(Achievement::CaptureNoBlocksBroken.unlocked(service));
}

/// The forced-capture path captures the played sector through the runtime:
/// disables waves, records the capture, applies the listener bookkeeping and
/// queues the ordered events. Without a played sector it reports `false`.
#[test]
fn capture_played_sector_forces_the_capture() {
    let registry = content();
    let campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
    let mut runtime = CampaignRuntime::new(PlaySession::default(), campaign);
    assert!(
        !capture_played_sector(&mut runtime),
        "no played sector -> no capture"
    );
    assert!(runtime.events.is_empty());

    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        waves: true,
        attack_mode: true,
        ..Rules::default()
    });
    session.sector = Some((planet, sector_id));
    session.phase = State::Playing;
    let sector = runtime
        .campaign
        .sector_mut(planet, sector_id)
        .expect("sector");
    sector.save = Some("sector-test".to_owned());
    sector.info.info.has_core = true;
    runtime.session = session;

    assert!(
        capture_played_sector(&mut runtime),
        "the played sector captures"
    );
    assert!(!runtime.session.rules.waves);
    assert!(!runtime.session.rules.attack_mode);
    let events = runtime.take_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, PlayEvent::SectorCapture { .. })),
        "the capture event is queued"
    );
    let record = runtime.campaign.sector(planet, sector_id).expect("sector");
    assert!(
        record.info.info.was_captured,
        "the record marks the capture"
    );
    assert_eq!(
        runtime.pending_derelict_team,
        Some(runtime.session.rules.wave_team),
        "the Logic.java:117-121 listener queues the derelict wave team"
    );
}
