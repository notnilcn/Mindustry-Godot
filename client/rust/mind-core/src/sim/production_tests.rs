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

pub(super) fn place(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity {
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

pub(super) fn item(sim: &Sim, name: &str) -> crate::content::ItemId {
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

/// J04 T2 (`ConsumeItemExplode.java:30-42`): explosive fuel damages a
/// combustion generator and fires `Trigger.blastGenerator` while
/// `rules.reactorExplosions` is on; both stop when the rule is off.
#[test]
fn explosive_fuel_damages_generator_and_fires_trigger() {
    use crate::world::blocks::power::behavior::GeneratorEnv;
    use crate::world::blocks::power::generator::{GeneratorConfig, ItemExplodeConfig};

    let mut sim = runtime_sim();
    let generator = place(&mut sim, "combustion-generator", 5, 5);
    // The vanilla chance is `delta * 0.06 * clamp(1.2 - 0.5)` (~4%/tick);
    // force the roll so this test is deterministic.
    sim.ecs
        .0
        .get_mut::<GeneratorConfig>(generator)
        .expect("generator config")
        .explode_items = Some(ItemExplodeConfig {
        base_chance: 600.0,
        ..ItemExplodeConfig::default()
    });
    let blast = item(&sim, "blast-compound");
    sim.ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(generator)
        .expect("item module")
        .add(blast, 20, i32::MAX);
    let health = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::entities::comp::Health>(generator)
            .map(|health| health.health)
            .unwrap_or(0.0)
    };
    let before = health(&sim);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert!(
        health(&sim) < before,
        "explosive fuel damaged the generator ({before} -> {})",
        health(&sim)
    );
    assert!(
        sim.triggers.fire_count(crate::sim::Trigger::BlastGenerator) > 0,
        "Trigger.blastGenerator fired"
    );

    // `Rules.reactorExplosions = false` -> the consumer is inert.
    sim.ecs.0.insert_resource(GeneratorEnv {
        reactor_explosions: false,
        ..GeneratorEnv::default()
    });
    let frozen = health(&sim);
    let fired = sim.triggers.fire_count(crate::sim::Trigger::BlastGenerator);
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    assert_eq!(health(&sim), frozen, "no damage with the rule off");
    assert_eq!(
        sim.triggers.fire_count(crate::sim::Trigger::BlastGenerator),
        fired,
        "no trigger with the rule off"
    );
}

/// J04 T3 (`GenericCrafter.dumpOutputs`/`liquidOutputDirections`,
/// `GenericCrafter.java:345-351`; electrolyzer `{1, 3}` at `Blocks.java:1397`):
/// a rotated electrolyzer dumps ozone only toward its relative-left side and
/// hydrogen only toward its relative-right side.
#[test]
fn electrolyzer_dumps_liquids_in_configured_directions() {
    let mut sim = runtime_sim();
    let electrolyzer = sim.world_block_id("electrolyzer").expect("electrolyzer");
    // Rotation 1: the relative output dirs become absolute -x (ozone) and +x
    // (hydrogen).
    sim.command(SimCommand::Place {
        x: 5,
        y: 5,
        block: electrolyzer,
        rotation: 1,
        team: 1,
        player: None,
    })
    .expect("placed");
    // The electrolyzer is 3x3 (anchor = center): conduits on all four sides,
    // each facing away (front tile empty), so dumped liquid stays put.
    let conduit = sim.world_block_id("conduit").expect("conduit");
    for (x, y, rotation) in [(3, 5, 2), (7, 5, 0), (5, 3, 3), (5, 7, 1)] {
        sim.command(SimCommand::Place {
            x,
            y,
            block: conduit,
            rotation,
            team: 1,
            player: None,
        })
        .expect("placed");
    }
    let crafter = sim.grid.entity_at(TilePos::new(5, 5)).expect("crafter");
    let left = sim.grid.entity_at(TilePos::new(3, 5)).expect("left");
    let right = sim.grid.entity_at(TilePos::new(7, 5)).expect("right");
    let above = sim.grid.entity_at(TilePos::new(5, 3)).expect("above");
    let below = sim.grid.entity_at(TilePos::new(5, 7)).expect("below");
    let ozone = liquid(&sim, "ozone");
    let hydrogen = liquid(&sim, "hydrogen");
    let seeds = [(ozone, 25.0), (hydrogen, 25.0)];
    for (liquid, amount) in seeds {
        sim.ecs
            .0
            .get_mut::<crate::world::modules::LiquidModule>(crafter)
            .expect("module")
            .add(liquid, amount, 500.0);
    }
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_liquid(&sim, left, ozone) > 0.0,
        "ozone dumps toward the relative-left side (dir 1 + rotation 1)"
    );
    assert!(
        stored_liquid(&sim, right, hydrogen) > 0.0,
        "hydrogen dumps toward the relative-right side (dir 3 + rotation 1)"
    );
    assert_eq!(stored_liquid(&sim, right, ozone), 0.0, "no ozone right");
    assert_eq!(stored_liquid(&sim, left, hydrogen), 0.0, "no hydrogen left");
    assert_eq!(
        stored_liquid(&sim, above, ozone) + stored_liquid(&sim, below, ozone),
        0.0,
        "no ozone above/below"
    );
    assert_eq!(
        stored_liquid(&sim, above, hydrogen) + stored_liquid(&sim, below, hydrogen),
        0.0,
        "no hydrogen above/below"
    );
}

/// J04 T6.2 (`Turret.java:348-353`): `TurretAmmo::Power` keeps only the bullet
/// id, so a power turret's `peekAmmo().rangeChange` used to read 0; the bullet
/// def's knobs are now snapshotted into the ammo at config time.
#[test]
fn power_turret_reads_bullet_range_change() {
    use crate::world::blocks::defense::turrets::{TurretAmmo, TurretState, turret_range_of};

    // The snapshot happens when the block runtime builds the turret configs,
    // so mutate the bullet def before installing the runtime.
    let mut content = super::runtime::load_vanilla_content().expect("content");
    let bullet = content.bullet_names()["lancer_laser"];
    content.bullet_mut(bullet).expect("bullet").range_change = 24.0;
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime_from(crate::sim::runtime::SimRuntimeConfig::default(), content)
        .expect("runtime");
    let turret = place(&mut sim, "lancer", 5, 5);
    let (config_range, ammo_change) = {
        let state = sim.ecs.0.get::<TurretState>(turret).expect("state");
        let TurretAmmo::Power(ammo) = &state.config.ammo else {
            panic!("lancer is a power turret");
        };
        (state.config.range, ammo.range_change)
    };
    assert!(
        (ammo_change - 24.0).abs() < 0.001,
        "the power ammo snapshots the bullet's rangeChange ({ammo_change})"
    );
    let range = sim
        .ecs
        .0
        .get::<TurretState>(turret)
        .map(|state| turret_range_of(state, &sim.ecs.0, turret))
        .expect("state");
    assert!(
        (range - (config_range + 24.0)).abs() < 0.001,
        "turret range {range} vs {}",
        config_range + 24.0
    );
}

/// J04 T6.1 (PAR-0349, `Door.java:49-53`): a door refuses to close while a
/// unit stands in its footprint (`Units.anyEntities(tile)`); it closes once
/// the unit leaves.
#[test]
fn door_refuses_to_close_with_a_unit_on_it() {
    use crate::entities::comp::DoorState;
    use crate::world::behavior::defense::set_open;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let door = place(&mut sim, "door", 5, 5);
    let unit = sim
        .ecs
        .0
        .spawn((
            crate::entities::comp::Unit,
            crate::entities::comp::Pos {
                x: 5.0 * 8.0 + 4.0,
                y: 5.0 * 8.0 + 4.0,
            },
            crate::entities::comp::TeamComp { team: 1 },
            crate::entities::comp::unit::comp::HitboxComp { hit_size: 8.0 },
        ))
        .id();
    // Open, then try to close: the unit keeps it open.
    set_open(&mut sim.ecs.0, door, true);
    set_open(&mut sim.ecs.0, door, false);
    assert!(
        sim.ecs
            .0
            .get::<DoorState>(door)
            .is_some_and(|state| state.open),
        "a door with a unit in its footprint stays open"
    );
    // Move the unit off the tile: the door closes.
    if let Some(mut pos) = sim.ecs.0.get_mut::<crate::entities::comp::Pos>(unit) {
        pos.x = 20.0 * 8.0 + 4.0;
    }
    set_open(&mut sim.ecs.0, door, false);
    assert!(
        !sim.ecs
            .0
            .get::<DoorState>(door)
            .is_some_and(|state| state.open),
        "the door closes once the unit leaves"
    );
}

/// J04 T4 (`PowerGenerator.onExplosion`, `PowerGenerator.java:149-201`): the
/// sim half of a generator explosion - radius damage, ignition fires, prop
/// breaking, fireballs and liquid puddles (the view half stays in
/// `ExplosionSink`).
#[test]
fn generator_explosion_applies_sim_side_effects() {
    use crate::world::blocks::power::explosion::{ReactorExplosion, fire_explosion};
    use crate::world::blocks::power::generator::GeneratorConfig;

    let mut sim = runtime_sim();
    // The fireball spawn (`Bullets.fireball.createNet`) needs the unit runtime.
    sim.install_unit_runtime().expect("unit runtime");
    let generator = place(&mut sim, "combustion-generator", 5, 5);
    let neoplasm = liquid(&sim, "neoplasm");
    // A neoplasia-style footprint with every sim effect active.
    {
        let mut config = sim
            .ecs
            .0
            .get_mut::<GeneratorConfig>(generator)
            .expect("generator config");
        config.explosion_radius = 3.0;
        config.explosion_damage = 120.0;
        config.explosion_puddles = 6;
        config.explosion_puddle_range = 16.0;
        config.explosion_puddle_amount = 50.0;
        config.explosion_puddle_liquid = Some(neoplasm);
        config.explosion_ignition_chance = 1.0;
        config.explosion_fireballs = 3;
    }
    // A `unitMoveBreakable` prop in range (`boulder` is a `StaticProp`).
    let boulder = BlockId::new(sim.world_block_id("boulder").expect("boulder"));
    sim.grid.tiles.get_mut(6, 6).block = boulder;
    // A nearby building takes falloff damage.
    let wall = place(&mut sim, "conveyor", 7, 5);
    let wall_health = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::entities::comp::Health>(wall)
            .map(|health| health.health)
            .unwrap_or(0.0)
    };
    let before = wall_health(&sim);

    let config = sim
        .ecs
        .0
        .get::<GeneratorConfig>(generator)
        .cloned()
        .expect("config");
    let explosion =
        ReactorExplosion::for_building(&sim.ecs.0, generator, &config, true).expect("params");
    assert!(fire_explosion(&mut sim.ecs.0, generator, explosion, true));
    // The sim half drains on the following ticks (fires are delayed by
    // `dst / explosionSpeed`).
    for _ in 0..20 {
        sim.tick().expect("tick");
    }

    assert!(
        wall_health(&sim) < before,
        "radius damage hit the wall ({before} -> {})",
        wall_health(&sim)
    );
    assert!(
        sim.grid.tiles.get(6, 6).block != boulder,
        "the explosion broke the unitMoveBreakable prop"
    );
    let mut puddle_query = sim.ecs.0.query::<&crate::combat::puddles::PuddleState>();
    let puddles = puddle_query.iter(&sim.ecs.0).count();
    assert!(puddles > 0, "the explosion left puddles");
    let mut fire_query = sim.ecs.0.query::<&crate::combat::fires::FireState>();
    let fires = fire_query.iter(&sim.ecs.0).count();
    assert!(fires > 0, "the explosion ignited fires");
    let mut bullet_query = sim.ecs.0.query::<&crate::combat::bullet::Bullet>();
    let fireballs = bullet_query
        .iter(&sim.ecs.0)
        .filter(|bullet| bullet.def == crate::content::registries::bullets::FIREBALL)
        .count();
    assert!(fireballs > 0, "the explosion spawned fireballs");
}

/// J04 T2 (`ConsumeGenerator.java:126-131`, `Blocks.java:2538`):
/// `itemDurationMultipliers` scale the burn time - pyratite burns 3x longer
/// than coal in a combustion generator.
#[test]
fn item_duration_multipliers_scale_burn_time() {
    use crate::world::blocks::power::generator::GeneratorConfig;

    let burn_ticks = |name: &str| -> i64 {
        let mut sim = runtime_sim();
        let generator = place(&mut sim, "combustion-generator", 5, 5);
        // Isolate the duration multiplier from the T2 explosive consumer.
        sim.ecs
            .0
            .get_mut::<GeneratorConfig>(generator)
            .expect("generator config")
            .explode_items = None;
        let fuel = item(&sim, name);
        sim.ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(generator)
            .expect("item module")
            .add(fuel, 2, i32::MAX);
        let mut first: Option<i64> = None;
        for tick in 1..=1500i64 {
            sim.tick().expect("tick");
            let left = stored_item(&sim, generator, fuel);
            if first.is_none() && left == 1 {
                first = Some(tick);
            } else if let (Some(first), 0) = (first, left) {
                return tick - first;
            }
        }
        panic!("`{name}` was not fully consumed");
    };

    let coal = burn_ticks("coal");
    let pyratite = burn_ticks("pyratite");
    assert!(
        (coal - 120).abs() <= 2,
        "coal burns for {coal} ticks (itemDuration 120)"
    );
    assert!(
        (pyratite - 360).abs() <= 2,
        "pyratite burns for {pyratite} ticks (120 x 3)"
    );
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

pub(super) fn runtime_sim() -> Sim {
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

pub(super) fn configure(sim: &mut Sim, x: i16, y: i16, value: crate::determinism::ConfigValue) {
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

/// PAR-0092 / J05 T2: `UnitFactory.config(UnitType.class)` selects the matching
/// plan (resetting progress) and `config()` reports the plan's unit
/// (`UnitFactory.java:375-378`, `build.currentPlan == -1 ? null :
/// plans.get(currentPlan).unit`). `config(Integer.class)` with `-1` clears the
/// plan; `configure(null)` (`configClear`, `:82`) only clears the *command*.
/// With no plan selected nothing is produced even with the items in storage.
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
    // the fallback default. `config()` reports that plan's unit type.
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::Unit(content_flare)
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
        crate::world::config::ConfigValue::Unit(mono)
    );

    // `configure(null)` / `configClear` (`UnitFactory.java:82`) clears only the
    // command; the plan stays selected. `config(Integer.class)` with `-1`
    // (`:58-67`) is the way to unset the plan.
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::None,
    })
    .expect("clear command");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::Unit(mono),
        "configClear keeps the plan"
    );
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Int(-1),
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

/// H07 T2 (`UnitFactory.java:440`): producing a unit plays `unitCreate` once
/// at the factory centre, volume 1, `pitch = 1 + range(0.06)`.
#[test]
fn ground_factory_plays_unit_create() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let factory = place(&mut sim, "ground-factory", 5, 5);
    let (silicon, lead) = (item(&sim, "silicon"), item(&sim, "lead"));
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(factory) {
            items.add(silicon, 10, 60);
            items.add(lead, 10, 60);
        }
    }
    let _source = place(&mut sim, "power-source", 3, 5);
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, dagger) {
            break;
        }
    }
    assert!(spawned_unit(&sim, dagger), "factory produced a dagger");
    let events = log.events();
    let creates: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::At { sound, .. } if *sound == SoundId::UNIT_CREATE => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(creates.len(), 1, "exactly one unitCreate: {events:?}");
    let AudioEvent::At {
        x,
        y,
        pitch,
        volume,
        ..
    } = creates[0]
    else {
        unreachable!()
    };
    // Size-3 factory at (5,5): centre tile 5*8, offset 0.
    assert_eq!((*x, *y), (40.0, 40.0), "factory centre");
    assert_eq!(*volume, 1.0);
    assert!((0.94..=1.06).contains(pitch), "unitCreate pitch {pitch}");
}

/// H07 T2 (`MendProjector.java:120`): a mender pulse that heals at least one
/// damaged building plays `healWave` (volume 0.5, `pitch = 1 + range(0.1)`)
/// at the mender centre; a pulse with nothing to heal is silent.
#[test]
fn mender_pulse_plays_heal_wave() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let _mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let wall = place(&mut sim, "copper-wall", 6, 5);
    if let Some(mut health) = sim.ecs.0.get_mut::<crate::entities::comp::Health>(wall) {
        health.health = 10.0;
    }
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let events = log.events();
    let heals: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::At { sound, .. } if *sound == SoundId::HEAL_WAVE => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(heals.len(), 1, "one healWave in 300 ticks: {events:?}");
    let AudioEvent::At {
        x,
        y,
        pitch,
        volume,
        ..
    } = heals[0]
    else {
        unreachable!()
    };
    assert_eq!((*x, *y), (40.0, 40.0), "mender centre");
    assert_eq!(*volume, 0.5);
    assert!((0.9..=1.1).contains(pitch), "healWave pitch {pitch}");

    // Same fixture with nothing damaged -> no sound.
    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let _mender = place(&mut sim, "mender", 5, 5);
    let _power = place(&mut sim, "power-source", 5, 6);
    let _wall = place(&mut sim, "copper-wall", 6, 5);
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let heals = log.events().iter().filter(|ticked| {
        matches!(&ticked.event, AudioEvent::At { sound, .. } if *sound == SoundId::HEAL_WAVE)
    }).count();
    assert_eq!(heals, 0, "undamaged targets -> silent");
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
        let _source = place(&mut sim, "power-source", 4, 5);
        let pump = place(&mut sim, "water-extractor", 5, 5);
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
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

/// T13 (`Door.tapped`, `Door.java:148-154`): `Door`'s **config body** applies
/// immediately (`Door.java:38-58` has no throttle); the 60-tick `timerToggle`
/// gate sits on the tap path only (`door_tap_allowed`; the timer semantics are
/// covered by `behavior::defense::tests::door_tap_timer_gates_and_records`).
#[test]
fn door_configs_apply_without_tap_throttle() {
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
    assert!(is_open(&sim), "opened on the first config");
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
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
    let _door = place(&mut sim, "door", 5, 5);
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
    let unloader = place(&mut sim, "unloader", 7, 5);
    let _ = unloader;
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
    let _drill = place(&mut sim, "mechanical-drill", 5, 5);
    let _container = place(&mut sim, "container", 7, 5);
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
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

/// PAR-0073/T3: `ConstructBlock.constructFinish` -> `block.placeSound.at(tile)`
/// (`ConstructBlock.java:71-120`); size 1 wall centre 5*8 (offset 0).
#[test]
fn placing_emits_construct_place_sound() {
    use crate::audio::{AudioEvent, SharedAudioLog};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    place(&mut sim, "copper-wall", 5, 5);
    sim.tick().expect("tick");
    let events = log.events();
    let placed: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::Construct { place: true, .. } => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), 1, "one place sound: {events:?}");
    let AudioEvent::Construct {
        sound,
        x,
        y,
        place,
        pitch_change,
    } = placed[0]
    else {
        unreachable!()
    };
    assert_eq!(*sound, crate::audio::SoundId::BLOCK_PLACE1);
    assert_eq!((*x, *y), (40.0, 40.0));
    assert!(*place && *pitch_change);
}

/// PAR-0073/T3: `ConstructBlock.deconstructFinish` -> `breakSound`, not the
/// destroy path (`ConstructBlock.java:58-69`).
#[test]
fn breaking_emits_break_not_destroy() {
    use crate::audio::{AudioEvent, SharedAudioLog};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    place(&mut sim, "copper-wall", 5, 5);
    sim.tick().expect("tick");
    log.clear();
    sim.command(SimCommand::Break {
        x: 5,
        y: 5,
        player: None,
    })
    .expect("break");
    sim.tick().expect("tick");
    let events = log.events();
    let constructs: Vec<&AudioEvent> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::Construct { place: false, .. } => Some(&ticked.event),
            _ => None,
        })
        .collect();
    assert_eq!(constructs.len(), 1, "one break sound: {events:?}");
    let AudioEvent::Construct { sound, .. } = constructs[0] else {
        unreachable!()
    };
    assert_eq!(*sound, crate::audio::SoundId::BLOCK_BREAK1);
    assert!(
        !events
            .iter()
            .any(|ticked| matches!(ticked.event, AudioEvent::At { .. })),
        "a deconstruct must not play destroy sounds: {events:?}"
    );
}

/// PAR-0073/T3: size-2 walls use `blockBreak2`/`blockPlace2` and the block
/// offset centre (`Block.java:1367-1379`).
#[test]
fn large_wall_place_sound_is_size_2() {
    use crate::audio::{AudioEvent, SharedAudioLog};

    let mut sim = runtime_sim();
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let offset = {
        let content = sim.block_runtime().expect("runtime").content();
        content
            .block_id("copper-wall-large")
            .and_then(|id| content.block(id))
            .expect("copper-wall-large")
            .offset
    };
    place(&mut sim, "copper-wall-large", 8, 8);
    sim.tick().expect("tick");
    let events = log.events();
    let placed = events
        .iter()
        .find_map(|ticked| match &ticked.event {
            AudioEvent::Construct { place: true, .. } => Some(&ticked.event),
            _ => None,
        })
        .expect("place sound");
    let AudioEvent::Construct { sound, x, y, .. } = placed else {
        unreachable!()
    };
    assert_eq!(*sound, crate::audio::SoundId::BLOCK_PLACE2);
    assert_eq!((*x, *y), (64.0 + offset, 64.0 + offset));
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

// ---- G06 T4 (PAR-0104): live world-processor bridge ----

/// Installs a campaign session (the live rules/markers/wave source) on `sim`.
pub(super) fn install_campaign_session(sim: &mut Sim, rules: crate::game::rules::Rules) {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    let content = crate::world::harness::BuildHarness::load_content();
    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
}

/// Live-client logic resources (what `gdext` `install_sim_seams` inserts).
pub(super) fn install_logic_world(sim: &mut Sim) {
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState {
            headless: false,
            ..crate::logic::world::LogicWorldState::new()
        });
    sim.ecs
        .0
        .insert_resource(crate::logic::world::MessageState::default());
    let content = crate::world::harness::BuildHarness::load_content();
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicContentIndex::from_content(
            &content,
        ));
}

/// Places a configurable world processor running `code`.
pub(super) fn place_world_processor(sim: &mut Sim, x: i16, y: i16, code: &str) {
    let id = sim
        .world_block_id("world-processor")
        .expect("world-processor");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let bytes = crate::logic::blocks::logic_block::compress(code, &[]);
    configure(sim, x, y, crate::determinism::ConfigValue::Bytes(bytes));
}

pub(super) fn test_rules() -> crate::game::rules::Rules {
    crate::game::rules::Rules {
        default_team: 1,
        allow_edit_world_processors: true,
        ..Default::default()
    }
}

/// `setrule` writes the live campaign session rules (not a dead copy).
#[test]
fn setrule_writes_live_rules() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule waveSending true 0 0 100 100\nsetrule unitCap 4 0 0 100 100\nstop\n",
    );
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(runtime.session.rules.wave_sending);
    assert_eq!(runtime.session.rules.unit_cap, 4);
}

/// Ground Zero block 2: the tutorial flag is consumed and waves enabled.
#[test]
fn flags_roundtrip_live() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .rules
        .objective_flags
        .insert("allowWaves".to_owned());
    place_world_processor(
        &mut sim,
        10,
        10,
        "getflag waves \"allowWaves\"\njump 0 equal waves false\nsetflag \"allowWaves\" false\nsetrule waveSending true 0 0 100 100\nsetrule waveTimer true 0 0 100 100\nstop\n",
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(!runtime.session.rules.objective_flags.contains("allowWaves"));
    assert!(runtime.session.rules.wave_sending);
    assert!(runtime.session.rules.wave_timer);
}

/// `setrule mapArea` updates the session and, through G04 T5, `grid.limit_area`.
#[test]
fn map_area_rule_toggles_live_limit() {
    let mut sim = Sim::new(3, 300, 300, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    install_campaign_session(&mut sim, test_rules());
    // Java `SetRuleI`: the `value` slot is unused for `mapArea`; x/y/w/h come
    // from p1..p4 (`setrule mapArea 10 0 0 256 256` -> x=0, w=256).
    place_world_processor(&mut sim, 10, 10, "setrule mapArea 10 0 0 256 256\nstop\n");
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    {
        let runtime = sim
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .expect("runtime");
        assert!(runtime.session.rules.limit_map_area);
        assert_eq!(runtime.session.rules.limit_width, 256);
    }
    assert_eq!(sim.grid.limit_area, Some((0, 0, 256, 256)));
    // Selecting the whole map disables the limit again.
    let bytes =
        crate::logic::blocks::logic_block::compress("setrule mapArea 0 0 0 300 300\nstop\n", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(!runtime.session.rules.limit_map_area);
    assert_eq!(sim.grid.limit_area, None);
}

/// `setrule wave` clamps to >= 1; `currentWaveTime` converts seconds to ticks.
#[test]
fn wave_rule_sets_wave_and_wavetime() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule wave 0 0 0 100 100\nsetrule currentWaveTime 2.5 0 0 100 100\nstop\n",
    );
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(runtime.session.wave, 1);
    assert_eq!(runtime.session.wavetime, 150.0);
}

/// The event log never grows: `Sim::tick` drains it every tick.
#[test]
fn logic_events_drain_each_tick() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState::new());
    place_world_processor(&mut sim, 10, 10, "setflag \"drained\" true\nstop\n");
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(runtime.session.rules.objective_flags.contains("drained"));
    let state = sim
        .ecs
        .0
        .get_resource::<crate::logic::world::LogicWorldState>()
        .expect("state");
    assert!(state.events.is_empty(), "{:?}", state.events);
}

/// H06 T1: `state_snapshot` lists the Java variables view of a live
/// processor (numbers, no constants, no `@`-names).
#[test]
fn logic_state_snapshot_lists_vars() {
    let mut sim = runtime_sim();
    place(&mut sim, "micro-processor", 10, 10);
    let bytes = crate::logic::blocks::logic_block::compress("set a 1\nop add b a 2", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let snapshot =
        crate::logic::blocks::logic_block::state_snapshot(&sim.ecs.0, e).expect("snapshot");
    assert!(
        snapshot
            .vars
            .iter()
            .any(|(name, value, type_)| name == "a" && value == "1" && *type_ == "number"),
        "{:?}",
        snapshot.vars
    );
    assert!(
        snapshot
            .vars
            .iter()
            .any(|(name, value, type_)| name == "b" && value == "3" && *type_ == "number"),
        "{:?}",
        snapshot.vars
    );
    assert!(
        !snapshot
            .vars
            .iter()
            .any(|(name, _, _)| name.starts_with('@')),
        "{:?}",
        snapshot.vars
    );
    assert_eq!(snapshot.ipt, 2);
    assert!(!snapshot.privileged);
    assert!(snapshot.accessible);
}

/// H06 T20: a processor decoded before `GlobalVars` exists assembles
/// `@sharded`/`@duo` as null variables; re-assembling after the globals
/// arrive resolves them (the fixed engine load order: `Vars.logicVars`
/// exists before any map/save is read, `Vars.java:369`).
#[test]
fn processor_loaded_before_globals_resolves_constants() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::logic::globals::GlobalVars>()
            .is_none(),
        "runtime_sim carries no GlobalVars"
    );
    place_world_processor(
        &mut sim,
        10,
        10,
        "fetch buildCount n @sharded 0 @duo\nstop\n",
    );
    {
        let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
        let state = sim
            .ecs
            .0
            .get::<crate::logic::blocks::LogicBlockState>(e)
            .expect("state");
        let sharded = state
            .executor
            .optional_var("@sharded")
            .expect("@sharded assembled");
        assert!(
            !state.executor.arena.get(sharded).constant,
            "without globals the content constant is a plain variable"
        );
    }
    // Fixed load order: content globals exist before the buildings decode.
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
    crate::logic::blocks::logic_block::reassemble_all_processors(&mut sim.ecs.0);
    place(&mut sim, "duo", 20, 20);
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let state = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(e)
        .expect("state");
    let mut bad = Vec::new();
    for id in &state.executor.var_ids {
        let cell = state.executor.arena.get(*id);
        if cell.name.starts_with('@') && !cell.constant && cell.name != "@counter" {
            bad.push(cell.name.clone());
        }
    }
    assert!(bad.is_empty(), "unresolved constants: {bad:?}");
    let n = state
        .executor
        .optional_var("n")
        .map(|id| state.executor.arena.get(id).num());
    assert_eq!(n, Some(1.0), "buildCount sees the team-1 duo");
}

/// H06 T2: the world processor's `spawnwave natural` runs one live wave
/// (`SpawnWaveI` -> `Logic.skipWave` -> `runWave`,
/// `J/logic/LExecutor.java:2173-2175`, `Logic.java:315-325`).
#[test]
fn spawnwave_natural_runs_wave() {
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = test_rules();
    rules.spawns.push(crate::io::json::SpawnGroup {
        type_: "dagger".to_owned(),
        begin: 0,
        unit_amount: 1,
        ..Default::default()
    });
    let wave_spacing = rules.wave_spacing;
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    let spawn = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("spawn")
        .expect("spawn");
    let index = sim.grid.tiles.index(6, 6);
    sim.grid.tiles.geti_mut(index).overlay = spawn;
    place_world_processor(&mut sim, 10, 10, "spawnwave natural 10 true\nstop\n");
    let before = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .wave;
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(runtime.session.wave, before + 1, "one wave advanced");
    assert_eq!(
        runtime.session.wavetime, wave_spacing,
        "countdown reset to waveSpacing (non-campaign multiplier 1)"
    );
    // The queued `spawnEnemies` runs in the next `run_wave_system`.
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let count = sim
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .filter(|e| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::UnitTypeComp>(**e)
                .map(|c| c.type_id)
                == Some(dagger)
                && sim
                    .ecs
                    .0
                    .get::<crate::entities::comp::TeamComp>(**e)
                    .map(|t| t.team)
                    == Some(2)
        })
        .count();
    assert_eq!(count, 1, "one crux dagger spawned");
    assert!(
        sim.logic_event_log.iter().any(|kind| kind == "spawnwave"),
        "{:?}",
        sim.logic_event_log
    );
}

/// H06 T2: Ground Zero GZ-1 verbatim — the flag is consumed, one wave runs,
/// and later ticks do not run more waves (the program falls off its end).
#[test]
fn gz1_spawn_wave_flag() {
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = test_rules();
    rules.spawns.push(crate::io::json::SpawnGroup {
        type_: "dagger".to_owned(),
        begin: 0,
        unit_amount: 1,
        ..Default::default()
    });
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    let spawn = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("spawn")
        .expect("spawn");
    let index = sim.grid.tiles.index(6, 6);
    sim.grid.tiles.geti_mut(index).overlay = spawn;
    sim.ecs
        .0
        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .rules
        .objective_flags
        .insert("spawnWave".to_owned());
    place_world_processor(
        &mut sim,
        10,
        10,
        "getflag wave \"spawnWave\"\njump 0 equal wave false\nsetflag \"spawnWave\" false\nspawnwave natural 10 true\n",
    );
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(
        !runtime.session.rules.objective_flags.contains("spawnWave"),
        "GZ-1 consumed the flag"
    );
    assert_eq!(runtime.session.wave, 2, "wave 1 -> 2");
    for _ in 0..100 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(runtime.session.wave, 2, "no further wave without the flag");
}

/// H06 T2: positional `spawnwave` spawns at the given point without
/// advancing the wave (`J/logic/LExecutor.java:2177-2198`).
#[test]
fn spawnwave_positional_no_wave_advance() {
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = test_rules();
    rules.spawns.push(crate::io::json::SpawnGroup {
        type_: "dagger".to_owned(),
        begin: 0,
        unit_amount: 1,
        spawn: -1,
        ..Default::default()
    });
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    place_world_processor(&mut sim, 10, 10, "spawnwave 20 20 false\nstop\n");
    let before = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .wave;
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(
        runtime.session.wave, before,
        "positional spawn keeps the wave"
    );
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let position = sim
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .find(|e| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::UnitTypeComp>(**e)
                .map(|c| c.type_id)
                == Some(dagger)
        })
        .and_then(|e| sim.ecs.0.get::<crate::entities::comp::Pos>(*e).copied());
    let position = position.expect("positional dagger spawned");
    let distance = ((position.x - 160.0).powi(2) + (position.y - 160.0).powi(2)).sqrt();
    assert!(
        distance <= 16.5,
        "dagger within 16 px of (160,160), got {distance}"
    );
}

/// H06 T3: `cutscene pan`/`cutscene active` set the InputHandler state
/// (`J/logic/LExecutor.java:1350-1369`).
#[test]
fn cutscene_pan_sets_state() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    install_logic_world(&mut sim);
    place_world_processor(
        &mut sim,
        10,
        10,
        "cutscene pan 10 20 0.05 0\ncutscene active a 0 0 0\nstop\n",
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let cutscene = sim.logic_cutscene();
    assert!(cutscene.active, "pan activates the cutscene");
    assert_eq!(cutscene.pan, (80.0, 160.0), "World.unconv applied");
    assert!((cutscene.speed - 0.05).abs() < 1e-6);
    assert_eq!(cutscene.zoom, -1.0, "pan-only keeps zoom -1");
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let a = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(e)
        .and_then(|state| {
            state
                .executor
                .optional_var("a")
                .map(|id| state.executor.arena.get(id).num())
        });
    assert_eq!(a, Some(1.0), "`cutscene active` wrote p1");
}

/// H06 T3: Ground Zero GZ-4 verbatim — the pan activates, the map area is
/// expanded and the `wait 11` + `cutscene stop` end the cutscene.
#[test]
fn gz4_cutscene_runs_and_stops() {
    let mut sim = Sim::new(3, 300, 300, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    install_campaign_session(&mut sim, test_rules());
    install_logic_world(&mut sim);
    {
        let mut runtime = sim
            .ecs
            .0
            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
            .expect("runtime");
        runtime
            .session
            .rules
            .objective_flags
            .insert("mineOver".to_owned());
        runtime
            .session
            .rules
            .objective_flags
            .insert("spawnOver".to_owned());
    }
    place_world_processor(
        &mut sim,
        10,
        10,
        "getflag mineOver \"mineOver\"\njump 0 equal mineOver false\nsetflag \"mineOver\" false\nsetrule mapArea 10 0 0 256 256\ngetflag end \"spawnOver\"\njump 4 equal end false\ncutscene pan 135 230 0.03 0\nwait 11\ncutscene stop 100 100 0.06 0\nstop\n",
    );
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let cutscene = sim.logic_cutscene();
    assert!(cutscene.active, "GZ-4 pan active after the wait starts");
    assert_eq!(cutscene.pan, (1080.0, 1840.0));
    {
        let runtime = sim
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .expect("runtime");
        assert!(runtime.session.rules.limit_map_area, "map area set");
        assert_eq!(runtime.session.rules.limit_width, 256);
        assert_eq!(runtime.session.rules.limit_height, 256);
    }
    // `wait 11` (seconds) then `cutscene stop`.
    for _ in 0..11 * 60 + 10 {
        sim.tick().expect("tick");
    }
    assert!(!sim.logic_cutscene().active, "stop ends the cutscene");
}

/// H06 T3: headless (`CutsceneI.run`: `if(headless) return;`) leaves no
/// presentation state.
#[test]
fn cutscene_headless_noop() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState::new());
    place_world_processor(
        &mut sim,
        10,
        10,
        "cutscene pan 10 20 0.05 0\ncutscene active a 0 0 0\nstop\n",
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        sim.logic_cutscene(),
        crate::logic::world::LogicCutscene::default()
    );
}

/// H06 T5: an unchanged `setrule` does not request a world re-derive
/// (frozenForest/fungalPass re-set `waveTimer` every tick).
#[test]
fn setrule_unchanged_is_not_dirty() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let mut rules = test_rules();
    rules.wave_timer = true;
    rules.waves = true;
    install_campaign_session(&mut sim, rules);
    // Sabotage the derived world copy: if the setrule re-derived the world,
    // `sync_build_rules` would copy `rules.waves = true` back.
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("build rules")
        .waves = false;
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule waveTimer true 0 0 0 0\nsetrule waves true 0 0 0 0\nstop\n",
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(runtime.session.rules.wave_timer);
    assert!(runtime.session.rules.waves);
    assert!(
        !runtime.rules_world_dirty,
        "no change -> no re-derive request"
    );
    assert!(
        !sim.ecs
            .0
            .get_resource::<BuildRules>()
            .expect("build rules")
            .waves,
        "the world copy was not re-synced"
    );
}

/// H06 T5: a real change marks the session dirty so the world re-derive runs
/// and `sync_build_rules` reaches `BuildRules`.
#[test]
fn setrule_waves_resyncs_build_rules() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules()); // waves = false
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("build rules")
        .waves = false;
    place_world_processor(&mut sim, 10, 10, "setrule waves true 0 0 0 0\nstop\n");
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert!(runtime.session.rules.waves);
    assert!(
        sim.ecs
            .0
            .get_resource::<BuildRules>()
            .expect("build rules")
            .waves,
        "the changed rule drove the world re-sync"
    );
}

/// H06 T5: per-team `setrule` marks the session dirty (the world re-derive
/// runs; `grid.border_darkness` is one of its writes).
#[test]
fn per_team_rule_changes_mark_dirty() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    // `@sharded` resolves through the content globals.
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
    // Out-of-band world copy of a re-derived field.
    sim.grid.border_darkness = false;
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule buildSpeed 2 @sharded 0 0 0\nstop\n",
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime");
    assert_eq!(
        runtime.session.rules.team_rule(1).build_speed_multiplier,
        2.0
    );
    assert!(
        sim.grid.border_darkness,
        "the world re-derive ran (dirty was set on the per-team change)"
    );
}

/// H06 T5: `mapArea` re-enables a building disabled outside the previous area
/// (Java `World.checkMapArea`, World.java:469-481).
#[test]
fn map_area_reenables_outside_buildings() {
    use crate::entities::comp::Building;

    let mut sim = Sim::new(3, 300, 300, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let mut rules = test_rules();
    rules.limit_map_area = true;
    rules.limit_x = 0;
    rules.limit_y = 0;
    rules.limit_width = 20;
    rules.limit_height = 20;
    rules.disable_outside_area = true;
    install_campaign_session(&mut sim, rules.clone());
    // Place before the limit is applied to the grid (placement outside the
    // rect is refused once `grid.limit_area` is set).
    let conveyor = place(&mut sim, "conveyor", 25, 25);
    sim.apply_rules_to_world(&rules);
    assert!(
        !sim.ecs
            .0
            .get::<Building>(conveyor)
            .expect("building")
            .enabled,
        "disabled outside the 20x20 area"
    );
    place_world_processor(&mut sim, 10, 10, "setrule mapArea 10 0 0 50 50\nstop\n");
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs
            .0
            .get::<Building>(conveyor)
            .expect("building")
            .enabled,
        "re-enabled by checkMapArea(prev)"
    );
}

/// `message announce 2 @wait` blocks until the announcement timer expires.
#[test]
fn message_wait_blocks_until_announcement_expires() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState {
            headless: false,
            ..crate::logic::world::LogicWorldState::new()
        });
    sim.ecs
        .0
        .insert_resource(crate::logic::world::MessageState::default());
    place_world_processor(
        &mut sim,
        10,
        10,
        "message announce 2 0\nmessage announce 2 @wait\nset x 2\nstop\n",
    );
    let state_at = |sim: &Sim| {
        let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
        sim.ecs
            .0
            .get::<crate::logic::blocks::LogicBlockState>(processor)
            .expect("state")
            .executor
            .optional_var("x")
            .map(|id| {
                sim.ecs
                    .0
                    .get::<crate::logic::blocks::LogicBlockState>(processor)
                    .expect("state")
                    .executor
                    .arena
                    .get(id)
                    .num()
            })
            .unwrap_or(0.0)
    };
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    {
        let messages = sim
            .ecs
            .0
            .get_resource::<crate::logic::world::MessageState>()
            .expect("messages");
        let tick = sim
            .ecs
            .0
            .get_resource::<crate::logic::world::LogicWorldState>()
            .expect("state")
            .tick;
        assert!(
            messages.announce_until > 0,
            "first announce armed the timer (tick={tick})"
        );
        assert!(
            tick < messages.announce_until,
            "still inside the announcement window (tick={tick}, until={})",
            messages.announce_until
        );
    }
    assert_eq!(state_at(&sim), 0.0, "second announce is still blocked");
    for _ in 0..140 {
        sim.tick().expect("tick");
    }
    assert_eq!(state_at(&sim), 2.0, "wait released after the announcement");
}

/// H06 T4: `take_logic_messages` drains queued `announce`/`notify`/`toast`
/// events (resolved text + duration) exactly once.
#[test]
fn message_announce_drained() {
    use crate::logic::enums::MessageType;

    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    install_logic_world(&mut sim);
    place_world_processor(
        &mut sim,
        10,
        10,
        "print \"hi\"\nmessage announce 3 ok\nstop\n",
    );
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let messages = sim.take_logic_messages();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0].0, MessageType::Announce);
    assert_eq!(messages[0].1, "hi");
    assert!((messages[0].2 - 3.0).abs() < 1e-6);
    assert!(
        sim.take_logic_messages().is_empty(),
        "messages drain exactly once"
    );
    // `outSuccess` stays 1 for a delivered (non-blocked) message.
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let ok = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(e)
        .and_then(|state| {
            state
                .executor
                .optional_var("ok")
                .map(|id| state.executor.arena.get(id).num())
        });
    assert_eq!(ok, Some(1.0));
}

/// H06 T6: `setprop` on an item stack SETS the count (`BuildingComp.setProp`,
/// `BuildingComp.java:2210-2227`), not adds; re-applying is a no-op.
#[test]
fn setprop_item_sets_count() {
    use crate::content::ContentRef;
    use crate::logic::access::set_prop_content;
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    let container = place(&mut sim, "container", 5, 5);
    let copper = item(&sim, "copper");
    let target = LogicObject::Building(container);
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        50.0
    ));
    assert_eq!(stored_item(&sim, container, copper), 50);
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        10.0
    ));
    assert_eq!(stored_item(&sim, container, copper), 10, "set, not added");
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        10.0
    ));
    assert_eq!(stored_item(&sim, container, copper), 10, "idempotent");

    // A core accepts up to its capacity (`getMaximumAccepted`), not 99999.
    let core = place(&mut sim, "core-shard", 10, 10);
    let capacity = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::core_block::CoreBuild>(core)
        .map(|core| core.storage_capacity)
        .expect("core build");
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &LogicObject::Building(core),
        ContentRef::item(copper),
        99999.0
    ));
    let stored = stored_item(&sim, core, copper);
    assert!(
        stored <= capacity,
        "capacity-limited: {stored} <= {capacity}"
    );
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &LogicObject::Building(core),
        ContentRef::item(copper),
        99999.0
    ));
    assert_eq!(stored_item(&sim, core, copper), stored, "stable on repeat");
}

/// H06 T6: `setprop @health u 0` kills the unit through the combat path
/// (`UnitComp.setProp` health; `UnitComp.java:356-362`).
#[test]
fn setprop_unit_health_zero_kills() {
    use crate::logic::LAccess;
    use crate::logic::access::set_prop_num;
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let unit = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    assert!(set_prop_num(
        &mut sim.ecs.0,
        &LogicObject::Unit(unit),
        LAccess::Health,
        0.0
    ));
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs.0.get_entity(unit).is_err(),
        "the unit was destroyed by resolve_deaths"
    );
}

/// H06 T6: `setprop` unit item stack + status duration + `@speed`/`@armor`
/// dynamic status (`UnitComp.setProp`; the multipliers survive ticks).
#[test]
fn setprop_unit_stack_and_status() {
    use crate::content::ContentRef;
    use crate::entities::comp::unit::comp::{DynamicStatus, ItemsComp, StatusComp};
    use crate::logic::LAccess;
    use crate::logic::access::{set_prop_content, set_prop_num};
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let unit = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    let copper = item(&sim, "copper");
    let wet = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .status_id("wet")
        .expect("wet");
    let dagger_speed = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .and_then(|id| sim.block_runtime().expect("runtime").content().unit(id))
        .map(|def| def.speed)
        .expect("dagger def");
    let target = LogicObject::Unit(unit);
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        20.0
    ));
    assert_eq!(
        sim.ecs
            .0
            .get::<ItemsComp>(unit)
            .and_then(|items| items.item),
        Some((copper, 20)),
        "unit stack set"
    );
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::new(crate::content::ContentType::Status, wet.raw()),
        3.0
    ));
    assert!(
        (sim.ecs
            .0
            .get::<StatusComp>(unit)
            .expect("status")
            .get_duration(wet)
            - 180.0)
            .abs()
            < 1e-3,
        "3 s = 180 ticks"
    );
    // `@speed` -> `applyDynamicStatus().speedMultiplier = v / (speed * 60 / 8)`.
    assert!(set_prop_num(&mut sim.ecs.0, &target, LAccess::Speed, 10.0));
    let dynamic = sim
        .ecs
        .0
        .get::<DynamicStatus>(unit)
        .copied()
        .expect("dynamic status");
    assert!(
        (dynamic.speed_multiplier - 10.0 / (dagger_speed * 60.0 / 8.0)).abs() < 1e-4,
        "{:?}",
        dynamic.speed_multiplier
    );
    assert!(set_prop_num(&mut sim.ecs.0, &target, LAccess::Armor, 5.0));
    // A few ticks: the per-tick status update folds the dynamic multipliers in
    // instead of resetting the override (the old bug).
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let status = sim.ecs.0.get::<StatusComp>(unit).expect("status").clone();
    assert!(
        (status.armor_override - 5.0).abs() < 1e-4,
        "armor override survives ticks: {}",
        status.armor_override
    );
    assert!(
        status.has_effect_of(
            sim.block_runtime()
                .expect("runtime")
                .content()
                .status_id("dynamic")
                .expect("dynamic id")
        )
    );
}

/// H06 T6: extractionOutpost block 2 verbatim on a team-2 core linked as
/// `nucleus1` -> `setprop @pyratite 99999` fills to the capacity limit.
#[test]
fn extraction_outpost_block2() {
    use crate::logic::blocks::{LogicLink, logic_block};
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
    // Team-2 core-nucleus (its link name root is `nucleus`).
    let core_id = sim.world_block_id("core-nucleus").expect("core-nucleus");
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: core_id,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("core placed");
    let core = sim
        .grid
        .entity_at(TilePos::new(14, 10))
        .expect("core entity");
    // World processor at (10,10) with the core linked as a relative `(4, 0)`.
    let code = "sensor result nucleus1 @team\njump 0 notEqual result 2\nsetprop @pyratite nucleus1 99999\nsetprop @metaglass nucleus1 99999\nsetprop @titanium nucleus1 99999\nsetprop @graphite nucleus1 99999\n";
    let processor = sim
        .world_block_id("world-processor")
        .expect("world-processor");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: processor,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("processor placed");
    let bytes = logic_block::compress(code, &[LogicLink::new("nucleus1", 4, 0)]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let entity = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let state = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(entity)
        .expect("state");
    let d = state.executor.optional_var("nucleus1");
    assert!(d.is_some(), "the link resolved to a variable");
    assert!(
        state
            .executor
            .arena
            .get(d.expect("id"))
            .value_obj()
            .is_some_and(|obj| matches!(obj, LogicObject::Building(e) if *e == core)),
        "nucleus1 binds the core"
    );
    let pyratite = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("pyratite")
        .expect("pyratite");
    let stored = stored_item(&sim, core, pyratite);
    let capacity = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::core_block::CoreBuild>(core)
        .map(|core| core.storage_capacity)
        .expect("core build");
    assert!(stored > 0, "pyratite set on the core");
    assert_eq!(stored, 99999.min(capacity), "capacity-limited set");
}

// ---- G06 T7 (sensor/control coverage) ----

/// Ground Zero's `duoAmmo` objective: `sensor amount d @ammo` on a duo reads
/// `TurretState.total_ammo` (`ItemTurretBuild.sense`), not 0.
#[test]
fn duo_ammo_sensed() {
    let mut sim = runtime_sim();
    let duo = place(&mut sim, "duo", 20, 20);
    let copper = item(&sim, "copper");
    let world = &mut sim.ecs.0;
    assert!(transfer::dispatch_accept_item(world, duo, duo, copper));
    for _ in 0..5 {
        transfer::dispatch_handle_item(world, duo, duo, copper);
    }
    let total = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map(|state| state.total_ammo)
        .unwrap_or(0);
    assert!(total > 0, "turret holds ammo");
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::logic::value::LogicObject;
    let target = LogicObject::Building(duo);
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::Ammo),
        Sensed::Num(total as f64)
    );
    let cap = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map(|state| state.config.max_ammo)
        .unwrap_or(0);
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::AmmoCapacity),
        Sensed::Num(cap as f64)
    );
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::Shooting),
        Sensed::Num(0.0)
    );
}

/// `@x/@y` match Java's building centre: 1x1/3x3 -> tile, 2x2 -> tile + 0.5.
#[test]
fn building_xy_tiles() {
    let mut sim = runtime_sim();
    let hyper = place(&mut sim, "hyper-processor", 10, 10);
    let logic = place(&mut sim, "logic-processor", 20, 20);
    let micro = place(&mut sim, "micro-processor", 3, 4);
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::logic::value::LogicObject;
    let x = |e| sense(&sim.ecs.0, &LogicObject::Building(e), LAccess::X);
    let y = |e| sense(&sim.ecs.0, &LogicObject::Building(e), LAccess::Y);
    assert_eq!(x(hyper), Sensed::Num(10.0));
    assert_eq!(x(logic), Sensed::Num(20.5));
    assert_eq!(x(micro), Sensed::Num(3.0));
    assert_eq!(y(micro), Sensed::Num(4.0));
}

/// G06 T3 (PAR-0107): `lookup item r 0` resolves through `logicids.dat`.
#[test]
fn lookup_resolves_content_from_logic_ids() {
    let mut sim = runtime_sim();
    let (globals, copper) = {
        let content = sim.block_runtime().expect("runtime").content();
        (
            crate::logic::globals::GlobalVars::with_content(content),
            content.item_id("copper").expect("copper"),
        )
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
    let bytes = crate::logic::blocks::logic_block::compress(
        "lookup item r 0\nlookup team t 1\nstop\n",
        &[],
    );
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let state = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(processor)
        .expect("state");
    let r = state.executor.optional_var("r").expect("r");
    assert!(
        matches!(
            &state.executor.arena.get(r).obj,
            Some(crate::logic::value::LogicObject::Content(c))
                if c.type_ == crate::content::ContentType::Item && c.id == copper.raw()
        ),
        "lookup item 0 -> copper"
    );
    let t = state.executor.optional_var("t").expect("t");
    assert_eq!(state.executor.arena.get(t).num(), 1.0, "team 1 = sharded");
}

/// H02 T1: a full campaign core keeps accepting items (Java
/// `Rules.coreIncinerates`, `CoreBlock.java:744-751,886-913`) and incinerates
/// the overflow instead of rejecting it.
#[test]
fn full_core_incinerates_and_counts() {
    let mut sim = runtime_sim();
    let core = place(&mut sim, "core-shard", 10, 10);
    let copper = item(&sim, "copper");
    let cap = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::CoreBuild>(core)
        .map(|c| c.storage_capacity)
        .expect("core build");
    assert_eq!(cap, 4000, "core-shard itemCapacity (Blocks.java:3152)");
    {
        let mut items = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(core)
            .expect("items");
        items.add(copper, cap, cap);
    }
    // Default hooks (no resource) incinerate, so the full core accepts and the
    // overflow handle is dropped instead of stored.
    assert!(transfer::dispatch_accept_item(
        &sim.ecs.0, core, core, copper
    ));
    transfer::dispatch_handle_item(&mut sim.ecs.0, core, core, copper);
    assert_eq!(stored_item(&sim, core, copper), cap, "overflow incinerated");
}

/// H02 T1: `Rules.coreIncinerates = false` restores the rejecting behavior.
#[test]
fn core_rejects_when_not_incinerating() {
    #[derive(Debug)]
    struct NotIncinerating;
    impl crate::world::blocks::storage::CoreCampaignHooks for NotIncinerating {
        fn core_incinerates(&self) -> bool {
            false
        }
    }
    let mut sim = runtime_sim();
    let core = place(&mut sim, "core-shard", 10, 10);
    let copper = item(&sim, "copper");
    let cap = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::CoreBuild>(core)
        .map(|c| c.storage_capacity)
        .expect("core build");
    {
        let mut items = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(core)
            .expect("items");
        items.add(copper, cap, cap);
    }
    sim.ecs
        .0
        .insert_resource(crate::world::blocks::storage::CoreHooks(
            std::sync::Arc::new(NotIncinerating),
        ));
    assert!(!transfer::dispatch_accept_item(
        &sim.ecs.0, core, core, copper
    ));
}

/// H02 T2: `sync_build_rules` copies the placement/break rule subset of a
/// `Rules` value into the `BuildRules` resource (`World.setRules` half).
#[test]
fn sync_build_rules_copies_rules() {
    use crate::game::rules::Rules;
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let copper_wall = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("copper-wall")
        .expect("copper-wall");
    let titanium_wall = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("titanium-wall")
        .expect("titanium-wall");
    let mut rules = Rules::default();
    rules.banned_blocks.insert("copper-wall".to_owned());
    rules.block_whitelist = false;
    rules.derelict_repair = false;
    rules.place_range_check = true;
    rules.enemy_core_build_radius = 320.0;
    rules.team_rule_mut(2).extra_core_build_radius = 40.0;
    crate::world::limits::sync_build_rules(&mut sim.ecs.0, &rules, true);
    {
        let build = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
        assert!(build.is_banned(copper_wall), "banned list copied");
        assert!(!build.allow_derelict_repair);
        assert!(build.place_range_check);
        assert_eq!(build.build_radius(2), 360.0, "320 + 40 extra");
        assert_eq!(build.build_radius(1), 320.0, "default team extra 0");
        assert!(build.is_campaign);
        assert_eq!(build.banned_blocks, vec![copper_wall]);
    }
    // `Rules.isBanned`: `blockWhitelist != bannedBlocks.contains(block)`.
    rules.block_whitelist = true;
    crate::world::limits::sync_build_rules(&mut sim.ecs.0, &rules, true);
    let build = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
    assert!(!build.is_banned(copper_wall), "whitelisted");
    assert!(build.is_banned(titanium_wall), "not in the whitelist");
}

/// H02 T2: `Build.validBreak` rejects other teams' buildings
/// (`Build.java:317-320`, `Teams.java:125-127`); derelict and own team stay
/// breakable.
#[test]
fn cannot_break_enemy_building() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let wall = sim.world_block_id("copper-wall").expect("copper-wall");
    for (x, y, team) in [(5i16, 5i16, 2u8), (7, 5, 0), (9, 5, 1)] {
        sim.command(SimCommand::Place {
            x,
            y,
            block: wall,
            rotation: 0,
            team,
            player: None,
        })
        .expect("place wall");
    }
    let rules = sim
        .ecs
        .0
        .get_resource::<BuildRules>()
        .cloned()
        .expect("rules");
    let runtime = sim.block_runtime().expect("runtime");
    assert!(
        !crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            5,
            5
        ),
        "enemy building rejected"
    );
    assert!(
        crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            7,
            5
        ),
        "derelict building allowed"
    );
    assert!(
        crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            9,
            5
        ),
        "own building allowed"
    );
}

/// H02 T2: `CoreBlock.canBreak` (`CoreBlock.java:173-177`) keeps the last core
/// unbreakable; `coreBuildAndConfig` with a second core allows it.
#[test]
fn last_core_unbreakable() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let core = sim.world_block_id("core-shard").expect("core-shard");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: core,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("place core");
    let rules = sim
        .ecs
        .0
        .get_resource::<BuildRules>()
        .cloned()
        .expect("rules");
    let runtime = sim.block_runtime().expect("runtime");
    assert!(
        !crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            10,
            10
        ),
        "last core unbreakable"
    );
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .core_build_and_config = true;
    sim.command(SimCommand::Place {
        x: 20,
        y: 20,
        block: core,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("place second core");
    let rules = sim
        .ecs
        .0
        .get_resource::<BuildRules>()
        .cloned()
        .expect("rules");
    let runtime = sim.block_runtime().expect("runtime");
    assert!(
        crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            10,
            10
        ),
        "two cores with coreBuildAndConfig are breakable"
    );
}

/// H02 T3: a placement query with the full flag set for the live world.
#[cfg(test)]
fn place_query(
    sim: &Sim,
    x: i16,
    y: i16,
    block: &str,
    rot: u8,
    team: u8,
    flags: crate::world::build::PlaceFlags,
) -> bool {
    let runtime = sim.block_runtime().expect("runtime");
    let rules = sim
        .ecs
        .0
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("table");
    let counter = sim
        .ecs
        .0
        .get_resource::<crate::world::limits::BlockCounter>()
        .expect("counter");
    let id = runtime.content().block_id(block).expect("block");
    crate::world::build::valid_place_query(
        runtime.content(),
        table,
        &rules,
        counter,
        &sim.grid,
        Some(&sim.ecs.0),
        id,
        team,
        rot,
        i32::from(x),
        i32::from(y),
        flags,
    )
}

fn place_team(sim: &mut Sim, x: i16, y: i16, block: &str, rot: i8, team: u8) -> Entity {
    let id = sim.world_block_id(block).expect("block");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: rot,
        team,
        player: None,
    })
    .expect("place");
    sim.grid
        .tiles
        .get(i32::from(x), i32::from(y))
        .build
        .expect("built")
}

/// H02 T3: enemy buildings cannot be replaced (`Build.java:250-262`); own
/// conveyor upgrades pass.
#[test]
fn cannot_replace_enemy_conveyor() {
    use crate::world::build::PlaceFlags;

    let mut sim = runtime_sim();
    place_team(&mut sim, 5, 5, "conveyor", 0, 2);
    assert!(
        !place_query(&sim, 5, 5, "titanium-conveyor", 0, 1, PlaceFlags::PLAYER),
        "enemy conveyor cannot be replaced"
    );
    place_team(&mut sim, 7, 5, "conveyor", 0, 1);
    assert!(
        place_query(&sim, 7, 5, "titanium-conveyor", 0, 1, PlaceFlags::PLAYER),
        "own conveyor upgrades"
    );
}

/// H02 T3: deep water rejects non-floating blocks and pumps-without-contact
/// (`Build.java:249,258`); a conduit at the deep-water edge passes via
/// `contactsShallows` (`Build.java:294-314`).
#[test]
fn cannot_build_on_deep_water() {
    use crate::world::build::PlaceFlags;

    let mut sim = runtime_sim();
    for x in 7..=9 {
        for y in 7..=9 {
            set_floor(&mut sim, x, y, "deep-water");
        }
    }
    assert!(
        !place_query(&sim, 8, 8, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "wall on deep water"
    );
    assert!(
        !place_query(&sim, 8, 8, "conduit", 0, 1, PlaceFlags::PLAYER),
        "conduit in the middle of deep water has no shallows contact"
    );
    assert!(
        place_query(&sim, 7, 8, "conduit", 0, 1, PlaceFlags::PLAYER),
        "conduit at the deep-water edge touches the stone neighbour"
    );
}

/// H02 T3: same block + same rotation is rejected (`Build.java:244`); a
/// different rotation passes; non-rotating blocks reject same over same.
#[test]
fn same_block_same_rotation_rejected() {
    use crate::world::build::PlaceFlags;

    let mut sim = runtime_sim();
    place_team(&mut sim, 5, 5, "conveyor", 0, 1);
    assert!(
        !place_query(&sim, 5, 5, "conveyor", 0, 1, PlaceFlags::PLAYER),
        "same rotation rejected"
    );
    assert!(
        place_query(&sim, 5, 5, "conveyor", 1, 1, PlaceFlags::PLAYER),
        "different rotation replaces"
    );
    place_team(&mut sim, 9, 5, "router", 0, 1);
    assert!(
        !place_query(&sim, 9, 5, "router", 0, 1, PlaceFlags::PLAYER),
        "a router does not rotate, so router-over-router is rejected"
    );
}

/// H02 T3: derelict same-block repair (`Build.java:251`);
/// `allowDerelictRepair = false` rejects it.
#[test]
fn derelict_same_block_repair() {
    use crate::world::build::PlaceFlags;
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    place_team(&mut sim, 5, 5, "copper-wall", 0, 0);
    assert!(
        place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "derelict same-block repair passes"
    );
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .allow_derelict_repair = false;
    assert!(
        !place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "derelict repair disabled"
    );
}

/// H02 T3: the enemy-core no-build radius (`Build.java:195-208`,
/// `Rules.buildRadius` `Rules.java:294-296`) gates player placement but not
/// the API form.
#[test]
fn enemy_core_radius_blocks_placement() {
    use crate::content::BlockId;
    use crate::world::build::PlaceFlags;
    use crate::world::limits::BuildRules;

    let mut sim = Sim::new(3, 128, 128, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .enemy_core_build_radius = 400.0;
    place_team(&mut sim, 40, 40, "core-shard", 0, 2);
    place_team(&mut sim, 40, 90, "copper-wall", 0, 1);
    // Core center (324, 324); wall center (324, 724): distance 400 < 400+8.
    assert!(
        !place_query(&sim, 40, 90, "titanium-wall", 0, 1, PlaceFlags::PLAYER),
        "inside the enemy core radius"
    );
    // (40, 92) center (324, 740): distance 416 > 408.
    assert!(
        place_query(&sim, 40, 92, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "outside the enemy core radius"
    );
    assert!(
        place_query(&sim, 40, 90, "titanium-wall", 0, 1, PlaceFlags::API),
        "API skips the core radius"
    );
    assert!(
        place_query(&sim, 40, 92, "copper-wall", 0, 1, PlaceFlags::API),
        "API outside the radius"
    );
}

/// H02 T3: bans/whitelist only gate the visible (player) form
/// (`Block.isPlaceable` `Block.java:1031-1037`).
#[test]
fn banned_block_rejected_for_player_only() {
    use crate::world::build::PlaceFlags;
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .banned_blocks = vec![
        sim.block_runtime()
            .expect("runtime")
            .content()
            .block_id("copper-wall")
            .expect("copper-wall"),
    ];
    assert!(
        !place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "banned for player placement"
    );
    assert!(
        place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::API),
        "API skips bans"
    );
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .block_whitelist = true;
    assert!(
        place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "whitelist inverts the ban"
    );
}

/// H02 T3: `kind_rotates` matches the Java `rotate = true` set
/// (`grep -rn "rotate = true\|rotate = false" J/world/blocks`).
#[test]
fn kind_rotates_matches_java() {
    use crate::content::BlockKind as K;
    use crate::world::block::kind_rotates;

    for kind in [
        K::Conveyor,
        K::Duct,
        K::Conduit,
        K::ItemTurret,
        K::UnitFactory,
        K::Reconstructor,
        K::HeatProducer,
        K::BeamDrill,
        K::PowerDiode,
        K::PayloadSource,
        K::ArmoredConveyor,
        K::ArmoredConduit,
        K::DuctBridge,
        K::StackRouter,
        K::Constructor,
        K::PayloadRouter,
        K::PayloadUnloader,
        K::RepairTurret,
        K::UnitAssemblerModule,
        K::WallCrafter,
    ] {
        assert!(kind_rotates(kind), "{kind:?} rotates");
    }
    for kind in [
        K::Junction,
        K::ItemBridge,
        K::Sorter,
        K::Router,
        K::OverflowGate,
        K::MassDriver,
        K::LiquidRouter,
        K::LiquidJunction,
        K::Drill,
        K::BurstDrill,
        K::Door,
        K::AutoDoor,
        K::PowerNode,
        K::BeamNode,
        K::PayloadDeconstructor,
        K::LiquidBridge,
        K::BufferedItemBridge,
    ] {
        assert!(!kind_rotates(kind), "{kind:?} does not rotate");
    }
}

/// H02 T8: per-block drill boost/warmup knobs (`Drill.java:38,40`,
/// `Blocks.java:2929,2933,3103`).
#[test]
fn blast_drill_boost_knobs_match_java() {
    use crate::world::BlockKindData;

    let sim = runtime_sim();
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("table");
    let drill_def = |name: &str| match &table.get_named(name).expect(name).kind_data {
        BlockKindData::Drill(def) => def.clone(),
        BlockKindData::BurstDrill(def) => def.drill.clone(),
        _ => panic!("not a drill"),
    };
    let blast = drill_def("blast-drill");
    assert_eq!(blast.liquid_boost_intensity, 1.8);
    assert_eq!(blast.warmup_speed, 0.01);
    let impact = drill_def("impact-drill");
    assert_eq!(impact.liquid_boost_intensity, 1.75);
    // `Drill.java:38,40` class defaults survive everywhere else.
    let mechanical = drill_def("mechanical-drill");
    assert_eq!(mechanical.liquid_boost_intensity, 1.6);
    assert_eq!(mechanical.warmup_speed, 0.015);
}

/// H02 T8: `Mathf.approachDelta(warmup, speed, warmupSpeed)` scales with the
/// building's delta (`Drill.java:304`): one tick at time scale 2 advances the
/// warmup by two steps.
#[test]
fn drill_warmup_scales_with_time_scale() {
    let mut sim = runtime_sim();
    for x in 10..12 {
        for y in 10..12 {
            set_floor(&mut sim, x, y, "ore-copper");
        }
    }
    let drill = place(&mut sim, "mechanical-drill", 10, 10);
    sim.tick().expect("tick");
    let warmup = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::entities::comp::DrillState>(drill)
            .map(|state| state.warmup)
            .unwrap_or(0.0)
    };
    assert!(
        (warmup(&sim) - 0.015).abs() < 1e-4,
        "one tick at 1x: {}",
        warmup(&sim)
    );
    {
        let mut building = sim
            .ecs
            .0
            .get_mut::<crate::entities::comp::Building>(drill)
            .expect("building");
        building.time_scale = 2.0;
        building.time_scale_duration = 1000.0;
    }
    sim.tick().expect("tick");
    assert!(
        (warmup(&sim) - 0.045).abs() < 1e-4,
        "overdrive doubles the warmup step: {}",
        warmup(&sim)
    );
}
/// H09 T4: a derelict building stops updating (`BuildingComp.allowUpdate`,
/// `BuildingComp.java:627-632`): a team-0 drill mines nothing.
#[test]
fn derelict_drill_does_not_mine() {
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "ore-copper");
    }
    let drill = place(&mut sim, "mechanical-drill", 5, 5);
    let copper = item(&sim, "copper");
    let container = place(&mut sim, "container", 7, 5);
    assert!(
        sim.debug_set_building_team(5, 5, 0),
        "team switch to derelict"
    );
    let enabled = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(drill)
        .map(|building| building.enabled);
    assert_eq!(enabled, Some(false), "derelict drill is disabled");
    for _ in 0..1625 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, container, copper),
        0,
        "a derelict drill never mines"
    );

    // Control: the same drill on team 1 mines (G02 T6 rate).
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "ore-copper");
    }
    place(&mut sim, "mechanical-drill", 5, 5);
    let container = place(&mut sim, "container", 7, 5);
    for _ in 0..1625 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_item(&sim, container, copper) >= 9,
        "a team-1 drill mines ~10 copper in 1625 ticks"
    );
}

/// H09 T4: buildings outside a limited map area are disabled; growing the area
/// re-enables the auto-reset ones (`World.checkMapArea`,
/// `World.java:469-481`).
#[test]
fn outside_map_area_disabled_and_reset() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;

    let mut sim = runtime_sim();
    let drill = place(&mut sim, "mechanical-drill", 20, 20);
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.limit_map_area = true;
    rules.limit_width = 16;
    rules.limit_height = 16;
    rules.disable_outside_area = true;
    install_campaign_session(&mut sim, rules.clone());
    sim.apply_rules_to_world(&rules);
    assert_eq!(
        sim.ecs.0.get::<Building>(drill).map(|b| b.enabled),
        Some(false),
        "a drill outside the 16x16 area is disabled"
    );

    rules.limit_width = 32;
    rules.limit_height = 32;
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.rules = rules.clone();
    }
    sim.apply_rules_to_world(&rules);
    assert_eq!(
        sim.ecs.0.get::<Building>(drill).map(|b| b.enabled),
        Some(true),
        "growing the area re-enables the auto-reset drill"
    );
}

/// H09 T4: a block whose env flags do not support the planet's env stops
/// updating (`Block.supportsEnv`, `BuildingComp.allowUpdate`).
#[test]
fn env_unsupported_block_disabled() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::world::build::supports_env_bits;

    let mut sim = runtime_sim();
    let name = "cultivator";
    let def = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_by_name(name)
        .expect("def")
        .clone();
    // Erekir env (17 = space | scorching).
    assert!(
        !supports_env_bits(&def, 17),
        "{name} (requires spores) is not supported on Erekir"
    );
    let generator = place(&mut sim, name, 5, 5);
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.env = 17;
    install_campaign_session(&mut sim, rules.clone());
    sim.apply_rules_to_world(&rules);
    assert_eq!(
        sim.ecs.0.get::<Building>(generator).map(|b| b.enabled),
        Some(false),
        "{name} is disabled under env 17"
    );
}

/// H09 T8: `TeamRule.fillItems` refills the first core of every active team
/// (`Logic.java:552-561`): with `coreIncinerates` the max accepted is
/// `i32::MAX / 2`, hidden items stay 0 and without incineration the core's
/// storage capacity is the cap.
#[test]
fn fill_items_fills_ai_core() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::game::runtime::{
        CampaignRuntime, install_campaign_core_hooks, sync_session_with_sim,
    };
    use crate::world::TilePos;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.team_rule_mut(2).fill_items = true;
    rules.core_incinerates = true;
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    // `PlaySession` starts in `Menu`; the campaign systems only run in
    // `Playing`.
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    // API staging form (`player: None`): the player core-placement gate
    // (`CoreBlock.canPlaceOn`, H02 T4) rightly refuses a fresh core on free
    // ground through the player form.
    sim.command(crate::determinism::SimCommand::Place {
        x: 5,
        y: 5,
        block: core.raw(),
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("team-2 core placed");
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
    let copper = item(&sim, "copper");
    assert_eq!(
        stored_item(&sim, entity, copper),
        i32::MAX / 2,
        "coreIncinerates -> getMaximumAccepted = i32::MAX / 2"
    );
    // A hidden item is never filled.
    let hidden = {
        let content = sim.block_runtime().expect("rt").content();
        content
            .items()
            .iter()
            .find(|item| item.is_hidden())
            .map(|item| item.id)
    };
    if let Some(hidden) = hidden {
        assert_eq!(
            stored_item(&sim, entity, hidden),
            0,
            "hidden items are skipped"
        );
    }
    assert!(
        sim.ecs.0.get::<Building>(entity).is_some(),
        "the core still exists"
    );

    // Without `coreIncinerates` the cap is the core's storage capacity.
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.team_rule_mut(2).fill_items = true;
    rules.core_incinerates = false;
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    sim.command(crate::determinism::SimCommand::Place {
        x: 5,
        y: 5,
        block: core.raw(),
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("team-2 core placed");
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
    let capacity = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::core_block::CoreBuild>(entity)
        .map(|core| core.storage_capacity)
        .expect("core build");
    assert_eq!(
        stored_item(&sim, entity, copper),
        capacity,
        "without incineration the core fills to its storage capacity"
    );
}

/// H06 T20 blocker regression: `team_ai_system` must skip teams whose rule
/// leaves `fillItems` false (`Logic.java:553-554`). The missing gate filled
/// the player team's core with `i32::MAX / 2` of every item every tick and
/// wrote an unloadable sector save.
#[test]
fn fill_items_gate_skips_other_teams() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::game::runtime::{
        CampaignRuntime, install_campaign_core_hooks, sync_session_with_sim,
    };
    use crate::world::TilePos;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let rules = Rules {
        default_team: 1,
        core_incinerates: true,
        ..Rules::default()
    };
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    sim.command(SimCommand::Place {
        x: 5,
        y: 5,
        block: core.raw(),
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("team-2 core placed");
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    let copper = item(&sim, "copper");
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
    assert_eq!(
        stored_item(&sim, entity, copper),
        0,
        "team 2 leaves fillItems off: the core is untouched"
    );
    // Flip the rule: the same core is refilled.
    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("runtime")
        .session
        .rules
        .team_rule_mut(2)
        .fill_items = true;
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored_item(&sim, entity, copper),
        i32::MAX / 2,
        "fillItems on: refilled to getMaximumAccepted"
    );
    assert!(sim.ecs.0.get::<Building>(entity).is_some(), "core exists");
}

/// PAR-0249 regression (found by the H03 session): only teams whose rule sets
/// `fillItems` may be refilled (`Logic.java:554`
/// `if(rules.fillItems && data.cores.size > 0)`). The campaign sets it on the
/// wave team alone (`apply_world_load_campaign_rules`), so the PLAYER core must
/// stay empty — filling it wrote `i32::MAX / 2` per item (`coreIncinerates`)
/// into every exit save.
#[test]
fn fill_items_does_not_fill_player_core() {
    use crate::game::rules::Rules;
    use crate::game::runtime::{
        CampaignRuntime, install_campaign_core_hooks, sync_session_with_sim,
    };
    use crate::world::TilePos;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    // Campaign shape: only the wave team's rule gets `fillItems`.
    rules.team_rule_mut(1).fill_items = false;
    rules.team_rule_mut(2).fill_items = true;
    rules.core_incinerates = true;
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    for (x, y, team) in [(5, 5, 1u8), (15, 15, 2u8)] {
        sim.command(crate::determinism::SimCommand::Place {
            x,
            y,
            block: core.raw(),
            rotation: 0,
            team,
            player: None,
        })
        .expect("core placed");
    }
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    for _ in 0..2 {
        sim.tick().expect("tick");
    }
    let copper = item(&sim, "copper");
    let player_core = sim.grid.entity_at(TilePos::new(5, 5)).expect("player core");
    let enemy_core = sim
        .grid
        .entity_at(TilePos::new(15, 15))
        .expect("enemy core");
    assert_eq!(
        stored_item(&sim, player_core, copper),
        0,
        "the player core (fillItems=false) is never refilled"
    );
    assert_eq!(
        stored_item(&sim, enemy_core, copper),
        i32::MAX / 2,
        "the wave team's core is still filled (fillItems=true)"
    );
}

/// H09 T10: the `Logic` event counters (`Logic.java:186-216`) follow player
/// builds, deconstructs and losses.
#[test]
fn game_stats_count_build_break_destroy() {
    use crate::command::Command;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;

    let mut sim = runtime_sim();
    install_campaign_session(
        &mut sim,
        Rules {
            default_team: 1,
            ..Rules::default()
        },
    );
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));

    // Two player-team walls; the API place path emits `BlockPlacedEvent`
    // (`BlockBuildEndEvent`).
    assert!(sim.place_for_team(5, 5, wall, 0, 1));
    assert!(sim.place_for_team(7, 5, wall, 0, 1));
    // Deconstruct one through the break command (`BlockBuildEndEvent{breaking}`).
    sim.apply(Command::Break { x: 5, y: 5 }).expect("break");
    // Destroy one player wall and one enemy wall (`BlockDestroyEvent`).
    let player_wall = sim.grid.entity_at(TilePos::new(7, 5)).expect("player wall");
    if let Some(mut health) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::Health>(player_wall)
    {
        health.health = 0.0;
    }
    assert!(sim.place_for_team(9, 5, wall, 0, 2));
    let enemy_wall = sim.grid.entity_at(TilePos::new(9, 5)).expect("enemy wall");
    if let Some(mut health) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::Health>(enemy_wall)
    {
        health.health = 0.0;
    }
    // One tick resolves the deaths and flushes the events.
    sim.tick().expect("tick");
    sim.tick().expect("tick");

    let stats = &sim
        .ecs
        .0
        .get_resource::<CampaignRuntime>()
        .expect("runtime")
        .session
        .stats;
    assert_eq!(stats.buildings_built, 2, "two walls built");
    assert_eq!(stats.buildings_deconstructed, 1, "one wall deconstructed");
    assert_eq!(stats.buildings_destroyed, 1, "one player wall destroyed");
    assert_eq!(
        stats.get_destroyed("copper-wall"),
        1,
        "one enemy wall destroyed by name"
    );
    // `placedBlockCount` only counts the default team's placements.
    assert_eq!(stats.placed_block_count.get("copper-wall"), Some(&2));
}

/// H09 T10: a factory-produced default-team unit bumps `unitsCreated`
/// (`Logic.java:228-235`).
#[test]
fn game_stats_count_units_created() {
    use crate::game::rules::Rules;

    let mut sim = runtime_sim();
    install_campaign_session(
        &mut sim,
        Rules {
            default_team: 1,
            // No core in this fixture; Java's unit-cap check would kill the
            // factory output at spawn.
            disable_unit_cap: true,
            ..Rules::default()
        },
    );
    if let Some(mut runtime) = sim
        .ecs
        .0
        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
    {
        runtime.session.phase = crate::game::State::Playing;
    }
    let factory = place(&mut sim, "ground-factory", 5, 5);
    let (silicon, lead) = (item(&sim, "silicon"), item(&sim, "lead"));
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(factory) {
            items.add(silicon, 10, 60);
            items.add(lead, 10, 60);
        }
    }
    let _source = place(&mut sim, "power-source", 3, 5);
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, dagger) {
            break;
        }
    }
    assert!(spawned_unit(&sim, dagger), "factory produced a dagger");
    let stats = &sim
        .ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .expect("runtime")
        .session
        .stats;
    assert_eq!(stats.units_created, 1, "one unit created");
}

/// PAR-0190: wire `ContentId`/`Bool` configs reach `configured()` — sorter,
/// door (with Java's 60-tick toggle throttle) and switch.
#[test]
fn configure_reaches_sorter_door_switch() {
    use crate::content::ContentType;
    use crate::entities::comp::{Building, DoorState};

    let mut sim = runtime_sim();
    let sorter = place(&mut sim, "sorter", 5, 5);
    let door = place(&mut sim, "door", 8, 5);
    let switch = place(&mut sim, "switch", 11, 5);
    let copper = item(&sim, "copper");

    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::ContentId {
            kind: ContentType::Item as u8,
            id: copper.raw(),
        },
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::world::blocks::distribution::SorterBuild>(sorter)
            .and_then(|sorter| sorter.sort_item),
        Some(copper),
        "typed item id reached Sorter.configured"
    );

    configure(&mut sim, 8, 5, crate::determinism::ConfigValue::Bool(true));
    assert!(
        sim.ecs
            .0
            .get::<DoorState>(door)
            .is_some_and(|state| state.open),
        "door opened"
    );
    for _ in 0..61 {
        sim.tick().expect("tick");
    }
    configure(&mut sim, 8, 5, crate::determinism::ConfigValue::Bool(false));
    assert!(
        sim.ecs
            .0
            .get::<DoorState>(door)
            .is_some_and(|state| !state.open),
        "door closed after the toggle throttle elapsed"
    );

    configure(
        &mut sim,
        11,
        5,
        crate::determinism::ConfigValue::Bool(false),
    );
    assert!(
        !sim.ecs.0.get::<Building>(switch).expect("switch").enabled,
        "switch disabled"
    );
}

/// PAR-0190: an absolute packed `Integer` config links a bridge conveyor and
/// the sim keeps ticking (the reported `configure_building(..,"int",packed)`
/// hang; `ItemBridge.java:66-69`).
#[test]
fn bridge_configure_int_absolute_links_and_ticks() {
    use crate::world::blocks::distribution::ItemBridgeBuild;
    use crate::world::modules::ItemModule;

    let mut sim = runtime_sim();
    let from = place(&mut sim, "bridge-conveyor", 5, 5);
    let _to = place(&mut sim, "bridge-conveyor", 8, 5);
    let belt = place(&mut sim, "conveyor", 9, 5);
    let copper = item(&sim, "copper");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(from) {
        items.add(copper, 10, 10);
    }
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Int(TilePos::new(8, 5).pack()),
    );
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(TilePos::new(8, 5).pack()),
        "absolute packed int links the bridge"
    );
    // Must terminate: the G03 hang was on the next step after this configure.
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let dest = sim.grid.entity_at(TilePos::new(8, 5)).expect("dest bridge");
    let belt_items = sim
        .ecs
        .0
        .get::<ItemModule>(belt)
        .map(|module| module.get(copper))
        .unwrap_or(0);
    assert!(
        stored_item(&sim, dest, copper) + belt_items > 0,
        "copper arrived at (8,5) or beyond (dest {}, belt {belt_items})",
        stored_item(&sim, dest, copper)
    );
}

/// PAR-0190: a relative `Point2` config links `(5,5)` to `(8,5)`
/// (`ItemBridge.config(Point2.class)`, `ItemBridge.java:66-69`); the old bridge
/// packed it as an absolute position and linked to tile (3,0).
#[test]
fn bridge_configure_point_relative() {
    use crate::world::blocks::distribution::ItemBridgeBuild;

    let mut sim = runtime_sim();
    let from = place(&mut sim, "bridge-conveyor", 5, 5);
    let _to = place(&mut sim, "bridge-conveyor", 8, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Point2(3, 0),
    );
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(TilePos::new(8, 5).pack())
    );
}

/// PAR-0190: a `Point2[]` (relative points) config autolinks a power node
/// (`PowerNode.config(Point2[].class)`, `PowerNode.java:97-109`).
#[test]
fn node_configure_point_array() {
    use crate::world::modules::PowerModule;

    let mut sim = runtime_sim();
    let node = place(&mut sim, "power-node", 5, 5);
    let _battery_a = place(&mut sim, "battery", 8, 5);
    let _battery_b = place(&mut sim, "battery", 5, 8);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Point2Array(vec![
            TilePos::new(3, 0).pack(),
            TilePos::new(0, 3).pack(),
        ]),
    );
    let links = sim
        .ecs
        .0
        .get::<PowerModule>(node)
        .map(|module| module.links.len())
        .unwrap_or(0);
    assert_eq!(links, 2, "node linked both batteries");
}

/// H02 T6: `SimCommand::ConfigTap` runs `config_tap` and applies the returned
/// configs in order (bridge tap-to-link through the sim command path).
#[test]
fn sim_config_tap_command_applies() {
    use crate::sim::ConfigTapKind;
    use crate::world::blocks::distribution::ItemBridgeBuild;

    let mut sim = runtime_sim();
    let from = place(&mut sim, "bridge-conveyor", 5, 5);
    let _to = place(&mut sim, "bridge-conveyor", 8, 5);
    sim.command(SimCommand::ConfigTap {
        x: 5,
        y: 5,
        other_x: 8,
        other_y: 5,
    })
    .expect("config tap");
    assert_eq!(sim.last_config_tap, Some(ConfigTapKind::Consume));
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(TilePos::new(8, 5).pack())
    );
    // Inverse tap: the reverse-link branch switches and clears the old link.
    sim.command(SimCommand::ConfigTap {
        x: 8,
        y: 5,
        other_x: 5,
        other_y: 5,
    })
    .expect("config tap");
    assert_eq!(sim.last_config_tap, Some(ConfigTapKind::Switch));
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(-1)
    );
    let to = sim.grid.entity_at(TilePos::new(8, 5)).expect("dest");
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(to).map(|b| b.link),
        Some(TilePos::new(5, 5).pack())
    );
}
// T14 tests (to append to client/rust/mind-core/src/sim/production_tests.rs)

/// H02 T14: `Separator` weighted outputs (`Blocks.java:1233-1249`:
/// `results = with(copper 5, lead 3, graphite 2, titanium 2)`,
/// `craftTime = 35`; pick is a uniform draw over the weight sum,
/// `Separator.java:132-162`).
#[test]
fn separator_weighted_results() {
    let mut sim = runtime_sim();
    let separator = place(&mut sim, "separator", 5, 5);
    let _power = place(&mut sim, "power-source", 7, 5);
    let _slag = place(&mut sim, "liquid-source", 4, 5);
    configure(
        &mut sim,
        4,
        5,
        crate::determinism::ConfigValue::Content("slag".to_owned()),
    );
    let results = [
        (item(&sim, "copper"), 41.67f32),
        (item(&sim, "lead"), 25.0),
        (item(&sim, "graphite"), 16.67),
        (item(&sim, "titanium"), 16.67),
    ];
    let mut counts = [0i32; 4];
    // 600 crafts at craftTime 35. The outputs are drained every tick so the
    // separator's itemCapacity never stalls the craft loop.
    for _ in 0..600 * 35 {
        sim.tick().expect("tick");
        if let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(separator)
        {
            for (slot, (id, _)) in results.iter().enumerate() {
                let held = module.get(*id);
                if held > 0 {
                    counts[slot] += held;
                    module.remove(*id, held);
                }
            }
        }
    }
    let total: i32 = counts.iter().sum();
    assert!(
        total >= 550,
        "separator crafted {total} items in 21000 ticks"
    );
    for (slot, (_, expected)) in results.iter().enumerate() {
        let pct = 100.0 * counts[slot] as f32 / total as f32;
        assert!(
            (pct - expected).abs() <= 5.0,
            "result {slot} {pct}% vs Java {expected}% (counts {counts:?})"
        );
    }
}

/// H02 T14: a configured mass driver shoots its payload to the linked driver
/// (`MassDriver.java` `updateTile`/`shoot`; `Blocks.java` `mass-driver`).
#[test]
fn mass_driver_shoots_to_linked() {
    let mut sim = runtime_sim();
    let first = place(&mut sim, "mass-driver", 5, 5);
    let second = place(&mut sim, "mass-driver", 25, 5);
    let _power_a = place(&mut sim, "power-source", 3, 5);
    let _power_b = place(&mut sim, "power-source", 27, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Int(TilePos::new(25, 5).pack()),
    );
    let copper = item(&sim, "copper");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(first)
    {
        module.add(copper, 50, 200);
    }
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let arrived = stored_item(&sim, second, copper);
    assert!(
        arrived > 0,
        "the linked mass driver delivered {arrived} copper in 600 ticks"
    );
}

/// H02 T14: `ThermalGeneratorBuild.updateTile` (`ThermalGenerator.java:87-89`):
/// `productionEfficiency = sumAttribute(heat) + Attribute.heat.env()`, power =
/// `powerProduction * productionEfficiency`. `Blocks.java:2543-2552`:
/// `powerProduction = 1.8`; `hotrock` heat is 0.5 (`Blocks.java:364-372`).
#[test]
fn thermal_generator_reads_heat_floor() {
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "hotrock");
    }
    let generator = place(&mut sim, "thermal-generator", 5, 5);
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let expected = 1.8 * 0.5 * 4.0;
    let production = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    let sum = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::behavior::ThermalState>(generator)
        .map(|state| state.sum)
        .unwrap_or(-1.0);
    let efficiency = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::generator::GeneratorState>(generator)
        .map(|state| state.production_efficiency)
        .unwrap_or(-1.0);
    assert!(
        (production - expected).abs() < 0.001,
        "thermal production {production} vs powerProduction * sum(heat) = {expected} (sum {sum}, efficiency {efficiency})"
    );
}

/// H02 T14: `steam-generator` needs its water consumer (`Blocks.java:2554-2560`
/// `consumeLiquid(Liquids.water, 0.1f)`): coal alone produces nothing.
#[test]
fn steam_generator_needs_water() {
    let mut sim = runtime_sim();
    let generator = place(&mut sim, "steam-generator", 5, 5);
    let coal = item(&sim, "coal");
    let water = liquid(&sim, "water");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(generator)
    {
        module.add(coal, 20, 200);
    }
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let dry = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    assert_eq!(dry, 0.0, "no water -> no production");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::LiquidModule>(generator)
    {
        module.add(water, 100.0, 1000.0);
    }
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let wet = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    assert!(wet > 0.0, "water -> production {wet}");
}

/// H02 T14: an unloader pulls a graphite press's output out (G02's generic
/// crafter test, pinned to the early-game chain).
#[test]
fn unloader_pulls_from_graphite_press() {
    let mut sim = runtime_sim();
    // Receiver lane: container <- belt(9,5) <- belt(8,5) <- unloader. Belts
    // resolve their forward target when placed, so build downstream-first.
    let container = place(&mut sim, "container", 10, 5);
    let _b2 = place(&mut sim, "conveyor", 9, 5);
    let _b1 = place(&mut sim, "conveyor", 8, 5);
    let press = place(&mut sim, "graphite-press", 5, 5);
    let _unloader = place(&mut sim, "unloader", 7, 5);
    let coal = item(&sim, "coal");
    let graphite = item(&sim, "graphite");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(press)
    {
        module.add(coal, 20, 200);
    }
    for _ in 0..1000 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_item(&sim, container, graphite) > 0,
        "graphite left the press (press {}, container {})",
        stored_item(&sim, press, graphite),
        stored_item(&sim, container, graphite)
    );
}

/// H02 T14: buffered bridge throughput. `BufferedItemBridge.updateTransport`
/// (`BufferedItemBridge.java:29-36`) sends one item per
/// `timer(timerAccept, 4)` = 15/s once the pipeline is full; `speed = 74`
/// (`Blocks.java:2111`) is the head item's ready latency, `bufferCapacity = 14`
/// (`:2113`). The bridge is fed directly: the titanium-conveyor rate
/// (4.8/s) is below the bridge's, so this pins the bridge.
#[test]
fn bridge_conveyor_throughput() {
    let mut sim = runtime_sim();
    let a = place(&mut sim, "bridge-conveyor", 5, 5);
    let b = place(&mut sim, "bridge-conveyor", 8, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Int(TilePos::new(8, 5).pack()),
    );
    let copper = item(&sim, "copper");
    let mut arrived = 0;
    for tick in 0..600 {
        sim.tick().expect("tick");
        let now = crate::world::update::build_time(&sim.ecs.0);
        if let Some(mut bridge) = sim
            .ecs
            .0
            .get_mut::<crate::world::blocks::distribution::ItemBridgeBuild>(a)
            && let Some(buffer) = bridge.buffer.as_mut()
        {
            // Stamped in the past: `speed = 74` is the head item's ready
            // latency, so this isolates the send timer (the sustained rate).
            buffer.accept(copper, -1, now - 200.0);
        }
        if let Some(mut module) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(b) {
            if tick >= 200 {
                arrived += module.get(copper);
            }
            module.remove(copper, 1_000_000);
        }
    }
    // `timer(timerAccept, 4)` fires every 5 ticks (the timer is set to the
    // interval and only decremented on later calls, `BuildingComp.timer`), so
    // the sustained rate is 12 items/s: 400 ticks / 5 = 80, +-10 %.
    assert!(
        (72..=88).contains(&arrived),
        "bridge moved {arrived} items in the 400-tick window (Java: 80)"
    );
}

/// H02 T15: `BeamNode.BeamNodeBuild.updateDirections` (`BeamNode.java:241-293`)
/// — a beam node links the first same-team powered, `connectedPower`,
/// non-`PowerNode` building in each direction within `range` and stops at
/// insulated blocks. `beam-node` range 10 (`Blocks.java:2671`);
/// `plastanium-wall` is insulated (`Blocks.java`).
#[test]
fn beam_node_links_battery_in_line() {
    let mut sim = runtime_sim();
    let node = place(&mut sim, "beam-node", 5, 5);
    let battery = place(&mut sim, "battery", 12, 5);
    let graph = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::world::modules::PowerModule>(e)
            .map(|module| module.graph)
    };
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        graph(&sim, node),
        graph(&sim, battery),
        "the beam node linked the battery 7 tiles in line"
    );

    // A power node is not a link target (`!(other.block instanceof PowerNode)`)
    // but it does not stop the scan: the beam still reaches the battery.
    let _power_node = place(&mut sim, "power-node", 8, 5);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        graph(&sim, node),
        graph(&sim, battery),
        "the power node is skipped, not treated as insulation"
    );

    // An insulated plastanium wall severs the line (`BeamNode.java:266`).
    let _wall = place(&mut sim, "plastanium-wall", 9, 5);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert_ne!(
        graph(&sim, node),
        graph(&sim, battery),
        "the insulated wall broke the beam link"
    );
}

/// I01 T7: `core-bastion`/`core-citadel`/`core-acropolis` set
/// `requiresCoreZone = true` (`Blocks.java:3198,3218,3236`), so the upgrade
/// branch of `CoreBlock.canPlaceOn` (`CoreBlock.java:202`) needs every footprint
/// floor to allow core placement.
#[test]
fn bastion_requires_core_zone() {
    let mut sim = runtime_sim();
    // API staging skips `canPlaceOn`; the player-form query below runs it.
    place_team(&mut sim, 10, 10, "core-shard", 0, 1);
    // Skip the item requirement so only the core-zone rule decides.
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .infinite_resources = true;
    // Player-form query (visible + core radius; no unit overlap).
    let flags = crate::world::build::PlaceFlags {
        check_visible: true,
        check_core_radius: true,
        check_units: false,
    };
    // Stone floor everywhere (no core zone): the upgrade is rejected.
    assert!(
        !place_query(&sim, 10, 10, "core-bastion", 0, 1, flags),
        "core-bastion needs the core zone under its footprint"
    );
    // A core-zone floor under the whole 4x4 footprint accepts it.
    let zone = BlockId::new(sim.world_block_id("core-zone").expect("core-zone"));
    for x in 9..13 {
        for y in 9..13 {
            sim.grid.tiles.get_mut(x, y).floor = zone;
        }
    }

    assert!(
        place_query(&sim, 10, 10, "core-bastion", 0, 1, flags),
        "core-zone floor satisfies requiresCoreZone"
    );
}

/// I04 T3: the Reconstructor refuses a locked upgrade target for a non-AI team
/// and arms `Trigger.cannotUpgrade` (`Reconstructor.java:252-257` ->
/// `HintsFragment.java:84`); once the target is in the live unlocked set the
/// payload is accepted again (`team.isAI() || upgrade.unlockedNowHost()`).
#[test]
fn locked_upgrade_blocks_and_fires_hint() {
    use crate::content::PlanetId;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::blocks::payloads::{PayloadRef, dispatch_accept_payload};

    let mut sim = runtime_sim();
    let campaign = {
        let content = sim.block_runtime().expect("runtime").content();
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.phase = crate::game::State::Playing;
    session.sector = Some((PlanetId::new(0), 170));
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let recon = place(&mut sim, "additive-reconstructor", 4, 4);
    let (dagger, mace_name) = {
        let content = sim.block_runtime().expect("runtime").content();
        let dagger = content.unit_id("dagger").expect("dagger");
        let mace = content.unit_id("mace").expect("mace");
        let name = content.unit(mace).expect("mace def").name.clone();
        (dagger, name)
    };
    let payload = PayloadRef {
        entity: None,
        content: dagger.raw(),
        is_block: false,
    };
    // The live unlocked set is empty: the dagger -> mace upgrade is locked.
    assert!(
        !dispatch_accept_payload(&sim.ecs.0, recon, recon, payload),
        "a locked upgrade target is refused for a non-AI team"
    );
    sim.tick().expect("tick");
    assert!(
        sim.hint_events.events.contains("cannotupgrade"),
        "the refusal fired Trigger.cannotUpgrade"
    );

    // Unlocking the target accepts it again.
    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign")
        .set_unlocked_content([mace_name]);
    assert!(
        dispatch_accept_payload(&sim.ecs.0, recon, recon, payload),
        "an unlocked upgrade target is accepted"
    );
}
