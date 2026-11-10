// SPDX-License-Identifier: GPL-3.0-only

//! `fx` subcommands (plan 13).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `fx` subcommands (plan 17 §7b).
#[derive(Debug, Subcommand)]
pub enum FxCommand {
    /// Audit the catalogue: length/order/name drift, classification counts and
    /// (with `--strict`) unported entries.
    Audit {
        /// Restrict to one wave (`F1`..`F6`).
        #[arg(long)]
        wave: Option<String>,
        /// Fail when any entry is unported.
        #[arg(long)]
        strict: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the scripted effect lifecycle trace and write/check its golden JSON.
    Lifecycle {
        /// Deterministic view seed.
        #[arg(long, default_value_t = 5)]
        seed: u64,
        /// Ticks to run.
        #[arg(long, default_value_t = 600)]
        ticks: u32,
        /// Write the trace JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the trace JSON against this golden.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Build the sorted `DrawPrim` program for one catalogue entry at a fixed
    /// time and write/check its golden JSON.
    Program {
        /// Effect name (`Fx.<name>`).
        name: String,
        /// Fixed time in ticks.
        #[arg(long, default_value_t = 3.0)]
        tick: f32,
        /// Write the program JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Compare the program JSON against this golden.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Deterministic trail update/detach/fade trace.
    Trail {
        /// Deterministic seed.
        #[arg(long, default_value_t = 3)]
        seed: u64,
        /// Write the trace JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the trace JSON against this golden.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark program resolution for N live states over M frames.
    Bench {
        /// Live states.
        #[arg(long, default_value_t = 2000)]
        states: usize,
        /// Frames to resolve.
        #[arg(long, default_value_t = 360)]
        frames: u32,
        /// Emit the machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Vertical-slice check: spawn one effect, build its program, assert pixels
    /// are emitted.
    Smoke {
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Effects-excluded proof (plan 17 §7b #3): run the same combat scenario
    /// with a no-op sink and with a recording `FxBus`, asserting identical sim
    /// checksums while effects were emitted.
    NoopHeadless {
        /// Deterministic sim seed.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Ticks to run.
        #[arg(long, default_value_t = 120)]
        ticks: u32,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
