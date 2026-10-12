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

/// Whether a unit of type `unit` exists in the sim.
fn spawned_unit(sim: &Sim, unit: crate::content::UnitTypeId) -> bool {
    sim.ecs.0.iter_entities().any(|entity| {
        entity
            .get::<crate::entities::comp::unit::UnitTypeComp>()
            .map(|comp| comp.type_id)
            == Some(unit)
    })
}
