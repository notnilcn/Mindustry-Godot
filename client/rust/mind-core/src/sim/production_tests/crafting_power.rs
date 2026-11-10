// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime crafting, power and auto-link regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// T1 (`Reconstructor.shouldConsume`, `Reconstructor.java:376-379` + the power
/// half of `BuildingComp.updateConsumption`, `BuildingComp.java:1955-2005`): a
/// reconstructor with items loaded but no payload draws no power; once a payload
/// is handed over it requests its `consumePower(3f)` per tick
/// (`ConsumePower.usage` is per tick in Java).
#[test]
fn reconstructor_idle_requests_no_power() {
    let mut sim = runtime_sim();
    let _source = place(&mut sim, "power-source", 3, 5);
    let rig = place(&mut sim, "additive-reconstructor", 5, 5);
    let (silicon, graphite) = (item(&sim, "silicon"), item(&sim, "graphite"));
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(rig) {
            items.add(silicon, 40, 80);
            items.add(graphite, 40, 80);
        }
    }
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let (efficiency, should) = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(rig)
        .map(|building| (building.efficiency, building.should_consume))
        .expect("building");
    assert!(!should, "no payload -> shouldConsume false");
    assert_eq!(efficiency, 0.0, "idle reconstructor efficiency");

    let graph = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(rig)
        .map(|module| module.graph)
        .unwrap_or_default();
    let needed = sim
        .ecs
        .0
        .resource::<crate::world::blocks::power::PowerGrids>()
        .graph(graph)
        .map(|graph| graph.get_power_needed(&sim.ecs.0, 1.0))
        .unwrap_or(-1.0);
    assert_eq!(needed, 0.0, "idle reconstructor requests no power");

    // Hand over a dagger payload; the graph then requests 3/tick.
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let inst = sim
        .ecs
        .0
        .resource::<crate::world::block::BlockTable>()
        .instance(
            sim.ecs
                .0
                .get::<crate::entities::comp::Building>(rig)
                .expect("building")
                .block,
        )
        .expect("instance");
    inst.behavior.handle_payload(
        &mut sim.ecs.0,
        rig,
        rig,
        crate::world::behavior::PayloadRef {
            entity: None,
            content: dagger.raw(),
            is_block: false,
        },
    );
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let should = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(rig)
        .map(|building| building.should_consume)
        .unwrap_or(false);
    assert!(should, "payload -> shouldConsume true");
    let needed = sim
        .ecs
        .0
        .resource::<crate::world::blocks::power::PowerGrids>()
        .graph(graph)
        .map(|graph| graph.get_power_needed(&sim.ecs.0, 1.0))
        .unwrap_or(-1.0);
    assert_eq!(needed, 3.0, "constructing reconstructor requests 3/tick");
}

/// T1 (`GenericCrafter.shouldConsume`, `GenericCrafter.java:205-235`): a
/// graphite press with 10/10 graphite stored keeps its 4 coal (before the fix
/// the press kept crafting and consuming coal).
#[test]
fn full_crafter_stops_consuming() {
    let mut sim = runtime_sim();
    let press = place(&mut sim, "graphite-press", 5, 5);
    let (graphite, coal) = (item(&sim, "graphite"), item(&sim, "coal"));
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(press) {
            items.add(graphite, 10, 10);
            items.add(coal, 4, 10);
        }
    }
    for _ in 0..200 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, press, coal),
        4,
        "coal untouched while the output is full"
    );
    let should = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(press)
        .map(|building| building.should_consume)
        .unwrap_or(true);
    assert!(!should, "full output -> shouldConsume false");
}

/// T1 (`Drill.shouldConsume`, `Drill.java:246-249`): a mechanical drill with a
/// full 10-item buffer idles (efficiency 0) even though it has ore to mine.
#[test]
fn full_drill_stops() {
    let mut sim = runtime_sim();
    set_floor(&mut sim, 5, 5, "ore-copper");
    set_floor(&mut sim, 6, 5, "ore-copper");
    let drill = place(&mut sim, "mechanical-drill", 5, 5);
    let copper = item(&sim, "copper");
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(drill) {
            items.add(copper, 10, 10);
        }
    }
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let (efficiency, should, total) = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(drill)
        .map(|building| (building.efficiency, building.should_consume, 0))
        .expect("building");
    assert!(!should, "full buffer -> shouldConsume false");
    assert_eq!(efficiency, 0.0, "full drill efficiency");
    assert_eq!(stored_item(&sim, drill, copper), 10, "buffer stays full");
    let _ = total;
}

/// T2 (`UnitFactory.updateTile`, `UnitFactory.java:400-450`): an unpowered
/// ground factory produces nothing even with the plan's items loaded; powered,
/// one dagger completes in the plan's 900 ticks and consumes 10 silicon + 10
/// lead (`Blocks.java:6305`).
#[test]
fn unit_factory_needs_power() {
    let mut sim = runtime_sim();
    let factory = place(&mut sim, "ground-factory", 5, 5);
    let (silicon, lead) = (item(&sim, "silicon"), item(&sim, "lead"));
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(factory) {
            items.add(silicon, 10, 60);
            items.add(lead, 10, 60);
        }
    }
    for _ in 0..3000 {
        sim.tick().expect("tick");
    }
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    assert!(
        !spawned_unit(&sim, dagger),
        "unpowered factory produces nothing"
    );
    assert_eq!(stored_item(&sim, factory, silicon), 10, "items untouched");

    let _source = place(&mut sim, "power-source", 3, 5);
    let start = sim.tick_count();
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, dagger) {
            break;
        }
    }
    let elapsed = sim.tick_count() - start;
    assert!(
        spawned_unit(&sim, dagger),
        "powered factory built the dagger"
    );
    assert!(
        (900..=1000).contains(&elapsed),
        "dagger build time {elapsed} ticks (plan time 900)"
    );
    assert_eq!(stored_item(&sim, factory, silicon), 0, "silicon consumed");
    assert_eq!(stored_item(&sim, factory, lead), 0, "lead consumed");
}

/// H07 T2 (`UnitFactory.java:440`): producing a unit plays `unitCreate` once
/// at the factory centre, volume 1, `pitch = 1 + range(0.06)`.
#[test]
fn ground_factory_plays_unit_create() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let factory = place(&mut sim, "ground-factory", 5, 5);
    let (silicon, lead) = (item(&sim, "silicon"), item(&sim, "lead"));
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(factory) {
            items.add(silicon, 10, 60);
            items.add(lead, 10, 60);
        }
    }
    let _source = place(&mut sim, "power-source", 3, 5);
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, dagger) {
            break;
        }
    }
    assert!(spawned_unit(&sim, dagger), "factory produced a dagger");
    let events = log.events();
    let creates: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::At { sound, .. } if *sound == SoundId::UNIT_CREATE => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(creates.len(), 1, "exactly one unitCreate: {events:?}");
    let AudioEvent::At {
        x,
        y,
        pitch,
        volume,
        ..
    } = creates[0]
    else {
        unreachable!()
    };
    // Size-3 factory at (5,5): centre tile 5*8, offset 0.
    assert_eq!((*x, *y), (40.0, 40.0), "factory centre");
    assert_eq!(*volume, 1.0);
    assert!((0.94..=1.06).contains(pitch), "unitCreate pitch {pitch}");
}

/// H07 T2 (`MendProjector.java:120`): a mender pulse that heals at least one
/// damaged building plays `healWave` (volume 0.5, `pitch = 1 + range(0.1)`)
/// at the mender centre; a pulse with nothing to heal is silent.
#[test]
fn mender_pulse_plays_heal_wave() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let _mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let wall = place(&mut sim, "copper-wall", 6, 5);
    if let Some(mut health) = sim.ecs.0.get_mut::<crate::entities::comp::Health>(wall) {
        health.health = 10.0;
    }
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let events = log.events();
    let heals: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::At { sound, .. } if *sound == SoundId::HEAL_WAVE => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(heals.len(), 1, "one healWave in 300 ticks: {events:?}");
    let AudioEvent::At {
        x,
        y,
        pitch,
        volume,
        ..
    } = heals[0]
    else {
        unreachable!()
    };
    assert_eq!((*x, *y), (40.0, 40.0), "mender centre");
    assert_eq!(*volume, 0.5);
    assert!((0.9..=1.1).contains(pitch), "healWave pitch {pitch}");

    // Same fixture with nothing damaged -> no sound.
    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let _mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let _wall = place(&mut sim, "copper-wall", 6, 5);
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let heals = log.events().iter().filter(|ticked| {
        matches!(&ticked.event, AudioEvent::At { sound, .. } if *sound == SoundId::HEAL_WAVE)
    }).count();
    assert_eq!(heals, 0, "undamaged targets -> silent");
}

/// T3 (`Reconstructor.java:346-352`): the command configured on a
/// reconstructor is given to the upgraded unit's command state; without one the
/// unit falls back to its type's `defaultCommand`.
#[test]
fn reconstructor_applies_command() {
    let mut sim = runtime_sim();
    // Player-team units take the `Command` controller (`select_team_controller`).
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let _source = place(&mut sim, "power-source", 3, 5);
    let rig = place(&mut sim, "additive-reconstructor", 5, 5);
    let rebuild = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .get_by_name(crate::content::ContentType::UnitCommand, "rebuild")
        .map(|reference| crate::content::UnitCommandId::new(reference.id))
        .expect("rebuild command");
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Content("rebuild".to_owned()),
    );

    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let mace = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("mace")
        .expect("mace");
    let inst = sim
        .ecs
        .0
        .resource::<crate::world::block::BlockTable>()
        .instance(
            sim.ecs
                .0
                .get::<crate::entities::comp::Building>(rig)
                .expect("building")
                .block,
        )
        .expect("instance");
    inst.behavior.handle_payload(
        &mut sim.ecs.0,
        rig,
        rig,
        crate::world::behavior::PayloadRef {
            entity: None,
            content: dagger.raw(),
            is_block: false,
        },
    );
    {
        let (silicon, graphite) = (item(&sim, "silicon"), item(&sim, "graphite"));
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(rig) {
            items.add(silicon, 40, 80);
            items.add(graphite, 40, 80);
        }
    }
    let mut spawned = None;
    for _ in 0..1500 {
        sim.tick().expect("tick");
        spawned = sim.ecs.0.iter_entities().find_map(|entity| {
            entity
                .get::<crate::entities::comp::unit::UnitTypeComp>()
                .filter(|comp| comp.type_id == mace)
                .map(|_| entity.id())
        });
        if spawned.is_some() {
            break;
        }
    }
    let entity = spawned.expect("mace spawned");
    let command = sim
        .ecs
        .0
        .get::<crate::ai::ControllerSlot>(entity)
        .and_then(|slot| slot.command);
    assert_eq!(command, Some(rebuild), "configured command applied");
}

/// T11 (`PowerNode.placed`, `PowerNode.java:400-411`): a freshly placed power
/// node grabs its potential links (the adjacent generator).
#[test]
fn node_autolinks_on_place() {
    let mut sim = runtime_sim();
    let generator = place(&mut sim, "combustion-generator", 5, 5);
    let node = place(&mut sim, "power-node", 9, 5);
    let generator_pack = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(generator)
        .map(|building| building.tile.pack())
        .expect("generator building");
    let links = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(node)
        .map(|module| module.links.clone())
        .unwrap_or_default();
    assert!(
        links.contains(&generator_pack),
        "node linked to the generator: {links:?}"
    );
}

/// T11 (`BuildingComp.placed`, `BuildingComp.java:1359-1370`): a consumer placed
/// inside a node's laser range links that node to it (`getNodeLinks`) and the
/// two graphs merge.
#[test]
fn consumer_autolinks_to_node_in_range() {
    let mut sim = runtime_sim();
    set_floor(&mut sim, 9, 5, "ore-copper");
    let node = place(&mut sim, "power-node", 5, 5);
    let drill = place(&mut sim, "laser-drill", 9, 5);
    let drill_pack = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(drill)
        .map(|building| building.tile.pack())
        .expect("drill building");
    let links = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(node)
        .map(|module| module.links.clone())
        .unwrap_or_default();
    assert!(
        links.contains(&drill_pack),
        "node linked to the consumer: {links:?}"
    );
    let node_graph = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(node)
        .map(|module| module.graph);
    let drill_graph = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(drill)
        .map(|module| module.graph);
    assert_eq!(node_graph, drill_graph, "graphs merged");
}

/// `PowerSource extends PowerNode` (`PowerSource.java:6-12`): a consumer placed
/// in the sandbox source's laser range autolinks to it (`BuildingComp.placed`
/// -> `PowerNode.getNodeLinks`) and its `power.status` reaches 1 (the hover
/// `Power` bar, `Block.java:716-721`).
#[test]
fn consumer_autolinks_to_power_source_in_range() {
    let mut sim = runtime_sim();
    let source = place(&mut sim, "power-source", 5, 5);
    let conveyor = place(&mut sim, "phase-conveyor", 8, 5);
    let conveyor_pack = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(conveyor)
        .map(|building| building.tile.pack())
        .expect("conveyor building");
    let links = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(source)
        .map(|module| module.links.clone())
        .unwrap_or_default();
    assert!(
        links.contains(&conveyor_pack),
        "source linked to the consumer: {links:?}"
    );
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let status = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(conveyor)
        .map(|module| module.status)
        .unwrap_or(0.0);
    assert!((status - 1.0).abs() < 1e-6, "consumer satisfied: {status}");
}
