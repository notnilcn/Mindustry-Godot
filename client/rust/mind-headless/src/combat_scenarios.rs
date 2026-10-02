// SPDX-License-Identifier: GPL-3.0-only

//! Plan 10 headless combat scenarios (`combat` subcommand).
//!
//! All scenarios run on the [`mind_core::combat::CombatHarness`] flat fixture
//! (plan 07's build world + fixture bullets). Golden checksums live under
//! `mind-core/tests/golden/combat_*.json`.

use std::time::Instant;

use anyhow::{Result, bail};
use mind_core::combat::CombatHarness;
use mind_core::combat::bullet::Bullet;

use crate::cli::CombatCommand;

/// Exit code: pass.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// Runs a `combat` subcommand.
pub fn run(command: &CombatCommand) -> Result<i32> {
    match command {
        CombatCommand::Scenario { name, json } => scenario(name, *json),
        CombatCommand::Dump { bullets_live, json } => dump(*bullets_live, *json),
        CombatCommand::Trace { kind, ticks, json } => trace(kind, *ticks, *json),
        CombatCommand::Bench {
            bullets,
            turrets,
            ticks,
            json,
        } => bench(*bullets, *turrets, *ticks, *json),
    }
}

fn print_json(value: &serde_json::Value, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    }
}

fn bullet_checksum(harness: &CombatHarness) -> String {
    harness.checksum_hex()
}

/// Runs one named combat scenario and prints its deterministic report.
fn scenario(name: &str, json: bool) -> Result<i32> {
    match name {
        "combat_basic" => {
            let mut harness = CombatHarness::new(32, 32, 7);
            let wall = harness
                .content()
                .block_id("copper-wall")
                .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
            let _ = harness.place(10, 10, wall, 0, true);
            let before = harness.building_health_at(10, 10);
            let (x, y) = CombatHarness::tile_center(8, 10);
            let spawned = harness.spawn_bullet("fuse", x, y, 0.0, 1).is_some();
            for _ in 0..120 {
                harness.tick();
            }
            let after = harness.building_health_at(10, 10);
            let checksum = bullet_checksum(&harness);
            let pass = spawned && after < before && harness.bullets_live() == 0;
            let report = serde_json::json!({
                "scenario": name,
                "seed": 7,
                "pass": pass,
                "spawned": spawned,
                "wall_before": before,
                "wall_after": after,
                "damage": before - after,
                "bullets_created": harness.bullets_created,
                "bullets_removed": harness.bullets_removed,
                "checksum": checksum,
            });
            print_json(&report, json);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        "combat_bullet_pierce" => {
            let mut harness = CombatHarness::new(48, 16, 11);
            let wall = harness
                .content()
                .block_id("copper-wall")
                .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
            for x in 12..=14 {
                let _ = harness.place(x, 8, wall, 0, true);
            }
            let (x, y) = CombatHarness::tile_center(4, 8);
            let _ = harness.spawn_bullet("rail", x, y, 0.0, 1);
            for _ in 0..80 {
                harness.tick();
            }
            let damaged = (12..=14)
                .filter(|x| harness.building_health_at(*x, 8) < 320.0)
                .count();
            let pass = damaged >= 2;
            let report = serde_json::json!({
                "scenario": name,
                "seed": 11,
                "pass": pass,
                "walls_damaged": damaged,
                "checksum": bullet_checksum(&harness),
            });
            print_json(&report, json);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        "combat_damage_matrix" => {
            let mut harness = CombatHarness::new(48, 32, 13);
            let wall = harness
                .content()
                .block_id("copper-wall")
                .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
            for (x, y) in [
                (14, 4),
                (10, 8),
                (10, 12),
                (12, 12),
                (10, 16),
                (12, 16),
                (10, 20),
                (10, 24),
            ] {
                let _ = harness.place(x, y, wall, 0, true);
            }
            let created_before = harness.bullets_created;
            let lanes = [
                ("point", 4),
                ("multi", 6),
                ("sap", 8),
                ("shrapnel", 12),
                ("emp", 16),
                ("flak", 20),
                ("continuous", 24),
            ];
            for (kind, ty) in lanes {
                let (x, y) = CombatHarness::tile_center(4, ty);
                let _ = harness.spawn_bullet(kind, x, y, 0.0, 1);
            }
            for _ in 0..200 {
                harness.tick();
            }
            let probes = [
                (14, 4),
                (10, 8),
                (10, 12),
                (12, 12),
                (10, 16),
                (12, 16),
                (10, 20),
                (10, 24),
            ];
            let damaged = probes
                .iter()
                .map(|(x, y)| harness.building_health_at(*x, *y) < 320.0)
                .collect::<Vec<_>>();
            let pass = damaged.iter().all(|d| *d);
            let report = serde_json::json!({
                "scenario": name,
                "seed": 13,
                "pass": pass,
                "created": harness.bullets_created - created_before,
                "damaged": damaged,
                "checksum": bullet_checksum(&harness),
            });
            print_json(&report, json);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        "combat_fire_puddle_tick" => {
            use mind_core::combat::{fires, puddles};
            use mind_core::determinism::SimRng;
            let mut harness = CombatHarness::new(32, 32, 23);
            let oil = harness
                .content()
                .liquid_id("oil")
                .ok_or_else(|| anyhow::anyhow!("oil missing"))?;
            let mut rng = SimRng::new(23);
            let _ = puddles::deposit(
                &mut harness.build.world,
                &harness.build.content,
                10,
                10,
                oil,
                70.0,
                &mut rng,
            );
            let _ = fires::create(&mut harness.build.world, 10, 10, &mut rng);
            for _ in 0..200 {
                let _ = puddles::deposit(
                    &mut harness.build.world,
                    &harness.build.content,
                    10,
                    10,
                    oil,
                    70.0,
                    &mut rng,
                );
                harness.tick();
            }
            let fire = fires::has(&harness.build.world, 10, 10);
            let puddle = puddles::find_at(&harness.build.world, 10, 10).is_some();
            let spread = [(9, 10), (11, 10), (10, 9), (10, 11)]
                .iter()
                .filter(|(x, y)| fires::has(&harness.build.world, *x, *y))
                .count();
            let pass = fire && puddle && spread >= 1;
            let report = serde_json::json!({
                "scenario": name,
                "seed": 23,
                "pass": pass,
                "fire": fire,
                "puddle": puddle,
                "spread_neighbors": spread,
                "checksum": bullet_checksum(&harness),
            });
            print_json(&report, json);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        "combat_determinism" => {
            let first = sample_swarm(29, 360, 60);
            let second = sample_swarm(29, 360, 60);
            let pass = first == second && !first.is_empty();
            let report = serde_json::json!({
                "scenario": name,
                "seed": 29,
                "pass": pass,
                "samples": first.len(),
                "checksums_a": first,
                "checksums_b": second,
            });
            print_json(&report, json);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        "combat_weapon_volley" => {
            use mind_core::content::registries::units::ResolvedBullet;
            use mind_core::content::registries::units::weapon::{ShootPatternSpec, WeaponSpec};
            let mut harness = CombatHarness::new(48, 16, 31);
            let wall = harness
                .content()
                .block_id("copper-wall")
                .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
            for x in 12..=13 {
                let _ = harness.place(x, 8, wall, 0, true);
            }
            let registry = harness.content();
            let bullet = harness
                .bullet_id("fuse")
                .ok_or_else(|| anyhow::anyhow!("fuse bullet missing"))?;
            let weapon = mind_core::content::registries::units::weapon::WeaponDef::from_spec(
                WeaponSpec {
                    name: "volley",
                    reload: Some(10.0),
                    x: Some(0.0),
                    shoot_y: Some(0.0),
                    recoil: Some(0.0),
                    rotate: Some(false),
                    mirror: Some(false),
                    alternate: Some(false),
                    shoot: Some(ShootPatternSpec::plain(1, 0.0, 0.0)),
                    ..WeaponSpec::default()
                },
                ResolvedBullet {
                    id: bullet,
                    range: 200.0,
                    heals: false,
                    kill_shooter: false,
                    dps: 0.0,
                },
                registry,
            )?;
            let (ux, uy) = CombatHarness::tile_center(4, 8);
            let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
            harness.set_unit_aim(unit, 0, CombatHarness::tile_center(12, 8));
            harness.set_unit_shoot(unit, 0, true);
            let before = harness.building_health_at(12, 8);
            for _ in 0..120 {
                harness.tick();
            }
            let after = harness.building_health_at(12, 8);
            let shots = harness
                .unit_weapons(unit)
                .map(|weapons| weapons.mounts[0].total_shots)
                .unwrap_or(0);
            let pass = before > after && shots >= 5;
            let report = serde_json::json!({
                "scenario": name,
                "seed": 31,
                "pass": pass,
                "shots_fired": shots,
                "wall_before": before,
                "wall_after": after,
                "damage": before - after,
                "checksum": bullet_checksum(&harness),
            });
            print_json(&report, json);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        "combat_turret_ammo" => {
            use mind_core::world::blocks::defense::turrets;
            let mut harness = CombatHarness::new(48, 16, 37);
            let wall = harness
                .content()
                .block_id("copper-wall")
                .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
            let _ = harness.place(12, 8, wall, 0, true);
            let (tx, ty) = CombatHarness::tile_center(4, 8);
            let turret = harness
                .spawn_test_turret("duo", tx, ty, 1)
                .ok_or_else(|| anyhow::anyhow!("duo turret config missing"))?;
            let copper = harness
                .content()
                .item_id("copper")
                .ok_or_else(|| anyhow::anyhow!("copper missing"))?;
            for _ in 0..10 {
                turrets::handle_item(&mut harness.build.world, turret, copper);
            }
            let before = harness.building_health_at(12, 8);
            for _ in 0..120 {
                harness.tick();
            }
            let after = harness.building_health_at(12, 8);
            let (shots, ammo) = harness
                .turret_state(turret)
                .map(|state| (state.total_shots, state.total_ammo))
                .unwrap_or((0, 0));
            let pass = after < before && shots >= 3 && ammo < 20;
            let report = serde_json::json!({
                "scenario": name,
                "seed": 37,
                "pass": pass,
                "shots_fired": shots,
                "ammo_remaining": ammo,
                "wall_before": before,
                "wall_after": after,
                "damage": before - after,
                "checksum": bullet_checksum(&harness),
            });
            print_json(&report, json);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        other => bail!("unknown combat scenario `{other}`"),
    }
}

/// Runs a seeded bullet swarm, sampling the checksum every `every` ticks.
fn sample_swarm(seed: u64, ticks: u64, every: u64) -> Vec<String> {
    let mut harness = CombatHarness::new(64, 64, seed);
    for i in 0..32u32 {
        // Slow bullets survive the whole run so the intermediate samples differ.
        let (x, y) = CombatHarness::tile_center(2 + (i % 8) as i32 * 6, 2 + (i / 8) as i32 * 6);
        let angle = (i as f32 * 37.0) % 360.0;
        let _ = harness.spawn_bullet("fuse_slow", x, y, angle, 1);
    }
    let mut samples = Vec::new();
    for tick in 1..=ticks {
        harness.step_bullets_only();
        if every > 0 && tick % every == 0 {
            samples.push(harness.checksum_hex());
        }
    }
    samples
}

/// Dumps live bullets (inspector feed).
fn dump(bullets_live: bool, json: bool) -> Result<i32> {
    let mut harness = CombatHarness::new(32, 32, 7);
    let _ = bullets_live;
    let (x, y) = CombatHarness::tile_center(8, 8);
    let _ = harness.spawn_bullet("fuse", x, y, 0.0, 1);
    harness.step_bullets_only();
    let bullets: Vec<serde_json::Value> = harness
        .bullets
        .iter()
        .filter_map(|e| {
            let b = harness.build.world.get::<Bullet>(*e)?;
            let p = harness
                .build
                .world
                .get::<mind_core::entities::comp::Pos>(*e)?;
            Some(serde_json::json!({
                "def": b.def.raw(),
                "x": p.x,
                "y": p.y,
                "time": b.time,
                "lifetime": b.lifetime,
            }))
        })
        .collect();
    let report = serde_json::json!({
        "scenario": "combat_dump",
        "live": harness.bullets_live(),
        "bullets": bullets,
        "checksum": bullet_checksum(&harness),
    });
    print_json(&report, json);
    Ok(EXIT_PASS)
}

/// Traces a bullet's per-tick position along its flight.
fn trace(kind: &str, ticks: u64, json: bool) -> Result<i32> {
    let mut harness = CombatHarness::new(32, 32, 7);
    let (x, y) = CombatHarness::tile_center(4, 4);
    let Some(entity) = harness.spawn_bullet(kind, x, y, 0.0, 1) else {
        bail!("unknown bullet kind `{kind}`");
    };
    let mut rows = Vec::new();
    for _ in 0..ticks {
        let alive = harness.build.world.get_entity(entity).is_ok();
        if let Some(p) = harness
            .build
            .world
            .get::<mind_core::entities::comp::Pos>(entity)
        {
            rows.push(serde_json::json!({ "x": p.x, "y": p.y }));
        }
        if !alive {
            break;
        }
        harness.step_bullets_only();
    }
    let report = serde_json::json!({
        "scenario": "combat_trace",
        "kind": kind,
        "ticks": ticks,
        "samples": rows.len(),
        "rows": rows,
    });
    print_json(&report, json);
    Ok(EXIT_PASS)
}

/// Benchmarks the full combat tick (turrets + bullets + defense) over
/// `bullets` live entities and `turrets` firing `test-item` fixtures (plan 10
/// §7d; `2_000 / 400 / 3600` is the recorded profile).
fn bench(bullets: usize, turrets: usize, ticks: u64, json: bool) -> Result<i32> {
    use mind_core::world::blocks::defense::turrets;

    let mut harness = CombatHarness::new(128, 128, 7);
    // A durable target row the turrets can shoot without destroying it quickly.
    if let Some(wall) = harness.content().block_id("copper-wall") {
        for tx in 2..60 {
            let _ = harness.place(tx, 60, wall, 0, true);
        }
    }
    for i in 0..turrets {
        let tx = 2 + (i % 58) as i32;
        let ty = 4 + (i / 58) as i32 * 2;
        let (x, y) = CombatHarness::tile_center(tx, ty);
        if let Some(turret) = harness.spawn_test_turret("test-item", x, y, 1)
            && let Some(copper) = harness.content().item_id("copper")
        {
            for _ in 0..30 {
                turrets::handle_item(&mut harness.build.world, turret, copper);
            }
        }
    }
    for i in 0..bullets {
        let (x, y) = CombatHarness::tile_center(2 + (i % 60) as i32, 2 + (i / 60) as i32 % 60);
        let angle = (i as f32 * 13.0) % 360.0;
        let _ = harness.spawn_bullet("fuse", x, y, angle, 2);
    }
    // Warmup (pool/content lazy init) then measured ticks.
    for _ in 0..10 {
        harness.tick();
    }
    let mut samples: Vec<u64> = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        let start = Instant::now();
        harness.tick();
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
    samples.sort_unstable();
    let percentile = |p: usize| {
        samples
            .get((samples.len() * p / 100).min(samples.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0)
    };
    let report = serde_json::json!({
        "scenario": "combat_bench",
        "bullets": bullets,
        "turrets": turrets,
        "ticks": ticks,
        "p50_us": percentile(50),
        "p95_us": percentile(95),
        "p99_us": percentile(99),
        "checksum": bullet_checksum(&harness),
    });
    print_json(&report, json);
    Ok(EXIT_PASS)
}
