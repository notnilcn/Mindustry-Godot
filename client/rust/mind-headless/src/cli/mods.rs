// SPDX-License-Identifier: GPL-3.0-only

//! `mods` subcommands (plan 20).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `mods` subcommands (plan 20 §7b).
#[derive(Debug, Subcommand)]
pub enum ModsCommand {
    /// Discover mods under a directory, resolve dependency states and print the
    /// report JSON. With `--check`, compare against
    /// `<dir>/expected_list.json` (normalized fields).
    List {
        /// Mod directory (defaults to `parity/mod_fixtures`).
        #[arg(long, default_value = "parity/mod_fixtures")]
        dir: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Compare the normalized mod list against `<dir>/expected_list.json`.
        #[arg(long)]
        check: bool,
    },

    /// Boot base content + one fixture mod's JSON content and dump the mod
    /// content records (plan 20 M1+).
    Content {
        /// Fixture name under `parity/mod_fixtures/`.
        #[arg(long)]
        fixture: String,
        /// Base content directory override (defaults to the repo root).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Write the content dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Build a fixture mod's sprite/bundle overlay and probe region names
    /// (plan 20 M5).
    Overlay {
        /// Fixture name under `parity/mod_fixtures/`.
        #[arg(long)]
        fixture: String,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Region name to probe (repeatable).
        #[arg(long)]
        probe: Vec<String>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Time a mod pipeline scene (plan 20 M9 §7d).
    Bench {
        /// Scene to time: `discover`, `parse`, `patch`, `cache`, `overlay`.
        #[arg(long, default_value = "discover")]
        scene: String,
        /// Mod directory (discover; defaults to `parity/mod_fixtures`).
        #[arg(long, default_value = "parity/mod_fixtures")]
        dir: PathBuf,
        /// Fixture name under `parity/mod_fixtures/` (parse/patch/overlay).
        #[arg(long)]
        fixture: Option<String>,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Timed iterations.
        #[arg(long, default_value_t = 20)]
        runs: usize,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Apply a fixture mod's `patches/*.json` and verify `unapply` restores the
    /// baseline (plan 20 M3).
    Patch {
        /// Fixture name under `parity/mod_fixtures/`.
        #[arg(long)]
        fixture: String,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Load a fixture mod's data assets (patches/content/bundles/images/audio)
    /// headlessly and report `dp-` names, sound ids and external assets
    /// (plan 20 M4).
    Assets {
        /// Fixture name under `parity/mod_fixtures/`.
        #[arg(long)]
        fixture: String,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Load a fixture with one bad content file among good ones and verify the
    /// bad record is isolated while the good records still register (plan 20
    /// M1/M3 §7b).
    ErrorIsolation {
        /// Fixture name under `parity/mod_fixtures/`.
        #[arg(long)]
        fixture: String,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
