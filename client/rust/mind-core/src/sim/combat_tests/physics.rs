// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Physics regressions: velocity drag, knockback, wind/ice, crashes and wreck overkill.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{live_sim, place_at, tick_n};

/// T4 (PAR-0202): `VelComp.update` integrates `vel` with tile collisions
/// (`EntityCollisions.move`), zeroes a blocked axis (`Mathf.equal`) and then
/// applies drag (`vel.scl(1 - drag)`).
#[test]
fn vel_update_applies_drag_and_stops_on_wall() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(20);
    let _wall = place_at(&mut sim, 10, 5, "copper-wall", 1);
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
    tick_n(&mut sim, 60);
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
    tick_n(&mut sim, 300);
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
    tick_n(&mut sim, 20);
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
    let enemy_core = place_at(&mut sim, 40, 40, "core-shard", 2);
    // A player core is required for the wave spawner's core step
    // (`WaveSpawner.eachGroundSpawn` steps toward `playerCore()`).
    let player_core = place_at(&mut sim, 30, 40, "core-shard", 1);
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
    tick_n(&mut sim, 3);
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
