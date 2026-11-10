// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime generator, turret and door regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

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
    let mut content = crate::sim::runtime::load_vanilla_content().expect("content");
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
