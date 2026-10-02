// SPDX-License-Identifier: GPL-3.0-only

//! `mind-tools` — offline asset pipeline CLI (plan 03 §3.5).
//!
//! The `gradlew tools:pack` replacement: upstream asset migration, sprite
//! staging/generation, atlas packing, id/bundle/shader indexes and manifests.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context;
use clap::{Parser, Subcommand};
use mind_tools::{
    antialias, base_content, generate, generated_assets, migrate, pack_pipeline, staging,
};

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
        /// generate, move-ui-icons, antialias, pack, pack-fallback, manifest,
        /// ids, shaders.
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Skip the 2048 fallback atlas (upstream `-Pargs`).
        #[arg(long)]
        no_fallback: bool,
        /// Write per-stage timings to build/assets/pack_timings.json.
        #[arg(long)]
        timings: bool,
    },

    /// Regenerate `assets/icons/icons.properties` + `icon_codes.json`.
    Icons {
        /// `sync` writes both files (append-only codes).
        #[arg(value_enum)]
        command: IconsCommand,
        /// Repo root (defaults to the current directory).
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// Regenerate `assets/sounds.index.json` (sounds + musics, §6.5).
    Sounds {
        /// `index` writes the registry.
        #[arg(value_enum)]
        command: SoundsCommand,
        /// Repo root (defaults to the current directory).
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// Prepare/validate the shader manifest (§3.7, §6.7).
    Shaders {
        /// `build` copies ported `.gdshader` files and writes the index;
        /// `check` reports unported shaders and uniform/texture drift.
        #[arg(value_enum)]
        command: ShadersCommand,
        /// Also fail on ported-only uniforms/textures (plan 16 M8 reverse check).
        #[arg(long)]
        reverse: bool,
        /// Repo root (defaults to the current directory).
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// Determinism gate (plan 03 §7.1b `assets determinism`): run the full pack
    /// `--runs` times and assert every run is byte-identical.
    Determinism {
        /// Repo root (defaults to the current directory).
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Number of full packs to compare (≥ 2).
        #[arg(long, default_value_t = 2)]
        runs: u32,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Mod tooling (plan 20): `ClassMap` replacement manifest + drift gate.
    Mods {
        /// Subcommand to run.
        #[command(subcommand)]
        command: ModsCommand,
    },
}

/// `mind-tools mods` actions (plan 20 §3.5/§6.7).
#[derive(Debug, Subcommand)]
enum ModsCommand {
    /// Regenerate or verify `parity/mod_classmap.json` against the committed
    /// alias table (`--check` fails on drift or an inconsistent table).
    Classmap {
        /// Repo root (defaults to the current directory).
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Compare against the committed manifest instead of writing it.
        #[arg(long)]
        check: bool,
        /// Emit the manifest on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// `mind-tools icons` actions.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum IconsCommand {
    /// Append missing content codes and rewrite both id files.
    Sync,
}

/// `mind-tools sounds` actions.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum SoundsCommand {
    /// Write `assets/sounds.index.json`.
    Index,
}

/// `mind-tools shaders` actions.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum ShadersCommand {
    /// Copy ported shaders and write `shader.index.json`.
    Build,
    /// Report unported shaders and uniform/texture drift.
    Check,
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = dispatch(&cli.command) {
        eprintln!("mind-tools: error: {error:#}");
        std::process::exit(2);
    }
}

/// Dispatches one subcommand.
fn dispatch(command: &Command) -> anyhow::Result<()> {
    match command {
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
        Command::Icons { command, root } => match command {
            IconsCommand::Sync => {
                let registry = base_content()?;
                let report = generated_assets::sync_icons_properties(root, &registry)?;
                let glyphs = generated_assets::write_icon_codes(root)?;
                println!(
                    "icons sync: {} entries (+{}), {} font glyphs",
                    report.entries, report.added, glyphs
                );
                Ok(())
            }
        },
        Command::Sounds { command, root } => match command {
            SoundsCommand::Index => {
                let index = mind_tools::sounds::write_index(root)?;
                println!(
                    "sounds index: {} sounds, {} musics -> assets/sounds.index.json",
                    index.sounds.len(),
                    index.musics.len()
                );
                Ok(())
            }
        },
        Command::Shaders {
            command,
            reverse,
            root,
        } => match command {
            ShadersCommand::Build => {
                let index = mind_tools::shaders::build(root)?;
                println!(
                    "shaders build: {} shaders -> assets/shaders/shader.index.json",
                    index.shaders.len()
                );
                Ok(())
            }
            ShadersCommand::Check => {
                let report = mind_tools::shaders::check(root, *reverse)?;
                println!(
                    "shaders check: {} ported, {} unported ({} ok), {} drift, {} reverse drift",
                    report.ported,
                    report.unported.len(),
                    report.unported_ok.len(),
                    report.drift.len(),
                    report.reverse_drift.len()
                );
                for name in &report.unported {
                    eprintln!("shaders check: unported: {name}");
                }
                for drift in &report.drift {
                    eprintln!("shaders check: drift: {drift}");
                }
                for drift in &report.reverse_drift {
                    eprintln!("shaders check: reverse drift: {drift}");
                }
                if report.is_clean(*reverse) {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "{} unported + {} forward + {} reverse shader drift entries",
                        report.unported.len(),
                        report.drift.len(),
                        report.reverse_drift.len()
                    )
                }
            }
        },
        Command::Determinism { root, runs, json } => run_determinism(root, *runs, *json),
        Command::Mods { command } => match command {
            ModsCommand::Classmap { root, check, json } => run_mods_classmap(root, *check, *json),
        },
    }
}

/// Plan 20 §3.5/§6.7: writes (or `--check`s) the committed `ClassMap`
/// replacement manifest and validates the alias table.
fn run_mods_classmap(root: &std::path::Path, check: bool, json: bool) -> anyhow::Result<()> {
    use mind_core::mods::json::classmap::ClassTagMap;

    let map = ClassTagMap::committed();
    let issues = map.audit();
    let mut entries: Vec<_> = map.entries().iter().collect();
    entries.sort_by(|a, b| (a.scope.name(), a.alias).cmp(&(b.scope.name(), b.alias)));
    let classes: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "alias": entry.alias,
                "scope": entry.scope.name(),
                "tag": entry.tag,
            })
        })
        .collect();
    let manifest = serde_json::json!({ "format": 1, "classes": classes });
    let text = format!("{}\n", serde_json::to_string_pretty(&manifest)?);
    let path = root.join("parity/mod_classmap.json");

    if check {
        let existing = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        if existing != text {
            anyhow::bail!(
                "parity/mod_classmap.json is out of date (run `mind-tools mods classmap`)"
            );
        }
        if !issues.is_empty() {
            for issue in &issues {
                eprintln!("mods classmap: {issue}");
            }
            anyhow::bail!("{} ClassMap issue(s)", issues.len());
        }
        if json {
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        } else {
            println!("mods classmap check: {} entries, 0 issues", map.len());
        }
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
        if json {
            print!("{text}");
        } else {
            println!("mods classmap: {} entries -> {}", map.len(), path.display());
        }
    }
    Ok(())
}

/// Runs the full pack `runs` times and asserts the output manifest is
/// byte-identical across runs (`assets determinism`, plan 03 §2.2/§7.1b).
fn run_determinism(root: &std::path::Path, runs: u32, json: bool) -> anyhow::Result<()> {
    if runs < 2 {
        anyhow::bail!("determinism needs at least 2 runs (got {runs})");
    }
    let manifest_path = root.join("build/assets/asset_manifest.json");
    let mut reference: Option<Vec<u8>> = None;
    let mut hashes: Vec<String> = Vec::new();
    for run in 0..runs {
        run_pack(root, &[], false, false)?;
        let bytes = std::fs::read(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        hashes.push(mind_atlas::manifest::sha256_hex(&bytes));
        match &reference {
            None => reference = Some(bytes),
            Some(expected) => {
                if *expected != bytes {
                    anyhow::bail!(
                        "pack output differs between run 1 and run {} (manifest sha256 {})",
                        run + 1,
                        hashes[run as usize]
                    );
                }
            }
        }
    }
    if json {
        let report = serde_json::json!({
            "root": root.display().to_string(),
            "runs": runs,
            "manifest": manifest_path.display().to_string(),
            "hashes": hashes,
            "deterministic": true,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "determinism: {runs} full packs byte-identical (manifest sha256 {})",
            hashes[0]
        );
    }
    Ok(())
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
        "ids",
        "shaders",
    ];
    // Kept as accepted names for forward-compatibility with the plan's stage
    // list; `enumerate` is folded into `generate` (content registry build).
    const LATER: &[&str] = &["enumerate"];
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
        // Upstream `tools:pack` writes `core/build/last_pack_version`.
        let version_path = root.join("build/assets/last_pack_version");
        if let Some(parent) = version_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&version_path, env!("CARGO_PKG_VERSION"))?;
        timings_map.insert(String::from("staging"), start.elapsed().as_secs_f64());
        println!("pack: staged {copied} files -> {}", staging_dir.display());
    }

    let mut generated: Option<generate::GenCtx> = None;
    if run("generate") {
        let start = Instant::now();
        let registry = base_content()?;
        let mut ctx = generate::GenCtx::new(&staging_dir)?;
        let pass_timings = generate::run_passes(&mut ctx, &registry, &mut |name, elapsed| {
            println!("pack: generate {name}: {elapsed:.2}s");
        })?;
        // Upstream writes icons.properties at the end of `ImagePacker.main`
        // and regenerates the Icon/Iconc code tables from it.
        let report = generated_assets::sync_icons_properties(root, &registry)?;
        let glyphs = generated_assets::write_icon_codes(root)?;
        println!(
            "pack: icons.properties {} entries (+{}), {} font glyphs",
            report.entries, report.added, glyphs
        );
        // Content-driven region inventory (plan 03 §7.1b).
        let inventory = generate::inventory::build(&ctx, &registry);
        let inventory_path = root.join("build/assets/region_inventory.json");
        if let Some(parent) = inventory_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &inventory_path,
            format!("{}\n", serde_json::to_string_pretty(&inventory)?),
        )?;
        println!(
            "pack: region inventory {} expected, {} missing in sources",
            inventory.len(),
            inventory.missing_in_sources.len()
        );
        if !inventory.missing_in_sources.is_empty() {
            anyhow::bail!(
                "region inventory has {} missing source regions (first: {})",
                inventory.missing_in_sources.len(),
                inventory.missing_in_sources[0]
            );
        }
        let elapsed = start.elapsed().as_secs_f64();
        timings_map.insert(String::from("generate"), elapsed);
        for (pass, seconds) in pass_timings {
            timings_map.insert(format!("generate/{pass}"), seconds);
        }
        generated = Some(ctx);
    }
    let no_extras: BTreeMap<String, mind_atlas::pixmaps::Pixmap> = BTreeMap::new();
    let extras = generated
        .as_ref()
        .map(|ctx| &ctx.extras)
        .unwrap_or(&no_extras);

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
        let output = pack_pipeline::pack(&staging_dir, &out_dir, false, extras)?;
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
        let output = pack_pipeline::pack(&staging_dir, &fallback_dir, true, extras)?;
        timings_map.insert(String::from("pack-fallback"), start.elapsed().as_secs_f64());
        println!(
            "pack: fallback: {} regions on {} page(s) -> {}",
            output.regions,
            output.pages,
            fallback_dir.display()
        );
        outputs.insert(String::from("assets/sprites/fallback"), output);
    }

    if run("shaders") {
        let start = Instant::now();
        let index = mind_tools::shaders::build(root)?;
        let report = mind_tools::shaders::check(root, true)?;
        timings_map.insert(String::from("shaders"), start.elapsed().as_secs_f64());
        println!(
            "pack: shaders {} indexed ({} ported, {} unported, {} drift, {} reverse drift)",
            index.shaders.len(),
            report.ported,
            report.unported.len(),
            report.drift.len(),
            report.reverse_drift.len()
        );
        for name in &report.unported {
            eprintln!("mind-tools: shader `{name}` is not ported to .gdshader yet");
        }
        for drift in &report.drift {
            eprintln!("mind-tools: shader drift: {drift}");
        }
        for drift in &report.reverse_drift {
            eprintln!("mind-tools: shader reverse drift: {drift}");
        }
    }

    if run("ids") {
        let start = Instant::now();
        // Id tables (plan 03 §3.5 stage 8).
        let registry = base_content()?;
        let icons = generated_assets::sync_icons_properties(root, &registry)?;
        let glyphs = generated_assets::write_icon_codes(root)?;
        let sounds = mind_tools::sounds::write_index(root)?;
        let locales = generated_assets::write_locales(root)?;
        if !run("shaders") {
            mind_tools::shaders::build(root)?;
        }
        let regions = pack_pipeline::write_region_names(
            &out_dir,
            &root.join("assets/parity/region_names.txt"),
        )?;
        timings_map.insert(String::from("ids"), start.elapsed().as_secs_f64());
        println!(
            "pack: ids -> icons {} (+{}), font glyphs {}, sounds {}, musics {}, locales {}, regions {}",
            icons.entries,
            icons.added,
            glyphs,
            sounds.sounds.len(),
            sounds.musics.len(),
            locales,
            regions
        );
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
