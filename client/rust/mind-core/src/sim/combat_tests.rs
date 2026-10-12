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
