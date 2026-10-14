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
use crate::game::runtime::CampaignRuntime;
use crate::logic::blocks::LogicLink;
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
