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
use mind_stdb::module_bindings::{
    CommandKind, MatchCommand, ProtocolInfo, ProtocolInfoTableAccessor,
};
use mind_stdb::{CommandStream, ConnectionConfig, Connector, OrderError, RowChange, StdbMode};
use serde::{Deserialize, Serialize};
use spacetimedb_sdk::{Identity, Timestamp};

use crate::cli::Cli;
use crate::paths;
use crate::report::{StdbBinderReport, StdbOrderReport, StdbReport};

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// A registered `stdb_*` scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdbScenario {
    /// Offline connector boots, pumps N times, reports `offline`.
    OfflineBoot,
    /// Binder replay/order contract against a synthetic row source.
    BinderReplay,
    /// CommandStream ordering/dedup/gap contract with canned command rows.
    CommandOrder,
}

impl StdbScenario {
    /// Scenario name as registered in [`crate::registry`].
    pub fn name(self) -> &'static str {
        match self {
            Self::OfflineBoot => "stdb_offline_boot",
            Self::BinderReplay => "stdb_binder_replay",
            Self::CommandOrder => "stdb_command_order",
        }
    }

    /// Maps a registered scenario name to its `stdb_*` handler.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "stdb_offline_boot" => Some(Self::OfflineBoot),
            "stdb_binder_replay" => Some(Self::BinderReplay),
            "stdb_command_order" => Some(Self::CommandOrder),
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
        StdbScenario::BinderReplay => run_binder_replay(cli, dump, json),
        StdbScenario::CommandOrder => run_command_order(cli, dump, json),
    }
}

/// `stdb_command_order` fixture (`scenarios/stdb_command_order.json`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandOrderScenario {
    /// Scenario name (must match the registry).
    pub name: String,
    /// Match both the stream and canned rows use.
    pub match_id: u64,
    /// Rows injected out of arrival order.
    pub rows: Vec<CannedCommand>,
    /// An already-delivered row injected again (must be ignored).
    pub duplicate: CannedCommand,
    /// Rows with a per-sender sequence gap.
    pub gap_rows: Vec<CannedCommand>,
    /// Golden expectations.
    #[serde(default)]
    pub expect: Option<CommandOrderExpect>,
}

/// One canned command row (subset of `MatchCommand`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CannedCommand {
    /// Global commit order.
    pub command_id: u64,
    /// Sender selector (repeated as a byte array identity).
    pub sender: u8,
    /// Per-sender sequence.
    pub sender_seq: u64,
    /// Sender's sim tick.
    pub client_tick: u64,
}

impl CannedCommand {
    fn to_row(&self, match_id: u64) -> MatchCommand {
        MatchCommand {
            command_id: self.command_id,
            match_id,
            sender: Identity::from_byte_array([self.sender; 32]),
            sender_seq: self.sender_seq,
            client_tick: self.client_tick,
            kind: CommandKind::Ping(self.command_id),
            sent_at: Timestamp::UNIX_EPOCH,
        }
    }
}

/// Golden expectations for [`CommandOrderScenario`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandOrderExpect {
    /// Expected applied `command_id` order after the first batch.
    pub applied_order: Vec<u64>,
    /// Whether [`CommandOrderScenario::duplicate`] must be ignored.
    pub duplicate_ignored: bool,
    /// Expected `sender_seq` in the gap error.
    pub gap_expected: u64,
    /// Received `sender_seq` in the gap error.
    pub gap_got: u64,
}

fn run_command_order(cli: &Cli, dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let fixture_path =
        fixture_file(cli, StdbScenario::CommandOrder).context("resolving stdb fixture")?;
    let text = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("reading `{}`", fixture_path.display()))?;
    let scenario: CommandOrderScenario = serde_json::from_str(&text)
        .with_context(|| format!("parsing `{}`", fixture_path.display()))?;

    let config = ConnectionConfig {
        mode: StdbMode::Offline,
        token_store_path: Some(std::env::temp_dir().join("mind-headless-stdb")),
        ..ConnectionConfig::local()
    };
    let mut connector = Connector::new(config);
    let mut stream = CommandStream::subscribe(&mut connector, scenario.match_id);

    for canned in &scenario.rows {
        stream.inject(canned.to_row(scenario.match_id));
    }
    let applied_order: Vec<u64> = stream
        .drain()
        .into_iter()
        .map(|row| row.command_id)
        .collect();

    stream.inject(scenario.duplicate.to_row(scenario.match_id));
    let duplicate_ignored = stream.drain().is_empty();

    for canned in &scenario.gap_rows {
        stream.inject(canned.to_row(scenario.match_id));
    }
    let _ = stream.drain();
    let order_error = stream.order_error().map(|error| match error {
        OrderError::Gap { expected, got, .. } => format!("gap: expected {expected}, got {got}"),
        OrderError::Regression { previous, incoming } => {
            format!("regression: {previous} -> {incoming}")
        }
    });

    let mut pass = true;
    if let Some(expect) = &scenario.expect {
        if applied_order != expect.applied_order {
            log::error!(
                "stdb_command_order: applied {applied_order:?} != {:?}",
                expect.applied_order
            );
            pass = false;
        }
        if duplicate_ignored != expect.duplicate_ignored {
            log::error!("stdb_command_order: duplicate was not ignored");
            pass = false;
        }
        let expect_error = format!(
            "gap: expected {}, got {}",
            expect.gap_expected, expect.gap_got
        );
        if order_error.as_deref() != Some(expect_error.as_str()) {
            log::error!("stdb_command_order: order error {order_error:?} != `{expect_error}`");
            pass = false;
        }
    }

    if let Some(path) = dump {
        let dump_json = serde_json::json!({
            "scenario": scenario.name,
            "applied_order": applied_order,
            "duplicate_ignored": duplicate_ignored,
            "order_error": order_error,
        });
        std::fs::write(path, serde_json::to_string_pretty(&dump_json)?)
            .with_context(|| format!("writing dump `{}`", path.display()))?;
    }

    let report = StdbOrderReport {
        scenario: scenario.name,
        pass,
        applied_order,
        duplicate_ignored,
        order_error,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", serde_json::to_string(&report)?);
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// `stdb_binder_replay` fixture (`scenarios/stdb_binder_replay.json`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BinderReplayScenario {
    /// Scenario name (must match the registry).
    pub name: String,
    /// Row versions replayed from the (synthetic) cache.
    pub cached_rows: Vec<u32>,
    /// Row version delivered as a live insert after replay.
    pub live_row: u32,
    /// Golden expectations.
    #[serde(default)]
    pub expect: Option<BinderReplayExpect>,
}

/// Golden expectations for [`BinderReplayScenario`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BinderReplayExpect {
    /// Expected replay drain order.
    pub replay_order: Vec<u32>,
    /// Expected live drain order.
    pub live_order: Vec<u32>,
}

fn protocol_row(version: u32) -> ProtocolInfo {
    ProtocolInfo {
        id: version as u8,
        protocol_version: version,
        min_client_build: 1,
        save_format_version: 1,
    }
}

fn run_binder_replay(cli: &Cli, dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let fixture_path =
        fixture_file(cli, StdbScenario::BinderReplay).context("resolving stdb fixture")?;
    let text = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("reading `{}`", fixture_path.display()))?;
    let scenario: BinderReplayScenario = serde_json::from_str(&text)
        .with_context(|| format!("parsing `{}`", fixture_path.display()))?;

    let config = ConnectionConfig {
        mode: StdbMode::Offline,
        token_store_path: Some(std::env::temp_dir().join("mind-headless-stdb")),
        ..ConnectionConfig::local()
    };
    let mut connector = Connector::new(config);
    let binder = connector.bind::<ProtocolInfoTableAccessor>("protocol_info");

    // Cache replay: rows enqueue as Insert in iteration order.
    binder.replay(scenario.cached_rows.iter().copied().map(protocol_row));
    let replay_order: Vec<u32> = binder
        .drain()
        .into_iter()
        .map(|change| match change {
            RowChange::Insert(row) => row.protocol_version,
            RowChange::Update { new, .. } => new.protocol_version,
            RowChange::Delete(row) => row.protocol_version,
        })
        .collect();

    // Live delivery arrives after replay; no duplicates in the drained queue.
    binder.inject(RowChange::Insert(protocol_row(scenario.live_row)));
    let live_order: Vec<u32> = binder
        .drain()
        .into_iter()
        .map(|change| match change {
            RowChange::Insert(row) => row.protocol_version,
            RowChange::Update { new, .. } => new.protocol_version,
            RowChange::Delete(row) => row.protocol_version,
        })
        .collect();

    let mut pass = true;
    let mut expect_replay = None;
    let mut expect_live = None;
    if let Some(expect) = &scenario.expect {
        expect_replay = Some(expect.replay_order.clone());
        expect_live = Some(expect.live_order.clone());
        if replay_order != expect.replay_order {
            log::error!(
                "stdb_binder_replay: replay order {replay_order:?} != {:?}",
                expect.replay_order
            );
            pass = false;
        }
        if live_order != expect.live_order {
            log::error!(
                "stdb_binder_replay: live order {live_order:?} != {:?}",
                expect.live_order
            );
            pass = false;
        }
        if binder.pending() != 0 {
            log::error!(
                "stdb_binder_replay: {} changes left undrained",
                binder.pending()
            );
            pass = false;
        }
    }

    if let Some(path) = dump {
        let dump_json = serde_json::json!({
            "scenario": scenario.name,
            "replay_order": replay_order,
            "live_order": live_order,
        });
        std::fs::write(path, serde_json::to_string_pretty(&dump_json)?)
            .with_context(|| format!("writing dump `{}`", path.display()))?;
    }

    let report = StdbBinderReport {
        scenario: scenario.name,
        pass,
        replay_order,
        live_order,
        expect_replay,
        expect_live,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", serde_json::to_string(&report)?);
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
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

/// Measures idle `Connector::pump()` overhead (plan §7.4 `stdb_pump`).
///
/// Uses an online-mode connector that never connects, so the measurement is
/// the pure per-frame pump path (state check + callback-queue drain), which is
/// also the offline single-player path.
pub fn bench_pump(ticks: u64) -> (u64, u64) {
    let config = ConnectionConfig {
        mode: StdbMode::Online,
        token_store_path: Some(std::env::temp_dir().join("mind-headless-stdb")),
        ..ConnectionConfig::local()
    };
    let mut connector = Connector::new(config);
    let mut samples = Vec::with_capacity(usize::try_from(ticks).unwrap_or(0));
    for _ in 0..ticks {
        let start = Instant::now();
        connector.pump();
        samples.push(start.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    (percentile(&samples, 50), percentile(&samples, 99))
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
