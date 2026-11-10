// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! H06 T8 world side effects applied live by world-processor programs.
//! Upstream: core/src/mindustry/logic/LExecutor.java.

use super::*;

// ---- H06 T8: world side effects applied live ----

/// Reads a processor variable as an object.
pub(super) fn processor_obj(sim: &Sim, x: i16, y: i16, name: &str) -> Option<LogicObject> {
    let e = sim.grid.entity_at(TilePos::new(x, y)).expect("processor");
    let state = sim.ecs.0.get::<LogicBlockState>(e).expect("state");
    let id = state.executor.optional_var(name)?;
    state.executor.arena.get(id).value_obj().cloned()
}

/// Reads a processor variable as a number.
pub(super) fn processor_num(sim: &Sim, x: i16, y: i16, name: &str) -> Option<f64> {
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
