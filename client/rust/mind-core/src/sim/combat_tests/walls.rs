// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Wall regressions: phase-wall reflect, collision logging and surge arcs.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

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
