// SPDX-License-Identifier: GPL-3.0-only

//! `world` subcommands (plan 06).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `world` subcommands (plan 06 §7b).
#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// Deterministic interleaved floor/overlay/block/air operations on a fresh
    /// grid; asserts counters equal event counts (plan 06 M0 §7b).
    TileOps {
        /// Simulation seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Grid width in tiles.
        #[arg(long, default_value_t = 64)]
        width: i32,
        /// Grid height in tiles.
        #[arg(long, default_value_t = 64)]
        height: i32,
        /// Number of operations.
        #[arg(long, default_value_t = 10_000)]
        ops: u64,
        /// Write the final JSON report here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Generate a world and dump its checksum/histogram (plan 06 M6/M7 §7b).
    Gen {
        /// Generator tag (`simplex`, `flat`, `planet`).
        #[arg(long, default_value = "simplex")]
        generator: String,
        /// Planet content name; selects the vanilla planet generator when set
        /// (`serpulo`, `erekir`, `tantros`, `asteroid`, `blank`).
        #[arg(long)]
        planet: Option<String>,
        /// Sector id (for `--generator planet`).
        #[arg(long, default_value_t = 0)]
        sector: u32,
        /// Generation seed.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Grid width in tiles.
        #[arg(long, default_value_t = 128)]
        width: i32,
        /// Grid height in tiles.
        #[arg(long, default_value_t = 128)]
        height: i32,
        /// Number of generations to run (goldens use 1; determinism runs >1).
        #[arg(long, default_value_t = 1)]
        iters: u64,
        /// Write the final JSON report here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Time world generation (plan 06 §7d).
    BenchGen {
        /// Generator tag (`simplex`, `tantros`, `blank`, `serpulo`, `erekir`,
        /// `asteroid`).
        #[arg(long, default_value = "simplex")]
        generator: String,
        /// Generation seed.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Grid width in tiles.
        #[arg(long, default_value_t = 256)]
        width: i32,
        /// Grid height in tiles.
        #[arg(long, default_value_t = 256)]
        height: i32,
        /// Timed iterations.
        #[arg(long, default_value_t = 20)]
        iters: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Apply a generation-filter stack to a deterministic base grid and dump
    /// the result checksum/histogram (plan 06 M5 §7b).
    Filters {
        /// Simulation seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Grid width in tiles.
        #[arg(long, default_value_t = 64)]
        width: i32,
        /// Grid height in tiles.
        #[arg(long, default_value_t = 64)]
        height: i32,
        /// Comma-separated filter class tags (default `scatter,ore,median,blend`).
        #[arg(long, default_value = "scatter,ore,median,blend")]
        stack: String,
        /// Apply order (`forward` or `reverse`).
        #[arg(long, default_value = "forward")]
        order: String,
        /// Write the final JSON report here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Place overlapping multiblocks and break the center; asserts shared-entity
    /// linkage and overlap clearing (plan 06 M3 §7b).
    Multiblock {
        /// Multiblock size in tiles.
        #[arg(long, default_value_t = 3)]
        size: i32,
        /// Block name to place (must have the requested size).
        #[arg(long, default_value = "core-shard")]
        block: String,
        /// Write the final JSON report here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
