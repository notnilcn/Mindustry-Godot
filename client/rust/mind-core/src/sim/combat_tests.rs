// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-sim combat regressions (turrets, bullet-vs-unit hits, unit/building
//! death) driven through the real `Sim` schedule with the block + unit
//! runtimes installed. Ported behaviors: `Turret.TurretBuild.updateTile`,
//! `BulletComp.collision`/`BulletType.hitEntity`, `UnitComp.update` death.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::determinism::SimCommand;
use crate::entities::comp::{Health, Pos, Vel};

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
        .spawn_live_unit("dagger", 1, 25.0 * 8.0, 10.0 * 8.0 + 4.0, 180.0)
        .expect("decoy");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(decoy) {
        slot.kind = AiKind::Player;
    }
    let _ = sim
        .spawn_live_unit("dagger", 2, 10.0 * 8.0, 10.0 * 8.0 + 4.0, 0.0)
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

/// PAR-0137: `canBoost` mechs lift off while boosting
/// (`UnitComp.updateBoosting`, UnitComp.java:121-130): elevation reaches 1,
/// `unit_speed` gains the boost multiplier and the lifted unit counts as
/// flying (ground-only bullets then skip it, `BulletComp.collision`).
#[test]
fn boosting_nova_lifts_off() {
    use crate::entities::comp::unit::comp::UnitCore;

    let mut sim = live_sim(48);
    let (rise_speed, speed, boost_multiplier) = {
        let content = &sim.unit_runtime().expect("rt").content;
        let id = content.unit_id("nova").expect("nova");
        let def = content.unit(id).expect("nova def");
        (def.rise_speed, def.speed, def.boost_multiplier)
    };
    let nova = sim
        .spawn_live_unit("nova", 1, 100.0, 100.0, 0.0)
        .expect("nova");
    if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(nova) {
        core.boosting = true;
    }
    let ticks = (1.0 / rise_speed).ceil() as usize + 2;
    for _ in 0..ticks {
        sim.tick().expect("tick");
        if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(nova) {
            core.boosting = true;
        }
    }
    let elevation = sim
        .ecs
        .0
        .get::<UnitCore>(nova)
        .map(|core| core.elevation)
        .expect("nova core");
    assert!(
        elevation >= 1.0,
        "elevation reached 1 after {ticks} ticks ({elevation})"
    );
    assert!(speed > 0.0 && boost_multiplier > 1.0);
    let speed_now = {
        let runtime = sim.unit_runtime().expect("rt");
        crate::entities::comp::unit::movement::unit_speed(
            &sim.ecs.0,
            &runtime.grid,
            &runtime.content,
            nova,
        )
    };
    assert!(
        (speed_now - speed * boost_multiplier).abs() < 0.01,
        "boosted speed {speed_now} ~= {speed} * {boost_multiplier}"
    );
    assert!(
        crate::combat::bullet::unit_is_flying(&sim.ecs.0, nova),
        "lifted nova counts as flying"
    );
}

/// T7: a team-2 flare keeps circling its building target and keeps firing
/// (`FlyingAI.updateMovement` -> `AIController.circleAttack`, FlyingAI.java:15,
/// AIController.java:318-351) instead of parking at the anchor tile.
///
/// Note: the flare's 9 damage / 80-tick reload cannot destroy a 320 hp copper
/// wall inside 1200 ticks (135 damage), so the assertions are continuous
/// circling, a growing shot count and wall damage.
#[test]
fn flare_circles_and_keeps_firing() {
    let mut sim = live_sim(64);
    let wall = place(&mut sim, 30, 10, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let flare = sim
        .spawn_live_unit("flare", 2, 6.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("flare");
    let shots = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(flare)
            .map(|weapons| weapons.mounts.iter().map(|m| m.total_shots).sum::<i32>())
            .unwrap_or(0)
    };
    let mut shots_mid = 0;
    let mut moving_ticks = 0;
    let mut sampled = 0;
    for tick in 0..1200 {
        sim.tick().expect("tick");
        let vel = sim
            .ecs
            .0
            .get::<Vel>(flare)
            .map(|vel| (vel.x * vel.x + vel.y * vel.y).sqrt())
            .unwrap_or(0.0);
        if tick > 120 {
            sampled += 1;
            if vel > 0.01 {
                moving_ticks += 1;
            }
        }
        if tick == 300 {
            shots_mid = shots(&sim);
        }
    }
    let shots_end = shots(&sim);
    assert!(
        sampled > 0 && moving_ticks * 10 >= sampled * 9,
        "flare keeps circling: {moving_ticks}/{sampled} sampled ticks with speed"
    );
    assert!(
        shots_end > shots_mid,
        "flare keeps firing after tick 300 ({shots_mid} -> {shots_end})"
    );
    assert!(
        health(&sim, wall).is_none_or(|hp| hp < wall_start),
        "wall damaged while the flare circles: {:?}",
        health(&sim, wall)
    );
}

/// T8: `PhysicsProcess` pushes overlapping same-layer unit bodies apart
/// (`PhysicsProcess.java:188-247`, body radius `hitSize * 0.6`); different
/// collision layers never interact.
#[test]
fn stacked_daggers_separate() {
    let mut sim = live_sim(32);
    let a = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger a");
    let b = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger b");
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let pa = sim.ecs.0.get::<Pos>(a).copied().expect("pos a");
    let pb = sim.ecs.0.get::<Pos>(b).copied().expect("pos b");
    let dist = ((pa.x - pb.x).powi(2) + (pa.y - pb.y).powi(2)).sqrt();
    assert!(
        dist >= 8.6,
        "stacked daggers separated to {dist} px (body sum = 9.6)"
    );

    // A flying flare and a grounded dagger share no layer: no push.
    let flare = sim
        .spawn_live_unit("flare", 1, 200.0, 200.0, 0.0)
        .expect("flare");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 200.0, 200.0, 0.0)
        .expect("dagger c");
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let pf = sim.ecs.0.get::<Pos>(flare).copied().expect("pos flare");
    let pd = sim.ecs.0.get::<Pos>(dagger).copied().expect("pos dagger");
    let layer_dist = ((pf.x - pd.x).powi(2) + (pf.y - pd.y).powi(2)).sqrt();
    assert!(
        layer_dist < 1.0,
        "different layers do not push each other ({layer_dist} px)"
    );
}

/// PAR-0138: `[` with no explicit target picks the closest unit around the
/// *carrier* (`Units.closest(team, pay.x(), pay.y(), hitSize * 2f, ...)`,
/// InputHandler.java:1043), not around the cursor.
#[test]
fn mega_pickup_without_target_uses_carrier_position() {
    use crate::entities::comp::unit::comp::UnitPayloads;

    let mut sim = live_sim(48);
    let mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    let _ = sim
        .spawn_live_unit("dagger", 1, 110.0, 100.0, 0.0)
        .expect("dagger");
    assert!(
        sim.apply_payload_command(0, 400.0, 400.0, None),
        "the cursor is far away; the carrier-adjacent dagger is picked"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitPayloads>(mega)
            .map(|payloads| payloads.stack.len()),
        Some(1),
        "one carried unit"
    );
}

/// PAR-0138: with no unit in reach, `[` picks the building under the carrier
/// (`world.buildWorld(pay.x(), pay.y())`, InputHandler.java:1045).
#[test]
fn mega_pickup_falls_back_to_building_under_carrier() {
    use crate::entities::comp::unit::comp::UnitPayloads;

    let mut sim = live_sim(48);
    let mega = sim
        .spawn_player_unit("mega", 1, 96.0, 96.0, 0.0)
        .expect("mega");
    // Tile (12,12) spans x 96..104, y 96..104: the carrier stands on it
    // ((96,96) maps to tile 12 under both `World.tileWorld` truncation and the
    // port's rounding `WorldGrid::to_tile`).
    let _ = place(&mut sim, 12, 12, "copper-wall", 1);
    assert!(
        sim.apply_payload_command(0, 400.0, 400.0, None),
        "no unit in reach; the wall under the carrier is picked"
    );
    assert!(
        sim.grid.tiles.get(12, 12).build.is_none(),
        "the wall left the grid"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitPayloads>(mega)
            .map(|payloads| payloads.stack.len()),
        Some(1),
        "one carried building"
    );
}

/// T15: `BulletType.lifesteal` heals the bullet owner by
/// `min(target_health, damage) * lifesteal` before damage is applied
/// (`BulletType.java:502-505`).
#[test]
fn lifesteal_bullet_heals_owner() {
    use crate::content::BulletKind;
    use crate::content::registries::bullets::BulletDef;

    let mut sim = live_sim(32);
    let owner = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("owner");
    let target = sim
        .spawn_live_unit("dagger", 2, 120.0, 100.0, 0.0)
        .expect("target");
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(owner) {
        health.health = 50.0;
    }
    {
        let world = &mut sim.ecs.0;
        let mut runtime = world
            .remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            .expect("runtime");
        let mut def = BulletDef::new(BulletKind::Basic);
        def.speed = 0.0;
        def.lifetime = 100.0;
        def.damage = 40.0;
        def.lifesteal = 0.5;
        let id = runtime.content.add_bullet(def).expect("add bullet");
        let audio = crate::audio::AudioSinkRes::noop();
        let mut spawned = Vec::new();
        {
            let crate::sim::unit_runtime::UnitRuntime {
                content,
                grid,
                rng,
                fx,
                seq,
                ..
            } = &mut runtime;
            let mut ctx = crate::combat::bullet::CombatCtx {
                world,
                content,
                grid,
                rng,
                fx: fx.as_ref(),
                audio: &audio,
                seq,
                spawned: &mut spawned,
            };
            let spawn = crate::combat::BulletSpawn {
                def: id,
                owner: Some(owner),
                x: 100.0,
                y: 100.0,
                ..crate::combat::BulletSpawn::default()
            };
            let entity = ctx.spawn(&spawn).expect("bullet");
            crate::combat::bullet::hit_entity(&mut ctx, entity, target, 150.0);
        }
        world.insert_non_send(runtime);
    }
    let owner_health = health(&sim, owner).expect("owner health");
    assert!(
        (owner_health - 70.0).abs() < 0.5,
        "owner healed by 40 * 0.5 = 20 (50 -> {owner_health})"
    );
}

/// T17: a dying flying unit crashes with area damage around its position
/// (`UnitComp.destroy` -> `Damage.damage(team, x, y, hitSize^0.94 * 1.25,
/// hitSize^0.75 * crashDamageMultiplier * 2.5 * rules.unitCrashDamage(team))`,
/// UnitComp.java:921-928).
#[test]
fn crashing_flare_damages_neighbours() {
    let mut sim = live_sim(32);
    let (wx, wy) = (15, 12);
    let wall = place(&mut sim, wx, wy, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let flare = sim
        .spawn_live_unit(
            "flare",
            2,
            wx as f32 * 8.0 + 4.0,
            wy as f32 * 8.0 + 4.0,
            0.0,
        )
        .expect("flare");
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(flare) {
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(flare).is_err(),
        "the crashed flare was removed"
    );
    assert!(
        health(&sim, wall).is_none_or(|hp| hp < wall_start),
        "the crash damaged the neighbouring wall: {:?}",
        health(&sim, wall)
    );
}

/// T16: destroying a rebuildable block pushes a ghost plan onto its team's
/// queue (`Logic`'s `BlockDestroyEvent` -> `BuildingComp.addPlan(true)`,
/// Logic.java:42-50).
#[test]
fn destroyed_building_leaves_rebuild_plan() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(32);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.phase = crate::game::State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let wall_block = block(&sim, "copper-wall");
    let wall = place(&mut sim, 10, 10, "copper-wall", 1);
    sim.ecs
        .0
        .get_mut::<Health>(wall)
        .expect("wall health")
        .health = 0.0;
    sim.tick().expect("tick");

    let campaign = sim
        .ecs
        .0
        .get_resource::<CampaignRuntime>()
        .expect("campaign");
    let plans = &campaign
        .session
        .teams
        .get_or_null(crate::ecs::TeamId(1))
        .expect("team 1")
        .plans;
    assert!(
        plans
            .front()
            .is_some_and(|plan| plan.block.raw() == wall_block && plan.x == 10 && plan.y == 10),
        "ghost plan queued for the dead wall: {plans:?}"
    );
}

/// T5: a destroyed container full of blast compound explodes with wave damage
/// (`BuildingComp.onDestroyed` -> `Damage.dynamicExplosion`,
/// BuildingComp.java:1490-1518).
#[test]
fn destroyed_blast_container_damages_neighbours() {
    let mut sim = live_sim(48);
    let container = place(&mut sim, 10, 10, "container", 1);
    let wall = place(&mut sim, 12, 10, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    {
        let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(container)
        else {
            return; // no item module fixture
        };
        module.items[blast.index()] = 300;
        module.total = 300;
    }
    sim.ecs
        .0
        .get_mut::<Health>(container)
        .expect("container health")
        .health = 0.0;
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(container).is_err(),
        "the container was destroyed"
    );
    assert!(
        health(&sim, wall).is_none_or(|hp| hp < wall_start),
        "the explosion damaged the wall 2 tiles away: {:?}",
        health(&sim, wall)
    );
}

/// T5: a destroyed container full of flammable items leaves a fire
/// (`fire = true` on the building-destroy explosion path).
#[test]
fn destroyed_coal_container_starts_fire() {
    let mut sim = live_sim(48);
    let container = place(&mut sim, 10, 10, "container", 1);
    let coal = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("coal")
        .expect("coal");
    {
        let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(container)
        else {
            return;
        };
        module.items[coal.index()] = 300;
        module.total = 300;
    }
    sim.ecs
        .0
        .get_mut::<Health>(container)
        .expect("container health")
        .health = 0.0;
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(container).is_err(),
        "the container was destroyed"
    );
    let fire = (10..=11)
        .any(|tx| (10..=11).any(|ty| crate::combat::fires::find_at(&sim.ecs.0, tx, ty).is_some()));
    assert!(
        fire,
        "the coal explosion left a fire at the container tile (`World.toTile` of the block centre (84,84) rounds to (11,11) like Java's `Math.round`)"
    );
}

/// PAR-0140: every registered command id is accepted for a commandable unit
/// and stored on it (`CommandAI.command`, CommandAI.java:57-64); move orders
/// keep the tile path.
#[test]
fn live_unit_command_switch_is_stored() {
    use crate::ai::controller::{AiKind, ControllerSlot, UnitCommandComp};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let unit = sim
        .spawn_live_unit("poly", 1, 100.0, 100.0, 0.0)
        .expect("poly");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    let id = sim.ecs.0.get::<crate::ecs::EntitySeq>(unit).expect("seq").0 as i32;
    let (repair_id, poly_allows) = {
        let runtime = sim.unit_runtime().expect("rt");
        let repair = runtime
            .content
            .unit_command_by_name("repair")
            .expect("repair command");
        let allows = runtime
            .content
            .unit_id("poly")
            .and_then(|id| runtime.content.unit(id))
            .is_some_and(|def| crate::ai::unit_command_runtime::allows_command(def, repair.id));
        (repair.id.raw(), allows)
    };
    assert!(poly_allows, "poly lists the repair command");
    assert_eq!(
        sim.apply_live_unit_command(&[id], repair_id, 0.0, 0.0),
        1,
        "the repair command is accepted"
    );
    assert_eq!(
        sim.ecs.0.get::<UnitCommandComp>(unit).copied(),
        Some(UnitCommandComp(repair_id)),
        "the active command is stored on the unit"
    );
    // A move order keeps the ordered tile (and clears nothing else).
    assert_eq!(
        sim.apply_live_unit_command(&[id], crate::sim::command::MOVE_COMMAND.raw(), 200.0, 100.0),
        1,
        "move order accepted"
    );
    assert!(
        sim.ecs
            .0
            .get::<ControllerSlot>(unit)
            .and_then(|slot| slot.target)
            .is_some(),
        "move target set"
    );
}

/// T12: the live AI aims with the `Predict.intercept` lead
/// (`AIController.updateWeapons`, AIController.java:231): a slow shooter
/// connects with a target moving perpendicular at full speed.
#[test]
fn mace_leads_a_moving_dagger() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::TilePos;

    let mut sim = live_sim(64);
    let shooter = sim
        .spawn_live_unit("mace", 1, 100.0, 100.0, 0.0)
        .expect("mace");
    let target = sim
        .spawn_live_unit("dagger", 2, 140.0, 100.0, 0.0)
        .expect("target");
    // Walk the target straight down (-y) at full speed.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(target) {
        slot.kind = AiKind::Command;
        slot.target = Some(TilePos::new(17, 20));
    }
    let target_start = health(&sim, target).expect("target health");
    let mut killed = false;
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(target).is_err() {
            killed = true;
            break;
        }
    }
    let target_end = health(&sim, target);
    // The mace (74 damage) kills a 150 hp dagger in 3 connected shots; a
    // no-lead aim hits nothing once the target has moved a bullet-flight away.
    assert!(
        killed || target_end.is_some_and(|hp| hp <= target_start - 3.0 * 74.0),
        "the lead lets the mace connect (start {target_start}, end {target_end:?}, shooter {:?})",
        health(&sim, shooter)
    );
}

/// T14: the live dispatch runs `SuicideAI` for crawlers (`AiKind::Suicide`):
/// the crawler rams a target and its death weapon fires on contact
/// (`ai/types/suicide.rs`, `UnitTypes.crawler`).
#[test]
fn crawler_rams_and_explodes() {
    let mut sim = live_sim(48);
    let target = sim
        .spawn_live_unit("dagger", 1, 150.0, 100.0, 0.0)
        .expect("target dagger");
    let target_start = health(&sim, target).expect("target health");
    let crawler = sim
        .spawn_live_unit("crawler", 2, 100.0, 100.0, 0.0)
        .expect("crawler");
    let mut ticks = 0;
    while sim.ecs.0.get_entity(crawler).is_ok() && ticks < 400 {
        sim.tick().expect("tick");
        ticks += 1;
    }
    assert!(
        sim.ecs.0.get_entity(crawler).is_err(),
        "the crawler rammed the target and died ({ticks} ticks); pos {:?} weapons {:?}",
        sim.ecs.0.get::<Pos>(crawler).map(|p| (p.x, p.y)),
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(crawler)
            .map(|w| w
                .mounts
                .iter()
                .map(|m| (m.total_shots, m.shoot, m.target.is_some()))
                .collect::<Vec<_>>())
    );
    assert!(
        health(&sim, target).is_none_or(|hp| hp < target_start),
        "the crawler's death explosion damaged the target: {:?}",
        health(&sim, target)
    );
}

/// T14: a commanded builder runs `BuilderAI` (`AiKind::Builder`): it walks to
/// the assigned tile and repairs the building there (`ai/types/builder.rs`).
#[test]
fn commanded_builder_repairs_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::TilePos;

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 16, 12, "copper-wall", 1);
    sim.ecs
        .0
        .get_mut::<Health>(wall)
        .expect("wall health")
        .health = 100.0;
    let builder = sim
        .spawn_live_unit("alpha", 1, 100.0, 100.0, 0.0)
        .expect("alpha");
    // `alpha`'s content controller is `BuilderOrCommand` -> `AiKind::Builder`
    // for spawned units; assign the repair tile.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(builder) {
        slot.kind = AiKind::Builder;
        slot.target = Some(TilePos::new(16, 12));
    }
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    assert!(
        health(&sim, wall).is_some_and(|hp| hp > 100.0),
        "the builder repaired the wall: {:?}",
        health(&sim, wall)
    );
    let pos = sim.ecs.0.get::<Pos>(builder).copied().expect("builder pos");
    let dist = ((pos.x - 132.0).powi(2) + (pos.y - 100.0).powi(2)).sqrt();
    assert!(dist < 40.0, "the builder closed on the target ({dist} px)");
}

/// T14: the `Boost` kind enables `UnitCore.boosting`, which lifts a mech
/// (`BoostAI.defaultBehavior` + `UnitComp.updateBoosting`).
#[test]
fn boost_kind_lifts_mech() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::unit::comp::UnitCore;

    let mut sim = live_sim(48);
    let nova = sim
        .spawn_live_unit("nova", 1, 100.0, 100.0, 0.0)
        .expect("nova");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(nova) {
        slot.kind = AiKind::Boost;
    }
    for _ in 0..20 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs
            .0
            .get::<UnitCore>(nova)
            .is_some_and(|core| core.elevation >= 1.0),
        "the boost kind lifted the mech: {:?}",
        sim.ecs.0.get::<UnitCore>(nova).map(|core| core.elevation)
    );
}

/// NEW-1: a status applied through the live applier ticks damage every tick
/// (`StatusComp.update` in the live unit loop).
#[test]
fn live_status_burning_damages_unit() {
    use crate::combat::damage::status::apply_status;
    let mut sim = live_sim(32);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let burning = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("burning")
        .expect("burning");
    let start = health(&sim, dagger).expect("health");
    apply_status(&mut sim.ecs.0, dagger, burning, 300.0);
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let end = health(&sim, dagger).expect("alive");
    assert!(end < start, "burning ticked live damage ({start} -> {end})");
}

/// NEW-1: a live `RepairFieldAbility` (`poly`) heals a damaged ally through
/// the live ability update in the unit loop.
#[test]
fn live_ability_repair_field_heals() {
    let mut sim = live_sim(48);
    let _poly = sim
        .spawn_live_unit("poly", 1, 200.0, 200.0, 0.0)
        .expect("poly");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 216.0, 200.0, 0.0)
        .expect("dagger");
    sim.ecs
        .0
        .get_mut::<Health>(dagger)
        .expect("dagger health")
        .health = 50.0;
    // Poly's `RepairFieldAbility` reload is 480 ticks (`standard.rs` poly spec).
    for _ in 0..520 {
        sim.tick().expect("tick");
    }
    let end = health(&sim, dagger).expect("alive");
    assert!(end > 50.0, "repair field healed live (50 -> {end})");
}

/// PAR-0137: `UnitComp.speed()` — a ground unit on `deep-water`
/// (`speedMultiplier 0.2`) moves at most 0.25x its stone speed; a flying unit
/// ignores the floor entirely.
#[test]
fn dagger_slowed_on_deep_water() {
    let probe = live_sim(1);
    let content = &probe.unit_runtime().expect("rt").content;
    let stone = content.block_id("stone").expect("stone").get();
    let deep = content.block_id("deep-water").expect("deep-water").get();
    let air = BlockId::AIR.get();
    drop(probe);

    let displacement = |name: &str, floor: u16| -> f32 {
        let mut sim = Sim::new(1, 40, 40, BlockId::new(floor), BlockId::new(air));
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let unit = sim
            .spawn_live_unit(name, 2, 100.0, 100.0, 0.0)
            .expect("unit");
        crate::entities::comp::unit::lifecycle::set_move_target(
            &mut sim.ecs.0,
            unit,
            crate::world::TilePos::new(36, 12),
        );
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        let pos = sim
            .ecs
            .0
            .get::<Pos>(unit)
            .map(|pos| (pos.x, pos.y))
            .expect("pos");
        ((pos.0 - 100.0).powi(2) + (pos.1 - 100.0).powi(2)).sqrt()
    };

    let dagger_stone = displacement("dagger", stone);
    let dagger_water = displacement("dagger", deep);
    let flare_stone = displacement("flare", stone);
    let flare_water = displacement("flare", deep);
    eprintln!(
        "dagger stone {dagger_stone} water {dagger_water}; flare stone {flare_stone} water {flare_water}"
    );
    assert!(
        dagger_water < 0.25 * dagger_stone,
        "deep water slows the dagger ({dagger_water} vs {dagger_stone})"
    );
    assert!(
        (flare_water - flare_stone).abs() < 0.001,
        "flying units ignore the floor ({flare_water} vs {flare_stone})"
    );
}

/// PAR-0140: a MOVE order issued through `SimCommand::UnitCommand` moves a
/// campaign player-team dagger to the world point (the core half of the
/// command-mode -> selection -> order chain).
#[test]
fn commanded_dagger_reaches_target() {
    use crate::ecs::TeamId;
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::{BlockTable, TilePos};

    let mut sim = live_sim(48);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        waves: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;

    // Materialize a real player core so `checkGameState` keeps the session
    // playing while the order runs (`campaign_wave_timer_spawns_live_units`).
    let table: BlockTable = sim
        .ecs
        .0
        .get_resource::<BlockTable>()
        .expect("table")
        .clone();
    let core_block = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("core-shard")
        .expect("core-shard");
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(&mut sim.ecs.0, 10, TilePos::new(12, 32), 1, 0, 0, 0);
    sim.ecs.0.entity_mut(core).insert(crate::ecs::BuildingComp {
        pos: TilePos::new(12, 32),
        block: core_block,
        team: TeamId(1),
        rot: 0,
    });
    session.teams.register_core(core, TeamId(1), &session.rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    assert_eq!(
        crate::entities::comp::unit::lifecycle::ai_kind_of(&sim.ecs.0, dagger),
        Some(crate::ai::controller::AiKind::Command),
        "player-team campaign unit is commanded"
    );
    let id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 260.0,
        y: 100.0,
    })
    .expect("move order");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let pos = sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let dist = ((pos.x - 260.0).powi(2) + (pos.y - 100.0).powi(2)).sqrt();
    assert!(
        dist < 16.0,
        "commanded dagger reached the target ({dist} px)"
    );
}

/// G08 T15: a unit counts as having a command only with a live order target
/// (`CommandAI.hasCommand()` = `targetPos != null`), and
/// `set_command_mode_satisfied` flips the objective resource (G04 T4).
#[test]
fn any_unit_has_command_needs_target() {
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::{CampaignRuntime, ObjectiveInputState};
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        waves: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(
        !sim.any_unit_has_command(&[id]),
        "no order yet -> no command"
    );
    assert!(!sim.any_unit_has_command(&[]), "empty selection");
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 200.0,
        y: 100.0,
    })
    .expect("move order");
    assert!(
        sim.any_unit_has_command(&[id]),
        "move order sets the target"
    );

    sim.set_command_mode_satisfied(true);
    let state = sim
        .ecs
        .0
        .get_resource::<ObjectiveInputState>()
        .expect("input state");
    assert!(state.command_mode_satisfied);
}

/// PAR-0091: a deposited water puddle applies `wet` to a unit standing in it
/// and evaporates to nothing on a dry floor (live `update_puddles`).
#[test]
fn live_puddle_evaporates() {
    use crate::combat::puddles;
    let mut sim = live_sim(16);
    let (water, wet) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.liquid_id("water").expect("water"),
            content.status_id("wet").expect("wet"),
        )
    };
    {
        let world = &mut sim.ecs.0;
        let mut runtime = world
            .remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            .expect("runtime");
        let entity = puddles::deposit(
            world,
            &sim.grid.tiles,
            &runtime.content,
            5,
            5,
            water,
            50.0,
            &mut runtime.rng,
        );
        assert!(entity.is_some(), "water deposited");
        world.insert_non_send(runtime);
    }
    let dagger = sim
        .spawn_live_unit("dagger", 1, 42.0, 42.0, 0.0)
        .expect("dagger");
    for _ in 0..40 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::StatusComp>(dagger)
            .is_some_and(|status| status.has_effect_of(wet)),
        "the puddle applied `wet` to the unit standing in it"
    );
    for _ in 0..2000 {
        sim.tick().expect("tick");
    }
    assert!(
        puddles::find_at(&sim.ecs.0, 5, 5).is_none(),
        "the water puddle evaporated"
    );
}

/// PAR-0091: a fire damages the building on its tile and burns out live
/// (`update_fires` in the tick).
#[test]
fn live_fire_burns_building() {
    let mut sim = live_sim(16);
    let wall = place(&mut sim, 5, 5, "copper-wall", 1);
    let start = health(&sim, wall).expect("wall health");
    let mut rng = crate::determinism::SimRng::new(3);
    assert!(
        crate::combat::fires::create(&mut sim.ecs.0, 5, 5, &mut rng).is_some(),
        "fire created"
    );
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let end = health(&sim, wall);
    assert!(
        end.is_none_or(|hp| hp < start),
        "the fire damaged the wall ({start} -> {end:?})"
    );
}

/// PAR-0091: an `incendChance` bullet hit creates a fire at the hit tile
/// (`BulletType.createIncend` via `hit_bullet`).
#[test]
fn incend_bullet_creates_fire() {
    use crate::content::BulletKind;
    use crate::content::registries::bullets::BulletDef;
    let mut sim = live_sim(16);
    {
        let world = &mut sim.ecs.0;
        let mut runtime = world
            .remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            .expect("runtime");
        let mut def = BulletDef::new(BulletKind::Basic);
        def.speed = 0.0;
        def.lifetime = 100.0;
        def.incend_chance = 1.0;
        def.incend_amount = 1;
        def.incend_spread = 0.0;
        let id = runtime.content.add_bullet(def).expect("add bullet");
        let audio = crate::audio::AudioSinkRes::noop();
        let mut spawned = Vec::new();
        {
            let crate::sim::unit_runtime::UnitRuntime {
                content,
                grid,
                rng,
                fx,
                seq,
                ..
            } = &mut runtime;
            let mut ctx = crate::combat::bullet::CombatCtx {
                world,
                content,
                grid,
                rng,
                fx: fx.as_ref(),
                audio: &audio,
                seq,
                spawned: &mut spawned,
            };
            let spawn = crate::combat::BulletSpawn {
                def: id,
                x: 42.0,
                y: 42.0,
                angle: 0.0,
                ..crate::combat::BulletSpawn::default()
            };
            let entity = ctx.spawn(&spawn).expect("bullet");
            crate::combat::bullet::hit_bullet(&mut ctx, entity, 42.0, 42.0, false);
        }
        world.insert_non_send(runtime);
    }
    assert!(
        crate::combat::fires::find_at(&sim.ecs.0, 5, 5).is_some(),
        "the incend bullet left a fire on the hit tile"
    );
}

/// PAR-0091: a destroyed liquid container spills its contents as puddles
/// (`BuildingComp.splashLiquid` via `resolve_deaths`, `damageExplosions` on).
#[test]
fn live_destroyed_liquid_container_splashes() {
    let mut sim = live_sim(24);
    let tank = place(&mut sim, 10, 10, "liquid-tank", 1);
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .liquid_id("water")
        .expect("water");
    {
        let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::LiquidModule>(tank)
        else {
            return; // no liquid module fixture
        };
        module.liquids[water.index()] = 40.0;
        module.current_amount = 40.0;
    }
    sim.ecs
        .0
        .get_mut::<Health>(tank)
        .expect("tank health")
        .health = 0.0;
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(tank).is_err(),
        "the tank was destroyed"
    );
    let puddles = sim
        .ecs
        .0
        .iter_entities()
        .filter(|entity| {
            entity
                .get::<crate::combat::puddles::PuddleState>()
                .is_some()
        })
        .count();
    assert!(puddles > 0, "the tank spilled water puddles ({puddles})");
}

/// PAR-0127: `rules.weather = [rain always]` spawns a live `WeatherRuntime`
/// and the rain wets a ground unit.
#[test]
fn weather_rain_spawns_and_wets_units() {
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::{Rules, WeatherEntry};
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(32);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        weather: vec![WeatherEntry {
            weather: String::from("rain"),
            always: true,
            intensity: 1.0,
            ..WeatherEntry::default()
        }],
        ..Rules::default()
    });
    session.phase = State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        sim.group_counts().get("weather").copied().unwrap_or(0),
        1,
        "one live weather entity"
    );
    let wet = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("wet")
        .expect("wet");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::StatusComp>(dagger)
            .is_some_and(|status| status.has_effect_of(wet)),
        "rain applied wet to the ground unit"
    );
}

/// PAR-0138: a player `mega` picks up an adjacent friendly dagger and drops it
/// elsewhere (`requestUnitPayload` -> `dropLastPayload`).
#[test]
fn mega_picks_up_and_drops_dagger() {
    use crate::entities::comp::unit::comp::{CarriedPayload, UnitPayloads};
    let mut sim = live_sim(48);
    let mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    let dagger_type = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("dagger")
        .expect("dagger id");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 110.0, 100.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(
        sim.apply_payload_command(0, 0.0, 0.0, Some(seq)),
        "the dagger is picked up"
    );
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "the carried dagger left the world"
    );
    let stack = sim.ecs.0.get::<UnitPayloads>(mega).expect("payload stack");
    assert_eq!(stack.stack.len(), 1, "one carried payload");
    assert!(matches!(stack.stack[0], CarriedPayload::Unit { .. }));

    // Move the carrier, then drop the payload at the new spot.
    sim.ecs.0.get_mut::<Pos>(mega).expect("carrier pos").x = 200.0;
    assert!(
        sim.apply_payload_command(2, 200.0, 100.0, None),
        "the payload is dropped"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitPayloads>(mega)
            .map(|payloads| payloads.stack.len()),
        Some(0),
        "the stack is empty after the drop"
    );
    let dropped = sim.unit_runtime().expect("rt").units.iter().any(|entity| {
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(*entity)
            .is_some_and(|comp| comp.type_id == dagger_type)
    });
    assert!(dropped, "a dagger exists again at the drop point");
}

/// PAR-0138: a player `mega` picks up a friendly 1x1 building and places it
/// back on drop (`requestBuildPayload` -> `dropBlock`).
#[test]
fn mega_picks_up_and_drops_building() {
    use crate::entities::comp::unit::comp::{CarriedPayload, UnitPayloads};
    let mut sim = live_sim(48);
    let mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    let wall = place(&mut sim, 14, 12, "copper-wall", 1);
    // A point inside the wall tile (tile 14 spans x 112..120, y 96..104).
    assert!(
        sim.apply_payload_command(1, 115.0, 99.0, None),
        "the wall is picked up"
    );
    assert!(
        sim.grid.tiles.get(14, 12).build.is_none(),
        "the wall left the grid"
    );
    assert!(
        sim.ecs.0.get_entity(wall).is_err(),
        "the wall entity is gone"
    );
    let stack = sim.ecs.0.get::<UnitPayloads>(mega).expect("payload stack");
    assert_eq!(stack.stack.len(), 1);
    assert!(matches!(stack.stack[0], CarriedPayload::Build { .. }));

    // Move the carrier and drop: the wall must land on a real tile again.
    sim.ecs.0.get_mut::<Pos>(mega).expect("carrier pos").x = 200.0;
    assert!(
        sim.apply_payload_command(2, 200.0, 100.0, None),
        "the wall is dropped"
    );
    let placed = sim
        .grid
        .tiles
        .array()
        .iter()
        .filter(|tile| tile.build.is_some())
        .count();
    assert_eq!(placed, 1, "the wall is back on the grid");
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitPayloads>(mega)
            .map(|payloads| payloads.stack.len()),
        Some(0),
        "the stack is empty after the drop"
    );
}

/// Audit fix: an idle unit turns to face an in-range target
/// (`AIController.faceTarget`) so its fixed weapons can fire. Before the fix a
/// stationary dagger facing away from an enemy never shot.
#[test]
fn idle_unit_faces_and_shoots_target_behind_it() {
    let mut sim = live_sim(48);
    // Facing +x (rotation 0); the enemy sits behind it on -x.
    let shooter = sim
        .spawn_live_unit("dagger", 1, 200.0, 200.0, 0.0)
        .expect("shooter");
    let enemy = sim
        .spawn_live_unit("dagger", 2, 150.0, 200.0, 0.0)
        .expect("enemy");
    // Keep the enemy passive and both units in place.
    if let Some(mut weapons) = sim.ecs.0.get_mut::<crate::weapons::UnitWeapons>(enemy) {
        weapons.mounts.clear();
        weapons.weapons.clear();
    }
    let start = health(&sim, enemy).expect("enemy health");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let rotation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::UnitCore>(shooter)
        .map(|core| core.rotation);
    let end = health(&sim, enemy).unwrap_or(0.0);
    assert!(
        end < start,
        "idle dagger shot the enemy behind it ({start} -> {end}, rotation {rotation:?})"
    );
}

/// The live combat systems must emit through the installed `FxSink`; the
/// gdext host swaps the noop sink for the `MindFx` bus at boot (T12/PAR-0070).
#[test]
fn live_combat_emits_fx_on_the_installed_sink() {
    let mut sim = live_sim(48);
    let bus = std::sync::Arc::new(crate::fx::sink::FxBus::new());
    sim.unit_runtime_mut().expect("runtime").fx = bus.clone();
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let (tx, ty) = (20.0 * 8.0, 20.0 * 8.0);
    let _ = sim
        .spawn_live_unit("dagger", 2, tx + 60.0, ty + 20.0, 180.0)
        .expect("dagger");
    let mut events = 0usize;
    for _ in 0..900 {
        sim.tick().expect("tick");
        events += bus.drain().len();
        if events > 0 {
            break;
        }
    }
    assert!(events > 0, "combat emitted no FxSink events");
}

/// G01 T3: a team-1 `quasar` beam (`LaserBulletType`, `Damage.collideLine`)
/// kills a team-2 dagger 100 px away. Needs T2's range fix (146 >= 100).
#[test]
fn quasar_laser_kills_dagger() {
    let mut sim = live_sim(48);
    let _quasar = sim
        .spawn_live_unit("quasar", 1, 100.0, 100.0, 0.0)
        .expect("quasar");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 200.0, 100.0, 180.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "quasar beam killed the dagger; health {:?}",
        health(&sim, dagger)
    );
}

/// G01 T3: a team-1 `locus` rail (`RailBulletType.init` -> `Damage.collideLine`)
/// kills a team-2 dagger 120 px away.
#[test]
fn locus_rail_kills_dagger() {
    let mut sim = live_sim(48);
    let _locus = sim
        .spawn_live_unit("locus", 1, 100.0, 100.0, 0.0)
        .expect("locus");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 220.0, 100.0, 180.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "locus rail killed the dagger; health {:?}",
        health(&sim, dagger)
    );
}

/// G01 T3: `RailBulletType.handlePierce` drains the rail's damage by
/// `initialHealth * pierceDamageFactor` after the first pierce, so only the
/// first dagger in the line takes the full 48 damage.
#[test]
fn rail_pierces_units_with_damage_falloff() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(48);
    let mut daggers = Vec::new();
    for x in [140.0, 160.0, 180.0] {
        let dagger = sim
            .spawn_live_unit("dagger", 2, x, 100.0, 180.0)
            .expect("dagger");
        if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
            slot.kind = AiKind::NoAi;
        }
        daggers.push(dagger);
    }
    let locus = sim
        .spawn_live_unit("locus", 1, 100.0, 100.0, 0.0)
        .expect("locus");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(locus) {
        slot.kind = AiKind::NoAi;
    }
    // Force one shot straight east (`WeaponsComp` aim + shoot).
    if let Some(mut weapons) = sim.ecs.0.get_mut::<crate::weapons::UnitWeapons>(locus) {
        let mount = &mut weapons.mounts[0];
        mount.aim_x = 300.0;
        mount.aim_y = 100.0;
        mount.rotate = true;
        mount.rotation = 0.0;
        mount.shoot = true;
    }
    // Read the weapon bullet's damage/falloff from content directly.
    let (rail_damage, pierce_factor) = {
        let rt = sim.unit_runtime().expect("rt");
        let unit = rt.content.unit_by_name("locus").expect("locus def").clone();
        let bullet = rt
            .content
            .bullet(unit.weapons[0].bullet.id)
            .expect("rail bullet");
        (bullet.damage, bullet.pierce_damage_factor)
    };
    let before: Vec<f32> = daggers
        .iter()
        .map(|d| health(&sim, *d).expect("dagger health"))
        .collect();
    let mut fired = 0;
    for _ in 0..60 {
        sim.tick().expect("tick");
        fired = sim.unit_runtime().expect("rt").bullets_created;
        if fired > 0 {
            break;
        }
    }
    assert!(fired > 0, "locus fired its rail");
    let after: Vec<f32> = daggers
        .iter()
        .map(|d| health(&sim, *d).unwrap_or(0.0))
        .collect();
    let health = before[0];
    let second = (rail_damage - pierce_factor * health).max(0.0);
    assert_eq!(
        before[0] - after[0],
        rail_damage,
        "first dagger takes the full rail damage"
    );
    assert!(
        (before[1] - after[1] - second).abs() < 0.01,
        "second dagger takes the falloff-drained damage: {} vs {second}",
        before[1] - after[1]
    );
    assert!(
        (before[2] - after[2] - second).abs() < 0.01,
        "third dagger takes the falloff-drained damage: {} vs {second}",
        before[2] - after[2]
    );
}

/// G01 T3: `SapBulletType.init` (`Damage.linecast`) damages a target unit and
/// heals the shooter by `sapStrength` of the capped damage.
#[test]
fn spiroct_sap_heals() {
    let mut sim = live_sim(48);
    let spiroct = sim
        .spawn_live_unit("spiroct", 1, 100.0, 100.0, 0.0)
        .expect("spiroct");
    let max_health = sim
        .ecs
        .0
        .get::<Health>(spiroct)
        .map(|h| h.max_health)
        .expect("spiroct health");
    if let Some(mut h) = sim.ecs.0.get_mut::<Health>(spiroct) {
        h.health = max_health - 100.0;
    }
    let dagger = sim
        .spawn_live_unit("dagger", 2, 130.0, 100.0, 180.0)
        .expect("dagger");
    let dagger_start = health(&sim, dagger).expect("dagger health");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let spiroct_now = health(&sim, spiroct).expect("spiroct alive");
    assert!(
        spiroct_now > max_health - 100.0,
        "sap healed the spiroct ({max_health} max -> {spiroct_now})"
    );
    let dagger_now = health(&sim, dagger).unwrap_or(0.0);
    assert!(
        dagger_now < dagger_start,
        "sap damaged the dagger ({dagger_start} -> {dagger_now})"
    );
}

/// Builds a `live_sim` with a `CampaignRuntime` whose rules are mutated by
/// `mutate` (team multipliers for the G01 T5 tests).
fn campaign_sim(size: i32, mutate: impl FnOnce(&mut crate::game::rules::Rules)) -> Sim {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(size);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    mutate(&mut rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
    sim
}

/// G01 T5: `ShieldComp.damage` divides by `rules.unitHealth(team)`, so a
/// wave-team `unitHealthMultiplier` of 2 halves incoming damage.
#[test]
fn wave_unit_health_multiplier_halves_damage() {
    let mut sim = campaign_sim(48, |rules| {
        rules.team_rule_mut(2).unit_health_multiplier = 2.0;
    });
    let dagger = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    let before = health(&sim, dagger).expect("health");
    let content = crate::content::test_support::test_registry();
    let applied = crate::combat::damage::area::damage_entity(
        &mut sim.ecs.0,
        &content,
        dagger,
        20.0,
        true,
        1.0,
    );
    let after = health(&sim, dagger).expect("health");
    assert_eq!(
        before - after,
        10.0,
        "unitHealth 2 halves 20 damage (applied {applied})"
    );
}

/// G01 T5: `BulletType.damageMultiplier` scales a unit-owned bullet by
/// `rules.unitDamage(team)`.
#[test]
fn unit_damage_multiplier_scales_bullets() {
    use crate::combat::bullet::{Bullet, BulletSpawn};
    use crate::determinism::SimRng;

    let mut sim = campaign_sim(48, |rules| {
        rules.team_rule_mut(1).unit_damage_multiplier = 2.0;
    });
    let shooter = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let def_id = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .unit_by_name("dagger")
            .expect("dagger def")
            .weapons[0]
            .bullet
            .id
    };
    let content = crate::content::test_support::test_registry();
    let mut rng = SimRng::new(7);
    let spawn = BulletSpawn {
        def: def_id,
        owner: Some(shooter),
        team: 1,
        x: 100.0,
        y: 100.0,
        damage: -1.0,
        damage_multiplier: 1.0,
        ..BulletSpawn::default()
    };
    let bullet = crate::combat::bullet::create(&mut sim.ecs.0, &content, &mut rng, 1, &spawn)
        .expect("bullet");
    let state = sim.ecs.0.get::<Bullet>(bullet).expect("bullet state");
    let base = content.bullet(def_id).expect("def").damage;
    assert_eq!(
        state.damage,
        2.0 * base,
        "unitDamage 2 doubles the {base} base damage"
    );
}

/// G01 T5: `BuildingComp.damage` turns a zero `rules.blockHealth(team)` into
/// `damage = health + 1` (any hit kills).
#[test]
fn block_health_zero_kills() {
    let mut sim = campaign_sim(48, |rules| {
        rules.block_health_multiplier = 0.0;
    });
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    crate::combat::damage::area::apply_health(&mut sim.ecs.0, wall, 1.0);
    let state = sim.ecs.0.get::<Health>(wall).expect("health");
    assert!(
        state.dead || state.health <= 0.0,
        "blockHealth 0 kills on 1 damage (health {})",
        state.health
    );
}

/// G01 T8: `CoreBlock.playerSpawn` uses the core's `unitType` and the death
/// timer respawns at the best core after 60 ticks.
#[test]
fn player_respawns_after_death() {
    use crate::content::ctype::Mappable;
    use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};

    let mut sim = live_sim(48);
    let _core = place(&mut sim, 20, 20, "core-foundation", 1);
    let player = sim.spawn_player_at_best_core(1).expect("player spawn");
    let unit_type = sim.ecs.0.get::<UnitTypeComp>(player).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(unit_type, Some("beta"), "foundation core spawns beta");
    assert!(
        sim.ecs
            .0
            .get::<UnitCore>(player)
            .is_some_and(|core| core.spawned_by_core),
        "spawnedByCore set"
    );
    // Kill the player unit.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(player) {
        health.health = 0.0;
        health.dead = true;
    }
    for _ in 0..59 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.player_unit().is_none(),
        "no respawn before the 60-tick death delay"
    );
    sim.tick().expect("tick");
    let respawned = sim.player_unit().expect("respawned after 60 ticks");
    let (rx, ry) = sim.player_unit_position().expect("position");
    assert!(
        (rx - 164.0).abs() <= 3.0 && (ry - 164.0).abs() <= 3.0,
        "respawn at the foundation core centre ({rx}, {ry})"
    );
    let kind = sim.ecs.0.get::<UnitTypeComp>(respawned).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(kind, Some("beta"), "respawn uses the core's unit type");
}

/// G01 T8: a shard core spawns `alpha`.
#[test]
fn core_shard_spawns_alpha() {
    use crate::content::ctype::Mappable;
    use crate::entities::comp::unit::comp::UnitTypeComp;

    let mut sim = live_sim(48);
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    let player = sim.spawn_player_at_best_core(1).expect("player spawn");
    let name = sim.ecs.0.get::<UnitTypeComp>(player).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(name, Some("alpha"));
}

/// G01 T9: possessing an own AI unit moves the Player controller and despawns
/// the previous core-spawned unit without death effects.
#[test]
fn possess_own_dagger_despawns_core_alpha() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::Player;

    let mut sim = live_sim(48);
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    let alpha = sim.spawn_player_at_best_core(1).expect("alpha");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 180.0, 180.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(sim.possess_unit(seq), "possess the dagger");
    assert_eq!(sim.player_unit(), Some(dagger));
    assert!(
        sim.ecs.0.get_entity(alpha).is_err(),
        "core alpha despawned by possession"
    );
    assert!(
        sim.ecs.0.get::<Player>(dagger).is_some(),
        "dagger carries the Player marker"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Player)
    );
}

/// G01 T9: `UnitClear` on a possessed unit releases it to AI and respawns at
/// the core.
#[test]
fn clear_unit_returns_to_core() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::content::ctype::Mappable;
    use crate::entities::comp::Player;
    use crate::entities::comp::unit::comp::UnitTypeComp;

    let mut sim = live_sim(48);
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    let _alpha = sim.spawn_player_at_best_core(1).expect("alpha");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 180.0, 180.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(sim.possess_unit(seq));
    assert!(sim.clear_unit(), "unit clear");
    let respawned = sim.player_unit().expect("respawned at the core");
    assert_ne!(respawned, dagger, "a fresh core unit was spawned");
    assert!(
        sim.ecs.0.get::<Player>(dagger).is_none(),
        "released unit dropped the Player marker"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Ground),
        "released unit went back to its team AI"
    );
    let name = sim.ecs.0.get::<UnitTypeComp>(respawned).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(name, Some("alpha"), "respawn uses the core unit type");
}

/// G01 T9: enemy and dead units cannot be possessed.
#[test]
fn cannot_possess_enemy_or_dead_unit() {
    let mut sim = live_sim(48);
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    let _alpha = sim.spawn_player_at_best_core(1).expect("alpha");
    let enemy = sim
        .spawn_live_unit("dagger", 2, 220.0, 220.0, 0.0)
        .expect("enemy");
    let enemy_seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(enemy)
        .expect("seq")
        .0 as i32;
    assert!(
        !sim.possess_unit(enemy_seq),
        "enemy unit is not possessable"
    );
    let own = sim
        .spawn_live_unit("dagger", 1, 180.0, 180.0, 0.0)
        .expect("own");
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(own) {
        health.health = 0.0;
        health.dead = true;
    }
    let own_seq = sim.ecs.0.get::<crate::ecs::EntitySeq>(own).expect("seq").0 as i32;
    assert!(!sim.possess_unit(own_seq), "dead unit is not possessable");
}

/// G01 T19: `Teams.bosses` lists wave-team units carrying the `boss` status.
#[test]
fn boss_list_tracks_wave_boss() {
    let mut sim = campaign_sim(48, |rules| {
        rules.wave_team = 2;
    });
    let boss = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("wave dagger");
    let own = sim
        .spawn_live_unit("dagger", 1, 130.0, 100.0, 0.0)
        .expect("own dagger");
    let boss_status = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("boss")
        .expect("boss status");
    for entity in [boss, own] {
        crate::combat::damage::status::apply_status(&mut sim.ecs.0, entity, boss_status, 999_999.0);
    }
    assert_eq!(sim.bosses(), vec![boss], "only the wave-team boss listed");
    let json: serde_json::Value = serde_json::from_str(&sim.boss_json()).expect("boss json parses");
    assert_eq!(json["count"], 1);
    assert_eq!(json["names"][0], "dagger");
    let first = &json["first"];
    assert_eq!(first["type"], "dagger");
    assert_eq!(first["flying"], false);
    // After the boss dies the list is empty.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(boss) {
        health.health = 0.0;
        health.dead = true;
    }
    assert!(sim.bosses().is_empty(), "dead boss is dropped");
    let json: serde_json::Value = serde_json::from_str(&sim.boss_json()).expect("boss json parses");
    assert_eq!(json["count"], 0);
    assert!(json["first"].is_null());
}

/// T12: a powered repair point heals a damaged friendly unit once its aim is
/// within 30 degrees (`RepairTurret.java:186-223`); the plan's expected
/// +`repairSpeed * strength * edelta` per aligned tick.
#[test]
fn repair_point_heals_damaged_dagger() {
    let mut sim = live_sim(48);
    let _repair = place(&mut sim, 20, 20, "repair-point", 1);
    let _power = place(&mut sim, 21, 20, "power-source", 1);
    let tx = 20.0 * 8.0 + 4.0;
    let ty = 20.0 * 8.0 + 4.0;
    let dagger = sim
        .spawn_live_unit("dagger", 1, tx + 30.0, ty, 180.0)
        .expect("friendly dagger");
    sim.ecs
        .0
        .get_mut::<Health>(dagger)
        .expect("dagger health")
        .health = 50.0;
    let max = sim
        .ecs
        .0
        .get::<Health>(dagger)
        .map(|health| health.max_health)
        .unwrap_or(0.0);
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let healed = health(&sim, dagger).expect("dagger alive");
    assert!(healed > 50.0, "repair point healed the dagger: {healed}");
    assert!(
        healed <= max,
        "heal clamped to max health: {healed} <= {max}"
    );
    // `UnitTypes.java:104`: dagger `health = 150`.
    assert_eq!(max, 150.0, "dagger max health");
}

/// PAR-0073/T3: a killed wall plays `destroySound` (an `At` with the size-1
/// alternate pair) and no `Construct`/break sound (`BuildingComp.java:1524-1543`).
#[test]
fn destroyed_wall_emits_destroy_sound() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = live_sim(32);
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let wall = place(&mut sim, 10, 10, "copper-wall", 1);
    sim.tick().expect("tick");
    // Drop the placement's own place sound so only the death remains.
    log.clear();
    sim.ecs
        .0
        .get_mut::<Health>(wall)
        .expect("wall health")
        .health = 0.0;
    sim.tick().expect("tick");
    let events = log.events();
    let ats: Vec<SoundId> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::At { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect();
    assert!(
        ats.iter().any(|sound| {
            *sound == SoundId::BLOCK_EXPLODE1 || *sound == SoundId::BLOCK_EXPLODE1_ALT
        }),
        "destroy sound in {ats:?}"
    );
    assert!(
        !events
            .iter()
            .any(|ticked| matches!(ticked.event, AudioEvent::Construct { .. })),
        "a destroyed wall must not play place/break sounds: {events:?}"
    );
}

/// G07 T14: payload pickup/drop sounds (`PayloadComp.java:107,117,145,203`).
#[test]
fn payload_commands_emit_sounds() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = live_sim(48);
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let _mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 110.0, 100.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(
        sim.apply_payload_command(0, 0.0, 0.0, Some(seq)),
        "the dagger is picked up"
    );
    let events = log.events();
    assert!(
        events.iter().any(|ticked| matches!(
            &ticked.event,
            AudioEvent::At { sound, pitch, .. }
                if *sound == SoundId::PAYLOAD_PICKUP && (0.9..=1.1).contains(pitch)
        )),
        "pickup sound: {events:?}"
    );

    log.clear();
    assert!(
        sim.apply_payload_command(2, 100.0, 100.0, None),
        "the payload is dropped"
    );
    let events = log.events();
    // Dagger hitSize 8 <= 12 -> payloadDrop1.
    assert!(
        events.iter().any(|ticked| matches!(
            &ticked.event,
            AudioEvent::At { sound, .. } if *sound == SoundId::PAYLOAD_DROP1
        )),
        "drop sound: {events:?}"
    );
}

/// G06 T7: unit `@x/@y` are tiles (px/8) and `controlled` uses the Java
/// constants (`ctrlProcessor = 1`, `ctrlPlayer = 2`, `ctrlCommand = 3`).
#[test]
fn unit_xy_tiles_and_controlled() {
    let mut sim = live_sim(16);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 80.0, 160.0, 0.0)
        .expect("dagger");
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::logic::value::LogicObject;
    let target = LogicObject::Unit(dagger);
    assert_eq!(sense(&sim.ecs.0, &target, LAccess::X), Sensed::Num(10.0));
    assert_eq!(sense(&sim.ecs.0, &target, LAccess::Y), Sensed::Num(20.0));
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::Controlled),
        Sensed::Num(0.0)
    );
    // Velocity is tiles/sec (`vel * 60 / 8`).
    let vel = sense(&sim.ecs.0, &target, LAccess::VelocityY);
    let Sensed::Num(vy) = vel else {
        panic!("velocity is numeric");
    };
    assert!((vy - 0.0).abs() < 1e-4, "stationary dagger: {vy}");
}
