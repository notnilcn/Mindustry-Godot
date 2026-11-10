// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit motion, collision avoidance, payload pickup/handoff and lifesteal regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

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
