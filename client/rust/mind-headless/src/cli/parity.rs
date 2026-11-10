// SPDX-License-Identifier: GPL-3.0-only

//! `parity` subcommands (plan 23 §3.7/§7).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `parity` subcommands (plan 23 §3.7/§7).
#[derive(Debug, Subcommand)]
pub enum ParityCommand {
    /// Run every structural check (matrix, checksums, scenarios, goldens,
    /// budgets, MCP catalog) and exit non-zero on any failure.
    Check {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// A `cargo test -- --list` dump used to resolve `status = "landed"` rows.
        #[arg(long)]
        tests: Option<PathBuf>,
    },

    /// Run a headless suite across the catalog (`smoke` = T0, `gate` = T1,
    /// `full` = T2) in-process and emit its `format: 1` report (§3.2).
    ///
    /// Only file-backed scenarios run here; embedded subsystem scenarios are
    /// exercised by their own subcommands/nightly runner.
    Run {
        /// Suite key (`smoke`, `gate`, `full`).
        #[arg(long, default_value = "smoke")]
        suite: String,
        /// Restrict to scenarios whose phase is at or before this phase (`P0`..`P8`).
        #[arg(long)]
        phase: Option<String>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Validate `parity/matrix.toml` (the upstream-test mapping registry).
    Matrix {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// A `cargo test -- --list` dump used to resolve `status = "landed"` rows.
        #[arg(long)]
        tests: Option<PathBuf>,
        /// Only check rows whose phase is at or before this phase (`P0`..`P8`).
        #[arg(long)]
        phase: Option<String>,
    },

    /// Validate `parity/checksum_registry.json` against `CHECKSUM_VERSION`.
    Registry {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Verify committed goldens in `parity/golden_manifest.json` (sha256).
    Goldens {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Validate the aggregated performance-budget registry.
    Budgets {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Validate the MCP scenario catalog.
    Mcp {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Validate the golden-scenario catalog and its backing files.
    Scenarios {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Run a bounded soak profile with RSS/alloc/checksum tracking.
    ///
    /// The full profile duration is minutes-scale and belongs on the nightly
    /// `perf` runner; `--minutes N` (or `--ticks N`) runs a bounded slice here.
    Soak {
        /// Profile name (`mid`, `stress`, `windowed`, `multiplayer`).
        #[arg(long, default_value = "mid")]
        profile: String,
        /// Override the profile duration in minutes.
        #[arg(long)]
        minutes: Option<u64>,
        /// Override the tick budget directly (`minutes * 60 * 60` otherwise).
        #[arg(long)]
        ticks: Option<u64>,
        /// Deterministic sim seed.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Feed deliberately corrupted checksum/command streams to the desync
    /// detector (plan 21) or, until 21 lands, its detector trait double.
    DesyncInject {
        /// Injection case (`wrong_checksum`, `reorder`, `duplicate`, `gap`,
        /// `skipped_command`, `content_hash_mismatch`, `late_command`,
        /// `speedhack_tick_stamp`).
        #[arg(long)]
        case: String,
        /// Deterministic corruption seed.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Compare headless scenario checksums against the in-engine MCP oracle.
    ///
    /// The windowed half needs the Godot editor (single-editor mutex) and is
    /// reported `deferred`; the headless half resolves every catalog entry
    /// against its committed golden now.
    McpParity {
        /// Suite (`T0` or a phase key `P0`..`P8`).
        #[arg(long, default_value = "T0")]
        suite: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Validate the screenshot-baseline manifest and (when a baseline exists)
    /// diff captures against it. In-engine capture is editor-gated.
    Screenshots {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Left PNG for a headless tolerance diff (`--diff-b` required).
        #[arg(long)]
        diff_a: Option<PathBuf>,
        /// Right PNG for a headless tolerance diff (`--diff-a` required).
        #[arg(long)]
        diff_b: Option<PathBuf>,
    },

    /// Roll up the parity registries into a program status report.
    Report {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Include the whole-program per-system roll-up (`parity report --all`,
        /// plan 23 M7).
        #[arg(long)]
        all: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Run the runnable steps of the phase-gate protocol and write its report.
    Gate {
        /// Phase key (`P0`..`P8`).
        phase: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Write the gate report JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Validate the benchmark-budget registry (recording is nightly-owned) and,
    /// when a committed baseline exists, its coverage + canonical rule.
    BenchGate {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Baseline file override (defaults to `bench/baselines.json`).
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Also validate the canonical regime (>10% regression gate, NUD-39/A).
        #[arg(long)]
        canonical: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Determinism matrix: run each catalogued scenario twice in-process, twice
    /// through its replayed `.simlog`, and once cross-process, comparing the
    /// canonical checksums across worker counts (`plan 23 M1`).
    Checksums {
        /// Suite key (`sm`/`smoke` = T0, `gate` = T1, `full` = T2).
        #[arg(long, default_value = "smoke")]
        suite: String,
        /// Worker counts to compare (recorded; plan 05 owns worker parity).
        #[arg(long, value_delimiter = ',', default_value = "1,4")]
        workers: Vec<usize>,
        /// Only run the in-process + replay halves (skip the child process).
        #[arg(long)]
        no_cross_process: bool,
        /// Emit the `format: 1` checksum matrix JSON on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Replay-order fuzz: mutate a known command log and prove the plan-05
    /// `CommandLog` ordering/dedup/truncation semantics (negative controls).
    ReplayFuzz {
        /// Deterministic base-log seed.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Comma-separated mutations (`reorder`, `dup`, `truncate`).
        #[arg(long, default_value = "reorder,dup,truncate")]
        mutations: String,
        /// Emit the `format: 1` fuzz report JSON on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Scenario-mirror check (plan 23 §5 M3/R-21): verify (or with `--write`
    /// materialize) the gitignored `client/scenarios/` mirror against the
    /// canonical repo `scenarios/` set.
    Mirror {
        /// Materialize the exact mirror instead of only checking it.
        #[arg(long)]
        write: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },
}
