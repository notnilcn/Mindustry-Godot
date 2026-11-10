// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless` command line (plan §3.5). The `Cli` root and the
//! `Command` tree live here; each subcommand enum lives in its own file.
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

mod assets;
mod audio;
mod blocks;
mod campaign;
mod combat;
mod content;
mod editor;
mod fx;
mod input;
mod io;
mod logic;
mod maps;
mod meta;
mod mods;
mod network;
mod parity;
mod render;
mod trace;
mod ui;
mod units;
mod world;

pub use assets::AssetsCommand;
pub use audio::AudioCommand;
pub use blocks::BlocksCommand;
pub use campaign::CampaignCommand;
pub use combat::CombatCommand;
pub use content::ContentCommand;
pub use editor::EditorCommand;
pub use fx::FxCommand;
pub use input::InputCommand;
pub use io::IoCommand;
pub use logic::LogicCommand;
pub use maps::MapsCommand;
pub use meta::MetaCommand;
pub use mods::ModsCommand;
pub use network::NetworkCommand;
pub use parity::ParityCommand;
pub use render::RenderCommand;
pub use trace::TraceCommand;
pub use ui::UiCommand;
pub use units::UnitsCommand;
pub use world::WorldCommand;

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
    /// List catalogued scenarios (optionally filtered by tier).
    List {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Only list scenarios in this tier (`T0`..`T3`).
        #[arg(long)]
        tier: Option<String>,
    },

    /// Run every file-backed scenario (optionally filtered by tier).
    RunAll {
        /// Only run scenarios in this tier (`T0`..`T3`).
        #[arg(long)]
        tier: Option<String>,
    },

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

    /// Run the dedicated multiplayer server shape (plan 21 §3.11 / plan 22).
    Serve {
        /// Server data root (default `./config`).
        #[arg(long)]
        config_dir: Option<PathBuf>,
        /// SpacetimeDB URI override (`--stdb-host`).
        #[arg(long)]
        stdb_host: Option<String>,
        /// SpacetimeDB database name override (`--db`).
        #[arg(long)]
        db: Option<String>,
        /// Service-token file: the dedicated server's identity (plan §3.11).
        #[arg(long)]
        admin_token_file: Option<PathBuf>,
        /// Match config JSON (`map_id`/`map_seed`/`mode`/`visibility`/...).
        #[arg(long)]
        match_config: Option<PathBuf>,
        /// SpacetimeDB mode (`online` connects; `offline` disables STDB).
        #[arg(long, default_value = "online")]
        stdb: String,
        /// Startup commands; comma-separated and/or repeatable.
        #[arg(long = "commands", value_delimiter = ',')]
        commands: Vec<String>,
        /// Write boot timings JSON here.
        #[arg(long)]
        boot_timing_json: Option<PathBuf>,
        /// Write the tick profile JSON here (accepted; plan 23).
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
        /// Record and print the final simulation checksum (plan 23 M4).
        #[arg(long)]
        checksum: bool,
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

    /// Map editor op-log oracle (plan 19 §7b).
    Editor {
        /// Editor subcommand.
        #[command(subcommand)]
        command: EditorCommand,
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

    /// UI text/builder/manifest scenarios (plan 14 §7b).
    Ui {
        /// UI subcommand.
        #[command(subcommand)]
        command: UiCommand,
    },

    /// Input/binding/placement scenarios and replay (plan 15 §7b).
    Input {
        /// Input subcommand.
        #[command(subcommand)]
        command: InputCommand,
    },

    /// Parity/verification registries, catalogs and gate reports (plan 23).
    Parity {
        /// Parity subcommand.
        #[command(subcommand)]
        command: ParityCommand,
    },
}
