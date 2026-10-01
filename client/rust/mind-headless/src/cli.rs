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
        /// Benchmark scenario/profile (`spine` maps to `bench_baseline`).
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
}
