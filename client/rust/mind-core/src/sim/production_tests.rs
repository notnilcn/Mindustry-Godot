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

/// PAR-0092: `UnitFactory.config(UnitType.class)` selects the matching plan
/// (resetting progress), `config()` reports the index and `None` clears it; with
/// no plan selected nothing is produced even with the items in storage.
#[test]
fn unit_factory_config_selects_plan() {
    let mut sim = runtime_sim();
    let _source = place(&mut sim, "power-source", 3, 5);
    let factory = place(&mut sim, "air-factory", 5, 5);
    let silicon = item(&sim, "silicon");
    let content_flare = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("flare")
        .expect("flare");
    let mono = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("mono")
        .expect("mono");

    // `created()` auto-selects the first `unlockedNow()` plan; the runtime
    // registry never carries campaign research, so the first plan (flare) is
    // the fallback default.
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::Number(0.0)
    );

    // Configure a plan by unit name (the config panel's content submit path).
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Content("mono".to_owned()),
    })
    .expect("configure mono");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::Number(1.0)
    );

    // Clearing removes the plan: storage full of requirements still builds nothing.
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::None,
    })
    .expect("clear plan");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::None
    );
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(factory) {
            items.add(silicon, 60, 100);
        }
    }
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    assert!(
        !spawned_unit(&sim, content_flare) && !spawned_unit(&sim, mono),
        "no plan selected -> nothing produced"
    );

    // Re-select flare: the 15 silicon plan completes after its 900 ticks.
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Content("flare".to_owned()),
    })
    .expect("configure flare");
    for _ in 0..1000 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, content_flare) {
            break;
        }
    }
    assert!(spawned_unit(&sim, content_flare), "flare produced");
}

/// PAR-0079: a plasma bore mines the Erekir wall ore in front of it
/// (`BeamDrill.updateFacing` + `canPlaceOn` scan the facing edge, not the
/// footprint).
#[test]
fn plasma_bore_mines_wall_ore() {
    let mut sim = runtime_sim();
    // `ore-wall-beryllium` two tiles in front of the facing edge (rotation 0);
    // `BeamDrill.canPlaceOn` requires it before the block can be placed.
    let ore =
        crate::content::BlockId::new(sim.world_block_id("ore-wall-beryllium").expect("ore wall"));
    // Java maps put `ore-wall-*` as the *overlay* of a static wall
    // (`Tile.wallDrop()`), never as the block itself.
    let wall = crate::content::BlockId::new(
        sim.world_block_id("beryllic-stone-wall")
            .expect("static wall"),
    );
    for y in [5, 6] {
        let tile = sim.grid.tiles.get_mut(9, y);
        tile.block = wall;
        tile.overlay = ore;
    }
    let _source = place(&mut sim, "power-source", 4, 5);
    let bore = place(&mut sim, "plasma-bore", 5, 5);
    {
        let crate::sim::Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = &mut sim;
        let content = block_runtime.as_ref().expect("runtime").content();
        crate::world::behavior::production::refresh_drill_ore(&mut ecs.0, grid, content, bore);
    }
    let state = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(bore)
        .expect("drill state");
    assert_eq!(
        state.facing_amount, 2.0,
        "two facing edges see the wall ore"
    );

    let beryllium = item(&sim, "beryllium");
    for _ in 0..400 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_item(&sim, bore, beryllium) > 0,
        "plasma bore produced beryllium"
    );
}

/// PAR-0079: a cliff crusher turns the sand attribute of the walls in front into
/// sand (`WallCrafter.getEfficiency`/`updateTile`).
#[test]
fn cliff_crusher_makes_sand_from_sand_walls() {
    let mut sim = runtime_sim();
    // `sand-wall` carries `Attribute.sand = 2` (Java Blocks.java);
    // `WallCrafter.canPlaceOn` requires it before the block can be placed.
    let wall = crate::content::BlockId::new(sim.world_block_id("sand-wall").expect("sand-wall"));
    sim.grid.tiles.get_mut(7, 5).block = wall;
    sim.grid.tiles.get_mut(7, 6).block = wall;
    let _source = place(&mut sim, "power-source", 4, 5);
    let crusher = place(&mut sim, "cliff-crusher", 5, 5);
    {
        let crate::sim::Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = &mut sim;
        let content = block_runtime.as_ref().expect("runtime").content();
        crate::world::behavior::production::refresh_drill_ore(&mut ecs.0, grid, content, crusher);
    }
    let eff = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(crusher)
        .map(|state| state.facing_amount)
        .unwrap_or(0.0);
    assert_eq!(eff, 4.0, "two facing edges * sand 2");

    let sand = item(&sim, "sand");
    for _ in 0..40 {
        sim.tick().expect("tick");
    }
    for _ in 0..400 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_item(&sim, crusher, sand) > 0,
        "cliff crusher produced sand"
    );
}

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

/// T7 (`SolidPump.updateTile`, `SolidPump.java:116-135`; `Blocks.java:2941-2948`):
/// the water extractor's yield is `pumpAmount * (validTiles + boost)` where
/// `validTiles = 1` on any pumpable floor (`baseEfficiency = 1`) and `boost` is
/// the footprint water attribute / size². Four `mud` tiles (`water = 1`) double
/// the rate.
#[test]
fn water_extractor_scales_with_floor() {
    let mut sim = runtime_sim();
    let _plain_source = place(&mut sim, "power-source", 4, 5);
    let _wet_source = place(&mut sim, "power-source", 11, 5);
    let plain = place(&mut sim, "water-extractor", 5, 5);
    for (x, y) in [(12, 5), (13, 5), (12, 6), (13, 6)] {
        set_floor(&mut sim, x, y, "mud");
    }
    let wet = place(&mut sim, "water-extractor", 12, 5);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let water = liquid(&sim, "water");
    let plain_amount = stored_liquid(&sim, plain, water);
    let wet_amount = stored_liquid(&sim, wet, water);
    assert!(plain_amount > 0.0, "plain extractor pumped {plain_amount}");
    assert!(
        (wet_amount / plain_amount - 2.0).abs() < 0.1,
        "wet {wet_amount} vs plain {plain_amount}: the water attribute doubles it"
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
    let _source = place(&mut sim, "power-source", 3, 5);
    let extractor = place(&mut sim, "oil-extractor", 5, 5);
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
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
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
    let _source = place(&mut sim, "power-source", 4, 5);
    let bore = place(&mut sim, "plasma-bore", 5, 5);
    let beryllium = item(&sim, "beryllium");
    for _ in 0..400 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..400 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, bore, beryllium),
        before,
        "no production once the ore is gone"
    );
}

/// T13 (`Door.tapped`, `Door.java:148-154`): a tap toggles `open`; a second tap
/// within 60 ticks is ignored (`origin().timer(timerToggle, 60f)`).
#[test]
fn door_toggles_and_throttles() {
    let mut sim = runtime_sim();
    let door = place(&mut sim, "door", 5, 5);
    let is_open = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::entities::comp::DoorState>(door)
            .map(|state| state.open)
            .unwrap_or(false)
    };
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Bool(true));
    assert!(is_open(&sim), "opened on the first tap");
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Bool(false));
    assert!(is_open(&sim), "second toggle within 60 ticks ignored");
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Bool(false));
    assert!(!is_open(&sim), "closed after the throttle");
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
    let drill = place(&mut sim, "mechanical-drill", 5, 5);
    let water = liquid(&sim, "water");
    let _source = place(&mut sim, "liquid-source", 4, 5);
    configure(
        &mut sim,
        4,
        5,
        crate::determinism::ConfigValue::Content("water".to_owned()),
    );
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
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
    let mender = place(&mut sim, "mender", 5, 5);
    let reactor = place(&mut sim, "thorium-reactor", 8, 8);
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
    let press = place(&mut sim, "graphite-press", 5, 5);
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
    let _unloader = place(&mut sim, "unloader", 7, 5);
    configure(
        &mut sim,
        7,
        5,
        crate::determinism::ConfigValue::Content("graphite".to_owned()),
    );
    let belt = place(&mut sim, "conveyor", 8, 5);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
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
    let crucible = place(&mut sim, "carbide-crucible", 5, 5);
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
    let _source = place(&mut sim, "power-source", 4, 5);
    let incinerator = place(&mut sim, "incinerator", 5, 5);
    let item_source = place(&mut sim, "item-source", 6, 5);
    let copper = item(&sim, "copper");
    configure(
        &mut sim,
        6,
        5,
        crate::determinism::ConfigValue::Content("copper".to_owned()),
    );
    for _ in 0..12 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..18 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
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
    let slag = place(&mut sim, "slag-incinerator", 5, 5);
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
    let _drill = place(&mut sim, "mechanical-drill", 5, 5);
    let copper = item(&sim, "copper");
    let container = place(&mut sim, "container", 7, 5);
    for _ in 0..3000 {
        sim.tick().expect("tick");
    }
    let offloaded = stored_item(&sim, container, copper);
    assert!(
        (17..=19).contains(&offloaded),
        "expected ~18 items at 162.5 ticks/item over 3000 ticks, got {offloaded}"
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
    let drill = place(&mut sim, "impact-drill", 5, 5);
    let copper = item(&sim, "copper");
    let _power = place(&mut sim, "power-source", 8, 5);
    let _water = place(&mut sim, "liquid-source", 5, 8);
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
    for _ in 0..710 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, drill, copper),
        0,
        "no trickle before the first burst"
    );
    for _ in 0..15 {
        sim.tick().expect("tick");
    }
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

/// T2: a live-placed power node keeps its behavior knobs
/// (`Blocks.java:2483-2505`): power-node-large 15/15, surge-tower 2/40.
/// Before the fix `SimHooks::new_building` overwrote the config with the
/// default (3, 6.0) right after `create_state`.
#[test]
fn large_node_keeps_knobs_on_live_place() {
    use crate::world::blocks::power::PowerNodeConfig;
    let mut sim = runtime_sim();
    let large = place(&mut sim, "power-node-large", 5, 5);
    let surge = place(&mut sim, "surge-tower", 10, 5);
    let large_config = sim
        .ecs
        .0
        .get::<PowerNodeConfig>(large)
        .cloned()
        .expect("large node config");
    assert_eq!(
        (large_config.max_nodes, large_config.laser_range),
        (15, 15.0)
    );
    let surge_config = sim
        .ecs
        .0
        .get::<PowerNodeConfig>(surge)
        .cloned()
        .expect("surge tower config");
    assert_eq!(
        (surge_config.max_nodes, surge_config.laser_range),
        (2, 40.0)
    );
}

/// T2: a Configure with a packed target toggles a node link
/// (`PowerNode.java:419-441`): link -> both ends share a graph; toggling again
/// unlinks and the graphs split.
#[test]
fn node_configure_toggles_link() {
    use crate::world::blocks::power::PowerNodeConfig;
    let mut sim = runtime_sim();
    let _battery = place(&mut sim, "battery", 10, 5);
    let node = place(&mut sim, "power-node", 5, 5);
    assert!(sim.ecs.0.get::<PowerNodeConfig>(node).is_some());
    let pack = TilePos::new(10, 5).pack();
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let linked = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::world::modules::PowerModule>(node)
            .is_some_and(|module| module.links.contains(&pack))
    };
    let graph_of = |sim: &Sim, pos: (i16, i16)| {
        sim.grid
            .entity_at(TilePos::new(pos.0, pos.1))
            .and_then(|e| sim.ecs.0.get::<crate::world::modules::PowerModule>(e))
            .map(|module| module.graph)
    };
    // Reach the linked state deterministically: the node's `placed()` autolink
    // may or may not have linked the battery first.
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Int(pack));
    if !linked(&sim) {
        configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Int(pack));
    }
    assert!(linked(&sim), "configure links the battery");
    assert_eq!(
        graph_of(&sim, (5, 5)),
        graph_of(&sim, (10, 5)),
        "linked ends share a graph"
    );
    // Toggle again: unlink and reflow both ends.
    configure(&mut sim, 5, 5, crate::determinism::ConfigValue::Int(pack));
    assert!(!linked(&sim), "second configure unlinks");
    assert_ne!(
        graph_of(&sim, (5, 5)),
        graph_of(&sim, (10, 5)),
        "unlinked ends split graphs"
    );
}

/// T2: a diode between a charged battery graph and an empty one transfers
/// power (`PowerDiode.java:47-80`); the diode is wired for the first time here.
#[test]
fn diode_moves_power_forward() {
    let mut sim = runtime_sim();
    let a = place(&mut sim, "battery", 2, 5);
    let diode = place(&mut sim, "diode", 3, 5);
    let b = place(&mut sim, "battery", 4, 5);
    let _ = diode;
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    sim.ecs
        .0
        .get_mut::<crate::world::modules::PowerModule>(a)
        .expect("battery a module")
        .status = 1.0;
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let status = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::world::modules::PowerModule>(e)
            .map(|module| module.status)
            .unwrap_or(0.0)
    };
    let (a_status, b_status) = (status(&sim, a), status(&sim, b));
    assert!(
        b_status > 0.0,
        "the empty battery received power: {b_status}"
    );
    assert!(a_status < 1.0, "the full battery drained: {a_status}");
}

/// T4: a mender only heals its own team's buildings
/// (`BlockIndexer.eachBlock` is team-filtered, `BlockIndexer.java:328-361`;
/// `MendProjector.java:107-116`).
#[test]
fn mender_heals_only_own_team() {
    let mut sim = runtime_sim();
    let _mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let own = place(&mut sim, "copper-wall", 6, 5);
    let wall = sim.world_block_id("copper-wall").expect("wall");
    sim.command(SimCommand::Place {
        x: 5,
        y: 4,
        block: wall,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("enemy wall placed");
    let enemy = sim
        .grid
        .entity_at(TilePos::new(5, 4))
        .expect("enemy wall entity");
    {
        let world = &mut sim.ecs.0;
        for entity in [own, enemy] {
            world
                .get_mut::<crate::entities::comp::Health>(entity)
                .expect("health")
                .health = 10.0;
        }
    }
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let health_of = |sim: &Sim, entity: Entity| {
        sim.ecs
            .0
            .get::<crate::entities::comp::Health>(entity)
            .map(|health| health.health)
            .unwrap_or(0.0)
    };
    let own_health = health_of(&sim, own);
    assert!(
        (own_health - 22.8).abs() < 0.01,
        "own wall healed by 4% of 320: {own_health}"
    );
    assert_eq!(health_of(&sim, enemy), 10.0, "the enemy wall is not healed");
}

/// T4: the mender's silicon boost is consumed on its `useTime` cycle
/// (`MendProjector.java:95-104`: `timer(timerUse, useTime / timeScale)` ->
/// `consume()`), and `phaseHeat` follows `optionalEfficiency`
/// (`MendProjector.java:100`).
#[test]
fn mender_silicon_boost_consumed() {
    let mut sim = runtime_sim();
    let mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let silicon = item(&sim, "silicon");
    {
        let world = &mut sim.ecs.0;
        transfer::dispatch_handle_stack(world, mender, silicon, 2);
    }
    assert_eq!(stored_item(&sim, mender, silicon), 2);
    // The power graph settles a few ticks in; `optionalEfficiency` is capped by
    // the required (power) efficiency, so `consume()` starts once powered.
    // `timer()`'s first call is true: that first useTime tick consumes one.
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, mender, silicon),
        1,
        "one silicon consumed on the first powered useTime tick"
    );
    for _ in 0..51 {
        sim.tick().expect("tick");
    }
    let phase_heat = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::shields::MendProjectorState>(mender)
        .map(|state| state.phase_heat)
        .unwrap_or(0.0);
    assert!(phase_heat > 0.9, "phaseHeat warmed up: {phase_heat}");
    for _ in 0..349 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, mender, silicon),
        0,
        "the second useTime tick (400 ticks later) consumed the last silicon"
    );
}

/// T4: the overdrive projector boosts only its own team
/// (`OverdriveProjector.java:105-113` -> team-filtered `eachBlock`) and
/// consumes its boost items on the `useTime` cycle.
#[test]
fn overdrive_does_not_boost_enemy() {
    let mut sim = runtime_sim();
    // The overdrive is 2x2 at (5,5) (footprint 5..6, 5..6); the source sits
    // outside the footprint at (4,5).
    let _overdrive = place(&mut sim, "overdrive-projector", 5, 5);
    let _power = place(&mut sim, "power-source", 4, 5);
    let own = place(&mut sim, "graphite-press", 8, 5);
    let press = sim.world_block_id("graphite-press").expect("press");
    sim.command(SimCommand::Place {
        x: 5,
        y: 9,
        block: press,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("enemy press placed");
    let enemy = sim
        .grid
        .entity_at(TilePos::new(5, 9))
        .expect("enemy press entity");
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let time_scale = |sim: &Sim, entity: Entity| {
        sim.ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .map(|building| building.time_scale)
            .unwrap_or(1.0)
    };
    let own_scale = time_scale(&sim, own);
    assert!(
        (own_scale - 1.5).abs() < 0.001,
        "own press is boosted to 1.5: {own_scale}"
    );
    assert_eq!(
        time_scale(&sim, enemy),
        1.0,
        "the enemy press is not boosted"
    );
}

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
