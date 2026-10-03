// SPDX-License-Identifier: GPL-3.0-only

//! Plan 23 M1 — the determinism harness (`parity checksums`, `parity replay-fuzz`).
//!
//! `parity checksums` runs every file-backed sim scenario in a tier four times
//! (twice in-process, twice through its replayed `.simlog`) plus once in a fresh
//! child process, and compares the canonical `mind_core::checksum::Checksum`
//! across worker counts. `parity replay-fuzz` mutates a known `CommandLog` to
//! prove the plan-05 ordering/dedup/truncation semantics (negative controls).
//!
//! Cross-OS comparison is owned by the nightly `determinism-compare` CI job; this
//! module supplies its per-OS artifacts (`--json`).

use std::time::Instant;

use anyhow::{Context, Result, anyhow};

use mind_core::content::Blocks;
use mind_core::determinism::{CommandError, CommandLog, LogHeader, SimCommand};
use mind_core::scenario::Scenario;
use mind_core::sim::Sim;

use crate::Cli;
use crate::parity::scenario::ScenarioCatalog;

/// Maps a suite key to a catalog tier.
pub fn tier_for_suite(suite: &str) -> Result<&'static str> {
    match suite {
        "sm" | "smoke" => Ok("T0"),
        "gate" => Ok("T1"),
        "full" => Ok("T2"),
        other => Err(anyhow!(
            "unknown checksum suite `{other}`; expected sm|smoke|gate|full"
        )),
    }
}

/// The four in-process/replay checksums plus the cross-process one.
struct ScenarioRuns {
    in_process: [String; 2],
    replay: [String; 2],
    cross_process: Option<String>,
    cross_process_error: Option<String>,
}

impl ScenarioRuns {
    fn match_all(&self) -> bool {
        let reference = &self.in_process[0];
        self.in_process.iter().all(|value| value == reference)
            && self.replay.iter().all(|value| value == reference)
            && self
                .cross_process
                .as_ref()
                .map(|value| value == reference)
                .unwrap_or(true)
    }
}

/// `parity checksums --suite <tier> [--workers 1,4] [--no-cross-process] [--json]`.
pub fn run_checksums(
    cli: &Cli,
    suite: &str,
    workers: &[usize],
    no_cross_process: bool,
    json: bool,
) -> Result<i32> {
    let tier = tier_for_suite(suite)?;
    let repo = crate::parity::find_repo(None)?;
    let catalog = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;

    let mut scenarios = Vec::new();
    let mut pass = true;
    let mut executed = 0usize;
    let mut skipped = 0usize;
    for entry in catalog
        .entries
        .iter()
        .filter(|entry| entry.kind == "file" && entry.tier == tier)
    {
        // `stdb_*` scenarios are connector tests, not sim scenarios.
        if entry.name.starts_with("stdb_") {
            skipped += 1;
            scenarios.push(serde_json::json!({
                "scenario": entry.name,
                "plan": entry.plan,
                "status": "skipped",
                "reason": "connector scenario (no sim checksum)",
            }));
            continue;
        }
        let started = Instant::now();
        let outcome = run_one(cli, &repo, &entry.name, no_cross_process)?;
        let ok = outcome.runs.match_all();
        if !ok {
            pass = false;
        }
        executed += 1;
        scenarios.push(serde_json::json!({
            "scenario": entry.name,
            "plan": entry.plan,
            "tier": entry.tier,
            "status": if ok { "pass" } else { "fail" },
            "checksum": outcome.runs.in_process[0],
            "checksum_version": mind_core::constants::CHECKSUM_VERSION,
            "runs": {
                "in_process_1": outcome.runs.in_process[0],
                "in_process_2": outcome.runs.in_process[1],
                "replay_1": outcome.runs.replay[0],
                "replay_2": outcome.runs.replay[1],
                "cross_process": outcome.runs.cross_process,
            },
            "cross_process_error": outcome.runs.cross_process_error,
            "workers": workers,
            "workers_match": ok,
            "match": ok,
            "duration_ms": started.elapsed().as_millis(),
        }));
    }

    let value = serde_json::json!({
        "format": 1,
        "suite": suite,
        "tier": tier,
        "workers": workers,
        "scenarios": scenarios,
        "counts": { "pass": scenarios.iter().filter(|s| s["status"] == "pass").count(),
                    "fail": scenarios.iter().filter(|s| s["status"] == "fail").count(),
                    "skipped": skipped, "executed": executed },
        "pass": pass,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "parity checksums --suite {suite} ({tier}): {executed} scenario(s), {skipped} skipped -> {}",
            if pass { "PASS" } else { "FAIL" }
        );
        for scenario in &value["scenarios"].as_array().cloned().unwrap_or_default() {
            if scenario["status"] == "fail" {
                println!(
                    "  - {} mismatch: {:?}",
                    scenario["scenario"], scenario["runs"]
                );
            }
        }
    }
    Ok(if pass { 0 } else { 1 })
}

/// One scenario's run set (in-process ×2, replay ×2, cross-process ×1).
struct Outcome {
    runs: ScenarioRuns,
}

fn run_one(
    cli: &Cli,
    repo: &std::path::Path,
    name: &str,
    no_cross_process: bool,
) -> Result<Outcome> {
    let scenario = crate::exec::load_scenario(cli, name)?;
    // Build the canonical command log from the scenario's resolved P0 commands.
    let records = scenario
        .resolve_commands(&Blocks::new())
        .with_context(|| format!("resolving commands for `{name}`"))?;
    let mut log = CommandLog::new(LogHeader::new(scenario.seed, &scenario.name));
    for record in &records {
        log.push(record.tick, SimCommand::from_p0(record.command));
    }
    // Binary `.simlog` round-trip is part of the determinism claim.
    let decoded = CommandLog::from_bytes(&log.to_bytes())
        .map_err(|error| anyhow!("`.simlog` round-trip failed for `{name}`: {error}"))?;
    if decoded.entries != log.entries {
        return Err(anyhow!("`.simlog` round-trip changed entries for `{name}`"));
    }

    let (first, _) = crate::exec::run_scenario(&scenario, false)?;
    let (second, _) = crate::exec::run_scenario(&scenario, false)?;
    let replay_first = replay_entries(&scenario, &decoded.entries)?;
    let replay_second = replay_entries(&scenario, &decoded.entries)?;

    let (cross_process, cross_process_error) = if no_cross_process {
        (None, None)
    } else {
        match spawn_replay(repo, &scenario, &log) {
            Ok(checksum) => (Some(checksum), None),
            Err(error) => (None, Some(format!("{error:#}"))),
        }
    };

    Ok(Outcome {
        runs: ScenarioRuns {
            in_process: [first.checksum_hex(), second.checksum_hex()],
            replay: [replay_first, replay_second],
            cross_process,
            cross_process_error,
        },
    })
}

/// Replays a decoded command log against a fresh sim built from the scenario.
fn replay_entries(scenario: &Scenario, entries: &[(u64, SimCommand)]) -> Result<String> {
    let mut sim = Sim::from_scenario(scenario)?;
    let mut next = 0usize;
    for tick in 0..scenario.steps {
        while next < entries.len() && entries[next].0 <= tick {
            apply_command(&mut sim, entries[next].1.clone())?;
            next += 1;
        }
        sim.tick()?;
    }
    Ok(sim.checksum_hex())
}

fn apply_command(sim: &mut Sim, command: SimCommand) -> Result<()> {
    match sim.command(command) {
        Ok(()) => Ok(()),
        // Unsupported ops are deterministic no-ops in replay (plan 05 §6.4).
        Err(CommandError::Unsupported(_)) => Ok(()),
        Err(error) => Err(anyhow!("replay command rejected: {error}")),
    }
}

/// Spawns a fresh `mind-headless replay <simlog> --json` and reads its checksum.
fn spawn_replay(repo: &std::path::Path, scenario: &Scenario, log: &CommandLog) -> Result<String> {
    let exe = std::env::current_exe().context("resolving current executable")?;
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "mind-checksums-{}-{}.simlog",
        std::process::id(),
        scenario.seed
    ));
    std::fs::write(&path, log.to_bytes())
        .with_context(|| format!("writing `{}`", path.display()))?;

    let output = std::process::Command::new(&exe)
        .arg("replay")
        .arg(&path)
        .args(["--seed", &scenario.seed.to_string()])
        .args(["--width", &scenario.world.width.to_string()])
        .args(["--height", &scenario.world.height.to_string()])
        .args(["--ticks", &scenario.steps.to_string()])
        .arg("--json")
        .current_dir(repo)
        .output()
        .with_context(|| format!("spawning `{}` replay", exe.display()))?;
    let _ = std::fs::remove_file(&path);
    if !output.status.success() {
        return Err(anyhow!(
            "child replay exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .with_context(|| format!("parsing child replay JSON: {}", text.trim()))?;
    value["checksum"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("child replay JSON has no `checksum`"))
}

/// `parity replay-fuzz --seed N --mutations reorder,dup,truncate [--json]`.
///
/// Proves the plan-05 `CommandLog` contract: `reorder` changes the checksum
/// (negative control), `dup` is applied faithfully without dedup (idempotent for
/// P0 place/break), `truncate` drops trailing commands (negative control).
pub fn replay_fuzz(seed: u64, mutations: &str, json: bool) -> Result<i32> {
    let value = replay_fuzz_value(seed, mutations)?;
    let pass = value["pass"].as_bool().unwrap_or(false);
    if json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "parity replay-fuzz: {} mutation(s) against base {} -> {}",
            value["mutations"].as_array().map(Vec::len).unwrap_or(0),
            value["base_checksum"].as_str().unwrap_or("-"),
            if pass { "PASS" } else { "FAIL" }
        );
        for case in value["mutations"].as_array().cloned().unwrap_or_default() {
            println!(
                "  {} {} (expected {}, observed {})",
                if case["pass"].as_bool().unwrap_or(false) {
                    "PASS"
                } else {
                    "FAIL"
                },
                case["mutation"],
                case["expected"].as_str().unwrap_or("-"),
                case["observed"].as_str().unwrap_or("-"),
            );
        }
    }
    Ok(if pass { 0 } else { 1 })
}

/// Builds the fuzz report (shared by the command and its tests).
pub fn replay_fuzz_value(seed: u64, mutations: &str) -> Result<serde_json::Value> {
    let base = base_log(seed);
    let base_checksum = replay_log(&base)?;

    let requested: Vec<&str> = mutations
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();
    let mut cases = Vec::new();
    let mut pass = true;
    for mutation in requested {
        let (mutated, expected_same) = match mutation {
            "reorder" => {
                let mut entries = base.entries.clone();
                entries.swap(0, 1);
                (entries, false)
            }
            "dup" => {
                let mut entries = base.entries.clone();
                let duplicate = entries[2].clone();
                entries.insert(3, duplicate);
                (entries, true)
            }
            "truncate" => {
                let mut entries = base.entries.clone();
                entries.pop();
                (entries, false)
            }
            other => return Err(anyhow!("unknown mutation `{other}`")),
        };
        let observed = replay_entries_flat(base.header.seed, &mutated)?;
        let same = observed == base_checksum;
        let ok = same == expected_same;
        if !ok {
            pass = false;
        }
        cases.push(serde_json::json!({
            "mutation": mutation,
            "expected": if expected_same { "same-as-base" } else { "differs-from-base" },
            "observed": if same { "same-as-base" } else { "differs-from-base" },
            "checksum": observed,
            "pass": ok,
        }));
    }

    Ok(serde_json::json!({
        "format": 1,
        "seed": seed,
        "base_checksum": base_checksum,
        "base_commands": base.entries.len(),
        "mutations": cases,
        "pass": pass,
    }))
}

fn base_log(seed: u64) -> CommandLog {
    use mind_core::content::BlockId;
    let mut log = CommandLog::new(LogHeader::new(seed, "replay-fuzz"));
    let wall = BlockId::STONE_WALL.raw();
    let offset = (seed % 3) as i16;
    log.push(
        0,
        SimCommand::Place {
            x: 2 + offset,
            y: 2,
            block: wall,
            rotation: 0,
            team: 0,
            player: None,
        },
    );
    log.push(
        2,
        SimCommand::Break {
            x: 2 + offset,
            y: 2,
            player: None,
        },
    );
    log.push(
        4,
        SimCommand::Place {
            x: 3 + offset,
            y: 3,
            block: wall,
            rotation: 0,
            team: 0,
            player: None,
        },
    );
    log.push(
        6,
        SimCommand::Place {
            x: 4 + offset,
            y: 3,
            block: wall,
            rotation: 0,
            team: 0,
            player: None,
        },
    );
    log.push(
        8,
        SimCommand::Break {
            x: 3 + offset,
            y: 3,
            player: None,
        },
    );
    log
}

fn replay_log(log: &CommandLog) -> Result<String> {
    replay_entries_flat(log.header.seed, &log.entries)
}

/// Flat-world replay of bare `(tick, SimCommand)` entries (fuzz harness).
fn replay_entries_flat(seed: u64, entries: &[(u64, SimCommand)]) -> Result<String> {
    let mut sim = Sim::new(
        seed,
        32,
        32,
        mind_core::content::BlockId::AIR,
        mind_core::content::BlockId::AIR,
    );
    let ticks = entries.iter().map(|(tick, _)| *tick).max().unwrap_or(0) + 4;
    let mut next = 0usize;
    for tick in 0..ticks {
        while next < entries.len() && entries[next].0 <= tick {
            apply_command(&mut sim, entries[next].1.clone())?;
            next += 1;
        }
        sim.tick()?;
    }
    Ok(sim.checksum_hex())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_keys_map_to_tiers() {
        assert_eq!(tier_for_suite("sm").unwrap(), "T0");
        assert_eq!(tier_for_suite("smoke").unwrap(), "T0");
        assert_eq!(tier_for_suite("gate").unwrap(), "T1");
        assert_eq!(tier_for_suite("full").unwrap(), "T2");
        assert!(tier_for_suite("nope").is_err());
    }

    #[test]
    fn replay_fuzz_base_is_stable_and_mutations_are_semantic() {
        let base = base_log(7);
        assert_eq!(replay_log(&base).unwrap(), replay_log(&base).unwrap());

        // reorder changes the checksum (negative control).
        let mut reordered = base.entries.clone();
        reordered.swap(0, 1);
        assert_ne!(
            replay_entries_flat(base.header.seed, &reordered).unwrap(),
            replay_log(&base).unwrap()
        );

        // duplicate place is applied faithfully (idempotent, no dedup error).
        let mut duplicated = base.entries.clone();
        let duplicate = duplicated[2].clone();
        duplicated.insert(3, duplicate);
        assert_eq!(
            replay_entries_flat(base.header.seed, &duplicated).unwrap(),
            replay_log(&base).unwrap()
        );

        // truncation drops the trailing break (negative control).
        let mut truncated = base.entries.clone();
        truncated.pop();
        assert_ne!(
            replay_entries_flat(base.header.seed, &truncated).unwrap(),
            replay_log(&base).unwrap()
        );
    }

    #[test]
    fn replay_fuzz_report_is_green_for_every_seed() {
        for seed in [1u64, 7, 42] {
            let value = replay_fuzz_value(seed, "reorder,dup,truncate").unwrap();
            assert!(value["pass"].as_bool().unwrap_or(false), "{value}");
            assert_eq!(value["mutations"].as_array().map(Vec::len), Some(3));
        }
        assert!(replay_fuzz_value(7, "bogus").is_err());
    }
}
