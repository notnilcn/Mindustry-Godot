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
use mind_core::determinism::Checksummer;

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
    &[
        "units_spawn_path_arrive",
        "units_formation",
        "units_spawn_group",
        "units_weapon_fire",
    ]
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
        "units_formation" => formation(),
        "units_spawn_group" => spawn_group(),
        "units_weapon_fire" => weapon_fire(),
        other => bail!("unknown units scenario `{other}`"),
    }
}

/// `units_weapon_fire`: a real `dagger` fires its plan-10 weapon mounts at a
/// wall and damages it (plan 11 §5 M2 weapon-mount wiring).
fn weapon_fire() -> Result<ScenarioOutput> {
    let mut harness = UnitHarness::new(32, 16, 7);
    let wall = harness
        .content()
        .block_id("copper-wall")
        .ok_or_else(|| anyhow::anyhow!("copper-wall missing from content"))?;
    harness.build.rules.default_team = 1;
    assert!(harness.build.place(12, 8, wall, 0, true), "wall placed");
    let (ux, uy) = tile_center(4, 8);
    let unit = harness
        .spawn("dagger", 0, ux, uy, 0.0)
        .ok_or_else(|| anyhow::anyhow!("dagger missing from content"))?;
    let aim = tile_center(12, 8);
    let mount_count = harness
        .unit_weapons(unit)
        .map(|w| w.mounts.len())
        .unwrap_or(0);
    for index in 0..mount_count {
        harness.set_weapon_aim(unit, index, aim);
        harness.set_weapon_shoot(unit, index, true);
    }
    let before = round3(harness.building_health_at(12, 8));
    for _ in 0..240 {
        harness.tick();
    }
    let after = round3(harness.building_health_at(12, 8));
    let pass = mount_count > 0 && harness.bullets_created > 0 && after < before;
    let report = serde_json::json!({
        "scenario": "units_weapon_fire",
        "pass": pass,
        "unit": "dagger",
        "mounts": mount_count,
        "ticks": 240,
        "wall_health_before": before,
        "wall_health_after": after,
        "bullets_created": harness.bullets_created,
        "bullets_removed": harness.bullets_removed,
        "bullets_live": harness.bullets.len(),
        "checksum": harness.checksum_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
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

/// `units_formation`: deterministic `UnitGroup` packing for a 10-unit squad.
///
/// Exercises plan 11 §3.8 deviation 5 (synchronous, join-free formation). The
/// per-unit `ControlPathfinder` raycast clamp is not on this branch, so the
/// dump covers the compression/physics result only.
fn formation() -> Result<ScenarioOutput> {
    use mind_core::ai::UnitGroup;
    use mind_core::ai::unit_group::FormationUnit;

    let units: Vec<FormationUnit> = (0..10)
        .map(|i| {
            FormationUnit::new(
                200.0 + (i % 3) as f32 * 0.75,
                200.0 + (i / 3) as f32 * 0.75,
                8.0,
            )
        })
        .collect();
    let mut group = UnitGroup::new();
    group.set_units(units);
    group.calculate_formation(0);
    let mut checksummer = Checksummer::new();
    let offsets: Vec<serde_json::Value> = group
        .positions
        .iter()
        .map(|(x, y)| {
            checksummer.part(x);
            checksummer.part(y);
            serde_json::json!({ "x": round3(*x), "y": round3(*y) })
        })
        .collect();
    let pass = group.valid && group.positions.len() == 10;
    let report = serde_json::json!({
        "scenario": "units_formation",
        "pass": pass,
        "unit_count": group.positions.len(),
        "offsets": offsets,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_spawn_group`: `SpawnGroup` scaling math across waves (plan 11 §3.10).
///
/// Runs the `getSpawned`/`getShield` formulas for a scaling group and a boss
/// group, dumping per-wave counts and shields plus a determinism checksum.
fn spawn_group() -> Result<ScenarioOutput> {
    use mind_core::game::spawn_group::SpawnGroup;

    let mut scaling = SpawnGroup::new("dagger");
    scaling.unit_scaling = 2.0;
    scaling.max = 20;

    let mut boss = SpawnGroup::new("mace");
    boss.begin = 10;
    boss.spacing = 5;
    boss.unit_amount = 1;
    boss.shields = 500.0;
    boss.shield_scaling = 100.0;

    let mut checksummer = Checksummer::new();
    let mut waves = Vec::new();
    let mut pass = true;
    for wave in 0..25 {
        let scaling_count = scaling.get_spawned(wave);
        let boss_count = boss.get_spawned(wave);
        let boss_shield = boss.get_shield(wave);
        pass &= (0..=scaling.max).contains(&scaling_count)
            && (0..=boss.max).contains(&boss_count)
            && boss_shield >= 0.0;
        checksummer.part(&scaling_count);
        checksummer.part(&boss_count);
        checksummer.part(&boss_shield);
        waves.push(serde_json::json!({
            "wave": wave,
            "dagger": scaling_count,
            "mace": boss_count,
            "mace_shield": round3(boss_shield),
        }));
    }
    let report = serde_json::json!({
        "scenario": "units_spawn_group",
        "pass": pass,
        "waves": waves,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
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
        for name in names() {
            let out = run_scenario(name).expect("scenario");
            let golden = std::fs::read_to_string(format!("{base}/{name}.json"))
                .unwrap_or_else(|_| panic!("missing units golden for {name}"));
            assert_eq!(out.dump, golden, "golden mismatch for {name}");
        }
    }
}
