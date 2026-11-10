// SPDX-License-Identifier: GPL-3.0-only

//! `content` subcommands (plan 02).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `content` subcommands (plan 02 §7b).
#[derive(Debug, Subcommand)]
pub enum ContentCommand {
    /// Boot the content registry and print per-type counts.
    Load {
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Dump the ordered per-type name/id list (`content_ids.json` shape).
    Ids {
        /// Emit the full JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the deterministic JSON dump here.
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Generate the server module's `content_seed.rs` from the vanilla manifest
    /// (plan 21 §3.12.4); regenerate via `server/gen_content_seed.sh`.
    Seed {
        /// Output path (`server/spacetimedb/src/main/content_seed.rs`).
        #[arg(long)]
        out: PathBuf,
    },

    /// Benchmark `createBaseContent` + `init` + `postInit`.
    Bench {
        /// Number of timed runs.
        #[arg(long, default_value_t = 20)]
        runs: usize,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Negative load-order scenario (liquids before statuses) — must fail.
    LoadOrderBad,

    /// Dump the parity golden snapshot (`parity/golden_content.json` shape).
    Dump {
        /// Write the deterministic golden JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Bundle keys file used to capture localized names.
        #[arg(long)]
        bundle: Option<PathBuf>,
        /// Emit the JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the mechanical parity audit (plan 02 §7b).
    Audit {
        /// Golden file (default `parity/golden_content.json`).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Bundle keys file (default `parity/bundle_keys.json` when present).
        #[arg(long)]
        bundle: Option<PathBuf>,
        /// Asset manifest (default `parity/asset_manifest.json` when present).
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Write the markdown audit report here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit the JSON audit report on stdout.
        #[arg(long)]
        json: bool,
    },
}
