// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shipped campaign map programs: Ground Zero, Frozen Forest, Frontier, fungal, extraction and overgrowth.
//! Upstream: core/src/mindustry/logic/LExecutor.java.

use super::*;

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
