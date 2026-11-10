// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the live-sim save/load executor.
//! Upstream: none (port-only; io handler tests)

use super::entities::LiveEntitySink;
use super::handler::boot_registry;
use super::*;
use crate::command::Command;
use crate::content::BlockId;
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Pos, TeamComp};
use crate::game::map_markers::MapMarkers;
use crate::io::StringMap;
use crate::io::fs::NativeFs;
use crate::io::save::SaveIo;
use crate::sim::IoStatus;
use crate::sim::unit_runtime::UnitRuntime;
use crate::world::TilePos;
use crate::world::WorldGrid;

fn temp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mind-sim-io-{tag}-{}.msav", std::process::id()))
}

#[test]
fn live_sim_save_then_load_roundtrips_the_tile_grid() {
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    let wall = sim.content().id("stone-wall").expect("stone-wall");
    sim.apply(Command::Place {
        x: 2,
        y: 3,
        block: wall,
    })
    .expect("place");
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("roundtrip");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);
    assert!(path.is_file(), "save file written");

    // Wipe the tile, then load it back at the next Apply boundary.
    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    assert_eq!(sim.grid.block_at(TilePos::new(2, 3)), Some(BlockId::AIR));
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);
    assert_eq!(sim.grid.block_at(TilePos::new(2, 3)), Some(wall));

    let _ = std::fs::remove_file(&path);
}

/// L02 T4 (PAR-0377): the save meta carries `state.map.name()`
/// (`SaveVersion.java:133`), a load restores it from `mapname`
/// (`SaveVersion.java:186-191`), `emptyMap` writes `Unknown`
/// (`Map.java:151-153`) and `logic.reset()` drops it.
#[test]
fn save_meta_mapname_is_the_loaded_map() {
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
    assert_eq!(sim.map_name(), "Unknown", "emptyMap before any load");
    sim.set_map_name("Ancient Caldera");

    let path = temp_path("mapname");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    assert_eq!(sim.take_io_responses()[0].status, IoStatus::Ok);
    let mut content = boot_registry().expect("content");
    let bytes = std::fs::read(&path).expect("save bytes");
    let save = decode_save_bytes(&bytes, &mut content).expect("decodes");
    assert_eq!(
        save.tags.get("mapname").map(String::as_str),
        Some("Ancient Caldera"),
        "not the `sim` placeholder"
    );

    sim.set_map_name("");
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    assert_eq!(sim.take_io_responses()[0].status, IoStatus::Ok);
    assert_eq!(sim.map_name(), "Ancient Caldera", "restored from mapname");

    sim.reset();
    assert_eq!(sim.map_name(), "Unknown", "logic.reset() -> emptyMap");
    let _ = std::fs::remove_file(&path);
}

/// A live-runtime building (plan 07 `Building` + modules) must write a
/// decodable tile-entity chunk, so a materialized map/sector core survives
/// the auto-save round trip (EV-0037).
#[test]
fn live_runtime_core_save_then_load_roundtrips() {
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let core = sim.content().id("core-shard").expect("core-shard");
    sim.apply(Command::Place {
        x: 5,
        y: 5,
        block: core,
    })
    .expect("place core");
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("live-core-roundtrip");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);
    assert!(path.is_file(), "save file written");

    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");
    assert_eq!(sim.grid.block_at(TilePos::new(5, 5)), Some(core));

    let _ = std::fs::remove_file(&path);
}

/// PAR-0161: a live save round-trips the per-kind bodies — the drill keeps
/// its progress/warmup and the sorter keeps its configured item. Before the
/// fix both were dropped by the v1 `read_building` (progress 0, config None).
#[test]
fn live_save_restores_drill_and_sorter_bodies() {
    fn configure(sim: &mut Sim, x: i16, y: i16, value: crate::determinism::ConfigValue) {
        sim.command(crate::determinism::SimCommand::Configure { x, y, value })
            .expect("configure");
    }

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    let drill = sim
        .content()
        .id("mechanical-drill")
        .expect("mechanical-drill");
    let sorter = sim.content().id("sorter").expect("sorter");
    sim.apply(Command::Place {
        x: 4,
        y: 4,
        block: drill,
    })
    .expect("place drill");
    sim.apply(Command::Place {
        x: 8,
        y: 8,
        block: sorter,
    })
    .expect("place sorter");
    let drill_entity = sim.grid.tile(4, 4).build.expect("drill entity");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::entities::comp::DrillState>(drill_entity)
            .expect("drill state");
        state.progress = 123.5;
        state.warmup = 0.75;
    }
    configure(
        &mut sim,
        8,
        8,
        crate::determinism::ConfigValue::Content("copper".to_owned()),
    );
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("drill-sorter-bodies");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    // Wipe the live world; the load must rebuild the per-kind bodies.
    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let drill_entity = sim.grid.tile(4, 4).build.expect("drill respawned");
    let state = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(drill_entity)
        .expect("drill state");
    assert!(
        (state.progress - 123.5).abs() < 1e-6,
        "drill progress {}",
        state.progress
    );
    assert!(
        (state.warmup - 0.75).abs() < 1e-6,
        "drill warmup {}",
        state.warmup
    );

    let sorter_entity = sim.grid.tile(8, 8).build.expect("sorter respawned");
    match crate::world::config::read_config(&sim.ecs.0, sorter_entity) {
        crate::world::config::ConfigValue::Item(item) => {
            assert_eq!(item, crate::content::ItemId::COPPER, "sorter keeps copper");
        }
        other => panic!("sorter config lost: {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn load_of_a_missing_file_is_a_failed_response() {
    let mut sim = Sim::new(1, 4, 4, BlockId::AIR, BlockId::AIR);
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
    let path = temp_path("missing");
    let _ = std::fs::remove_file(&path);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1);
    assert!(matches!(responses[0].status, IoStatus::Failed(_)));
}

/// T2: Java writes `tick` as a float (`SaveVersion.java:135`); the parser
/// must floor it instead of silently keeping the old tick.
#[test]
fn meta_tick_parses_java_float() {
    let mut tags = StringMap::new();
    tags.insert("tick".to_owned(), "74802.1339981854".to_owned());
    assert_eq!(meta_tick(&tags), Some(74802));
    tags.insert("tick".to_owned(), "12".to_owned());
    assert_eq!(meta_tick(&tags), Some(12));
    tags.insert("tick".to_owned(), "x".to_owned());
    assert_eq!(meta_tick(&tags), None);
}

/// T2: the shared decoder carries the saved units and meta tags without
/// touching the live sim (`SaveIO.load`'s world half) — the sector-resume
/// path uses it to restore what the old `load_grid_file` dropped.
#[test]
fn decode_save_bytes_carries_units_tags_and_markers() {
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    session.wavetime = 1234.5;

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    sim.spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
        .expect("spawn 1");
    sim.spawn_live_unit("dagger", 1, 48.0, 40.0, 0.0)
        .expect("spawn 2");
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("decode-units");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    let bytes = std::fs::read(&path).expect("save bytes");
    let mut content = boot_registry().expect("content");
    let save = decode_save_bytes(&bytes, &mut content).expect("decode");
    assert_eq!(save.entities.units.len(), 2, "two units decoded");
    assert_eq!(
        save.tags.get("wavetime").map(String::as_str),
        Some("1234.5"),
        "wavetime tag carried"
    );

    let _ = std::fs::remove_file(&path);
}

/// T9: revision 3 saves the real `spawnedByCore` flag, so a default-team
/// core-type unit WITHOUT the flag (a factory-made alpha) survives the
/// respawn; `UnitComp.java:868-871` only removes the flagged units (the
/// host spawns the player's own unit). The v1/v2 name fallback is covered
/// by `apply_decoded_entities_skips_spawned_by_core`.
#[test]
fn apply_decoded_entities_skips_core_units() {
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
    let session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let alpha = sim
        .spawn_live_unit("alpha", 1, 100.0, 100.0, 0.0)
        .expect("alpha");
    let _ = alpha;
    sim.spawn_live_unit("dagger", 1, 120.0, 100.0, 0.0)
        .expect("dagger");
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("decode-core-unit");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok);

    let bytes = std::fs::read(&path).expect("save bytes");
    let mut content = boot_registry().expect("content");
    let save = decode_save_bytes(&bytes, &mut content).expect("decode");
    assert_eq!(save.entities.units.len(), 2, "both units decoded");

    // Apply to a fresh sim: both respawn (no `spawnedByCore` flag).
    let mut dst = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    dst.install_block_runtime().expect("block runtime");
    dst.install_unit_runtime().expect("unit runtime");
    dst.ecs.0.insert_resource(CampaignRuntime::new(
        PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        }),
        Campaign::from_registry(&registry, &EmptyNeighborhood),
    ));
    dst.apply_decoded_entities(&save);
    let runtime = dst.unit_runtime().expect("runtime");
    let mut names = Vec::new();
    for entity in runtime.units.iter().copied() {
        if let Some(comp) = dst.ecs.0.get::<UnitTypeComp>(entity)
            && let Some(def) = runtime.content.unit(comp.type_id)
        {
            names.push(def.name.clone());
        }
    }
    assert_eq!(names, vec!["alpha".to_owned(), "dagger".to_owned()]);

    let _ = std::fs::remove_file(&path);
}

/// T9: `spawnedByCore` default-team units are skipped on respawn; v1/v2
/// saves (no flag) fall back to the core unit type names
/// (`UnitComp.java:868-871`).
#[test]
fn apply_decoded_entities_skips_spawned_by_core() {
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let unit = |name: &str, revision: u16, spawned_by_core: bool| DecodedUnit {
        revision,
        saved_id: -1,
        name: name.to_owned(),
        team: 1,
        x: 100.0,
        y: 100.0,
        rotation: 0.0,
        health: 150.0,
        payloads: Vec::new(),
        spawned_by_core,
        elevation: 0.0,
        shield: 0.0,
        stack: None,
        statuses: Vec::new(),
        command: None,
        controller: None,
        plans: Vec::new(),
        ability_data: Vec::new(),
        spawner_unit_id: -1,
    };
    let save_with = |units: Vec<DecodedUnit>| DecodedSave {
        grid: WorldGrid::new(0, 0),
        pending: Vec::new(),
        plans: Vec::new(),
        tags: StringMap::default(),
        rules: None,
        stats: None,
        markers: MapMarkers::default(),
        entities: LiveEntitySink {
            units,
            fires: Vec::new(),
            puddles: Vec::new(),
        },
        static_fog: None,
        map_gen_seed: None,
    };
    let mut dst = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    dst.install_block_runtime().expect("block runtime");
    dst.install_unit_runtime().expect("unit runtime");
    dst.ecs.0.insert_resource(CampaignRuntime::new(
        PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        }),
        Campaign::from_registry(&registry, &EmptyNeighborhood),
    ));
    // Revision 3: the flag skips the alpha; the dagger stays.
    dst.apply_decoded_entities(&save_with(vec![
        unit("alpha", 3, true),
        unit("dagger", 3, false),
    ]));
    let names: Vec<String> = dst
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .copied()
        .filter_map(|entity| {
            let comp = dst.ecs.0.get::<UnitTypeComp>(entity)?;
            dst.unit_runtime()?
                .content
                .unit(comp.type_id)
                .map(|def| def.name.clone())
        })
        .collect();
    assert_eq!(names, vec!["dagger".to_owned()]);

    // Revision 2 (no flag): the core type name skips the alpha.
    let mut legacy = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    legacy.install_block_runtime().expect("block runtime");
    legacy.install_unit_runtime().expect("unit runtime");
    legacy.ecs.0.insert_resource(CampaignRuntime::new(
        PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        }),
        Campaign::from_registry(&registry, &EmptyNeighborhood),
    ));
    legacy.apply_decoded_entities(&save_with(vec![
        unit("alpha", 2, false),
        unit("dagger", 2, false),
    ]));
    let names: Vec<String> = legacy
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .copied()
        .filter_map(|entity| {
            let comp = legacy.ecs.0.get::<UnitTypeComp>(entity)?;
            legacy
                .unit_runtime()?
                .content
                .unit(comp.type_id)
                .map(|def| def.name.clone())
        })
        .collect();
    assert_eq!(names, vec!["dagger".to_owned()]);
}

/// H04 T13: `UnitComp.afterRead` (`UnitComp.java:585-593`). A world load
/// spawns the saved units before the campaign session exists, so the spawn
/// fallback (`BuildRules.defaultTeam` = derelict) gives a player-team unit
/// the per-type AI. Once the session rules are installed,
/// `reset_loaded_controllers` re-derives `type.createController`
/// (`UnitType.java:281-283`): team `defaultTeam` -> `CommandAI`, wave team
/// -> `GroundAI`; a saved `Player` bridge is never replaced.
#[test]
fn loaded_player_team_units_get_command_ai() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let unit = |name: &str, team: u8, controller: Option<AiKind>| DecodedUnit {
        revision: 3,
        saved_id: -1,
        name: name.to_owned(),
        team,
        x: 100.0,
        y: 100.0,
        rotation: 0.0,
        health: 150.0,
        payloads: Vec::new(),
        spawned_by_core: false,
        elevation: 0.0,
        shield: 0.0,
        stack: None,
        statuses: Vec::new(),
        command: None,
        controller,
        plans: Vec::new(),
        ability_data: Vec::new(),
        spawner_unit_id: -1,
    };
    let save_with = |units: Vec<DecodedUnit>| DecodedSave {
        grid: WorldGrid::new(0, 0),
        pending: Vec::new(),
        plans: Vec::new(),
        tags: StringMap::default(),
        rules: None,
        stats: None,
        markers: MapMarkers::default(),
        entities: LiveEntitySink {
            units,
            fires: Vec::new(),
            puddles: Vec::new(),
        },
        static_fog: None,
        map_gen_seed: None,
    };
    let by_team = |sim: &Sim| -> Vec<(u8, AiKind)> {
        sim.unit_runtime()
            .expect("runtime")
            .units
            .iter()
            .copied()
            .filter_map(|entity| {
                let team = sim.ecs.0.get::<TeamComp>(entity)?.team;
                let kind = sim.ecs.0.get::<ControllerSlot>(entity)?.kind;
                Some((team, kind))
            })
            .collect()
    };

    // World-load order: no `CampaignRuntime` yet (`load_sector` runs before
    // `start_sector_with_loadout` installs the session).
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim.apply_decoded_entities(&save_with(vec![
        unit("dagger", 1, None),
        unit("dagger", 2, None),
        unit("dagger", 1, Some(AiKind::Player)),
    ]));
    // Fallback: every team but derelict reads as AI, so both daggers get
    // their per-type `GroundAI` (the bug state).
    assert!(by_team(&sim).iter().enumerate().all(|(i, (_, kind))| {
        if i == 2 {
            *kind == AiKind::Player
        } else {
            *kind == AiKind::Ground
        }
    }));

    // The session rules reach the world; Java's `afterRead` then re-derives
    // the controllers. A live campaign sector (`PlaySession.sector`) is
    // what makes wave team 2 an AI team (`Team.isAI`) even with
    // `rules.waves` false.
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    session.sector = Some((crate::content::PlanetId::new(0), 170));
    sim.ecs.0.insert_resource(CampaignRuntime::new(
        session,
        Campaign::from_registry(&registry, &EmptyNeighborhood),
    ));
    sim.reset_loaded_controllers();
    let kinds = by_team(&sim);
    assert!(
        kinds.contains(&(1, AiKind::Command)),
        "player team: {kinds:?}"
    );
    assert!(kinds.contains(&(2, AiKind::Ground)), "wave team: {kinds:?}");
    assert!(
        kinds.contains(&(1, AiKind::Player)),
        "player bridge: {kinds:?}"
    );

    // The stash is consumed: a second call changes nothing.
    sim.reset_loaded_controllers();
    assert_eq!(by_team(&sim), kinds);
}

/// H04 T13: a revision-3 unit whose saved controller keeps state
/// (`LogicAI`, `AIController.keepState`) survives `afterRead`.
#[test]
fn loaded_logic_unit_keeps_state() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let save = DecodedSave {
        grid: WorldGrid::new(0, 0),
        pending: Vec::new(),
        plans: Vec::new(),
        tags: StringMap::default(),
        rules: None,
        stats: None,
        markers: MapMarkers::default(),
        entities: LiveEntitySink {
            units: vec![DecodedUnit {
                revision: 3,
                saved_id: -1,
                name: "dagger".to_owned(),
                team: 1,
                x: 100.0,
                y: 100.0,
                rotation: 0.0,
                health: 150.0,
                payloads: Vec::new(),
                spawned_by_core: false,
                elevation: 0.0,
                shield: 0.0,
                stack: None,
                statuses: Vec::new(),
                command: None,
                controller: Some(AiKind::Logic),
                plans: Vec::new(),
                ability_data: Vec::new(),
                spawner_unit_id: -1,
            }],
            fires: Vec::new(),
            puddles: Vec::new(),
        },
        static_fog: None,
        map_gen_seed: None,
    };

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim.apply_decoded_entities(&save);
    sim.ecs.0.insert_resource(CampaignRuntime::new(
        PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        }),
        Campaign::from_registry(&registry, &EmptyNeighborhood),
    ));
    sim.reset_loaded_controllers();
    let entity = sim.unit_runtime().expect("runtime").units[0];
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(entity)
            .map(|slot| slot.kind),
        Some(AiKind::Logic)
    );
}

/// H04 T10: the reference client's Ground Zero save (v13 `MSAV`) entity
/// region decodes — the only chunk is the player's core alpha
/// (`spawnedByCore`), which `apply_decoded_entities` then skips
/// (`apply_decoded_entities_skips_core_units` contract: 0 units restored).
#[cfg(feature = "msav-import")]
#[test]
fn java_gz_fixture_skips_core_unit() {
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    registry.init().expect("init");
    registry.post_init().expect("post init");
    let path = format!(
        "{}/tests/fixtures/saves/java-gz-r05.msav",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).expect("java gz fixture");
    let save = decode_save_bytes(&bytes, &mut registry).expect("decode");
    assert_eq!(save.entities.units.len(), 1, "the core alpha decodes");
    assert_eq!(save.entities.units[0].name, "alpha");
    assert!(save.entities.units[0].spawned_by_core);
    assert_eq!(save.entities.units[0].team, 1);

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim.ecs.0.insert_resource(CampaignRuntime::new(
        PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        }),
        Campaign::from_registry(&registry, &EmptyNeighborhood),
    ));
    sim.apply_decoded_entities(&save);
    assert!(
        sim.unit_runtime().expect("runtime").units.is_empty(),
        "the core unit is skipped on apply"
    );
}

/// The live unit/fire/puddle chunk class ids must stay clear of the
/// upstream `classids.properties` table (append-only ABI).
#[test]
fn live_unit_class_id_is_reserved() {
    let mapping = crate::entities::mapping::EntityMapping::load_default();
    for id in [SIM_UNIT_CLASS_ID, SIM_FIRE_CLASS_ID, SIM_PUDDLE_CLASS_ID] {
        assert!(
            mapping.name_of(u16::from(id)).is_none(),
            "class id {id} collides with an upstream class id"
        );
    }
    assert_eq!(SIM_UNIT_REVISION, 4);
    assert_eq!(SIM_FIRE_REVISION, 1);
    assert_eq!(SIM_PUDDLE_REVISION, 1);
}

/// H04 T9: `entity_count` equals the chunks `write_entities` emits even
/// when a unit is filtered out; the reader then never eats the fire bytes
/// as a unit chunk (`SaveVersion.java:425-439`).
#[test]
fn entity_count_matches_written_chunks() {
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let first = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger 1");
    sim.spawn_live_unit("dagger", 2, 120.0, 100.0, 0.0)
        .expect("dagger 2");
    // Filtered out of the chunk stream: no `UnitCore`.
    sim.ecs.0.entity_mut(first).remove::<UnitCore>();
    sim.ecs.0.spawn((
        Pos { x: 104.0, y: 104.0 },
        crate::combat::fires::FireState {
            tile: (13, 13),
            time: 5.0,
            lifetime: 600.0,
            puddle_flammability: 0.0,
            spread_timer: 0.0,
            fireball_timer: 0.0,
            damage_timer: 0.0,
            warmup: 0.0,
            animation: 0.0,
        },
    ));
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("entity-count");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok);

    let bytes = std::fs::read(&path).expect("save bytes");
    let mut content = boot_registry().expect("content");
    let save = decode_save_bytes(&bytes, &mut content).expect("decode");
    assert_eq!(save.entities.units.len(), 1, "only the live dagger");
    assert_eq!(save.entities.fires.len(), 1, "the fire chunk follows");

    let mut dst = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    dst.install_block_runtime().expect("block runtime");
    dst.install_unit_runtime().expect("unit runtime");
    dst.apply_decoded_entities(&save);
    assert_eq!(
        dst.unit_runtime().expect("runtime").units.len(),
        1,
        "one dagger restored"
    );

    let _ = std::fs::remove_file(&path);
}

/// H04 T9: revision 3 round-trips elevation, the carried item, statuses,
/// shield, the command id + move target and the builder plan queue.
#[test]
fn unit_rev3_round_trip() {
    use crate::ai::controller::UnitCommandComp;
    use crate::entities::comp::unit::comp::{ItemsComp, ShieldComp, StatusComp, StatusEntry};
    use crate::sim::builder::{BuilderPlans, UnitBuildPlan};

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let entity = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    let content = boot_registry().expect("content");
    let copper = content.item_id("copper").expect("copper");
    let wet = content.status_id("wet").expect("wet");
    if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(entity) {
        core.elevation = 0.5;
    }
    sim.ecs.0.entity_mut(entity).insert(ItemsComp {
        item: Some((copper, 10)),
    });
    sim.ecs.0.entity_mut(entity).insert(ShieldComp {
        shield: 20.0,
        ..Default::default()
    });
    sim.ecs.0.entity_mut(entity).insert(StatusComp {
        statuses: vec![StatusEntry::new(wet, 120.0)],
        damage_times: vec![0.0],
        ..Default::default()
    });
    sim.ecs.0.entity_mut(entity).insert(UnitCommandComp(3));
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(entity)
    {
        slot.kind = crate::ai::controller::AiKind::Logic;
    } else {
        sim.ecs
            .0
            .entity_mut(entity)
            .insert(crate::ai::controller::ControllerSlot::new(
                crate::ai::controller::AiKind::Logic,
            ));
    }
    sim.ecs.0.entity_mut(entity).insert(BuilderPlans {
        plans: std::collections::VecDeque::from([UnitBuildPlan::place(
            10,
            12,
            1,
            BlockId::STONE_WALL,
        )]),
        ..Default::default()
    });
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("unit-rev3");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok);

    let bytes = std::fs::read(&path).expect("save bytes");
    let mut content = boot_registry().expect("content");
    let save = decode_save_bytes(&bytes, &mut content).expect("decode");
    assert_eq!(save.entities.units.len(), 1);
    let unit = &save.entities.units[0];
    assert_eq!(unit.revision, SIM_UNIT_REVISION);
    assert_eq!(unit.elevation, 0.5);
    assert_eq!(unit.stack, Some((copper, 10)));
    assert_eq!(unit.shield, 20.0);
    assert_eq!(unit.statuses.len(), 1);
    assert_eq!(unit.statuses[0].effect, wet);
    assert_eq!(
        unit.statuses[0].duration, 119.0,
        "one capture tick elapsed before the save"
    );
    assert_eq!(unit.command, Some(3));
    assert_eq!(unit.controller, Some(crate::ai::controller::AiKind::Logic));
    assert_eq!(unit.plans.len(), 1);
    assert_eq!(unit.plans[0].block, BlockId::STONE_WALL);
    assert_eq!((unit.plans[0].x, unit.plans[0].y), (10, 12));

    let _ = std::fs::remove_file(&path);
}

/// A live save must carry real meta (`tick`/`playtime`) and the entities
/// region must round-trip the placed building and the live unit: load
/// materializes the building into the ECS runtime and respawns the unit.
/// L06 T4: unit revision 4 round-trips `Ability.data` (`TypeIO.writeAbilities`
/// / `readAbilities`, TypeIO.java:429-451, copied index-aligned by
/// `UnitComp.setType`, UnitComp.java:549-557) and the tether
/// (`UnitTetherComp.spawnerUnitId`, resolved in `afterRead`,
/// UnitTetherComp.java:17-24): the child stays tethered to its spawner.
#[test]
fn unit_abilities_and_tether_round_trip() {
    use crate::combat::abilities::AbilityComp;
    use crate::entities::comp::unit::comp::UnitTetherComp;
    let mut sim = Sim::new(1, 48, 48, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let oct = sim
        .spawn_live_unit("oct", 2, 120.0, 120.0, 0.0)
        .expect("oct");
    let child = sim
        .spawn_live_unit("flare", 2, 140.0, 120.0, 0.0)
        .expect("flare");
    let ability_count = {
        let mut comp = sim
            .ecs
            .0
            .get_mut::<AbilityComp>(oct)
            .expect("oct abilities");
        for (index, state) in comp.states.iter_mut().enumerate() {
            state.data = 10.0 + index as f32;
        }
        comp.states.len()
    };
    assert!(ability_count > 0, "oct carries abilities");
    sim.ecs
        .0
        .entity_mut(child)
        .insert(UnitTetherComp { spawner: Some(oct) });
    let oct_seq = sim.ecs.0.get::<crate::ecs::EntitySeq>(oct).expect("seq").0;
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
    let path = temp_path("abilities-tether-roundtrip");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    assert_eq!(sim.take_io_responses()[0].status, IoStatus::Ok);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    assert_eq!(sim.take_io_responses()[0].status, IoStatus::Ok);

    let find = |sim: &Sim, name: &str| {
        let runtime = sim.unit_runtime().expect("runtime");
        let id = runtime.content.unit_id(name).expect("type");
        sim.ecs
            .0
            .iter_entities()
            .find(|entity| {
                entity
                    .get::<crate::entities::comp::unit::comp::UnitTypeComp>()
                    .is_some_and(|comp| comp.type_id == id)
            })
            .map(|entity| entity.id())
            .expect(name)
    };
    let oct = find(&sim, "oct");
    let child = find(&sim, "flare");
    let data: Vec<f32> = sim
        .ecs
        .0
        .get::<AbilityComp>(oct)
        .expect("abilities")
        .states
        .iter()
        .map(|state| state.data)
        .collect();
    let expected: Vec<f32> = (0..ability_count).map(|i| 10.0 + i as f32).collect();
    assert_eq!(data, expected, "Ability.data restored");
    let spawner = sim
        .ecs
        .0
        .get::<UnitTetherComp>(child)
        .and_then(|tether| tether.spawner)
        .expect("still tethered");
    assert_eq!(spawner, oct);
    assert_eq!(
        sim.ecs.0.get::<crate::ecs::EntitySeq>(spawner).map(|s| s.0),
        Some(oct_seq),
        "the restored spawner id"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn live_sim_save_then_load_restores_entities_and_meta() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let core = sim.content().id("core-shard").expect("core-shard");
    sim.apply(Command::Place {
        x: 5,
        y: 5,
        block: core,
    })
    .expect("place core");
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    sim.spawn_live_unit("dagger", 1, 64.0, 64.0, 0.0)
        .expect("spawn unit");
    sim.set_save_playtime(4321);
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("live-entities-roundtrip");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    let meta = SaveIo::get_meta(&NativeFs, &path).expect("meta");
    assert_eq!(meta.time_played, 4321, "host playtime lands in the meta");
    let saved_tick: u64 = meta
        .tags
        .get("tick")
        .and_then(|tick| tick.parse().ok())
        .expect("tick tag");
    assert!(saved_tick > 0, "tick tag is live: {saved_tick}");

    // Wipe the live world; the load must rebuild it from the file.
    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");
    assert_eq!(sim.grid.block_at(TilePos::new(5, 5)), Some(core));
    assert_eq!(sim.group_counts().get("build"), Some(&1));
    assert_eq!(sim.group_counts().get("unit"), Some(&1));
    assert!(sim.tick_count() >= saved_tick, "tick header resumes");

    let _ = std::fs::remove_file(&path);
}

/// PAR-0249: a core holding `i32::MAX / 2` item stacks (the `fillItems`
/// value under `coreIncinerates`, `CoreBlock.java:749-751`) saves and
/// reloads without panicking; `ItemModule.total` wraps like Java's `int`
/// (`ItemModule.java:319-334`).
#[test]
fn live_sim_save_then_load_keeps_oversized_item_stacks() {
    use crate::world::modules::ItemModule;

    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let core = sim.content().id("core-shard").expect("core-shard");
    sim.apply(Command::Place {
        x: 5,
        y: 5,
        block: core,
    })
    .expect("place core");
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
    let content = boot_registry().expect("content");
    let copper = content.item_id("copper").expect("copper");
    let lead = content.item_id("lead").expect("lead");
    let graphite = content.item_id("graphite").expect("graphite");
    let silicon = content.item_id("silicon").expect("silicon");
    let huge = i32::MAX / 2;
    // Four `i32::MAX / 2` stacks sum past `i32::MAX`: the plain `+=` the
    // decoder used panicked here in debug builds (the H03 crash report).
    let expected_total = huge
        .wrapping_add(huge)
        .wrapping_add(huge)
        .wrapping_add(huge);
    {
        let mut items = sim.ecs.0.get_mut::<ItemModule>(entity).expect("items");
        items.set(copper, huge);
        items.set(lead, huge);
        items.set(graphite, huge);
        items.set(silicon, huge);
        assert_eq!(items.total(), expected_total, "Java int wrap (set)");
    }
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("live-oversized-items-roundtrip");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    // Wipe the live world; the load rebuilds the core and must not panic
    // on the overflow the sum of the two stacks produces in `apply_base`.
    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let entity = sim
        .grid
        .entity_at(TilePos::new(5, 5))
        .expect("core reloaded");
    let items = sim.ecs.0.get::<ItemModule>(entity).expect("items");
    assert_eq!(items.get(copper), huge, "stack round-trips");
    assert_eq!(items.get(lead), huge, "stack round-trips");
    assert_eq!(items.get(graphite), huge, "stack round-trips");
    assert_eq!(items.get(silicon), huge, "stack round-trips");
    assert_eq!(items.total(), expected_total, "wrapped total round-trips");
    // The live module keeps working after the oversized load.
    let mut items = sim.ecs.0.get_mut::<ItemModule>(entity).expect("items");
    assert_eq!(items.remove(copper, 1), 1);
    assert_eq!(items.total(), expected_total.wrapping_sub(1));

    let _ = std::fs::remove_file(&path);
}

/// H09 T11: the `static-fog-data` custom chunk (`FogControl.java:97,
/// 437-505`) round-trips a live save: the reloaded fog has the same
/// discovered tiles as the saved one.
#[test]
fn live_save_keeps_static_fog() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let rules = Rules {
        default_team: 1,
        waves: true,
        fog: true,
        static_fog: true,
        ..Rules::default()
    };
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
    sim.reset_fog();
    let _dagger = sim
        .spawn_live_unit("dagger", 1, 20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let sample: Vec<(i32, i32)> = (18..23)
        .flat_map(|y| (18..23).map(move |x| (x, y)))
        .collect();
    let before: Vec<bool> = sample
        .iter()
        .map(|(x, y)| sim.fog_discovered(1, *x, *y))
        .collect();
    assert!(
        before.iter().any(|discovered| *discovered),
        "the unit discovered tiles"
    );
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("live-static-fog");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    // Wipe the live world; the load must restore the fog from the chunk.
    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");
    for (index, (x, y)) in sample.iter().enumerate() {
        assert_eq!(
            sim.fog_discovered(1, *x, *y),
            before[index],
            "discovered ({x}, {y}) round-trips"
        );
    }

    let _ = std::fs::remove_file(&path);
}

/// PAR-0138: a live save round-trips the carried payload stack (revision 2
/// unit chunk); a v1 save without payloads still loads.
#[test]
fn live_save_keeps_unit_payloads() {
    use crate::entities::comp::unit::comp::{CarriedPayload, UnitPayloads};

    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    sim.spawn_live_unit("dagger", 1, 110.0, 100.0, 0.0)
        .expect("dagger");
    assert!(
        sim.apply_payload_command(0, 0.0, 0.0, None),
        "the dagger is picked up"
    );
    let before = sim
        .ecs
        .0
        .get::<UnitPayloads>(mega)
        .expect("payloads")
        .stack
        .clone();
    assert!(matches!(before.as_slice(), [CarriedPayload::Unit { .. }]));

    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
    let path = temp_path("live-unit-payloads");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    // Wipe the live world; the load must restore the carried stack.
    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let mega_id = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("mega")
        .expect("mega id");
    let loaded = sim
        .unit_runtime()
        .expect("rt")
        .units
        .iter()
        .copied()
        .find(|entity| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(*entity)
                .is_some_and(|comp| comp.type_id == mega_id)
        })
        .expect("mega respawned");
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitPayloads>(loaded)
            .map(|payloads| payloads.stack.clone()),
        Some(before),
        "the carried stack survived the round-trip"
    );

    let _ = std::fs::remove_file(&path);
}

/// GAPS #23 / PAR-0518: a unit carried by a payload block round-trips a
/// live save. The nested `Payload.write` unit body is the only place the
/// carried unit exists in the save (it is not a `Groups.unit` row), so
/// before this fix the payload block read stopped and the unit was lost.
#[test]
fn live_save_keeps_block_unit_payload() {
    use crate::determinism::{ConfigValue, SimCommand};
    use crate::entities::comp::unit::comp::UnitTypeComp;
    use crate::entities::comp::{Health, Unit};
    use crate::world::blocks::payloads::{CarriedUnit, PayloadHolder};

    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let source = sim
        .world_block_id("payload-source")
        .expect("payload-source");
    sim.command(SimCommand::Place {
        x: 5,
        y: 5,
        block: source,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("place source");
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_id("dagger")
        .expect("dagger");
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: ConfigValue::Content("dagger".to_owned()),
    })
    .expect("configure dagger");
    // One update creates the carried payload; it takes ~17 ticks to move
    // out, so the save below captures it still held.
    sim.tick().expect("tick");
    let source_e = sim
        .grid
        .entity_at(TilePos::new(5, 5))
        .expect("source entity");
    let payload = sim.held_payload(source_e).expect("carried payload");
    assert!(!payload.is_block);
    let entity = payload.entity.expect("payload entity");
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitTypeComp>(entity)
            .map(|comp| comp.type_id),
        Some(dagger)
    );
    // Make the restored health distinguishable from a freshly spawned one.
    sim.ecs
        .0
        .get_mut::<Health>(entity)
        .expect("payload health")
        .health = 30.0;
    assert!(
        sim.ecs.0.get::<Unit>(entity).is_none(),
        "carried unit is not in Groups.unit before the save"
    );

    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
    let path = temp_path("live-block-unit-payload");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    // Wipe the live world; the load must restore the carried unit.
    sim.grid.fill(BlockId::AIR, BlockId::AIR);
    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let source_e = sim
        .grid
        .entity_at(TilePos::new(5, 5))
        .expect("source reloaded");
    let payload = sim
        .held_payload(source_e)
        .expect("the payload block kept its payload");
    assert!(!payload.is_block, "unit payload restored");
    assert_eq!(payload.content, dagger.raw());
    let entity = payload.entity.expect("read entity");
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitTypeComp>(entity)
            .map(|comp| comp.type_id),
        Some(dagger),
        "unit type round-trips"
    );
    assert_eq!(
        sim.ecs.0.get::<Health>(entity).map(|health| health.health),
        Some(30.0),
        "unit health round-trips"
    );
    assert!(
        sim.ecs.0.get::<Unit>(entity).is_none(),
        "a restored carried unit is still outside Groups.unit"
    );
    assert!(
        sim.ecs.0.get::<CarriedUnit>(entity).is_some(),
        "carried marker restored"
    );
    assert!(
        sim.ecs
            .0
            .get::<PayloadHolder>(source_e)
            .is_some_and(|holder| holder.payload == Some(payload))
    );

    let _ = std::fs::remove_file(&path);
}

/// PAR-0090: a live save keeps fires and puddles and the load restores
/// them with the same tile/amount/time.
#[test]
fn live_sim_save_keeps_fires_and_puddles() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let mut rng = crate::determinism::SimRng::new(5);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 7, 7, &mut rng).expect("fire");
    {
        let world = &mut sim.ecs.0;
        let mut runtime = world.remove_non_send::<UnitRuntime>().expect("runtime");
        let water = runtime.content.liquid_id("water").expect("water");
        let puddle = crate::combat::puddles::deposit(
            world,
            &sim.grid.tiles,
            &runtime.content,
            9,
            9,
            water,
            30.0,
            &mut runtime.rng,
        );
        assert!(puddle.is_some(), "puddle deposited");
        world.insert_non_send(runtime);
    }
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("live-fires-puddles-roundtrip");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);
    let fire_before = sim
        .ecs
        .0
        .get::<crate::combat::fires::FireState>(fire)
        .copied()
        .expect("fire state");
    let puddle_before = sim
        .ecs
        .0
        .iter_entities()
        .find_map(|entity| entity.get::<crate::combat::puddles::PuddleState>().copied())
        .expect("puddle state");

    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let fires: Vec<crate::combat::fires::FireState> = sim
        .ecs
        .0
        .iter_entities()
        .filter_map(|entity| entity.get::<crate::combat::fires::FireState>().copied())
        .collect();
    assert_eq!(fires.len(), 1, "one fire restored");
    assert_eq!(fires[0].tile, fire_before.tile);
    // One tick (the load tick) advances the restored fire: 1 + the
    // bare-ground 8 (`FireComp.java:67-69`).
    assert!(
        (fires[0].time - fire_before.time - 9.0).abs() < 0.001,
        "fire time resumes ({} vs {})",
        fires[0].time,
        fire_before.time
    );
    let puddles: Vec<crate::combat::puddles::PuddleState> = sim
        .ecs
        .0
        .iter_entities()
        .filter_map(|entity| entity.get::<crate::combat::puddles::PuddleState>().copied())
        .collect();
    assert_eq!(puddles.len(), 1, "one puddle restored");
    assert_eq!(puddles[0].tile, puddle_before.tile);
    assert_eq!(puddles[0].liquid, puddle_before.liquid);
    assert!(
        puddles[0].amount <= puddle_before.amount,
        "puddle amount resumes ({} <= {})",
        puddles[0].amount,
        puddle_before.amount
    );

    let _ = std::fs::remove_file(&path);
}

/// I01 T10: the unit's `EntitySeq` (written as the chunk id,
/// `SaveVersion.writeEntity`: `out.i(entity.id())`) survives a save/load;
/// `unit_by_id` resolves it and the allocators move past the restored ids
/// (`EntityGroup.checkNextId`).
#[test]
fn unit_ids_round_trip_through_save() {
    let sim_seq = |sim: &Sim, entity| sim.ecs.0.get::<EntitySeq>(entity).map(|seq| seq.0);
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    let first = sim
        .spawn_live_unit("dagger", 2, 40.0, 40.0, 0.0)
        .expect("first dagger");
    let second = sim
        .spawn_live_unit("flare", 2, 48.0, 40.0, 0.0)
        .expect("flare");
    let first_seq = sim_seq(&sim, first).expect("first seq");
    let second_seq = sim_seq(&sim, second).expect("second seq");
    assert_ne!(first_seq, second_seq, "distinct ids");
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("unit-ids");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one load response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    // Every restored unit kept its saved id, in saved order.
    let restored: Vec<(u64, String)> = sim
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .copied()
        .filter_map(|entity| {
            let seq = sim.ecs.0.get::<EntitySeq>(entity)?.0;
            let name = sim
                .unit_runtime()?
                .content
                .unit(sim.ecs.0.get::<UnitTypeComp>(entity)?.type_id)?
                .name
                .clone();
            Some((seq, name))
        })
        .collect();
    assert_eq!(
        restored,
        vec![
            (first_seq, "dagger".to_owned()),
            (second_seq, "flare".to_owned())
        ],
        "ids and order survive"
    );
    assert_eq!(
        crate::sim::unit_runtime::unit_by_id(&sim.ecs.0, first_seq as i32)
            .and_then(|entity| sim_seq(&sim, entity)),
        Some(first_seq),
        "unit_by_id resolves the saved id"
    );

    // The allocators moved past the restored ids: a new spawn cannot reuse
    // either id (`EntityGroup.checkNextId`).
    let max = first_seq.max(second_seq);
    let third = sim
        .spawn_live_unit("dagger", 2, 56.0, 40.0, 0.0)
        .expect("third dagger");
    let third_seq = sim_seq(&sim, third).expect("third seq");
    assert!(third_seq > max, "allocator bumped ({third_seq} > {max})");

    let _ = std::fs::remove_file(&path);
}

/// I01 T10: `UnitCargoLoaderBuild`'s save body (the tethered unit id,
/// `UnitCargoLoader.java:174-185`) re-tethers the unit after a load through
/// `readUnitId` + `Groups.unit.getByID` (here `unit_by_id`).
#[test]
fn cargo_loader_relinks_unit_after_load() {
    let sim_seq = |sim: &Sim, entity| sim.ecs.0.get::<EntitySeq>(entity).map(|seq| seq.0);
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    // Place on team 1: derelict (team 0) buildings are disabled by
    // `checkAllowUpdate` and would never reach `updateTile`.
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;
    let loader = sim
        .content()
        .id("unit-cargo-loader")
        .expect("unit-cargo-loader");
    sim.apply(Command::Place {
        x: 8,
        y: 8,
        block: loader,
    })
    .expect("place loader");
    let loader_entity = sim.grid.tile(8, 8).build.expect("loader entity");
    let manifold = sim
        .spawn_live_unit("manifold", 2, 64.0, 64.0, 90.0)
        .expect("manifold");
    let manifold_seq = sim_seq(&sim, manifold).expect("manifold seq");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::world::blocks::units::CargoLoaderState>(loader_entity)
            .expect("loader state");
        state.unit = Some(manifold);
    }
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("cargo-loader-relink");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok);

    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let loader_entity = sim.grid.tile(8, 8).build.expect("loader respawned");
    let state = sim
        .ecs
        .0
        .get::<crate::world::blocks::units::CargoLoaderState>(loader_entity)
        .expect("loader state");
    assert_eq!(
        state.read_unit_id, -1,
        "pending read id resolved on updateTile"
    );
    let relinked = state.unit.expect("unit relinked after load");
    assert_eq!(
        sim_seq(&sim, relinked),
        Some(manifold_seq),
        "relinked to the saved unit id"
    );

    let _ = std::fs::remove_file(&path);
}

/// I01 T10: the cargo loader body stays stream-aligned with the following
/// building — a save holding a loader and a drill restores BOTH bodies
/// (H02 T13 recorded the unimplemented body as a "misreads the following
/// building" blocker).
#[test]
fn save_with_cargo_loader_reads_next_building() {
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;
    let loader = sim
        .content()
        .id("unit-cargo-loader")
        .expect("unit-cargo-loader");
    let drill = sim
        .content()
        .id("mechanical-drill")
        .expect("mechanical-drill");
    sim.apply(Command::Place {
        x: 8,
        y: 8,
        block: loader,
    })
    .expect("place loader");
    sim.apply(Command::Place {
        x: 12,
        y: 12,
        block: drill,
    })
    .expect("place drill");
    let drill_entity = sim.grid.tile(12, 12).build.expect("drill entity");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::entities::comp::building::DrillState>(drill_entity)
            .expect("drill state");
        state.progress = 77.5;
    }
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("cargo-loader-next-building");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok);

    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let drill_entity = sim.grid.tile(12, 12).build.expect("drill respawned");
    let state = sim
        .ecs
        .0
        .get::<crate::entities::comp::building::DrillState>(drill_entity)
        .expect("drill state survived the loader body");
    assert!(
        (state.progress - 77.5).abs() < 1e-6,
        "drill body aligned after the loader body: {}",
        state.progress
    );

    let _ = std::fs::remove_file(&path);
}

/// I01 T11: the assembler save body (`UnitAssembler.java:756-789`):
/// `progress`, the drone id list (resolved through the restored unit ids),
/// the `blocks` `PayloadSeq` and `commandPos` (revision 1).
#[test]
fn assembler_save_round_trip() {
    use crate::world::blocks::units::{AssemblerPayload, UnitAssembler, payload_content_ref};

    let sim_seq = |sim: &Sim, entity| sim.ecs.0.get::<EntitySeq>(entity).map(|seq| seq.0);
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;
    let assembler_block = sim.content().id("tank-assembler").expect("tank-assembler");
    sim.apply(Command::Place {
        x: 12,
        y: 12,
        block: assembler_block,
    })
    .expect("place assembler");
    let assembler_entity = sim.grid.tile(12, 12).build.expect("assembler entity");
    let drone = sim
        .spawn_live_unit("assembly-drone", 2, 96.0, 96.0, 0.0)
        .expect("drone");
    let drone_seq = sim_seq(&sim, drone).expect("drone seq");
    let content = boot_registry().expect("content");
    let stell = content.unit_id("stell").expect("stell").raw();
    let wall = content
        .block_id("tungsten-wall-large")
        .expect("tungsten-wall-large")
        .raw();
    {
        let mut assembler = sim
            .ecs
            .0
            .get_mut::<UnitAssembler>(assembler_entity)
            .expect("assembler state");
        assembler.units = vec![drone];
        assembler.progress = 0.42;
        assembler.command_pos = Some((123.0, 456.0));
        // Partial requirements: the loaded tick must not advance progress.
        assembler.stored.add(
            payload_content_ref(AssemblerPayload {
                is_block: false,
                content: stell,
                amount: 0,
            }),
            3,
        );
        assembler.stored.add(
            payload_content_ref(AssemblerPayload {
                is_block: true,
                content: wall,
                amount: 0,
            }),
            2,
        );
    }
    sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

    let path = temp_path("assembler-roundtrip");
    let _ = std::fs::remove_file(&path);
    sim.request_save(&path, false);
    sim.tick().expect("capture tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses.len(), 1, "one save response: {responses:?}");
    assert_eq!(responses[0].status, IoStatus::Ok);

    sim.request_load(&path);
    sim.tick().expect("apply tick");
    let responses = sim.take_io_responses();
    assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

    let assembler_entity = sim.grid.tile(12, 12).build.expect("assembler respawned");
    let state = sim
        .ecs
        .0
        .get::<UnitAssembler>(assembler_entity)
        .expect("assembler respawned");
    assert!(
        (state.progress - 0.42).abs() < 1e-6,
        "progress restored ({})",
        state.progress
    );
    assert!(state.read_unit_ids.is_empty(), "read ids resolved");
    assert_eq!(state.units.len(), 1, "one drone restored");
    assert_eq!(
        sim_seq(&sim, state.units[0]),
        Some(drone_seq),
        "drone id restored through the save body"
    );
    assert_eq!(
        state.command_pos,
        Some((123.0, 456.0)),
        "commandPos restored"
    );
    assert_eq!(
        state.stored.get(payload_content_ref(AssemblerPayload {
            is_block: false,
            content: stell,
            amount: 0,
        })),
        3,
        "unit payload count restored"
    );
    assert_eq!(
        state.stored.get(payload_content_ref(AssemblerPayload {
            is_block: true,
            content: wall,
            amount: 0,
        })),
        2,
        "block payload count restored"
    );
    assert_eq!(state.stored.total(), 5);

    let _ = std::fs::remove_file(&path);
}
