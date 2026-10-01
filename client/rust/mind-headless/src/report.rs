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

/// `stdb_command_order` report (plan 01 §7.2).
#[derive(Debug, Clone, Serialize)]
pub struct StdbOrderReport {
    /// Scenario name.
    pub scenario: String,
    /// Overall pass/fail.
    pub pass: bool,
    /// Applied command IDs in order.
    pub applied_order: Vec<u64>,
    /// Whether the injected duplicate was ignored.
    pub duplicate_ignored: bool,
    /// First order error observed, if any (`"gap: expected 2, got 3"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_error: Option<String>,
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

/// `io dump-meta` report (plan 04 M0): meta-only read of one save/map file.
#[derive(Debug, Clone, Serialize)]
pub struct IoDumpMetaReport {
    /// File that was read.
    pub file: String,
    /// Save format version from the container header.
    pub format_version: i32,
    /// Build tag.
    pub build: i32,
    /// Save timestamp (millis).
    pub timestamp: i64,
    /// Accumulated playtime (millis).
    pub time_played: i64,
    /// `mapname` tag.
    pub map_name: String,
    /// `wave` tag.
    pub wave: i32,
    /// `width` tag.
    pub width: i32,
    /// `height` tag.
    pub height: i32,
    /// `SaveMeta.isMap` (tags contain `name`).
    pub is_map: bool,
    /// Parsed `mods` list.
    pub mods: Vec<String>,
    /// All meta tags in file order.
    pub tags: Vec<(String, String)>,
}

/// `io settings` report (plan 04 M1 §7b).
#[derive(Debug, Clone, Serialize)]
pub struct IoSettingsReport {
    /// Data root the scenario ran against.
    pub data_dir: String,
    /// Set → flush → reload equality held for every key.
    pub persisted: bool,
    /// Corrupted settings file fell back to defaults without a panic.
    pub corrupt_fallback: bool,
    /// The store rewrote a valid file after the corruption.
    pub recovered: bool,
    /// Keys exercised in the persistence phase.
    pub keys_checked: Vec<String>,
    /// Overall pass/fail.
    pub pass: bool,
}
