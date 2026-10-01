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

/// `content load` report (per-type counts, plan 02 §7b).
#[derive(Debug, Clone, Serialize)]
pub struct ContentLoadReport {
    /// Per-type counts in `ContentType.all` order (live types only).
    pub types: Vec<ContentTypeCount>,
    /// Sum of all reported counts.
    pub total: usize,
}

/// One per-type count entry.
#[derive(Debug, Clone, Serialize)]
pub struct ContentTypeCount {
    /// Content type (serialized as its Java enum identifier).
    #[serde(rename = "type")]
    pub type_: mind_core::content::ContentType,
    /// Number of registered records.
    pub count: usize,
}

/// `content ids` report: the `content_ids.json` `types` block (plan 02 §6.2).
#[derive(Debug, Clone, Serialize)]
pub struct ContentIdsReport {
    /// Dump format (1).
    pub format: u32,
    /// Per-type ordered entries in `ContentType.all` order.
    pub types: Vec<ContentTypeEntries>,
}

/// Ordered entries for one content type.
#[derive(Debug, Clone, Serialize)]
pub struct ContentTypeEntries {
    /// Content type.
    #[serde(rename = "type")]
    pub type_: mind_core::content::ContentType,
    /// Entries in dense id order.
    pub entries: Vec<ContentIdEntry>,
}

/// One content id/name/kind entry.
#[derive(Debug, Clone, Serialize)]
pub struct ContentIdEntry {
    /// Dense id.
    pub id: u16,
    /// Mappable name (omitted for non-mappable kinds such as bullets).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Java class-ish kind tag.
    pub kind: String,
}

/// `content bench` report (plan 02 §7d).
#[derive(Debug, Clone, Serialize)]
pub struct ContentBenchReport {
    /// Timed runs.
    pub runs: usize,
    /// Median wall time in milliseconds.
    pub median_ms: f64,
    /// Minimum wall time in milliseconds.
    pub min_ms: f64,
    /// Maximum wall time in milliseconds.
    pub max_ms: f64,
    /// Whether the median is within the 200 ms budget.
    pub within_budget: bool,
    /// Final per-type counts (sanity).
    pub types: Vec<ContentTypeCount>,
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
