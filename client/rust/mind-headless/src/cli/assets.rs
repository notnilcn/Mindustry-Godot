// SPDX-License-Identifier: GPL-3.0-only

//! `assets` subcommands (plan 03).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `assets` subcommands (plan 03 §7.1b).
#[derive(Debug, Subcommand)]
pub enum AssetsCommand {
    /// Verify the vendored `assets/` + `assets-raw/` trees against
    /// `build/assets/migration_manifest.json` (M0).
    MigrateCheck {
        /// Repo root override (defaults to the nearest ancestor containing
        /// `assets-raw/sprites/pack.json`).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Manifest path override (defaults to
        /// `build/assets/migration_manifest.json` under the repo root).
        #[arg(long)]
        manifest: Option<PathBuf>,
    },

    /// Load a `sprites.atlas.json` manifest and dump the index summary as
    /// JSON (M1). With `--region`, probe specific regions; exits non-zero
    /// when any probe misses.
    Index {
        /// Directory containing `sprites.atlas.json` (e.g. `assets/sprites`).
        #[arg(long)]
        atlas: PathBuf,
        /// Region name to probe (repeatable).
        #[arg(long)]
        region: Vec<String>,
    },

    /// Assert the content-driven region inventory resolves in the packed
    /// atlas (M3/M4). `--assert-complete` fails on any missing name.
    Regions {
        /// Directory containing `sprites.atlas.json` (e.g. `assets/sprites`).
        #[arg(long, default_value = "assets/sprites")]
        atlas: PathBuf,
        /// Inventory produced by `mind-tools pack`
        /// (defaults to `build/assets/region_inventory.json`).
        #[arg(long)]
        inventory: Option<PathBuf>,
        /// Fail (non-zero exit) when any expected region is missing.
        #[arg(long)]
        assert_complete: bool,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Bundle key inventory (M7 §7.1b): no locale may contain keys absent from
    /// English; reports per-locale missing keys and confirms `global.properties`.
    BundleDiff {
        /// Directory containing `bundle.properties` / `bundle_<locale>.properties`.
        #[arg(long, default_value = "assets/bundles")]
        dir: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// `sounds.index.json` registry equals the recursive sound file listing, ids
    /// are dense/append-only and duplicates are rejected (M8/M10 §7.1b).
    SoundsCheck {
        /// Repo root override.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Fallback atlas boots and every region fits within 2048 (M4/M10 §7.1b).
    FallbackBoot {
        /// Directory containing the fallback `sprites.atlas.json`.
        #[arg(long, default_value = "assets/sprites/fallback")]
        atlas: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}
