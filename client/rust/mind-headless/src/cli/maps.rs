// SPDX-License-Identifier: GPL-3.0-only

//! `maps` subcommands (plan 19).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `maps` subcommands (plan 06 §7b).
#[derive(Debug, Subcommand)]
pub enum MapsCommand {
    /// Meta-only listing of a map/save directory, sorted per `Map.compareTo`
    /// (corrupt entries skipped with a warning).
    List {
        /// Directory to list.
        #[arg(long, default_value = "tests/fixtures/maps")]
        dir: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Editor-map save → load → save idempotence with rules/genfilters/locales
    /// tags (plan 19 M2 §5/§7b).
    SaveLoadSave {
        /// Fixture map name (only used for the temp file stems).
        #[arg(long, default_value = "editor_fixture")]
        map: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// `save_map`/`import_map` end-to-end with preview pixels (plan 06 M4 /
    /// plan 19 M7 §3.8): save a live grid, generate the preview PNG/cache,
    /// import it into a second registry and assert the preview checksums match.
    Roundtrip {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Deterministic hand-built tile-set preview pixels + PNG round-trip
    /// (plan 19 M2 §5/§7b).
    PreviewTiles {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// PNG image-map import/export round-trip (plan 19 M2 §5/§7b).
    ImageRoundtrip {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Registry ordering + `ShuffleMode` selection across seeds (plan 06 §7b,
    /// consumed/verified by plan 19 M2).
    RegistryShuffle {
        /// Number of selection iterations per mode.
        #[arg(long, default_value_t = 16)]
        seeds: u32,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// `MapFixer` port (plan 19 M8 §3.13/§7b): fixes a fixture map dir; the
    /// second write is a no-op. `--dry-run` prints the would-change list.
    Fix {
        /// Directory to scan; when omitted a temp fixture dir is generated.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Report changes without writing.
        #[arg(long)]
        dry_run: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
