// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live world-processor regressions, first half: rules, flags, events and spawnwave.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// G06 T1 (PAR-0106): the live sim refreshes the logic globals every tick
/// before entity updates, so `@tick` equals the tick the instruction runs in.
#[test]
fn live_tick_global_advances() {
    let mut sim = runtime_sim();
    let globals = {
        let content = sim.block_runtime().expect("runtime").content();
        crate::logic::globals::GlobalVars::with_content(content)
    };
    sim.ecs.0.insert_resource(globals);
    let id = sim.world_block_id("micro-processor").expect("block");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let bytes = crate::logic::blocks::logic_block::compress("set t @tick\n", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let state = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(processor)
        .expect("state");
    let t = state.executor.optional_var("t").expect("t");
    // `set` runs as the first of the micro-processor's 2 ipt each tick, in the
    // tick where `@tick = state.tick + 1` (Logic.java:526-546).
    assert_eq!(state.executor.arena.get(t).num(), 30.0);
}

/// `setrule` writes the live campaign session rules (not a dead copy).
#[test]
fn setrule_writes_live_rules() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule waveSending true 0 0 100 100\nsetrule unitCap 4 0 0 100 100\nstop\n",
    );
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(runtime.session.rules.wave_sending);
    assert_eq!(runtime.session.rules.unit_cap, 4);
}

/// Ground Zero block 2: the tutorial flag is consumed and waves enabled.
#[test]
fn flags_roundtrip_live() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .rules
        .objective_flags
        .insert("allowWaves".to_owned());
    place_world_processor(
        &mut sim,
        10,
        10,
        "getflag waves \"allowWaves\"\njump 0 equal waves false\nsetflag \"allowWaves\" false\nsetrule waveSending true 0 0 100 100\nsetrule waveTimer true 0 0 100 100\nstop\n",
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(!runtime.session.rules.objective_flags.contains("allowWaves"));
    assert!(runtime.session.rules.wave_sending);
    assert!(runtime.session.rules.wave_timer);
}

/// `setrule mapArea` updates the session and, through G04 T5, `grid.limit_area`.
#[test]
fn map_area_rule_toggles_live_limit() {
    let mut sim = Sim::new(3, 300, 300, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    install_campaign_session(&mut sim, test_rules());
    // Java `SetRuleI`: the `value` slot is unused for `mapArea`; x/y/w/h come
    // from p1..p4 (`setrule mapArea 10 0 0 256 256` -> x=0, w=256).
    place_world_processor(&mut sim, 10, 10, "setrule mapArea 10 0 0 256 256\nstop\n");
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    {
        let runtime = sim
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .expect("runtime");
        assert!(runtime.session.rules.limit_map_area);
        assert_eq!(runtime.session.rules.limit_width, 256);
    }
    assert_eq!(sim.grid.limit_area, Some((0, 0, 256, 256)));
    // Selecting the whole map disables the limit again.
    let bytes =
        crate::logic::blocks::logic_block::compress("setrule mapArea 0 0 0 300 300\nstop\n", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(!runtime.session.rules.limit_map_area);
    assert_eq!(sim.grid.limit_area, None);
}

/// `setrule wave` clamps to >= 1; `currentWaveTime` converts seconds to ticks.
#[test]
fn wave_rule_sets_wave_and_wavetime() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule wave 0 0 0 100 100\nsetrule currentWaveTime 2.5 0 0 100 100\nstop\n",
    );
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(runtime.session.wave, 1);
    assert_eq!(runtime.session.wavetime, 150.0);
}

/// The event log never grows: `Sim::tick` drains it every tick.
#[test]
fn logic_events_drain_each_tick() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState::new());
    place_world_processor(&mut sim, 10, 10, "setflag \"drained\" true\nstop\n");
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(runtime.session.rules.objective_flags.contains("drained"));
    let state = sim
        .ecs
        .0
        .get_resource::<crate::logic::world::LogicWorldState>()
        .expect("state");
    assert!(state.events.is_empty(), "{:?}", state.events);
}

/// H06 T1: `state_snapshot` lists the Java variables view of a live
/// processor (numbers, no constants, no `@`-names).
#[test]
fn logic_state_snapshot_lists_vars() {
    let mut sim = runtime_sim();
    place(&mut sim, "micro-processor", 10, 10);
    let bytes = crate::logic::blocks::logic_block::compress("set a 1\nop add b a 2", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let snapshot =
        crate::logic::blocks::logic_block::state_snapshot(&sim.ecs.0, e).expect("snapshot");
    assert!(
        snapshot
            .vars
            .iter()
            .any(|(name, value, type_)| name == "a" && value == "1" && *type_ == "number"),
        "{:?}",
        snapshot.vars
    );
    assert!(
        snapshot
            .vars
            .iter()
            .any(|(name, value, type_)| name == "b" && value == "3" && *type_ == "number"),
        "{:?}",
        snapshot.vars
    );
    assert!(
        !snapshot
            .vars
            .iter()
            .any(|(name, _, _)| name.starts_with('@')),
        "{:?}",
        snapshot.vars
    );
    assert_eq!(snapshot.ipt, 2);
    assert!(!snapshot.privileged);
    assert!(snapshot.accessible);
}

/// H06 T20: a processor decoded before `GlobalVars` exists assembles
/// `@sharded`/`@duo` as null variables; re-assembling after the globals
/// arrive resolves them (the fixed engine load order: `Vars.logicVars`
/// exists before any map/save is read, `Vars.java:369`).
#[test]
fn processor_loaded_before_globals_resolves_constants() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::logic::globals::GlobalVars>()
            .is_none(),
        "runtime_sim carries no GlobalVars"
    );
    place_world_processor(
        &mut sim,
        10,
        10,
        "fetch buildCount n @sharded 0 @duo\nstop\n",
    );
    {
        let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
        let state = sim
            .ecs
            .0
            .get::<crate::logic::blocks::LogicBlockState>(e)
            .expect("state");
        let sharded = state
            .executor
            .optional_var("@sharded")
            .expect("@sharded assembled");
        assert!(
            !state.executor.arena.get(sharded).constant,
            "without globals the content constant is a plain variable"
        );
    }
    // Fixed load order: content globals exist before the buildings decode.
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
    crate::logic::blocks::logic_block::reassemble_all_processors(&mut sim.ecs.0);
    place(&mut sim, "duo", 20, 20);
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let state = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(e)
        .expect("state");
    let mut bad = Vec::new();
    for id in &state.executor.var_ids {
        let cell = state.executor.arena.get(*id);
        if cell.name.starts_with('@') && !cell.constant && cell.name != "@counter" {
            bad.push(cell.name.clone());
        }
    }
    assert!(bad.is_empty(), "unresolved constants: {bad:?}");
    let n = state
        .executor
        .optional_var("n")
        .map(|id| state.executor.arena.get(id).num());
    assert_eq!(n, Some(1.0), "buildCount sees the team-1 duo");
}

/// H06 T2: the world processor's `spawnwave natural` runs one live wave
/// (`SpawnWaveI` -> `Logic.skipWave` -> `runWave`,
/// `J/logic/LExecutor.java:2173-2175`, `Logic.java:315-325`).
#[test]
fn spawnwave_natural_runs_wave() {
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = test_rules();
    rules.spawns.push(crate::io::json::SpawnGroup {
        type_: "dagger".to_owned(),
        begin: 0,
        unit_amount: 1,
        ..Default::default()
    });
    let wave_spacing = rules.wave_spacing;
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    let spawn = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("spawn")
        .expect("spawn");
    let index = sim.grid.tiles.index(6, 6);
    sim.grid.tiles.geti_mut(index).overlay = spawn;
    place_world_processor(&mut sim, 10, 10, "spawnwave natural 10 true\nstop\n");
    let before = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .wave;
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(runtime.session.wave, before + 1, "one wave advanced");
    assert_eq!(
        runtime.session.wavetime, wave_spacing,
        "countdown reset to waveSpacing (non-campaign multiplier 1)"
    );
    // The queued `spawnEnemies` runs in the next `run_wave_system`.
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let count = sim
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .filter(|e| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::UnitTypeComp>(**e)
                .map(|c| c.type_id)
                == Some(dagger)
                && sim
                    .ecs
                    .0
                    .get::<crate::entities::comp::TeamComp>(**e)
                    .map(|t| t.team)
                    == Some(2)
        })
        .count();
    assert_eq!(count, 1, "one crux dagger spawned");
    assert!(
        sim.logic_event_log.iter().any(|kind| kind == "spawnwave"),
        "{:?}",
        sim.logic_event_log
    );
}

/// H06 T2: Ground Zero GZ-1 verbatim — the flag is consumed, one wave runs,
/// and later ticks do not run more waves (the program falls off its end).
#[test]
fn gz1_spawn_wave_flag() {
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = test_rules();
    rules.spawns.push(crate::io::json::SpawnGroup {
        type_: "dagger".to_owned(),
        begin: 0,
        unit_amount: 1,
        ..Default::default()
    });
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    let spawn = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("spawn")
        .expect("spawn");
    let index = sim.grid.tiles.index(6, 6);
    sim.grid.tiles.geti_mut(index).overlay = spawn;
    sim.ecs
        .0
        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .rules
        .objective_flags
        .insert("spawnWave".to_owned());
    place_world_processor(
        &mut sim,
        10,
        10,
        "getflag wave \"spawnWave\"\njump 0 equal wave false\nsetflag \"spawnWave\" false\nspawnwave natural 10 true\n",
    );
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(
        !runtime.session.rules.objective_flags.contains("spawnWave"),
        "GZ-1 consumed the flag"
    );
    assert_eq!(runtime.session.wave, 2, "wave 1 -> 2");
    for _ in 0..100 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(runtime.session.wave, 2, "no further wave without the flag");
}

/// H06 T2: positional `spawnwave` spawns at the given point without
/// advancing the wave (`J/logic/LExecutor.java:2177-2198`).
#[test]
fn spawnwave_positional_no_wave_advance() {
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = test_rules();
    rules.spawns.push(crate::io::json::SpawnGroup {
        type_: "dagger".to_owned(),
        begin: 0,
        unit_amount: 1,
        spawn: -1,
        ..Default::default()
    });
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    place_world_processor(&mut sim, 10, 10, "spawnwave 20 20 false\nstop\n");
    let before = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .wave;
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(
        runtime.session.wave, before,
        "positional spawn keeps the wave"
    );
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let position = sim
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .find(|e| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::UnitTypeComp>(**e)
                .map(|c| c.type_id)
                == Some(dagger)
        })
        .and_then(|e| sim.ecs.0.get::<crate::entities::comp::Pos>(*e).copied());
    let position = position.expect("positional dagger spawned");
    let distance = ((position.x - 160.0).powi(2) + (position.y - 160.0).powi(2)).sqrt();
    assert!(
        distance <= 16.5,
        "dagger within 16 px of (160,160), got {distance}"
    );
}

/// H06 T3: `cutscene pan`/`cutscene active` set the InputHandler state
/// (`J/logic/LExecutor.java:1350-1369`).
#[test]
fn cutscene_pan_sets_state() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    install_logic_world(&mut sim);
    place_world_processor(
        &mut sim,
        10,
        10,
        "cutscene pan 10 20 0.05 0\ncutscene active a 0 0 0\nstop\n",
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let cutscene = sim.logic_cutscene();
    assert!(cutscene.active, "pan activates the cutscene");
    assert_eq!(cutscene.pan, (80.0, 160.0), "World.unconv applied");
    assert!((cutscene.speed - 0.05).abs() < 1e-6);
    assert_eq!(cutscene.zoom, -1.0, "pan-only keeps zoom -1");
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let a = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(e)
        .and_then(|state| {
            state
                .executor
                .optional_var("a")
                .map(|id| state.executor.arena.get(id).num())
        });
    assert_eq!(a, Some(1.0), "`cutscene active` wrote p1");
}
