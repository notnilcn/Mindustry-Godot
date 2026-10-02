// SPDX-License-Identifier: GPL-3.0-only

//! Plan 11 headless unit/AI scenarios (`units` subcommand).
//!
//! All scenarios run on the [`UnitHarness`] flat fixture (plan 07's build world
//! plus the plan-11 unit components, pathfinder and `GroundAI`). Golden
//! checksums live under `tests/golden/units/`.

use std::time::Instant;

use anyhow::{Result, bail};
use bevy_ecs::entity::Entity;
use mind_core::ai::UnitHarness;

use crate::cli::UnitsCommand;

/// Exit code: pass.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// A scenario's report plus its canonical (golden) JSON dump.
pub struct ScenarioOutput {
    /// Human/machine-readable report (printed with `--json`).
    pub report: serde_json::Value,
    /// Canonical pretty JSON used for byte-for-byte golden comparison.
    pub dump: String,
}

/// Scenario names registered for the `units` subcommand (append-only).
pub fn names() -> &'static [&'static str] {
    &["units_spawn_path_arrive"]
}

/// Runs a `units` subcommand.
pub fn run(command: &UnitsCommand) -> Result<i32> {
    match command {
        UnitsCommand::Scenario { name, json } => {
            let out = run_scenario(name)?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&out.report)?);
            }
            let pass = out.report["pass"].as_bool().unwrap_or(false);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        UnitsCommand::Spawn {
            unit,
            team,
            x,
            y,
            ticks,
            json,
        } => spawn_report(unit, *team, *x, *y, *ticks, *json),
        UnitsCommand::Path {
            unit,
            tx,
            ty,
            ticks,
            json,
        } => path_report(unit, *tx, *ty, *ticks, *json),
        UnitsCommand::Bench { units, ticks, json } => bench(*units, *ticks, *json),
    }
}

fn canonical(value: &serde_json::Value) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

fn round3(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

/// Runs one named unit scenario and returns its report + golden dump.
pub fn run_scenario(name: &str) -> Result<ScenarioOutput> {
    match name {
        "units_spawn_path_arrive" => spawn_path_arrive(),
        other => bail!("unknown units scenario `{other}`"),
    }
}

/// `units_spawn_path_arrive`: one dagger spawns, paths corner-to-corner, arrives.
fn spawn_path_arrive() -> Result<ScenarioOutput> {
    let mut harness = UnitHarness::new(64, 64, 7);
    let entity = harness
        .spawn("dagger", 0, 44.0, 44.0, 0.0)
        .ok_or_else(|| anyhow::anyhow!("dagger missing from content"))?;
    assert!(harness.command_move(entity, 60, 60));
    for _ in 0..1500 {
        harness.tick();
    }
    let snap = harness
        .snapshot(entity)
        .ok_or_else(|| anyhow::anyhow!("unit despawned"))?;
    let (tx, ty) = tile_center(60, 60);
    let distance = ((snap.x - tx).powi(2) + (snap.y - ty).powi(2)).sqrt();
    let arrived = distance <= snap.hit_size.max(4.0);
    let checksum = harness.checksum_hex();
    let report = serde_json::json!({
        "scenario": "units_spawn_path_arrive",
        "seed": 7,
        "pass": arrived && harness.unit_count() == 1,
        "spawned": true,
        "ticks": 1500,
        "arrived": arrived,
        "distance": round3(distance),
        "hit_size": snap.hit_size,
        "x": round3(snap.x),
        "y": round3(snap.y),
        "rotation": round3(snap.rotation),
        "unit_count": harness.unit_count(),
        "units_created": harness.units_created,
        "units_removed": harness.units_removed,
        "checksum": checksum,
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

fn tile_center(x: i32, y: i32) -> (f32, f32) {
    let ts = mind_core::config::TILESIZE as f32;
    ((x as f32 + 0.5) * ts, (y as f32 + 0.5) * ts)
}

fn spawn_report(unit: &str, team: u8, x: f32, y: f32, ticks: u64, json: bool) -> Result<i32> {
    let mut harness = UnitHarness::new(64, 64, 7);
    let Some(entity) = harness.spawn(unit, team, x, y, 0.0) else {
        bail!("unknown unit `{unit}`");
    };
    for _ in 0..ticks {
        harness.tick();
    }
    let snap = harness
        .snapshot(entity)
        .ok_or_else(|| anyhow::anyhow!("unit despawned"))?;
    let report = serde_json::json!({
        "scenario": "units_spawn",
        "unit": unit,
        "team": team,
        "ticks": ticks,
        "x": round3(snap.x),
        "y": round3(snap.y),
        "rotation": round3(snap.rotation),
        "health": snap.health,
        "hit_size": snap.hit_size,
        "checksum": harness.checksum_hex(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(EXIT_PASS)
}

fn path_report(unit: &str, tx: i32, ty: i32, ticks: u64, json: bool) -> Result<i32> {
    let mut harness = UnitHarness::new(64, 64, 7);
    let Some(entity) = harness.spawn(unit, 0, 44.0, 44.0, 0.0) else {
        bail!("unknown unit `{unit}`");
    };
    harness.command_move(entity, tx, ty);
    let mut samples = Vec::new();
    let mut arrived_at = None;
    for tick in 0..ticks {
        harness.tick();
        if let Some(snap) = harness.snapshot(entity) {
            samples.push(serde_json::json!({
                "tick": tick,
                "x": round3(snap.x),
                "y": round3(snap.y),
            }));
            if arrived_at.is_none() {
                let (cx, cy) = tile_center(tx, ty);
                let dist = ((snap.x - cx).powi(2) + (snap.y - cy).powi(2)).sqrt();
                if dist <= snap.hit_size.max(4.0) {
                    arrived_at = Some(tick);
                }
            }
        }
    }
    let report = serde_json::json!({
        "scenario": "units_path",
        "unit": unit,
        "target": {"x": tx, "y": ty},
        "ticks": ticks,
        "arrived_at": arrived_at,
        "checksum": harness.checksum_hex(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(if arrived_at.is_some() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn bench(units: usize, ticks: u64, json: bool) -> Result<i32> {
    let mut harness = UnitHarness::new(128, 128, 7);
    let mut entities: Vec<Entity> = Vec::with_capacity(units);
    for i in 0..units {
        let x = 32.0 + (i % 20) as f32 * 16.0;
        let y = 32.0 + ((i / 20) % 20) as f32 * 16.0;
        if let Some(entity) = harness.spawn("dagger", 0, x, y, 0.0) {
            harness.command_move(entity, 120 - (i % 40) as i32, 120);
            entities.push(entity);
        }
    }
    let mut samples = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        let start = Instant::now();
        harness.tick();
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
    samples.sort_unstable();
    let p50 = samples.get(samples.len() * 50 / 100).copied().unwrap_or(0);
    let p99 = samples
        .get((samples.len() * 99 / 100).min(samples.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    let report = serde_json::json!({
        "scenario": "units_bench",
        "units": entities.len(),
        "ticks": ticks,
        "p50_us": p50,
        "p99_us": p99,
        "checksum": harness.checksum_hex(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(EXIT_PASS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_runs_and_arrives() {
        let out = run_scenario("units_spawn_path_arrive").expect("scenario");
        assert!(out.report["pass"].as_bool().unwrap_or(false));
    }

    #[test]
    fn committed_goldens_match() {
        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/units");
        let out = run_scenario("units_spawn_path_arrive").expect("scenario");
        let golden = std::fs::read_to_string(format!("{base}/units_spawn_path_arrive.json"))
            .expect("units golden");
        assert_eq!(
            out.dump, golden,
            "golden mismatch for units_spawn_path_arrive"
        );
    }
}
