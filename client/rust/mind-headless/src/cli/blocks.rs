// SPDX-License-Identifier: GPL-3.0-only

//! `blocks` subcommands (plan 07).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use clap::Subcommand;

/// `blocks` subcommands (plan 07 §7b).
#[derive(Debug, Subcommand)]
pub enum BlocksCommand {
    /// Dump per-block `kind`/`building`/`family`/`consumers` for every block.
    Audit {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Run a deterministic block/building scenario.
    Scenario {
        /// Scenario name: `place_construct_destroy`, `multiblock_cover_clear`,
        /// `spawn_update`, `config_roundtrip`, `proximity_multiblock`.
        name: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Time the building update loop over N buildings.
    Bench {
        /// Benchmark profile: `buildings` (idle), `active` (producing
        /// crafters), `place` (`valid_place`+place), `construct` (progress),
        /// `logistics` (plan 08: loaded conveyor/router lanes).
        #[arg(long, default_value = "buildings")]
        profile: String,
        /// Number of buildings to place.
        #[arg(long, default_value_t = 2000)]
        buildings: usize,
        /// Timed ticks.
        #[arg(long, default_value_t = 3600)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
