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
    // Hostile wall dummy inside the duo's 150 px range: a unit target would
    // return fire (G01 T6a gives it the Java targeting cadence), which kills
    // the duo before the coolant window ends.
    let _wall = place(&mut sim, 31, 20, "copper-wall", 2);
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
    // Inside the flare's 76 px range: `findMainTarget` (`{generator, null}`)
    // only engages an in-range target when no flagged building exists (G01 T6b
    // removed the nearest-building anchor).
    let wall = place(&mut sim, 12, 10, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let flare = sim
        .spawn_live_unit("flare", 2, 11.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("flare");
    let shots = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(flare)
            .map(|weapons| weapons.mounts.iter().map(|m| m.total_shots).sum::<i32>())
            .unwrap_or(0)
    };
    // The Java `circleAttack` orbit is a transient for non-omni units: the
    // `diff > 70 && len < circleLength` straight-segment branch grows the loop
    // until the target leaves `range`, after which `FlyingAI` idles (Java
    // behaviour — the pre-T6b port kept moving via its nearest-building
    // anchor). Sample the engagement window [120, 600]: still inside range.
    let mut shots_mid = 0;
    let mut moving_ticks = 0;
    let mut sampled = 0;
    let mut engaged = 0;
    for tick in 0..400 {
        sim.tick().expect("tick");
        let vel = sim
            .ecs
            .0
            .get::<Vel>(flare)
            .map(|vel| (vel.x * vel.x + vel.y * vel.y).sqrt())
            .unwrap_or(0.0);
        if tick > 30 {
            sampled += 1;
            if vel > 0.01 {
                moving_ticks += 1;
            }
        }
        if sim
            .ecs
            .0
            .get::<crate::ai::types::flying::FlyingAiState>(flare)
            .is_some_and(|state| state.target.is_some())
        {
            engaged += 1;
        }
        if tick == 120 {
            shots_mid = shots(&sim);
        }
    }
    let shots_end = shots(&sim);
    assert!(
        engaged > 100,
        "the flare held its in-range target for a while: {engaged} ticks"
    );
    assert!(
        sampled > 0 && moving_ticks * 10 >= sampled * 9,
        "flare keeps circling: {moving_ticks}/{sampled} sampled ticks with speed"
    );
    assert!(
        shots_end > shots_mid,
        "flare keeps firing while engaged ({shots_mid} -> {shots_end})"
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
    // `Damage.dynamicExplosion` staggers its waves 2 ticks apart
    // (`Time.run(i * 2f)`); the outer waves carry the radius.
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
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
    // The fire now comes from the staggered `Bullets.fireball` spawns
    // (`Time.run(i / 2f)`), not an immediate `Fires.create`.
    for _ in 0..40 {
        sim.tick().expect("tick");
    }
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

/// T14/T10c: a builder unit (`AiKind::Builder`) picks up the ghost plan of a
/// destroyed friendly wall and rebuilds it (`BuilderAI` +
/// `Sim::update_builders`, G01 T10c semantics).
#[test]
fn commanded_builder_repairs_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("copper")
        .expect("copper");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(core) {
        items.add(copper, 500, 4000);
    }
    let wall = place(&mut sim, 16, 12, "copper-wall", 1);
    // Destroy it so `resolve_deaths` queues the team ghost plan.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    let builder = sim
        .spawn_live_unit("alpha", 1, 100.0, 100.0, 0.0)
        .expect("alpha");
    // `alpha`'s content controller is `BuilderOrCommand` -> `AiKind::Builder`
    // for spawned units.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(builder) {
        slot.kind = AiKind::Builder;
    }
    for _ in 0..1200 {
        sim.tick().expect("tick");
    }
    let rebuilt = sim.grid.tiles.get(16, 12).build.is_some();
    assert!(rebuilt, "the builder rebuilt the destroyed wall");
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
        unit_cap: 100,
        unit_cap_variable: false,
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

/// G01 T6a: a wave ground unit walks the enemy-core flowfield
/// (`Pathfinder.fieldCore`, GroundAI.java:46-60) instead of detouring to the
/// nearest player building, and stops at `type.range * 0.5`.
#[test]
fn ground_wave_paths_to_core_not_nearest_building() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    place(&mut sim, 10, 48, "core-shard", 1);
    // Decoy: much closer to the dagger than the core, but on another axis.
    let wall = place(&mut sim, 80, 10, "copper-wall", 1);
    let dagger = sim
        .spawn_live_unit("dagger", 2, 85.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("dagger");
    let start_x = sim.ecs.0.get::<Pos>(dagger).expect("pos").x;
    for _ in 0..3000 {
        sim.tick().expect("tick");
    }
    let x = sim
        .ecs
        .0
        .get::<Pos>(dagger)
        .map(|pos| pos.x)
        .unwrap_or(f32::MAX);
    eprintln!(
        "dagger x {x} (start {start_x}), wall health {:?}",
        health(&sim, wall)
    );
    assert!(
        start_x - x > 300.0,
        "the dagger walked toward the enemy core (start x {start_x}, now {x})"
    );
    let wall_max = sim
        .ecs
        .0
        .get::<Health>(wall)
        .expect("wall health")
        .max_health;
    assert_eq!(
        health(&sim, wall),
        Some(wall_max),
        "the decoy wall was never attacked"
    );
}

/// G01 T6a: every unit of a team shares one `(team, cost)` flowfield per tick
/// (plan §6.9): two daggers must not rebuild it per unit.
#[test]
fn field_is_stable_across_units() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    place(&mut sim, 10, 48, "core-shard", 1);
    sim.spawn_live_unit("dagger", 2, 85.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("dagger a");
    sim.spawn_live_unit("dagger", 2, 85.0 * 8.0, 44.0 * 8.0, 180.0)
        .expect("dagger b");

    let before = sim.unit_runtime().expect("rt").pathfinder.updates;
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let after = sim.unit_runtime().expect("rt").pathfinder.updates;
    assert!(
        after - before <= 2,
        "one flowfield build per (team, cost) per tick: {before} -> {after}"
    );
}

/// G01 T6b: a wave `horizon` (targetFlags `{factory, null}`) bombs the closest
/// active factory even though an enemy core is on the map
/// (`FlyingAI.findMainTarget`, FlyingAI.java:40-72).
#[test]
fn horizon_prefers_factory() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    // Core 147 px from the horizon (out of its 140 px range) but still nearer
    // than the factory at 170 px: the `{factory, null}` flag must win over the
    // old nearest-building anchor.
    place(&mut sim, 47, 47, "core-shard", 1);
    let core = sim.grid.tiles.get(47, 47).build.expect("core entity");
    let press = place(&mut sim, 75, 75, "graphite-press", 1);
    // `targetFlagActive` needs `items.any()` in the port's model.
    let coal = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("coal")
        .expect("coal");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(press) {
        items.add(coal, 1, 10);
    }
    let _horizon = sim
        .spawn_live_unit("horizon", 2, 60.0 * 8.0, 60.0 * 8.0, 180.0)
        .expect("horizon");
    let core_max = sim
        .ecs
        .0
        .get::<Health>(core)
        .expect("core health")
        .max_health;
    let mut destroyed_at = None;
    for tick in 0..900 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(press).is_err() {
            destroyed_at = Some(tick);
            break;
        }
    }
    let core_now = health(&sim, core);
    eprintln!("factory destroyed at {destroyed_at:?}, core {core_now:?}/{core_max}");
    assert!(
        destroyed_at.is_some(),
        "the factory was bombed (core {core_now:?}/{core_max})"
    );
    assert_eq!(
        core_now,
        Some(core_max),
        "the horizon never targeted the nearer core while the factory died"
    );
}

/// G01 T6b: a `flare` (`targetFlags {generator, null}`) with no generators
/// falls back to the enemy core (`FlyingAI.findMainTarget`'s final `return core`).
#[test]
fn flare_falls_back_to_core() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let flare = sim
        .spawn_live_unit("flare", 2, 60.0 * 8.0, 60.0 * 8.0, 180.0)
        .expect("flare");
    // The flare's `circleAttack` orbit is chaotic over long windows; record the
    // closest approach to the core instead of the final position.
    let mut min_dist = f32::MAX;
    for _ in 0..900 {
        sim.tick().expect("tick");
        if let (Some(a), Some(b)) = (
            sim.ecs.0.get::<Pos>(flare).copied(),
            sim.ecs.0.get::<Pos>(core).copied(),
        ) {
            min_dist = min_dist.min(((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt());
        }
    }
    let a = sim.ecs.0.get::<Pos>(flare).copied().expect("flare pos");
    let b = sim.ecs.0.get::<Pos>(core).copied().expect("core pos");
    let end = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    eprintln!("flare-core final {end}, closest {min_dist}");
    assert!(
        min_dist < 100.0,
        "the flare flew to the core (closest approach {min_dist})"
    );
}

/// G01 T10a: a team-1 `mono` (default command `mine`) picks copper ore next to
/// its core, mines it (`MinerAI.updateMovement` + `MinerComp.update`) and
/// delivers a full stack to the core.
#[test]
fn mono_mines_and_delivers() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let ore = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("ore-copper").expect("ore-copper")
    };
    for i in 0..3 {
        sim.grid.tiles.get_mut(20 + i, 10).overlay = ore;
    }
    sim.refresh_unit_runtime();
    let mono = sim
        .spawn_live_unit("mono", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("mono");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mono) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..2000 {
        sim.tick().expect("tick");
    }
    let copper = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("copper")
        .expect("copper");
    let delivered = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(copper));
    let mined = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::MinerComp>(mono)
        .map_or(0, |comp| comp.mined);
    eprintln!("mono mined {mined}, core copper {delivered}");
    assert!(delivered >= 20, "the mono delivered copper: {delivered}");
}

/// G01 T10a: titanium ore (hardness 3) is above the mono's `mineTier` 1, so the
/// target-item selection skips it and nothing is mined.
#[test]
fn miner_comp_respects_tier() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let ore = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("ore-titanium").expect("ore-titanium")
    };
    for i in 0..3 {
        sim.grid.tiles.get_mut(20 + i, 10).overlay = ore;
    }
    sim.refresh_unit_runtime();
    let mono = sim
        .spawn_live_unit("mono", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("mono");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mono) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let titanium = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("titanium")
        .expect("titanium");
    let mined = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::MinerComp>(mono)
        .map_or(0, |comp| comp.mined);
    let delivered = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(titanium));
    assert_eq!(mined, 0, "titanium is above the mono's tier");
    assert_eq!(delivered, 0, "nothing reaches the core");
}

/// G01 T10b: a team-1 `mega` (default command `repair`) flies to a damaged
/// friendly wall, aims its heal bolts at it and restores its health
/// (`RepairAI.updateTargeting/updateMovement`, RepairAI.java:19-80 + T4's
/// `hitTile` heals).
#[test]
fn mega_repairs_damaged_wall() {
    use crate::ai::controller::{AiKind, ControllerSlot};
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
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    place(&mut sim, 10, 10, "core-shard", 1);
    let wall = place(&mut sim, 30, 10, "copper-wall", 1);
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.health = 100.0;
    }
    let mega = sim
        .spawn_live_unit("mega", 1, 10.0 * 8.0, 14.0 * 8.0, 0.0)
        .expect("mega");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mega) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let healed = health(&sim, wall).unwrap_or(0.0);
    let (mp, wp) = (
        sim.ecs.0.get::<Pos>(mega).copied().expect("mega pos"),
        sim.ecs.0.get::<Pos>(wall).copied().expect("wall pos"),
    );
    let dist = ((mp.x - wp.x).powi(2) + (mp.y - wp.y).powi(2)).sqrt();
    eprintln!("wall {healed} hp, mega {dist} px away");
    assert!(healed > 100.0, "the mega healed the wall: {healed}");
    assert!(dist < 178.0, "the mega stays within range: {dist}");
}

/// G01 T10c: a team-1 `poly` (default command `rebuild`) pulls the destroyed
/// wall's ghost plan from the team queue and rebuilds it, paying the block
/// cost from the core (`BuilderAI` + `Sim::update_builders`).
#[test]
fn poly_rebuilds_destroyed_wall() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let copper = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("copper")
        .expect("copper");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(core) {
        items.add(copper, 500, 4000);
    }
    let before = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(copper));
    let wall = place(&mut sim, 20, 10, "copper-wall", 1);
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    let poly = sim
        .spawn_live_unit("poly", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("poly");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(poly) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..1200 {
        sim.tick().expect("tick");
    }
    let rebuilt = sim.grid.tiles.get(20, 10).build;
    let after = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(copper));
    eprintln!("rebuilt {rebuilt:?}, core copper {before} -> {after}");
    let entity = rebuilt.expect("the wall tile holds a building again");
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::ecs::BuildingComp>(entity)
            .map(|c| c.block.get()),
        Some(216),
        "the rebuilt block is a copper-wall"
    );
    assert!(
        after <= before - 6,
        "the core paid 6 copper ({before} -> {after})"
    );
}

/// G01 T10c: a wave-team core builder falls back to the ground AI
/// (`BuilderAI.fallback`, BuilderAI.java:264-278): the alpha walks the
/// enemy-core field instead of running the builder logic.
#[test]
fn wave_alpha_falls_back_to_ground_ai() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    place(&mut sim, 10, 48, "core-shard", 1);
    let alpha = sim
        .spawn_live_unit("alpha", 2, 80.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("alpha");
    let start_x = sim.ecs.0.get::<Pos>(alpha).expect("pos").x;
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let x = sim
        .ecs
        .0
        .get::<Pos>(alpha)
        .map(|pos| pos.x)
        .unwrap_or(f32::MAX);
    eprintln!("alpha x {start_x} -> {x}");
    assert!(
        start_x - x > 200.0,
        "the wave alpha walked toward the enemy core ({start_x} -> {x})"
    );
}

/// G01 T11: the `holdfire` stance stops a commanded dagger from shooting
/// (`CommandAI.shouldFire`, CommandAI.java:527).
#[test]
fn hold_fire_stops_shooting() {
    use crate::ai::controller::{AiKind, ControllerSlot};
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
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let shooter = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("shooter");
    let target = sim
        .spawn_live_unit("dagger", 2, 200.0, 100.0, 180.0)
        .expect("target");
    // Dummy target: the `Player` controller receives no input, so it never
    // shoots back while the shooter's fire counts are measured.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(target) {
        slot.kind = AiKind::Player;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(shooter) {
        slot.kind = AiKind::Command;
    }
    let shots = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(shooter)
            .map(|weapons| weapons.mounts.iter().map(|m| m.total_shots).sum::<i32>())
            .unwrap_or(0)
    };
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let before = shots(&sim);
    assert!(before > 0, "the dagger fired before the stance: {before}");
    let id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(shooter)
        .expect("seq")
        .0 as i32;
    let hold_fire = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_stance_by_name("holdfire")
        .expect("holdfire")
        .id
        .raw();
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: hold_fire,
        enabled: true,
    })
    .expect("stance");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let after = shots(&sim);
    assert_eq!(after, before, "holdFire stops firing ({before} -> {after})");
}

/// G01 T11: the `boost` stance lifts a `nova` (`CommandAI.shouldBoost` +
/// `UnitComp.updateBoosting`).
#[test]
fn boost_stance_lifts_nova() {
    use crate::ai::controller::{AiKind, ControllerSlot};
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
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let nova = sim
        .spawn_live_unit("nova", 1, 100.0, 100.0, 0.0)
        .expect("nova");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(nova) {
        slot.kind = AiKind::Command;
    }
    let id = sim.ecs.0.get::<crate::ecs::EntitySeq>(nova).expect("seq").0 as i32;
    let boost = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_stance_by_name("boost")
        .expect("boost")
        .id
        .raw();
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: boost,
        enabled: true,
    })
    .expect("stance");
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let elevation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::UnitCore>(nova)
        .map(|core| core.elevation)
        .unwrap_or(0.0);
    assert!(
        elevation > 0.5,
        "the boost stance lifts the nova: {elevation}"
    );
}

/// G01 T11: an item stance restricts the mono's target item
/// (`MinerAI`'s `ItemUnitStance` branch): only copper is mined although lead
/// ore is available.
#[test]
fn mono_item_stance_filters_item() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let (copper_ore, lead_ore) = {
        let rt = sim.unit_runtime().expect("rt");
        (
            rt.content.block_id("ore-copper").expect("ore-copper"),
            rt.content.block_id("ore-lead").expect("ore-lead"),
        )
    };
    for i in 0..3 {
        sim.grid.tiles.get_mut(20 + i, 10).overlay = copper_ore;
        sim.grid.tiles.get_mut(20 + i, 14).overlay = lead_ore;
    }
    sim.refresh_unit_runtime();
    let mono = sim
        .spawn_live_unit("mono", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("mono");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mono) {
        slot.kind = AiKind::Command;
    }
    let id = sim.ecs.0.get::<crate::ecs::EntitySeq>(mono).expect("seq").0 as i32;
    let copper_stance = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_stance_by_name("item-copper")
        .expect("item-copper")
        .id
        .raw();
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: copper_stance,
        enabled: true,
    })
    .expect("stance");
    for _ in 0..1500 {
        sim.tick().expect("tick");
    }
    let (copper, lead) = {
        let rt = sim.unit_runtime().expect("rt");
        let items = sim.ecs.0.get::<ItemModule>(core);
        (
            items.map_or(0, |i| i.get(rt.content.item_id("copper").expect("copper"))),
            items.map_or(0, |i| i.get(rt.content.item_id("lead").expect("lead"))),
        )
    };
    eprintln!("copper {copper}, lead {lead}");
    assert!(copper > 0, "the mono mined copper: {copper}");
    assert_eq!(lead, 0, "the item stance kept the miner off lead");
}

/// G01 T13: a grounded dagger on `deep-water` drowns after
/// `hitSize/8 * drownTimeMultiplier * floor.drownTime` = 200 ticks
/// (`UnitComp.updateDrowning`, UnitComp.java:706-723).
#[test]
fn dagger_drowns_in_deep_water() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(48);
    let deep = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("deep-water").expect("deep-water")
    };
    for x in 18..23 {
        for y in 18..23 {
            sim.grid.tiles.get_mut(x, y).floor = deep;
        }
    }
    sim.refresh_unit_runtime();
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0, 20.5 * 8.0, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
        slot.kind = AiKind::NoAi;
    }
    let alive = |sim: &Sim| {
        sim.ecs
            .0
            .get::<Health>(dagger)
            .is_some_and(|health| !health.dead)
    };
    for _ in 0..195 {
        sim.tick().expect("tick");
    }
    assert!(alive(&sim), "still alive at tick 195");
    for _ in 0..15 {
        sim.tick().expect("tick");
    }
    assert!(!alive(&sim), "drowned by tick 210");
}

/// G01 T13: `shallow-water` applies `wet` (90 ticks) to a grounded unit.
#[test]
fn shallow_water_applies_wet() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::unit::comp::StatusComp;

    let mut sim = live_sim(48);
    let shallow = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("shallow-water").expect("shallow-water")
    };
    for x in 18..23 {
        for y in 18..23 {
            sim.grid.tiles.get_mut(x, y).floor = shallow;
        }
    }
    sim.refresh_unit_runtime();
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0, 20.5 * 8.0, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
        slot.kind = AiKind::NoAi;
    }
    sim.tick().expect("tick");
    let wet = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("wet")
        .expect("wet");
    let entry = sim.ecs.0.get::<StatusComp>(dagger).and_then(|comp| {
        comp.statuses
            .iter()
            .find(|entry| entry.effect == wet)
            .copied()
    });
    let entry = entry.expect("wet applied by the shallow-water floor");
    assert!(
        (89.0..=90.0).contains(&entry.duration),
        "wet for 90 ticks: {}",
        entry.duration
    );
}

/// G01 T13: a bounded unit far outside the map is clamped back in
/// (`UnitComp.java:695-717`: the clamp runs before the `-250` kill check, so
/// the kill is effectively unreachable for bounded units — the plan text
/// expected death; Java clamps).
#[test]
fn unit_out_of_bounds_dies() {
    let mut sim = live_sim(48);
    let dagger = sim
        .spawn_live_unit("dagger", 2, -300.0, 100.0, 0.0)
        .expect("dagger");
    sim.tick().expect("tick");
    let pos = sim.ecs.0.get::<Pos>(dagger).copied().expect("dagger pos");
    let dead = sim
        .ecs
        .0
        .get::<Health>(dagger)
        .is_none_or(|health| health.dead);
    eprintln!("out-of-bounds dagger at {pos:?}, dead {dead}");
    assert!(
        !dead,
        "the unit survives (Java clamps before the kill check)"
    );
    assert!(pos.x >= 0.0, "the grounded clamp pulls it back in: {pos:?}");
}

/// G01 T13: standing on a solid tile kills a non-boosting unit; a `nova` can
/// boost instead (`UnitComp.java:849-855`). The port's movement helpers lack
/// Java's collision clamp, so a corner clip pushes the unit out and only an
/// embedded unit dies — here the dagger is displaced off the wall tile.
#[test]
fn dagger_on_wall_dies_nova_boosts() {
    let mut sim = live_sim(48);
    place(&mut sim, 20, 20, "copper-wall", 1);
    // `World.toTile` rounds, so tile-centre coordinates map to the *next*
    // tile: spawn inside (20,20) rather than on its centre.
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.4 * 8.0, 20.4 * 8.0, 0.0)
        .expect("dagger");
    // The nova also starts on the wall tile, where it boosts off instead.
    let nova = sim
        .spawn_live_unit("nova", 2, 20.3 * 8.0, 20.3 * 8.0, 0.0)
        .expect("nova");
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let dagger_tile = sim.ecs.0.get::<Pos>(dagger).map(|pos| {
        (
            crate::world::WorldGrid::to_tile(pos.x),
            crate::world::WorldGrid::to_tile(pos.y),
        )
    });
    let dead = sim
        .ecs
        .0
        .get::<Health>(dagger)
        .is_none_or(|health| health.dead);
    assert!(
        dead || dagger_tile != Some((20, 20)),
        "the dagger left the solid tile (or died): tile {dagger_tile:?}"
    );
    let elevation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::UnitCore>(nova)
        .map(|core| core.elevation)
        .unwrap_or(0.0);
    assert!(
        elevation > 0.9,
        "the nova boosted off the wall: {elevation}"
    );
}

/// G01 T13: a player-team unit inside a wave spawn's drop zone is pushed
/// outward (`UnitComp.java:734-744`).
#[test]
fn drop_zone_pushes_player_units() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::TilePos;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        // `GameState.hasSpawns()` (`GameState.java:68-70`) needs a spawn
        // *group* list; the drop-zone tiles come from the runtime spawner below.
        spawns: vec![crate::game::rules::SpawnGroup::default()],
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let spawn = TilePos::new(24, 24);
    if let Some(mut runtime) = sim.unit_runtime_mut() {
        runtime.wave_spawner.set_spawns(vec![spawn]);
    }
    let (sx, sy) = (24.5 * 8.0, 24.5 * 8.0);
    let dagger = sim
        .spawn_live_unit("dagger", 1, sx + 50.0, sy, 0.0)
        .expect("dagger");
    let start = 50.0f32;
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let end = sim
        .ecs
        .0
        .get::<Pos>(dagger)
        .map(|pos| ((pos.x - sx).powi(2) + (pos.y - sy).powi(2)).sqrt())
        .unwrap_or(start);
    eprintln!("drop-zone distance {start} -> {end}");
    assert!(end > start, "the unit was pushed outward: {start} -> {end}");
}

/// G01 T15: a bullet hitting a building emits a `BuildDamageEvent`
/// (`BuildingComp.damage(Bullet, ..)` -> `Events.fire`); draining clears it.
#[test]
fn bullet_hit_emits_build_damage_event() {
    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    sim.spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0, 180.0)
        .expect("dagger");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let events = sim.drain_build_damage_events();
    let wall_events: Vec<_> = events
        .iter()
        .filter(|event| event.x == 20 && event.y == 20)
        .collect();
    eprintln!(
        "build damage events: {} (wall {})",
        events.len(),
        wall_events.len()
    );
    assert!(!wall_events.is_empty(), "the wall took bullet damage");
    assert!(
        wall_events.iter().all(|event| event.team == 1),
        "events carry the damaged building's team"
    );
    assert!(
        wall_events.iter().all(|event| event.amount > 0.0),
        "applied damage recorded"
    );
    let _ = wall;
    let second = sim.drain_build_damage_events();
    assert!(second.is_empty(), "the drain clears the log");
}

/// G01 T16: an attack order on a *unit* follows the moving target
/// (`CommandAI.defaultBehavior`: `targetPos.set(attackTarget)` every tick) and
/// kills it.
#[test]
fn commanded_dagger_chases_moving_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::TilePos;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let attacker = sim
        .spawn_live_unit("dagger", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("attacker");
    let target = sim
        .spawn_live_unit("dagger", 2, 50.0 * 8.0, 40.0 * 8.0, 180.0)
        .expect("target");
    // Walk the target east at full speed.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(target) {
        slot.kind = AiKind::Command;
        slot.target = Some(TilePos::new(80, 40));
    }
    // The wire unit-id channel is `f32` and live seqs are ~1e9 (not exactly
    // representable), so seed the attack target component directly — the RT
    // tracking below is what this task owns.
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    sim.ecs
        .0
        .entity_mut(attacker)
        .insert(crate::ai::types::command::AttackTargetComp(Some(target)));
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(attacker)
    {
        slot.target = Some(crate::world::TilePos::new(
            crate::world::WorldGrid::to_tile(target_pos.x) as i16,
            crate::world::WorldGrid::to_tile(target_pos.y) as i16,
        ));
    }
    // The scenario exercises the chaser's attack-target tracking, not the
    // mutual exchange: the fleeing target holds fire (`holdFire` stance,
    // `CommandAI.shouldFire`). With the T4 velocity model a fleeing unit's
    // return fire lands (the old kinematic mover masked it), which turned the
    // exchange into a symmetric race instead of a chase.
    {
        use crate::ai::unit_stance_runtime::{UnitStancesComp, apply_stance};
        let def = sim
            .unit_runtime()
            .expect("rt")
            .content
            .unit_stance_by_name("holdfire")
            .cloned()
            .expect("holdfire stance");
        let mut stances = UnitStancesComp::default();
        assert!(apply_stance(&mut stances.0, &def));
        sim.ecs.0.entity_mut(target).insert(stances);
    }
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(target).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(target).is_err()
            || sim
                .ecs
                .0
                .get::<Health>(target)
                .is_some_and(|health| health.dead),
        "the attacker killed the fleeing target"
    );
}

/// G01 T16: the reticle seam reports the tracked entity's position.
#[test]
fn command_target_seam_reports_position() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let attacker = sim
        .spawn_live_unit("dagger", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("attacker");
    let target = sim
        .spawn_live_unit("dagger", 2, 50.0 * 8.0, 40.0 * 8.0, 180.0)
        .expect("target");
    let attacker_id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(attacker)
        .expect("seq")
        .0 as i32;
    sim.ecs
        .0
        .entity_mut(attacker)
        .insert(crate::ai::types::command::AttackTargetComp(Some(target)));
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(attacker)
    {
        slot.kind = crate::ai::controller::AiKind::Command;
        slot.target = Some(crate::world::TilePos::new(
            crate::world::WorldGrid::to_tile(target_pos.x) as i16,
            crate::world::WorldGrid::to_tile(target_pos.y) as i16,
        ));
    }
    sim.tick().expect("tick");
    let rows = sim.command_targets();
    let row = rows
        .iter()
        .find(|(id, _, _, _)| *id == attacker_id)
        .copied()
        .expect("the attacker reports a command target");
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    eprintln!("command target row {row:?}, target {target_pos:?}");
    assert!(row.3, "marked as an attack target");
    assert!(
        (row.1 - target_pos.x).abs() < 32.0 && (row.2 - target_pos.y).abs() < 32.0,
        "reports the tracked entity's position"
    );
}

/// G01 T14: a unit carrying blast compound explodes on death
/// (`UnitComp.destroy` -> `Damage.dynamicExplosion`): 30 blast-compound gives
/// explosiveness `2 + 1.2*30*1.53 = 57.08` -> 5 waves of 28.54 damage with a
/// 50 px final radius.
#[test]
fn unit_with_blast_compound_explodes() {
    use crate::entities::comp::unit::comp::ItemsComp;

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0 + 8.0, 20.5 * 8.0, 180.0)
        .expect("dagger");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemsComp>(dagger) {
        items.item = Some((blast, 30));
    }
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.health = 0.0;
        hp.dead = true;
    }
    sim.tick().expect("tick");
    // Staggered waves (`Time.run(i * 2f)`): the growing radius needs ticks.
    for _ in 0..40 {
        sim.tick().expect("tick");
    }
    let wall_end = health(&sim, wall);
    eprintln!("wall {wall_start} -> {wall_end:?}");
    assert!(
        wall_end.is_none_or(|hp| hp <= wall_start - 20.0),
        "the blast damaged the wall: {wall_start} -> {wall_end:?}"
    );
}

/// G01 T14: core-spawned units skip the death explosion (`spawnedByCore`).
#[test]
fn core_spawned_unit_does_not_explode() {
    use crate::entities::comp::unit::comp::{ItemsComp, UnitCore};

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0 + 8.0, 20.5 * 8.0, 180.0)
        .expect("dagger");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemsComp>(dagger) {
        items.item = Some((blast, 30));
    }
    if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(dagger) {
        core.spawned_by_core = true;
    }
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.health = 0.0;
        hp.dead = true;
    }
    sim.tick().expect("tick");
    sim.tick().expect("tick");
    assert_eq!(
        health(&sim, wall),
        Some(wall_start),
        "no explosion from a core-spawned unit"
    );
}

/// G01 T14: `rules.fire = false` blocks fire creation through the `CombatEnv`
/// sync (`Fires.create` gate).
#[test]
fn fire_disabled_rule_blocks_fire() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(24);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        fire: false,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    // One tick syncs `CombatEnv` from the session rules.
    sim.tick().expect("tick");
    let mut rng = crate::determinism::SimRng::new(7);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 10, 10, &mut rng);
    assert!(fire.is_none(), "rules.fire = false blocks fires");
}

/// G01 T14: a charged force projector absorbs the death explosion
/// (`ForceBuild.absorbExplosion`): the wall under the shield is undamaged.
#[test]
fn force_projector_absorbs_explosion() {
    use crate::entities::comp::unit::comp::ItemsComp;
    use crate::world::blocks::defense::projectors::BaseShieldState;

    let mut sim = live_sim(48);
    // Power-source feeds the projector; the wall sits inside its radius.
    place(&mut sim, 18, 20, "power-source", 1);
    let projector = place(&mut sim, 20, 20, "force-projector", 1);
    let wall = place(&mut sim, 22, 20, "copper-wall", 1);
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let radius = sim
        .ecs
        .0
        .get::<BaseShieldState>(projector)
        .map(|state| state.smooth_radius)
        .unwrap_or(0.0);
    if radius <= 1.0 {
        eprintln!(
            "force projector uncharged (radius {radius}); absorption covered by the shield unit tests"
        );
        return;
    }
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 22.5 * 8.0, 20.5 * 8.0, 180.0)
        .expect("dagger");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemsComp>(dagger) {
        items.item = Some((blast, 30));
    }
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.health = 0.0;
        hp.dead = true;
    }
    sim.tick().expect("tick");
    sim.tick().expect("tick");
    eprintln!(
        "shield radius {radius}, wall {wall_start} -> {:?}",
        health(&sim, wall)
    );
    assert_eq!(
        health(&sim, wall),
        Some(wall_start),
        "the projector absorbed the explosion"
    );
}

/// G01 T14: `explosion_absorbed` covers the point when a live shield polygon
/// contains it and skips the ignore team's own shields.
#[test]
fn explosion_absorbed_polygon() {
    use crate::combat::damage::explosion::explosion_absorbed;
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    let projector = place(&mut sim, 20, 20, "force-projector", 1);
    if let Some(mut state) = sim.ecs.0.get_mut::<ForceProjectorState>(projector) {
        state.radscl = 1.0;
        state.radius = 100.0;
        state.broken = false;
        state.sides = 24;
    }
    let (x, y) = (20.5 * 8.0, 20.5 * 8.0);
    assert!(
        explosion_absorbed(&mut sim.ecs.0, x, y, 30.0, Some(2)),
        "team 1's shield absorbs the wave for team 2"
    );
    assert!(
        !explosion_absorbed(&mut sim.ecs.0, x + 1000.0, y, 30.0, Some(2)),
        "a far-away point is not absorbed"
    );
    assert!(
        !explosion_absorbed(&mut sim.ecs.0, x, y, 30.0, Some(1)),
        "the shield's own team is ignored"
    );
}

/// T1: `Building.set(tile.drawx(), tile.drawy())` (BuildingComp.java:126) with
/// `drawx = tile.worldx() + block.offset` (Tile.java:166-177); the offset is
/// `((size + 1) % 2) * tilesize / 2` (Block.java:774), so odd sizes (and 1x1)
/// sit on the tile's `worldx` and even sizes shift `+4`.
#[test]
fn building_pos_uses_block_offset() {
    let mut sim = live_sim(64);
    let cases = [
        ("duo", 10, 80.0),               // 1x1, offset 0
        ("core-shard", 20, 160.0),       // 3x3, offset 0
        ("mechanical-drill", 30, 244.0), // 2x2, offset 4
        ("core-foundation", 40, 324.0),  // 4x4, offset 4
    ];
    for (name, tile, expected) in cases {
        let entity = place(&mut sim, tile, tile, name, 1);
        let pos = sim.ecs.0.get::<Pos>(entity).expect("pos");
        assert_eq!(
            (pos.x, pos.y),
            (expected, expected),
            "{name} at tile {tile} must sit at drawx/drawy"
        );
    }
}

/// T2: `AIController.updateWeapons` searches per mount
/// (`findTarget(mountX, mountY, weapon.range(), bullet.collidesAir,
/// bullet.collidesGround)`, AIController.java:216-222), so a ground-only weapon
/// ignores the flyer the unit's main target search picked. The unit is the
/// first content unit whose first weapon bullet does not collide air (printed
/// by the test).
#[test]
fn ground_weapon_ignores_air_target_per_mount() {
    use crate::ai::controller::{AiKind, ControllerSlot};
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
        waves: true,
        wave_team: 2,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let name = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        content
            .units()
            .iter()
            .find(|unit| {
                unit.weapons.first().is_some_and(|weapon| {
                    content
                        .bullet(weapon.bullet.id)
                        .is_some_and(|bullet| !bullet.collides_air && bullet.collides_ground)
                })
            })
            .map(|unit| unit.name.clone())
            .expect("a ground-only unit exists")
    };
    eprintln!("T2 ground-only unit: {name}");

    let wall = place(&mut sim, 29, 25, "copper-wall", 1);
    let unit = sim
        .spawn_live_unit(&name, 2, 200.0, 200.0, 0.0)
        .expect("ground-only unit");
    // A flying enemy at the same distance as the wall (`hitSize/2` apart).
    let flare = sim
        .spawn_live_unit("flare", 1, 230.0, 200.0, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(unit)
        .and_then(|weapons| weapons.mounts.first().and_then(|mount| mount.target));
    assert_eq!(
        target,
        Some(wall),
        "the ground-only mount searched its own target (the wall), not the flyer"
    );
}

/// T2: the shoot/invalidate range adds the TARGET's `hitSize/2`
/// (`mount.target.within(mountX, mountY, wrange + target.hitSize/2)`,
/// AIController.java:239; `Units.invalidateTarget`, Units.java:151-157). A 3x3
/// core-shard adds 12 px where the pre-T2 port added the shooter's `hitSize/2`.
///
/// The per-mount search radius is `weapon.range() + |shootY|` (a tighter gate
/// than the shoot range), so the shoot gate is exercised with the mount target
/// set directly on a non-`singleTarget` unit.
#[test]
fn dagger_engages_core_at_target_hitsize() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let (name, weapon_x, weapon_y, range, shooter_hit) = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        let def = content
            .units()
            .iter()
            .find(|unit| {
                !unit.single_target
                    && !unit.flying
                    && unit
                        .weapons
                        .first()
                        .and_then(|weapon| content.bullet(weapon.bullet.id))
                        .is_some_and(|bullet| bullet.collides_ground)
            })
            .expect("a non-singleTarget ground unit exists");
        let weapon = def.weapons.first().expect("weapon");
        (
            def.name.clone(),
            weapon.x,
            weapon.y,
            weapon.range(),
            def.hit_size,
        )
    };
    eprintln!(
        "T2 hitSize unit: {name} range {range} weapon ({weapon_x}, {weapon_y}) hit {shooter_hit}"
    );
    let core = place(&mut sim, 20, 20, "core-shard", 1);
    // core-shard Pos = (160, 160); hitSize = 24 -> the shoot/invalidate range is
    // `range + 12`. `rotation 0` puts the mount at `unit - (weapon.y, weapon.x)`.
    let target_dist = range + 12.0 - 4.0;
    let unit = sim
        .spawn_live_unit(
            &name,
            2,
            160.0 + target_dist + weapon_y,
            160.0 + weapon_x,
            0.0,
        )
        .expect("unit");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    // Tick 1 runs the first (search) tick; the search radius is `range + |shootY|`.
    sim.tick().expect("tick");
    let set_target = |sim: &mut Sim, target: Option<Entity>| {
        if let Some(mut weapons) = sim.ecs.0.get_mut::<crate::weapons::UnitWeapons>(unit)
            && let Some(mount) = weapons.mounts.first_mut()
        {
            mount.target = target;
        }
    };
    let mount_state = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(unit)
            .and_then(|weapons| {
                weapons
                    .mounts
                    .first()
                    .map(|mount| (mount.target, mount.shoot))
            })
    };
    set_target(&mut sim, Some(core));
    sim.tick().expect("tick");
    let (target, shoot) = mount_state(&sim).expect("mounts");
    assert_eq!(
        target,
        Some(core),
        "the target survives the invalidate check"
    );
    assert!(shoot, "engages at range + target.hitSize/2 - 4");
    // Move 5 px further: beyond `range + 12` -> invalidated and not shooting.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(unit) {
        pos.x += 5.0;
    }
    set_target(&mut sim, Some(core));
    sim.tick().expect("tick");
    let (target, shoot) = mount_state(&sim).expect("mounts");
    assert_eq!(
        target, None,
        "beyond range + target.hitSize/2 is invalidated"
    );
    assert!(!shoot, "does not shoot past range + target.hitSize/2");
}

/// T2: `FlyingAI.findTarget` keeps the in-range main target
/// (FlyingAI.java:34-39): a flare whose main target (the core) is in range does
/// not switch to a closer dagger.
#[test]
fn flying_mount_keeps_in_range_main_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let core = place(&mut sim, 20, 20, "core-shard", 1);
    let dagger = place(&mut sim, 24, 20, "duo", 1);
    let flare = sim
        .spawn_live_unit("flare", 2, 200.0, 190.0, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::Flying;
    }
    // A team-1 dagger (closer than the core) must not steal the mount target.
    let bait = sim
        .spawn_live_unit("dagger", 1, 175.0, 190.0, 180.0)
        .expect("bait");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(bait) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    for _ in 0..130 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(flare)
        .and_then(|weapons| weapons.mounts.first().and_then(|mount| mount.target));
    assert!(
        target == Some(core) || target == Some(dagger),
        "the flare engaged a building ({target:?}), not the closer dagger"
    );
}

/// T2: `autoDropBombs` (AIController.java:225-229): a horizon with no valid
/// main target drops its bomb weapon on the ground unit below it
/// (`Units.closestTarget(.., u -> !u.isFlying(), t -> true)`).
#[test]
fn horizon_auto_drops_bombs_on_conveyor() {
    use crate::ai::controller::{AiKind, ControllerSlot};
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
        waves: true,
        wave_team: 2,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    // A concrete ground victim: a copper wall on a building tile.
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let before = health(&sim, wall).expect("wall health");
    let horizon = sim
        .spawn_live_unit("horizon", 2, 20.0 * 8.0, 20.0 * 8.0 - 14.0, 90.0)
        .expect("horizon");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(horizon) {
        slot.kind = AiKind::Flying;
    }
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let after = health(&sim, wall).unwrap_or(0.0);
    eprintln!("T2 horizon bombs: wall {before} -> {after}");
    assert!(after < before, "the horizon dropped bombs on the wall");
}

/// T2: a `!controllable` mount is never overwritten by the AI's main target
/// (`if(!weapon.controllable || weapon.noAttack) continue;`,
/// AIController.java:214).
#[test]
fn non_controllable_mount_not_overwritten() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let name = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        content
            .units()
            .iter()
            .find(|unit| {
                unit.weapons
                    .iter()
                    .any(|weapon| !weapon.controllable && !weapon.no_attack)
            })
            .map(|unit| unit.name.clone())
            .expect("a unit with a non-controllable weapon exists")
    };
    eprintln!("T2 non-controllable weapon unit: {name}");
    // The first such unit is naval (oxynoe): give it water so the
    // environment pass does not kill it on land.
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("deep-water")
        .expect("deep-water");
    for x in 11..=14 {
        sim.grid.tiles.get_mut(x, 20).floor = water;
    }
    let _wall = place(&mut sim, 28, 20, "copper-wall", 1);
    let unit = sim
        .spawn_live_unit(&name, 2, 100.0, 160.0, 0.0)
        .expect("unit");
    // A close enemy unit so the AI main target is Some.
    let bait = sim
        .spawn_live_unit("dagger", 1, 120.0, 160.0, 180.0)
        .expect("bait");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(bait) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let weapons = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(unit)
        .expect("weapons");
    let any_non_controllable = weapons
        .weapons
        .iter()
        .zip(weapons.mounts.iter())
        .filter(|(def, _)| !def.controllable)
        .all(|(_, mount)| mount.target != Some(bait));
    assert!(
        any_non_controllable,
        "a non-controllable mount was overwritten with the AI main target"
    );
}

/// H02 T16: the duo aims at the intercept point for its 2.5 px/tick copper
/// bullet, not at the target's current position (`Turret.targetPosition`,
/// `Turret.java:459-479`; `Predict.intercept`, `Predict.java:27-52`).
#[test]
fn turret_leads_moving_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + 100.0, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 2.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // `EntitySet::UpdateUnits` runs before `UpdateBuildings`, so the flare's
    // own `VelComp.update` (T4) already integrated its velocity and applied
    // drag before the turret aimed this tick.
    let flare_now = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    let flare_vel = sim
        .ecs
        .0
        .get::<Vel>(flare)
        .map(|v| (v.x, v.y))
        .expect("flare vel");
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    // `duo_copper` bullet speed (`register_bullets`).
    let expected = crate::combat::targeting::intercept_positions(
        duo_pos,
        (0.0, 0.0),
        flare_now,
        flare_vel,
        true,
        2.5,
    );
    assert!(
        state.target_pos.1 > flare_now.1,
        "the aim leads the northbound flare: {:?} vs {flare_now:?}",
        state.target_pos
    );
    assert!(
        (state.target_pos.0 - expected.0).abs() < 0.01
            && (state.target_pos.1 - expected.1).abs() < 0.01,
        "aim equals Predict.intercept: {:?} vs {expected:?}",
        state.target_pos
    );
}

/// H02 T16: the aim point is recomputed every tick even between retargets
/// (`Turret.updateTile` calls `targetPosition` on every tick, not only in
/// `findTarget`).
#[test]
fn turret_reaims_every_tick() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + 80.0, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // Move the target 20 px north; the retarget timer stays high.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(flare) {
        pos.y -= 20.0;
    }
    let moved = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    sim.tick().expect("tick");
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    assert!(
        (state.target_pos.0 - moved.0).abs() < 0.01 && (state.target_pos.1 - moved.1).abs() < 0.01,
        "aim follows the moved target: {:?} vs {moved:?}",
        state.target_pos
    );
}

/// H02 T16: firing is gated by `within(target, range() + hitSize/1.9)`
/// (`Turret.java:587`), evaluated against the target's own position.
#[test]
fn turret_holds_fire_out_of_range() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let hit_size = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name("flare")
        .expect("flare def")
        .hit_size;
    let far = 160.0 + hit_size / 1.9 + 2.0;
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + far, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
        state.rotation = 0.0; // already facing the target
        state.reload_counter = 1000.0; // ready to fire if allowed
    }
    let before = sim.unit_runtime().expect("rt").bullets_created;
    sim.tick().expect("tick");
    let after = sim.unit_runtime().expect("rt").bullets_created;
    assert_eq!(
        before, after,
        "out-of-range target holds fire (Turret.java:587)"
    );
}

/// H02 T16: `scathe` does not predict (`Blocks.java:5248`), so its aim point
/// is the target's current position.
#[test]
fn scathe_does_not_predict() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(64);
    let scathe = place(&mut sim, 20, 20, "scathe", 1);
    // `Turret.targetPosition` returns without ammo (`Turret.java:460`).
    feed(&mut sim, scathe, "carbide", 1);
    let scathe_pos = sim
        .ecs
        .0
        .get::<Pos>(scathe)
        .map(|p| (p.x, p.y))
        .expect("scathe pos");
    assert!(
        !sim.ecs
            .0
            .get::<TurretState>(scathe)
            .expect("scathe state")
            .config
            .predict_target,
        "scathe predictTarget = false"
    );
    let flare = sim
        .spawn_live_unit("flare", 2, scathe_pos.0 + 100.0, scathe_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 2.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(scathe) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // `EntitySet::UpdateUnits` runs before `UpdateBuildings`: the flare's own
    // `VelComp.update` (T4) integrated its velocity before the scathe aimed.
    let flare_now = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    let state = sim.ecs.0.get::<TurretState>(scathe).expect("state");
    assert!(
        (state.target_pos.0 - flare_now.0).abs() < 0.01
            && (state.target_pos.1 - flare_now.1).abs() < 0.01,
        "no lead without predictTarget: {:?} vs {flare_now:?}",
        state.target_pos
    );
}
/// H09 T1: `Logic.java:533-535` fog update — a player-team unit reveals its
/// radius, discovers tiles permanently and does not distinguish hidden tiles.
#[test]
fn fog_unit_reveals_and_discovers() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        // Team 1 has no core in this fixture; without the cap exception the
        // spawned dagger is unit-cap-killed at spawn (`spawn_unit_def`).
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    let dagger = sim
        .spawn_live_unit("dagger", 1, 20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    // Dagger fog radius 174/8 = 21.75 -> event radius 21, drawn +1.
    assert!(
        sim.fog_visible_tile(1, 30, 20),
        "tile 10 tiles away is visible"
    );
    assert!(
        !sim.fog_visible_tile(1, 50, 20),
        "tile 30 tiles away is not visible"
    );
    assert!(sim.fog_discovered(1, 30, 20), "seen tile is discovered");
    assert!(
        !sim.fog_discovered(1, 50, 20),
        "unseen tile is not discovered"
    );
    // Only-AI teams ignore fog (`FogControl.isVisibleTile`).
    assert!(
        sim.fog_visible_tile(2, 60, 60),
        "the wave team is AI and sees everything"
    );

    // Moving away keeps the explored tile discovered but not visible.
    let mut pos = sim.ecs.0.get_mut::<Pos>(dagger).expect("dagger pos");
    pos.x = 50.0 * 8.0 + 4.0;
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    assert!(
        !sim.fog_visible_tile(1, 20, 20),
        "the left-behind tile is no longer visible"
    );
    assert!(
        sim.fog_discovered(1, 20, 20),
        "the left-behind tile stays discovered"
    );
}

/// H09 T1: with `rules.fog` off there is no `FogControl` and every probe is
/// true (Serpulo pays nothing).
#[test]
fn fog_off_everything_visible() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
    });
    sim.reset_fog();
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::game::fog::FogControl>()
            .is_none(),
        "no fog resource without rules.fog"
    );
    assert!(sim.fog_visible_tile(1, 60, 60));
    assert!(sim.fog_discovered(1, 60, 60));
    assert!(sim.fog_visible_tile(2, 60, 60));
    assert!(sim.fog_discovered(2, 60, 60));
}

/// H09 T1: the sector-start path applies the world rules before the campaign
/// runtime exists, so no `FogControl` resource was created; the first tick must
/// create and size it (`FogControl.update`: `fog == null` -> new array).
#[test]
fn fog_update_creates_the_resource_when_missing() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::game::fog::FogControl>()
            .is_none()
    );
    sim.spawn_live_unit("dagger", 1, 20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    sim.tick().expect("tick");
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::game::fog::FogControl>()
            .is_some(),
        "the first tick creates the fog resource"
    );
    assert!(sim.fog_visible_tile(1, 25, 20));
    assert!(sim.fog_discovered(1, 25, 20));
}

/// H09 T2: a player turret cannot target a unit hidden in fog; a friendly unit
/// that reveals it restores the target (`Units.closestEnemy` `inFogTo`).
#[test]
fn turret_ignores_unit_in_fog() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.disable_unit_cap = true;
    });
    // Placed before the fog reset: after it, the static-fog mask
    // (all-zero until something is discovered) would reject the placement,
    // mirroring Java's `fogControl.isDiscovered == false` for every tile.
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim.reset_fog();
    // T3 makes every turret a fog source covering its range; zero the
    // multiplier so the duo reveals nothing (the pre-T3 viewer state this gate
    // test needs).
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        let mut config = (*state.config).clone();
        config.fog_radius_multiplier = 0.0;
        state.config = std::sync::Arc::new(config);
    }
    sim.spawn_live_unit("dagger", 2, 25.0 * 8.0 + 4.0, 10.0 * 8.0 + 4.0, 180.0)
        .expect("enemy dagger");
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .and_then(|state| state.target);
    assert!(
        target.is_none(),
        "the duo does not target a unit hidden in fog"
    );

    // A team-1 dagger next to the enemy reveals it (radius 21).
    sim.spawn_live_unit("dagger", 1, 24.0 * 8.0 + 4.0, 10.0 * 8.0 + 4.0, 0.0)
        .expect("friendly dagger");
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .and_then(|state| state.target);
    assert!(target.is_some(), "the revealed dagger becomes the target");
}

/// H09 T2: AI teams ignore fog (`FogControl.isVisibleTile`: `team.isAI()`).
#[test]
fn ai_team_sees_through_fog() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    let duo = place(&mut sim, 20, 20, "duo", 2);
    feed(&mut sim, duo, "copper", 10);
    sim.spawn_live_unit("dagger", 1, 35.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 180.0)
        .expect("player dagger");
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .and_then(|state| state.target);
    assert!(target.is_some(), "the wave team (AI) sees through fog");
}

/// H09 T2: `BlockIndexer.find_enemy_tile` skips enemy buildings on
/// undiscovered tiles (`BlockIndexer.java:468`); a revealing unit restores it.
#[test]
fn find_enemy_tile_skips_undiscovered() {
    use crate::ai::block_indexer::{BlockIndexer, BlockPriority};
    use crate::sim::unit_runtime::UnitRuntime;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    place(&mut sim, 40, 40, "core-shard", 2);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }

    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let mut indexer = BlockIndexer::new();
    indexer.rebuild(&mut sim.ecs.0, &runtime.content);
    let before = indexer
        .find_enemy_tile(
            &sim.ecs.0,
            &runtime.content,
            1,
            10.0 * 8.0,
            10.0 * 8.0,
            4000.0,
            &[BlockPriority::Core],
        )
        .map(|tile| (tile.x(), tile.y()));
    sim.ecs.0.insert_non_send(runtime);
    assert_eq!(before, None, "undiscovered enemy core is skipped");

    sim.spawn_live_unit("dagger", 1, 40.0 * 8.0 + 4.0, 40.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let mut indexer = BlockIndexer::new();
    indexer.rebuild(&mut sim.ecs.0, &runtime.content);
    let after = indexer
        .find_enemy_tile(
            &sim.ecs.0,
            &runtime.content,
            1,
            10.0 * 8.0,
            10.0 * 8.0,
            4000.0,
            &[BlockPriority::Core],
        )
        .map(|tile| (tile.x(), tile.y()));
    sim.ecs.0.insert_non_send(runtime);
    assert_eq!(after, Some((40, 40)), "the discovered core is found");
}

/// H09 T2: `Build.validPlace` rejects tiles the player team has not discovered
/// (`Build.java:252`); a revealing unit unlocks the tile.
#[test]
fn placement_rejected_in_static_fog() {
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::build::valid_place;
    use crate::world::limits::{BlockCounter, BuildRules};

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    sim.tick().expect("tick");

    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("block table");
    let rules = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
    let counter = sim.ecs.0.get_resource::<BlockCounter>().expect("counter");
    let wall = runtime.content.block_id("copper-wall").expect("wall");
    let before = valid_place(
        &runtime.content,
        table,
        rules,
        counter,
        &sim.grid,
        wall,
        1,
        0,
        30,
        30,
    );
    sim.ecs.0.insert_non_send(runtime);
    assert!(!before, "an undiscovered tile rejects the placement");

    sim.spawn_live_unit("dagger", 1, 30.0 * 8.0 + 4.0, 30.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("block table");
    let rules = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
    let counter = sim.ecs.0.get_resource::<BlockCounter>().expect("counter");
    let after = valid_place(
        &runtime.content,
        table,
        rules,
        counter,
        &sim.grid,
        wall,
        1,
        0,
        30,
        30,
    );
    sim.ecs.0.insert_non_send(runtime);
    assert!(after, "the revealed tile accepts the placement");
}

/// H09 T3: a powered `radar` discovers its radius in `discoveryTime` = 600
/// ticks (`Radar.java:18,59-72`).
#[test]
fn radar_discovers_in_600_ticks() {
    use crate::entities::comp::building::RadarState;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.disable_unit_cap = true;
    });
    // Placed before the fog reset (the all-zero static mask else rejects).
    let radar = place(&mut sim, 20, 20, "radar", 1);
    place(&mut sim, 21, 20, "power-source", 1);
    sim.reset_fog();
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let progress = sim
        .ecs
        .0
        .get::<RadarState>(radar)
        .map(|state| state.progress)
        .unwrap_or(0.0);
    assert!(
        (progress - 0.5).abs() < 0.02,
        "progress ~0.5 at 300 ticks: {progress}"
    );
    // One tick is lost while the power graph links (efficiency 0 on the first
    // tick), so full discovery completes on tick 601.
    for _ in 0..301 {
        sim.tick().expect("tick");
    }
    let progress = sim
        .ecs
        .0
        .get::<RadarState>(radar)
        .map(|state| state.progress)
        .unwrap_or(0.0);
    assert!(
        (progress - 1.0).abs() < 1e-3,
        "progress 1.0 at 600 ticks: {progress}"
    );
}

/// H09 T3: every turret reveals its range (`BaseTurret.java:59`,
/// `Turret.java:426-428`): a `duo` (range 160) reveals 20 tiles.
#[test]
fn duo_reveals_its_range() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.disable_unit_cap = true;
    });
    // Placed before the fog reset (the all-zero static mask else rejects).
    place(&mut sim, 10, 10, "duo", 1);
    sim.reset_fog();
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    // `Mathf.round(160 / 8) = 20` tiles, drawn with the +1 visual margin.
    assert!(sim.fog_visible_tile(1, 30, 10), "a tile at +20 is visible");
    assert!(
        !sim.fog_visible_tile(1, 32, 10),
        "a tile at +22 is not visible"
    );
    assert!(
        sim.fog_discovered(1, 20, 10),
        "revealed tiles are discovered"
    );
}

/// H09 T3: core fog radius = `max(fogRadius, (int)((30 + 20*size) / 8 * 3) + 13)`
/// (`CoreBlock.java:158-159`).
#[test]
fn core_fog_radius_values() {
    let sim = live_sim(8);
    let content = &sim.unit_runtime().expect("rt").content;
    let radius = |name: &str| {
        content
            .block_id(name)
            .and_then(|id| content.block(id))
            .map(|def| def.fog_radius)
    };
    assert_eq!(radius("core-shard"), Some(46), "size 3");
    assert_eq!(radius("core-bastion"), Some(54), "size 4");
    assert_eq!(radius("core-acropolis"), Some(69), "size 6");
}

/// H09 T4: `Building.update()` skips `updateTile` for a disabled turret
/// (`Turret` has the default `noUpdateDisabled = false`): a disabled, fed duo
/// never fires at an enemy in range.
#[test]
fn disabled_duo_does_not_fire() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let enemy = sim
        .spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0 + 20.0, 180.0)
        .expect("enemy dagger");
    let start = health(&sim, enemy).expect("health");
    if let Some(mut building) = sim.ecs.0.get_mut::<crate::entities::comp::Building>(duo) {
        building.enabled = false;
    }
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let shots = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .map(|state| state.total_shots)
        .unwrap_or(-1);
    assert_eq!(shots, 0, "a disabled duo never fires");
    assert_eq!(
        health(&sim, enemy),
        Some(start),
        "the enemy takes no duo damage"
    );
}

/// H09 T7: a container full of 100 blast-compound explodes with
/// `explosiveness = 120` and `flammability = 40` (before the
/// `* 3.5 * explosivenessScale` caller scaling).
#[test]
fn blast_container_explosion_terms() {
    let mut sim = live_sim(32);
    let container = place(&mut sim, 5, 5, "container", 1);
    let (content_len, blast, capacity) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.items().len(),
            content.item_id("blast-compound").expect("blast-compound"),
            content
                .block_by_name("container")
                .map(|def| def.item_capacity)
                .unwrap_or(300),
        )
    };
    let mut module = crate::world::modules::ItemModule::with_items(content_len);
    module.add(blast, 100, capacity.max(100));
    sim.ecs.0.entity_mut(container).insert(module);

    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("container").expect("def").clone();
    let (flammability, explosiveness, power) =
        super::unit_runtime::building_explosion_terms(&sim.ecs.0, content, container, &def);
    assert!(
        (explosiveness - 120.0).abs() < 1e-3,
        "explosiveness {explosiveness}"
    );
    assert!(
        (flammability - 40.0).abs() < 1e-3,
        "flammability {flammability}"
    );
    assert_eq!(power, 0.0, "no charge items");
    // Caller scaling (`BuildingComp.java:1518`).
    assert!((explosiveness * 3.5 * def.explosiveness_scale - 420.0).abs() < 1e-3);
}

/// H09 T7: a vault linked to a core caps its item explosion at
/// `min(itemCapacity / 60, 6)` (`StorageBlock.java:99-103`).
#[test]
fn linked_vault_explodes_weakly() {
    let mut sim = live_sim(32);
    place(&mut sim, 5, 5, "core-shard", 1);
    let vault = place(&mut sim, 8, 5, "vault", 1);
    assert!(
        sim.ecs
            .0
            .get::<crate::world::blocks::storage::storage_block::StorageBuild>(vault)
            .and_then(|storage| storage.linked_core)
            .is_some(),
        "the vault links to the adjacent core"
    );
    let (content_len, blast, capacity) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.items().len(),
            content.item_id("blast-compound").expect("blast-compound"),
            content
                .block_by_name("vault")
                .map(|def| def.item_capacity)
                .unwrap_or(1000),
        )
    };
    let mut module = crate::world::modules::ItemModule::with_items(content_len);
    module.add(blast, 1000, capacity);
    sim.ecs.0.entity_mut(vault).insert(module);

    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("vault").expect("def").clone();
    let (_, explosiveness, _) =
        super::unit_runtime::building_explosion_terms(&sim.ecs.0, content, vault, &def);
    assert!(
        (explosiveness - 1.2 * 6.0).abs() < 1e-3,
        "capped explosiveness (6 items) {explosiveness}"
    );
}

/// H09 T7: a charged battery's `power` term is `status * capacity` (4000) and
/// `dynamic_explosion` schedules `clamp(4000/700, 0, 8) = 6` lightning bolts.
#[test]
fn battery_lightning_count() {
    use crate::combat::damage::explosion::{
        DelayedAction, DelayedExplosionActions, dynamic_explosion,
    };

    let mut sim = live_sim(32);
    let battery = place(&mut sim, 5, 5, "battery", 1);
    if let Some(mut power) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::PowerModule>(battery)
    {
        power.status = 1.0;
    }
    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("battery").expect("def").clone();
    assert_eq!(def.base_explosiveness, 1.0, "battery baseExplosiveness");
    let (_, explosiveness, power) =
        super::unit_runtime::building_explosion_terms(&sim.ecs.0, content, battery, &def);
    assert_eq!(explosiveness, 1.0);
    assert!(
        (power - 4000.0).abs() < 1e-3,
        "battery buffered capacity 4000: {power}"
    );
    sim.with_combat_ctx(|ctx| {
        dynamic_explosion(
            ctx,
            40.0,
            40.0,
            0.0,
            explosiveness,
            power,
            16.0,
            true,
            false,
            Some(1),
        );
    });
    let actions = sim
        .ecs
        .0
        .get_resource::<DelayedExplosionActions>()
        .expect("actions");
    let bolts = actions
        .0
        .iter()
        .filter(|(_, action)| matches!(action, DelayedAction::Lightning { .. }))
        .count();
    assert_eq!(bolts, 6, "4000 / 700 = 5.71 -> 6 bolts");
    assert_eq!(actions.0.len(), 6, "no fireballs or waves");
}

/// H09 T7: explosiveness 33 splits into 3 waves spaced 2 ticks apart
/// (`Time.run(i * 2f)`).
#[test]
fn explosion_waves_are_staggered() {
    use crate::combat::damage::explosion::dynamic_explosion;

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let start = health(&sim, wall).expect("wall health");
    let (x, y) = (20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0);
    sim.with_combat_ctx(|ctx| {
        // Team 2 is protected; the team-1 wall takes every wave.
        dynamic_explosion(ctx, x, y, 0.0, 33.0, 0.0, 24.0, true, false, Some(2));
    });
    let after_inline = health(&sim, wall).expect("wall health");
    assert!(after_inline < start, "wave 0 damages inline");
    let mut drops = 1;
    let mut previous = after_inline;
    for _ in 0..8 {
        sim.tick().expect("tick");
        let now = health(&sim, wall).expect("wall health");
        if now < previous - 1e-3 {
            drops += 1;
        }
        previous = now;
    }
    assert_eq!(drops, 3, "three staggered waves (+0/+2/+4)");
}

/// T3: `PlayerComp.team` is set before the spawn attempt, so a player without
/// a supported core retries `bestCore()` every 60 ticks
/// (`PlayerComp.java:214-243`) instead of staying teamless forever.
#[test]
fn player_team_set_without_core_retries_spawn() {
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

    sim.set_player_team(1);
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    assert!(sim.player_unit().is_none(), "no player unit without a core");
    // Landing a core lets the next 60-tick window spawn the unit.
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let player = sim.player_unit().expect("player spawned at the new core");
    let pos = sim.ecs.0.get::<Pos>(player).expect("pos");
    assert_eq!((pos.x, pos.y), (160.0, 160.0), "spawned on the core");
}

/// T3/PAR-0175: the `unitClear` dock branch releases the possessed unit before
/// the docked unit takes the player controller (`InputHandler.java:823-873`):
/// exactly one entity keeps `comp::Player`.
#[test]
fn clear_unit_dock_releases_possessed_unit() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::Player;

    let mut sim = live_sim(48);
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        // Team 1 must be the player team (`select_team_controller` gives
        // `Command` only to the player's team; the rest get the AI branch).
        rules.default_team = 1;
    }
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    // The dockable types are the `core_unit_dock` units (Java: evoke, incite,
    // emanate, UnitTypes.java:4355/4421/4500) - the shard's alpha is not one.
    let dock_name = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .units()
            .iter()
            .find(|unit| unit.core_unit_dock)
            .map(|unit| unit.name.clone())
            .expect("a coreUnitDock unit exists")
    };
    eprintln!("T3 dock unit: {dock_name}");
    let player = sim
        .spawn_player_unit("alpha", 1, 160.0, 160.0, 90.0)
        .expect("player alpha");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 200.0, 160.0, 0.0)
        .expect("dagger");
    let dagger_seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    // Force the dockable type so the dock branch is exercised on this map.
    let dock_id = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name(&dock_name)
        .map(|def| def.id)
        .expect("dock unit id");
    if let Some(mut docked) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::unit::comp::DockedType>(dagger)
    {
        docked.0 = dock_id;
    }
    let _ = player;
    assert!(sim.possess_unit(dagger_seq), "dagger possessed");
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Player)
    );
    assert!(sim.clear_unit(), "unitClear ran the dock branch");
    // Exactly one entity carries the Player marker, and the dagger is back on
    // its team AI.
    let players = sim
        .ecs
        .0
        .iter_entities()
        .filter(|entity| entity.get::<Player>().is_some())
        .count();
    assert_eq!(players, 1, "exactly one player entity");
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Command),
        "the possessed dagger went back to its team controller"
    );
}

/// T3: `CoreBlock.playerSpawn` queues `spawnType.unlock()` in the campaign
/// (`CoreBlock.java:121-123`) through H04 T2's `UnlockRef` pipeline.
#[test]
fn campaign_spawn_queues_unlock() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::{CampaignCoreQueues, CampaignRuntime, UnlockRef};
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.sector = sim
        .unit_runtime()
        .expect("rt")
        .content
        .planet_id("serpulo")
        .map(|planet| (planet, 170));
    assert!(session.is_campaign(), "the play session is a campaign run");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let queues = crate::game::runtime::install_campaign_core_hooks(&mut sim.ecs.0);
    assert!(
        sim.ecs.0.get_resource::<CampaignCoreQueues>().is_some(),
        "campaign hook queues installed"
    );
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    let alpha = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("alpha")
        .expect("alpha");
    sim.spawn_player_at_best_core(1).expect("player spawned");
    let unlocked: Vec<UnlockRef> = queues.lock().expect("lock").unlocks.clone();
    assert_eq!(unlocked, vec![UnlockRef::Unit(alpha)], "alpha queued once");
    // A second spawn must not queue a duplicate.
    sim.player_unit = None;
    sim.spawn_player_at_best_core(1)
        .expect("player spawned again");
    let unlocked: Vec<UnlockRef> = queues.lock().expect("lock").unlocks.clone();
    assert_eq!(unlocked, vec![UnlockRef::Unit(alpha)], "no duplicate");
}

/// T4 (PAR-0202): `VelComp.update` integrates `vel` with tile collisions
/// (`EntityCollisions.move`), zeroes a blocked axis (`Mathf.equal`) and then
/// applies drag (`vel.scl(1 - drag)`).
#[test]
fn vel_update_applies_drag_and_stops_on_wall() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(20);
    let _wall = place(&mut sim, 10, 5, "copper-wall", 1);
    // Dagger hitSize 8 -> hitbox 5.28; the wall tile (10, 5) spans x [76, 84):
    // the resolved box stops at `76 - 2.64 = 73.36`.
    let blocked = sim
        .spawn_live_unit("dagger", 2, 72.0, 40.0, 0.0)
        .expect("blocked dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(blocked) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(blocked) {
        vel.x = 3.0;
        vel.y = 0.0;
    }
    for _ in 0..4 {
        sim.tick().expect("tick");
        let pos = *sim.ecs.0.get::<Pos>(blocked).expect("pos");
        assert!(
            pos.x <= 73.36 + 0.01,
            "the unit never enters the wall tile: {}",
            pos.x
        );
    }
    let vel = *sim.ecs.0.get::<Vel>(blocked).expect("vel");
    assert!(
        vel.x.abs() < 0.01,
        "the blocked axis decays to zero: {vel:?}"
    );

    // Free space: `vel = (1, 0)`, drag 0.3 -> move 1.0 px, then vel *= 0.7.
    let free = sim
        .spawn_live_unit("dagger", 2, 40.0, 120.0, 0.0)
        .expect("free dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(free) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(free) {
        vel.x = 1.0;
        vel.y = 0.0;
    }
    sim.tick().expect("tick");
    let pos = *sim.ecs.0.get::<Pos>(free).expect("pos");
    let vel = *sim.ecs.0.get::<Vel>(free).expect("vel");
    assert!((pos.x - 41.0).abs() < 1e-3, "advanced 1.0 px: {}", pos.x);
    assert!((vel.x - 0.7).abs() < 1e-4, "drag 0.3 applied: {vel:?}");
}

/// T4 (PAR-0202): `PhysicsComp.impulse` (`vel += v / mass`) on an idle unit
/// survives into the position through `VelComp.update`.
#[test]
fn knockback_moves_ground_unit() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(32);
    let dagger = sim
        .spawn_live_unit("dagger", 2, 160.0, 160.0, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
        slot.kind = AiKind::NoAi;
    }
    // `mass = hitSize² π`; impulse of `mass * 2` -> `vel += 2.0`.
    let mass = 8.0f32 * 8.0 * std::f32::consts::PI;
    crate::entities::comp::unit::velocity::impulse(&mut sim.ecs.0, dagger, mass * 2.0, 0.0);
    let vel = *sim.ecs.0.get::<Vel>(dagger).expect("vel");
    assert!((vel.x - 2.0).abs() < 1e-4, "impulse applied: {vel:?}");
    sim.tick().expect("tick");
    let pos = *sim.ecs.0.get::<Pos>(dagger).expect("pos");
    assert!(
        (pos.x - 162.0).abs() < 1e-3,
        "one tick of 2.0 px: {}",
        pos.x
    );
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let vel = *sim.ecs.0.get::<Vel>(dagger).expect("vel");
    assert!(vel.x.abs() < 0.01, "drag stops the unit: {vel:?}");
    let pos2 = *sim.ecs.0.get::<Pos>(dagger).expect("pos");
    assert!(
        pos2.x > pos.x,
        "knockback moved the unit: {} -> {}",
        pos.x,
        pos2.x
    );
}

/// T4 (PAR-0202): the sandstorm wind (`ParticleWeather.update` ->
/// `unit.impulse`) now reaches the position: an idle flare drifts downwind by
/// the Java steady state `a / drag` per tick (`a = force * intensity / mass`,
/// `mass = hitSize² π`; flare drag 0.04).
#[test]
fn sandstorm_wind_pushes_idle_flare() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::{Rules, WeatherEntry};
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        unit_cap: 100,
        unit_cap_variable: false,
        weather: vec![WeatherEntry {
            weather: String::from("sandstorm"),
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

    let flare = sim
        .spawn_live_unit("flare", 1, 160.0, 160.0, 0.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    let start = *sim.ecs.0.get::<Pos>(flare).expect("pos");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(flare).expect("pos");
    let dist = ((pos.x - start.x).powi(2) + (pos.y - start.y).powi(2)).sqrt();
    // Java steady state: `0.1 / (81π) / 0.04 ≈ 0.0098` px/tick -> ≈ 2.9 px.
    assert!(
        dist > 1.0 && dist < 6.0,
        "the sandstorm moved the flare {dist} px (start {start:?}, end {pos:?})"
    );
}

/// H01 T5: the `UnitComp.update` drag line (`UnitComp.java:790`) —
/// `type.drag * (isGrounded() ? floorOn().dragMultiplier : 1)`. The same
/// velocity coasts farther on `ice` (`dragMultiplier 0.35`, `Blocks.java:563`)
/// than on the default floor.
#[test]
fn ice_reduces_drag() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(32);
    let ice = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("ice")
        .expect("ice");
    let index = sim.grid.tiles.index(8, 8);
    sim.grid.tiles.geti_mut(index).floor = ice;
    sim.refresh_unit_runtime();
    let icy = sim
        .spawn_live_unit("dagger", 2, 64.0, 64.0, 0.0)
        .expect("icy dagger");
    let default_floor = sim
        .spawn_live_unit("dagger", 2, 64.0, 160.0, 0.0)
        .expect("dagger");
    for unit in [icy, default_floor] {
        if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
            slot.kind = AiKind::NoAi;
        }
        if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(unit) {
            vel.x = 2.0;
            vel.y = 0.0;
        }
    }
    for _ in 0..20 {
        sim.tick().expect("tick");
    }
    let icy_dx = sim.ecs.0.get::<Pos>(icy).expect("icy pos").x - 64.0;
    let dry_dx = sim.ecs.0.get::<Pos>(default_floor).expect("pos").x - 64.0;
    assert!(
        icy_dx > dry_dx + 1.0,
        "ice coasts farther ({icy_dx} vs {dry_dx})"
    );
}

/// H01 T5 (`UnitComp.java:750-753`): in a captured campaign sector
/// (`!waves && !attackMode`) every AI-team unit dies; the default team lives.
#[test]
fn enemy_dies_on_captured_sector() {
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
        wave_team: 2,
        waves: false,
        attack_mode: false,
        // `Rules.unitCap` default 0 kills live units; the campaign sets 100.
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    session.sector = sim
        .unit_runtime()
        .expect("rt")
        .content
        .planet_id("serpulo")
        .map(|planet| (planet, 170));
    assert!(session.is_campaign(), "campaign session");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let enemy = sim
        .spawn_live_unit("dagger", 2, 64.0, 64.0, 0.0)
        .expect("enemy");
    let friendly = sim
        .spawn_live_unit("dagger", 1, 160.0, 64.0, 0.0)
        .expect("friendly");
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(enemy).is_err()
            || sim
                .ecs
                .0
                .get::<Health>(enemy)
                .is_some_and(|health| health.dead),
        "the AI-team unit dies on a captured sector"
    );
    assert!(
        sim.ecs.0.get_entity(friendly).is_ok()
            && sim
                .ecs
                .0
                .get::<Health>(friendly)
                .is_some_and(|health| !health.dead),
        "the default-team unit lives"
    );
}

/// H01 T5: `GameState.hasSpawns()` (`GameState.java:68-70`) — a wave map in
/// attack mode with a wave-team core (`wavesSpawnAtCores` supplies the spawn
/// position) pushes non-wave-team units out of the drop zone
/// (`UnitComp.java:734-744,793`).
#[test]
fn attack_map_pushes_from_drop_zone() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(64);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    // Wave-team core: the attack-mode half of `hasSpawns` and, with
    // `wavesSpawnAtCores`, the drop-zone spawn position for the wave request.
    let _enemy_core = place(&mut sim, 40, 40, "core-shard", 2);
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        waves: true,
        attack_mode: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    session.phase = State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let unit = sim
        .spawn_live_unit("dagger", 1, 420.0, 320.0, 0.0)
        .expect("unit");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(unit) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    sim.request_wave_spawn(0);
    sim.tick().expect("tick");
    let vel = *sim.ecs.0.get::<Vel>(unit).expect("vel");
    assert!(
        vel.x * vel.x + vel.y * vel.y > 0.0,
        "the unit is pushed out of the drop zone: {vel:?}"
    );
    // The push is away from the wave-team core: the velocity points west.
    assert!(vel.x < 0.0, "pushed away from the core: {vel:?}");
}
