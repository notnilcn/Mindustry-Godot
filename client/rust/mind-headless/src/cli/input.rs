// SPDX-License-Identifier: GPL-3.0-only

//! `input` subcommands (plan 15 §7b).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `input` subcommands (plan 15 §7b).
#[derive(Debug, Subcommand)]
pub enum InputCommand {
    /// Dump the full binding registry parity table (`Binding.java`).
    Dump {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the registry JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Compare the registry JSON against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Replay a `format:1` input-event log (`.events.jsonl`).
    Replay {
        /// Input-event log path.
        events: PathBuf,
        /// Write the replay summary JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Force the mobile handler (overrides the header flag).
        #[arg(long)]
        mobile: bool,
    },

    /// Run a named input scenario.
    Scenario {
        /// Scenario name (`input_focus_guards`, ...).
        name: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the scenario report JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the scenario report against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Benchmark an input/placement path against the §7d budget.
    Bench {
        /// Profile: `input_line`, `input_astar`, `input_bridges`,
        /// `input_select`, `input_command`, `input_idle`.
        #[arg(long, default_value = "input_line")]
        profile: String,
        /// Line length in tiles (`input_line`).
        #[arg(long, default_value_t = 100)]
        len: i32,
        /// Unit count (`input_select`/`input_command`).
        #[arg(long, default_value_t = 200)]
        units: usize,
        /// Timed iterations.
        #[arg(long, default_value_t = 2000)]
        iters: u64,
        /// Fail when steady-state allocations exceed this count
        /// (`alloc-audit` feature builds only).
        #[arg(long)]
        assert_alloc: Option<u64>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
