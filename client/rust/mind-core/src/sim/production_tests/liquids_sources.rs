// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime pump, conduit, item-source and liquid-source regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// PAR-0078: a mechanical pump needs a liquid floor, pumps that floor's
/// liquid (tar -> oil) and dumps it into an adjacent conduit.
#[test]
fn pumps_read_the_floor_liquid_and_feed_conduits() {
    let mut sim = runtime_sim();
    set_floor(&mut sim, 5, 5, "tar");
    set_floor(&mut sim, 9, 5, "shallow-water");
    let dry = sim.world_block_id("mechanical-pump").expect("pump");
    sim.command(crate::determinism::SimCommand::Place {
        x: 12,
        y: 12,
        block: dry,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("command");
    assert!(
        sim.grid.entity_at(TilePos::new(12, 12)).is_none(),
        "pump rejected on a dry floor"
    );
    let oil_pump = place(&mut sim, "mechanical-pump", 5, 5);
    let water_pump = place(&mut sim, "mechanical-pump", 9, 5);
    let conduit = place(&mut sim, "conduit", 10, 5);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let (oil, water) = (liquid(&sim, "oil"), liquid(&sim, "water"));
    assert!(stored_liquid(&sim, oil_pump, oil) > 0.0, "tar pumps oil");
    assert_eq!(stored_liquid(&sim, oil_pump, water), 0.0);
    assert!(stored_liquid(&sim, water_pump, water) >= 0.0);
    assert!(
        stored_liquid(&sim, conduit, water) > 0.0,
        "pump dumped water into the conduit"
    );
}

/// PAR-0075 liquid half: a conduit feeds a steam generator's water consumer.
#[test]
fn conduit_feeds_a_liquid_consumer() {
    let mut sim = runtime_sim();
    set_floor(&mut sim, 5, 5, "shallow-water");
    let _pump = place(&mut sim, "mechanical-pump", 5, 5);
    let generator = place(&mut sim, "steam-generator", 6, 5);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let water = liquid(&sim, "water");
    assert!(
        stored_liquid(&sim, generator, water) > 0.0,
        "generator got water"
    );
}

/// PAR-0077: a configured item source emits copper into a conveyor, which
/// forwards it into an item void; the void accepts and discards it.
#[test]
fn item_source_feeds_conveyor_into_void() {
    let mut sim = runtime_sim();
    let copper = item(&sim, "copper");
    let source = place(&mut sim, "item-source", 5, 5);
    let conveyor = place(&mut sim, "conveyor", 6, 5);
    let sink = place(&mut sim, "item-void", 7, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Content("copper".to_owned()),
    );
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let count = emitted(&sim, source);
    let belt_heat = sim
        .ecs
        .0
        .get::<crate::world::blocks::distribution::conveyor::ConveyorBuild>(conveyor)
        .map(|belt| belt.clog_heat)
        .unwrap_or(1.0);
    assert!(count >= 100, "source emitted {count} items in 120 ticks");
    assert_eq!(
        stored_item(&sim, source, copper),
        0,
        "source clears its buffer after each emission"
    );
    assert!(
        stored_item(&sim, conveyor, copper) >= 1,
        "belt received items from the source"
    );
    // A clogged belt approaches `clog_heat` 1; the void drains the front, so
    // the belt keeps flowing.
    assert!(
        belt_heat < 0.5,
        "belt is not clogged (void drains it): {belt_heat}"
    );
    let world = &mut sim.ecs.0;
    assert!(
        transfer::dispatch_accept_item(world, sink, conveyor, copper),
        "item void accepts downstream items"
    );
    assert_eq!(stored_item(&sim, sink, copper), 0, "void stores nothing");
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::world::modules::ItemModule>(sink)
            .map(|module| module.total())
            .unwrap_or(0),
        0,
        "void has no item storage at all"
    );
}

/// PAR-0077: an unconfigured item source emits nothing and accepts nothing
/// (Java default `outputItem = null`, `acceptItem` returns false).
#[test]
fn unconfigured_item_source_emits_nothing() {
    let mut sim = runtime_sim();
    let copper = item(&sim, "copper");
    let source = place(&mut sim, "item-source", 5, 5);
    let conveyor = place(&mut sim, "conveyor", 6, 5);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        emitted(&sim, source),
        0,
        "unconfigured source emits nothing"
    );
    assert_eq!(stored_item(&sim, conveyor, copper), 0);
    let world = &mut sim.ecs.0;
    assert!(
        !transfer::dispatch_accept_item(world, source, source, copper),
        "item source accepts nothing"
    );
}

/// PAR-0077: a configured liquid source fills itself and dumps water into a
/// conduit; clearing the config empties the source.
#[test]
fn liquid_source_fills_conduit() {
    let mut sim = runtime_sim();
    let water = liquid(&sim, "water");
    let source = place(&mut sim, "liquid-source", 5, 5);
    let conduit = place(&mut sim, "conduit", 6, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Content("water".to_owned()),
    );
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_liquid(&sim, conduit, water) > 0.0,
        "conduit carried water"
    );
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::None);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_liquid(&sim, source, water),
        0.0,
        "cleared source empties its tank"
    );
}

/// H09 T6: `BuildingComp.onDeconstructed` (`BuildingComp.java:1478-1487`) drops
/// `currentAmount / (size * size) * 2` per footprint tile for a
/// non-incinerable liquid (neoplasm); an incinerable one (slag) is not dropped.
#[test]
fn deconstructing_tank_drops_non_incinerable_puddles() {
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let neph = liquid(&sim, "neoplasm");
    let tank = place(&mut sim, "liquid-tank", 6, 6);
    {
        let mut liquids = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::LiquidModule>(tank)
            .expect("liquids");
        liquids.add(neph, 1500.0, 1500.0);
    }
    // The player deconstruct path starts through `Build.beginBreak`
    // (`Command::Break` is the instant/API break), so call it directly.
    {
        let crate::sim::Sim {
            block_runtime,
            ecs,
            grid,
            ..
        } = &mut sim;
        let runtime = block_runtime.as_mut().expect("block runtime");
        assert!(runtime.begin_break(&mut ecs.0, grid, 6, 6, 1, 1.0));
    }
    // 3x3 footprint (5..7, 5..7): 1500/9*2 = 333 per tile, capped at
    // maxLiquid (70).
    for y in 5..8 {
        for x in 5..8 {
            let puddle = crate::combat::puddles::find_at(&sim.ecs.0, x, y)
                .and_then(|entity| {
                    sim.ecs
                        .0
                        .get::<crate::combat::puddles::PuddleState>(entity)
                        .copied()
                })
                .unwrap_or_else(|| panic!("puddle at ({x}, {y})"));
            assert_eq!(puddle.liquid, neph);
            assert!(
                (puddle.amount - crate::combat::puddles::MAX_LIQUID).abs() < 1e-3,
                "({x}, {y}) amount {}",
                puddle.amount
            );
        }
    }

    // Incinerable slag is left behind (Java `!liquids.current().incinerable`).
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let slag = liquid(&sim, "slag");
    let tank = place(&mut sim, "liquid-tank", 6, 6);
    {
        let mut liquids = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::LiquidModule>(tank)
            .expect("liquids");
        liquids.add(slag, 1500.0, 1500.0);
    }
    {
        let crate::sim::Sim {
            block_runtime,
            ecs,
            grid,
            ..
        } = &mut sim;
        let runtime = block_runtime.as_mut().expect("block runtime");
        assert!(runtime.begin_break(&mut ecs.0, grid, 6, 6, 1, 1.0));
    }
    for y in 5..8 {
        for x in 5..8 {
            assert!(
                crate::combat::puddles::find_at(&sim.ecs.0, x, y).is_none(),
                "no puddle at ({x}, {y}) for incinerable slag"
            );
        }
    }
}

/// PAR-0077: a power source feeds an adjacent power consumer; the graph reports
/// full satisfaction.
#[test]
fn power_source_powers_consumer() {
    let mut sim = runtime_sim();
    set_floor(&mut sim, 7, 5, "ore-copper");
    let source = place(&mut sim, "power-source", 5, 5);
    let drill = place(&mut sim, "laser-drill", 7, 5);
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let production = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(source)
        .map(|production| production.0)
        .unwrap_or(0.0);
    assert!(
        (production - 1_000_000.0 / 60.0).abs() < 0.01,
        "power source reports its production: {production}"
    );
    let graph = sim
        .ecs
        .0
        .get::<crate::world::modules::PowerModule>(drill)
        .map(|module| module.graph)
        .unwrap_or_default();
    let satisfaction = sim
        .ecs
        .0
        .resource::<crate::world::blocks::power::PowerGrids>()
        .graph(graph)
        .map(|graph| graph.get_satisfaction())
        .unwrap_or(0.0);
    assert_eq!(satisfaction, 1.0, "power graph satisfaction");
    let efficiency = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(drill)
        .map(|building| building.efficiency)
        .unwrap_or(0.0);
    assert!(efficiency > 0.0, "drill became efficient: {efficiency}");
}

/// T14 (PAR-0286): breaking and re-placing a power building keeps the consumer
/// on the powered graph. The split (`PowerGraph.remove`) must run while the
/// removed building's proximity still lists its neighbours, and the re-placed
/// building merges back (`Building.updatePowerGraph`).
#[test]
fn power_source_replacement_keeps_consumer_powered() {
    let mut sim = runtime_sim();
    set_floor(&mut sim, 8, 5, "ore-copper");
    // Report staging order: machine -> power-node -> power-source.
    let drill = place(&mut sim, "laser-drill", 8, 5);
    let _node = place(&mut sim, "power-node", 6, 5);
    let _source = place(&mut sim, "power-source", 5, 5);
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let satisfaction = |sim: &Sim, entity| -> f32 {
        let graph = sim
            .ecs
            .0
            .get::<crate::world::modules::PowerModule>(entity)
            .map(|module| module.graph)
            .unwrap_or_default();
        sim.ecs
            .0
            .resource::<crate::world::blocks::power::PowerGrids>()
            .graph(graph)
            .map(|graph| graph.get_satisfaction())
            .unwrap_or(0.0)
    };
    assert_eq!(satisfaction(&sim, drill), 1.0, "drill powered before");

    // A large multiblock (adjacent to the drill) breaks and is re-placed: it
    // must rejoin the powered graph.
    let battery = place(&mut sim, "battery-large", 6, 8);
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        satisfaction(&sim, battery),
        1.0,
        "battery powered before the break"
    );
    sim.command(SimCommand::Break {
        x: 6,
        y: 8,
        player: None,
    })
    .expect("break battery");
    sim.tick().expect("tick");
    let battery = place(&mut sim, "battery-large", 6, 8);
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        satisfaction(&sim, battery),
        1.0,
        "battery stays powered after the replacement"
    );

    // The report's simple case: break + re-place the source itself.
    sim.command(SimCommand::Break {
        x: 5,
        y: 5,
        player: None,
    })
    .expect("break source");
    sim.tick().expect("tick");
    let _replacement = place(&mut sim, "power-source", 5, 5);
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        satisfaction(&sim, drill),
        1.0,
        "drill stays powered after the source replacement"
    );
}

/// §10 sweep (laser/airblast drills): the ore footprint is centred on
/// `Building.tile` (`Drill.countOre` over `Tile.getLinkedTilesAs`, offset
/// `-(size-1)/2`), so ore under the left column of a 3x3 counts and a tile
/// outside the footprint does not (the old scan ran `x..x+size` from the
/// centre tile).
#[test]
fn laser_drill_counts_centered_footprint() {
    let mut sim = runtime_sim();
    for y in 7..=9 {
        set_floor(&mut sim, 7, y, "ore-copper");
    }
    // Outside the footprint; the old `x..x+size` scan would have counted it.
    set_floor(&mut sim, 10, 8, "ore-copper");
    let drill = place(&mut sim, "laser-drill", 8, 8);
    let (dominant, count) = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(drill)
        .map(|state| (state.dominant_item, state.dominant_items))
        .expect("drill state");
    assert_eq!(count, 3, "only the left column is ore");
    assert_eq!(dominant, Some(item(&sim, "copper")));
}

/// §10 sweep (generators with `outputLiquid`): `ConsumeGeneratorBuild.updateTile`
/// calls `dumpLiquid(outputLiquid.liquid)` right after the output add, so a
/// conduit next to the generator receives the produced liquid.
#[test]
fn generator_dumps_output_liquid() {
    let mut sim = runtime_sim();
    let generator = place(&mut sim, "pyrolysis-generator", 5, 5);
    let conduit = place(&mut sim, "conduit", 7, 5);
    let (slag, arkycite, water) = (
        liquid(&sim, "slag"),
        liquid(&sim, "arkycite"),
        liquid(&sim, "water"),
    );
    {
        let world = &mut sim.ecs.0;
        if let Some(mut module) = world.get_mut::<crate::world::modules::LiquidModule>(generator) {
            module.add(slag, 50.0, 150.0);
            module.add(arkycite, 50.0, 150.0);
        }
    }
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_liquid(&sim, conduit, water) > 0.0,
        "conduit received the dumped output liquid"
    );
}
