// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret and unit-weapon bullet regressions: Duo targeting, fire and coolant, spawn-unit missiles, bullet lifetimes and conveyor pass-over.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

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

/// I01 T9: the coolant consumer reads `liquids.current()`
/// (`ConsumeLiquidFilter.getConsumed`, `ConsumeLiquidFilter.java:57-71`), so a
/// module holding water and cryofluid consumes the cryofluid (the last added).
#[test]
fn coolant_uses_current_liquid() {
    use crate::world::blocks::defense::turrets::handle_liquid;
    let mut sim = live_sim(16);
    let duo = place(&mut sim, 5, 5, "duo", 1);
    let (water, cryo) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.liquid_id("water").expect("water"),
            content.liquid_id("cryofluid").expect("cryo"),
        )
    };
    handle_liquid(&mut sim.ecs.0, duo, water, 100.0);
    handle_liquid(&mut sim.ecs.0, duo, cryo, 100.0);
    let (water_before, cryo_before) = {
        let module = sim
            .ecs
            .0
            .get::<crate::world::modules::LiquidModule>(duo)
            .expect("module");
        assert_eq!(module.current_liquid(), cryo, "cryofluid is current");
        (module.get(water), module.get(cryo))
    };
    for _ in 0..240 {
        sim.tick().expect("tick");
    }
    let module = sim
        .ecs
        .0
        .get::<crate::world::modules::LiquidModule>(duo)
        .expect("module");
    assert_eq!(
        module.get(water),
        water_before,
        "the non-current liquid is untouched"
    );
    assert!(
        module.get(cryo) < cryo_before,
        "the current liquid is consumed"
    );
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

/// T2: ordinary bullets fly over enemy `underBullets` buildings they were not
/// aimed at (`BulletComp.checkUnderBuild`, BulletComp.java:212-225) — the
/// dagger's shots cross the conveyor and hit the wall behind it.
///
/// The decoy is needed to aim the shot: units take precedence over buildings in
/// `Units.closestTarget`, and a closer conveyor outranks a wall in building
/// priority (`UnitSorts.buildingDefault`), so a dagger with only the wall and
/// conveyor in range would target the conveyor itself.
#[test]
fn bullets_pass_over_enemy_conveyor() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 10, "copper-wall", 1);
    let conveyor = place(&mut sim, 15, 10, "conveyor", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let conveyor_start = health(&sim, conveyor).expect("conveyor health");
    // Immobile enemy unit behind the wall: the dagger's primary target.
    let decoy = sim
        .spawn_live_unit("dagger", 1, 25.0 * 8.0, 10.0 * 8.0, 180.0)
        .expect("decoy");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(decoy) {
        slot.kind = AiKind::Player;
    }
    let _ = sim
        .spawn_live_unit("dagger", 2, 10.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        health(&sim, conveyor),
        Some(conveyor_start),
        "conveyor untouched (not the aimed tile)"
    );
    assert!(
        health(&sim, wall).is_none_or(|hp| hp < wall_start),
        "wall damaged by the bullet that passed over the conveyor: {:?}",
        health(&sim, wall)
    );
}

/// T1/PAR-0088: a carbide-fed `scathe` fires the `spawnUnit` carrier
/// (`BulletType.create` spawns no bullet), the `scathe-missile` flies to the
/// target and its shoot-on-death explosion kills it (`Blocks.java:5251-5330`).
#[test]
fn scathe_launches_missile_and_kills() {
    use crate::entities::comp::unit::comp::UnitTypeComp;

    let mut sim = live_sim(64);
    let scathe = place(&mut sim, 10, 10, "scathe", 1);
    feed(&mut sim, scathe, "carbide", 10);
    let (tx, ty) = (10.0 * 8.0, 10.0 * 8.0);
    let dagger = sim
        .spawn_live_unit("dagger", 2, tx + 240.0, ty, 180.0)
        .expect("dagger");
    let missile_id = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("scathe-missile")
        .expect("scathe-missile");
    let mut saw_missile = false;
    for _ in 0..900 {
        sim.tick().expect("tick");
        if sim.unit_runtime().expect("rt").units.iter().any(|entity| {
            sim.ecs
                .0
                .get::<UnitTypeComp>(*entity)
                .is_some_and(|comp| comp.type_id == missile_id)
        }) {
            saw_missile = true;
        }
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    assert!(saw_missile, "scathe launched a scathe-missile");
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "dagger killed by the missile's death explosion"
    );
}

/// T2: a missile flies over an enemy `underBullets` conveyor it was not aimed
/// at (`MissileAI.java:33-36`: `build == target || !build.block.underBullets`).
///
/// The scathe targets the unit first (`Units.closestTarget` checks units before
/// buildings), so the missile's target is the dagger, not the conveyor on its
/// flight path.
#[test]
fn missile_ignores_conveyor_below() {
    let mut sim = live_sim(64);
    let scathe = place(&mut sim, 10, 10, "scathe", 1);
    feed(&mut sim, scathe, "carbide", 10);
    let conveyor = place(&mut sim, 15, 10, "conveyor", 2);
    let conveyor_start = health(&sim, conveyor).expect("conveyor health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 10.0 * 8.0 + 240.0, 10.0 * 8.0, 180.0)
        .expect("dagger");
    for _ in 0..900 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    assert_eq!(
        health(&sim, conveyor),
        Some(conveyor_start),
        "missile passed over the conveyor (not its target)"
    );
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "dagger killed after the missile crossed the conveyor"
    );
}
