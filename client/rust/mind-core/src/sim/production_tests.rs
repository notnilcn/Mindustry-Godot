// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime production/logistics regressions (PAR-0075/0076/0077/0078):
//! blocks placed through `Sim` with the block runtime installed.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::world::TilePos;

use super::Sim;
use crate::determinism::SimCommand;
use crate::world::blocks::distribution::transfer;

fn place(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity {
    let id = sim.world_block_id(name).expect("block");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    sim.grid.entity_at(TilePos::new(x, y)).expect("entity")
}

fn item(sim: &Sim, name: &str) -> crate::content::ItemId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .item_id(name)
        .expect("item")
}

/// PAR-0075: a combustion generator accepts coal (not copper) and makes
/// power from it.
#[test]
fn combustion_generator_accepts_coal_and_generates() {
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let generator = place(&mut sim, "combustion-generator", 8, 8);
    let coal = item(&sim, "coal");
    let copper = item(&sim, "copper");
    let world = &mut sim.ecs.0;
    assert!(transfer::dispatch_accept_item(
        world, generator, generator, coal
    ));
    assert!(!transfer::dispatch_accept_item(
        world, generator, generator, copper
    ));
    transfer::dispatch_handle_item(world, generator, generator, coal);
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let production = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    assert!(production > 0.0, "production {production}");
}

fn liquid(sim: &Sim, name: &str) -> crate::content::LiquidId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .liquid_id(name)
        .expect("liquid")
}

fn set_floor(sim: &mut Sim, x: i32, y: i32, name: &str) {
    let floor = BlockId::new(sim.world_block_id(name).expect("floor"));
    sim.grid.tiles.get_mut(x, y).floor = floor;
}

fn stored_liquid(sim: &Sim, e: Entity, liquid: crate::content::LiquidId) -> f32 {
    sim.ecs
        .0
        .get::<crate::world::modules::LiquidModule>(e)
        .map(|m| m.get(liquid))
        .unwrap_or(0.0)
}

fn runtime_sim() -> Sim {
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim
}

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

fn configure(sim: &mut Sim, x: i16, y: i16, value: crate::determinism::ConfigValue) {
    sim.command(SimCommand::Configure { x, y, value })
        .expect("configure");
}

fn stored_item(sim: &Sim, e: Entity, item: crate::content::ItemId) -> i32 {
    sim.ecs
        .0
        .get::<crate::world::modules::ItemModule>(e)
        .map(|module| module.get(item))
        .unwrap_or(0)
}

fn emitted(sim: &Sim, e: Entity) -> u32 {
    sim.ecs
        .0
        .get::<crate::entities::comp::SandboxState>(e)
        .map(|state| state.emitted)
        .unwrap_or(0)
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

/// PAR-0076: an additive reconstructor only upgrades a payload with power and
/// its 40 silicon + 40 graphite, after `constructTime` (600) ticks, and consumes
/// those items on completion.
#[test]
fn reconstructor_needs_items_and_time() {
    let mut sim = runtime_sim();
    let _source = place(&mut sim, "power-source", 3, 5);
    let rig = place(&mut sim, "additive-reconstructor", 5, 5);
    let (silicon, graphite) = (item(&sim, "silicon"), item(&sim, "graphite"));
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

    // Load a dagger payload through the behavior hook (`acceptPayload`).
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
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::world::blocks::units::ReconstructorBuild>(rig)
            .and_then(|state| state.payload),
        Some(dagger)
    );

    // No items: the payload waits, nothing spawns.
    for _ in 0..700 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::world::blocks::units::ReconstructorBuild>(rig)
            .and_then(|state| state.payload),
        Some(dagger),
        "payload waits without items"
    );
    assert!(!spawned_unit(&sim, mace), "no upgrade without items");

    // Feed the required stacks: the upgrade lands after constructTime ticks.
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(rig) {
            items.add(silicon, 40, 80);
            items.add(graphite, 40, 80);
        }
    }
    let start = sim.tick_count();
    for _ in 0..900 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, mace) {
            break;
        }
    }
    let elapsed = sim.tick_count() - start;
    assert!(spawned_unit(&sim, mace), "upgraded to a mace");
    assert!(
        (600..=700).contains(&elapsed),
        "construct time {elapsed} ticks"
    );
    assert_eq!(stored_item(&sim, rig, silicon), 0, "silicon consumed");
    assert_eq!(stored_item(&sim, rig, graphite), 0, "graphite consumed");
}

/// §7.0 (config prerequisite): free text reaches a building through the wire
/// `Text` value and reads back as the behavior's `String` config.
#[test]
fn message_block_accepts_text_config() {
    let mut sim = runtime_sim();
    let message = place(&mut sim, "message", 5, 5);
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Text("hello\nworld".to_owned()),
    })
    .expect("configure");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, message),
        crate::world::config::ConfigValue::String("hello\nworld".to_owned())
    );
}

/// Whether a unit of type `unit` exists in the sim.
fn spawned_unit(sim: &Sim, unit: crate::content::UnitTypeId) -> bool {
    sim.ecs.0.iter_entities().any(|entity| {
        entity
            .get::<crate::entities::comp::unit::UnitTypeComp>()
            .map(|comp| comp.type_id)
            == Some(unit)
    })
}
