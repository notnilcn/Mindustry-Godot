// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! I03 T3 sensor/fetch/printchar conformance plus the logic sense `family` overrides and PAR-0218 audit.
//! Upstream: core/src/mindustry/logic/LExecutor.java.

use super::world_effects::{processor_num, processor_obj};
use super::*;

// ---- I03 T3: sensor/fetch/printchar tables (PAR-0218) ----

/// The content `sense` tables (`Block/UnitType/Item/Liquid.sense`).
#[test]
fn content_sense_table() {
    let mut sim = campaign_sim();
    place_world_processor(
        &mut sim,
        10,
        10,
        "sensor h @dagger @health\nsensor n @copper @name\nsensor c @copper @color\nstop\n",
    );
    tick(&mut sim, 5);
    let dagger_health = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_by_name("dagger")
        .expect("dagger")
        .health;
    assert_eq!(processor_num(&sim, 10, 10, "h"), Some(dagger_health as f64));
    assert_eq!(
        processor_obj(&sim, 10, 10, "n"),
        Some(LogicObject::Str("copper".to_owned()))
    );
    let color = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("copper")
        .and_then(|id| sim.block_runtime().expect("runtime").content().item(id))
        .expect("copper")
        .color;
    let expected = crate::logic::globals::rgba_u8_to_double_bits(
        (color.r * 255.0) as i32,
        (color.g * 255.0) as i32,
        (color.b * 255.0) as i32,
        (color.a * 255.0) as i32,
    );
    assert_eq!(processor_num(&sim, 10, 10, "c"), Some(expected));
    assert_events_drained(&sim);
}

/// `BuildingComp.senseObject` has no `name`/`id` row: both read null.
#[test]
fn building_name_and_id_are_null() {
    let mut sim = campaign_sim();
    let container = place(&mut sim, "container", 12, 12);
    place_world_processor(
        &mut sim,
        10,
        10,
        "fetch build b @sharded 0 @container\nsensor n b @name\nsensor i b @id\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(
        processor_obj(&sim, 10, 10, "b"),
        Some(LogicObject::Building(container))
    );
    assert_eq!(processor_obj(&sim, 10, 10, "n"), None, "@name is null");
    assert_eq!(processor_obj(&sim, 10, 10, "i"), None, "@id is null");
    assert_events_drained(&sim);
}

/// Unit `@range` (`range()/8`) and `@armor` (the type armor, or the override).
#[test]
fn unit_range_and_armor() {
    let mut sim = campaign_sim();
    place_world_processor(
        &mut sim,
        10,
        10,
        "sensor a @mace @armor\nsensor r @mace @range\nstop\n",
    );
    tick(&mut sim, 5);
    let def = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_by_name("mace")
        .expect("mace")
        .clone();
    assert_eq!(processor_num(&sim, 10, 10, "a"), Some(def.armor as f64));
    let range = if def.max_range > 0.0 {
        def.max_range
    } else {
        def.range
    };
    assert_eq!(processor_num(&sim, 10, 10, "r"), Some((range / 8.0) as f64));
    assert_events_drained(&sim);
}

/// A liquid turret's `@ammo` is `totalAmmo` (0), not the liquid amount
/// (`Turret.sense`; liquid turrets never increment `totalAmmo`).
#[test]
fn liquid_turret_ammo_is_total_ammo() {
    let mut sim = campaign_sim();
    let wave = place(&mut sim, "wave", 12, 12);
    let water = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .liquid_id("water")
        .expect("water");
    sim.ecs
        .0
        .get_mut::<crate::world::modules::LiquidModule>(wave)
        .expect("liquids")
        .add(water, 100.0, 100.0);
    let links = [LogicLink::new("wave1", 2, 2)];
    place_linked_world_processor(&mut sim, 10, 10, "sensor a wave1 @ammo\nstop\n", &links);
    tick(&mut sim, 5);
    assert_eq!(processor_num(&sim, 10, 10, "a"), Some(0.0));
    assert_events_drained(&sim);
}

/// `fetch core`/`fetch coreCount` read the team's live core list.
#[test]
fn fetch_core_and_count() {
    let mut sim = campaign_sim();
    let core = place(&mut sim, "core-shard", 20, 20);
    place_world_processor(
        &mut sim,
        10,
        10,
        "fetch core c @sharded 0 @air\nfetch coreCount k @sharded 0 @air\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(
        processor_obj(&sim, 10, 10, "c"),
        Some(LogicObject::Building(core))
    );
    assert_eq!(processor_num(&sim, 10, 10, "k"), Some(1.0));
    assert_events_drained(&sim);
}

/// `fetch player`/`fetch playerCount` return the local player unit at index 0.
#[test]
fn fetch_player() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    let player = sim
        .spawn_player_unit("alpha", 1, 80.0, 80.0, 0.0)
        .expect("player");
    place_world_processor(
        &mut sim,
        10,
        10,
        "fetch player p @sharded 0 @air\nfetch playerCount n @sharded 0 @air\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(
        processor_obj(&sim, 10, 10, "p"),
        Some(LogicObject::Unit(player))
    );
    assert_eq!(processor_num(&sim, 10, 10, "n"), Some(1.0));
    assert_events_drained(&sim);
}

/// `printchar @copper` appends `(char)cont.emojiChar()` — the icons.properties
/// code for copper.
#[test]
fn printchar_content_emoji() {
    const ICONS: &str = include_str!("../../../assets/icons.properties");
    let expected = ICONS
        .lines()
        .find_map(|line| {
            let (code, rest) = line.split_once('=')?;
            let (name, _) = rest.split_once('|').unwrap_or((rest, ""));
            (name == "copper").then(|| code.trim().parse::<u32>().ok())?
        })
        .expect("copper has an icon code");
    let mut sim = campaign_sim();
    place_world_processor(&mut sim, 10, 10, "printchar @copper\nstop\n");
    tick(&mut sim, 3);
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("processor");
    let state = sim.ecs.0.get::<LogicBlockState>(e).expect("state");
    let got = state.executor.text_buffer.chars().next().map(|c| c as u32);
    assert_eq!(got, Some(expected), "the copper icon char is in the buffer");
    assert_events_drained(&sim);
}

// ---- I03 T3 remainder: family `logic_sense` overrides ----

/// Family `sense(LAccess)` overrides: crafter `@progress`, door `@enabled`,
/// conveyor `@progress`, core `@itemCapacity`, heat `@heat`, light `@color`
/// (`GenericCrafter.java:355`, `Door.java:88`, `Conveyor.java:436`,
/// `CoreBlock.java:607`, `HeatProducer.java:65`, `LightBlock.java:90`).
#[test]
fn family_sense_overrides() {
    use crate::entities::comp::{CrafterState, DoorState};
    use crate::world::blocks::distribution::ConveyorBuild;
    let mut sim = campaign_sim();
    let press = place(&mut sim, "graphite-press", 20, 20);
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<CrafterState>(press)
            .expect("crafter state");
        state.progress = 0.5;
    }
    let door = place(&mut sim, "door", 22, 20);
    {
        let mut state = sim.ecs.0.get_mut::<DoorState>(door).expect("door state");
        state.open = true;
    }
    let conveyor = place(&mut sim, "conveyor", 24, 20);
    let copper = item(&sim, "copper");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<ConveyorBuild>(conveyor)
            .expect("conveyor state");
        state.ids[0] = copper;
        state.ys[0] = 0.75;
        state.len = 1;
    }
    let core = place(&mut sim, "core-shard", 30, 30);
    let illuminator = place(&mut sim, "illuminator", 26, 20);
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::world::behavior::light::LightState>(illuminator)
            .expect("light state");
        state.color = 0x1122_33ff;
    }
    let heat = place(&mut sim, "heat-source", 28, 20);
    if let Some(mut state) = sim
        .ecs
        .0
        .get_mut::<crate::world::blocks::heat::HeatState>(heat)
    {
        state.heat = 42.0;
        // The producer lerps toward `heatOutput` every tick.
        state.heat_output = 42.0;
    }
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "sensor p press1 @progress\nsensor e door1 @enabled\nsensor c conveyor1 @progress\nsensor cap shard1 @itemCapacity\nsensor col light1 @color\nsensor h heat1 @heat\nstop\n",
        &[
            LogicLink::new("press1", 10, 10),
            LogicLink::new("door1", 12, 10),
            LogicLink::new("conveyor1", 14, 10),
            LogicLink::new("shard1", 20, 20),
            LogicLink::new("light1", 16, 10),
            LogicLink::new("heat1", 18, 10),
        ],
    );
    tick(&mut sim, 3);
    let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("processor");
    let num = |name: &str| {
        sim.ecs
            .0
            .get::<LogicBlockState>(processor)
            .and_then(|state| {
                state
                    .executor
                    .arena
                    .get_id(name)
                    .map(|id| state.executor.arena.get(id).num())
            })
    };
    assert_eq!(num("p"), Some(0.5), "crafter @progress");
    assert_eq!(num("e"), Some(1.0), "open door @enabled");
    // The belt keeps moving between the sensor and the readback, so assert the
    // recorded value is the live `ys` range (the base table returns NaN).
    let c = num("c").expect("conveyor progress sensed");
    assert!(
        (0.7..=1.0).contains(&c),
        "conveyor @progress reads the last item's ys: {c}"
    );
    assert!(
        num("cap").is_some_and(|cap| cap > 0.0),
        "core @itemCapacity"
    );
    // The heat producer's `updateTile` recomputes `heat` from its efficiency
    // (0 on this Serpulo fixture), so the override is exercised directly.
    let heat_behavior = sim
        .ecs
        .0
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| {
            sim.ecs
                .0
                .get::<crate::entities::comp::Building>(heat)
                .map(|building| building.block)
                .and_then(|block| table.get(block))
        })
        .map(|inst| inst.behavior.clone())
        .expect("heat behavior");
    let sensed = heat_behavior.logic_sense(&sim.ecs.0, heat, crate::logic::LAccess::Heat);
    let live_heat = sim
        .ecs
        .0
        .get::<crate::world::blocks::heat::HeatState>(heat)
        .map(|state| f64::from(state.heat))
        .unwrap_or(f64::NAN);
    assert_eq!(
        sensed,
        Some(crate::logic::access::Sensed::Num(live_heat)),
        "heat producer @heat reads HeatState.heat"
    );
    assert!(live_heat > 0.0, "the heat source is hot: {live_heat}");
    let _ = (core, num);
    assert_events_drained(&sim);
}

// ---- PAR-0218 audit: building `@config`/`@payloadCount`/`@payloadType`/
// `@controlled` (`BuildingComp.java:2100-2144`) ----

/// `@config` reads `config()` only for `configSenseable()` blocks; the unit
/// factory overrides it with the selected plan's unit
/// (`BuildingComp.java:2140`, `Block.java:822-825`, `UnitFactory.java:272-275`).
#[test]
fn building_config_sensor() {
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::world::config::{ConfigValue, configure};
    let mut sim = campaign_sim();
    let copper = item(&sim, "copper");
    let sorter = place(&mut sim, "sorter", 20, 20);
    let unloader = place(&mut sim, "unloader", 22, 20);
    let conveyor = place(&mut sim, "conveyor", 24, 20);
    let factory = place(&mut sim, "ground-factory", 30, 30);
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let copper_obj = Sensed::Obj(Some(LogicObject::Content(
        crate::content::ContentRef::item(copper),
    )));
    let config_of =
        |sim: &Sim, e: Entity| sense(&sim.ecs.0, &LogicObject::Building(e), LAccess::Config);
    // Unset configs read null.
    assert_eq!(config_of(&sim, sorter), Sensed::Obj(None), "unset sorter");
    assert!(configure(
        &mut sim.ecs.0,
        sorter,
        None,
        ConfigValue::Item(copper)
    ));
    assert!(configure(
        &mut sim.ecs.0,
        unloader,
        None,
        ConfigValue::Item(copper)
    ));
    assert_eq!(config_of(&sim, sorter), copper_obj, "sorter sortItem");
    assert_eq!(config_of(&sim, unloader), copper_obj, "unloader sortItem");
    // A conveyor has no content configuration: `configSenseable()` is false.
    assert_eq!(config_of(&sim, conveyor), Sensed::Obj(None), "conveyor");
    // The factory reads the plan's unit type, not the plan index.
    assert!(configure(
        &mut sim.ecs.0,
        factory,
        None,
        ConfigValue::Unit(dagger)
    ));
    assert_eq!(
        config_of(&sim, factory),
        Sensed::Obj(Some(LogicObject::Content(crate::content::ContentRef::new(
            crate::content::ContentType::Unit,
            dagger.raw(),
        )))),
        "factory plan unit"
    );
    // `configClear` (`UnitFactory.java:82`) clears only the command; the plan
    // is unset through `config(Integer.class)` with `-1` (`:58-67`).
    assert!(configure(&mut sim.ecs.0, factory, None, ConfigValue::None));
    assert_eq!(
        config_of(&sim, factory),
        Sensed::Obj(Some(LogicObject::Content(crate::content::ContentRef::new(
            crate::content::ContentType::Unit,
            dagger.raw(),
        )))),
        "configClear keeps the plan"
    );
    assert!(configure(
        &mut sim.ecs.0,
        factory,
        None,
        ConfigValue::Number(-1.0)
    ));
    assert_eq!(config_of(&sim, factory), Sensed::Obj(None), "no plan");
    // Through the VM: `sensor c sorter1 @config`.
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "sensor c sorter1 @config\nsensor n conveyor1 @config\nstop\n",
        &[
            LogicLink::new("sorter1", 10, 10),
            LogicLink::new("conveyor1", 14, 10),
        ],
    );
    tick(&mut sim, 3);
    assert_eq!(
        processor_obj(&sim, 10, 10, "c"),
        Some(LogicObject::Content(crate::content::ContentRef::item(
            copper
        ))),
        "sensor @config through the VM"
    );
    assert_eq!(processor_obj(&sim, 10, 10, "n"), None, "conveyor @config");
    assert_events_drained(&sim);
}

/// `@payloadCount` = `getPayloads().total() + (getPayload() != null)` and
/// `@payloadType` = the held payload's unit type or block
/// (`BuildingComp.java:2128,2141`).
#[test]
fn building_payload_sensors() {
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::world::behavior::PayloadRef;
    let mut sim = campaign_sim();
    let conveyor = place(&mut sim, "payload-conveyor", 20, 20);
    let wall = place(&mut sim, "copper-wall", 26, 20);
    let content = sim.block_runtime().expect("runtime").content();
    let dagger = content.unit_id("dagger").expect("dagger");
    let wall_block = content.block_id("copper-wall").expect("wall");
    let read = |sim: &Sim, e: Entity, a: LAccess| sense(&sim.ecs.0, &LogicObject::Building(e), a);
    assert_eq!(
        read(&sim, conveyor, LAccess::PayloadCount),
        Sensed::Num(0.0)
    );
    assert_eq!(
        read(&sim, conveyor, LAccess::PayloadType),
        Sensed::Obj(None)
    );
    let hold = |sim: &mut Sim, payload: PayloadRef| {
        if sim
            .ecs
            .0
            .get::<crate::world::blocks::payloads::PayloadHolder>(conveyor)
            .is_none()
        {
            sim.ecs
                .0
                .entity_mut(conveyor)
                .insert(crate::world::blocks::payloads::PayloadHolder::default());
        }
        sim.ecs
            .0
            .get_mut::<crate::world::blocks::payloads::PayloadHolder>(conveyor)
            .expect("holder")
            .payload = Some(payload);
    };
    hold(
        &mut sim,
        PayloadRef {
            entity: None,
            content: dagger.raw(),
            is_block: false,
        },
    );
    assert_eq!(
        read(&sim, conveyor, LAccess::PayloadCount),
        Sensed::Num(1.0)
    );
    assert_eq!(
        read(&sim, conveyor, LAccess::PayloadType),
        Sensed::Obj(Some(LogicObject::Content(crate::content::ContentRef::new(
            crate::content::ContentType::Unit,
            dagger.raw(),
        )))),
        "unit payload type"
    );
    hold(
        &mut sim,
        PayloadRef {
            entity: None,
            content: wall_block.raw(),
            is_block: true,
        },
    );
    assert_eq!(
        read(&sim, conveyor, LAccess::PayloadType),
        Sensed::Obj(Some(LogicObject::Content(
            crate::content::ContentRef::block(wall_block)
        ))),
        "build payload type"
    );
    // A plain block has neither `getPayload()` nor `getPayloads()`.
    assert_eq!(read(&sim, wall, LAccess::PayloadCount), Sensed::Num(0.0));
    assert_eq!(read(&sim, wall, LAccess::PayloadType), Sensed::Obj(None));
    // `UnitAssemblerBuild.getPayloads()` = `blocks`: the stored total.
    let assembler = place(&mut sim, "tank-assembler", 8, 26);
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::world::blocks::units::UnitAssembler>(assembler)
            .expect("assembler state");
        state.stored.add(
            crate::content::ContentRef::new(crate::content::ContentType::Unit, dagger.raw()),
            3,
        );
        state
            .stored
            .add(crate::content::ContentRef::block(wall_block), 2);
    }
    assert_eq!(
        read(&sim, assembler, LAccess::PayloadCount),
        Sensed::Num(5.0),
        "assembler payload total"
    );
}

/// `@controlled` on a building is `ctrlPlayer` while a player possesses the
/// turret's proxy unit, else 0 (`BuildingComp.java:2127`,
/// `ControlBlock.isControlled`).
#[test]
fn building_controlled_sensor() {
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    let mut sim = campaign_sim();
    let duo = place(&mut sim, "duo", 20, 20);
    let wall = place(&mut sim, "copper-wall", 24, 20);
    let read =
        |sim: &Sim, e: Entity| sense(&sim.ecs.0, &LogicObject::Building(e), LAccess::Controlled);
    assert_eq!(read(&sim, duo), Sensed::Num(0.0), "unpossessed turret");
    assert_eq!(read(&sim, wall), Sensed::Num(0.0), "not a ControlBlock");
    let proxy = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .and_then(|state| state.block_unit)
        .expect("turret proxy unit");
    sim.ecs
        .0
        .entity_mut(proxy)
        .insert(crate::entities::comp::Player);
    assert_eq!(
        read(&sim, duo),
        Sensed::Num(f64::from(crate::logic::globals::CTRL_PLAYER)),
        "possessed turret"
    );
}

/// Base `BuildingComp.sense` capacity rows: `itemCapacity`/`liquidCapacity`
/// are 0 without an item/liquid module, `powerCapacity` is the buffered
/// `consPower.capacity` (`BuildingComp.java:2116-2118`).
#[test]
fn building_capacity_sensors() {
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    let mut sim = campaign_sim();
    let node = place(&mut sim, "power-node", 20, 20);
    let battery = place(&mut sim, "battery", 22, 20);
    let conduit = place(&mut sim, "conduit", 24, 20);
    let read = |sim: &Sim, e: Entity, a: LAccess| sense(&sim.ecs.0, &LogicObject::Building(e), a);
    assert_eq!(read(&sim, node, LAccess::ItemCapacity), Sensed::Num(0.0));
    assert_eq!(read(&sim, node, LAccess::LiquidCapacity), Sensed::Num(0.0));
    assert_eq!(read(&sim, node, LAccess::PowerCapacity), Sensed::Num(0.0));
    // `Blocks.battery`: `consumePowerBuffered(4000f)`.
    assert_eq!(
        read(&sim, battery, LAccess::PowerCapacity),
        Sensed::Num(4000.0)
    );
    assert_eq!(read(&sim, conduit, LAccess::ItemCapacity), Sensed::Num(0.0));
    assert!(
        matches!(read(&sim, conduit, LAccess::LiquidCapacity), Sensed::Num(v) if v > 0.0),
        "conduit liquid capacity"
    );
    // Non-turret buildings have no `shooting` row: Java `NaN` -> null.
    assert_eq!(read(&sim, node, LAccess::Shooting), Sensed::Obj(None));
}

/// Unit item rows read the single `ItemsComp` stack and `type.itemCapacity`;
/// `@controller` of a non-logic unit is the unit itself
/// (`UnitComp.java` `sense`/`senseObject`: `totalItems`, `itemCapacity`,
/// `firstItem`, `controller`).
#[test]
fn unit_item_and_controller_sensors() {
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    let mut sim = campaign_sim();
    let copper = item(&sim, "copper");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let capacity = {
        let content = sim.block_runtime().expect("runtime").content();
        let id = content.unit_id("dagger").expect("dagger");
        content.unit(id).expect("def").item_capacity
    };
    let read = |sim: &Sim, a: LAccess| sense(&sim.ecs.0, &LogicObject::Unit(dagger), a);
    assert_eq!(read(&sim, LAccess::TotalItems), Sensed::Num(0.0));
    assert_eq!(read(&sim, LAccess::FirstItem), Sensed::Obj(None));
    sim.ecs
        .0
        .entity_mut(dagger)
        .insert(crate::entities::comp::unit::comp::ItemsComp {
            item: Some((copper, 7)),
        });
    assert_eq!(read(&sim, LAccess::TotalItems), Sensed::Num(7.0));
    assert_eq!(
        read(&sim, LAccess::FirstItem),
        Sensed::Obj(Some(LogicObject::Content(
            crate::content::ContentRef::item(copper)
        )))
    );
    assert!(capacity > 0, "dagger item capacity {capacity}");
    assert_eq!(
        read(&sim, LAccess::ItemCapacity),
        Sensed::Num(f64::from(capacity))
    );
    assert_eq!(
        read(&sim, LAccess::Controller),
        Sensed::Obj(Some(LogicObject::Unit(dagger))),
        "a non-logic unit is its own controller"
    );
}

/// `ubind` only binds `logicControllable` unit types (`LExecutor.java:188`):
/// a `manifold` (`logicControllable = false`) binds null.
#[test]
fn ubind_skips_non_logic_controllable_types() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    let manifold = sim
        .spawn_live_unit("manifold", 1, 60.0, 60.0, 0.0)
        .expect("manifold");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @manifold\nset m @unit\nubind @dagger\nset d @unit\nstop\n",
    );
    tick(&mut sim, 3);
    assert_eq!(processor_obj(&sim, 10, 10, "m"), None, "manifold not bound");
    assert_eq!(
        processor_obj(&sim, 10, 10, "d"),
        Some(LogicObject::Unit(dagger)),
        "dagger bound"
    );
    let _ = manifold;
    assert_events_drained(&sim);
}

/// `ucontrol within` is Arc `Position.within`: `dst2 < r * r`, so a target
/// exactly `r` away is outside (`LExecutor.java:379-381`).
#[test]
fn ucontrol_within_is_strict() {
    let mut sim = campaign_sim();
    place(&mut sim, "core-shard", 20, 20);
    sim.spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    // One processor batch: read the unit tile position, then test a point
    // exactly 2.5 tiles above it at radius 2.5 (outside) and 2.6 (inside).
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nsensor ux @unit @x\nsensor uy @unit @y\nop add ty uy 2.5\nucontrol within ux ty 2.5 edge\nucontrol within ux ty 2.6 inside\nstop\n",
    );
    tick(&mut sim, 5);
    assert_eq!(processor_num(&sim, 10, 10, "edge"), Some(0.0), "exactly r");
    assert_eq!(processor_num(&sim, 10, 10, "inside"), Some(1.0), "inside");
    assert_events_drained(&sim);
}

/// PAR-0458: `SenseI.run` dispatches on `target instanceof Senseable`
/// (`J/logic/LExecutor.java:671-700`) and `Team implements Senseable`
/// (`J/game/Team.java:17`), so a team value senses like any other object.
#[test]
fn sensor_reads_team_properties() {
    let mut sim = campaign_sim();
    place_world_processor(
        &mut sim,
        10,
        10,
        "set t @sharded\nsensor id t @id\nsensor col t @color\nsensor nm t @name\nstop\n",
    );
    tick(&mut sim, 20);
    assert_eq!(
        processor_num(&sim, 10, 10, "id"),
        Some(1.0),
        "Team.sense(id) is the team id"
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "nm"),
        Some(LogicObject::Str("sharded".to_owned())),
        "Team.senseObject(name) is the registry name"
    );
    // `Team.sense(color)` is `color.toDoubleBits()` (`J/game/Team.java:173`);
    // sharded's color is `Pal.accent` = `ffd37f` (`J/graphics/Pal.java:79`),
    // encoded like the building team-colour row (`J/entities/comp/BuildingComp.java:2104`).
    let color = crate::game::team::Team::get(1).color;
    let expected = crate::logic::globals::rgba_to_double_bits((
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
        255,
    ));
    assert_eq!(
        expected,
        f64::from_bits(0xffd3_7fff_u64),
        "sharded color bits from Team.java + Pal.accent"
    );
    assert_eq!(
        processor_num(&sim, 10, 10, "col"),
        Some(expected),
        "Team.sense(color) is the team color double bits"
    );
    assert_events_drained(&sim);
}
