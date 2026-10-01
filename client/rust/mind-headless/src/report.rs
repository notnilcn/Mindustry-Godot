// SPDX-License-Identifier: GPL-3.0-only

//! Machine-readable CLI reports.

use serde::Serialize;

/// `run --json` report.
#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    /// Scenario name.
    pub scenario: String,
    /// Scenario format.
    pub format: u32,
    /// Seed.
    pub seed: u64,
    /// Configured steps.
    pub steps: u64,
    /// Completed ticks.
    pub tick: u64,
    /// Final checksum.
    pub checksum: String,
    /// Golden checksum, when the scenario has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_checksum: Option<String>,
    /// Overall pass/fail.
    pub pass: bool,
    /// Whether a second in-process run produced identical per-tick checksums.
    pub in_process_stable: bool,
    /// Number of commands applied.
    pub commands_applied: u64,
    /// Sparse tile assertion results.
    pub tiles: Vec<TileCheck>,
    /// Per-tick checksums (only for scenarios with `emit_per_tick`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_tick: Option<Vec<String>>,
    /// Path of the emitted command log, when requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands_emitted: Option<String>,
}

/// One tile assertion result.
#[derive(Debug, Clone, Serialize)]
pub struct TileCheck {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Expected block name.
    pub expect: String,
    /// Actual block name.
    pub actual: String,
    /// Whether they matched.
    pub ok: bool,
}

/// `sim`/`replay --json` report.
#[derive(Debug, Clone, Serialize)]
pub struct SimReport {
    /// Origin (`sim` or `replay`).
    pub mode: String,
    /// Seed.
    pub seed: u64,
    /// World width.
    pub width: i32,
    /// World height.
    pub height: i32,
    /// Completed ticks.
    pub tick: u64,
    /// Final checksum.
    pub checksum: String,
    /// Commands applied.
    pub commands_applied: u64,
    /// Per-tick checksums, when requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_tick: Option<Vec<String>>,
}

/// `stdb_*` scenario report (plan 01 §7.2).
#[derive(Debug, Clone, Serialize)]
pub struct StdbReport {
    /// Scenario name.
    pub scenario: String,
    /// Connection mode (`"offline"`).
    pub mode: String,
    /// Final connector state name.
    pub state: String,
    /// `pump()` calls processed.
    pub frames: u64,
    /// Requested pump count.
    pub pumps: u64,
    /// Median pump overhead in nanoseconds.
    pub pump_p50_ns: u64,
    /// 99th percentile pump overhead in nanoseconds.
    pub pump_p99_ns: u64,
    /// Overall pass/fail.
    pub pass: bool,
    /// Golden state, when the fixture has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_state: Option<String>,
    /// Golden frame count, when the fixture has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_frames: Option<u64>,
}

/// `stdb_binder_replay` report (plan 01 §7.2).
#[derive(Debug, Clone, Serialize)]
pub struct StdbBinderReport {
    /// Scenario name.
    pub scenario: String,
    /// Overall pass/fail.
    pub pass: bool,
    /// Replayed row versions in drain order.
    pub replay_order: Vec<u32>,
    /// Live-injected row versions in drain order.
    pub live_order: Vec<u32>,
    /// Golden replay order, when the fixture has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_replay: Option<Vec<u32>>,
    /// Golden live order, when the fixture has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_live: Option<Vec<u32>>,
}

/// `bench` report (also committed into `bench_baseline.json`).
#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    /// Scenario name.
    pub scenario: String,
    /// Timed ticks.
    pub ticks: u64,
    /// Median nanoseconds per tick.
    pub p50_ns: u64,
    /// 99th percentile nanoseconds per tick.
    pub p99_ns: u64,
    /// Median microseconds per tick.
    pub p50_us: u64,
    /// 99th percentile microseconds per tick.
    pub p99_us: u64,
    /// Final checksum (sanity).
    pub checksum: String,
    /// Whether the baseline was exceeded by < 20% (warning) or more (failure).
    pub baseline_status: String,
}
