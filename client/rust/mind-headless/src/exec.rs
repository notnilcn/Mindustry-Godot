// SPDX-License-Identifier: GPL-3.0-only

//! Command implementations.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, anyhow};
use log::LevelFilter;
use mind_core::command::CommandRecord;
use mind_core::config::MindConfig;
use mind_core::content::{
    AssetManifest, Blocks, BundleKeysFile, ContentRegistry, GoldenContent, MemoryBundle,
    MemoryUnlockStore, audit, content_counts, create_base_content, dump_golden,
};
use mind_core::scenario::{Scenario, ScenarioPlayer, read_command_log, write_command_log};
use mind_core::sim::{Sim, StateDump};
use mind_core::world::TilePos;

use crate::cli::{
    AssetsCommand, Cli, Command, ContentCommand, IoCommand, MetaCommand, TraceCommand,
};
use crate::paths;
use crate::registry;
use crate::report::{
    BenchReport, ContentBenchReport, ContentIdEntry, ContentIdsReport, ContentLoadReport,
    ContentTypeCount, ContentTypeEntries, IoBenchSaveReport, IoBenchStat, IoCheckClassIdsReport,
    IoCheckRevisionsReport, IoDefRevisionReport, IoDumpMetaReport, IoMapListEntry, IoMapListReport,
    IoRoundtripReport, IoSettingsReport, RunReport, SimReport, TileCheck,
};
use crate::stdb_scenarios::StdbScenario;

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;
/// Exit code: usage/IO error.
const EXIT_USAGE: i32 = 2;

/// Initializes logging and dispatches the parsed command.
pub fn run(cli: Cli) -> i32 {
    init_logging(&cli);
    match dispatch(cli) {
        Ok(code) => code,
        Err(error) => {
            log::error!("{error:#}");
            EXIT_USAGE
        }
    }
}

fn init_logging(cli: &Cli) {
    let data_dir = cli
        .data_dir
        .clone()
        .unwrap_or_else(|| MindConfig::new().data_dir);
    let file = match cli.log_file.as_deref() {
        Some("-") => None,
        Some(path) => Some(PathBuf::from(path)),
        None => Some(data_dir.join("last_log.txt")),
    };
    let config = mind_core::log::LogConfig {
        level: LevelFilter::Info,
        file,
        color: std::io::stderr().is_terminal(),
    };
    if let Err(error) = mind_core::log::init(config) {
        eprintln!("mind-headless: warning: {error}");
    }
}

fn dispatch(cli: Cli) -> anyhow::Result<i32> {
    match &cli.command {
        Command::List => {
            for name in registry::names() {
                println!("{name}");
            }
            Ok(EXIT_PASS)
        }
        Command::Run {
            scenario,
            dump,
            json,
            emit_commands,
        } => cmd_run(
            &cli,
            scenario,
            dump.as_deref(),
            *json,
            emit_commands.as_deref(),
        ),
        Command::Sim {
            ticks,
            seed,
            width,
            height,
            dump,
            json,
        } => cmd_sim(&cli, *ticks, *seed, *width, *height, dump.as_deref(), *json),
        Command::Replay {
            commands,
            seed,
            ticks,
            width,
            height,
            dump,
            json,
            per_tick,
        } => cmd_replay(
            &cli,
            commands,
            *seed,
            *ticks,
            *width,
            *height,
            dump.as_deref(),
            *json,
            *per_tick,
        ),
        Command::Bench { ticks, scenario } => cmd_bench(&cli, *ticks, scenario),
        Command::Dump {
            scenario,
            out,
            all_tiles,
        } => cmd_dump(&cli, scenario, out, *all_tiles),
        Command::Content { command } => match command {
            ContentCommand::Load { json } => cmd_content_load(*json),
            ContentCommand::Ids { json, out } => cmd_content_ids(*json, out.as_deref()),
            ContentCommand::Bench { runs, json } => cmd_content_bench(*runs, *json),
            ContentCommand::LoadOrderBad => cmd_content_load_order_bad(),
            ContentCommand::Dump { out, bundle, json } => {
                cmd_content_dump(out.as_deref(), bundle.as_deref(), *json)
            }
            ContentCommand::Audit {
                golden,
                bundle,
                manifest,
                out,
                json,
            } => cmd_content_audit(
                golden.as_deref(),
                bundle.as_deref(),
                manifest.as_deref(),
                out.as_deref(),
                *json,
            ),
        },
        Command::Assets { command } => match command {
            AssetsCommand::MigrateCheck { repo, manifest } => {
                cmd_assets_migrate_check(repo.as_deref(), manifest.as_deref())
            }
            AssetsCommand::Index { atlas, region } => cmd_assets_index(atlas, region),
            AssetsCommand::Regions {
                atlas,
                inventory,
                assert_complete,
                json,
            } => cmd_assets_regions(atlas, inventory.as_deref(), *assert_complete, *json),
        },
        Command::Meta { command } => match command {
            MetaCommand::Entities { out, json } => cmd_meta_entities(out.as_deref(), *json),
        },
        Command::Trace { command } => match command {
            TraceCommand::Order { ticks, out, json } => {
                cmd_trace_order(*ticks, out.as_deref(), *json)
            }
        },
        Command::Io { command } => match command {
            IoCommand::DumpMeta { file, json } => cmd_io_dump_meta(file, *json),
            IoCommand::Settings { json } => cmd_io_settings(&cli, *json),
            IoCommand::CheckRevisions {
                update,
                mind_core_dir,
                json,
            } => cmd_io_check_revisions(*update, mind_core_dir.as_deref(), *json),
            IoCommand::CheckClassIds {
                update,
                mind_core_dir,
                json,
            } => cmd_io_check_class_ids(*update, mind_core_dir.as_deref(), *json),
            IoCommand::Roundtrip {
                map,
                width,
                height,
                ticks,
                out,
                json,
            } => cmd_io_roundtrip(map, *width, *height, *ticks, out.as_deref(), *json),
            IoCommand::MapList { dir, json } => cmd_io_map_list(dir, *json),
            IoCommand::BenchSave {
                map,
                width,
                height,
                ticks,
                iters,
                json,
            } => cmd_io_bench_save(map, *width, *height, *ticks, *iters, *json),
        },
    }
}

/// Plan 03 M1 `assets index`: manifest summary + region probes as JSON.
fn cmd_assets_index(atlas: &Path, probes: &[String]) -> anyhow::Result<i32> {
    let manifest_path = atlas.join("sprites.atlas.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let index = mind_core::assets::atlas::AtlasIndex::from_manifest_json(&text)
        .map_err(|error| anyhow!("{error}"))?;

    let mut pass = true;
    let probe_reports: Vec<serde_json::Value> = probes
        .iter()
        .map(|name| {
            let found = index.find(name);
            if found.is_none() {
                log::error!("assets index: region `{name}` not found");
                pass = false;
            }
            match found {
                Some(region) => serde_json::json!({
                    "name": name,
                    "found": true,
                    "page": region.page,
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                    "splits": region.splits,
                    "pads": region.pads,
                    "offsets": region.offsets,
                    "pageType": region.page_type.name(),
                }),
                None => serde_json::json!({"name": name, "found": false}),
            }
        })
        .collect();

    let report = serde_json::json!({
        "atlas": manifest_path.display().to_string(),
        "fallback": index.fallback,
        "inputsHash": index.inputs_hash,
        "pages": index.pages().iter().map(|page| serde_json::json!({
            "index": page.index,
            "type": page.type_.name(),
            "file": page.file,
            "width": page.width,
            "height": page.height,
        })).collect::<Vec<_>>(),
        "regions": index.len(),
        "probes": probe_reports,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Minimal shape of `build/assets/region_inventory.json` (plan 03 M3).
#[derive(Debug, serde::Deserialize)]
struct RegionInventoryFile {
    #[serde(default)]
    regions: Vec<String>,
    #[serde(default, rename = "missingInSources")]
    missing_in_sources: Vec<String>,
}

/// Plan 03 §7.1b `assets regions`: every content-driven expected region must
/// resolve in the packed atlas. `--assert-complete` turns misses into a
/// non-zero exit; misses are always listed.
fn cmd_assets_regions(
    atlas: &Path,
    inventory: Option<&Path>,
    assert_complete: bool,
    json: bool,
) -> anyhow::Result<i32> {
    let manifest_path = atlas.join("sprites.atlas.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let index = mind_core::assets::atlas::AtlasIndex::from_manifest_json(&text)
        .map_err(|error| anyhow!("{error}"))?;

    let inventory_path = match inventory {
        Some(path) => path.to_path_buf(),
        None => paths::find_repo_root(None)?.join("build/assets/region_inventory.json"),
    };
    let inventory_text = std::fs::read_to_string(&inventory_path)
        .with_context(|| format!("reading {}", inventory_path.display()))?;
    let inventory: RegionInventoryFile = serde_json::from_str(&inventory_text)
        .with_context(|| format!("parsing {}", inventory_path.display()))?;

    // `block_colors` is a loose texture next to the pages, not an atlas region.
    let missing: Vec<&str> = inventory
        .regions
        .iter()
        .filter(|name| {
            if name.as_str() == "block_colors" {
                !atlas.join("block_colors.png").is_file()
            } else {
                index.find(name).is_none()
            }
        })
        .map(String::as_str)
        .collect();

    if json {
        let report = serde_json::json!({
            "atlas": manifest_path.display().to_string(),
            "inventory": inventory_path.display().to_string(),
            "expected": inventory.regions.len(),
            "resolved": inventory.regions.len() - missing.len(),
            "missing": missing,
            "missingInSources": inventory.missing_in_sources,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets regions: {}/{} expected regions resolve ({} missing)",
            inventory.regions.len() - missing.len(),
            inventory.regions.len(),
            missing.len()
        );
        for name in &missing {
            log::error!("assets regions: missing region `{name}`");
        }
    }

    if inventory.missing_in_sources.is_empty() && (missing.is_empty() || !assert_complete) {
        Ok(EXIT_PASS)
    } else {
        if !inventory.missing_in_sources.is_empty() {
            log::error!(
                "assets regions: {} expected regions were missing in the source atlas",
                inventory.missing_in_sources.len()
            );
        }
        Ok(EXIT_FAIL)
    }
}

/// Plan 03 §7.1b `assets migrate-check`: the vendored trees match
/// `build/assets/migration_manifest.json` and forbidden generated files are absent.
fn cmd_assets_migrate_check(repo: Option<&Path>, manifest: Option<&Path>) -> anyhow::Result<i32> {
    let root = paths::find_repo_root(repo)?;
    let manifest_path = manifest
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("build/assets/migration_manifest.json"));
    let problems = mind_atlas::migrate::MigrationManifest::read(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?
        .verify(&root)?;
    if problems.is_empty() {
        println!("assets migrate-check: OK (repo {})", root.display());
        Ok(EXIT_PASS)
    } else {
        for problem in &problems {
            log::error!("assets migrate-check: {problem}");
        }
        Ok(EXIT_FAIL)
    }
}

/// Plan 04 M5 (§7b): parallel meta-only listing of one map/save directory.
fn cmd_io_map_list(dir: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::slot::list_files_meta;

    let fs = NativeFs;
    let candidates = fs
        .ls(dir)?
        .into_iter()
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("msav")
                && !path
                    .file_name()
                    .map(|name| name.to_string_lossy().contains("backup"))
                    .unwrap_or(false)
        })
        .count();
    let entries: Vec<IoMapListEntry> = list_files_meta(&fs, dir)
        .into_iter()
        .map(|(file, meta)| IoMapListEntry {
            file: file.display().to_string(),
            name: if meta.is_map() {
                meta.tags.get("name").cloned().unwrap_or_default()
            } else {
                meta.map_name.clone()
            },
            width: meta.width(),
            height: meta.height(),
            wave: meta.wave,
            build: meta.build,
            format_version: meta.version,
            is_map: meta.is_map(),
            mods: meta.mods.len(),
        })
        .collect();
    let skipped = candidates.saturating_sub(entries.len());
    if !json {
        for entry in &entries {
            println!(
                "{}: {} {}x{} wave={} build={} v={}{}",
                entry.file,
                entry.name,
                entry.width,
                entry.height,
                entry.wave,
                entry.build,
                entry.format_version,
                if entry.is_map { " [map]" } else { "" }
            );
        }
        println!("{} listed, {skipped} skipped (corrupt)", entries.len());
    }
    let report = IoMapListReport {
        dir: dir.display().to_string(),
        listed: entries.len(),
        skipped,
        entries,
    };
    print_report(&report, json)?;
    Ok(EXIT_PASS)
}

/// Plan 04 M4 (§7b): the native v1 map/entities round-trip on the synthetic
/// fixture world (stands in for `serpulo/groundZero` until plan 06 lands).
fn cmd_io_roundtrip(
    map: &str,
    width: u16,
    height: u16,
    ticks: u64,
    out: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::fixture::{FixtureContext, FixtureSink, FixtureWorld};
    use mind_core::io::save::versions::v1::base_meta_tags;
    use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};

    if map != "synthetic" {
        return Err(anyhow!(
            "io roundtrip --map {map}: real maps need plan 06 (world/generators); use `--map synthetic`"
        ));
    }
    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("mind-io-roundtrip.msav"));
    let fs = NativeFs;
    let registry = boot_content()?;

    // Build + simulate.
    let mut world = FixtureWorld::synthetic(&registry, width, height);
    for _ in 0..ticks {
        world.tick();
    }
    let checksum_before = world.checksum_hex();

    // Save.
    let mut tags = base_meta_tags(width, height, world.wave, "synthetic");
    tags.insert("tick".to_owned(), world.tick.to_string());
    let mut ctx = WriteContext::meta_only(tags);
    ctx.content = Some(&registry);
    ctx.map = Some(&world);
    ctx.entities = Some(&world);
    SaveIo::save(&fs, &out, &ctx, &SaveOptions::new())?;
    let bytes = fs.len(&out).unwrap_or(0);

    // Load into a fresh world.
    let cell = std::cell::RefCell::new(FixtureWorld::new(&registry, 0, 0));
    let mut context = FixtureContext(&cell);
    let mut sink = FixtureSink(&cell);
    let mut load_registry = boot_content()?;
    let mut state = SaveReadState {
        context: Some(&mut context),
        content: Some(&mut load_registry),
        entities: Some(&mut sink),
        ..SaveReadState::default()
    };
    SaveIo::load(&fs, &out, &mut state)?;
    let state_tags = state.tags.clone();
    let state_team_plans = state.team_plans.clone();
    let all_buildings = state.all_buildings.len();
    drop(state);
    let mut loaded = cell.into_inner();
    loaded.apply_meta(&state_tags);
    loaded.apply_team_plans(state_team_plans);
    let checksum_after = loaded.checksum_hex();

    let pass = checksum_before == checksum_after;
    if !pass {
        log::error!("io roundtrip checksum mismatch: {checksum_before} != {checksum_after}");
    }
    let report = IoRoundtripReport {
        map: map.to_owned(),
        width,
        height,
        ticks,
        out: out.display().to_string(),
        bytes,
        buildings: all_buildings,
        checksum_before,
        checksum_after,
        pass,
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 04 M8 (§7b/§7d): P50/P95 timings for save/load/meta on the synthetic
/// fixture world (the §7d `serpulo/groundZero` baseline needs plan 06 maps).
fn cmd_io_bench_save(
    map: &str,
    width: u16,
    height: u16,
    ticks: u64,
    iters: u64,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::fixture::{FixtureContext, FixtureSink, FixtureWorld};
    use mind_core::io::save::versions::v1::base_meta_tags;
    use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};

    if map != "synthetic" {
        return Err(anyhow!(
            "io bench-save --map {map}: real maps need plan 06 (world/generators); use `--map synthetic`"
        ));
    }
    if iters == 0 {
        return Err(anyhow!("io bench-save --iters must be greater than 0"));
    }
    let fs = NativeFs;
    let path = std::env::temp_dir().join("mind-io-bench.msav");
    let backup = SaveIo::backup_file_for(&path);
    let registry = boot_content()?;

    // Mid-game-ish fixture: build + tick once (outside the timed regions).
    let mut world = FixtureWorld::synthetic(&registry, width, height);
    for _ in 0..ticks {
        world.tick();
    }
    let checksum_before = world.checksum_hex();
    let mut tags = base_meta_tags(width, height, world.wave, "synthetic");
    tags.insert("tick".to_owned(), world.tick.to_string());

    // Save: serialize + deflate + atomic file write (fresh file every run).
    let mut save_samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let _ = fs.delete(&path);
        let _ = fs.delete(&backup);
        let mut ctx = WriteContext::meta_only(tags.clone());
        ctx.content = Some(&registry);
        ctx.map = Some(&world);
        ctx.entities = Some(&world);
        let start = Instant::now();
        SaveIo::save(&fs, &path, &ctx, &SaveOptions::new())?;
        save_samples.push(start.elapsed().as_nanos() as u64);
    }
    let bytes = fs.len(&path).unwrap_or(0);

    // Load: read + inflate + apply regions into a fresh fixture world.
    let mut load_registry = boot_content()?;
    let mut load_samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let cell = std::cell::RefCell::new(FixtureWorld::new(&registry, 0, 0));
        let mut context = FixtureContext(&cell);
        let mut sink = FixtureSink(&cell);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut load_registry),
            entities: Some(&mut sink),
            ..SaveReadState::default()
        };
        let start = Instant::now();
        SaveIo::load(&fs, &path, &mut state)?;
        load_samples.push(start.elapsed().as_nanos() as u64);
    }

    // Meta-only read.
    let mut meta_samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let start = Instant::now();
        let _ = SaveIo::get_meta(&fs, &path)?;
        meta_samples.push(start.elapsed().as_nanos() as u64);
    }

    // Load verification (not timed): last read applies meta + plans and must
    // reproduce the save-side checksum.
    let cell = std::cell::RefCell::new(FixtureWorld::new(&registry, 0, 0));
    let mut context = FixtureContext(&cell);
    let mut sink = FixtureSink(&cell);
    let mut state = SaveReadState {
        context: Some(&mut context),
        content: Some(&mut load_registry),
        entities: Some(&mut sink),
        ..SaveReadState::default()
    };
    SaveIo::load(&fs, &path, &mut state)?;
    let state_tags = state.tags.clone();
    let state_team_plans = state.team_plans.clone();
    drop(state);
    let mut loaded = cell.into_inner();
    loaded.apply_meta(&state_tags);
    loaded.apply_team_plans(state_team_plans);
    let pass = checksum_before == loaded.checksum_hex();
    if !pass {
        log::error!("io bench-save load verification failed: checksums differ");
    }

    let report = IoBenchSaveReport {
        map: map.to_owned(),
        width,
        height,
        ticks,
        iters,
        bytes,
        save: io_bench_stat(&mut save_samples),
        load: io_bench_stat(&mut load_samples),
        meta: io_bench_stat(&mut meta_samples),
        pass,
        note: String::from(
            "synthetic 64x64 fixture; the 7d groundZero baseline needs plan 06 real maps",
        ),
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 04 M3 (§7b): revision drift check for every `EntityDefs!` def.
fn cmd_io_check_revisions(
    update: bool,
    mind_core_dir: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::entity::registry::entity_defs;
    use mind_core::io::entity::revisions::{RevisionCheck, check_all};
    use mind_core::io::fs::NativeFs;

    let dir = paths::find_mind_core_dir(mind_core_dir)?;
    let root = dir.join("revisions");
    let reports = check_all(&NativeFs, &root, entity_defs(), update)?;
    let mut pass = true;
    let mut def_reports = Vec::new();
    for report in &reports {
        let (status, details) = match &report.outcome {
            RevisionCheck::UpToDate => (String::from("up-to-date"), Vec::new()),
            RevisionCheck::Missing => {
                if update {
                    (String::from("written"), Vec::new())
                } else {
                    pass = false;
                    (
                        String::from("missing"),
                        vec![String::from("no manifests committed")],
                    )
                }
            }
            RevisionCheck::Drift { details, .. } => {
                if update {
                    (String::from("updated"), details.clone())
                } else {
                    pass = false;
                    (String::from("drift"), details.clone())
                }
            }
        };
        if !json {
            println!("{}: {status}", report.name);
            for detail in &details {
                println!("  - {detail}");
            }
        }
        def_reports.push(IoDefRevisionReport {
            name: report.name.clone(),
            status,
            updated_to: report.updated_to,
            details,
        });
    }
    let report = IoCheckRevisionsReport {
        revisions_root: root.display().to_string(),
        update,
        pass,
        defs: def_reports,
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 04 M3: class-ID drift gate (`entity_class_ids.toml` ↔ registry ↔
/// generated `class_ids.rs`).
fn cmd_io_check_class_ids(
    update: bool,
    mind_core_dir: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::entity::idfile::ClassIdFile;
    use mind_core::io::entity::registry::entity_defs;
    use mind_core::io::fs::{FileSystem, NativeFs};

    let dir = paths::find_mind_core_dir(mind_core_dir)?;
    let toml_path = dir.join("entity_class_ids.toml");
    let rs_path = dir.join("src/io/entity/class_ids.rs");
    let fs = NativeFs;

    let text = String::from_utf8(fs.read(&toml_path)?)
        .map_err(|_| anyhow!("`{}` is not UTF-8", toml_path.display()))?;
    let file = ClassIdFile::parse(&text)?;
    let mut problems = file.problems(entity_defs());

    // The generated constants file must match the TOML exactly (drift gate).
    let expected_rs = file.render_rs();
    let committed_rs = fs
        .read(&rs_path)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    if committed_rs != expected_rs {
        problems.push(String::from(
            "src/io/entity/class_ids.rs is stale (run `io check-class-ids --update`)",
        ));
    }

    let mut updated = false;
    if update && !problems.is_empty() {
        let with_new = file.with_new_defs(entity_defs());
        fs.write(&toml_path, with_new.render_toml().as_bytes())?;
        fs.write(&rs_path, with_new.render_rs().as_bytes())?;
        updated = true;
        // Re-validate after regeneration.
        problems = with_new.problems(entity_defs());
    }

    let pass = problems.is_empty();
    let report = IoCheckClassIdsReport {
        toml: toml_path.display().to_string(),
        generated: rs_path.display().to_string(),
        entries: file.entries.len(),
        update,
        updated,
        problems,
        pass,
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 04 M1 (§7b): settings set → flush → reload equality, corrupt file →
/// defaults, no panic. Uses NativeFs against `--data-dir` (a scratch dir).
fn cmd_io_settings(cli: &Cli, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::fs::NativeFs;
    use mind_core::io::settings::{
        KEY_LAST_SECTOR_SAVE, KEY_SAVE_INTERVAL, KEY_UI_SCALE, slot_autosave_key, slot_name_key,
    };
    use mind_core::io::{FileSystem, Paths, SettingsStore};

    let fs = NativeFs;
    let paths = Paths::resolve(cli.data_dir.as_deref());
    fs.mkdirs(&paths.config())?;
    // Deterministic start: the scenario owns this data-dir.
    let _ = fs.delete(&paths.settings_file());

    // Phase 1: set + flush.
    let mut store = SettingsStore::new();
    store.put_string(&slot_name_key("0"), "m1-base");
    store.put_bool(&slot_autosave_key("0"), false);
    store.put_i32(KEY_SAVE_INTERVAL, 7);
    store.put_i32(KEY_UI_SCALE, 150);
    store.put_string(KEY_LAST_SECTOR_SAVE, "sector-serpulo-12");
    store.put_json("controlGroups", &vec![vec![1i32, 2], vec![3]])?;
    store.force_save(&fs, &paths)?;

    // Phase 2: reload → equality.
    let reloaded = SettingsStore::load(&fs, &paths);
    let checks: Vec<(&str, bool)> = vec![
        (
            "slot-name",
            reloaded.get_string(&slot_name_key("0"), "?") == "m1-base",
        ),
        (
            "slot-autosave",
            !reloaded.get_bool(&slot_autosave_key("0"), true),
        ),
        ("saveinterval", reloaded.get_i32(KEY_SAVE_INTERVAL, 2) == 7),
        ("uiscale", reloaded.get_i32(KEY_UI_SCALE, 100) == 150),
        (
            "last-sector-save",
            reloaded.get_string(KEY_LAST_SECTOR_SAVE, "<none>") == "sector-serpulo-12",
        ),
        (
            "json",
            reloaded.get_json::<Vec<Vec<i32>>>("controlGroups")? == Some(vec![vec![1, 2], vec![3]]),
        ),
    ];
    let persisted = checks.iter().all(|(_, ok)| *ok);
    for (name, ok) in &checks {
        if !ok {
            log::error!("settings persistence check failed: {name}");
        }
    }

    // Phase 3: corrupt the file → defaults, no panic.
    fs.write(&paths.settings_file(), b"garbage-not-settings")?;
    let corrupt = SettingsStore::load(&fs, &paths);
    let corrupt_fallback = corrupt.is_empty() && corrupt.get_i32(KEY_SAVE_INTERVAL, 2) == 2;

    // Phase 4: the store recovers (rewrites a valid file).
    let mut recovered = corrupt;
    recovered.put_i32(KEY_SAVE_INTERVAL, 3);
    recovered.force_save(&fs, &paths)?;
    let recovered_ok = SettingsStore::load(&fs, &paths).get_i32(KEY_SAVE_INTERVAL, 2) == 3;

    let pass = persisted && corrupt_fallback && recovered_ok;
    let report = IoSettingsReport {
        data_dir: paths.root().display().to_string(),
        persisted,
        corrupt_fallback,
        recovered: recovered_ok,
        keys_checked: checks.iter().map(|(name, _)| (*name).to_owned()).collect(),
        pass,
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 04 M0: meta-only read of a save file written by the IO engine.
fn cmd_io_dump_meta(file: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::{NativeFs, SaveIo};

    let fs = NativeFs;
    let meta = SaveIo::get_meta(&fs, file)
        .with_context(|| format!("reading meta of `{}`", file.display()))?;
    let report = IoDumpMetaReport {
        file: file.display().to_string(),
        format_version: meta.version,
        build: meta.build,
        timestamp: meta.timestamp,
        time_played: meta.time_played,
        map_name: meta.map_name.clone(),
        wave: meta.wave,
        width: meta.width(),
        height: meta.height(),
        is_map: meta.is_map(),
        mods: meta.mods.clone(),
        tags: meta
            .tags
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "meta: format={} build={} map={} wave={} {}x{} is_map={} mods={}",
            report.format_version,
            report.build,
            report.map_name,
            report.wave,
            report.width,
            report.height,
            report.is_map,
            report.mods.len()
        );
    }
    Ok(EXIT_PASS)
}

/// Plan 05 M5: stable entity/component metadata JSON (`entitymeta.json`).
fn cmd_meta_entities(out: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let registry = mind_core::entities::vanilla_registry()
        .map_err(|error| anyhow!("building entity registry: {error}"))?;
    let value = registry.metadata_json();
    let text = format!("{}\n", serde_json::to_string_pretty(&value)?);
    if let Some(path) = out {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || out.is_none() {
        print!("{text}");
    } else if let Some(path) = out {
        log::info!(
            "meta entities: {} defs / {} components written to {}",
            registry.defs_len(),
            registry.components_len(),
            path.display()
        );
    }
    Ok(EXIT_PASS)
}

/// Plan 05 M6: deterministic schedule-order trace (golden `trace order`).
fn cmd_trace_order(ticks: u64, out: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    use mind_core::sim::schedule::{RunContext, render_trace, schedule_trace};

    let contexts = [
        ("playing", RunContext::playing_headless()),
        ("menu", RunContext::menu()),
        ("paused", RunContext::paused()),
        ("editor", RunContext::editor()),
        ("client", RunContext::client()),
    ];
    let text = render_trace(&contexts);
    if let Some(path) = out {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json {
        let rows: Vec<serde_json::Value> = contexts
            .iter()
            .map(|(label, ctx)| {
                let entries: Vec<serde_json::Value> = schedule_trace(*ctx)
                    .into_iter()
                    .map(|entry| serde_json::json!({"set": entry.set, "ran": entry.ran}))
                    .collect();
                serde_json::json!({"context": label, "sets": entries})
            })
            .collect();
        let report = serde_json::json!({
            "ticks": ticks,
            "tickSets": mind_core::sim::schedule::TICK_SETS.len(),
            "entitySets": mind_core::sim::schedule::ENTITY_SETS.len(),
            "contexts": rows,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if out.is_none() {
        print!("{text}");
    }
    Ok(EXIT_PASS)
}

fn load_scenario(cli: &Cli, name: &str) -> anyhow::Result<Scenario> {
    let fixture = registry::find(name)
        .ok_or_else(|| anyhow!("unknown scenario `{name}`; run `mind-headless list`"))?;
    let dir = paths::find_scenarios_dir(cli.scenarios_dir.as_deref())?;
    let path = dir.join(fixture.file_name());
    Scenario::read(&path).with_context(|| format!("loading scenario `{}`", path.display()))
}

/// Runs a scenario to completion; optionally collects per-tick checksums.
fn run_scenario(scenario: &Scenario, collect_ticks: bool) -> anyhow::Result<(Sim, Vec<String>)> {
    let mut sim = Sim::from_scenario(scenario)?;
    let mut player = ScenarioPlayer::new(scenario, sim.content())?;
    let mut per_tick = Vec::new();
    while player.step(&mut sim)? {
        if collect_ticks {
            per_tick.push(sim.checksum_hex());
        }
    }
    Ok((sim, per_tick))
}

fn checksums_equal(expected: &str, actual: &str) -> bool {
    expected.eq_ignore_ascii_case(actual)
}

fn write_dump(sim: &Sim, path: &Path, all_tiles: bool) -> anyhow::Result<()> {
    let json = sim.dump_json(all_tiles)?;
    // Schema validation: the dump must round-trip through the serde types (§6.3).
    let _decoded: StateDump = serde_json::from_str(&json)
        .with_context(|| format!("dump failed schema round-trip for `{}`", path.display()))?;
    std::fs::write(path, json).with_context(|| format!("writing dump `{}`", path.display()))
}

fn print_report<T: serde::Serialize>(report: &T, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!("{}", serde_json::to_string(report)?);
    }
    Ok(())
}

fn cmd_run(
    cli: &Cli,
    name: &str,
    dump: Option<&Path>,
    json: bool,
    emit_commands: Option<&Path>,
) -> anyhow::Result<i32> {
    // `stdb_*` scenarios (plan 01 §7.2) are connector tests, not sim scenarios:
    // they have their own fixture schema and never run `mind-core`.
    if let Some(kind) = StdbScenario::from_name(name) {
        if emit_commands.is_some() {
            log::warn!("--emit-commands is ignored for `{name}` (no sim commands)");
        }
        return crate::stdb_scenarios::run(cli, kind, dump, json);
    }
    let scenario = load_scenario(cli, name)?;
    let collect = scenario.emit_per_tick;

    // Two in-process runs: the determinism assertion of §7b.
    let (sim, per_tick) = run_scenario(&scenario, collect)?;
    let (second, second_per_tick) = run_scenario(&scenario, collect)?;
    let in_process_stable = sim.checksum() == second.checksum() && per_tick == second_per_tick;
    if !in_process_stable {
        log::error!("scenario `{name}` produced different checksums across two in-process runs");
    }

    let mut pass = in_process_stable;
    let mut tile_checks = Vec::new();
    if let Some(expect) = &scenario.expect {
        if let Some(expected) = &expect.checksum {
            let actual = sim.checksum_hex();
            if !checksums_equal(expected, &actual) {
                log::error!("checksum mismatch for `{name}`: expected {expected}, got {actual}");
                pass = false;
            }
        }
        for tile in &expect.tiles {
            let pos = TilePos::new(tile.x, tile.y);
            let actual = match sim.block_at(pos) {
                Some(block) => sim.block_name_of(block),
                None => String::from("out-of-bounds"),
            };
            let ok = actual == tile.block;
            if !ok {
                log::error!(
                    "tile ({}, {}) mismatch for `{name}`: expected {}, got {}",
                    tile.x,
                    tile.y,
                    tile.block,
                    actual
                );
                pass = false;
            }
            tile_checks.push(TileCheck {
                x: tile.x,
                y: tile.y,
                expect: tile.block.clone(),
                actual,
                ok,
            });
        }
    }

    if let Some(path) = dump {
        write_dump(&sim, path, false)?;
    }
    let commands_emitted = match emit_commands {
        Some(path) => {
            let records = scenario.resolve_commands(sim.content())?;
            write_command_log(sim.content(), path, &records)?;
            Some(path.display().to_string())
        }
        None => None,
    };

    let report = RunReport {
        scenario: scenario.name.clone(),
        format: scenario.format,
        seed: scenario.seed,
        steps: scenario.steps,
        tick: sim.tick_count(),
        checksum: sim.checksum_hex(),
        expect_checksum: scenario
            .expect
            .as_ref()
            .and_then(|expect| expect.checksum.clone()),
        pass,
        in_process_stable,
        commands_applied: sim.commands_applied(),
        tiles: tile_checks,
        per_tick: if collect { Some(per_tick) } else { None },
        commands_emitted,
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_sim(
    _cli: &Cli,
    ticks: u64,
    seed: u64,
    width: i32,
    height: i32,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    let mut sim = Sim::new(
        seed,
        width,
        height,
        mind_core::content::BlockId::AIR,
        mind_core::content::BlockId::AIR,
    );
    let mut per_tick = Vec::new();
    for _ in 0..ticks {
        sim.tick()?;
        if json {
            per_tick.push(sim.checksum_hex());
        }
    }
    if let Some(path) = dump {
        write_dump(&sim, path, false)?;
    }
    let report = SimReport {
        mode: String::from("sim"),
        seed,
        width,
        height,
        tick: sim.tick_count(),
        checksum: sim.checksum_hex(),
        commands_applied: sim.commands_applied(),
        per_tick: if json { Some(per_tick) } else { None },
    };
    print_report(&report, json)?;
    Ok(EXIT_PASS)
}

#[allow(clippy::too_many_arguments)]
fn cmd_replay(
    _cli: &Cli,
    commands: &Path,
    seed: u64,
    ticks: Option<u64>,
    width: i32,
    height: i32,
    dump: Option<&Path>,
    json: bool,
    per_tick: bool,
) -> anyhow::Result<i32> {
    let blocks = Blocks::new();
    let records: Vec<CommandRecord> = read_command_log(&blocks, commands)
        .with_context(|| format!("reading command log `{}`", commands.display()))?;
    let ticks = ticks.unwrap_or_else(|| {
        records
            .last()
            .map(|record| record.tick.saturating_add(1))
            .unwrap_or(0)
    });

    let mut sim = Sim::new(
        seed,
        width,
        height,
        mind_core::content::BlockId::AIR,
        mind_core::content::BlockId::AIR,
    );
    let mut next = 0usize;
    let mut checksums = Vec::new();
    for tick in 0..ticks {
        while next < records.len() && records[next].tick <= tick {
            sim.apply(records[next].command)?;
            next += 1;
        }
        sim.tick()?;
        if per_tick {
            checksums.push(sim.checksum_hex());
        }
    }
    if let Some(path) = dump {
        write_dump(&sim, path, false)?;
    }
    let report = SimReport {
        mode: String::from("replay"),
        seed,
        width,
        height,
        tick: sim.tick_count(),
        checksum: sim.checksum_hex(),
        commands_applied: sim.commands_applied(),
        per_tick: if per_tick { Some(checksums) } else { None },
    };
    print_report(&report, json)?;
    Ok(EXIT_PASS)
}

fn cmd_bench(cli: &Cli, ticks: u64, scenario_name: &str) -> anyhow::Result<i32> {
    if ticks == 0 {
        return Err(anyhow!("--ticks must be greater than zero"));
    }
    let ticks_usize = usize::try_from(ticks).context("--ticks does not fit in memory")?;
    // Plan 01 §7.4: STDB pump overhead is not a sim scenario.
    if scenario_name == "stdb_pump" {
        const P99_BUDGET_NS: u64 = 200_000;
        let (p50_ns, p99_ns) = crate::stdb_scenarios::bench_pump(ticks);
        let report = BenchReport {
            scenario: String::from("stdb_pump"),
            ticks,
            p50_ns,
            p99_ns,
            p50_us: p50_ns.div_ceil(1_000),
            p99_us: p99_ns.div_ceil(1_000),
            checksum: String::from("n/a"),
            baseline_status: if p99_ns <= P99_BUDGET_NS {
                String::from("ok")
            } else {
                String::from("fail")
            },
        };
        println!("{}", serde_json::to_string(&report)?);
        return Ok(if p99_ns <= P99_BUDGET_NS {
            EXIT_PASS
        } else {
            EXIT_FAIL
        });
    }
    let name = registry::bench_alias(scenario_name);
    let scenario = load_scenario(cli, name)?;

    // Build the steady state (e.g. 256 placed blocks) and warm up the executor.
    let (mut sim, _) = run_scenario(&scenario, false)?;
    const WARMUP: u64 = 1_000;
    for _ in 0..WARMUP {
        sim.tick()?;
    }

    let mut samples: Vec<u64> = Vec::with_capacity(ticks_usize);
    for _ in 0..ticks {
        let start = Instant::now();
        sim.tick()?;
        samples.push(start.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let p50_ns = percentile(&samples, 50);
    let p99_ns = percentile(&samples, 99);
    // Microseconds round up so sub-microsecond ticks are never recorded as 0.
    let p50_us = p50_ns.div_ceil(1_000);
    let p99_us = p99_ns.div_ceil(1_000);

    let baseline = scenario.expect.as_ref().and_then(|expect| expect.bench);
    let (baseline_status, failed) = match baseline {
        Some(base) if base.p50_us > 0 || base.p99_us > 0 => {
            let warn_p50 = base.p50_us * 1_000 * 120 / 100;
            let warn_p99 = base.p99_us * 1_000 * 120 / 100;
            let fail_p50 = base.p50_us * 1_000 * 150 / 100;
            let fail_p99 = base.p99_us * 1_000 * 150 / 100;
            if p50_ns > fail_p50 || p99_ns > fail_p99 {
                if p50_ns > fail_p50 {
                    log::error!(
                        "bench p50 {p50_us}us exceeds baseline {}us by >50%",
                        base.p50_us
                    );
                }
                if p99_ns > fail_p99 {
                    log::error!(
                        "bench p99 {p99_us}us exceeds baseline {}us by >50%",
                        base.p99_us
                    );
                }
                (String::from("fail"), true)
            } else if p50_ns > warn_p50 || p99_ns > warn_p99 {
                log::warn!(
                    "bench regression >20%: p50 {p50_us}us (base {}us), p99 {p99_us}us (base {}us)",
                    base.p50_us,
                    base.p99_us
                );
                (String::from("warn"), false)
            } else {
                (String::from("ok"), false)
            }
        }
        Some(_) => (String::from("no-baseline"), false),
        None => (String::from("no-baseline"), false),
    };

    let report = BenchReport {
        scenario: name.to_owned(),
        ticks,
        p50_ns,
        p99_ns,
        p50_us,
        p99_us,
        checksum: sim.checksum_hex(),
        baseline_status,
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(if failed { EXIT_FAIL } else { EXIT_PASS })
}

fn cmd_dump(cli: &Cli, scenario_name: &str, out: &Path, all_tiles: bool) -> anyhow::Result<i32> {
    let scenario = load_scenario(cli, scenario_name)?;
    let (sim, _) = run_scenario(&scenario, false)?;

    let start = Instant::now();
    let json = sim.dump_json(all_tiles)?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0;

    // Schema validation outside the timed section: the §7d budget covers the
    // checksum + dump construction/serialization only.
    let _decoded: StateDump = serde_json::from_str(&json)?;
    std::fs::write(out, &json).with_context(|| format!("writing dump `{}`", out.display()))?;

    println!(
        "dump: scenario={} ticks={} sparse={} bytes={} ms={elapsed_ms:.3}",
        scenario.name,
        sim.tick_count(),
        !all_tiles,
        json.len()
    );
    Ok(EXIT_PASS)
}

fn percentile(samples: &[u64], percent: usize) -> u64 {
    debug_assert!(!samples.is_empty());
    let index = (samples.len() * percent / 100).min(samples.len().saturating_sub(1));
    samples.get(index).copied().unwrap_or(0)
}

/// Sorts nanosecond samples and reports the P50/P95/min/max in milliseconds.
fn io_bench_stat(samples: &mut [u64]) -> IoBenchStat {
    samples.sort_unstable();
    let millis = |value: u64| value as f64 / 1_000_000.0;
    let last = samples.last().copied().unwrap_or(0);
    IoBenchStat {
        p50_ms: millis(percentile(samples, 50)),
        p95_ms: millis(percentile(samples, 95)),
        min_ms: millis(samples.first().copied().unwrap_or(0)),
        max_ms: millis(last),
    }
}

/// Boots base content (create + init + postInit + load), the `content` harness
/// path. Assets/bundle are in-memory; headless skips icon/region loading.
fn boot_content() -> anyhow::Result<ContentRegistry> {
    boot_content_with(None)
}

/// Boots base content with an optional bundle key map (localized names).
fn boot_content_with(keys: Option<&BTreeMap<String, String>>) -> anyhow::Result<ContentRegistry> {
    let bundle = match keys {
        Some(keys) => {
            MemoryBundle::with_pairs(keys.iter().map(|(key, value)| (key.clone(), value.clone())))
        }
        None => MemoryBundle::new(),
    };
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)?;
    registry.init()?;
    registry.post_init()?;
    registry.load()?;
    registry.log_content()?;
    Ok(registry)
}

/// Loads a `parity/bundle_keys.json` file, if the path exists.
fn load_bundle_keys(path: Option<&Path>) -> anyhow::Result<Option<BundleKeysFile>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading `{}`", path.display()))?;
    let file: BundleKeysFile = serde_json::from_str(&text)
        .with_context(|| format!("parsing bundle keys `{}`", path.display()))?;
    Ok(Some(file))
}

/// Loads an `asset_manifest.json` file, if the path exists.
fn load_manifest(path: Option<&Path>) -> anyhow::Result<Option<AssetManifest>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading `{}`", path.display()))?;
    let manifest: AssetManifest = serde_json::from_str(&text)
        .with_context(|| format!("parsing manifest `{}`", path.display()))?;
    Ok(Some(manifest))
}

fn type_counts(registry: &ContentRegistry) -> Vec<ContentTypeCount> {
    let counts = content_counts(registry);
    registry::LIVE_CONTENT_TYPES
        .iter()
        .map(|type_| ContentTypeCount {
            type_: *type_,
            count: counts.get(type_).copied().unwrap_or(0),
        })
        .collect()
}

/// Plan 02 §7b negative scenario: `Liquids` before `StatusEffects` must fail
/// with the missing-status error naming the load-order violation.
fn cmd_content_load_order_bad() -> anyhow::Result<i32> {
    match mind_core::content::registries::create_base_content_bad_order(
        &MemoryBundle::new(),
        &MemoryUnlockStore::new(),
        true,
    ) {
        Err(error) => {
            println!("content load-order-bad: failed as expected: {error}");
            Ok(EXIT_PASS)
        }
        Ok(_) => {
            log::error!("content load-order-bad: content loaded, expected a load-order error");
            Ok(EXIT_FAIL)
        }
    }
}

/// `content dump`: writes the parity golden snapshot (plan 02 §6.2/§7b).
fn cmd_content_dump(out: Option<&Path>, bundle: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let bundle_file = load_bundle_keys(bundle)?;
    let registry = boot_content_with(bundle_file.as_ref().map(|file| &file.keys))?;
    let golden = dump_golden(&registry);
    let text = format!("{}\n", serde_json::to_string_pretty(&golden)?);
    if let Some(path) = out {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || out.is_none() {
        print!("{text}");
    } else if let Some(path) = out {
        log::info!("content dump written to {}", path.display());
    }
    Ok(EXIT_PASS)
}

/// `content audit`: mechanical parity gate (plan 02 §7b/§7e).
fn cmd_content_audit(
    golden: Option<&Path>,
    bundle: Option<&Path>,
    manifest: Option<&Path>,
    out: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    let golden_path = golden.unwrap_or_else(|| Path::new("parity/golden_content.json"));
    let text = std::fs::read_to_string(golden_path).with_context(|| {
        format!(
            "reading golden `{}` (run `content dump --out` to create it)",
            golden_path.display()
        )
    })?;
    let golden: GoldenContent = serde_json::from_str(&text)
        .with_context(|| format!("parsing golden `{}`", golden_path.display()))?;

    let bundle_path = bundle.unwrap_or_else(|| Path::new("parity/bundle_keys.json"));
    let manifest_path = manifest.unwrap_or_else(|| Path::new("parity/asset_manifest.json"));
    let bundle_file = load_bundle_keys(Some(bundle_path))?;
    let manifest = load_manifest(Some(manifest_path))?;

    let registry = boot_content_with(bundle_file.as_ref().map(|file| &file.keys))?;
    let report = audit(&registry, &golden, bundle_file.as_ref(), manifest.as_ref());

    if let Some(path) = out {
        let mut markdown = String::new();
        markdown.push_str("# Content audit report\n\n");
        markdown.push_str(&format!("- Golden: `{}`\n", golden_path.display()));
        markdown.push_str(&format!(
            "- Status: **{}**\n\n",
            if report.pass() { "PASS" } else { "FAIL" }
        ));
        markdown.push_str("| check | status | detail |\n|---|---|---|\n");
        for check in &report.checks {
            markdown.push_str(&format!(
                "| {} | {} | {} |\n",
                check.name, check.status, check.detail
            ));
        }
        if !report.errors.is_empty() {
            markdown.push_str("\n## Errors\n\n");
            for error in &report.errors {
                markdown.push_str(&format!("- {error}\n"));
            }
        }
        if !report.warnings.is_empty() {
            markdown.push_str("\n## Warnings\n\n");
            for warning in &report.warnings {
                markdown.push_str(&format!("- {warning}\n"));
            }
        }
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, markdown).with_context(|| format!("writing `{}`", path.display()))?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for check in &report.checks {
            println!("{:>16}: {} ({})", check.name, check.status, check.detail);
        }
        println!(
            "content audit: {} errors, {} warnings",
            report.errors.len(),
            report.warnings.len()
        );
        for error in report.errors.iter().take(10) {
            println!("  error: {error}");
        }
    }
    Ok(if report.pass() { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_content_load(json: bool) -> anyhow::Result<i32> {
    let registry = boot_content()?;
    let types = type_counts(&registry);
    let total: usize = types.iter().map(|entry| entry.count).sum();
    if json {
        let report = ContentLoadReport { types, total };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for entry in &types {
            println!("{:>12}: {}", entry.type_.name(), entry.count);
        }
        println!("       total: {total}");
    }
    Ok(EXIT_PASS)
}

fn cmd_content_ids(json: bool, out: Option<&Path>) -> anyhow::Result<i32> {
    let registry = boot_content()?;
    let types: Vec<ContentTypeEntries> = registry::LIVE_CONTENT_TYPES
        .iter()
        .map(|type_| ContentTypeEntries {
            type_: *type_,
            entries: registry
                .entries(*type_)
                .into_iter()
                .map(|entry| ContentIdEntry {
                    id: entry.id,
                    name: entry.name.map(str::to_owned),
                    kind: entry.kind.to_owned(),
                })
                .collect(),
        })
        .collect();
    let report = ContentIdsReport { format: 1, types };
    let text = format!("{}\n", serde_json::to_string_pretty(&report)?);
    if let Some(path) = out {
        // Schema round-trip before writing (same rule as state dumps).
        let _decoded: serde_json::Value = serde_json::from_str(&text).with_context(|| {
            format!(
                "content ids failed schema round-trip for `{}`",
                path.display()
            )
        })?;
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json {
        print!("{text}");
    } else {
        for entry in &report.types {
            println!("{:>12}: {}", entry.type_.name(), entry.entries.len());
        }
        if out.is_none() {
            log::info!("pass --out <path> to write the deterministic content_ids.json dump");
        }
    }
    Ok(EXIT_PASS)
}

fn cmd_content_bench(runs: usize, json: bool) -> anyhow::Result<i32> {
    if runs == 0 {
        return Err(anyhow!("--runs must be greater than zero"));
    }
    let mut samples_ns: Vec<u64> = Vec::with_capacity(runs);
    let mut last: Option<ContentRegistry> = None;
    for _ in 0..runs {
        let start = Instant::now();
        let registry = boot_content()?;
        samples_ns.push(start.elapsed().as_nanos() as u64);
        last = Some(registry);
    }
    let types = last.as_ref().map(type_counts).unwrap_or_default();
    samples_ns.sort_unstable();
    let pick = |index: usize| samples_ns.get(index).copied().unwrap_or(0);
    let median_ns = {
        let index = samples_ns.len() / 2;
        pick(index)
    };
    let min_ns = pick(0);
    let max_ns = pick(samples_ns.len().saturating_sub(1));
    let to_ms = |ns: u64| ns as f64 / 1_000_000.0;
    let median_ms = to_ms(median_ns);
    let within_budget = median_ms <= 200.0;
    if !within_budget {
        log::warn!(
            "content load median {median_ms:.1}ms exceeds the 200ms budget (release budget; debug builds are slower)"
        );
    }
    let report = ContentBenchReport {
        runs,
        median_ms,
        min_ms: to_ms(min_ns),
        max_ms: to_ms(max_ns),
        within_budget,
        types,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "content bench: runs={} median={:.2}ms min={:.2}ms max={:.2}ms within_budget={}",
            report.runs, report.median_ms, report.min_ms, report.max_ms, report.within_budget
        );
    }
    Ok(EXIT_PASS)
}
