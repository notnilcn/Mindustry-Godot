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
    /// Path of the emitted binary `.simlog`, when requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simlog_emitted: Option<String>,
    /// Phase name after the run (plan 05 M9 inspector surface).
    pub state: String,
    /// Monotonic update counter.
    pub update_id: u64,
    /// Number of `// plan NN` stub systems that ran this tick.
    pub unimplemented_stub: u32,
    /// Live per-group entity counts.
    pub group_counts: Vec<GroupCount>,
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

/// One group count row (plan 05 M9 inspector surface).
#[derive(Debug, Clone, Serialize)]
pub struct GroupCount {
    /// Group name.
    pub name: String,
    /// Live entity count.
    pub count: usize,
}

/// Binary/text `.simlog` replay report (plan 05 M8 §7.2).
#[derive(Debug, Clone, Serialize)]
pub struct SimCoreReplayReport {
    /// Command log that was replayed.
    pub file: String,
    /// `binary` or `text`.
    pub format: String,
    /// Seed used.
    pub seed: u64,
    /// World width.
    pub width: i32,
    /// World height.
    pub height: i32,
    /// Completed ticks.
    pub tick: u64,
    /// Worker count requested (determinism harness; identical output expected).
    pub workers: usize,
    /// Final checksum.
    pub checksum: String,
    /// Per-command errors that the current build does not represent (never fatal).
    pub unsupported_commands: usize,
    /// Sampled checksums (`--checksum-every`), if requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksums: Option<Vec<String>>,
    /// Golden checksums read from `--golden`, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_checksums: Option<Vec<String>>,
    /// Overall pass/fail (golden comparison; no golden = pass).
    pub pass: bool,
}

/// One per-profile budget row for `bench sim_core`.
#[derive(Debug, Clone, Serialize)]
pub struct SimCoreProfileReport {
    /// Profile name (`empty`/`mid`/`stress`).
    pub profile: String,
    /// World width.
    pub width: i32,
    /// World height.
    pub height: i32,
    /// Buildings placed before timing.
    pub buildings: usize,
    /// Timed ticks.
    pub ticks: u64,
    /// Median ns/tick.
    pub p50_ns: u64,
    /// 95th percentile ns/tick.
    pub p95_ns: u64,
    /// 99th percentile ns/tick.
    pub p99_ns: u64,
    /// Final checksum (sanity).
    pub checksum: String,
    /// p99 budget in microseconds (plan 05 §7.4).
    pub budget_us: u64,
    /// Whether p99 is within budget.
    pub within_budget: bool,
    /// Allocations observed across the timed region.
    pub allocs: u64,
    /// Whether the alloc counters were compiled in.
    pub alloc_audit: bool,
    /// `--assert-alloc` limit, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assert_alloc: Option<u64>,
    /// Overall pass/fail (alloc assertion only; budgets are recording-only).
    pub pass: bool,
}

/// `sim_core_reset_play_cycle` report (plan 05 §7.2).
#[derive(Debug, Clone, Serialize)]
pub struct SimCoreCycleReport {
    /// Cycles run.
    pub cycles: u64,
    /// Ticks simulated after each `play`.
    pub ticks: u64,
    /// Every cycle returned to `menu` with empty groups/grid and zero clock.
    pub reset_clean: bool,
    /// Every `play` produced `playing` and advanced the clock.
    pub play_advances: bool,
    /// Entity count baseline (0) held across resets.
    pub entity_baseline: usize,
    /// Final checksum (determinism sanity).
    pub checksum: String,
    /// Overall pass/fail.
    pub pass: bool,
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

/// One def row in the `io check-revisions` report (plan 04 M3).
#[derive(Debug, Clone, Serialize)]
pub struct IoDefRevisionReport {
    /// Def name.
    pub name: String,
    /// `up-to-date` / `missing` / `drift` / `written` / `updated`.
    pub status: String,
    /// Version appended by `--update`, when applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_to: Option<u32>,
    /// Drift details (empty when clean).
    pub details: Vec<String>,
}

/// `io check-revisions` report (plan 04 M3 §7b).
#[derive(Debug, Clone, Serialize)]
pub struct IoCheckRevisionsReport {
    /// Manifest root that was checked.
    pub revisions_root: String,
    /// Whether `--update` ran.
    pub update: bool,
    /// Overall pass/fail (check mode).
    pub pass: bool,
    /// Per-def rows.
    pub defs: Vec<IoDefRevisionReport>,
}

/// `io check-class-ids` report (plan 04 M3).
#[derive(Debug, Clone, Serialize)]
pub struct IoCheckClassIdsReport {
    /// TOML file checked.
    pub toml: String,
    /// Generated constants file checked.
    pub generated: String,
    /// Number of ID entries.
    pub entries: usize,
    /// Whether `--update` ran.
    pub update: bool,
    /// Whether files were rewritten.
    pub updated: bool,
    /// Problems found (empty when clean).
    pub problems: Vec<String>,
    /// Overall pass/fail.
    pub pass: bool,
}

/// `io roundtrip` report (plan 04 M4 §7b).
#[derive(Debug, Clone, Serialize)]
pub struct IoRoundtripReport {
    /// Map that was round-tripped.
    pub map: String,
    /// World width.
    pub width: u16,
    /// World height.
    pub height: u16,
    /// Ticks simulated before saving.
    pub ticks: u64,
    /// Save file written.
    pub out: String,
    /// Save file size in bytes.
    pub bytes: u64,
    /// Buildings read back from the map region.
    pub buildings: usize,
    /// Fixture checksum before save.
    pub checksum_before: String,
    /// Fixture checksum after load.
    pub checksum_after: String,
    /// `checksum_before == checksum_after`.
    pub pass: bool,
}

/// One entry in the `io map-list` report (plan 04 M5 §7b).
#[derive(Debug, Clone, Serialize)]
pub struct IoMapListEntry {
    /// File path.
    pub file: String,
    /// Display name (map `name` tag or save `mapname`).
    pub name: String,
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// Wave.
    pub wave: i32,
    /// Build that wrote the file.
    pub build: i32,
    /// Save format version.
    pub format_version: i32,
    /// Whether the file is a map (`SaveMeta.isMap`).
    pub is_map: bool,
    /// Mod count.
    pub mods: usize,
}

/// `io map-list` report (plan 04 M5 §7b).
#[derive(Debug, Clone, Serialize)]
pub struct IoMapListReport {
    /// Directory listed.
    pub dir: String,
    /// Entries with readable meta.
    pub listed: usize,
    /// Entries skipped as corrupt.
    pub skipped: usize,
    /// The listing (sorted by file name).
    pub entries: Vec<IoMapListEntry>,
}

/// One phase timing distribution in milliseconds (plan 04 M8 §7d).
#[derive(Debug, Clone, Serialize)]
pub struct IoBenchStat {
    /// 50th percentile.
    pub p50_ms: f64,
    /// 95th percentile.
    pub p95_ms: f64,
    /// Fastest sample.
    pub min_ms: f64,
    /// Slowest sample.
    pub max_ms: f64,
}

/// `io bench-save` report (plan 04 M8 §7b/§7d).
#[derive(Debug, Clone, Serialize)]
pub struct IoBenchSaveReport {
    /// Map profile (only `synthetic` until plan 06).
    pub map: String,
    /// World width.
    pub width: u16,
    /// World height.
    pub height: u16,
    /// Ticks simulated before saving.
    pub ticks: u64,
    /// Timed iterations per phase.
    pub iters: u64,
    /// Save file size in bytes.
    pub bytes: u64,
    /// Save (serialize + deflate + file write).
    pub save: IoBenchStat,
    /// Load (read + inflate + apply regions).
    pub load: IoBenchStat,
    /// Meta-only read.
    pub meta: IoBenchStat,
    /// Load verification (checksum equality on the last timed load).
    pub pass: bool,
    /// Budget note (the §7d groundZero baseline needs plan 06 real maps).
    pub note: String,
}
