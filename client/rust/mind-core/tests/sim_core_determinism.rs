// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 05 M8 integration oracle: the `sim_core_determinism` scenario's sampled
//! checkpoints must match `tests/golden/sim_core_determinism.checksums`, and the
//! binary `.simlog` form must round-trip and replay to the same final checksum
//! (plan 05 §6.4/§7.2).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use mind_core::content::{BlockId, Blocks};
use mind_core::determinism::{CommandLog, LogHeader, SimCommand};
use mind_core::scenario::{Scenario, ScenarioPlayer};
use mind_core::sim::Sim;

fn scenario_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scenarios/sim_core_determinism.json")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/sim_core_determinism.checksums")
}

fn sampled_checksums(scenario: &Scenario, every: usize) -> Vec<String> {
    let mut sim = Sim::from_scenario(scenario).expect("sim boot");
    let mut player = ScenarioPlayer::new(scenario, sim.content()).expect("player");
    let mut per_tick = Vec::new();
    while player.step(&mut sim).expect("step") {
        per_tick.push(sim.checksum_hex());
    }
    per_tick
        .iter()
        .enumerate()
        .filter(|(index, _)| (index + 1) % every == 0)
        .map(|(_, checksum)| checksum.clone())
        .collect()
}

#[test]
fn sampled_checksums_match_golden_across_runs() {
    let scenario = Scenario::read(scenario_path()).expect("scenario reads");
    let first = sampled_checksums(&scenario, 60);
    let second = sampled_checksums(&scenario, 60);
    assert_eq!(first, second, "two in-process runs must agree");
    let golden_text = std::fs::read_to_string(golden_path()).expect("golden committed");
    let golden: Vec<String> = golden_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(!golden.is_empty());
    assert_eq!(
        first, golden,
        "sampled determinism checksums drifted; regenerate with \
         `mind-headless run sim_core_determinism --checksum-every 60 --emit-checksums`"
    );
}

#[test]
fn binary_simlog_roundtrip_replays_to_golden_checksum() {
    let scenario = Scenario::read(scenario_path()).expect("scenario reads");
    let blocks = Blocks::new();
    let mut log = CommandLog::new(LogHeader::new(scenario.seed, &scenario.name));
    for record in scenario.resolve_commands(&blocks).expect("commands") {
        log.push(record.tick, SimCommand::from_p0(record.command));
    }
    let bytes = log.to_bytes();
    assert!(CommandLog::is_binary(&bytes));
    let decoded = CommandLog::from_bytes(&bytes).expect("decodes");
    assert_eq!(decoded, log, "binary `.simlog` must round-trip");

    // Replay the decoded log through `Sim::command` (plan 21's ordered path).
    let mut sim = Sim::new(
        decoded.header.seed,
        scenario.world.width,
        scenario.world.height,
        BlockId::AIR,
        BlockId::AIR,
    );
    let mut next = 0usize;
    for tick in 0..scenario.steps {
        while next < decoded.entries.len() && decoded.entries[next].0 <= tick {
            sim.command(decoded.entries[next].1.clone())
                .expect("place/break commands are representable");
            next += 1;
        }
        sim.tick().expect("tick");
    }
    let expected = scenario
        .expect
        .as_ref()
        .and_then(|expect| expect.checksum.clone())
        .expect("scenario has a golden checksum");
    assert_eq!(sim.checksum_hex(), expected);
}
