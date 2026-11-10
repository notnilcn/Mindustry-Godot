// SPDX-License-Identifier: GPL-3.0-only

//! `power`/`liquid`/`heat` subcommands (plan 09 §7b).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `power`/`liquid`/`heat` subcommands (plan 09 §7b).
#[derive(Debug, Subcommand)]
pub enum NetworkCommand {
    /// List registered scenarios for this network.
    List,

    /// Run a network scenario; optionally write/verify its `NetworkState` JSON.
    Scenario {
        /// Scenario name.
        name: String,
        /// Write the canonical `NetworkState` dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the canonical dump against this golden file.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark the network update loop against the §7d budget.
    Bench {
        /// Number of network buildings to place.
        #[arg(long, default_value_t = 2000)]
        buildings: usize,
        /// Timed ticks.
        #[arg(long, default_value_t = 3600)]
        ticks: u64,
        /// Untimed warmup ticks.
        #[arg(long, default_value_t = 600)]
        warmup: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
