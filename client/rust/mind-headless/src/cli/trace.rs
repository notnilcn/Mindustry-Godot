// SPDX-License-Identifier: GPL-3.0-only

//! `trace` subcommands (plan 05).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

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
