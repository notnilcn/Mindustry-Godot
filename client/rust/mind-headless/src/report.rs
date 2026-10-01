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
