// SPDX-License-Identifier: GPL-3.0-only

//! `mind-tools` — offline asset pipeline CLI (plan 03 §3.5).
//!
//! The `gradlew tools:pack` replacement: upstream asset migration, sprite
//! staging/generation, atlas packing, id/bundle/shader indexes and manifests.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod migrate;

/// Offline asset pipeline for the Mindustry-Godot port.
#[derive(Debug, Parser)]
#[command(
    name = "mind-tools",
    version,
    about = "Offline asset pipeline (migrate/pack/ids) for Mindustry-Godot"
)]
struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Vendor upstream `core/assets` + `core/assets-raw` into this repo (M0).
    Migrate {
        /// Upstream Mindustry checkout (defaults to `$MIND_UPSTREAM`).
        #[arg(long)]
        from: Option<PathBuf>,
        /// Repo root to migrate into (defaults to the current directory).
        #[arg(long, default_value = ".")]
        to: PathBuf,
        /// Verify the vendored trees against the existing manifest instead of
        /// copying.
        #[arg(long)]
        check: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Migrate { from, to, check } => {
            let from = from
                .clone()
                .or_else(|| std::env::var_os("MIND_UPSTREAM").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("../Mindustry"));
            migrate::run(&from, to, *check).map(|_| ())
        }
    };
    if let Err(error) = result {
        eprintln!("mind-tools: error: {error:#}");
        std::process::exit(2);
    }
}
