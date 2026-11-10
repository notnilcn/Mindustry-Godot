// SPDX-License-Identifier: GPL-3.0-only

//! `editor` subcommands (plan 19).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `editor` subcommands (plan 19 §7b).
#[derive(Debug, Subcommand)]
pub enum EditorCommand {
    /// Replay a packed op-log fixture: apply → undo-all → redo-all, matching the
    /// three committed world checksums (plan 19 M0 §5/§7b).
    Ops {
        /// Fixture JSON path (relative to the current directory unless absolute).
        #[arg(long, default_value = "mind-core/tests/fixtures/editor/basic_ops.json")]
        fixture: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Editor lifecycle round-trip (plan 19 M1 §5/§7b): begin_edit/adopt → draw
    /// → undo-all → redo-all → save → reload, comparing normalized world
    /// checksums at each checkpoint.
    Roundtrip {
        /// Optional source map file; omitted builds a synthetic 32×32 editor map.
        #[arg(long)]
        map: Option<PathBuf>,
        /// Deterministic seed (reserved for generation filters).
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Resize-with-shift scenario (plan 19 M1 §5/§7b): a 100×100 painted map
    /// resized to 80×80 with a ±10 shift preserves in-bounds tiles/config data.
    ResizeShift {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Generation-preview determinism scenario (plan 19 M4 §5/§7b): a fixed
    /// filter stack over a 64×64 snapshot produces the same preview pixels on
    /// every run, and a mid-generation re-run coalesces to one result.
    GenPreview {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Editor playtest state-machine transitions (plan 19 M7 §3.11):
    /// `edit_in_game` → `resume_editing` → Shift `playtest` → `try_exit`, with
    /// the plan-12 `PlaySession` phase/rules asserted at each step.
    Playtest {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Objective JSON golden round-trip incl. `editorPos`/`parents` and
    /// descriptor-key coverage (plan 19 M5 §5/§7b).
    Objectives {
        /// Fixture JSON path (relative to the current directory unless absolute).
        #[arg(
            long,
            default_value = "mind-core/tests/fixtures/editor/objectives.json"
        )]
        fixture: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Wave-graph numeric series golden (`WaveGraphData` counts/health) (plan 19
    /// M5 §5/§7b).
    WaveGraph {
        /// Fixture JSON path holding the expected series checksum.
        #[arg(
            long,
            default_value = "mind-core/tests/fixtures/editor/wave_graph.json"
        )]
        fixture: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Map-locale apply/rollback scenario (plan 19 M6 §5).
    Locales {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Banned-block/unit set mutates `Rules` JSON round-trip (plan 19 M6 §5).
    Banned {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Data-asset zip import/export round-trip (plan 19 M6 §5/§3.12).
    Assets {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Editor op-recording / recache / fill benchmark (plan 19 M7 §7d).
    Bench {
        /// Benchmark suite: `recache`, `line`, `undo`, `fill`, `replay`.
        #[arg(long, default_value = "line")]
        suite: String,
        /// Grid edge size for the suite.
        #[arg(long, default_value_t = 200)]
        size: i32,
        /// Timed iterations.
        #[arg(long, default_value_t = 20)]
        runs: u32,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}
