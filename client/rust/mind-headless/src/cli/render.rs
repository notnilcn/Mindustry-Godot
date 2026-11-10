// SPDX-License-Identifier: GPL-3.0-only

//! `render` subcommands (plan 16 §7.2).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `render` subcommands (plan 16 §7.2).
#[derive(Debug, Subcommand)]
pub enum RenderCommand {
    /// Extract the deterministic render list for a registered `render_*`
    /// scenario and write/check its golden JSON.
    List {
        /// Scenario name (`render_flat_floor`, `render_block_change`,
        /// `render_layer_order`).
        scenario: String,
        /// Write the render-list JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Compare the produced JSON against this golden file.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit the raw emission order (no `(z, seq)` sort).
        #[arg(long)]
        no_sort: bool,
        /// Emit every chunk instead of only the camera's dirty set.
        #[arg(long)]
        all_chunks: bool,
        /// Emit a machine-readable JSON summary on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Write the append-only band table audit (`build/render/bands.json`).
    Bands {
        /// Output path (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Benchmark the render-list build path (plan 16 §7.4/M9).
    Bench {
        /// Grid width in tiles.
        #[arg(long, default_value_t = 250)]
        width: i32,
        /// Grid height in tiles.
        #[arg(long, default_value_t = 250)]
        height: i32,
        /// Buildings to place.
        #[arg(long, default_value_t = 5000)]
        buildings: usize,
        /// Measured iterations.
        #[arg(long, default_value_t = 200)]
        iters: usize,
        /// Warmup iterations.
        #[arg(long, default_value_t = 20)]
        warmup: usize,
        /// Emit the machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}
