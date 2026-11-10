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

mod turrets_bullets;
mod motion_payload;
mod deaths;
mod commands_ai;
mod fire_weather;
mod payload_weapons;
mod multipliers_player;
mod pathing_mining;
mod environment;
mod command_explosion;
mod defense_blocks;
mod targeting;
mod fog;
mod building_explosions;
mod player_campaign;

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
///
/// T9 note: the unit runs `NoAI` so the only velocity source is the drop-zone
/// push (the pre-T9 form let the ground controller's own walk obscure the
/// assertion).
#[test]
fn attack_map_pushes_from_drop_zone() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ecs::TeamId;
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
    // Wave-team core: the attack-mode half of `hasSpawns()` and, with
    // `wavesSpawnAtCores`, a spawn position stepped toward the player core.
    let enemy_core = place(&mut sim, 40, 40, "core-shard", 2);
    // A player core is required for the wave spawner's core step
    // (`WaveSpawner.eachGroundSpawn` steps toward `playerCore()`).
    let player_core = place(&mut sim, 30, 40, "core-shard", 1);
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
    session
        .teams
        .register_core(player_core, TeamId(1), &session.rules);
    session
        .teams
        .register_core(enemy_core, TeamId(2), &session.rules);
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
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let vel = *sim.ecs.0.get::<Vel>(unit).expect("vel");
    let pos = *sim.ecs.0.get::<Pos>(unit).expect("pos");
    let spawn = sim
        .unit_runtime()
        .and_then(|runtime| runtime.wave_spawner.get_spawns().first().copied())
        .expect("core spawn position");
    let (sx, sy) = (spawn.x() as f32 * 8.0, spawn.y() as f32 * 8.0);
    assert!(
        vel.x * vel.x + vel.y * vel.y > 0.0,
        "the unit is pushed out of the drop zone: {vel:?}"
    );
    // The push points away from the spawn tile (the unit sits east of the
    // wave-team core, so the velocity has a positive x component).
    assert!(
        vel.x * (pos.x - sx) + vel.y * (pos.y - sy) > 0.0,
        "pushed away from the spawn tile ({sx}, {sy}): {vel:?} from {pos:?}"
    );
}

/// H01 T6: `UnitComp.killed` (`UnitComp.java:980-991`) — a dead flying unit
/// with `createWreck` keeps existing as a falling wreck (`Falling`) and only
/// runs the destroy body once grounded (`UnitComp.update`,
/// `UnitComp.java:803-830`, `fallSpeed = 0.018`, `wreckHealthMultiplier = 0.25`).
#[test]
fn dead_flare_falls_then_crashes() {
    let mut sim = live_sim(64);
    let flare = sim
        .spawn_live_unit("flare", 2, 100.0, 100.0, 0.0)
        .expect("flare");
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 2.0;
    }
    {
        let mut health = sim.ecs.0.get_mut::<Health>(flare).expect("health");
        health.health = 0.0;
        health.dead = true;
    }
    let x0 = sim.ecs.0.get::<Pos>(flare).expect("pos").x;
    // Tick 1: `resolve_deaths` starts the fall (`Falling`) instead of
    // destroying the unit.
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(flare).is_ok(),
        "the wreck exists after death"
    );
    assert!(
        sim.ecs.0.get::<Health>(flare).is_some_and(|h| h.dead),
        "the wreck is dead"
    );
    // `elevation 1 / fallSpeed 0.018` = 56 ticks of fall (ceil(1/0.018)).
    let mut ticks = 1usize;
    let mut last_x = x0;
    while sim.ecs.0.get_entity(flare).is_ok() && ticks < 120 {
        if let Some(pos) = sim.ecs.0.get::<Pos>(flare) {
            last_x = pos.x;
        }
        sim.tick().expect("tick");
        ticks += 1;
        if let Some(pos) = sim.ecs.0.get::<Pos>(flare) {
            last_x = pos.x;
        }
    }
    assert!(
        sim.ecs.0.get_entity(flare).is_err(),
        "the wreck is destroyed after landing"
    );
    assert!(
        (55..=58).contains(&ticks),
        "landed after ~56 fall ticks (got {ticks})"
    );
    assert!(
        last_x - x0 > 2.0,
        "the wreck kept moving while falling ({x0} -> {last_x})"
    );
}

/// H01 T6: a grounded unit without `createWreck` still dies through the
/// destroy body on the death tick (`UnitComp.killed`: `!type.flying` ->
/// `destroy()`).
#[test]
fn ground_unit_dies_instantly() {
    let mut sim = live_sim(64);
    let dagger = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    {
        let mut health = sim.ecs.0.get_mut::<Health>(dagger).expect("health");
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "the ground unit is destroyed on the death tick"
    );
}

/// H01 T6: `UnitComp.update`: `health <= -maxHealth * wreckHealthMultiplier`
/// destroys the falling wreck immediately (`UnitComp.java:827`).
#[test]
fn wreck_destroyed_by_overkill() {
    let mut sim = live_sim(64);
    let flare = sim
        .spawn_live_unit("flare", 2, 100.0, 100.0, 0.0)
        .expect("flare");
    {
        let mut health = sim.ecs.0.get_mut::<Health>(flare).expect("health");
        health.health = 0.0;
        health.dead = true;
    }
    // Tick 1 starts the fall.
    sim.tick().expect("tick");
    assert!(sim.ecs.0.get_entity(flare).is_ok(), "falling");
    let max_health = {
        let runtime = sim.unit_runtime().expect("rt");
        let type_id = sim
            .ecs
            .0
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(flare)
            .expect("type")
            .type_id;
        runtime.content.unit(type_id).expect("flare def").health
    };
    {
        let mut health = sim.ecs.0.get_mut::<Health>(flare).expect("health");
        health.health = -max_health * 0.25 - 1.0;
    }
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(flare).is_err(),
        "overkill destroys the wreck mid-fall"
    );
}

/// H01 T9: `DefenderAI.findTarget` (`DefenderAI.java:26-38`) — an oct follows
/// the healthiest player-controllable ally in `max(range, 400)` (key
/// `-maxHealth + dst2 / 6400`), keeping `targetHit/2*1.1 + ownHit/2 + 15` px.
#[test]
fn oct_follows_healthiest_ally() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let oct = sim
        .spawn_live_unit("oct", 2, 100.0, 100.0, 0.0)
        .expect("oct");
    let _dagger = sim
        .spawn_live_unit("dagger", 2, 150.0, 100.0, 0.0)
        .expect("dagger");
    let mace = sim
        .spawn_live_unit("mace", 2, 300.0, 100.0, 0.0)
        .expect("mace");
    for _ in 0..1500 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let opos = *sim.ecs.0.get::<Pos>(oct).expect("oct alive");
    let mpos = *sim.ecs.0.get::<Pos>(mace).expect("mace alive");
    let keep = hit(&sim, mace) / 2.0 * 1.1 + hit(&sim, oct) / 2.0 + 15.0;
    let dst = ((mpos.x - opos.x).powi(2) + (mpos.y - opos.y).powi(2)).sqrt();
    assert!(
        dst <= keep + 10.0,
        "oct within keep {keep} + 10 of the mace (dist {dst}, oct {opos:?}, mace {mpos:?})"
    );
}

/// H01 T9: `FlyingFollowAI` (`FlyingFollowAI.java:26-29`) — a wave-team quell
/// follows the healthiest allied non-follower.
#[test]
fn quell_follows_ally_on_wave_team() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let quell = sim
        .spawn_live_unit("quell", 2, 60.0, 100.0, 0.0)
        .expect("quell");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 200.0, 100.0, 0.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let qpos = *sim.ecs.0.get::<Pos>(quell).expect("quell alive");
    let dpos = *sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let keep = hit(&sim, dagger) / 2.0 * 1.1 + hit(&sim, quell) / 2.0 + 15.0;
    let dst = ((dpos.x - qpos.x).powi(2) + (dpos.y - qpos.y).powi(2)).sqrt();
    assert!(
        dst <= keep + 10.0,
        "quell within keep {keep} + 10 of the dagger (dist {dst})"
    );
}

/// H01 T9: `FlyingFollowAI.useFallback()` — a player-team quell runs `FlyingAI`
/// (toward the enemy core) instead of following the ally.
#[test]
fn quell_uses_flying_ai_on_player_team() {
    let mut sim = live_sim(64);
    let quell = sim
        .spawn_live_unit("quell", 1, 60.0, 100.0, 0.0)
        .expect("quell");
    let _ally = sim
        .spawn_live_unit("dagger", 1, 200.0, 100.0, 0.0)
        .expect("dagger");
    let core = place(&mut sim, 30, 30, "core-shard", 2);
    let core_pos = *sim.ecs.0.get::<Pos>(core).expect("core pos");
    let start = *sim.ecs.0.get::<Pos>(quell).expect("quell pos");
    let start_dst = ((core_pos.x - start.x).powi(2) + (core_pos.y - start.y).powi(2)).sqrt();
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(quell).expect("quell alive");
    let dst = ((core_pos.x - pos.x).powi(2) + (core_pos.y - pos.y).powi(2)).sqrt();
    assert!(
        dst < start_dst - 30.0,
        "the player-team quell flew toward the enemy core ({start_dst} -> {dst})"
    );
}

/// H01 T9: `HugAI.updateMovement` (`HugAI.java:16-61`) — a renale closes on an
/// in-range enemy until the contact circle
/// `(unit.hitSize + target.hitSize) * 0.5`.
#[test]
fn renale_hugs_target() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let range = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name("renale")
        .expect("renale")
        .range;
    let renale = sim
        .spawn_live_unit("renale", 2, 150.0, 100.0, 0.0)
        .expect("renale");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 150.0 + range * 0.8, 100.0, 180.0)
        .expect("dagger");
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let rpos = *sim.ecs.0.get::<Pos>(renale).expect("renale alive");
    let dpos = *sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let contact = (hit(&sim, renale) + hit(&sim, dagger)) * 0.5;
    let dst = ((dpos.x - rpos.x).powi(2) + (dpos.y - rpos.y).powi(2)).sqrt();
    assert!(
        dst <= contact + 2.0,
        "renale reached the contact circle {contact} (dist {dst})"
    );
}

/// H01 T9: `NoAI.updateUnit(){}` (`NoAI.java`) — the dummy never moves.
#[test]
fn dummy_never_moves() {
    let mut sim = live_sim(64);
    let dummy = sim
        .spawn_live_unit("dummy", 2, 100.0, 100.0, 0.0)
        .expect("dummy");
    let start = *sim.ecs.0.get::<Pos>(dummy).expect("dummy pos");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(dummy).expect("dummy alive");
    assert_eq!((pos.x, pos.y), (start.x, start.y), "the dummy never moves");
}

/// H01 T11: `GroundAI` stuck bookkeeping (`GroundAI.java:24,63-85`): a unit
/// that has `moved` true but cannot leave a 12 px window accumulates
/// `stuckTime` past `stuckThreshold = max(1, tilesize * 1.5 * 2 / speed)`
/// (48 ticks for a dagger), then requests flowfield avoidance: the process
/// activates (`AvoidanceProcess.java:40-48`) and its grid marks every ground
/// AI unit's tile (`:52-106`). The "different next tile" selection itself is
/// covered by `get_target_tile_skips_avoided_neighbour`.
#[test]
fn stuck_dagger_activates_avoidance_in_a_pocket() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::ground::GroundAiState;
    use crate::ecs::EntitySeq;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::WorldGrid;

    let mut sim = live_sim(64);
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

    // The enemy core and a stationary same-team unit elsewhere on the map.
    place(&mut sim, 48, 31, "core-shard", 1);
    let blocker = sim
        .spawn_live_unit("dagger", 2, 24.0 * 8.0, 24.0 * 8.0, 0.0)
        .expect("blocker");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(blocker) {
        slot.kind = AiKind::NoAi;
    }
    // A 1-tile pocket: the follower is walled in on all four sides, so
    // `should_move` stays true while its position cannot change.
    for (x, y) in [(20, 30), (20, 32), (19, 31), (21, 31)] {
        place(&mut sim, x, y, "copper-wall", 2);
    }
    let follower = sim
        .spawn_live_unit("dagger", 2, 20.0 * 8.0, 31.0 * 8.0, 0.0)
        .expect("follower");
    let blocker_id = sim
        .ecs
        .0
        .get::<EntitySeq>(blocker)
        .map_or(0, |seq| (seq.0 & 0x7fff_ffff) as i32);
    let width = sim.unit_runtime().expect("rt").grid.width();
    let start = *sim.ecs.0.get::<Pos>(follower).expect("follower pos");

    let mut max_stuck = 0.0f32;
    let mut activated_at = None;
    let mut marked = false;
    for tick in 0..300 {
        sim.tick().expect("tick");
        if let Some(state) = sim.ecs.0.get::<GroundAiState>(follower) {
            max_stuck = max_stuck.max(state.stuck_time);
        }
        if activated_at.is_none() && sim.unit_runtime().is_some_and(|rt| rt.avoidance_active) {
            activated_at = Some(tick);
        }
        let bpos = *sim.ecs.0.get::<Pos>(blocker).expect("blocker alive");
        if let Some(rt) = sim.unit_runtime() {
            let bx = WorldGrid::to_tile(bpos.x);
            let by = WorldGrid::to_tile(bpos.y);
            if rt.avoidance.get((bx + by * width) as usize).copied() == Some(i32::MAX - blocker_id)
            {
                marked = true;
            }
        }
    }
    // `stuckThreshold = max(1, tilesize * 1.5 * 2 / speed)` = 48 for a dagger.
    assert!(
        max_stuck >= 48.0,
        "the stuck time crossed the threshold (max {max_stuck})"
    );
    assert!(
        activated_at.is_some(),
        "the flowfield avoidance grid activated after the jam"
    );
    assert!(marked, "the blocker's tile is marked on the avoidance grid");
    // Walled in: the follower never left its tile.
    let end = *sim.ecs.0.get::<Pos>(follower).expect("follower alive");
    assert_eq!(
        (end.x, end.y),
        (start.x, start.y),
        "the pocketed follower stayed put"
    );
    // The blocker never moved.
    let bpos = *sim.ecs.0.get::<Pos>(blocker).expect("blocker alive");
    assert_eq!(
        (bpos.x, bpos.y),
        (24.0 * 8.0, 24.0 * 8.0),
        "the NoAI blocker stayed put"
    );
}

/// H02 T12: `Wall.WallBuild.collision` (`Wall.java:177-207`) — a phase wall
/// deflects a share of the bullets that hit it (chance `chanceDeflect /
/// bullet.damage`) and steals the deflected bullet for its own team.
#[test]
fn phase_wall_reflects_low_damage_bullet() {
    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "phase-wall", 1);
    // Keep the wall alive across the volley so every bullet rolls.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    // Dagger blaster clone re-damaged to 20 so `chanceDeflect = 10f` is Java's
    // 0.5 roll (`Blocks.java:1761`) instead of a certainty (the vanilla dagger
    // bullet deals 9).
    let base = {
        let runtime = sim.unit_runtime().expect("rt");
        let dagger = runtime.content.unit_by_name("dagger").expect("dagger");
        let bullet = dagger.weapons[0].bullet.id;
        runtime.content.bullet(bullet).expect("bullet def").clone()
    };
    let mut def = base;
    def.damage = 20.0;
    let def = sim
        .unit_runtime_mut()
        .expect("rt")
        .content
        .add_bullet(def)
        .expect("add bullet");
    let (wx, wy) = (20.0 * 8.0, 20.0 * 8.0);
    let mut spawned: Vec<Entity> = Vec::new();
    sim.with_combat_ctx(|ctx| {
        for i in 0..40 {
            let spawn = crate::combat::BulletSpawn {
                def,
                team: 2,
                x: wx + 6.0,
                y: wy - 3.0 + 6.0 * ((i % 5) as f32 / 4.0),
                angle: 180.0,
                ..crate::combat::BulletSpawn::default()
            };
            if let Some(entity) = ctx.spawn(&spawn) {
                spawned.push(entity);
            }
        }
    });
    assert_eq!(spawned.len(), 40, "all fixture bullets spawned");
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let deflected = spawned
        .iter()
        .filter(|entity| {
            sim.ecs
                .0
                .get::<crate::entities::comp::TeamComp>(**entity)
                .is_some_and(|team| team.team == 1)
        })
        .count();
    assert!(
        deflected > 0,
        "phase wall deflected {deflected} of {} bullets",
        spawned.len()
    );
    assert!(
        deflected < spawned.len(),
        "the deflect roll is probabilistic (all {} bullets deflected)",
        spawned.len()
    );
}

/// I02 T4: `Wall.WallBuild.collision` (`Wall.java:164-166`) — a `flashHit`
/// wall (phase walls only, `Blocks.java:1759-1772`) logs a `WallFlashEvent`
/// for its anchor tile on every collision, deflected or not; the renderer owns
/// the `hit` decay. Non-`flashHit` walls log nothing.
#[test]
fn flash_hit_wall_logs_collision() {
    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "phase-wall", 1);
    // Keep the wall alive across the volley so every bullet lands.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    let plain = place(&mut sim, 26, 20, "copper-wall", 1);
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(plain) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    let def = {
        let runtime = sim.unit_runtime().expect("rt");
        let dagger = runtime.content.unit_by_name("dagger").expect("dagger");
        dagger.weapons[0].bullet.id
    };
    for (wx, _entity) in [(20.0f32 * 8.0, wall), (26.0f32 * 8.0, plain)] {
        sim.with_combat_ctx(|ctx| {
            for i in 0..4 {
                let spawn = crate::combat::BulletSpawn {
                    def,
                    team: 2,
                    x: wx + 6.0,
                    y: 20.0 * 8.0 - 3.0 + 2.0 * i as f32,
                    angle: 180.0,
                    ..crate::combat::BulletSpawn::default()
                };
                let _ = ctx.spawn(&spawn);
            }
        });
        for _ in 0..4 {
            sim.tick().expect("tick");
        }
    }
    let flashes = sim.drain_wall_flashes();
    assert!(
        flashes.contains(&(20, 20)),
        "the flashHit wall logged its hit: {flashes:?}"
    );
    assert!(
        !flashes.iter().any(|(x, _)| *x == 26),
        "a non-flashHit wall logs nothing: {flashes:?}"
    );
    assert!(sim.drain_wall_flashes().is_empty(), "the flash log drains");
}

/// H02 T12: `Wall.WallBuild.collision` (`Wall.java:169-175`) — a surge wall
/// arcs `Lightning.create(..)` from a share of the hits it takes
/// (`lightningChance = 0.05f`, `Blocks.java:1776`).
#[test]
fn surge_wall_arcs() {
    use crate::combat::lightning::LightningCounter;

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "surge-wall", 1);
    // Keep the wall alive across the volley so every bullet rolls.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    let def = {
        let runtime = sim.unit_runtime().expect("rt");
        let dagger = runtime.content.unit_by_name("dagger").expect("dagger");
        dagger.weapons[0].bullet.id
    };
    let (wx, wy) = (20.0 * 8.0, 20.0 * 8.0);
    sim.with_combat_ctx(|ctx| {
        for i in 0..200 {
            let spawn = crate::combat::BulletSpawn {
                def,
                team: 2,
                x: wx + 6.0,
                y: wy - 3.0 + 6.0 * ((i % 5) as f32 / 4.0),
                angle: 180.0,
                ..crate::combat::BulletSpawn::default()
            };
            let _ = ctx.spawn(&spawn);
        }
    });
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let branches = sim
        .ecs
        .0
        .get_resource::<LightningCounter>()
        .map(|counter| counter.0)
        .unwrap_or(0);
    assert!(
        branches > 0,
        "surge wall arced 0 times over 200 hits (5% chance each)"
    );
}

/// H01 T12: a team-1 `poly` with the `assist` command escorts the local
/// builder player and copies their build plan (`BuilderAI.java:57-100,143-186`).
/// Before T12 `onlyAssist` returned early for every tick and the poly idled.
#[test]
fn assist_poly_follows_building_player() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
    use crate::world::ConstructState;

    let mut sim = live_sim(64);
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
    // A core funds the build (`shouldSkip`) and the campaign unit cap
    // (`unitCap 0 + core +8`).
    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.item_id("copper").expect("copper")
    };
    if let Some(mut items) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(core) {
        items.add(copper, 500, 4000);
    }

    let player = sim
        .spawn_player_unit("alpha", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("player alpha");
    let poly = sim
        .spawn_live_unit("poly", 1, 10.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("poly");
    let assist = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .unit_command_by_name("assist")
            .expect("assist")
            .id
    };
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(poly) {
        slot.kind = AiKind::Command;
        slot.command = Some(assist);
    }

    // Phase 1: the player is idle -> `moveTo(assistFollowing, hit + hit/2 + 60)`.
    let start = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let ppos = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    let apos = *sim.ecs.0.get::<Pos>(player).expect("player pos");
    let dst = ((ppos.x - apos.x).powi(2) + (ppos.y - apos.y).powi(2)).sqrt();
    eprintln!("assist escort: poly {ppos:?} player {apos:?} dst {dst:.1} start {start:?}");
    assert!(
        ppos.x != start.x || ppos.y != start.y,
        "the assist poly left its idle spot"
    );
    // `assistFollowing.type.hitSize + unit.type.hitSize/2 + 60` (+ the `moving`
    // tolerance of 5): alpha hit 8, poly hit 9.
    assert!(
        dst <= 8.0 + 9.0 / 2.0 + 65.0,
        "the poly escorts the player ({dst})"
    );

    // Phase 2: queue the player's wall plan one tile east; the alpha starts the
    // construct (`updateBuildLogic`), then the cost is frozen so the plan stays
    // open for the whole acceptance window.
    let wall = BlockId::new(block(&sim, "copper-wall"));
    let plan = UnitBuildPlan::place(41, 40, 0, wall);
    sim.ecs
        .0
        .get_mut::<BuilderPlans>(player)
        .expect("player plans")
        .plans
        .push_back(plan);
    let mut construct = None;
    for _ in 0..30 {
        sim.tick().expect("tick");
        if let Some(entity) = sim.grid.tiles.get(41, 40).build
            && sim.ecs.0.get::<ConstructState>(entity).is_some()
        {
            construct = Some(entity);
            break;
        }
    }
    let construct = construct.expect("the alpha started the construct");
    sim.ecs
        .0
        .get_mut::<ConstructState>(construct)
        .expect("construct state")
        .build_cost = 1_000_000.0;
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let player_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(player)
        .and_then(|plans| plans.plans.front().cloned());
    let poly_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(poly)
        .and_then(|plans| plans.plans.front().cloned());
    // The per-unit plan progress mirrors diverge as each builder works; the
    // plan identity (tile/block/rotation/mode) is what is copied.
    let plan_id = |plan: Option<UnitBuildPlan>| {
        plan.map(|plan| (plan.x, plan.y, plan.block, plan.rotation, plan.breaking))
    };
    eprintln!("assist copy: player {player_plan:?} poly {poly_plan:?}");
    assert!(
        player_plan.is_some(),
        "the player is still actively building"
    );
    assert_eq!(
        plan_id(poly_plan),
        plan_id(player_plan),
        "the poly mimics the player's front plan"
    );
    // The poly reached the construct's range (buildRange - hitSize*2 = 202).
    let plan_tile = (41.0 * 8.0, 40.0 * 8.0);
    let ppos = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    let plan_dst = ((ppos.x - plan_tile.0).powi(2) + (ppos.y - plan_tile.1).powi(2)).sqrt();
    assert!(
        plan_dst <= 202.0 + 12.0,
        "the poly moved into build range of the plan ({plan_dst})"
    );
}

/// H01 T12: an idle `rebuild` poly (the poly default command, `onlyAssist`
/// false) picks an actively-building unit within `buildRadius` and copies its
/// plan (`BuilderAI.java:149-169`).
#[test]
fn idle_rebuild_poly_helps_builder() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::builder::{BuilderAiState, DEFAULT_REBUILD_PERIOD};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
    use crate::world::ConstructState;

    let mut sim = live_sim(64);
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
    // A core funds the build (`shouldSkip`) and the campaign unit cap
    // (`unitCap 0 + core +8`).
    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.item_id("copper").expect("copper")
    };
    if let Some(mut items) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(core) {
        items.add(copper, 500, 4000);
    }

    let player = sim
        .spawn_player_unit("alpha", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("player alpha");
    let helper = sim
        .spawn_live_unit("poly", 1, 10.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("helper poly");
    // `slot.command = None` -> the poly's `defaultCommand = rebuild`.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(helper) {
        slot.kind = AiKind::Command;
    }

    let wall = BlockId::new(block(&sim, "copper-wall"));
    let plan = UnitBuildPlan::place(41, 40, 0, wall);
    sim.ecs
        .0
        .get_mut::<BuilderPlans>(player)
        .expect("player plans")
        .plans
        .push_back(plan);
    let mut construct = None;
    for _ in 0..30 {
        sim.tick().expect("tick");
        if let Some(entity) = sim.grid.tiles.get(41, 40).build
            && sim.ecs.0.get::<ConstructState>(entity).is_some()
        {
            construct = Some(entity);
            break;
        }
    }
    let construct = construct.expect("the alpha started the construct");
    sim.ecs
        .0
        .get_mut::<ConstructState>(construct)
        .expect("construct state")
        .build_cost = 1_000_000.0;

    let start = *sim.ecs.0.get::<Pos>(helper).expect("helper pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }

    let state = sim
        .ecs
        .0
        .get::<BuilderAiState>(helper)
        .copied()
        .expect("builder state");
    let player_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(player)
        .and_then(|plans| plans.plans.front().cloned());
    let helper_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(helper)
        .and_then(|plans| plans.plans.front().cloned());
    let hpos = *sim.ecs.0.get::<Pos>(helper).expect("helper pos");
    let plan_tile = (41.0 * 8.0, 40.0 * 8.0);
    let plan_dst = ((hpos.x - plan_tile.0).powi(2) + (hpos.y - plan_tile.1).powi(2)).sqrt();
    eprintln!(
        "rebuild help: following {:?} plan_dst {plan_dst:.1} start {start:?} state \
         {state:?}",
        state.following
    );
    assert_eq!(
        state.following,
        Some(player),
        "the idle rebuild poly follows the building player"
    );
    // The per-unit plan progress mirrors diverge as each builder works; the
    // plan identity (tile/block/rotation/mode) is what is copied.
    let plan_id = |plan: Option<UnitBuildPlan>| {
        plan.map(|plan| (plan.x, plan.y, plan.block, plan.rotation, plan.breaking))
    };
    assert_eq!(
        plan_id(helper_plan),
        plan_id(player_plan),
        "the helper poly copies the player's front plan"
    );
    assert!(
        plan_dst <= 202.0 + 12.0,
        "the helper moved into build range of the plan ({plan_dst})"
    );
    assert_eq!(
        state.rebuild_period, DEFAULT_REBUILD_PERIOD,
        "no buildAi rule -> default rebuild period"
    );
}

/// H01 T13: a `manifold` (CargoAI) is dispatched to `AiKind::Cargo` instead of
/// the generic flying branch. Without a tether the controller early-returns
/// (`CargoAI.java:28`), so the unit stays put; the generic FlyingAI would have
/// closed on the enemy core.
#[test]
fn manifold_dispatches_cargo_ai() {
    use crate::ai::controller::{AiKind, ControllerSlot};
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
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    // A team-1 core satisfies the campaign unit cap (`unitCap 0 + core +8`).
    let _core = place(&mut sim, 5, 5, "core-shard", 1);
    let _enemy_core = place(&mut sim, 55, 55, "core-shard", 2);
    let manifold = sim
        .spawn_live_unit("manifold", 1, 20.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("manifold");
    let start = *sim.ecs.0.get::<Pos>(manifold).expect("manifold pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let kind = sim
        .ecs
        .0
        .get::<ControllerSlot>(manifold)
        .map(|slot| slot.kind);
    assert_eq!(kind, Some(AiKind::Cargo), "the manifold keeps CargoAI");
    let pos = *sim.ecs.0.get::<Pos>(manifold).expect("manifold alive");
    assert_eq!(
        (pos.x, pos.y),
        (start.x, start.y),
        "no tether -> the cargo unit never paths to the enemy core"
    );
}

/// H01 T13: an `assembly-drone` (AssemblerAI) is dispatched and reaches the
/// spot the assembler arranges (`AssemblerAI.java:12-24`).
#[test]
fn assembly_drone_dispatches() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::assembler::AssemblerAi;

    let mut sim = live_sim(64);
    let _enemy_core = place(&mut sim, 55, 55, "core-shard", 2);
    let drone = sim
        .spawn_live_unit("assembly-drone", 1, 20.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("assembly-drone");
    assert_eq!(
        sim.ecs.0.get::<ControllerSlot>(drone).map(|slot| slot.kind),
        Some(AiKind::Assembler),
        "the drone's content controller is AssemblerAI"
    );
    // The assembler block arranges the drone around its spawn perimeter
    // (`UnitAssembler.java:509-513`); the port writes the same state directly
    // until the block half lands.
    sim.ecs.0.entity_mut(drone).insert(AssemblerAi {
        unit: Some(drone),
        target_x: 26.0 * 8.0,
        target_y: 26.0 * 8.0,
        target_angle: 90.0,
    });
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(drone).expect("drone alive");
    let dst = ((pos.x - 26.0 * 8.0).powi(2) + (pos.y - 26.0 * 8.0).powi(2)).sqrt();
    let rotation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::UnitCore>(drone)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    eprintln!(
        "drone at ({}, {}) rotation {rotation} dst {dst}",
        pos.x, pos.y
    );
    assert!(dst <= 10.0, "the drone reached its arranged spot ({dst})");
    assert!(
        (rotation - 90.0).abs() < 0.001 || (rotation - 450.0).abs() < 0.001,
        "the drone faces the arranged angle ({rotation})"
    );
}

/// I01 T12: every turret carries the detached `block` unit proxy
/// (`TurretBuild.unit = UnitTypes.block.create(team)`, `Turret.java:297`),
/// positioned on the turret each tick (`unit.tile(this)`, `Turret.java:535-537`)
/// and dropped with the building. The proxy is never in the live unit list, so
/// it is never saved.
#[test]
fn turret_has_block_unit() {
    use crate::ecs::EntitySeq;
    use crate::entities::comp::unit::comp::{BlockUnitComp, UnitTypeComp};
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    let unit = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .block_unit
        .expect("block unit proxy");
    // `UnitTypes.block`.
    let runtime = sim.unit_runtime().expect("runtime");
    let type_id = sim
        .ecs
        .0
        .get::<UnitTypeComp>(unit)
        .expect("proxy type")
        .type_id;
    assert_eq!(
        runtime.content.unit(type_id).expect("block def").name,
        "block"
    );
    // `unit.tile(this)`: the proxy sits on the turret's tile.
    let block_tile = sim
        .ecs
        .0
        .get::<Building>(duo)
        .expect("building")
        .tile
        .pack();
    assert_eq!(
        sim.ecs
            .0
            .get::<BlockUnitComp>(unit)
            .expect("block unit comp")
            .tile,
        block_tile
    );
    // Team + position follow the turret.
    let (bx, by) = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|pos| (pos.x, pos.y))
        .expect("turret pos");
    let (ux, uy) = sim
        .ecs
        .0
        .get::<Pos>(unit)
        .map(|pos| (pos.x, pos.y))
        .expect("proxy pos");
    assert_eq!((ux, uy), (bx, by), "proxy sits on the turret");
    assert_eq!(sim.ecs.0.get::<TeamComp>(unit).expect("proxy team").team, 1);
    assert!(
        !runtime.units.contains(&unit),
        "the proxy is detached (never saved / never unit-updated)"
    );
    let _ = sim.ecs.0.get::<EntitySeq>(unit).expect("proxy seq");

    // Removing the turret removes the proxy.
    sim.command(SimCommand::Break {
        x: 20,
        y: 20,
        player: None,
    })
    .expect("break");
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(unit).is_err(),
        "proxy despawned with the turret"
    );
}

/// I01 T12 (`Turret.java:571-600`): a possessed turret aims at the player's
/// cursor and fires on the shoot button (`controlled()` -> `targetPos.set(
/// unit.aimX(), unit.aimY()); canShoot = unit.isShooting()`).
#[test]
fn controlled_turret_shoots_at_aim() {
    use crate::ecs::EntitySeq;
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim.set_player_team(1);
    let unit = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .block_unit
        .expect("block unit proxy");
    let seq = sim.ecs.0.get::<EntitySeq>(unit).expect("proxy seq").0 as i32;
    assert!(sim.possess_unit(seq), "player possesses the turret proxy");
    assert_eq!(sim.player_unit(), Some(unit));

    // Aim east of the turret (its default rotation is 90/south) and hold fire.
    let (bx, by) = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|pos| (pos.x, pos.y))
        .expect("turret pos");
    let before = sim.unit_runtime().expect("runtime").bullets_created;
    for _ in 0..120 {
        sim.set_player_control(0.0, 0.0, true, bx + 120.0, by, false, false);
        sim.tick().expect("tick");
    }
    let after = sim.unit_runtime().expect("runtime").bullets_created;
    assert!(
        after > before,
        "the controlled turret fired ({before} -> {after})"
    );
    let rotation = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .rotation;
    assert!(
        (rotation.rem_euclid(360.0)).abs() < 5.0
            || (rotation.rem_euclid(360.0) - 360.0).abs() < 5.0,
        "the turret turned toward the cursor ({rotation})"
    );

    // Releasing the control stops the player-driven fire (the turret drops the
    // `Player` marker).
    sim.clear_unit();
    assert!(sim.player_unit().is_none());
}

/// I01 T12 (`CoreBlock.java:619-637`): Ctrl-click on the own core clears the
/// player's unit and respawns it at that core (`player.clearUnit();
/// player.deathTimer = deathDelay + 1; requestSpawn(player)`).
#[test]
fn core_control_select_respawns() {
    let mut sim = live_sim(48);
    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let alpha = sim
        .spawn_player_unit("alpha", 1, 200.0, 200.0, 0.0)
        .expect("alpha");
    assert_eq!(sim.player_unit(), Some(alpha));

    sim.command(SimCommand::BuildingControlSelect { x: 10, y: 10 })
        .expect("control select");
    sim.tick().expect("tick");
    let player = sim.player_unit().expect("respawned player");
    assert_ne!(player, alpha, "the old unit was released");
    let (cx, cy) = sim
        .ecs
        .0
        .get::<Pos>(core)
        .map(|pos| (pos.x, pos.y))
        .expect("core pos");
    let (px, py) = sim
        .ecs
        .0
        .get::<Pos>(player)
        .map(|pos| (pos.x, pos.y))
        .expect("player pos");
    assert!(
        (px - cx).abs() < 1.0 && (py - cy).abs() < 1.0,
        "respawned at the selected core ({px}, {py}) vs ({cx}, {cy})"
    );
}

/// J03 T2: a freshly placed idle duo draws its head facing up.
///
/// `BaseTurretBuild.rotation = 90` (`BaseTurret.java:47`) and the barrel angle
/// is `drawrot() = rotation - 90 = 0` (`Turret.java:327-329`); with no ammo the
/// turret never turns (`Turret.updateTile` gates targeting on `hasAmmo`,
/// `Turret.java:571-605`). Pins the sim half of the drawn angle: a no-ammo duo
/// keeps `rotation = 90` (drawrot 0) across 60 ticks.
#[test]
fn placed_duo_idle_head_rotation_is_up() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    assert_eq!(
        sim.ecs
            .0
            .get::<TurretState>(duo)
            .expect("turret state")
            .rotation,
        90.0,
        "BaseTurretBuild starts at rotation 90"
    );
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    assert_eq!(state.total_ammo, 0, "no ammo");
    assert!(state.target.is_none(), "no target");
    assert_eq!(state.rotation, 90.0, "idle turret stays at 90");
    // The renderer draws at `rotation - 90` (blocks.rs / render/turret.rs).
    let drawrot = state.rotation - 90.0;
    assert!(
        drawrot.abs() < 1e-6,
        "idle head drawrot is 0 (up): {drawrot}"
    );
}

/// L06 T1: `UnitAssemblerBuild.wasOccupied = checkSolid(getUnitSpawn(), false)`
/// (`UnitAssembler.java:515,679-685`) with the vanilla `areaSize = 13`
/// (`Blocks.java:6566`): a solid tile or a grounded unit on the spawn point
/// occupies a ground output; a flyer or a `spawnedByCore` unit does not.
#[test]
fn assembler_output_area_occupancy() {
    use crate::world::blocks::units::UnitAssembler;
    let mut sim = live_sim(64);
    let assembler = place(&mut sim, 20, 20, "tank-assembler", 1);
    sim.tick().expect("tick");
    let occupied = |sim: &Sim| {
        sim.ecs
            .0
            .get::<UnitAssembler>(assembler)
            .expect("assembler")
            .was_occupied
    };
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitAssembler>(assembler)
            .map(|a| a.area_size),
        Some(13)
    );
    assert!(!occupied(&sim), "free output area");
    // Spawn point: rotation 0, `x + tilesize * (13 + 5) / 2` = tile (29, 20).
    let (sx, sy) = (29.0 * 8.0, 20.0 * 8.0);
    let flare = sim.spawn_live_unit("flare", 1, sx, sy, 0.0).expect("flare");
    sim.tick().expect("tick");
    assert!(!occupied(&sim), "a flyer does not block a ground output");
    sim.ecs.0.despawn(flare);
    let dagger = sim
        .spawn_live_unit("dagger", 2, sx, sy, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(dagger)
    {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    sim.tick().expect("tick");
    assert!(occupied(&sim), "a grounded unit on the spawn point");
    if let Some(mut core) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::unit::comp::UnitCore>(dagger)
    {
        core.spawned_by_core = true;
    }
    sim.tick().expect("tick");
    assert!(!occupied(&sim), "spawnedByCore units are ignored");
    sim.ecs.0.despawn(dagger);
    let _wall = place(&mut sim, 29, 20, "copper-wall", 1);
    sim.tick().expect("tick");
    assert!(occupied(&sim), "a solid tile under a ground output");
}

/// L06 T1: `UnitAssemblerBuild.spawned()` (`UnitAssembler.java:556-574`): the
/// product takes `commandPos` (`CommandAI.commandPosition`) and goes into the
/// same-team payload block under the spawn point when it accepts it
/// (`unit.buildOn()`); otherwise it is added to the world.
#[test]
fn assembled_unit_takes_command_pos_and_payload_handoff() {
    use crate::world::blocks::payloads::PayloadHolder;
    use crate::world::blocks::units::behavior::deliver_assembled_unit;
    let mut sim = live_sim(64);
    // Team 1 is the player's (`rules.defaultTeam`), so products get `CommandAI`
    // (`UnitType.controller`, UnitType.java:283).
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let conveyor = place(&mut sim, 10, 10, "payload-conveyor", 1);
    let def = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .cloned()
        .expect("dagger");
    let in_runtime = |sim: &Sim, unit: Entity| {
        sim.unit_runtime()
            .is_some_and(|runtime| runtime.units.contains(&unit))
    };
    // Payload conveyor at tile (10, 10) (size 3, centred) under the spawn.
    let (unit, handed) = deliver_assembled_unit(
        &mut sim.ecs.0,
        &def,
        1,
        80.0,
        80.0,
        0.0,
        Some((400.0, 240.0)),
    );
    assert!(handed, "handed to the payload conveyor");
    assert!(!in_runtime(&sim, unit), "a payload unit is not added");
    let held = sim
        .ecs
        .0
        .get::<PayloadHolder>(conveyor)
        .and_then(|holder| holder.payload);
    assert_eq!(held.and_then(|p| p.entity), Some(unit), "conveyor holds it");
    // Open ground: `unit.add()`.
    let (free, handed) = deliver_assembled_unit(
        &mut sim.ecs.0,
        &def,
        1,
        300.0,
        300.0,
        0.0,
        Some((400.0, 240.0)),
    );
    assert!(!handed);
    assert!(in_runtime(&sim, free), "added to the world");
    let slot = sim
        .ecs
        .0
        .get::<crate::ai::controller::ControllerSlot>(free)
        .cloned()
        .expect("controller");
    if slot.kind == crate::ai::controller::AiKind::Command {
        assert_eq!(
            slot.target,
            Some(crate::world::TilePos::new(50, 30)),
            "commandPos -> move target"
        );
    } else {
        panic!(
            "a player-team product should be commandable: {:?}",
            slot.kind
        );
    }
    // An enemy-team payload block never takes it.
    let enemy = place(&mut sim, 40, 40, "payload-conveyor", 2);
    let (_, handed) = deliver_assembled_unit(&mut sim.ecs.0, &def, 1, 320.0, 320.0, 0.0, None);
    assert!(!handed, "team check");
    assert!(
        sim.ecs
            .0
            .get::<PayloadHolder>(enemy)
            .and_then(|holder| holder.payload)
            .is_none()
    );
}

/// L06 T2: `RouterBuild implements ControlBlock` (`Router.java:30-53`): an own
/// size-1 router yields a lazily created, stable `block` proxy
/// (`unit()`, `:37-44`); the distributor (size 2) cannot be controlled
/// (`canControl() = size == 1`); turrets keep theirs (`playerControllable`).
#[test]
fn router_is_a_control_block() {
    let mut sim = live_sim(48);
    sim.set_player_team(1);
    let router = place(&mut sim, 10, 10, "router", 1);
    let _distributor = place(&mut sim, 20, 20, "distributor", 1);
    let _duo = place(&mut sim, 30, 30, "duo", 1);
    let _enemy = place(&mut sim, 40, 40, "router", 2);
    let pos = |x: i16, y: i16| crate::world::TilePos::new(x, y);
    let first = sim
        .control_block_unit_id(pos(10, 10))
        .expect("router proxy");
    assert_eq!(
        sim.control_block_unit_id(pos(10, 10)),
        Some(first),
        "stable"
    );
    let proxy = sim
        .ecs
        .0
        .get::<crate::world::blocks::distribution::router::RouterBuild>(router)
        .and_then(|state| state.block_unit)
        .expect("proxy stored");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::BlockUnitComp>(proxy)
            .is_some()
    );
    assert!(
        !sim.unit_runtime().expect("runtime").units.contains(&proxy),
        "never added to the unit pass"
    );
    assert_eq!(sim.control_block_unit_id(pos(20, 20)), None, "distributor");
    assert!(sim.control_block_unit_id(pos(30, 30)).is_some(), "duo");
    assert_eq!(sim.control_block_unit_id(pos(40, 40)), None, "enemy team");
}

/// L06 T2: a possessed router (`Router.java:102-117`) routes only toward the
/// aimed quadrant and only while shooting; unpossessed it round-robins.
#[test]
fn possessed_router_routes_toward_aim_while_shooting() {
    use crate::ai::types::player::PlayerInput;
    use crate::world::blocks::distribution::transfer;
    let mut sim = live_sim(48);
    sim.set_player_team(1);
    let router = place(&mut sim, 10, 10, "router", 1);
    let east = place(&mut sim, 11, 10, "container", 1);
    let north = place(&mut sim, 9, 11, "container", 1);
    let copper = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .item_id("copper")
        .expect("copper");
    let proxy_seq = sim
        .control_block_unit_id(crate::world::TilePos::new(10, 10))
        .expect("proxy");
    let proxy = sim
        .ecs
        .0
        .get::<crate::world::blocks::distribution::router::RouterBuild>(router)
        .and_then(|state| state.block_unit)
        .expect("proxy");
    // `InputHandler.unitControl` on the proxy (the Ctrl+click path).
    assert!(sim.possess_unit(proxy_seq), "possess the router proxy");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::Player>(proxy)
            .is_some()
    );
    let set_input = |sim: &mut Sim, shooting: bool| {
        let mut input = sim
            .ecs
            .0
            .remove_resource::<PlayerInput>()
            .unwrap_or_default();
        // Aim at tile (14, 10): due east of the router.
        input.aim_x = 14.0 * 8.0;
        input.aim_y = 10.0 * 8.0;
        input.shooting = shooting;
        sim.ecs.0.insert_resource(input);
    };
    let stored = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::world::modules::ItemModule>(e)
            .map_or(0, |items| items.get(copper))
    };
    set_input(&mut sim, false);
    transfer::dispatch_handle_item(&mut sim.ecs.0, router, router, copper);
    for _ in 0..30 {
        set_input(&mut sim, false);
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored(&sim, east) + stored(&sim, north),
        0,
        "holds while not shooting"
    );
    assert_eq!(stored(&sim, router), 1);
    for _ in 0..30 {
        set_input(&mut sim, true);
        sim.tick().expect("tick");
    }
    assert_eq!(stored(&sim, east), 1, "aimed side");
    assert_eq!(stored(&sim, north), 0, "never the other side");
}
