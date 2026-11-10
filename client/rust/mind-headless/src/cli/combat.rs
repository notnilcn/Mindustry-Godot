// SPDX-License-Identifier: GPL-3.0-only

//! `combat` subcommands (plan 10).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use clap::Subcommand;

/// `combat` subcommands (plan 10 §7b).
#[derive(Debug, Subcommand)]
pub enum CombatCommand {
    /// Run a deterministic combat scenario and print its report/checksum.
    Scenario {
        /// Scenario name (`combat_basic`, `combat_bullet_pierce`,
        /// `combat_determinism`).
        name: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Dump live bullets for the state inspector.
    Dump {
        /// Only list live bullets.
        #[arg(long)]
        bullets_live: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Trace one bullet kind's position over N ticks.
    Trace {
        /// Fixture bullet kind/name (`fuse`, `rail`, `laser`, ...).
        #[arg(long)]
        kind: String,
        /// Ticks to trace.
        #[arg(long, default_value_t = 40)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark bullet update/collision.
    Bench {
        /// Live traveling bullets.
        #[arg(long, default_value_t = 2000)]
        bullets: usize,
        /// Firing `test-item` turrets included in the tick.
        #[arg(long, default_value_t = 0)]
        turrets: usize,
        /// Timed ticks.
        #[arg(long, default_value_t = 3600)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
