// SPDX-License-Identifier: GPL-3.0-only

//! `io` subcommands (plan 04).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `io` subcommands (plan 04 §7b).
#[derive(Debug, Subcommand)]
pub enum IoCommand {
    /// Meta-only read of a save/map file (`SaveIO.getMeta`).
    DumpMeta {
        /// Save/map file path.
        file: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Settings persistence self-check (plan 04 M1 §7b): set → flush → reload
    /// equality, then a corrupted settings file falling back to defaults.
    /// Uses the global `--data-dir` as the settings root.
    Settings {
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Revision-manifest drift check for all entity defs (plan 04 M3 §7b):
    /// `EntityDefs!` codec fields vs committed `revisions/<NAME>/<N>.json`.
    CheckRevisions {
        /// Append the next `<N>.json` on drift instead of failing
        /// (old manifests are never touched).
        #[arg(long)]
        update: bool,
        /// `mind-core` crate directory (default: discovered upward from cwd).
        #[arg(long)]
        mind_core_dir: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Class-ID drift check (`entity_class_ids.toml` vs registry vs generated
    /// `class_ids.rs`; append-only discipline).
    CheckClassIds {
        /// Regenerate `class_ids.rs` and append missing defs at max+1.
        #[arg(long)]
        update: bool,
        /// `mind-core` crate directory (default: discovered upward from cwd).
        #[arg(long)]
        mind_core_dir: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Save round-trip on the synthetic world fixture (plan 04 M4 §7b):
    /// build → tick → checksum C0 → save → reset → load → assert C1 == C0.
    Roundtrip {
        /// Map to load. Only `synthetic` (the 64×64 fixture) exists until
        /// plan 06 lands real maps/generators.
        #[arg(long, default_value = "synthetic")]
        map: String,
        /// Synthetic world width.
        #[arg(long, default_value_t = 64)]
        width: u16,
        /// Synthetic world height.
        #[arg(long, default_value_t = 64)]
        height: u16,
        /// Ticks to simulate before saving.
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Save file path (`<out>-backup.msav` rotates on the next run).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Parallel meta-only listing of a map/save directory (plan 04 M5 §7b):
    /// corrupt entries are skipped with a warning.
    MapList {
        /// Directory to list (maps or saves).
        #[arg(long)]
        dir: PathBuf,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Save/load/meta timings on the synthetic fixture (plan 04 M8 §7b/§7d):
    /// records P50/P95 per phase. `serpulo/groundZero` waits for plan 06.
    BenchSave {
        /// Map profile. Only `synthetic` (64×64 fixture) exists until plan 06.
        #[arg(long, default_value = "synthetic")]
        map: String,
        /// Synthetic world width.
        #[arg(long, default_value_t = 64)]
        width: u16,
        /// Synthetic world height.
        #[arg(long, default_value_t = 64)]
        height: u16,
        /// Ticks simulated before saving (mid-game-ish fixture state).
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Timed iterations per phase.
        #[arg(long, default_value_t = 50)]
        iters: u64,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}
