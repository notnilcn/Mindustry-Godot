// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless` command line (plan §3.5).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Headless test-rig for the `mind-core` simulation.
#[derive(Debug, Parser)]
#[command(
    name = "mind-headless",
    version = mind_core::MIND_VERSION,
    about = "Headless test-rig for the mind-core simulation (scenarios, dumps, benchmarks)"
)]
pub struct Cli {
    /// Scenarios directory override (defaults to the nearest `scenarios/` from cwd).
    #[arg(long, global = true)]
    pub scenarios_dir: Option<PathBuf>,

    /// Data directory override (logs; defaults to the platform data dir).
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,

    /// Log file path; defaults to `<data-dir>/last_log.txt`; pass `-` to disable.
    #[arg(long, global = true)]
    pub log_file: Option<String>,

    /// Requested command.
    #[command(subcommand)]
    pub command: Command,
}

/// `mind-headless` subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List registered scenarios.
    List,

    /// Run a registered scenario and verify its golden expectations.
    Run {
        /// Scenario name (see `list`).
        scenario: String,
        /// Write the final state dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the applied command log (JSONL) here.
        #[arg(long)]
        emit_commands: Option<PathBuf>,
    },

    /// Run a fresh flat world for N ticks.
    Sim {
        /// Number of ticks.
        ticks: u64,
        /// Simulation seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// World width in tiles.
        #[arg(long, default_value_t = 32)]
        width: i32,
        /// World height in tiles.
        #[arg(long, default_value_t = 32)]
        height: i32,
        /// Write the final state dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Replay a command log against a fresh flat world.
    Replay {
        /// Command log (`.jsonl`) to replay.
        commands: PathBuf,
        /// Simulation seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Number of ticks (defaults to the last command tick + 1).
        #[arg(long)]
        ticks: Option<u64>,
        /// World width in tiles.
        #[arg(long, default_value_t = 32)]
        width: i32,
        /// World height in tiles.
        #[arg(long, default_value_t = 32)]
        height: i32,
        /// Write the final state dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Include every per-tick checksum in the JSON report.
        #[arg(long)]
        per_tick: bool,
    },

    /// Benchmark `Sim::tick` on a scenario (emits `{p50_us,p99_us}` JSON).
    Bench {
        /// Number of timed ticks.
        #[arg(long, default_value_t = 100_000)]
        ticks: u64,
        /// Benchmark scenario/profile (`spine` maps to `bench_baseline`;
        /// `stdb_pump` measures connector pump overhead, plan 01 §7.4).
        #[arg(long, default_value = "spine")]
        scenario: String,
    },

    /// Run a scenario and write its state dump.
    Dump {
        /// Scenario name.
        #[arg(long)]
        scenario: String,
        /// Output path for the dump JSON.
        #[arg(long)]
        out: PathBuf,
        /// Emit every tile instead of the sparse (non-air) default.
        #[arg(long)]
        all_tiles: bool,
    },

    /// Content registry inspection (plan 02).
    Content {
        /// Content subcommand.
        #[command(subcommand)]
        command: ContentCommand,
    },

    /// Asset pipeline verification (plan 03 §7b).
    Assets {
        /// Assets subcommand.
        #[command(subcommand)]
        command: AssetsCommand,
    },

    /// IO engine inspection (plan 04).
    Io {
        /// IO subcommand.
        #[command(subcommand)]
        command: IoCommand,
    },

    /// Entity/component metadata inspection (plan 05 M5).
    Meta {
        /// Metadata subcommand.
        #[command(subcommand)]
        command: MetaCommand,
    },

    /// Schedule order inspection (plan 05 M6 §7.2).
    Trace {
        /// Trace subcommand.
        #[command(subcommand)]
        command: TraceCommand,
    },

    /// Mod discovery/content/patch/asset inspection (plan 20).
    Mods {
        /// Mods subcommand.
        #[command(subcommand)]
        command: ModsCommand,
    },
}

/// `mods` subcommands (plan 20 §7b).
#[derive(Debug, Subcommand)]
pub enum ModsCommand {
    /// Discover mods under a directory, resolve dependency states and print the
    /// report JSON. With `--check`, compare against
    /// `<dir>/expected_list.json` (normalized fields).
    List {
        /// Mod directory (defaults to `parity/mod_fixtures`).
        #[arg(long, default_value = "parity/mod_fixtures")]
        dir: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Compare the normalized mod list against `<dir>/expected_list.json`.
        #[arg(long)]
        check: bool,
    },

    /// Boot base content + one fixture mod's JSON content and dump the mod
    /// content records (plan 20 M1+).
    Content {
        /// Fixture name under `parity/mod_fixtures/`.
        #[arg(long)]
        fixture: String,
        /// Base content directory override (defaults to the repo root).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Write the content dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// `trace` subcommands (plan 05 M6).
#[derive(Debug, Subcommand)]
pub enum TraceCommand {
    /// Render the deterministic `TickSet`/`EntitySet` execution order for the
    /// menu/paused/playing/editor/client run conditions.
    Order {
        /// Ticks the trace represents (informational; order is per-tick).
        #[arg(long, default_value_t = 1)]
        ticks: u64,
        /// Write the golden text here (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// `meta` subcommands (plan 05 M5 §6.2).
#[derive(Debug, Subcommand)]
pub enum MetaCommand {
    /// Dump the entity/component metadata JSON (`entitymeta.json` shape).
    Entities {
        /// Write the stable JSON here (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit the JSON report on stdout even when `--out` is set.
        #[arg(long)]
        json: bool,
    },
}

/// `assets` subcommands (plan 03 §7.1b).
#[derive(Debug, Subcommand)]
pub enum AssetsCommand {
    /// Verify the vendored `assets/` + `assets-raw/` trees against
    /// `build/assets/migration_manifest.json` (M0).
    MigrateCheck {
        /// Repo root override (defaults to the nearest ancestor containing
        /// `assets-raw/sprites/pack.json`).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Manifest path override (defaults to
        /// `build/assets/migration_manifest.json` under the repo root).
        #[arg(long)]
        manifest: Option<PathBuf>,
    },

    /// Load a `sprites.atlas.json` manifest and dump the index summary as
    /// JSON (M1). With `--region`, probe specific regions; exits non-zero
    /// when any probe misses.
    Index {
        /// Directory containing `sprites.atlas.json` (e.g. `assets/sprites`).
        #[arg(long)]
        atlas: PathBuf,
        /// Region name to probe (repeatable).
        #[arg(long)]
        region: Vec<String>,
    },

    /// Assert the content-driven region inventory resolves in the packed
    /// atlas (M3/M4). `--assert-complete` fails on any missing name.
    Regions {
        /// Directory containing `sprites.atlas.json` (e.g. `assets/sprites`).
        #[arg(long, default_value = "assets/sprites")]
        atlas: PathBuf,
        /// Inventory produced by `mind-tools pack`
        /// (defaults to `build/assets/region_inventory.json`).
        #[arg(long)]
        inventory: Option<PathBuf>,
        /// Fail (non-zero exit) when any expected region is missing.
        #[arg(long)]
        assert_complete: bool,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Bundle key inventory (M7 §7.1b): no locale may contain keys absent from
    /// English; reports per-locale missing keys and confirms `global.properties`.
    BundleDiff {
        /// Directory containing `bundle.properties` / `bundle_<locale>.properties`.
        #[arg(long, default_value = "assets/bundles")]
        dir: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// `sounds.index.json` registry equals the recursive sound file listing, ids
    /// are dense/append-only and duplicates are rejected (M8/M10 §7.1b).
    SoundsCheck {
        /// Repo root override.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Fallback atlas boots and every region fits within 2048 (M4/M10 §7.1b).
    FallbackBoot {
        /// Directory containing the fallback `sprites.atlas.json`.
        #[arg(long, default_value = "assets/sprites/fallback")]
        atlas: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}

/// `io` subcommands (plan 04 §7b).
#[derive(Debug, Subcommand)]
pub enum IoCommand {
    /// Meta-only read of a save/map file (`SaveIO.getMeta`).
    DumpMeta {
        /// Save/map file path.
        file: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Settings persistence self-check (plan 04 M1 §7b): set → flush → reload
    /// equality, then a corrupted settings file falling back to defaults.
    /// Uses the global `--data-dir` as the settings root.
    Settings {
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Revision-manifest drift check for all entity defs (plan 04 M3 §7b):
    /// `EntityDefs!` codec fields vs committed `revisions/<NAME>/<N>.json`.
    CheckRevisions {
        /// Append the next `<N>.json` on drift instead of failing
        /// (old manifests are never touched).
        #[arg(long)]
        update: bool,
        /// `mind-core` crate directory (default: discovered upward from cwd).
        #[arg(long)]
        mind_core_dir: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Class-ID drift check (`entity_class_ids.toml` vs registry vs generated
    /// `class_ids.rs`; append-only discipline).
    CheckClassIds {
        /// Regenerate `class_ids.rs` and append missing defs at max+1.
        #[arg(long)]
        update: bool,
        /// `mind-core` crate directory (default: discovered upward from cwd).
        #[arg(long)]
        mind_core_dir: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Save round-trip on the synthetic world fixture (plan 04 M4 §7b):
    /// build → tick → checksum C0 → save → reset → load → assert C1 == C0.
    Roundtrip {
        /// Map to load. Only `synthetic` (the 64×64 fixture) exists until
        /// plan 06 lands real maps/generators.
        #[arg(long, default_value = "synthetic")]
        map: String,
        /// Synthetic world width.
        #[arg(long, default_value_t = 64)]
        width: u16,
        /// Synthetic world height.
        #[arg(long, default_value_t = 64)]
        height: u16,
        /// Ticks to simulate before saving.
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Save file path (`<out>-backup.msav` rotates on the next run).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Parallel meta-only listing of a map/save directory (plan 04 M5 §7b):
    /// corrupt entries are skipped with a warning.
    MapList {
        /// Directory to list (maps or saves).
        #[arg(long)]
        dir: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Save/load/meta timings on the synthetic fixture (plan 04 M8 §7b/§7d):
    /// records P50/P95 per phase. `serpulo/groundZero` waits for plan 06.
    BenchSave {
        /// Map profile. Only `synthetic` (64×64 fixture) exists until plan 06.
        #[arg(long, default_value = "synthetic")]
        map: String,
        /// Synthetic world width.
        #[arg(long, default_value_t = 64)]
        width: u16,
        /// Synthetic world height.
        #[arg(long, default_value_t = 64)]
        height: u16,
        /// Ticks simulated before saving (mid-game-ish fixture state).
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Timed iterations per phase.
        #[arg(long, default_value_t = 50)]
        iters: u64,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}

/// `content` subcommands (plan 02 §7b).
#[derive(Debug, Subcommand)]
pub enum ContentCommand {
    /// Boot the content registry and print per-type counts.
    Load {
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Dump the ordered per-type name/id list (`content_ids.json` shape).
    Ids {
        /// Emit the full JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the deterministic JSON dump here.
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Benchmark `createBaseContent` + `init` + `postInit`.
    Bench {
        /// Number of timed runs.
        #[arg(long, default_value_t = 20)]
        runs: usize,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Negative load-order scenario (liquids before statuses) — must fail.
    LoadOrderBad,

    /// Dump the parity golden snapshot (`parity/golden_content.json` shape).
    Dump {
        /// Write the deterministic golden JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Bundle keys file used to capture localized names.
        #[arg(long)]
        bundle: Option<PathBuf>,
        /// Emit the JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the mechanical parity audit (plan 02 §7b).
    Audit {
        /// Golden file (default `parity/golden_content.json`).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Bundle keys file (default `parity/bundle_keys.json` when present).
        #[arg(long)]
        bundle: Option<PathBuf>,
        /// Asset manifest (default `parity/asset_manifest.json` when present).
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Write the markdown audit report here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit the JSON audit report on stdout.
        #[arg(long)]
        json: bool,
    },
}
