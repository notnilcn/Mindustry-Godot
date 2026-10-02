// SPDX-License-Identifier: GPL-3.0-only

//! `mind-tools` — offline asset pipeline CLI (plan 03 §3.5).
//!
//! The `gradlew tools:pack` replacement: upstream asset migration, sprite
//! staging/generation, atlas packing, id/bundle/shader indexes and manifests.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, Subcommand};
use mind_tools::{antialias, generate, migrate, pack_pipeline, staging};

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

    /// Run the pack pipeline (§3.5): staging → … → pack → pack-fallback.
    Pack {
        /// Repo root (defaults to the current directory).
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Run only these stages (comma-separated). Implemented: staging,
        /// pack, pack-fallback, manifest.
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Skip the 2048 fallback atlas (upstream `-Pargs`).
        #[arg(long)]
        no_fallback: bool,
        /// Write per-stage timings to build/assets/pack_timings.json.
        #[arg(long)]
        timings: bool,
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
        Command::Pack {
            root,
            only,
            no_fallback,
            timings,
        } => run_pack(root, only, *no_fallback, *timings),
    };
    if let Err(error) = result {
        eprintln!("mind-tools: error: {error:#}");
        std::process::exit(2);
    }
}

/// Implemented pack stages (§3.5). Unknown names error; known-but-later
/// stages warn and skip.
fn run_pack(
    root: &std::path::Path,
    only: &[String],
    no_fallback: bool,
    timings: bool,
) -> anyhow::Result<()> {
    let source = root.join("assets-raw/sprites");
    let staging_dir = root.join("build/assets/staging");
    let out_dir = root.join("assets/sprites");
    let fallback_dir = out_dir.join("fallback");

    const IMPLEMENTED: &[&str] = &[
        "staging",
        "generate",
        "move-ui-icons",
        "antialias",
        "pack",
        "pack-fallback",
        "manifest",
    ];
    const LATER: &[&str] = &["enumerate", "ids", "shaders"];
    let selected: Vec<String> = if only.is_empty() {
        IMPLEMENTED.iter().map(|s| (*s).to_owned()).collect()
    } else {
        only.to_vec()
    };
    for stage in &selected {
        if !IMPLEMENTED.contains(&stage.as_str()) {
            if LATER.contains(&stage.as_str()) {
                eprintln!(
                    "mind-tools: stage `{stage}` is not implemented yet (see plan 03 §5); skipping"
                );
            } else {
                anyhow::bail!("unknown pack stage `{stage}` (implemented: {IMPLEMENTED:?})");
            }
        }
    }
    let run = |stage: &str| selected.iter().any(|s| s == stage);

    let mut timings_map: BTreeMap<String, f64> = BTreeMap::new();
    let mut outputs: BTreeMap<String, pack_pipeline::PackOutput> = BTreeMap::new();

    if run("staging") {
        let start = Instant::now();
        let copied = staging::stage(&source, &staging_dir)?;
        timings_map.insert(String::from("staging"), start.elapsed().as_secs_f64());
        println!("pack: staged {copied} files -> {}", staging_dir.display());
    }

    if run("generate") {
        let start = Instant::now();
        let mut ctx = generate::GenCtx::new(&staging_dir)?;
        let pass_timings = generate::run_filename_passes(&mut ctx, &mut |name, elapsed| {
            println!("pack: generate {name}: {elapsed:.2}s");
        })?;
        let elapsed = start.elapsed().as_secs_f64();
        timings_map.insert(String::from("generate"), elapsed);
        for (pass, seconds) in pass_timings {
            timings_map.insert(format!("generate/{pass}"), seconds);
        }
    }

    if run("move-ui-icons") {
        let start = Instant::now();
        let moved = antialias::move_ui_icons(&staging_dir)?;
        timings_map.insert(String::from("move-ui-icons"), start.elapsed().as_secs_f64());
        if moved > 0 {
            println!("pack: moved {moved} ui icon(s)");
        }
    }

    if run("antialias") {
        let start = Instant::now();
        let count = antialias::antialias(&staging_dir)?;
        timings_map.insert(String::from("antialias"), start.elapsed().as_secs_f64());
        println!("pack: antialiased {count} sprites");
    }

    if run("pack") {
        let start = Instant::now();
        let output = pack_pipeline::pack(&staging_dir, &out_dir, false)?;
        timings_map.insert(String::from("pack"), start.elapsed().as_secs_f64());
        println!(
            "pack: {} regions on {} page(s) -> {} (inputsHash {})",
            output.regions,
            output.pages,
            out_dir.display(),
            output.inputs_hash
        );
        outputs.insert(String::from("assets/sprites"), output);
    }

    if run("pack-fallback") && !no_fallback {
        let start = Instant::now();
        let output = pack_pipeline::pack(&staging_dir, &fallback_dir, true)?;
        timings_map.insert(String::from("pack-fallback"), start.elapsed().as_secs_f64());
        println!(
            "pack: fallback: {} regions on {} page(s) -> {}",
            output.regions,
            output.pages,
            fallback_dir.display()
        );
        outputs.insert(String::from("assets/sprites/fallback"), output);
    }

    if run("manifest") {
        pack_pipeline::write_asset_manifest(
            &outputs,
            &root.join("build/assets/asset_manifest.json"),
        )?;
    }

    if timings {
        let path = root.join("build/assets/pack_timings.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&timings_map)?),
        )?;
        for (stage, seconds) in &timings_map {
            println!("pack: {stage}: {seconds:.2}s");
        }
    }
    Ok(())
}
