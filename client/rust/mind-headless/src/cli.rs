// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless` command line (plan §3.5).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Headless test-rig for the `mind-core` simulation.
#[derive(Debug, Parser)]
#[command(
    name = "mind-headless",
    version = mind_core::MIND_VERSION,
    about = "Headless test-rig for the mind-core simulation (scenarios, dumps, benchmarks)"
)]
pub struct Cli {
    /// Scenarios directory override (defaults to the nearest `scenarios/` from cwd).
    #[arg(long, global = true)]
    pub scenarios_dir: Option<PathBuf>,

    /// Data directory override (logs; defaults to the platform data dir).
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,

    /// Log file path; defaults to `<data-dir>/last_log.txt`; pass `-` to disable.
    #[arg(long, global = true)]
    pub log_file: Option<String>,

    /// Requested command.
    #[command(subcommand)]
    pub command: Command,
}

/// `mind-headless` subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List registered scenarios.
    List,

    /// Print the build report (`Version.java` port; plan 22 M0).
    Version {
        /// Emit a machine-readable JSON object.
        #[arg(long)]
        json: bool,
        /// Read `version.properties` from this path instead of the embedded copy.
        #[arg(long)]
        file: Option<PathBuf>,
    },

    /// Run the dedicated server (`ServerLauncher`/`ServerControl` port; plan 22 M2).
    Server {
        /// Server data root (default `./config`).
        #[arg(long)]
        config_dir: Option<PathBuf>,
        /// Startup commands; comma-separated and/or repeatable (`--commands "a,b"`).
        #[arg(long = "commands", value_delimiter = ',')]
        commands: Vec<String>,
        /// Socket port override (`--socket-port 0` picks an ephemeral test port).
        #[arg(long)]
        socket_port: Option<u16>,
        /// SpacetimeDB mode (`online` connects; `offline` disables admin/host delegation).
        #[arg(long, default_value = "online")]
        stdb: String,
        /// Write boot timings JSON here.
        #[arg(long)]
        boot_timing_json: Option<PathBuf>,
        /// Write the tick profile JSON here (accepted; implemented with plan 23).
        #[arg(long)]
        profile_json: Option<PathBuf>,
    },

    /// Run a registered scenario and verify its golden expectations.
    Run {
        /// Scenario name (see `list`).
        scenario: String,
        /// Write the final state dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the applied command log (JSONL) here.
        #[arg(long)]
        emit_commands: Option<PathBuf>,
        /// Write the applied command log in binary `.simlog` form here.
        #[arg(long)]
        emit_simlog: Option<PathBuf>,
        /// Reset/play cycles for `sim_core_reset_play_cycle`.
        #[arg(long, default_value_t = 20)]
        cycles: u64,
        /// Collect a checksum every N ticks (0 = off) for `sim_core_determinism`.
        #[arg(long, default_value_t = 0)]
        checksum_every: u64,
        /// Compare sampled checksums against this golden file (one hex per line).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Write the sampled checksums (one hex per line) here.
        #[arg(long)]
        emit_checksums: Option<PathBuf>,
    },

    /// Run a fresh flat world for N ticks.
    Sim {
        /// Number of ticks.
        ticks: u64,
        /// Simulation seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// World width in tiles.
        #[arg(long, default_value_t = 32)]
        width: i32,
        /// World height in tiles.
        #[arg(long, default_value_t = 32)]
        height: i32,
        /// Write the final state dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Replay a command log against a fresh flat world.
    Replay {
        /// Command log (`.jsonl`) to replay.
        commands: PathBuf,
        /// Simulation seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Number of ticks (defaults to the last command tick + 1).
        #[arg(long)]
        ticks: Option<u64>,
        /// World width in tiles.
        #[arg(long, default_value_t = 32)]
        width: i32,
        /// World height in tiles.
        #[arg(long, default_value_t = 32)]
        height: i32,
        /// Write the final state dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Include every per-tick checksum in the JSON report.
        #[arg(long)]
        per_tick: bool,
        /// Collect a checksum every N ticks (0 = off).
        #[arg(long, default_value_t = 0)]
        checksum_every: u64,
        /// Worker count recorded for determinism runs (does not change output).
        #[arg(long, default_value_t = 1)]
        workers: usize,
        /// Compare sampled checksums against this golden file (one hex per line).
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Benchmark `Sim::tick` on a scenario (emits `{p50_us,p99_us}` JSON).
    Bench {
        /// Number of timed ticks.
        #[arg(long, default_value_t = 100_000)]
        ticks: u64,
        /// Benchmark scenario/profile (`spine` maps to `bench_baseline`;
        /// `stdb_pump` measures connector pump overhead, plan 01 §7.4;
        /// `sim_core` selects the `--profile` sim-core benchmark).
        #[arg(long, default_value = "spine")]
        scenario: String,
        /// Sim-core profile (`empty`/`mid`/`stress`; plan 05 §7.4).
        #[arg(long)]
        profile: Option<String>,
        /// Fail when more than N allocations occur across the timed region
        /// (requires `--features alloc-audit`).
        #[arg(long)]
        assert_alloc: Option<u64>,
    },

    /// Run a scenario and write its state dump.
    Dump {
        /// Scenario name.
        #[arg(long)]
        scenario: String,
        /// Output path for the dump JSON.
        #[arg(long)]
        out: PathBuf,
        /// Emit every tile instead of the sparse (non-air) default.
        #[arg(long)]
        all_tiles: bool,
    },

    /// Content registry inspection (plan 02).
    Content {
        /// Content subcommand.
        #[command(subcommand)]
        command: ContentCommand,
    },

    /// Asset pipeline verification (plan 03 §7b).
    Assets {
        /// Assets subcommand.
        #[command(subcommand)]
        command: AssetsCommand,
    },

    /// IO engine inspection (plan 04).
    Io {
        /// IO subcommand.
        #[command(subcommand)]
        command: IoCommand,
    },

    /// Entity/component metadata inspection (plan 05 M5).
    Meta {
        /// Metadata subcommand.
        #[command(subcommand)]
        command: MetaCommand,
    },

    /// Schedule order inspection (plan 05 M6 §7.2).
    Trace {
        /// Trace subcommand.
        #[command(subcommand)]
        command: TraceCommand,
    },

    /// World/terrain inspection (plan 06 §7b).
    World {
        /// World subcommand.
        #[command(subcommand)]
        command: WorldCommand,
    },

    /// Map registry inspection (plan 06 §7b).
    Maps {
        /// Map subcommand.
        #[command(subcommand)]
        command: MapsCommand,
    },

    /// Block/building runtime scenarios (plan 07 §7b).
    Blocks {
        /// Blocks subcommand.
        #[command(subcommand)]
        command: BlocksCommand,
    },

    /// Combat/bullets/defense scenarios (plan 10 §7b).
    Combat {
        /// Combat subcommand.
        #[command(subcommand)]
        command: CombatCommand,
    },

    /// Unit/AI/pathfinding scenarios (plan 11 §7b).
    Units {
        /// Units subcommand.
        #[command(subcommand)]
        command: UnitsCommand,
    },

    /// mlog text/VM scenarios (plan 13 §7b).
    Logic {
        /// Logic subcommand.
        #[command(subcommand)]
        command: LogicCommand,
    },

    /// Audio state-machine and event inspection (plan 18).
    Audio {
        /// Audio subcommand.
        #[command(subcommand)]
        command: AudioCommand,
    },

    /// Mod discovery/content/patch/asset inspection (plan 20).
    Mods {
        /// Mods subcommand.
        #[command(subcommand)]
        command: ModsCommand,
    },

    /// World render pipeline inspection (plan 16 §7.2).
    Render {
        /// Render subcommand.
        #[command(subcommand)]
        command: RenderCommand,
    },

    /// Campaign rules/tech scenarios (plan 12 §7b).
    Campaign {
        /// Campaign subcommand.
        #[command(subcommand)]
        command: CampaignCommand,
    },

    /// FX/effect/draw-part scenarios (plan 17 §7b).
    Fx {
        /// FX subcommand.
        #[command(subcommand)]
        command: FxCommand,
    },

    /// Power-network scenarios/benches (plan 09 §7b).
    Power {
        /// Power subcommand.
        #[command(subcommand)]
        command: NetworkCommand,
    },

    /// Liquid-network scenarios/benches (plan 09 §7b).
    Liquid {
        /// Liquid subcommand.
        #[command(subcommand)]
        command: NetworkCommand,
    },

    /// Heat-network scenarios/benches (plan 09 §7b).
    Heat {
        /// Heat subcommand.
        #[command(subcommand)]
        command: NetworkCommand,
    },

    /// Parity/verification registries, catalogs and gate reports (plan 23).
    Parity {
        /// Parity subcommand.
        #[command(subcommand)]
        command: ParityCommand,
    },
}

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

    /// Emit the execution plan for a soak profile (skeleton; nightly runs it).
    Soak {
        /// Profile name (`mid`, `stress`, `windowed`, `multiplayer`).
        #[arg(long, default_value = "mid")]
        profile: String,
        /// Override the profile duration in minutes.
        #[arg(long)]
        minutes: Option<u64>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Roll up the parity registries into a program status report.
    Report {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
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

    /// Validate the benchmark-budget registry (recording is nightly-owned).
    BenchGate {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },
}

/// `power`/`liquid`/`heat` subcommands (plan 09 §7b).
#[derive(Debug, Subcommand)]
pub enum NetworkCommand {
    /// List registered scenarios for this network.
    List,

    /// Run a network scenario; optionally write/verify its `NetworkState` JSON.
    Scenario {
        /// Scenario name.
        name: String,
        /// Write the canonical `NetworkState` dump JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the canonical dump against this golden file.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark the network update loop against the §7d budget.
    Bench {
        /// Number of network buildings to place.
        #[arg(long, default_value_t = 2000)]
        buildings: usize,
        /// Timed ticks.
        #[arg(long, default_value_t = 3600)]
        ticks: u64,
        /// Untimed warmup ticks.
        #[arg(long, default_value_t = 600)]
        warmup: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

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

/// `campaign` subcommands (plan 12 §7b).
#[derive(Debug, Subcommand)]
pub enum CampaignCommand {
    /// `campaign_rules_roundtrip`: JSON → Rules → TypeIO → JSON equality,
    /// `mode()`/checksum and one `RulesLoadEvent` (`from_save=false`).
    Rules {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_tech_unlock_gating`: locked→unlock gating, item spend,
    /// `req-` persistence, auto-unlocks and MP `Rules.researched`.
    Tech {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_sector_cycle`: planet/sector runtime, campaign rules + one
    /// production turn for a vanilla sector.
    Sector {
        /// Planet content name.
        #[arg(long, default_value = "serpulo")]
        planet: String,
        /// Sector preset name.
        #[arg(long, default_value = "groundZero")]
        sector: String,
        /// Fixed ticks to advance before the turn.
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_turn`: deterministic multi-turn production/export means.
    Turn {
        /// Number of turns to run.
        #[arg(long, default_value_t = 10)]
        turns: u32,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_schematic`: `.msch`/base64 round-trip, loadout decode and
    /// rotation of the vanilla starting schematics.
    Schematic {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_fog`: static exploration vs dynamic visibility + RLE chunk
    /// round-trip and attack-indicator lifecycle.
    Fog {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },
}

/// `fx` subcommands (plan 17 §7b).
#[derive(Debug, Subcommand)]
pub enum FxCommand {
    /// Audit the catalogue: length/order/name drift, classification counts and
    /// (with `--strict`) unported entries.
    Audit {
        /// Restrict to one wave (`F1`..`F6`).
        #[arg(long)]
        wave: Option<String>,
        /// Fail when any entry is unported.
        #[arg(long)]
        strict: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the scripted effect lifecycle trace and write/check its golden JSON.
    Lifecycle {
        /// Deterministic view seed.
        #[arg(long, default_value_t = 5)]
        seed: u64,
        /// Ticks to run.
        #[arg(long, default_value_t = 600)]
        ticks: u32,
        /// Write the trace JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the trace JSON against this golden.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Build the sorted `DrawPrim` program for one catalogue entry at a fixed
    /// time and write/check its golden JSON.
    Program {
        /// Effect name (`Fx.<name>`).
        name: String,
        /// Fixed time in ticks.
        #[arg(long, default_value_t = 3.0)]
        tick: f32,
        /// Write the program JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Compare the program JSON against this golden.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Deterministic trail update/detach/fade trace.
    Trail {
        /// Deterministic seed.
        #[arg(long, default_value_t = 3)]
        seed: u64,
        /// Write the trace JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the trace JSON against this golden.
        #[arg(long)]
        check: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark program resolution for N live states over M frames.
    Bench {
        /// Live states.
        #[arg(long, default_value_t = 2000)]
        states: usize,
        /// Frames to resolve.
        #[arg(long, default_value_t = 360)]
        frames: u32,
        /// Emit the machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Vertical-slice check: spawn one effect, build its program, assert pixels
    /// are emitted.
    Smoke {
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}

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
}

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

/// `combat` subcommands (plan 10 §7b).
#[derive(Debug, Subcommand)]
pub enum CombatCommand {
    /// Run a deterministic combat scenario and print its report/checksum.
    Scenario {
        /// Scenario name (`combat_basic`, `combat_bullet_pierce`,
        /// `combat_determinism`).
        name: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Dump live bullets for the state inspector.
    Dump {
        /// Only list live bullets.
        #[arg(long)]
        bullets_live: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Trace one bullet kind's position over N ticks.
    Trace {
        /// Fixture bullet kind/name (`fuse`, `rail`, `laser`, ...).
        #[arg(long)]
        kind: String,
        /// Ticks to trace.
        #[arg(long, default_value_t = 40)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark bullet update/collision.
    Bench {
        /// Live traveling bullets.
        #[arg(long, default_value_t = 2000)]
        bullets: usize,
        /// Firing `test-item` turrets included in the tick.
        #[arg(long, default_value_t = 0)]
        turrets: usize,
        /// Timed ticks.
        #[arg(long, default_value_t = 3600)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// `units` subcommands (plan 11 §7b).
#[derive(Debug, Subcommand)]
pub enum UnitsCommand {
    /// Run a deterministic unit/AI scenario and print its report/checksum.
    Scenario {
        /// Scenario name (`units_spawn_path_arrive`).
        name: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Spawn a unit and dump its state.
    Spawn {
        /// Unit content name (`dagger`, `mace`, `risso`, ...).
        #[arg(long, default_value = "dagger")]
        unit: String,
        /// Team id.
        #[arg(long, default_value_t = 0)]
        team: u8,
        /// World-pixel x.
        #[arg(long, default_value_t = 64.0)]
        x: f32,
        /// World-pixel y.
        #[arg(long, default_value_t = 64.0)]
        y: f32,
        /// Ticks to run after spawning.
        #[arg(long, default_value_t = 0)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Trace a unit pathing to a destination tile.
    Path {
        /// Unit content name.
        #[arg(long, default_value = "dagger")]
        unit: String,
        /// Destination tile x.
        #[arg(long, default_value_t = 40)]
        tx: i32,
        /// Destination tile y.
        #[arg(long, default_value_t = 40)]
        ty: i32,
        /// Ticks to run.
        #[arg(long, default_value_t = 1500)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Benchmark the unit AI + movement tick.
    Bench {
        /// Live units.
        #[arg(long, default_value_t = 300)]
        units: usize,
        /// Timed ticks.
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// `logic` subcommands (plan 13 §7b).
#[derive(Debug, Subcommand)]
pub enum LogicCommand {
    /// Parse a program, write it back, and (optionally) save the normalized text.
    Assemble {
        /// Input mlog file.
        file: PathBuf,
        /// Write the normalized program here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Treat the program as privileged (world processor).
        #[arg(long)]
        privileged: bool,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run a registered mlog scenario and print its checksum/variable report.
    Run {
        /// Scenario name (`logic_arith`, `logic_strings`, `logic_budget`, `logic_globals`).
        name: String,
        /// Override scenario ticks.
        #[arg(long)]
        ticks: Option<u64>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Parse a program and dump its statement field order (plan-14 metadata probe).
    Dump {
        /// Input mlog file.
        file: PathBuf,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// `audio` subcommands (plan 18 §7b).
#[derive(Debug, Subcommand)]
pub enum AudioCommand {
    /// Emit the scripted block-place/break/shoot/loop/music event sequence and
    /// dump the `RecordingAudioSink` output (`events_blocks.json` shape).
    Events {
        /// Scenario name (only `audio_events_blocks` is defined).
        #[arg(long, default_value = "audio_events_blocks")]
        scenario: String,
        /// Write the event dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the dump against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the scripted `SoundControl` context timeline and dump the selection
    /// trace (`music_select.json` shape).
    Music {
        /// Scenario name (only `audio_music_select` is defined).
        #[arg(long, default_value = "audio_music_select")]
        scenario: String,
        /// Frames to run.
        #[arg(long, default_value_t = 600)]
        frames: u32,
        /// Deterministic audio-RNG seed.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Write the trace dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the scripted loop aggregation and dump per-tick rows
    /// (`loops_aggregate.json` shape).
    Loops {
        /// Scenario name (only `audio_loop_aggregate` is defined).
        #[arg(long, default_value = "audio_loop_aggregate")]
        scenario: String,
        /// Ticks to run.
        #[arg(long, default_value_t = 120)]
        ticks: u32,
        /// Write the loop dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the `SoundPriority` admission/eviction policy over a synthetic or
    /// fixture request stream (`--requests`).
    Policy {
        /// Request fixture JSON (defaults to the built-in synthetic stream).
        #[arg(long)]
        requests: Option<PathBuf>,
        /// Write the decision dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Measure the Rust mix update (`MusicPlayer` + `LoopMixer` + drain).
    Bench {
        /// Timed ticks.
        #[arg(long, default_value_t = 3600)]
        ticks: u64,
        /// Live one-shot voices.
        #[arg(long, default_value_t = 128)]
        voices: usize,
        /// Loop sounds to aggregate per tick.
        #[arg(long, default_value_t = 256)]
        loops: usize,
        /// Ambient candidate count (0 disables).
        #[arg(long, default_value_t = 0)]
        ambient: usize,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

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
}

/// `trace` subcommands (plan 05 M6).
#[derive(Debug, Subcommand)]
pub enum TraceCommand {
    /// Render the deterministic `TickSet`/`EntitySet` execution order for the
    /// menu/paused/playing/editor/client run conditions.
    Order {
        /// Ticks the trace represents (informational; order is per-tick).
        #[arg(long, default_value_t = 1)]
        ticks: u64,
        /// Write the golden text here (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// `meta` subcommands (plan 05 M5 §6.2).
#[derive(Debug, Subcommand)]
pub enum MetaCommand {
    /// Dump the entity/component metadata JSON (`entitymeta.json` shape).
    Entities {
        /// Write the stable JSON here (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit the JSON report on stdout even when `--out` is set.
        #[arg(long)]
        json: bool,
    },
}

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
