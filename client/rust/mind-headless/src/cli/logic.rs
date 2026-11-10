// SPDX-License-Identifier: GPL-3.0-only

//! `logic` subcommands (plan 11).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `logic` subcommands (plan 13 §7b).
#[derive(Debug, Subcommand)]
pub enum LogicCommand {
    /// Parse a program, write it back, and (optionally) save the normalized text.
    Assemble {
        /// Input mlog file.
        file: PathBuf,
        /// Write the normalized program here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Treat the program as privileged (world processor).
        #[arg(long)]
        privileged: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run a registered mlog scenario and print its checksum/variable report.
    Run {
        /// Scenario name (`logic_arith`, `logic_strings`, `logic_budget`, `logic_globals`).
        name: String,
        /// Override scenario ticks.
        #[arg(long)]
        ticks: Option<u64>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Parse a program and dump its statement field order (plan-14 metadata probe).
    Dump {
        /// Input mlog file.
        file: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
