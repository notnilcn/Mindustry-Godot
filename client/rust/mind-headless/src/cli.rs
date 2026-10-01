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

    /// IO engine inspection (plan 04).
    Io {
        /// IO subcommand.
        #[command(subcommand)]
        command: IoCommand,
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
}
