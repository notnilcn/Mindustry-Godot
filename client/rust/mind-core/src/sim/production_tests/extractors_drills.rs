// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime extractor, drill, mender and incinerator regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;
use crate::sim::test_support::{
    configure, item, liquid, place_team1, runtime_sim, stored_item, stored_liquid, tick_n,
};

/// T7 (`SolidPump.updateTile`, `SolidPump.java:116-135`; `Blocks.java:2941-2948`):
/// the water extractor's yield is `pumpAmount * (validTiles + boost)` where
/// `validTiles = 1` on any pumpable floor (`baseEfficiency = 1`) and `boost` is
/// the footprint water attribute / size². Four `mud` tiles (`water = 1`) double
/// the rate.
#[test]
fn water_extractor_scales_with_floor() {
    let mut sim = runtime_sim();
    let _plain_source = place_team1(&mut sim, "power-source", 4, 5);
    let _wet_source = place_team1(&mut sim, "power-source", 11, 5);
    let plain = place_team1(&mut sim, "water-extractor", 5, 5);
    for (x, y) in [(12, 5), (13, 5), (12, 6), (13, 6)] {
        set_floor(&mut sim, x, y, "mud");
    }
    let wet = place_team1(&mut sim, "water-extractor", 12, 5);
    tick_n(&mut sim, 60);
    let water = liquid(&sim, "water");
    let plain_amount = stored_liquid(&sim, plain, water);
    let wet_amount = stored_liquid(&sim, wet, water);
    assert!(plain_amount > 0.0, "plain extractor pumped {plain_amount}");
    assert!(
        (wet_amount / plain_amount - 2.0).abs() < 0.1,
        "wet {wet_amount} vs plain {plain_amount}: the water attribute doubles it"
    );
}

/// H07 T9 (`SolidPump.java:124`): `attribute.env()` scales the water
/// extractor; rain carries `water = 0.2`, so output is `1.2x` the no-weather
/// run (`Attribute.java:33`).
#[test]
fn water_extractor_rain_bonus() {
    use crate::world::blocks::power::behavior::GeneratorEnv;

    let run = |water_env: Option<f32>| {
        let mut sim = runtime_sim();
        if let Some(water_env) = water_env {
            sim.ecs.0.insert_resource(GeneratorEnv {
                water_env,
                ..GeneratorEnv::default()
            });
        }
        let _source = place_team1(&mut sim, "power-source", 4, 5);
        let pump = place_team1(&mut sim, "water-extractor", 5, 5);
        tick_n(&mut sim, 60);
        let water = liquid(&sim, "water");
        stored_liquid(&sim, pump, water)
    };
    let base = run(None);
    let boosted = run(Some(0.2));
    assert!(base > 0.0, "base extractor pumped {base}");
    assert!(
        (boosted / base - 1.2).abs() < 0.05,
        "rain bonus {boosted} vs {base}"
    );
}

/// T7 (`SolidPump.canPlaceOn`/`updateTile`, `SolidPump.java:68-75`, `:116-135`;
/// `Blocks.java:2985-2992`): the oil extractor has `baseEfficiency = 0`, so it
/// cannot be placed on attribute-less floors and its yield is the oil affinity
/// (`sand-floor = 0.7`) times `pumpAmount` (0.25).
#[test]
fn oil_extractor_needs_oil_floor() {
    let mut sim = runtime_sim();
    let oil_id = sim.world_block_id("oil-extractor").expect("oil-extractor");
    let _ = sim.command(SimCommand::Place {
        x: 5,
        y: 5,
        block: oil_id,
        rotation: 0,
        team: 1,
        player: None,
    });
    assert!(
        sim.grid.entity_at(TilePos::new(5, 5)).is_none(),
        "no oil affinity and baseEfficiency 0 -> canPlaceOn rejects the block"
    );
    for x in 4..=6 {
        for y in 4..=6 {
            set_floor(&mut sim, x, y, "sand-floor");
        }
    }
    let _source = place_team1(&mut sim, "power-source", 3, 5);
    let extractor = place_team1(&mut sim, "oil-extractor", 5, 5);
    {
        let (sand, water) = (item(&sim, "sand"), liquid(&sim, "water"));
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(extractor) {
            items.add(sand, 10, 10);
        }
        if let Some(mut liquids) = world.get_mut::<crate::world::modules::LiquidModule>(extractor) {
            liquids.add(water, 20.0, 40.0);
        }
    }
    tick_n(&mut sim, 60);
    let oil = stored_liquid(&sim, extractor, liquid(&sim, "oil"));
    // `pumpAmount` 0.25 * (0 + 9 * 0.7 / 9) = 0.175 per tick.
    assert!((oil - 10.5).abs() < 1.5, "oil pumped {oil}");
}

/// T5 (`BeamDrill.updateFacing`, `BeamDrill.java:237-266`): the facing scan runs
/// every tick, so removing the wall ore in front stops production; the port
/// cached the scan at placement.
#[test]
fn plasma_bore_stops_when_ore_removed() {
    let mut sim = runtime_sim();
    let ore =
        crate::content::BlockId::new(sim.world_block_id("ore-wall-beryllium").expect("ore wall"));
    let wall = crate::content::BlockId::new(
        sim.world_block_id("beryllic-stone-wall")
            .expect("static wall"),
    );
    for y in [5, 6] {
        let tile = sim.grid.tiles.get_mut(9, y);
        tile.block = wall;
        tile.overlay = ore;
    }
    let _source = place_team1(&mut sim, "power-source", 4, 5);
    let bore = place_team1(&mut sim, "plasma-bore", 5, 5);
    let beryllium = item(&sim, "beryllium");
    tick_n(&mut sim, 400);
    let produced = stored_item(&sim, bore, beryllium);
    assert!(produced > 0, "bore produced before removal: {produced}");

    // Remove the ore overlay; within one tick the facing scan reports nothing.
    for y in [5, 6] {
        sim.grid.tiles.get_mut(9, y).overlay = crate::content::BlockId::AIR;
    }
    sim.tick().expect("tick");
    let facing = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(bore)
        .map(|state| state.facing_amount)
        .unwrap_or(-1.0);
    assert_eq!(facing, 0.0, "facing rescan dropped the removed ore");
    let before = stored_item(&sim, bore, beryllium);
    tick_n(&mut sim, 400);
    assert_eq!(
        stored_item(&sim, bore, beryllium),
        before,
        "no production once the ore is gone"
    );
}

/// T13 (`Door.tapped`, `Door.java:148-154`): `Door`'s **config body** applies
/// immediately (`Door.java:38-58` has no throttle); the 60-tick `timerToggle`
/// gate sits on the tap path only (`door_tap_allowed`; the timer semantics are
/// covered by `behavior::defense::tests::door_tap_timer_gates_and_records`).
#[test]
fn door_configs_apply_without_tap_throttle() {
    let mut sim = runtime_sim();
    let door = place_team1(&mut sim, "door", 5, 5);
    let is_open = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::entities::comp::DoorState>(door)
            .map(|state| state.open)
            .unwrap_or(false)
    };
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Bool(true));
    assert!(is_open(&sim), "opened on the first config");
    tick_n(&mut sim, 10);
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Bool(false));
    assert!(
        !is_open(&sim),
        "a config is not a tap: no 60-tick gate (`Door.java:38-58`)"
    );
}

/// H07 T2 (`Door.java:40`): a live door config plays `door` once at the door
/// centre (volume 1, pitch 1).
#[test]
fn door_toggle_plays_door() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let _door = place_team1(&mut sim, "door", 5, 5);
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Bool(true));
    let events = log.events();
    let doors: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::At { sound, .. } if *sound == SoundId::DOOR => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(doors.len(), 1, "one door sound: {events:?}");
    let AudioEvent::At {
        x,
        y,
        pitch,
        volume,
        ..
    } = doors[0]
    else {
        unreachable!()
    };
    assert_eq!((*x, *y), (40.0, 40.0), "door centre");
    assert_eq!(*volume, 1.0);
    assert_eq!(*pitch, 1.0);
}

/// T1: the mechanical drill accepts the optional water consumer through the
/// trait default (`BuildingComp.acceptLiquid`,
/// `BuildingComp.java:873-875`), so a fed drill ramps past warmup 1.0
/// (`speed = lerp(1, 1.6, optionalEfficiency)`, `Drill.java:301-304`).
#[test]
fn drill_accepts_water_boost() {
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "ore-copper");
    }
    let drill = place_team1(&mut sim, "mechanical-drill", 5, 5);
    let water = liquid(&sim, "water");
    let _source = place_team1(&mut sim, "liquid-source", 4, 5);
    configure(
        &mut sim,
        4,
        5,
        crate::determinism::ConfigValue::Content("water".to_owned()),
    );
    tick_n(&mut sim, 120);
    assert!(
        stored_liquid(&sim, drill, water) > 0.0,
        "the drill accepted the water from the source"
    );
    let building = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(drill)
        .expect("building");
    assert_eq!(
        building.optional_efficiency, 1.0,
        "optional (booster) consumer satisfied"
    );
    let warmup = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(drill)
        .map(|state| state.warmup)
        .expect("drill state");
    assert!(warmup > 1.0, "water boost ramps warmup past 1.0: {warmup}");
}

/// T1: consumers declare items via `consumeItem`; the trait default accepts
/// exactly those (`BuildingComp.acceptItem`, `BuildingComp.java:869-871`), and
/// the reactor's cryofluid via `consumeLiquid`.
#[test]
fn mender_accepts_silicon_and_reactor_accepts_thorium() {
    let mut sim = runtime_sim();
    let mender = place_team1(&mut sim, "mender", 5, 5);
    let reactor = place_team1(&mut sim, "thorium-reactor", 8, 8);
    let silicon = item(&sim, "silicon");
    let copper = item(&sim, "copper");
    let thorium = item(&sim, "thorium");
    let cryofluid = liquid(&sim, "cryofluid");
    let water = liquid(&sim, "water");
    let world = &mut sim.ecs.0;
    assert!(
        transfer::dispatch_accept_item(world, mender, mender, silicon),
        "mender accepts silicon (consumeItem boost)"
    );
    assert!(
        !transfer::dispatch_accept_item(world, mender, mender, copper),
        "mender rejects copper"
    );
    assert!(
        transfer::dispatch_accept_item(world, reactor, reactor, thorium),
        "thorium reactor accepts thorium"
    );
    assert!(
        !transfer::dispatch_accept_item(world, reactor, reactor, copper),
        "thorium reactor rejects copper"
    );
    assert!(
        crate::world::blocks::liquid::movement::accept_liquid(world, reactor, reactor, cryofluid),
        "thorium reactor accepts cryofluid"
    );
    assert!(
        !crate::world::blocks::liquid::movement::accept_liquid(world, reactor, reactor, water),
        "thorium reactor rejects water"
    );
}

/// T1: `BuildingComp.canUnload` default (`Block.unloadable`, `Block.java:108`)
/// lets an unloader pull from a crafter that only produces into its own module.
///
/// The receiver must be a non-storage building: Java
/// `UnloaderBuild.isPossibleItem` requires `canLoad = notStorage && acceptItem`
/// (`Unloader.java:118`), so a bare container next to the unloader cannot be a
/// load target (the plan sketch's `container` receiver would fail in Java too);
/// use a belt, exactly like `unloader_drains_storage_into_a_lane`.
#[test]
fn unloader_pulls_from_crafter() {
    let mut sim = runtime_sim();
    let press = place_team1(&mut sim, "graphite-press", 5, 5);
    let graphite = item(&sim, "graphite");
    {
        let world = &mut sim.ecs.0;
        transfer::dispatch_handle_stack(world, press, graphite, 5);
    }
    assert_eq!(
        stored_item(&sim, press, graphite),
        5,
        "press holds graphite"
    );
    let unloader = place_team1(&mut sim, "unloader", 7, 5);
    let _ = unloader;
    configure(
        &mut sim,
        7,
        5,
        crate::determinism::ConfigValue::Content("graphite".to_owned()),
    );
    let belt = place_team1(&mut sim, "conveyor", 8, 5);
    tick_n(&mut sim, 60);
    assert!(
        stored_item(&sim, press, graphite) < 5,
        "unloader pulled graphite from the press (Java canUnload = block.unloadable)"
    );
    assert!(
        stored_item(&sim, belt, graphite) >= 1,
        "the receiving belt holds the pulled graphite"
    );
}

/// T1: Erekir heat crafters (which delegate to `CrafterBehavior`) accept their
/// consumers' inputs (`HeatCrafterBuild` extends `GenericCrafterBuild`).
#[test]
fn heat_crafter_accepts_inputs() {
    let mut sim = runtime_sim();
    let crucible = place_team1(&mut sim, "carbide-crucible", 5, 5);
    let tungsten = item(&sim, "tungsten");
    let graphite = item(&sim, "graphite");
    let copper = item(&sim, "copper");
    let world = &mut sim.ecs.0;
    assert!(transfer::dispatch_accept_item(
        world, crucible, crucible, tungsten
    ));
    assert!(transfer::dispatch_accept_item(
        world, crucible, crucible, graphite
    ));
    assert!(!transfer::dispatch_accept_item(
        world, crucible, crucible, copper
    ));
}

/// T5: a powered incinerator heats up (`Incinerator.java:30-81`:
/// `heat = approachDelta(heat, efficiency, 0.04)`), then accepts and destroys
/// belt items; before `heat > 0.5` it rejects them (`acceptItem`).
#[test]
fn incinerator_burns_belt_items() {
    let mut sim = runtime_sim();
    let _source = place_team1(&mut sim, "power-source", 4, 5);
    let incinerator = place_team1(&mut sim, "incinerator", 5, 5);
    let item_source = place_team1(&mut sim, "item-source", 6, 5);
    let copper = item(&sim, "copper");
    configure(
        &mut sim,
        6,
        5,
        crate::determinism::ConfigValue::Content("copper".to_owned()),
    );
    tick_n(&mut sim, 12);
    let heat = sim
        .ecs
        .0
        .get::<crate::entities::comp::IncineratorState>(incinerator)
        .map(|state| state.heat)
        .unwrap_or(0.0);
    assert!(
        heat < 0.5,
        "heat still below the acceptance threshold: {heat}"
    );
    assert!(
        !transfer::dispatch_accept_item(&sim.ecs.0, incinerator, item_source, copper),
        "cold incinerator rejects items"
    );
    tick_n(&mut sim, 18);
    let heat = sim
        .ecs
        .0
        .get::<crate::entities::comp::IncineratorState>(incinerator)
        .map(|state| state.heat)
        .unwrap_or(0.0);
    assert_eq!(heat, 1.0, "heat clamped at efficiency after ~25 ticks");
    assert!(
        transfer::dispatch_accept_item(&sim.ecs.0, incinerator, item_source, copper),
        "hot incinerator accepts any item"
    );
    tick_n(&mut sim, 30);
    assert!(
        emitted(&sim, item_source) > 0,
        "the item source ran and fed the incinerator"
    );
    assert_eq!(
        stored_item(&sim, incinerator, copper),
        0,
        "the incinerator stores nothing"
    );
}

/// T5: the item incinerator accepts while powered (`ItemIncinerator.java:36-66`)
/// and stores nothing; the slag incinerator is its behavior, not a crafter.
#[test]
fn item_incinerator_needs_power() {
    let mut sim = runtime_sim();
    let slag = place_team1(&mut sim, "slag-incinerator", 5, 5);
    let copper = item(&sim, "copper");
    {
        let world = &mut sim.ecs.0;
        world
            .get_mut::<crate::entities::comp::Building>(slag)
            .expect("building")
            .efficiency = 0.0;
        assert!(!transfer::dispatch_accept_item(world, slag, slag, copper));
        world
            .get_mut::<crate::entities::comp::Building>(slag)
            .expect("building")
            .efficiency = 1.0;
        assert!(transfer::dispatch_accept_item(world, slag, slag, copper));
        transfer::dispatch_handle_item(world, slag, slag, copper);
    }
    assert_eq!(stored_item(&sim, slag, copper), 0, "destroyed on intake");
}

/// T6: the drill period includes the item-hardness term
/// (`Drill.getDrillTime`, `Drill.java:170-172`): mechanical drill (drillTime
/// 600, hardnessDrillMultiplier 50, `Blocks.java:2884-2888`) on 4 copper
/// (hardness 1) tiles -> `(600 + 50*1)/4 = 162.5` ticks per item; the old
/// code used `delay = drill_time` -> 150 ticks per item.
#[test]
fn mechanical_drill_period_includes_hardness() {
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "ore-copper");
    }
    let _drill = place_team1(&mut sim, "mechanical-drill", 5, 5);
    let copper = item(&sim, "copper");
    let container = place_team1(&mut sim, "container", 7, 5);
    tick_n(&mut sim, 3000);
    let offloaded = stored_item(&sim, container, copper);
    assert!(
        (17..=19).contains(&offloaded),
        "expected ~18 items at 162.5 ticks/item over 3000 ticks, got {offloaded}"
    );
}

/// H04 T2: `BuildingComp.produced` (`BuildingComp.java:1044-1050`) records
/// default-team production into the shared campaign queues and unlocks the
/// item when it is offloaded.
#[test]
fn drill_offload_records_production() {
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "ore-copper");
    }
    let queues = crate::game::runtime::install_campaign_core_hooks(&mut sim.ecs.0);
    {
        let mut state = queues.lock().expect("lock");
        state.is_campaign = true;
        state.default_team = 1;
    }
    let copper = item(&sim, "copper");
    let _drill = place_team1(&mut sim, "mechanical-drill", 5, 5);
    let _container = place_team1(&mut sim, "container", 7, 5);
    tick_n(&mut sim, 600);
    let state = queues.lock().expect("lock");
    assert!(
        state.production_deltas.contains(&(copper, 1)),
        "offload queues produced(copper, 1): {:?}",
        state.production_deltas
    );
    assert!(
        state
            .unlocks
            .contains(&crate::game::runtime::UnlockRef::Item(copper)),
        "producing copper queues its unlock: {:?}",
        state.unlocks
    );
}

/// T6: burst drills emit one burst of `dominantItems` per `drillTime`
/// (`BurstDrillBuild.updateTile`, `BurstDrill.java:76-120`) instead of a
/// continuous trickle; impact-drill drillTime 720, 16 ore tiles -> 16 items at
/// once (`Blocks.java:3089-3110`).
#[test]
fn impact_drill_bursts() {
    let mut sim = runtime_sim();
    // Impact-drill is 4x4 at (5,5): footprint (4..7, 4..7) = 16 ore tiles
    // (`(-(size-1)/2) = -1` offset, the `valid_place_at` footprint rule).
    for y in 4..=7 {
        for x in 4..=7 {
            set_floor(&mut sim, x, y, "ore-copper");
        }
    }
    let drill = place_team1(&mut sim, "impact-drill", 5, 5);
    let copper = item(&sim, "copper");
    let _power = place_team1(&mut sim, "power-source", 8, 5);
    let _water = place_team1(&mut sim, "liquid-source", 5, 8);
    configure(
        &mut sim,
        5,
        8,
        crate::determinism::ConfigValue::Content("water".to_owned()),
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::entities::comp::DrillState>(drill)
            .map(|state| state.dominant_items),
        Some(16),
        "all 16 footprint tiles are ore"
    );
    tick_n(&mut sim, 710);
    assert_eq!(
        stored_item(&sim, drill, copper),
        0,
        "no trickle before the first burst"
    );
    tick_n(&mut sim, 15);
    assert_eq!(
        stored_item(&sim, drill, copper),
        16,
        "one burst of dominantItems items at drillTime"
    );
    let invert = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(drill)
        .map(|state| state.invert_time)
        .unwrap_or(0.0);
    assert!(invert > 0.0, "the burst latched invertTime: {invert}");
}
