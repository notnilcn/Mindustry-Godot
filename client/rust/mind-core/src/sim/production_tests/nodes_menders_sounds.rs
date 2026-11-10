// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime power-node, mender, overdrive and place/break sound regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// T2: a live-placed power node keeps its behavior knobs
/// (`Blocks.java:2483-2505`): power-node-large 15/15, surge-tower 2/40.
/// Before the fix `SimHooks::new_building` overwrote the config with the
/// default (3, 6.0) right after `create_state`.
#[test]
fn large_node_keeps_knobs_on_live_place() {
    use crate::world::blocks::power::PowerNodeConfig;
    let mut sim = runtime_sim();
    let large = place(&mut sim, "power-node-large", 5, 5);
    let surge = place(&mut sim, "surge-tower", 10, 5);
    let large_config = sim
        .ecs
        .0
        .get::<PowerNodeConfig>(large)
        .cloned()
        .expect("large node config");
    assert_eq!(
        (large_config.max_nodes, large_config.laser_range),
        (15, 15.0)
    );
    let surge_config = sim
        .ecs
        .0
        .get::<PowerNodeConfig>(surge)
        .cloned()
        .expect("surge tower config");
    assert_eq!(
        (surge_config.max_nodes, surge_config.laser_range),
        (2, 40.0)
    );
}

/// T2: a Configure with a packed target toggles a node link
/// (`PowerNode.java:419-441`): link -> both ends share a graph; toggling again
/// unlinks and the graphs split.
#[test]
fn node_configure_toggles_link() {
    use crate::world::blocks::power::PowerNodeConfig;
    let mut sim = runtime_sim();
    let _battery = place(&mut sim, "battery", 10, 5);
    let node = place(&mut sim, "power-node", 5, 5);
    assert!(sim.ecs.0.get::<PowerNodeConfig>(node).is_some());
    let pack = TilePos::new(10, 5).pack();
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let linked = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::world::modules::PowerModule>(node)
            .is_some_and(|module| module.links.contains(&pack))
    };
    let graph_of = |sim: &Sim, pos: (i16, i16)| {
        sim.grid
            .entity_at(TilePos::new(pos.0, pos.1))
            .and_then(|e| sim.ecs.0.get::<crate::world::modules::PowerModule>(e))
            .map(|module| module.graph)
    };
    // Reach the linked state deterministically: the node's `placed()` autolink
    // may or may not have linked the battery first.
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Int(pack));
    if !linked(&sim) {
        configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Int(pack));
    }
    assert!(linked(&sim), "configure links the battery");
    assert_eq!(
        graph_of(&sim, (5, 5)),
        graph_of(&sim, (10, 5)),
        "linked ends share a graph"
    );
    // Toggle again: unlink and reflow both ends.
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Int(pack));
    assert!(!linked(&sim), "second configure unlinks");
    assert_ne!(
        graph_of(&sim, (5, 5)),
        graph_of(&sim, (10, 5)),
        "unlinked ends split graphs"
    );
}

/// T2: a diode between a charged battery graph and an empty one transfers
/// power (`PowerDiode.java:47-80`); the diode is wired for the first time here.
#[test]
fn diode_moves_power_forward() {
    let mut sim = runtime_sim();
    let a = place(&mut sim, "battery", 2, 5);
    let diode = place(&mut sim, "diode", 3, 5);
    let b = place(&mut sim, "battery", 4, 5);
    let _ = diode;
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    sim.ecs
        .0
        .get_mut::<crate::world::modules::PowerModule>(a)
        .expect("battery a module")
        .status = 1.0;
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let status = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::world::modules::PowerModule>(e)
            .map(|module| module.status)
            .unwrap_or(0.0)
    };
    let (a_status, b_status) = (status(&sim, a), status(&sim, b));
    assert!(
        b_status > 0.0,
        "the empty battery received power: {b_status}"
    );
    assert!(a_status < 1.0, "the full battery drained: {a_status}");
}

/// T4: a mender only heals its own team's buildings
/// (`BlockIndexer.eachBlock` is team-filtered, `BlockIndexer.java:328-361`;
/// `MendProjector.java:107-116`).
#[test]
fn mender_heals_only_own_team() {
    let mut sim = runtime_sim();
    let _mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let own = place(&mut sim, "copper-wall", 6, 5);
    let wall = sim.world_block_id("copper-wall").expect("wall");
    sim.command(SimCommand::Place {
        x: 5,
        y: 4,
        block: wall,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("enemy wall placed");
    let enemy = sim
        .grid
        .entity_at(TilePos::new(5, 4))
        .expect("enemy wall entity");
    {
        let world = &mut sim.ecs.0;
        for entity in [own, enemy] {
            world
                .get_mut::<crate::entities::comp::Health>(entity)
                .expect("health")
                .health = 10.0;
        }
    }
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let health_of = |sim: &Sim, entity: Entity| {
        sim.ecs
            .0
            .get::<crate::entities::comp::Health>(entity)
            .map(|health| health.health)
            .unwrap_or(0.0)
    };
    let own_health = health_of(&sim, own);
    assert!(
        (own_health - 22.8).abs() < 0.01,
        "own wall healed by 4% of 320: {own_health}"
    );
    assert_eq!(health_of(&sim, enemy), 10.0, "the enemy wall is not healed");
}

/// T4: the mender's silicon boost is consumed on its `useTime` cycle
/// (`MendProjector.java:95-104`: `timer(timerUse, useTime / timeScale)` ->
/// `consume()`), and `phaseHeat` follows `optionalEfficiency`
/// (`MendProjector.java:100`).
#[test]
fn mender_silicon_boost_consumed() {
    let mut sim = runtime_sim();
    let mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let silicon = item(&sim, "silicon");
    {
        let world = &mut sim.ecs.0;
        transfer::dispatch_handle_stack(world, mender, silicon, 2);
    }
    assert_eq!(stored_item(&sim, mender, silicon), 2);
    // The power graph settles a few ticks in; `optionalEfficiency` is capped by
    // the required (power) efficiency, so `consume()` starts once powered.
    // `timer()`'s first call is true: that first useTime tick consumes one.
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, mender, silicon),
        1,
        "one silicon consumed on the first powered useTime tick"
    );
    for _ in 0..51 {
        sim.tick().expect("tick");
    }
    let phase_heat = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::shields::MendProjectorState>(mender)
        .map(|state| state.phase_heat)
        .unwrap_or(0.0);
    assert!(phase_heat > 0.9, "phaseHeat warmed up: {phase_heat}");
    for _ in 0..349 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, mender, silicon),
        0,
        "the second useTime tick (400 ticks later) consumed the last silicon"
    );
}

/// T4: the overdrive projector boosts only its own team
/// (`OverdriveProjector.java:105-113` -> team-filtered `eachBlock`) and
/// consumes its boost items on the `useTime` cycle.
#[test]
fn overdrive_does_not_boost_enemy() {
    let mut sim = runtime_sim();
    // The overdrive is 2x2 at (5,5) (footprint 5..6, 5..6); the source sits
    // outside the footprint at (4,5).
    let _overdrive = place(&mut sim, "overdrive-projector", 5, 5);
    let _power = place(&mut sim, "power-source", 4, 5);
    let own = place(&mut sim, "graphite-press", 8, 5);
    let press = sim.world_block_id("graphite-press").expect("press");
    sim.command(SimCommand::Place {
        x: 5,
        y: 9,
        block: press,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("enemy press placed");
    let enemy = sim
        .grid
        .entity_at(TilePos::new(5, 9))
        .expect("enemy press entity");
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let time_scale = |sim: &Sim, entity: Entity| {
        sim.ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .map(|building| building.time_scale)
            .unwrap_or(1.0)
    };
    let own_scale = time_scale(&sim, own);
    assert!(
        (own_scale - 1.5).abs() < 0.001,
        "own press is boosted to 1.5: {own_scale}"
    );
    assert_eq!(
        time_scale(&sim, enemy),
        1.0,
        "the enemy press is not boosted"
    );
}

/// PAR-0073/T3: `ConstructBlock.constructFinish` -> `block.placeSound.at(tile)`
/// (`ConstructBlock.java:71-120`); size 1 wall centre 5*8 (offset 0).
#[test]
fn placing_emits_construct_place_sound() {
    use crate::audio::{AudioEvent, SharedAudioLog};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    place(&mut sim, "copper-wall", 5, 5);
    sim.tick().expect("tick");
    let events = log.events();
    let placed: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::Construct { place: true, .. } => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), 1, "one place sound: {events:?}");
    let AudioEvent::Construct {
        sound,
        x,
        y,
        place,
        pitch_change,
    } = placed[0]
    else {
        unreachable!()
    };
    assert_eq!(*sound, crate::audio::SoundId::BLOCK_PLACE1);
    assert_eq!((*x, *y), (40.0, 40.0));
    assert!(*place && *pitch_change);
}

/// PAR-0073/T3: `ConstructBlock.deconstructFinish` -> `breakSound`, not the
/// destroy path (`ConstructBlock.java:58-69`).
#[test]
fn breaking_emits_break_not_destroy() {
    use crate::audio::{AudioEvent, SharedAudioLog};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    place(&mut sim, "copper-wall", 5, 5);
    sim.tick().expect("tick");
    log.clear();
    sim.command(SimCommand::Break {
        x: 5,
        y: 5,
        player: None,
    })
    .expect("break");
    sim.tick().expect("tick");
    let events = log.events();
    let constructs: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::Construct { place: false, .. } => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(constructs.len(), 1, "one break sound: {events:?}");
    let AudioEvent::Construct { sound, .. } = constructs[0] else {
        unreachable!()
    };
    assert_eq!(*sound, crate::audio::SoundId::BLOCK_BREAK1);
    assert!(
        !events
            .iter()
            .any(|ticked| matches!(ticked.event, AudioEvent::At { .. })),
        "a deconstruct must not play destroy sounds: {events:?}"
    );
}

/// PAR-0073/T3: size-2 walls use `blockBreak2`/`blockPlace2` and the block
/// offset centre (`Block.java:1367-1379`).
#[test]
fn large_wall_place_sound_is_size_2() {
    use crate::audio::{AudioEvent, SharedAudioLog};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let offset = {
        let content = sim.block_runtime().expect("runtime").content();
        content
            .block_id("copper-wall-large")
            .and_then(|id| content.block(id))
            .expect("copper-wall-large")
            .offset
    };
    place(&mut sim, "copper-wall-large", 8, 8);
    sim.tick().expect("tick");
    let events = log.events();
    let placed = events
        .iter()
        .find_map(|ticked| match &ticked.event {
            AudioEvent::Construct { place: true, .. } => Some(&ticked.event),
            _ => None,
        })
        .expect("place sound");
    let AudioEvent::Construct { sound, x, y, .. } = placed else {
        unreachable!()
    };
    assert_eq!(*sound, crate::audio::SoundId::BLOCK_PLACE2);
    assert_eq!((*x, *y), (64.0 + offset, 64.0 + offset));
}
