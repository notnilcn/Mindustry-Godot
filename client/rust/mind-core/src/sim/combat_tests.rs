// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-sim combat regressions (turrets, bullet-vs-unit hits, unit/building
//! death) driven through the real `Sim` schedule with the block + unit
//! runtimes installed. Ported behaviors: `Turret.TurretBuild.updateTile`,
//! `BulletComp.collision`/`BulletType.hitEntity`, `UnitComp.update` death.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::determinism::SimCommand;
use crate::entities::comp::{Health, Pos};

use super::Sim;

pub(super) fn live_sim(size: i32) -> Sim {
    let mut sim = Sim::new(1, size, size, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim
}

pub(super) fn block(sim: &Sim, name: &str) -> u16 {
    sim.unit_runtime()
        .expect("runtime")
        .content
        .block_id(name)
        .expect("block")
        .get()
}

pub(super) fn place(sim: &mut Sim, x: i16, y: i16, name: &str, team: u8) -> Entity {
    let block = block(sim, name);
    sim.command(SimCommand::Place {
        x,
        y,
        block,
        rotation: 0,
        team,
        player: None,
    })
    .expect("place");
    sim.grid
        .tiles
        .get(x as i32, y as i32)
        .build
        .expect("placed building")
}

fn feed(sim: &mut Sim, turret: Entity, item: &str, count: usize) {
    let id = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .item_id(item)
        .expect("item");
    for _ in 0..count {
        crate::world::blocks::defense::turrets::handle_item(&mut sim.ecs.0, turret, id);
    }
}

fn health(sim: &Sim, e: Entity) -> Option<f32> {
    sim.ecs.0.get::<Health>(e).map(|h| h.health)
}

/// PAR-0086: a placed `duo` fed copper acquires an enemy dagger, rotates,
/// fires and kills it.
#[test]
fn placed_duo_kills_enemy_dagger() {
    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let (tx, ty) = (20.0 * 8.0, 20.0 * 8.0);
    let dagger = sim
        .spawn_live_unit("dagger", 2, tx + 60.0, ty + 20.0, 180.0)
        .expect("dagger");
    let start = health(&sim, dagger).expect("dagger health");
    let mut created = 0;
    for _ in 0..900 {
        sim.tick().expect("tick");
        created = sim.unit_runtime().expect("rt").bullets_created;
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    let state = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map(|s| (s.rotation, s.total_ammo));
    eprintln!(
        "duo state {:?} bullets {created} dagger {:?}/{start} pos {:?}",
        state,
        health(&sim, dagger),
        sim.ecs.0.get::<Pos>(dagger).copied().map(|p| (p.x, p.y))
    );
    assert!(state.is_some(), "duo has turret state");
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "dagger killed by the duo"
    );
}

fn shots_in(sim_setup: impl Fn(&mut Sim, Entity), ticks: usize) -> (i32, f32) {
    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim_setup(&mut sim, duo);
    let _ = sim
        .spawn_live_unit("fortress", 2, 20.0 * 8.0 + 90.0, 20.0 * 8.0, 180.0)
        .expect("fortress");
    let start = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map_or(0, |s| s.total_ammo);
    for _ in 0..ticks {
        sim.tick().expect("tick");
    }
    let end = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map_or(0, |s| s.total_ammo);
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .liquid_id("water")
        .expect("water");
    let left = sim
        .ecs
        .0
        .get::<crate::world::modules::LiquidModule>(duo)
        .map_or(0.0, |l| l.get(water));
    (start - end, left)
}

/// PAR-0087: a duo accepts water coolant (capacity 20) and fires faster
/// while consuming it; slag/oil are rejected by the `ConsumeCoolant` filter.
#[test]
fn duo_water_coolant_speeds_fire_and_drains() {
    use crate::world::blocks::defense::turrets::{accept_liquid, handle_liquid};
    let mut sim = live_sim(16);
    let duo = place(&mut sim, 5, 5, "duo", 1);
    let content = &sim.unit_runtime().expect("rt").content;
    let (water, slag, oil, cryo) = (
        content.liquid_id("water").expect("water"),
        content.liquid_id("slag").expect("slag"),
        content.liquid_id("oil").expect("oil"),
        content.liquid_id("cryofluid").expect("cryo"),
    );
    assert!(accept_liquid(&sim.ecs.0, duo, water), "water accepted");
    assert!(accept_liquid(&sim.ecs.0, duo, cryo), "cryofluid accepted");
    assert!(!accept_liquid(&sim.ecs.0, duo, slag), "slag rejected (hot)");
    assert!(
        !accept_liquid(&sim.ecs.0, duo, oil),
        "oil rejected (flammable)"
    );
    handle_liquid(&mut sim.ecs.0, duo, water, 100.0);
    let stored = sim
        .ecs
        .0
        .get::<crate::world::modules::LiquidModule>(duo)
        .map_or(0.0, |l| l.get(water));
    assert_eq!(stored, 20.0, "clamped to duo liquidCapacity 20");

    let (dry, _) = shots_in(|_, _| {}, 240);
    let (wet, left) = shots_in(
        |sim, duo| {
            let water = sim
                .unit_runtime()
                .expect("rt")
                .content
                .liquid_id("water")
                .expect("water");
            handle_liquid(&mut sim.ecs.0, duo, water, 20.0);
        },
        240,
    );
    assert!(wet > dry, "coolant sped up fire: wet {wet} > dry {dry}");
    assert!(left < 20.0, "coolant drained ({left} left)");
}

/// Finds the first weapon bullet on `unit` whose def sets `spawnUnit`
/// (`BulletType.spawnUnit`), returning `(bullet, unit)` ids.
fn spawn_unit_weapon(sim: &Sim, unit: &str) -> Option<(u16, crate::content::UnitTypeId)> {
    let content = &sim.unit_runtime().expect("rt").content;
    let id = content.unit_id(unit)?;
    let def = content.unit(id)?;
    def.weapons
        .iter()
        .map(|weapon| weapon.bullet.id)
        .find_map(|bullet| {
            let def = content.bullet(bullet)?;
            def.spawn_unit.map(|unit| (bullet.get(), unit))
        })
}

/// PAR-0088: a `spawnUnit` bullet spawns the unit instead of a bullet
/// (`BulletType.create` returns `null`), with launch velocity and the
/// `MissileShooter` binding.
#[test]
fn spawn_unit_bullet_spawns_missile() {
    use crate::combat::bullet::spawn::{BulletSpawn, SpawnedByBullet, create as create_bullet};
    use crate::entities::comp::unit::comp::UnitTypeComp;

    let mut sim = live_sim(64);
    let (bullet, missile_id) =
        spawn_unit_weapon(&sim, "anthicus").expect("anthicus has a spawnUnit weapon");
    let content = &sim.unit_runtime().expect("rt").content;
    assert_eq!(
        content.unit(missile_id).map(|def| def.name.as_str()),
        Some("anthicus-missile")
    );
    // A tracked unit makes `unit_update_system` run its adopt pass.
    let _ = sim
        .spawn_live_unit("dagger", 1, 400.0, 400.0, 0.0)
        .expect("dagger");

    let world = &mut sim.ecs.0;
    let mut runtime = world
        .remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .expect("runtime");
    let spawn = BulletSpawn {
        def: crate::content::BulletId::new(bullet),
        team: 2,
        x: 100.0,
        y: 100.0,
        angle: 0.0,
        ..BulletSpawn::default()
    };
    let seq = runtime.seq;
    runtime.seq = seq.wrapping_add(1);
    let created = create_bullet(world, &runtime.content, &mut runtime.rng, seq, &spawn);
    assert!(created.is_none(), "spawnUnit bullets return no bullet");
    let missile = world
        .iter_entities()
        .filter(|entity| {
            entity
                .get::<UnitTypeComp>()
                .is_some_and(|comp| comp.type_id == missile_id)
        })
        .map(|entity| entity.id())
        .next()
        .expect("missile unit spawned");
    assert!(
        world.get::<SpawnedByBullet>(missile).is_some(),
        "runtime can adopt the unit"
    );
    let vel = world
        .get::<crate::entities::comp::Vel>(missile)
        .expect("vel");
    assert!(
        vel.x > 0.0 && vel.y.abs() < 0.001,
        "launched along +x: {vel:?}"
    );
    world.insert_non_send(runtime);

    // The live runtime adopts it after a pass (`adopt_spawned_units`).
    sim.tick().expect("tick");
    assert!(
        sim.unit_runtime().expect("rt").units.contains(&missile),
        "missile joined the live unit list"
    );
}

/// PAR-0088: the missile dies at its `TimedKill` lifetime.
#[test]
fn timed_kill_counts_up() {
    use crate::entities::comp::unit::comp::UnitTypeComp;
    let mut sim = live_sim(32);
    let (missile_id, lifetime) = {
        let content = &sim.unit_runtime().expect("rt").content;
        let id = content.unit_id("anthicus-missile").expect("id");
        let lifetime = content.unit(id).expect("def").lifetime;
        (id, lifetime)
    };
    let missile = sim
        .spawn_live_unit("anthicus-missile", 1, 100.0, 100.0, 0.0)
        .expect("missile");
    for _ in 0..(lifetime as usize / 2) {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs
            .0
            .get::<UnitTypeComp>(missile)
            .is_some_and(|comp| comp.type_id == missile_id),
        "still flying mid-lifetime"
    );
    for _ in 0..(lifetime as usize / 2 + 5) {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs.0.get_entity(missile).is_err(),
        "removed at lifetime {lifetime}"
    );
}

/// PAR-0088: an anthicus missile contacts a team-1 copper wall and its
/// shoot-on-death explosion damages it; direct missiles finish it off
/// (`resolve_deaths` -> `run_shoot_on_death` -> explosion bullet).
#[test]
fn missile_dies_at_lifetime_and_explodes() {
    use crate::entities::comp::unit::comp::UnitTypeComp;
    let mut sim = live_sim(64);
    let missile_id = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("anthicus-missile")
        .expect("anthicus-missile");
    let wall = place(&mut sim, 20, 12, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let _ = sim
        .spawn_live_unit("anthicus", 2, 100.0, 100.0, 0.0)
        .expect("anthicus");
    let mut saw_missile = false;
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.unit_runtime().expect("rt").units.iter().any(|entity| {
            sim.ecs
                .0
                .get::<UnitTypeComp>(*entity)
                .is_some_and(|comp| comp.type_id == missile_id)
        }) {
            saw_missile = true;
        }
        if sim.grid.tiles.get(20, 12).build.is_none() {
            break;
        }
    }
    let wall_mid = health(&sim, wall);
    assert!(saw_missile, "anthicus fired at least one missile");
    assert!(
        wall_mid.is_none_or(|hp| hp < wall_start),
        "shoot-on-death explosion damaged the wall: {wall_mid:?}"
    );

    // Direct missiles from (100, 99) fly +x into the wall and die on contact;
    // their explosions finish the wall (`BulletType.create` spawnUnit branch,
    // MissileAI contact death, `UnitComp.destroy` shootOnDeath).
    let mut launched = 0;
    while sim.grid.tiles.get(20, 12).build.is_some() && launched < 5 {
        launched += 1;
        let _ = sim
            .spawn_live_unit("anthicus-missile", 2, 100.0, 99.0, 0.0)
            .expect("missile");
        for _ in 0..200 {
            sim.tick().expect("tick");
            if sim.grid.tiles.get(20, 12).build.is_none() {
                break;
            }
        }
    }
    assert!(
        sim.grid.tiles.get(20, 12).build.is_none(),
        "wall destroyed after {launched} direct missile(s)"
    );
}
