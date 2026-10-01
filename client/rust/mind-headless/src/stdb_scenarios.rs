// SPDX-License-Identifier: GPL-3.0-only

//! `stdb_*` scenarios (plan 01 §7.2): network-free proofs of the `mind-stdb`
//! connector/binder/relay contracts, using `scenarios/stdb_*.json` fixtures.
//!
//! These scenarios never open a socket: `stdb_offline_boot` drives
//! `StdbMode::Offline`; the binder/order scenarios feed synthetic rows. Live
//! relay verification is env-gated (`MIND_STDB_IT=1`) integration tests, not
//! harness scenarios.

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, anyhow};
use mind_stdb::{ConnectionConfig, Connector, StdbMode};
use serde::{Deserialize, Serialize};

use crate::cli::Cli;
use crate::paths;
use crate::report::StdbReport;

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// A registered `stdb_*` scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdbScenario {
    /// Offline connector boots, pumps N times, reports `offline`.
    OfflineBoot,
}

impl StdbScenario {
    /// Scenario name as registered in [`crate::registry`].
    pub fn name(self) -> &'static str {
        match self {
            Self::OfflineBoot => "stdb_offline_boot",
        }
    }

    /// Maps a registered scenario name to its `stdb_*` handler.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "stdb_offline_boot" => Some(Self::OfflineBoot),
            _ => None,
        }
    }
}

/// `stdb_offline_boot` fixture (`scenarios/stdb_offline_boot.json`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OfflineBootScenario {
    /// Scenario name (must match the registry).
    pub name: String,
    /// Number of `pump()` calls to drive.
    pub pumps: u64,
    /// Golden expectations.
    #[serde(default)]
    pub expect: Option<OfflineBootExpect>,
}

/// Golden expectations for [`OfflineBootScenario`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OfflineBootExpect {
    /// Expected final connector state name (`"offline"`).
    pub state: String,
    /// Expected `frame_count()`.
    pub frames: u64,
    /// Optional p99 budget in microseconds (plan §7.4).
    #[serde(default)]
    pub pump_p99_us: Option<u64>,
}

/// Runs a registered `stdb_*` scenario; returns the process exit code.
pub fn run(cli: &Cli, kind: StdbScenario, dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    match kind {
        StdbScenario::OfflineBoot => run_offline_boot(cli, dump, json),
    }
}

fn run_offline_boot(cli: &Cli, dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let fixture_path =
        fixture_file(cli, StdbScenario::OfflineBoot).context("resolving stdb fixture")?;
    let text = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("reading `{}`", fixture_path.display()))?;
    let scenario: OfflineBootScenario = serde_json::from_str(&text)
        .with_context(|| format!("parsing `{}`", fixture_path.display()))?;

    let config = ConnectionConfig {
        mode: StdbMode::Offline,
        token_store_path: Some(std::env::temp_dir().join("mind-headless-stdb")),
        ..ConnectionConfig::local()
    };
    let mut connector = Connector::new(config);
    connector
        .connect()
        .map_err(|error| anyhow!("offline connect returned an error: {error}"))?;

    let mut samples = Vec::with_capacity(usize::try_from(scenario.pumps).unwrap_or(0));
    for _ in 0..scenario.pumps {
        let start = Instant::now();
        connector.pump();
        samples.push(start.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let p50_ns = percentile(&samples, 50);
    let p99_ns = percentile(&samples, 99);

    let state = connector.state().name().to_string();
    let frames = connector.frame_count();

    let mut pass = state == "offline" && frames == scenario.pumps;
    let mut expect_state = None;
    let mut expect_frames = None;
    if let Some(expect) = &scenario.expect {
        expect_state = Some(expect.state.clone());
        expect_frames = Some(expect.frames);
        if state != expect.state {
            log::error!(
                "stdb_offline_boot: state `{state}` != expected `{}`",
                expect.state
            );
            pass = false;
        }
        if frames != expect.frames {
            log::error!(
                "stdb_offline_boot: frames {frames} != expected {}",
                expect.frames
            );
            pass = false;
        }
        if let Some(budget_us) = expect.pump_p99_us {
            let p99_us = p99_ns.div_ceil(1_000);
            if p99_us > budget_us {
                log::error!("stdb_offline_boot: pump p99 {p99_us}us exceeds {budget_us}us");
                pass = false;
            }
        }
    }

    if let Some(path) = dump {
        let dump_json = serde_json::json!({
            "scenario": scenario.name,
            "mode": "offline",
            "state": state,
            "frames": frames,
        });
        let decoded: serde_json::Value =
            serde_json::from_value(dump_json.clone()).context("dump failed schema round-trip")?;
        std::fs::write(path, serde_json::to_string_pretty(&decoded)?)
            .with_context(|| format!("writing dump `{}`", path.display()))?;
    }

    let report = StdbReport {
        scenario: scenario.name,
        mode: "offline".to_string(),
        state,
        frames,
        pumps: scenario.pumps,
        pump_p50_ns: p50_ns,
        pump_p99_ns: p99_ns,
        pass,
        expect_state,
        expect_frames,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", serde_json::to_string(&report)?);
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn fixture_file(cli: &Cli, kind: StdbScenario) -> anyhow::Result<std::path::PathBuf> {
    let fixture = crate::registry::find(kind.name())
        .ok_or_else(|| anyhow!("`{}` is not registered", kind.name()))?;
    let dir = paths::find_scenarios_dir(cli.scenarios_dir.as_deref())?;
    Ok(dir.join(fixture.file_name()))
}

fn percentile(samples: &[u64], percent: usize) -> u64 {
    debug_assert!(!samples.is_empty());
    if samples.is_empty() {
        return 0;
    }
    let index = (samples.len() * percent / 100).min(samples.len().saturating_sub(1));
    samples.get(index).copied().unwrap_or(0)
}
