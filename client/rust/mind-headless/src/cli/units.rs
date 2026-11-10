// SPDX-License-Identifier: GPL-3.0-only

//! `units` subcommands (plan 08).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use clap::Subcommand;

/// `units` subcommands (plan 11 §7b).
#[derive(Debug, Subcommand)]
pub enum UnitsCommand {
    /// Run a deterministic unit/AI scenario and print its report/checksum.
    Scenario {
        /// Scenario name (`units_spawn_path_arrive`).
        name: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Spawn a unit and dump its state.
    Spawn {
        /// Unit content name (`dagger`, `mace`, `risso`, ...).
        #[arg(long, default_value = "dagger")]
        unit: String,
        /// Team id.
        #[arg(long, default_value_t = 0)]
        team: u8,
        /// World-pixel x.
        #[arg(long, default_value_t = 64.0)]
        x: f32,
        /// World-pixel y.
        #[arg(long, default_value_t = 64.0)]
        y: f32,
        /// Ticks to run after spawning.
        #[arg(long, default_value_t = 0)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Trace a unit pathing to a destination tile.
    Path {
        /// Unit content name.
        #[arg(long, default_value = "dagger")]
        unit: String,
        /// Destination tile x.
        #[arg(long, default_value_t = 40)]
        tx: i32,
        /// Destination tile y.
        #[arg(long, default_value_t = 40)]
        ty: i32,
        /// Ticks to run.
        #[arg(long, default_value_t = 1500)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark the unit AI + movement tick.
    Bench {
        /// Live units.
        #[arg(long, default_value_t = 300)]
        units: usize,
        /// Timed ticks.
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Fail when steady-state tick allocations exceed this count
        /// (`alloc-audit` feature builds only).
        #[arg(long)]
        assert_alloc: Option<u64>,
        /// Benchmark profile: `ai` (unit AI+movement tick, default) or `path`
        /// (`ControlPathfinder` field build + request, §7d `pathfinder_flat256`).
        #[arg(long)]
        profile: Option<String>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
