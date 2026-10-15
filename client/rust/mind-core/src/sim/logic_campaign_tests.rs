// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! H06 T7: early-sector world-processor conformance suite. Every test runs a
//! shipped Serpulo map program verbatim against the live sim and asserts the
//! Java effect (`J/logic/LExecutor.java`). The programs were dumped from
//! `assets/maps/serpulo/*.msav` on 2026-10-14 (`H06-logic.md` §0); processor
//! tiles come from Ground Zero (GZ-1 (94,42), GZ-2 (95,42), GZ-3 (96,42),
//! GZ-4 (94,43), GZ-5 (96,43)).

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::content::registries::fx_meta::{EffectRef, effect_by_name};
use crate::game::runtime::CampaignRuntime;
use crate::logic::blocks::LogicBlockState;
use crate::logic::blocks::LogicLink;
use crate::logic::value::LogicObject;
use crate::world::TilePos;
use crate::world::blocks::distribution::transfer;

use super::Sim;
use super::production_tests::{
    configure, install_campaign_session, install_logic_world, item, place, place_world_processor,
    runtime_sim, test_rules,
};

// ---- shipped programs (verbatim, `H06-logic.md` §0) ----

/// Ground Zero GZ-1.
const GZ1: &str = "getflag wave \"spawnWave\"\njump 0 equal wave false\nsetflag \"spawnWave\" false\nspawnwave natural 10 true\n";
/// Ground Zero GZ-2.
const GZ2: &str = "getflag waves \"allowWaves\"\njump 0 equal waves false\nsetflag \"allowWaves\" false\nsetrule waveSending true 0 0 100 100\nsetrule waveTimer true 0 0 100 100\n";
/// Ground Zero GZ-3.
const GZ3: &str = "fetch build d @sharded i @scatter\nfetch buildCount scatters @sharded 0 @scatter\nop add i i 1\nop mod i i scatters\nsensor amount d @ammo\njump 0 equal amount 0\nsetflag \"scatterAmmo\" true\nstop\n";
/// Ground Zero GZ-4.
const GZ4: &str = "getflag mineOver \"mineOver\"\njump 0 equal mineOver false\nsetflag \"mineOver\" false\nsetrule mapArea 10 0 0 256 256\ngetflag end \"spawnOver\"\njump 4 equal end false\ncutscene pan 135 230 0.03 0\nwait 11\ncutscene stop 100 100 0.06 0\nstop\n";
/// Ground Zero GZ-5.
const GZ5: &str = "fetch build d @sharded i @duo\nsensor amount d @ammo\nop add i i 1\nfetch buildCount totalDuos @sharded 0 @duo\nop mod i i totalDuos\njump 0 equal amount 0\nsetflag \"duoAmmo\" true\nstop\n";
/// frozenForest FF-1.
const FF1: &str = "fetch buildCount gens @sharded 0 @combustion-generator\nset i 0\nfetch build gen @sharded i @combustion-generator\nsensor eff gen @efficiency\njump 7 lessThanEq eff 0\nsetflag \"power\" true\nend\njump 2 lessThan x gens\n";
/// frozenForest FF-2.
const FF2: &str = "setrate 1\ngetflag result \"done\"\nsetrule waveTimer result 0 0 100 100\n";
/// frontier block 1.
const FRONTIER: &str = "setrule waves false 0 0 100 100\ngetflag f \"revealMap\"\njump 1 notEqual f true\nsetrule mapArea 10 0 0 1000 1000\nsetrule waves true 0 0 100 100\nstop\n";
/// fungalPass block 1.
const FUNGAL: &str = "getflag result \"unitsBuilt\"\nsetrule waveTimer result 0 0 100 100\n";
/// extractionOutpost block 1.
const EXTRACTION: &str = "control enabled factory1 0 0 0 0\nwait 720\ncontrol enabled factory1 1 0 0 0\njump 2 always x false\n";
/// overgrowth block 1.
const OVERGROWTH: &str = "fetch buildCount count @crux 0 @air-factory\nop add i i 1\nop mod i i count\nfetch build build @crux i @air-factory\nop greaterThanEq var @waveNumber 2\ncontrol enabled build var 0 0 0\n";

// ---- helpers (the live-sim shape the `gdext` host installs) ----

fn tick(sim: &mut Sim, ticks: usize) {
    for _ in 0..ticks {
        sim.tick().expect("tick");
    }
}

/// Content/team logic constants, installed before the processors are decoded
/// (`GlobalVars.install_world`, H06 T20).
fn install_globals(sim: &mut Sim) {
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
}

/// `runtime_sim` + unit runtime + logic globals + campaign session + live
/// logic resources (what the host installs before a sector starts).
fn finish_sim(mut sim: Sim, rules: crate::game::rules::Rules) -> Sim {
    sim.install_unit_runtime().expect("unit runtime");
    install_globals(&mut sim);
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    sim
}

fn campaign_sim() -> Sim {
    finish_sim(runtime_sim(), test_rules())
}

/// A flat map of `size` x `size` (Ground Zero's 256x256 map area needs a
/// winner that cannot disable the limit).
fn wide_sim(size: i32) -> Sim {
    let mut sim = Sim::new(3, size, size, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim
}

fn runtime(sim: &Sim) -> &CampaignRuntime {
    sim.ecs
        .0
        .get_resource::<CampaignRuntime>()
        .expect("campaign runtime")
}

fn session_wave(sim: &Sim) -> i32 {
    runtime(sim).session.wave
}

fn has_flag(sim: &Sim, flag: &str) -> bool {
    runtime(sim).session.rules.objective_flags.contains(flag)
}

fn set_flags(sim: &mut Sim, flags: &[&str]) {
    let mut rt = sim
        .ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign runtime");
    for flag in flags {
        rt.session.rules.objective_flags.insert((*flag).to_owned());
    }
}

fn set_wave(sim: &mut Sim, wave: i32) {
    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign runtime")
        .session
        .wave = wave;
}

fn assert_events_drained(sim: &Sim) {
    let state = sim
        .ecs
        .0
        .get_resource::<crate::logic::world::LogicWorldState>()
        .expect("logic world");
    assert!(
        state.events.is_empty(),
        "logic events are drained each tick"
    );
}

fn unit_count(sim: &Sim, name: &str, team: u8) -> usize {
    let id = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id(name)
        .expect("unit");
    let units = sim.unit_runtime().expect("unit runtime");
    units
        .units
        .iter()
        .filter(|e| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::UnitTypeComp>(**e)
                .map(|c| c.type_id)
                == Some(id)
                && sim
                    .ecs
                    .0
                    .get::<crate::entities::comp::TeamComp>(**e)
                    .map(|t| t.team)
                    == Some(team)
        })
        .count()
}

/// Adds a spawn rule to the live session (`Rules.spawns`).
fn add_spawn_group(sim: &mut Sim, unit: &str, amount: i32) {
    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign runtime")
        .session
        .rules
        .spawns
        .push(crate::io::json::SpawnGroup {
            type_: unit.to_owned(),
            begin: 0,
            unit_amount: amount,
            spawn: -1,
            ..Default::default()
        });
}

/// Puts a `spawn` overlay on the grid (`run_wave_system` reads it).
fn add_spawn_tile(sim: &mut Sim, x: i32, y: i32) {
    let spawn = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("spawn")
        .expect("spawn");
    let index = sim.grid.tiles.index(x, y);
    sim.grid.tiles.geti_mut(index).overlay = spawn;
}

/// Feeds turret ammo through the real `acceptItem`/`handleItem` dispatch (the
/// item module does not hold turret ammo).
fn feed_turret(sim: &mut Sim, turret: Entity, item_name: &str, amount: usize) {
    let ammo = item(sim, item_name);
    let world = &mut sim.ecs.0;
    for _ in 0..amount {
        assert!(
            transfer::dispatch_accept_item(world, turret, turret, ammo),
            "{item_name} accepted"
        );
        transfer::dispatch_handle_item(world, turret, turret, ammo);
    }
}

/// Places a world processor whose code carries one link (`LogicBlock` link
/// coordinates are relative to the processor; `relativeConnections`).
fn place_linked_world_processor(sim: &mut Sim, x: i16, y: i16, code: &str, links: &[LogicLink]) {
    let id = sim
        .world_block_id("world-processor")
        .expect("world-processor");
    sim.command(crate::determinism::SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let bytes = crate::logic::blocks::logic_block::compress(code, links);
    configure(sim, x, y, crate::determinism::ConfigValue::Bytes(bytes));
}

fn building_enabled(sim: &Sim, e: Entity) -> bool {
    sim.ecs
        .0
        .get::<crate::entities::comp::Building>(e)
        .expect("building")
        .enabled
}

fn efficiency(sim: &Sim, e: Entity) -> f32 {
    sim.ecs
        .0
        .get::<crate::entities::comp::Building>(e)
        .expect("building")
        .efficiency
}

// ---- Ground Zero ----

/// GZ-1: the timer objective's flag runs one wave, then the program waits for
/// the flag again (`SpawnWaveI` -> `Logic.skipWave`, `Logic.java:315-325`).
#[test]
fn gz1_spawn_wave() {
    let mut sim = campaign_sim();
    add_spawn_group(&mut sim, "dagger", 1);
    add_spawn_tile(&mut sim, 6, 6);
    set_flags(&mut sim, &["spawnWave"]);
    place_world_processor(&mut sim, 10, 10, GZ1);
    let before = session_wave(&sim);
    tick(&mut sim, 5);
    assert!(!has_flag(&sim, "spawnWave"), "GZ-1 consumes the flag");
    assert_eq!(session_wave(&sim), before + 1, "one wave advanced");
    tick(&mut sim, 2);
    assert_eq!(unit_count(&sim, "dagger", 2), 1, "one crux dagger spawned");
    tick(&mut sim, 100);
    assert_eq!(session_wave(&sim), before + 1, "no wave without the flag");
    assert_events_drained(&sim);
}

/// GZ-2: `allowWaves` turns the wave timer and the send switch on.
#[test]
fn gz2_allow_waves() {
    let mut rules = test_rules();
    rules.wave_sending = false;
    rules.wave_timer = false;
    rules.waves = true;
    let mut sim = finish_sim(runtime_sim(), rules);
    set_flags(&mut sim, &["allowWaves"]);
    place_world_processor(&mut sim, 10, 10, GZ2);
    tick(&mut sim, 5);
    assert!(!has_flag(&sim, "allowWaves"));
    assert!(runtime(&sim).session.rules.wave_sending);
    assert!(runtime(&sim).session.rules.wave_timer);
    assert_events_drained(&sim);
}

/// GZ-3: a fed scatter raises the `scatterAmmo` objective flag.
#[test]
fn gz3_scatter_ammo() {
    let mut sim = campaign_sim();
    let scatter = place(&mut sim, "scatter", 20, 20);
    feed_turret(&mut sim, scatter, "scrap", 5);
    place_world_processor(&mut sim, 10, 10, GZ3);
    tick(&mut sim, 20);
    assert!(has_flag(&sim, "scatterAmmo"), "GZ-3 senses turret ammo");
    assert_events_drained(&sim);
}

/// GZ-3 with no scatter: the null `d`/`amount` loop keeps waiting instead of
/// raising the flag (`jump 0 equal <null> 0` is true, `ConditionOp.test`).
#[test]
fn gz3_no_scatter_waits() {
    let mut sim = campaign_sim();
    place_world_processor(&mut sim, 10, 10, GZ3);
    tick(&mut sim, 60);
    assert!(!has_flag(&sim, "scatterAmmo"), "no scatter -> no flag");
    assert_events_drained(&sim);
}

/// GZ-4: the map-area expansion plus the camera pan/stop cutscene.
#[test]
fn gz4_map_area_and_cutscene() {
    let mut sim = finish_sim(wide_sim(300), test_rules());
    set_flags(&mut sim, &["mineOver", "spawnOver"]);
    place_world_processor(&mut sim, 10, 10, GZ4);
    tick(&mut sim, 5);
    let cutscene = sim.logic_cutscene();
    assert!(cutscene.active, "GZ-4 pan is active");
    assert_eq!(cutscene.pan, (1080.0, 1840.0), "World.unconv of (135,230)");
    assert!(runtime(&sim).session.rules.limit_map_area);
    assert_eq!(runtime(&sim).session.rules.limit_width, 256);
    assert_eq!(runtime(&sim).session.rules.limit_height, 256);
    // `wait 11` (seconds) then `cutscene stop`.
    tick(&mut sim, 11 * 60 + 10);
    assert!(!sim.logic_cutscene().active, "stop ends the cutscene");
    assert_events_drained(&sim);
}

/// GZ-5: with two duos and only the second one fed, the round-robin `i`
/// reaches it and raises `duoAmmo`.
#[test]
fn gz5_duo_ammo() {
    let mut sim = campaign_sim();
    let _empty = place(&mut sim, "duo", 20, 20);
    let fed = place(&mut sim, "duo", 25, 20);
    feed_turret(&mut sim, fed, "copper", 1);
    place_world_processor(&mut sim, 10, 10, GZ5);
    tick(&mut sim, 20);
    assert!(has_flag(&sim, "duoAmmo"), "GZ-5 round-robin found ammo");
    assert_events_drained(&sim);
}

// ---- Frozen Forest / Frontier / fungalPass ----

/// FF-1: a burning combustion generator raises `power`; without fuel the
/// program keeps looping and the flag stays unset.
#[test]
fn ff1_power_flag() {
    let mut sim = campaign_sim();
    let generator = place(&mut sim, "combustion-generator", 20, 20);
    let coal = item(&sim, "coal");
    {
        let world = &mut sim.ecs.0;
        assert!(transfer::dispatch_accept_item(
            world, generator, generator, coal
        ));
        transfer::dispatch_handle_item(world, generator, generator, coal);
    }
    tick(&mut sim, 30);
    assert!(efficiency(&sim, generator) > 0.0, "generator is burning");
    place_world_processor(&mut sim, 10, 10, FF1);
    tick(&mut sim, 6);
    assert!(has_flag(&sim, "power"), "eff > 0 raises the flag");

    // Unfuelled: the flag never appears.
    let mut cold = campaign_sim();
    place(&mut cold, "combustion-generator", 20, 20);
    place_world_processor(&mut cold, 10, 10, FF1);
    tick(&mut cold, 120);
    assert!(!has_flag(&cold, "power"), "no fuel -> no flag");
    assert_events_drained(&cold);
}

/// FF-2: `waveTimer` follows the `done` flag (`setrate 1` + `getflag` +
/// `setrule`).
#[test]
fn ff2_wave_timer_follows_done() {
    let mut sim = campaign_sim();
    assert!(runtime(&sim).session.rules.wave_timer, "rules default on");
    place_world_processor(&mut sim, 10, 10, FF2);
    tick(&mut sim, 5);
    assert!(
        !runtime(&sim).session.rules.wave_timer,
        "no done flag -> off"
    );
    set_flags(&mut sim, &["done"]);
    tick(&mut sim, 5);
    assert!(runtime(&sim).session.rules.wave_timer, "done flag -> on");
    assert_events_drained(&sim);
}

/// frontier: `revealMap` widens the map area to 1000x1000 (covers the map, so
/// Java disables the limit) and turns waves on.
#[test]
fn frontier_reveal() {
    let mut sim = campaign_sim();
    set_flags(&mut sim, &["revealMap"]);
    place_world_processor(&mut sim, 10, 10, FRONTIER);
    tick(&mut sim, 5);
    assert!(runtime(&sim).session.rules.waves, "waves enabled");
    assert!(
        !runtime(&sim).session.rules.limit_map_area,
        "1000x1000 covers the 32x32 map -> limit disabled"
    );
    assert_events_drained(&sim);
}

/// fungalPass: `waveTimer` tracks the `unitsBuilt` flag.
#[test]
fn fungal_wave_timer() {
    let mut sim = campaign_sim();
    place_world_processor(&mut sim, 10, 10, FUNGAL);
    tick(&mut sim, 5);
    assert!(!runtime(&sim).session.rules.wave_timer);
    set_flags(&mut sim, &["unitsBuilt"]);
    tick(&mut sim, 5);
    assert!(runtime(&sim).session.rules.wave_timer);
    assert_events_drained(&sim);
}

/// extractionOutpost block 1: the linked `factory1` is disabled, then
/// re-enabled after `wait 720` (720 s = 43200 ticks).
#[test]
fn extraction_factory_toggle() {
    let mut sim = campaign_sim();
    let factory = place(&mut sim, "ground-factory", 20, 20);
    // Relative link from the processor at (10,10) to the factory at (20,20).
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        EXTRACTION,
        &[LogicLink::new("factory1", 10, 10)],
    );
    tick(&mut sim, 3);
    assert!(!building_enabled(&sim, factory), "control enabled 0");
    tick(&mut sim, 10_000);
    assert!(
        !building_enabled(&sim, factory),
        "still disabled 10000 ticks into `wait 720` (43200 ticks)"
    );
    tick(&mut sim, 40_000);
    assert!(building_enabled(&sim, factory), "re-enabled after wait 720");
    assert_events_drained(&sim);
}

/// overgrowth block 1: the crux air-factory is held disabled until
/// `@waveNumber >= 2`.
#[test]
fn overgrowth_factory_gate() {
    let mut sim = campaign_sim();
    let id = sim.world_block_id("air-factory").expect("air-factory");
    sim.command(crate::determinism::SimCommand::Place {
        x: 20,
        y: 20,
        block: id,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("placed");
    let factory = sim.grid.entity_at(TilePos::new(20, 20)).expect("entity");
    place_world_processor(&mut sim, 10, 10, OVERGROWTH);
    set_wave(&mut sim, 1);
    tick(&mut sim, 5);
    assert!(!building_enabled(&sim, factory), "wave 1 -> disabled");
    set_wave(&mut sim, 2);
    tick(&mut sim, 5);
    assert!(building_enabled(&sim, factory), "wave 2 -> enabled");
    assert_events_drained(&sim);
}

// ---- H06 T8: world side effects applied live ----

/// Reads a processor variable as an object.
fn processor_obj(sim: &Sim, x: i16, y: i16, name: &str) -> Option<LogicObject> {
    let e = sim.grid.entity_at(TilePos::new(x, y)).expect("processor");
    let state = sim.ecs.0.get::<LogicBlockState>(e).expect("state");
    let id = state.executor.optional_var(name)?;
    state.executor.arena.get(id).value_obj().cloned()
}

/// Reads a processor variable as a number.
fn processor_num(sim: &Sim, x: i16, y: i16, name: &str) -> Option<f64> {
    let e = sim.grid.entity_at(TilePos::new(x, y)).expect("processor");
    let state = sim.ecs.0.get::<LogicBlockState>(e).expect("state");
    let id = state.executor.optional_var(name)?;
    Some(state.executor.arena.get(id).num())
}

fn health_of(sim: &Sim, e: Entity) -> f32 {
    sim.ecs
        .0
        .get::<crate::entities::comp::Health>(e)
        .expect("health")
        .health
}

/// Installs a recording `FxSink` on the unit runtime.
fn install_fx_recorder(sim: &mut Sim) -> std::sync::Arc<crate::fx::sink::FxBus> {
    use crate::fx::sink::FxBus;
    let bus = std::sync::Arc::new(FxBus::new());
    let mut runtime = sim
        .ecs
        .0
        .remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .expect("unit runtime");
    runtime.fx = bus.clone();
    sim.ecs.0.insert_non_send(runtime);
    bus
}

/// T8: `spawn @dagger ... u` puts the unit on the map and into `u`
/// (`SpawnUnitI`, `J/logic/LExecutor.java:1615-1650`).
#[test]
fn spawn_sets_result() {
    let mut rules = test_rules();
    rules.unit_cap = 20;
    rules.unit_cap_variable = false;
    let mut sim = finish_sim(runtime_sim(), rules);
    place_world_processor(
        &mut sim,
        10,
        10,
        "spawn @dagger 20 20 0 @sharded u\nsensor x u @x\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(unit_count(&sim, "dagger", 1), 1, "unit spawned");
    let x = processor_num(&sim, 10, 10, "x").expect("x bound");
    assert!(
        (x - 20.0).abs() < 0.02,
        "sensor @x reads the spawn tile: {x}"
    );
    assert_events_drained(&sim);
}

/// T8: `Units.canCreate` gates the spawn (`Rules.unitCap`).
#[test]
fn spawn_respects_unit_cap() {
    let mut rules = test_rules();
    rules.unit_cap = 1;
    rules.unit_cap_variable = false;
    let mut sim = finish_sim(runtime_sim(), rules);
    place_world_processor(
        &mut sim,
        10,
        10,
        "spawn @dagger 20 20 0 @sharded u\nspawn @dagger 24 20 0 @sharded v\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(unit_count(&sim, "dagger", 1), 1, "cap 1 blocks the second");
    assert_events_drained(&sim);
}

/// T8: `setblock block @air ...` clears the tile live
/// (`SetBlockI`, `J/logic/LExecutor.java:1566-1612`).
#[test]
fn setblock_clears_tile() {
    let mut sim = campaign_sim();
    place(&mut sim, "conveyor", 20, 20);
    assert!(sim.grid.entity_at(TilePos::new(20, 20)).is_some());
    place_world_processor(
        &mut sim,
        10,
        10,
        "setblock block @air 20 20 @derelict 0\nstop\n",
    );
    tick(&mut sim, 3);
    assert!(
        sim.grid.entity_at(TilePos::new(20, 20)).is_none(),
        "building removed"
    );
    assert_eq!(
        sim.grid.block_at(TilePos::new(20, 20)),
        Some(BlockId::AIR),
        "tile is air"
    );
    assert!(sim.logic_world_changed, "renderer dirty flag set");
    assert_events_drained(&sim);
}

/// T8: `status false fast u <s>` applies the status for `duration * 60` ticks
/// (`ApplyEffectI`, `J/logic/LExecutor.java:1765-1776`).
#[test]
fn status_applies_fast() {
    let mut rules = test_rules();
    rules.unit_cap = 20;
    rules.unit_cap_variable = false;
    let mut sim = finish_sim(runtime_sim(), rules);
    place_world_processor(
        &mut sim,
        10,
        10,
        "spawn @dagger 20 20 0 @sharded u\nstatus false @status-fast u 100\nstop\n",
    );
    tick(&mut sim, 3);
    let dagger = sim
        .unit_runtime()
        .expect("unit runtime")
        .units
        .iter()
        .copied()
        .find(|e| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::UnitTypeComp>(*e)
                .is_some_and(|c| {
                    sim.block_runtime()
                        .expect("runtime")
                        .content()
                        .unit(c.type_id)
                        .is_some_and(|def| def.name == "dagger")
                })
        })
        .expect("dagger");
    let fast = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .status_id("fast")
        .expect("fast");
    let comp = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::StatusComp>(dagger)
        .expect("statuses");
    let entry = comp
        .statuses
        .iter()
        .find(|entry| entry.effect == fast)
        .expect("fast applied");
    assert!(
        entry.duration > 5900.0,
        "100 s -> 6000 ticks, got {}",
        entry.duration
    );
    assert_events_drained(&sim);
}

/// T8: `explosion` damages the crux dagger at its centre
/// (`Call.logicExplosion`, `J/logic/LExecutor.java:2004-2040`).
#[test]
fn explosion_damages() {
    let mut sim = finish_sim(wide_sim(64), test_rules());
    let dagger = sim
        .spawn_live_unit("dagger", 2, 320.0, 320.0, 0.0)
        .expect("dagger");
    let before = health_of(&sim, dagger);
    place_world_processor(
        &mut sim,
        10,
        10,
        "explosion @sharded 40 40 3 50 1 1 0 0\nstop\n",
    );
    tick(&mut sim, 3);
    let after = health_of(&sim, dagger);
    let armor = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .and_then(|id| sim.block_runtime().expect("runtime").content().unit(id))
        .map(|def| def.armor)
        .unwrap_or(0.0);
    let expected = (50.0 - armor).max(50.0 * crate::combat::damage::armor::MIN_ARMOR_DAMAGE);
    assert!(
        (before - after - expected).abs() < 0.5,
        "damage {after} vs {before} (expected -{expected})"
    );
    assert_events_drained(&sim);
}

/// T8: `effect warn ...` plays the mapped `Fx` effect through the view sink
/// (`LogicFx.java`, `EffectI`).
#[test]
fn effect_emits_named_fx() {
    use crate::fx::sink::FxEvent;
    let mut sim = campaign_sim();
    let bus = install_fx_recorder(&mut sim);
    place_world_processor(&mut sim, 10, 10, "effect warn 130 60 2 %ffaaff\nstop\n");
    tick(&mut sim, 3);
    let expected = effect_by_name("unitCapKill").expect("unitCapKill");
    assert!(
        bus.drain().iter().any(|event| matches!(
            event,
            FxEvent::Effect {
                effect: EffectRef::Named(id),
                ..
            } if *id == expected
        )),
        "warn plays Fx.unitCapKill"
    );
    assert_events_drained(&sim);
}

/// T8: `getblock floor` reads the live grid (no `WorldGrid` resource).
#[test]
fn getblock_floor_live() {
    let mut sim = campaign_sim();
    place_world_processor(&mut sim, 10, 10, "getblock floor fl 5 5\nstop\n");
    tick(&mut sim, 3);
    let floor = sim.grid.tile(5, 5).floor;
    assert_ne!(floor, BlockId::AIR, "runtime_sim floors are stone");
    assert_eq!(
        processor_obj(&sim, 10, 10, "fl"),
        Some(LogicObject::Content(crate::content::ContentRef::block(
            floor
        ))),
        "floor readback"
    );
    assert_events_drained(&sim);
}

/// T8: `weatherset @rain true` creates the live weather entity and
/// `weathersense` reads it back (`SetWeatherI`/`SenseWeatherI`).
#[test]
fn weatherset_creates_rain() {
    use crate::entities::comp::weather::WeatherRuntime;
    let mut sim = campaign_sim();
    let rain = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .weather_by_name("rain")
        .expect("rain")
        .id;
    place_world_processor(&mut sim, 10, 10, "weatherset @rain true\nstop\n");
    tick(&mut sim, 3);
    assert!(
        sim.ecs
            .0
            .iter_entities()
            .any(|e| e.get::<WeatherRuntime>().is_some_and(|w| w.weather == rain)),
        "rain entity created"
    );
    place_world_processor(&mut sim, 12, 10, "weathersense r @rain\nstop\n");
    tick(&mut sim, 3);
    assert_eq!(processor_num(&sim, 12, 10, "r"), Some(1.0), "sense reads 1");
    place_world_processor(&mut sim, 14, 10, "weatherset @rain false\nstop\n");
    tick(&mut sim, 360);
    let life = sim
        .ecs
        .0
        .iter_entities()
        .find_map(|e| e.get::<WeatherRuntime>().map(|w| w.life));
    assert!(
        life.is_some_and(|life| life <= 240.0),
        "off clamps life to fadeTime: {life:?}"
    );
    assert_events_drained(&sim);
}

/// T8: atolls block 1's first steps (verbatim): the four `setblock`s clear the
/// tiles and the eight `spawn`s create the units (the `ubind m1` half follows
/// in T9).
#[test]
fn atolls_block1_first_steps() {
    let mut sim = finish_sim(wide_sim(320), test_rules());
    set_flags(&mut sim, &["attack"]);
    for x in [210, 215, 220, 225] {
        place(&mut sim, "conveyor", x, 306);
    }
    // Atolls is an ocean sector: the two naval spawns stand on deep water.
    let water = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("deep-water")
        .expect("deep-water");
    for x in [220, 225] {
        sim.grid.tiles.get_mut(x, 306).floor = water;
    }
    place_world_processor(
        &mut sim,
        10,
        10,
        "getflag go \"attack\"\njump 0 equal go false\nwait 1\nsetrule waves false 0 0 100 100\ncutscene pan 263 135 0.06 0\ncutscene zoom 0.1 135 0.06 0\nlocaleprint \"atolls.noairunit\"\nmessage announce 6 @wait\nsetrule unitCap 4 0 0 100 100\nsetblock block @air 210 306 @derelict 0\nsetblock block @air 215 306 @derelict 0\nsetblock block @air 220 306 @derelict 0\nsetblock block @air 225 306 @derelict 0\nwait 0.02\nspawn @fortress 210 306 270 @sharded l1\nspawn @fortress 215 306 270 @sharded l2\nspawn @minke 220 306 270 @sharded n1\nspawn @minke 225 306 270 @sharded n2\nspawn @mega 110 400 270 @sharded m1\nspawn @mega 115 400 270 @sharded m2\nspawn @mega 120 400 270 @sharded m3\nspawn @mega 125 400 270 @sharded m4\nstop\n",
    );
    // 60 ticks `wait 1` + 360 ticks `message announce 6 @wait` + the spawn run.
    tick(&mut sim, 440);
    for x in [210, 215, 220, 225] {
        assert!(
            sim.grid.entity_at(TilePos::new(x, 306)).is_none(),
            "setblock cleared ({x},306)"
        );
    }
    assert_eq!(unit_count(&sim, "fortress", 1), 2, "2 fortresses");
    assert_eq!(unit_count(&sim, "minke", 1), 2, "2 minkes");
    assert_eq!(unit_count(&sim, "mega", 1), 4, "4 megas");
    assert_events_drained(&sim);
}

// ---- I03 T1: ubind/ucontrol/ulocate drive live units (PAR-0221) ----

/// Reads the processor's bound `@unit`.
fn processor_bound_unit(sim: &Sim, x: i16, y: i16) -> Option<Entity> {
    let e = sim.grid.entity_at(TilePos::new(x, y)).expect("processor");
    let state = sim.ecs.0.get::<LogicBlockState>(e).expect("state");
    match state.executor.arena.get(state.executor.unit).value_obj() {
        Some(LogicObject::Unit(unit)) => Some(*unit),
        _ => None,
    }
}

fn controller_kind(sim: &Sim, e: Entity) -> crate::ai::AiKind {
    sim.ecs
        .0
        .get::<crate::ai::ControllerSlot>(e)
        .expect("controller slot")
        .kind
}

fn entity_pos(sim: &Sim, e: Entity) -> (f32, f32) {
    let pos = sim.ecs.0.get::<crate::entities::comp::Pos>(e).expect("pos");
    (pos.x, pos.y)
}

fn mono_stack(sim: &Sim, e: Entity) -> Option<(crate::content::ItemId, i32)> {
    sim.ecs
        .0
        .get::<crate::entities::comp::unit::comp::ItemsComp>(e)
        .and_then(|items| items.item)
}

fn container_count(sim: &Sim, e: Entity, item: crate::content::ItemId) -> i32 {
    sim.ecs
        .0
        .get::<crate::world::modules::ItemModule>(e)
        .map(|module| module.get(item))
        .unwrap_or(0)
}

/// `ubind @poly` cycles the team's polys in spawn order (`%=`/`++`).
#[test]
fn ubind_type_cycles_in_spawn_order() {
    let mut sim = campaign_sim();
    let p1 = sim
        .spawn_live_unit("poly", 1, 80.0, 80.0, 0.0)
        .expect("poly 1");
    let p2 = sim
        .spawn_live_unit("poly", 1, 120.0, 80.0, 0.0)
        .expect("poly 2");
    let p3 = sim
        .spawn_live_unit("poly", 1, 160.0, 80.0, 0.0)
        .expect("poly 3");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @poly\nset a @unit\nubind @poly\nset b @unit\nubind @poly\nset c @unit\nubind @poly\nset d @unit\nstop\n",
    );
    tick(&mut sim, 10);
    assert_eq!(
        processor_obj(&sim, 10, 10, "a"),
        Some(LogicObject::Unit(p1))
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "b"),
        Some(LogicObject::Unit(p2))
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "c"),
        Some(LogicObject::Unit(p3))
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "d"),
        Some(LogicObject::Unit(p1)),
        "the cursor wraps"
    );
    assert_events_drained(&sim);
}

/// `ubind <unit object>` binds that unit (`UnitBindI` object branch).
#[test]
fn ubind_unit_object() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    place_world_processor(
        &mut sim,
        10,
        10,
        "spawn @dagger 14 14 0 @sharded u\nubind u\nset b @unit\nstop\n",
    );
    tick(&mut sim, 10);
    let spawned = processor_obj(&sim, 10, 10, "u").expect("spawn result");
    assert!(matches!(spawned, LogicObject::Unit(_)));
    assert_eq!(processor_obj(&sim, 10, 10, "b"), Some(spawned));
    assert_events_drained(&sim);
}

/// A non-privileged micro processor with `logicUnitControl = false` binds
/// nothing (`UnitBindI` gate).
#[test]
fn ubind_gate() {
    let mut rules = test_rules();
    rules.logic_unit_control = false;
    let mut sim = finish_sim(runtime_sim(), rules);
    let poly = sim
        .spawn_live_unit("poly", 1, 80.0, 80.0, 0.0)
        .expect("poly");
    place(&mut sim, "micro-processor", 12, 12);
    let bytes =
        crate::logic::blocks::logic_block::compress("ubind @poly\nset a @unit\nstop\n", &[]);
    configure(
        &mut sim,
        12,
        12,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick(&mut sim, 10);
    assert_eq!(
        processor_bound_unit(&sim, 12, 12),
        None,
        "gate leaves @unit null"
    );
    let _ = poly;
    assert_events_drained(&sim);
}

/// `ucontrol move` swaps the controller to LogicAI and walks to the target.
#[test]
fn ucontrol_move_reaches_target() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol move 10 10 0 0 0\n",
    );
    tick(&mut sim, 300);
    let (x, y) = entity_pos(&sim, dagger);
    let (dx, dy) = (x - 80.0, y - 80.0);
    assert!(
        dx * dx + dy * dy <= 16.0 * 16.0,
        "dagger within 16 px of (80,80): ({x},{y})"
    );
    assert_eq!(controller_kind(&sim, dagger), crate::ai::AiKind::Logic);
    assert_events_drained(&sim);
}

/// A unit with a live CommandAI move order is not logic-controllable
/// (`CommandAI.isLogicControllable` = `!hasCommand()`).
#[test]
fn commanded_unit_not_logic_controllable() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
        .expect("dagger");
    {
        let mut slot = sim
            .ecs
            .0
            .get_mut::<crate::ai::ControllerSlot>(dagger)
            .expect("slot");
        slot.kind = crate::ai::AiKind::Command;
        slot.target = Some(TilePos::new(30, 30));
    }
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol move 10 10 0 0 0\n",
    );
    tick(&mut sim, 20);
    assert_eq!(
        controller_kind(&sim, dagger),
        crate::ai::AiKind::Command,
        "the command order keeps CommandAI"
    );
    assert!(
        sim.ecs
            .0
            .get::<crate::ai::types::logic::LogicAi>(dagger)
            .is_none(),
        "no LogicAI component is installed"
    );
    assert_events_drained(&sim);
}

/// 600 ticks after the processor stops issuing unit commands, the unit
/// returns to its type's default controller (`LogicAI.controlTimer`).
#[test]
fn logic_ai_times_out() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol move 10 10 0 0 0\n",
    );
    tick(&mut sim, 30);
    assert_eq!(controller_kind(&sim, dagger), crate::ai::AiKind::Logic);
    // Replace the code with `stop`: no further ucontrol for 601 ticks.
    let bytes = crate::logic::blocks::logic_block::compress("stop\n", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick(&mut sim, 601);
    assert_eq!(
        controller_kind(&sim, dagger),
        crate::ai::AiKind::Command,
        "the campaign default for a player-team dagger"
    );
    assert!(
        sim.ecs
            .0
            .get::<crate::ai::types::logic::LogicAi>(dagger)
            .is_none(),
        "LogicAi is removed on timeout"
    );
    assert_events_drained(&sim);
}

/// `ucontrol within` writes p4; `ucontrol getBlock` writes block/building/
/// floor.
#[test]
fn ucontrol_within_and_getblock() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol within 12 12 2 r\nucontrol getBlock 10 10 t b f\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(processor_num(&sim, 10, 10, "r"), Some(1.0), "inside 16 px");
    let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("processor");
    let content = sim.block_runtime().expect("runtime").content();
    let world_processor = content.block_id("world-processor").expect("block");
    let stone = content.block_id("stone").expect("stone");
    assert_eq!(
        processor_obj(&sim, 10, 10, "t"),
        Some(LogicObject::Content(crate::content::ContentRef::block(
            world_processor
        ))),
        "t is the block at (10,10)"
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "b"),
        Some(LogicObject::Building(processor)),
        "b is the building at (10,10)"
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "f"),
        Some(LogicObject::Content(crate::content::ContentRef::block(
            stone
        ))),
        "f is the floor"
    );
    let _ = dagger;
    assert_events_drained(&sim);
}

/// `ucontrol itemTake` pulls items into the unit stack; a second call within
/// the 90-tick `transferDelay` does nothing; `ucontrol itemDrop` puts them
/// back into the container (`BuildingComp.acceptStack`).
#[test]
fn ucontrol_item_take_and_drop() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    let container = place(&mut sim, "container", 12, 12);
    let copper = item(&sim, "copper");
    sim.ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(container)
        .expect("items")
        .add(copper, 20, 100);
    let mono = sim
        .spawn_live_unit("mono", 1, 96.0, 96.0, 0.0)
        .expect("mono");
    let links = [LogicLink::new("container1", 2, 2)];
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "ubind @mono\nucontrol itemTake container1 @copper 10\n",
        &links,
    );
    // First transfer happens at the first tick past the 90-tick delay.
    tick(&mut sim, 100);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 10)));
    assert_eq!(container_count(&sim, container, copper), 10);
    // Still inside the delay for the second call.
    tick(&mut sim, 70);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 10)));
    // Past it: the take runs again.
    tick(&mut sim, 20);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 20)));
    assert_eq!(container_count(&sim, container, copper), 0);
    // Drop half of it back.
    let bytes = crate::logic::blocks::logic_block::compress(
        "ubind @mono\nucontrol itemDrop container1 10\n",
        &links,
    );
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick(&mut sim, 100);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 10)));
    assert_eq!(container_count(&sim, container, copper), 10);
    assert_events_drained(&sim);
}

/// `ucontrol build` queues a plan the builder executes (`logicUnitBuild`).
#[test]
fn ucontrol_build_places_plan() {
    let mut rules = test_rules();
    rules.logic_unit_build = true;
    rules.instant_build = true;
    rules.infinite_resources = true;
    let mut sim = finish_sim(wide_sim(64), rules);
    place(&mut sim, "core-shard", 20, 20);
    let poly = sim
        .spawn_live_unit("poly", 1, 96.0, 112.0, 0.0)
        .expect("poly");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @poly\nucontrol build 12 12 @copper-wall 0 0\nstop\n",
    );
    tick(&mut sim, 200);
    let wall = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("copper-wall")
        .expect("wall");
    assert_eq!(
        sim.grid.block_at(TilePos::new(12, 12)),
        Some(wall),
        "the poly placed the wall at (12,12)"
    );
    assert_eq!(controller_kind(&sim, poly), crate::ai::AiKind::Logic);
    assert_events_drained(&sim);
}

/// `ulocate building core` returns the own core's tile, `f == 1` and the
/// building object.
#[test]
fn ulocate_building_core() {
    let mut sim = campaign_sim();
    let core = place(&mut sim, "core-shard", 20, 20);
    let poly = sim
        .spawn_live_unit("poly", 1, 100.0, 100.0, 0.0)
        .expect("poly");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @poly\nulocate building core false @copper x y f b\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(processor_num(&sim, 10, 10, "f"), Some(1.0), "found");
    let (cx, cy) = entity_pos(&sim, core);
    assert_eq!(
        processor_num(&sim, 10, 10, "x"),
        Some((cx / 8.0) as f64),
        "outX is the core's tile x"
    );
    assert_eq!(processor_num(&sim, 10, 10, "y"), Some((cy / 8.0) as f64));
    assert_eq!(
        processor_obj(&sim, 10, 10, "b"),
        Some(LogicObject::Building(core))
    );
    let _ = poly;
    assert_events_drained(&sim);
}
