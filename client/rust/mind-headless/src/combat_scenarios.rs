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

/// Benchmarks bullet update/collision over `bullets` live entities.
fn bench(bullets: usize, turrets: usize, ticks: u64, json: bool) -> Result<i32> {
    let _ = turrets;
    let mut harness = CombatHarness::new(128, 128, 7);
    for i in 0..bullets {
        let (x, y) = CombatHarness::tile_center(2 + (i % 60) as i32, 2 + (i / 60) as i32 % 60);
        let angle = (i as f32 * 13.0) % 360.0;
        let _ = harness.spawn_bullet("fuse", x, y, angle, 1);
    }
    let mut samples: Vec<u64> = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        let start = Instant::now();
        harness.step_bullets_only();
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
    samples.sort_unstable();
    let p50 = samples.get(samples.len() * 50 / 100).copied().unwrap_or(0);
    let p99 = samples
        .get((samples.len() * 99 / 100).min(samples.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    let report = serde_json::json!({
        "scenario": "combat_bench",
        "bullets": bullets,
        "ticks": ticks,
        "p50_us": p50,
        "p99_us": p99,
        "checksum": bullet_checksum(&harness),
    });
    print_json(&report, json);
    Ok(EXIT_PASS)
}
