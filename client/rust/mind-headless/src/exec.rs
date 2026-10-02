// SPDX-License-Identifier: GPL-3.0-only

//! Command implementations.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, anyhow};
use log::LevelFilter;
use mind_core::config::MindConfig;
use mind_core::content::{
    AssetManifest, Blocks, BundleKeysFile, ContentRegistry, GoldenContent, MemoryBundle,
    MemoryUnlockStore, audit, content_counts, create_base_content, dump_golden,
};
use mind_core::determinism::{CommandError, CommandLog, LogHeader, SimCommand};
use mind_core::scenario::{Scenario, ScenarioPlayer, write_command_log};
use mind_core::sim::{Sim, StateDump};
use mind_core::util::alloc::{alloc_count, enabled as alloc_audit_enabled};
use mind_core::world::TilePos;

use crate::cli::{
    AssetsCommand, Cli, Command, ContentCommand, IoCommand, MapsCommand, MetaCommand, ModsCommand,
    TraceCommand, WorldCommand,
};
use crate::parity::scenario::ScenarioCatalog;
use crate::paths;
use crate::registry;
use crate::report::{
    BenchReport, ContentBenchReport, ContentIdEntry, ContentIdsReport, ContentLoadReport,
    ContentTypeCount, ContentTypeEntries, GroupCount, IoBenchSaveReport, IoBenchStat,
    IoCheckClassIdsReport, IoCheckRevisionsReport, IoDefRevisionReport, IoDumpMetaReport,
    IoMapListEntry, IoMapListReport, IoRoundtripReport, IoSettingsReport, RunReport,
    SimCoreCycleReport, SimCoreProfileReport, SimCoreReplayReport, SimReport, TileCheck,
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
        Command::List { json, tier } => cmd_list(*json, tier.as_deref()),
        Command::RunAll { tier } => cmd_run_all(&cli, tier.as_deref()),
        Command::Version { json, file } => cmd_version(*json, file.as_deref()),
        Command::Server {
            config_dir,
            commands,
            socket_port,
            stdb,
            boot_timing_json,
            profile_json,
        } => Ok(crate::server::run(crate::server::ServerOptions {
            config_dir: config_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("config")),
            commands: commands.clone(),
            socket_port: *socket_port,
            offline: stdb.eq_ignore_ascii_case("offline"),
            boot_timing_json: boot_timing_json.clone(),
            profile_json: profile_json.clone(),
        })),
        Command::Run {
            scenario,
            dump,
            json,
            emit_commands,
            emit_simlog,
            cycles,
            checksum_every,
            golden,
            emit_checksums,
        } => cmd_run(
            &cli,
            scenario,
            dump.as_deref(),
            *json,
            emit_commands.as_deref(),
            emit_simlog.as_deref(),
            *cycles,
            *checksum_every,
            golden.as_deref(),
            emit_checksums.as_deref(),
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
            checksum_every,
            workers,
            golden,
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
            *checksum_every,
            *workers,
            golden.as_deref(),
        ),
        Command::Bench {
            ticks,
            scenario,
            profile,
            assert_alloc,
        } => cmd_bench(&cli, *ticks, scenario, profile.as_deref(), *assert_alloc),
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
            AssetsCommand::BundleDiff { dir, json } => cmd_assets_bundle_diff(dir, *json),
            AssetsCommand::SoundsCheck { root, json } => {
                cmd_assets_sounds_check(root.as_deref(), *json)
            }
            AssetsCommand::FallbackBoot { atlas, json } => cmd_assets_fallback_boot(atlas, *json),
        },
        Command::Meta { command } => match command {
            MetaCommand::Entities { out, json } => cmd_meta_entities(out.as_deref(), *json),
        },
        Command::Trace { command } => match command {
            TraceCommand::Order { ticks, out, json } => {
                cmd_trace_order(*ticks, out.as_deref(), *json)
            }
        },
        Command::World { command } => match command {
            WorldCommand::TileOps {
                seed,
                width,
                height,
                ops,
                dump,
                json,
            } => cmd_world_tile_ops(*seed, *width, *height, *ops, dump.as_deref(), *json),
            WorldCommand::Gen {
                generator,
                planet,
                sector,
                seed,
                width,
                height,
                iters,
                dump,
                json,
            } => cmd_world_gen(
                generator,
                planet.as_deref(),
                *sector,
                *seed,
                *width,
                *height,
                *iters,
                dump.as_deref(),
                *json,
            ),
            WorldCommand::BenchGen {
                generator,
                seed,
                width,
                height,
                iters,
                json,
            } => cmd_world_bench_gen(generator, *seed, *width, *height, *iters, *json),
            WorldCommand::Filters {
                seed,
                width,
                height,
                stack,
                order,
                dump,
                json,
            } => cmd_world_filters(*seed, *width, *height, stack, order, dump.as_deref(), *json),
            WorldCommand::Multiblock {
                size,
                block,
                dump,
                json,
            } => cmd_world_multiblock(*size, block, dump.as_deref(), *json),
        },
        Command::Maps { command } => match command {
            MapsCommand::List { dir, json } => cmd_maps_list(dir, *json),
        },
        Command::Blocks { command } => crate::blocks_scenarios::run(command).map(|()| EXIT_PASS),
        Command::Combat { command } => crate::combat_scenarios::run(command),
        Command::Units { command } => crate::units_scenarios::run(command),
        Command::Logic { command } => crate::logic_scenarios::run(command),
        Command::Render { command } => crate::render_scenarios::run(command),
        Command::Campaign { command } => crate::campaign_scenarios::run(command),
        Command::Fx { command } => crate::fx_scenarios::run(command),
        Command::Power { command } => {
            crate::network_scenarios::run(crate::network_scenarios::NetworkKind::Power, command)
        }
        Command::Liquid { command } => {
            crate::network_scenarios::run(crate::network_scenarios::NetworkKind::Liquid, command)
        }
        Command::Heat { command } => {
            crate::network_scenarios::run(crate::network_scenarios::NetworkKind::Heat, command)
        }
        Command::Ui { command } => crate::ui_scenarios::run(command).map(|()| EXIT_PASS),
        Command::Parity { command } => crate::parity::run(command),
        Command::Audio { command } => crate::audio_scenarios::run(command),
        Command::Mods { command } => match command {
            ModsCommand::List { dir, json, check } => cmd_mods_list(dir, *json, *check),
            ModsCommand::Content {
                fixture,
                repo,
                dump,
                json,
            } => cmd_mods_content(fixture, repo.as_deref(), dump.as_deref(), *json),
            ModsCommand::Overlay {
                fixture,
                repo,
                probe,
                json,
            } => cmd_mods_overlay(fixture, repo.as_deref(), probe, *json),
            ModsCommand::Bench {
                scene,
                dir,
                fixture,
                repo,
                runs,
                json,
            } => cmd_mods_bench(
                scene,
                dir,
                fixture.as_deref(),
                repo.as_deref(),
                *runs,
                *json,
            ),
            ModsCommand::Patch {
                fixture,
                repo,
                json,
            } => cmd_mods_patch(fixture, repo.as_deref(), *json),
            ModsCommand::Assets {
                fixture,
                repo,
                json,
            } => cmd_mods_assets(fixture, repo.as_deref(), *json),
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

    // M6 runtime half: resolve every `@Load` block field through the derive
    // macro and fold misses into a `RegionAudit`.
    //
    // Arc `find(name)` with no explicit `fallback` returns the `error` region
    // (`found() == false`) rather than failing, so several vanilla `@Load`
    // fields (e.g. `Conveyor.regions[5..7]`, `StaticWall.large` for walls with
    // no large sprite) are legitimately absent. The port keeps the strict
    // `fallback=error` audit *for regions the pack promises*: a default miss is
    // fatal only when the resolved name is in the generated inventory.
    let mut registry = boot_content()?;
    let audit = registry.load_regions(&index);
    let expected: std::collections::BTreeSet<&str> =
        inventory.regions.iter().map(String::as_str).collect();
    let runtime_errors: Vec<&str> = audit
        .errors
        .iter()
        .map(String::as_str)
        .filter(|name| expected.contains(name))
        .collect();
    let upstream_not_found = audit.errors.len() - runtime_errors.len();

    if json {
        let report = serde_json::json!({
            "atlas": manifest_path.display().to_string(),
            "inventory": inventory_path.display().to_string(),
            "expected": inventory.regions.len(),
            "resolved": inventory.regions.len() - missing.len(),
            "missing": missing,
            "missingInSources": inventory.missing_in_sources,
            "loadRegions": {
                "fatal": runtime_errors,
                "explicitFallbacks": audit.explicit_fallbacks,
                "upstreamNotFound": upstream_not_found,
            },
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets regions: {}/{} expected regions resolve ({} missing)",
            inventory.regions.len() - missing.len(),
            inventory.regions.len(),
            missing.len()
        );
        println!(
            "assets regions: @Load audit {} fallback=error miss(es), {} explicit fallback(s), {} upstream not-found",
            runtime_errors.len(),
            audit.explicit_fallbacks.len(),
            upstream_not_found
        );
        for name in &missing {
            log::error!("assets regions: missing region `{name}`");
        }
        for name in &runtime_errors {
            log::error!("assets regions: @Load fallback=error miss `{name}`");
        }
    }

    let inventory_ok =
        inventory.missing_in_sources.is_empty() && (missing.is_empty() || !assert_complete);
    if inventory_ok && runtime_errors.is_empty() {
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

/// Plan 03 M7 `assets bundle-diff`: no locale may contain keys absent from
/// English; reports per-locale missing keys and confirms the `global.properties`
/// overlay. Locales are allowed to be partial (report only).
fn cmd_assets_bundle_diff(dir: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::assets::bundle::parse_properties;

    let base_path = dir.join("bundle.properties");
    let base_text = std::fs::read_to_string(&base_path)
        .with_context(|| format!("reading {}", base_path.display()))?;
    let base = parse_properties(&base_text);
    let base_keys: std::collections::BTreeSet<String> = base.keys().cloned().collect();

    let mut locales: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
    {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(locale) = stem.strip_prefix("bundle_") else {
            continue;
        };
        if path.extension().is_some_and(|ext| ext == "properties") {
            locales.push((locale.to_owned(), path));
        }
    }
    locales.sort_by(|a, b| a.0.cmp(&b.0));

    let mut errors = 0usize;
    let mut reports = Vec::new();
    for (locale, path) in &locales {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let properties = parse_properties(&text);
        let extra: Vec<&str> = properties
            .keys()
            .filter(|key| !base_keys.contains(key.as_str()))
            .map(String::as_str)
            .collect();
        let missing: Vec<&str> = base_keys
            .iter()
            .filter(|key| !properties.contains_key(key.as_str()))
            .map(String::as_str)
            .collect();
        if !extra.is_empty() {
            errors += 1;
            log::error!(
                "assets bundle-diff: locale `{locale}` has {} key(s) absent from English",
                extra.len()
            );
        }
        reports.push(serde_json::json!({
            "locale": locale,
            "keys": properties.len(),
            "extra": extra,
            "missing": missing,
        }));
    }

    let global_path = dir.join("global.properties");
    let global_keys = if global_path.is_file() {
        let text = std::fs::read_to_string(&global_path)
            .with_context(|| format!("reading {}", global_path.display()))?;
        parse_properties(&text).len()
    } else {
        0
    };

    if json {
        let report = serde_json::json!({
            "dir": dir.display().to_string(),
            "englishKeys": base_keys.len(),
            "globalKeys": global_keys,
            "locales": reports,
            "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets bundle-diff: {} english keys, {} locales, {} global keys, {} error locale(s)",
            base_keys.len(),
            locales.len(),
            global_keys,
            errors
        );
    }
    Ok(if errors == 0 { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 03 M8/M10 `assets sounds-check`: the registry equals the recursive
/// sound file listing, ids are dense/append-only and `none`/`unset` are present.
fn cmd_assets_sounds_check(root: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    use mind_core::assets::sounds::Sounds;

    let root = paths::find_repo_root(root)?;
    let index_path = root.join("assets/sounds.index.json");
    let text = std::fs::read_to_string(&index_path)
        .with_context(|| format!("reading {}", index_path.display()))?;
    let sounds = Sounds::from_index_json(&text).map_err(|error| anyhow!("{error}"))?;

    let mut files = Vec::new();
    collect_audio(&root.join("assets/sounds"), &mut files)?;
    let file_count = files.len();

    let mut missing_files = Vec::new();
    let mut ids = Vec::new();
    for entry in sounds.entries().iter().filter(|entry| !entry.is_dummy()) {
        ids.push(entry.id);
        let Some(file) = &entry.file else {
            missing_files.push(entry.name.clone());
            continue;
        };
        if !root.join("assets").join(file).is_file() {
            missing_files.push(entry.name.clone());
        }
    }
    ids.sort_unstable();
    let dense = ids.iter().enumerate().all(|(i, id)| *id == i as i32);
    let count_matches = ids.len() == file_count;
    let has_none = sounds.get("none").is_some_and(|entry| entry.is_dummy());
    let has_unset = sounds.get("unset").is_some_and(|entry| entry.is_dummy());

    let pass = dense && count_matches && missing_files.is_empty() && has_none && has_unset;
    if json {
        let report = serde_json::json!({
            "index": index_path.display().to_string(),
            "entries": sounds.len(),
            "realSounds": ids.len(),
            "files": file_count,
            "denseIds": dense,
            "countMatches": count_matches,
            "noneDummy": has_none,
            "unsetDummy": has_unset,
            "missingFiles": missing_files,
            "pass": pass,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets sounds-check: {} entries, {} files, dense={dense}, countMatch={count_matches}, missing={} -> {}",
            sounds.len(),
            file_count,
            missing_files.len(),
            if pass { "PASS" } else { "FAIL" }
        );
        for name in &missing_files {
            log::error!("assets sounds-check: missing file for sound `{name}`");
        }
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Recursively collects `*.ogg`/`*.mp3` paths under `dir` (sorted).
fn collect_audio(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_audio(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "ogg" || ext == "mp3")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Plan 03 M4/M10 `assets fallback-boot`: the 2048 fallback atlas parses and
/// every page & region fits within 2048.
fn cmd_assets_fallback_boot(atlas: &Path, json: bool) -> anyhow::Result<i32> {
    let manifest_path = atlas.join("sprites.atlas.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let index = mind_core::assets::atlas::AtlasIndex::from_manifest_json(&text)
        .map_err(|error| anyhow!("{error}"))?;

    let oversized_pages: Vec<&str> = index
        .pages()
        .iter()
        .filter(|page| page.width > 2048 || page.height > 2048)
        .map(|page| page.file.as_str())
        .collect();
    let oversized_regions: Vec<&str> = index
        .regions()
        .iter()
        .filter(|region| region.w > 2048 || region.h > 2048)
        .map(|region| region.name.as_str())
        .collect();
    let pass = index.fallback
        && !index.pages().is_empty()
        && oversized_pages.is_empty()
        && oversized_regions.is_empty();

    if json {
        let report = serde_json::json!({
            "atlas": manifest_path.display().to_string(),
            "fallback": index.fallback,
            "pages": index.pages().len(),
            "regions": index.len(),
            "oversizedPages": oversized_pages,
            "oversizedRegions": oversized_regions,
            "pass": pass,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets fallback-boot: fallback={} pages={} regions={} -> {}",
            index.fallback,
            index.pages().len(),
            index.len(),
            if pass { "PASS" } else { "FAIL" }
        );
        for name in &oversized_pages {
            log::error!("assets fallback-boot: oversized page `{name}`");
        }
        for name in &oversized_regions {
            log::error!("assets fallback-boot: oversized region `{name}`");
        }
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
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

/// Plan 20 M0 (`mods list`): discover fixture/server mods and resolve states.
fn cmd_mods_list(dir: &Path, json: bool, check: bool) -> anyhow::Result<i32> {
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;

    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        paths::find_repo_root(None)?.join(dir)
    };
    let fs = NativeFs;
    let settings = SettingsStore::new();
    let mut mods = Mods::new(true, &dir);
    let report = mods
        .load(&fs, &dir, &settings)
        .with_context(|| format!("loading mods from `{}`", dir.display()))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for entry in &report.mods {
            println!(
                "{}: {:?} enabled={} v{} source={}",
                entry.name, entry.state, entry.enabled, entry.version, entry.source
            );
        }
        println!("{} mod(s) discovered", report.mods.len());
    }

    if !check {
        return Ok(EXIT_PASS);
    }

    let expected_path = dir.join("expected_list.json");
    let text = std::fs::read_to_string(&expected_path)
        .with_context(|| format!("reading `{}`", expected_path.display()))?;
    let expected: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing `{}`", expected_path.display()))?;
    let actual = normalized_mod_list(&serde_json::to_value(&report)?);
    let expected_mods = expected
        .get("mods")
        .cloned()
        .unwrap_or(serde_json::Value::Array(Vec::new()));
    if expected_mods != actual {
        log::error!("mods list mismatch");
        log::error!("expected: {expected_mods}");
        log::error!("actual:   {actual}");
        return Ok(EXIT_FAIL);
    }
    Ok(EXIT_PASS)
}

/// Plan 20 M1 (`mods content`): boot base content + one fixture mod's JSON
/// content and dump the resulting mod content records.
fn cmd_mods_content(
    fixture: &str,
    repo: Option<&Path>,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::{Mods, provider::ModsContentProvider};

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let settings = SettingsStore::new();
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &settings)
        .with_context(|| format!("loading fixture `{fixture}`"))?;
    let files = mods.collect_content_files(&fs);
    let mut provider = ModsContentProvider::new(files);

    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow!("base content: {error}"))?;
    registry
        .init()
        .map_err(|error| anyhow!("content init: {error}"))?;
    registry
        .post_init()
        .map_err(|error| anyhow!("content post-init: {error}"))?;
    let content_result = registry.create_mod_content(&mut provider);
    let errors: Vec<String> = match &content_result {
        Ok(()) => Vec::new(),
        Err(errors) => errors.iter().map(|error| error.to_string()).collect(),
    };

    let items: Vec<serde_json::Value> = registry
        .items()
        .iter()
        .filter(|item| item.minfo.is_modded())
        .map(|item| serde_json::json!({"name": item.name, "id": item.id.raw()}))
        .collect();
    let blocks: Vec<serde_json::Value> = registry
        .blocks()
        .iter()
        .filter(|block| block.minfo.is_modded())
        .map(|block| {
            serde_json::json!({
                "name": block.name,
                "id": block.id.raw(),
                "kind": block.kind.name(),
            })
        })
        .collect();
    let liquids: Vec<serde_json::Value> = registry
        .liquids()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "gas": record.gas}))
        .collect();
    let statuses: Vec<serde_json::Value> = registry
        .statuses()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw()}))
        .collect();
    let units: Vec<serde_json::Value> = registry
        .units()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| {
            serde_json::json!({
                "name": record.name,
                "id": record.id.raw(),
                "kind": record.kind.name(),
                "weapons": record.weapons.len(),
            })
        })
        .collect();
    let weathers: Vec<serde_json::Value> = registry
        .weathers()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "kind": record.kind.name()}))
        .collect();
    let planets: Vec<serde_json::Value> = registry
        .planets()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| {
            serde_json::json!({
                "name": record.name,
                "id": record.id.raw(),
                "radius": record.radius,
                "sectors": record.sector_count,
            })
        })
        .collect();
    let sectors: Vec<serde_json::Value> = registry
        .sectors()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "planet": record.planet.raw()}))
        .collect();
    let teams: Vec<serde_json::Value> = registry
        .teams()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "team": record.team}))
        .collect();
    let warnings: Vec<String> = provider
        .parser()
        .warnings
        .iter()
        .map(|warning| format!("{}: {}", warning.file, warning.message))
        .collect();

    let report = serde_json::json!({
        "fixture": fixture,
        "items": items,
        "blocks": blocks,
        "liquids": liquids,
        "statuses": statuses,
        "units": units,
        "weathers": weathers,
        "planets": planets,
        "sectors": sectors,
        "teams": teams,
        "warnings": warnings,
        "errors": errors,
    });
    let text = format!("{}\n", serde_json::to_string_pretty(&report)?);
    if let Some(path) = dump {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        print!("{text}");
    }
    if errors.is_empty() {
        Ok(EXIT_PASS)
    } else {
        log::error!("mods content: {} content error(s)", errors.len());
        Ok(EXIT_FAIL)
    }
}

/// Plan 20 M3 (`mods patch`): apply a fixture mod's patches and assert that
/// `unapply` restores the baseline field values.
fn cmd_mods_patch(fixture: &str, repo: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    use mind_core::content::parser_hooks::PatchAsset;
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::FileSystem;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;
    use mind_core::mods::patch::DataPatcher;

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &mind_core::io::SettingsStore::new())
        .with_context(|| format!("loading fixture `{fixture}`"))?;
    let patch_dir = dir.join("patches");
    let mut patches = Vec::new();
    if let Ok(files) = fs.walk(&patch_dir) {
        let mut files: Vec<_> = files
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        for file in files {
            let text = fs
                .read(&file)
                .with_context(|| format!("reading `{}`", file.display()))?;
            let text = String::from_utf8(text)
                .map_err(|_| anyhow!("`{}` is not UTF-8", file.display()))?;
            patches.push(PatchAsset {
                name: file.display().to_string(),
                json: text,
            });
        }
    }

    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow!("base content: {error}"))?;
    registry
        .init()
        .map_err(|error| anyhow!("content init: {error}"))?;
    let baseline_health = registry
        .block_id("router")
        .and_then(|id| registry.block(id))
        .map(|block| block.health)
        .unwrap_or(0);

    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &patches)
        .map_err(|error| anyhow!("apply: {error}"))?;
    let patched_health = registry
        .block_id("router")
        .and_then(|id| registry.block(id))
        .map(|block| block.health)
        .unwrap_or(0);
    let applied = patcher.is_applied();
    patcher.unapply(&mut registry);
    let restored_health = registry
        .block_id("router")
        .and_then(|id| registry.block(id))
        .map(|block| block.health)
        .unwrap_or(0);

    let pass = applied && restored_health == baseline_health && patched_health != baseline_health;
    let report = serde_json::json!({
        "fixture": fixture,
        "patches": patches.len(),
        "baselineHealth": baseline_health,
        "patchedHealth": patched_health,
        "restoredHealth": restored_health,
        "warnings": patcher.warnings(),
        "pass": pass,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods patch: {} patch(es), router health {} -> {} -> {}: {}",
            report["patches"],
            baseline_health,
            patched_health,
            restored_health,
            if pass { "PASS" } else { "FAIL" }
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 20 M4 (`mods assets`): load a fixture mod's data assets headlessly and
/// report `dp-` image names, sound ids, bundle merges and external assets.
fn cmd_mods_assets(fixture: &str, repo: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;
    use mind_core::mods::assets::{DataAsset, DataAssetType, DataAssets, ModDataManager};

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &mind_core::io::SettingsStore::new())
        .with_context(|| format!("loading fixture `{fixture}`"))?;

    let mut assets = Vec::new();
    for file in mods.collect_content_files(&fs) {
        assets.push(DataAsset::content(file.path, file.type_, file.json));
    }
    let listed: Vec<(DataAssetType, String)> = {
        let mod_ = mods
            .mod_at(0)
            .ok_or_else(|| anyhow!("fixture `{fixture}` has no mod record"))?;
        let mut out = Vec::new();
        for (folder, ty) in [
            ("patches", DataAssetType::Patch),
            ("bundles", DataAssetType::Bundle),
            ("sprites", DataAssetType::Image),
            ("sounds", DataAssetType::Sound),
            ("music", DataAssetType::Music),
        ] {
            if let Ok(files) = mod_.root.walk(&fs, folder) {
                for path in files {
                    out.push((ty, path));
                }
            }
        }
        out
    };
    {
        let mod_ = mods
            .mod_at(0)
            .ok_or_else(|| anyhow!("fixture `{fixture}` has no mod record"))?;
        for (ty, path) in listed {
            match ty {
                DataAssetType::Patch => {
                    if let Ok(text) = mod_.root.read_to_string(&fs, &path) {
                        assets.push(DataAsset::patch(path, text));
                    }
                }
                DataAssetType::Bundle => {
                    if let Ok(text) = mod_.root.read_to_string(&fs, &path) {
                        assets.push(DataAsset::bundle(path, &text));
                    }
                }
                _ => {
                    if let Ok(bytes) = mod_.root.read(&fs, &path) {
                        assets.push(DataAsset::blob(path, ty, bytes, false));
                    }
                }
            }
        }
    }

    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow!("base content: {error}"))?;
    registry
        .init()
        .map_err(|error| anyhow!("content init: {error}"))?;

    let mut manager = ModDataManager::new();
    manager
        .load(assets, &mut registry)
        .map_err(|error| anyhow!("asset load: {error}"))?;
    manager.regenerate_content_sprites(false);

    let images: Vec<serde_json::Value> = manager
        .image_applier()
        .entries()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "page": entry.page.name(),
                "generated": entry.generated,
            })
        })
        .collect();
    let audio: Vec<serde_json::Value> = manager
        .audio_applier()
        .entries()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "id": entry.id,
                "streaming": entry.streaming,
                "music": entry.music,
            })
        })
        .collect();
    let external: Vec<&str> = manager
        .ordered_external_assets()
        .iter()
        .map(|asset| asset.path.as_str())
        .collect();
    let missing: Vec<&str> = manager
        .get_missing_assets()
        .iter()
        .map(|asset| asset.path.as_str())
        .collect();
    let bundles: Vec<&str> = manager.bundle_applier().files().collect();

    let report = serde_json::json!({
        "fixture": fixture,
        "assets": manager.all_assets().len(),
        "external": external,
        "missing": missing,
        "bundles": bundles,
        "images": images,
        "audio": audio,
        "logicVars": manager.audio_applier().logic_vars(),
        "contentErrors": manager.content_errors(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods assets: {} assets ({} external), {} images, {} audio, {} bundle(s): PASS",
            report["assets"],
            external.len(),
            images.len(),
            audio.len(),
            bundles.len()
        );
    }
    Ok(EXIT_PASS)
}

/// Plan 20 M9 (`mods bench`): times one pipeline scene (§7d).
fn cmd_mods_bench(
    scene: &str,
    dir: &Path,
    fixture: Option<&str>,
    repo: Option<&Path>,
    runs: usize,
    json: bool,
) -> anyhow::Result<i32> {
    if runs == 0 {
        return Err(anyhow!("mods bench --runs must be greater than 0"));
    }
    let mut samples = Vec::with_capacity(runs);
    let mut detail = serde_json::Map::new();

    match scene {
        "discover" => {
            use mind_core::io::SettingsStore;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::Mods;
            let dir = if dir.is_absolute() {
                dir.to_path_buf()
            } else {
                paths::find_repo_root(None)?.join(dir)
            };
            let fs = NativeFs;
            let settings = SettingsStore::new();
            let mut mod_count = 0;
            for _ in 0..runs {
                let mut mods = Mods::new(true, &dir);
                let start = Instant::now();
                let report = mods
                    .load(&fs, &dir, &settings)
                    .with_context(|| format!("loading mods from `{}`", dir.display()))?;
                samples.push(start.elapsed().as_nanos() as u64);
                mod_count = report.mods.len();
            }
            detail.insert("mods".into(), serde_json::json!(mod_count));
        }
        "parse" => {
            use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
            use mind_core::io::SettingsStore;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::{Mods, provider::ModsContentProvider};
            let root = paths::find_repo_root(repo)?;
            let fixture = fixture.ok_or_else(|| anyhow!("bench parse needs --fixture"))?;
            let dir = root.join("parity/mod_fixtures").join(fixture);
            let fs = NativeFs;
            let mut mods = Mods::new(true, &dir);
            mods.load_single(&fs, &dir, &SettingsStore::new())
                .with_context(|| format!("loading fixture `{fixture}`"))?;
            let files = mods.collect_content_files(&fs);
            let file_count = files.len();
            for _ in 0..runs {
                let mut provider = ModsContentProvider::new(files.clone());
                let bundle = MemoryBundle::new();
                let store = MemoryUnlockStore::new();
                let mut registry = create_base_content(&bundle, &store, true)
                    .map_err(|error| anyhow!("base content: {error}"))?;
                registry.init().map_err(|error| anyhow!("init: {error}"))?;
                let start = Instant::now();
                let parsed = registry.create_mod_content(&mut provider);
                samples.push(start.elapsed().as_nanos() as u64);
                if let Err(errors) = parsed {
                    return Err(anyhow!("mod content errors: {errors:?}"));
                }
            }
            detail.insert("files".into(), serde_json::json!(file_count));
        }
        "patch" => {
            use mind_core::content::parser_hooks::PatchAsset;
            use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
            use mind_core::io::FileSystem;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::Mods;
            use mind_core::mods::patch::DataPatcher;
            let root = paths::find_repo_root(repo)?;
            let fixture = fixture.ok_or_else(|| anyhow!("bench patch needs --fixture"))?;
            let dir = root.join("parity/mod_fixtures").join(fixture);
            let fs = NativeFs;
            let mut mods = Mods::new(true, &dir);
            mods.load_single(&fs, &dir, &mind_core::io::SettingsStore::new())
                .with_context(|| format!("loading fixture `{fixture}`"))?;
            let patch_dir = dir.join("patches");
            let mut patches = Vec::new();
            if let Ok(files) = fs.walk(&patch_dir) {
                let mut files: Vec<_> = files
                    .into_iter()
                    .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                    .collect();
                files.sort();
                for file in files {
                    let text = String::from_utf8(fs.read(&file)?)
                        .map_err(|_| anyhow!("`{}` is not UTF-8", file.display()))?;
                    patches.push(PatchAsset {
                        name: file.display().to_string(),
                        json: text,
                    });
                }
            }
            let patch_count = patches.len();
            for _ in 0..runs {
                let bundle = MemoryBundle::new();
                let store = MemoryUnlockStore::new();
                let mut registry = create_base_content(&bundle, &store, true)
                    .map_err(|error| anyhow!("base content: {error}"))?;
                registry.init().map_err(|error| anyhow!("init: {error}"))?;
                let start = Instant::now();
                let mut patcher = DataPatcher::new();
                patcher
                    .apply(&mut registry, &patches)
                    .map_err(|error| anyhow!("apply: {error}"))?;
                patcher.unapply(&mut registry);
                samples.push(start.elapsed().as_nanos() as u64);
            }
            detail.insert("patches".into(), serde_json::json!(patch_count));
        }
        "cache" => {
            use mind_core::io::MockFs;
            use mind_core::mods::assets::DataAssetCache;
            let fs = MockFs::new();
            let mut cache = DataAssetCache::load("/cache");
            let payload = vec![0xABu8; 1024 * 1024];
            for _ in 0..runs {
                let start = Instant::now();
                cache
                    .add(&fs, &payload)
                    .map_err(|e| anyhow!("cache add: {e}"))?;
                samples.push(start.elapsed().as_nanos() as u64);
            }
            detail.insert("bytes".into(), serde_json::json!(payload.len()));
        }
        "overlay" => {
            use mind_core::assets::atlas::AtlasIndex;
            use mind_core::assets::overlay::AssetOverlayProvider;
            use mind_core::io::SettingsStore;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::Mods;
            let root = paths::find_repo_root(repo)?;
            let fixture = fixture.ok_or_else(|| anyhow!("bench overlay needs --fixture"))?;
            let dir = root.join("parity/mod_fixtures").join(fixture);
            let fs = NativeFs;
            let mut mods = Mods::new(true, &dir);
            mods.load_single(&fs, &dir, &SettingsStore::new())
                .with_context(|| format!("loading fixture `{fixture}`"))?;
            let atlas = std::fs::read_to_string(root.join("assets/sprites/sprites.atlas.json"))
                .ok()
                .and_then(|text| AtlasIndex::from_manifest_json(&text).ok());
            let atlas_has = |name: &str| atlas.as_ref().is_some_and(|i| i.find(name).is_some());
            for _ in 0..runs {
                let start = Instant::now();
                let overlay = mods.build_overlay(&fs, &atlas_has);
                samples.push(start.elapsed().as_nanos() as u64);
                detail.insert("sprites".into(), serde_json::json!(overlay.sprites().len()));
            }
        }
        "save-patches" => {
            use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
            use mind_core::io::save::versions::v1::base_meta_tags;
            use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
            use mind_core::mods::assets::{DataAsset, DataAssetType, DataAssets, ModDataManager};
            for _ in 0..runs {
                let bundle = MemoryBundle::new();
                let store = MemoryUnlockStore::new();
                let mut registry = create_base_content(&bundle, &store, true)
                    .map_err(|error| anyhow!("base content: {error}"))?;
                registry
                    .init()
                    .map_err(|error| anyhow!("content init: {error}"))?;
                let mut manager = ModDataManager::new();
                manager.push(DataAsset::patch(
                    "patches/a.json",
                    "{\"block.router.health\": 7}".to_owned(),
                ));
                manager.push(DataAsset::bundle("bundles/bundle.properties", "key=value"));
                manager.push(DataAsset::blob(
                    "sprites/external.png",
                    DataAssetType::Image,
                    vec![0xAB; 4096],
                    false,
                ));

                let start = Instant::now();
                let mut ctx = WriteContext::meta_only(base_meta_tags(0, 0, 0, "bench"));
                ctx.patches = Some(&manager);
                let bytes = SaveIo::write_to_vec(&ctx, &SaveOptions::new())
                    .map_err(|error| anyhow!("save write: {error}"))?;
                let mut state = SaveReadState::default();
                SaveIo::load_bytes(&bytes, &mut state)
                    .map_err(|error| anyhow!("save read: {error}"))?;
                let decoded = state.patches.clone().unwrap_or_default();
                let mut loaded = ModDataManager::new();
                loaded
                    .load(decoded, &mut registry)
                    .map_err(|error| anyhow!("asset load: {error}"))?;
                samples.push(start.elapsed().as_nanos() as u64);
            }
            detail.insert("bytes".into(), serde_json::json!(4096 + 64));
        }
        other => {
            return Err(anyhow!(
                "unknown mods bench scene `{other}` (discover/parse/patch/cache/overlay/save-patches)"
            ));
        }
    }

    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[samples.len() - 1];
    let mut report = serde_json::json!({
        "scene": scene,
        "runs": runs,
        "p50_us": p50.div_ceil(1_000),
        "p99_us": p99.div_ceil(1_000),
    });
    if let Some(object) = report.as_object_mut() {
        for (key, value) in detail {
            object.insert(key, value);
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods bench {scene}: p50 {}us, p99 {}us over {runs} runs",
            p50.div_ceil(1_000),
            p99.div_ceil(1_000)
        );
    }
    Ok(EXIT_PASS)
}

/// Plan 20 M5 (`mods overlay`): build a fixture mod's overlay and probe region
/// names against the resolved prefix/override/page rules.
fn cmd_mods_overlay(
    fixture: &str,
    repo: Option<&Path>,
    probes: &[String],
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::assets::atlas::AtlasIndex;
    use mind_core::assets::overlay::AssetOverlayProvider;
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &SettingsStore::new())
        .with_context(|| format!("loading fixture `{fixture}`"))?;

    let atlas_path = root.join("assets/sprites/sprites.atlas.json");
    let atlas = std::fs::read_to_string(&atlas_path)
        .ok()
        .and_then(|text| AtlasIndex::from_manifest_json(&text).ok());
    let atlas_has = |name: &str| {
        atlas
            .as_ref()
            .is_some_and(|index| index.find(name).is_some())
    };

    let overlay = mods.build_overlay(&fs, &atlas_has);
    let mut pass = true;
    let probe_reports: Vec<serde_json::Value> = probes
        .iter()
        .map(|name| match overlay.probe(name) {
            Some((path, page)) => serde_json::json!({
                "name": name,
                "found": true,
                "path": path,
                "page": page.name(),
            }),
            None => {
                pass = false;
                serde_json::json!({"name": name, "found": false})
            }
        })
        .collect();

    let report = serde_json::json!({
        "fixture": fixture,
        "sprites": overlay.sprites().len(),
        "bundles": overlay.bundles().len(),
        "pregenerated": overlay.pregenerated,
        "warnings": overlay.warnings,
        "probes": probe_reports,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods overlay: {} sprites, {} bundles, {} warning(s)",
            overlay.sprites().len(),
            overlay.bundles().len(),
            overlay.warnings.len()
        );
        for probe in &probe_reports {
            println!("{probe}");
        }
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Normalizes a mod report to the stable `expected_list.json` fields.
fn normalized_mod_list(report: &serde_json::Value) -> serde_json::Value {
    let mods = report
        .get("mods")
        .and_then(|value| value.as_array())
        .map(|mods| {
            mods.iter()
                .map(|entry| {
                    serde_json::json!({
                        "name": entry.get("name"),
                        "state": entry.get("state"),
                        "enabled": entry.get("enabled"),
                        "version": entry.get("version"),
                        "texturescale": entry.get("texturescale"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::Value::Array(mods)
}

fn load_scenario(cli: &Cli, name: &str) -> anyhow::Result<Scenario> {
    let fixture = registry::find(name)
        .ok_or_else(|| anyhow!("unknown scenario `{name}`; run `mind-headless list`"))?;
    let dir = paths::find_scenarios_dir(cli.scenarios_dir.as_deref())?;
    let path = dir.join(fixture.file_name());
    Scenario::read(&path).with_context(|| format!("loading scenario `{}`", path.display()))
}

/// Loads `parity/scenario_catalog.json` from the discovered repo root.
fn load_scenario_catalog() -> anyhow::Result<ScenarioCatalog> {
    let repo = crate::parity::find_repo(None)?;
    ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))
}

/// `list [--json] [--tier <T>]`: catalogued scenarios (plan 23 §3.2).
fn cmd_list(json: bool, tier: Option<&str>) -> anyhow::Result<i32> {
    let catalog = load_scenario_catalog()?;
    let entries: Vec<&crate::parity::scenario::ScenarioEntry> = catalog
        .entries
        .iter()
        .filter(|entry| tier.is_none_or(|tier| entry.tier == tier))
        .collect();
    if json {
        let scenarios: Vec<serde_json::Value> = entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "name": entry.name,
                    "plan": entry.plan,
                    "phase": entry.phase,
                    "tier": entry.tier,
                    "kind": entry.kind,
                    "runnable": entry.kind == "file",
                    "path": entry.path,
                    "command": entry.command,
                    "expect_checksum": entry.expect_checksum,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "format": 1,
                "tier": tier,
                "count": scenarios.len(),
                "scenarios": scenarios,
            }))?
        );
    } else {
        for entry in &entries {
            println!("{}", entry.name);
        }
    }
    Ok(EXIT_PASS)
}

/// `run-all [--tier <T>]`: run every file-backed scenario in the tier
/// (plan 23 §3.2). Embedded/planned entries are catalogued but not run.
fn cmd_run_all(cli: &Cli, tier: Option<&str>) -> anyhow::Result<i32> {
    let catalog = load_scenario_catalog()?;
    let entries: Vec<&crate::parity::scenario::ScenarioEntry> = catalog
        .entries
        .iter()
        .filter(|entry| entry.kind == "file")
        .filter(|entry| tier.is_none_or(|tier| entry.tier == tier))
        .collect();
    if entries.is_empty() {
        return Err(anyhow!(
            "no file-backed scenarios match tier `{}`",
            tier.unwrap_or("all")
        ));
    }
    let mut failed = 0;
    for entry in &entries {
        let name = entry.name.as_str();
        let code = if let Some(kind) = StdbScenario::from_name(name) {
            crate::stdb_scenarios::run(cli, kind, None, false)?
        } else if name == "sim_core_reset_play_cycle" {
            cmd_sim_core_reset_play_cycle(cli, 20, false)?
        } else {
            cmd_run(cli, name, None, false, None, None, 20, 0, None, None)?
        };
        if code != EXIT_PASS {
            failed += 1;
        }
    }
    println!(
        "run-all ({}): {}/{} passed",
        tier.unwrap_or("all tiers"),
        entries.len() - failed,
        entries.len()
    );
    Ok(if failed == 0 { EXIT_PASS } else { EXIT_FAIL })
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

/// Plan 22 M0 §3.2/§7b: `mind-headless version [--json] [--file <path>]`.
fn cmd_version(json: bool, file: Option<&Path>) -> anyhow::Result<i32> {
    let info = match file {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading `{}`", path.display()))?;
            mind_core::version::BuildInfo::init_from(&text)?
        }
        None => mind_core::version::BuildInfo::embedded().clone(),
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&info.to_json())?);
    } else {
        println!("[Mindustry] Version: {}", info.build_string());
        println!("  combined: {}", info.combined());
        println!("  type: {}", info.r#type);
        println!("  modifier: {}", info.modifier);
        println!("  commitHash: {}", info.commit_hash);
        println!("  buildDate: {}", info.build_date);
        println!("  number: {}", info.number);
        println!("  build: {}", info.build);
        println!("  revision: {}", info.revision);
        println!("  isSteam: {}", info.is_steam);
    }
    Ok(EXIT_PASS)
}

#[allow(clippy::too_many_arguments)]
fn cmd_run(
    cli: &Cli,
    name: &str,
    dump: Option<&Path>,
    json: bool,
    emit_commands: Option<&Path>,
    emit_simlog: Option<&Path>,
    cycles: u64,
    checksum_every: u64,
    golden: Option<&Path>,
    emit_checksums: Option<&Path>,
) -> anyhow::Result<i32> {
    // `stdb_*` scenarios (plan 01 §7.2) are connector tests, not sim scenarios:
    // they have their own fixture schema and never run `mind-core`.
    if let Some(kind) = StdbScenario::from_name(name) {
        if emit_commands.is_some() || emit_simlog.is_some() {
            log::warn!("--emit-commands/--emit-simlog are ignored for `{name}` (no sim commands)");
        }
        return crate::stdb_scenarios::run(cli, kind, dump, json);
    }
    // Plan 05 M8 §7.2: the reset/play cycle needs the `reset()`/`play()` flows.
    if name == "sim_core_reset_play_cycle" {
        return cmd_sim_core_reset_play_cycle(cli, cycles, json);
    }

    let scenario = load_scenario(cli, name)?;
    let collect = scenario.emit_per_tick;

    // Two in-process runs: the determinism assertion of §7b.
    let collect_ticks = collect || checksum_every > 0;
    let (mut sim, per_tick) = run_scenario(&scenario, collect_ticks)?;
    let (second, second_per_tick) = run_scenario(&scenario, collect_ticks)?;
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

    // Sampled checksum checkpoints (`sim_core_determinism`) + golden.
    let sampled = (checksum_every > 0).then(|| sample_checksums(&per_tick, checksum_every));
    if let Some(golden_path) = golden {
        match &sampled {
            Some(actual) => {
                let expected = read_golden_checksums(golden_path)?;
                if *actual != expected {
                    log::error!(
                        "sampled checksum golden mismatch for `{name}` ({} vs {} checkpoints)",
                        actual.len(),
                        expected.len()
                    );
                    pass = false;
                }
            }
            None => {
                log::error!("--golden requires --checksum-every for `{name}`");
                pass = false;
            }
        }
    }
    if let Some(path) = emit_checksums {
        let text = sampled
            .as_ref()
            .map(|checksums| checksums.join("\n") + "\n")
            .unwrap_or_default();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, text).with_context(|| format!("writing `{}`", path.display()))?;
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
    let simlog_emitted = match emit_simlog {
        Some(path) => {
            write_simlog(&scenario, &mut sim, path)?;
            Some(path.display().to_string())
        }
        None => None,
    };

    let group_counts: Vec<GroupCount> = sim
        .group_counts()
        .into_iter()
        .map(|(name, count)| GroupCount {
            name: name.to_owned(),
            count,
        })
        .collect();
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
        per_tick: if checksum_every > 0 {
            sampled.clone()
        } else if collect {
            Some(per_tick)
        } else {
            None
        },
        commands_emitted,
        simlog_emitted,
        state: sim.state_name().to_owned(),
        update_id: sim.update_id(),
        unimplemented_stub: 0,
        group_counts,
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Every `every`-th per-tick checksum plus the final tick (deterministic).
fn sample_checksums(per_tick: &[String], every: u64) -> Vec<String> {
    let every = every.max(1) as usize;
    let mut out = Vec::new();
    for (index, checksum) in per_tick.iter().enumerate() {
        if (index + 1) % every == 0 {
            out.push(checksum.clone());
        }
    }
    if let Some(last) = per_tick.last()
        && out.last() != Some(last)
    {
        out.push(last.clone());
    }
    out
}

/// Reads one hex checksum per non-empty line (`sim_core_*.checksums`).
fn read_golden_checksums(path: &Path) -> anyhow::Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading golden `{}`", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

/// Writes the scenario's resolved commands as a binary `.simlog`.
fn write_simlog(scenario: &Scenario, sim: &mut Sim, path: &Path) -> anyhow::Result<()> {
    let records = scenario.resolve_commands(sim.content())?;
    let mut log = CommandLog::new(LogHeader::new(scenario.seed, &scenario.name));
    for record in &records {
        log.push(record.tick, SimCommand::from_p0(record.command));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating `{}`", parent.display()))?;
    }
    std::fs::write(path, log.to_bytes())
        .with_context(|| format!("writing `{}`", path.display()))?;
    Ok(())
}

/// Plan 05 §7.2: `sim_core_reset_play_cycle` — groups/clock/entities must return
/// to baseline on every `reset`, and `play` must resume the fixed step.
fn cmd_sim_core_reset_play_cycle(cli: &Cli, cycles: u64, json: bool) -> anyhow::Result<i32> {
    if cycles == 0 {
        return Err(anyhow!("--cycles must be greater than zero"));
    }
    let scenario = load_scenario(cli, "sim_core_reset_play_cycle")?;
    let ticks = scenario.steps;
    let mut sim = Sim::new(
        scenario.seed,
        scenario.world.width,
        scenario.world.height,
        mind_core::content::BlockId::AIR,
        mind_core::content::BlockId::AIR,
    );
    // Seed a building so the reset has something to clear.
    sim.apply(mind_core::command::Command::Place {
        x: 1,
        y: 1,
        block: mind_core::content::BlockId::STONE_WALL,
    })?;

    let mut reset_clean = true;
    let mut play_advances = true;
    for _ in 0..cycles {
        sim.reset();
        if sim.phase() != mind_core::game::State::Menu {
            reset_clean = false;
        }
        if sim.clock().time != 0.0 || sim.clock().update_id != 0 {
            reset_clean = false;
        }
        if sim.group_counts().get("all").copied().unwrap_or(0) != 0 {
            reset_clean = false;
        }
        if sim
            .grid
            .iter_row_major()
            .any(|(_, index)| sim.grid.block_id_at(index) != mind_core::content::BlockId::AIR)
        {
            reset_clean = false;
        }

        if !sim.play() {
            play_advances = false;
        }
        for _ in 0..ticks {
            sim.tick()?;
        }
        if sim.phase() != mind_core::game::State::Playing || sim.tick_count() != ticks {
            play_advances = false;
        }
    }

    let pass = reset_clean && play_advances;
    let report = SimCoreCycleReport {
        cycles,
        ticks,
        reset_clean,
        play_advances,
        entity_baseline: 0,
        checksum: sim.checksum_hex(),
        pass,
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

/// One replay entry. Legacy JSONL commands stay on the P0 path (`Sim::apply`)
/// so `select_block` keeps its exact behavior; binary `.simlog` commands use the
/// canonical `SimCommand` path (plan 05 §6.4).
enum ReplayCommand {
    /// P0 `Command` from a JSONL log.
    P0(mind_core::command::Command),
    /// Canonical `SimCommand` from a binary `.simlog`.
    Sim(SimCommand),
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
    checksum_every: u64,
    workers: usize,
    golden: Option<&Path>,
) -> anyhow::Result<i32> {
    let bytes = std::fs::read(commands)
        .with_context(|| format!("reading command log `{}`", commands.display()))?;
    // Binary `.simlog` (plan 05 M8) or legacy JSONL text (`scenario` module).
    let (format, log_seed, entries): (String, u64, Vec<(u64, ReplayCommand)>) =
        if CommandLog::is_binary(&bytes) {
            let log = CommandLog::from_bytes(&bytes).map_err(|error| anyhow!("{error}"))?;
            let seed = log.header.seed;
            (
                String::from("binary"),
                seed,
                log.entries
                    .into_iter()
                    .map(|(tick, command)| (tick, ReplayCommand::Sim(command)))
                    .collect(),
            )
        } else {
            let text = String::from_utf8(bytes)
                .map_err(|_| anyhow!("`{}` is not UTF-8", commands.display()))?;
            let blocks = Blocks::new();
            let records = mind_core::scenario::parse_command_log(&blocks, &text)
                .with_context(|| format!("parsing command log `{}`", commands.display()))?;
            (
                String::from("text"),
                seed,
                records
                    .into_iter()
                    .map(|record| (record.tick, ReplayCommand::P0(record.command)))
                    .collect(),
            )
        };
    // A binary header carries the authoritative seed; `--seed` overrides it.
    let effective_seed = if format == "binary" && seed == 0 {
        log_seed
    } else {
        seed
    };
    let ticks = ticks.unwrap_or_else(|| {
        entries
            .last()
            .map(|(tick, _)| tick.saturating_add(1))
            .unwrap_or(0)
    });

    let mut sim = Sim::new(
        effective_seed,
        width,
        height,
        mind_core::content::BlockId::AIR,
        mind_core::content::BlockId::AIR,
    );
    let mut next = 0usize;
    let mut unsupported = 0usize;
    let mut all_checksums = Vec::new();
    for tick in 0..ticks {
        while next < entries.len() && entries[next].0 <= tick {
            match &entries[next].1 {
                ReplayCommand::P0(command) => sim.apply(*command)?,
                ReplayCommand::Sim(command) => match sim.command(command.clone()) {
                    Ok(()) => {}
                    Err(CommandError::Unsupported(op)) => {
                        // Later plans register the entity/unit paths; recording
                        // is still deterministic (the no-op is the same on every
                        // peer).
                        debug_assert!(!op.is_empty());
                        unsupported += 1;
                    }
                    Err(error) => log::warn!("replay command rejected: {error}"),
                },
            }
            next += 1;
        }
        sim.tick()?;
        all_checksums.push(sim.checksum_hex());
    }
    let sampled = if checksum_every > 0 {
        Some(sample_checksums(&all_checksums, checksum_every))
    } else if per_tick {
        Some(all_checksums.clone())
    } else {
        None
    };
    let expect_checksums = golden.map(read_golden_checksums).transpose()?;
    let pass = match (&sampled, &expect_checksums) {
        (Some(actual), Some(expected)) if actual == expected => true,
        (Some(_), Some(expected)) => {
            log::error!(
                "replay checksum golden mismatch (`{}` vs `{}` checkpoints)",
                sampled.as_ref().map(Vec::len).unwrap_or(0),
                expected.len()
            );
            false
        }
        (None, Some(_)) => {
            log::error!("--golden requires --checksum-every");
            false
        }
        _ => true,
    };

    if let Some(path) = dump {
        write_dump(&sim, path, false)?;
    }
    let report = SimCoreReplayReport {
        file: commands.display().to_string(),
        format,
        seed: effective_seed,
        width,
        height,
        tick: sim.tick_count(),
        workers,
        checksum: sim.checksum_hex(),
        unsupported_commands: unsupported,
        checksums: sampled,
        expect_checksums,
        pass,
    };
    print_report(&report, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_bench(
    cli: &Cli,
    ticks: u64,
    scenario_name: &str,
    profile: Option<&str>,
    assert_alloc: Option<u64>,
) -> anyhow::Result<i32> {
    if ticks == 0 {
        return Err(anyhow!("--ticks must be greater than zero"));
    }
    // Plan 05 M8 §7.4: `bench sim_core --profile {empty,mid,stress}`.
    if scenario_name == "sim_core" || profile.is_some() {
        return cmd_bench_sim_core(ticks, profile.unwrap_or("mid"), assert_alloc);
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

/// Plan 05 M8 §7.4: `bench sim_core --profile {empty,mid,stress}`.
///
/// Builds a flat grid and `buildings` placed blocks (units/bullets/items arrive
/// with plans 08–11 and extend these same profiles), warms the schedule, then
/// measures `Sim::tick`. The p99 budget is recording-only (plan 23 owns the hard
/// gate); `--assert-alloc N` fails when the timed region allocates more than `N`
/// times (requires `--features alloc-audit`).
fn cmd_bench_sim_core(ticks: u64, profile: &str, assert_alloc: Option<u64>) -> anyhow::Result<i32> {
    let (width, height, buildings, budget_us) = match profile {
        "empty" => (128i32, 128i32, 0usize, 500u64),
        "mid" => (256, 256, 600, 4_000),
        "stress" => (512, 512, 2_000, 10_000),
        other => {
            return Err(anyhow!(
                "unknown sim_core profile `{other}` (expected empty|mid|stress)"
            ));
        }
    };
    let mut sim = Sim::new(
        1,
        width,
        height,
        mind_core::content::BlockId::AIR,
        mind_core::content::BlockId::AIR,
    );
    let mut placed = 0usize;
    'place: for y in 0..height {
        for x in 0..width {
            if placed >= buildings {
                break 'place;
            }
            sim.apply(mind_core::command::Command::Place {
                x: x as i16,
                y: y as i16,
                block: mind_core::content::BlockId::STONE_WALL,
            })?;
            placed += 1;
        }
    }

    const WARMUP: u64 = 600;
    for _ in 0..WARMUP {
        sim.tick()?;
    }
    // A settling tick excludes process-level lazy initialization (dependency
    // threads, clock/time sources). `samples` is reserved before the baseline so
    // its allocation is not attributed to the timed region.
    sim.tick()?;
    let mut samples: Vec<u64> = Vec::with_capacity(ticks as usize);
    let allocs_before = alloc_count();
    for _ in 0..ticks {
        let start = Instant::now();
        sim.tick()?;
        samples.push(start.elapsed().as_nanos() as u64);
    }
    let allocs = alloc_count().saturating_sub(allocs_before);
    samples.sort_unstable();
    let p50_ns = percentile(&samples, 50);
    let p95_ns = percentile(&samples, 95);
    let p99_ns = percentile(&samples, 99);
    let within_budget = p99_ns.div_ceil(1_000) <= budget_us;
    let pass = assert_alloc.is_none_or(|limit| allocs <= limit);
    if !pass {
        log::error!(
            "sim_core/{profile} alloc audit failed: {allocs} allocation(s) across {ticks} ticks"
        );
    }
    if assert_alloc.is_some() && !alloc_audit_enabled() {
        log::warn!("--assert-alloc is a no-op without `--features alloc-audit`");
    }

    let report = SimCoreProfileReport {
        profile: profile.to_owned(),
        width,
        height,
        buildings: placed,
        ticks,
        p50_ns,
        p95_ns,
        p99_ns,
        checksum: sim.checksum_hex(),
        budget_us,
        within_budget,
        allocs,
        alloc_audit: alloc_audit_enabled(),
        assert_alloc,
        pass,
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
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

/// Deterministic hooks that spawn a bare building entity (plan 06 M3 oracle
/// until plan 07 provides the real building runtime).
struct BareBuildingHooks;

impl mind_core::world::WorldHooks for BareBuildingHooks {
    fn new_building(
        &self,
        world: &mut bevy_ecs::world::World,
        request: mind_core::world::NewBuilding,
    ) -> Option<bevy_ecs::entity::Entity> {
        use mind_core::ecs::{BuildingComp, EntitySeq, TeamId};
        Some(
            world
                .spawn((
                    EntitySeq(0),
                    BuildingComp {
                        pos: TilePos::new(request.x, request.y),
                        block: request.block,
                        team: TeamId(request.team),
                        rot: request.rot,
                    },
                ))
                .id(),
        )
    }
}

fn cmd_world_tile_ops(
    seed: u64,
    width: i32,
    height: i32,
    ops: u64,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::determinism::{RngStream, SimRng};
    use mind_core::world::ops::{WorldCtx, WorldEventLog};
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    if width <= 0 || height <= 0 {
        return Err(anyhow!("--width and --height must be positive"));
    }
    let content = boot_content()?;
    let mut grid = WorldGrid::new(width, height);
    let mut ecs = bevy_ecs::world::World::new();
    let hooks = NoopWorldHooks;
    let render = NoopRenderHooks;
    let mut log = WorldEventLog::default();
    let mut rng = SimRng::new(seed);

    let pick = |names: &[&str]| -> Vec<mind_core::content::BlockId> {
        names
            .iter()
            .filter_map(|name| content.block_id(name))
            .collect()
    };
    let floors = pick(&["stone", "sand-floor", "grass", "dirt", "ice", "moss"]);
    let overlays = pick(&["ore-copper", "ore-lead", "air"]);
    let walls = pick(&["stone-wall", "sand-wall", "air"]);

    {
        let mut ctx = WorldCtx {
            grid: &mut grid,
            content: &content,
            ecs: &mut ecs,
            hooks: &hooks,
            render: &render,
            log: &mut log,
        };
        for _ in 0..ops {
            let x = rng.random(RngStream::MapGen, width) as i16;
            let y = rng.random(RngStream::MapGen, height) as i16;
            match rng.random(RngStream::MapGen, 4) {
                0 => {
                    if !floors.is_empty() {
                        let floor =
                            floors[rng.random(RngStream::MapGen, floors.len() as i32) as usize];
                        ctx.set_floor(x, y, floor);
                    }
                }
                1 => {
                    if !overlays.is_empty() {
                        let overlay =
                            overlays[rng.random(RngStream::MapGen, overlays.len() as i32) as usize];
                        ctx.set_overlay(x, y, overlay);
                    }
                }
                2 => {
                    if !walls.is_empty() {
                        let wall =
                            walls[rng.random(RngStream::MapGen, walls.len() as i32) as usize];
                        ctx.set_block(x, y, wall, 0, 0);
                    }
                }
                _ => ctx.set_air(x, y),
            }
        }
    }

    // Counter == event-count invariant (M0 §7b).
    let tile_events = log.tile_changes.len() as i32;
    let floor_events = log.floor_changes.len() as i32;
    let counters_ok =
        grid.tile_changes == 1 + tile_events && grid.floor_changes == 1 + floor_events;

    // Histograms (BTreeMap → deterministic).
    let mut blocks: BTreeMap<String, u64> = BTreeMap::new();
    let mut floors_hist: BTreeMap<String, u64> = BTreeMap::new();
    for tile in grid.tiles.iter() {
        *blocks
            .entry(tile.block_name(&content).to_owned())
            .or_default() += 1;
        *floors_hist
            .entry(
                content
                    .block(tile.floor)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
    }

    let report = serde_json::json!({
        "format": 1,
        "generator": "tile_ops",
        "seed": seed,
        "width": width,
        "height": height,
        "ops": ops,
        "tile_changes": grid.tile_changes,
        "floor_changes": grid.floor_changes,
        "tile_events": tile_events,
        "floor_events": floor_events,
        "counters_ok": counters_ok,
        "counts": { "blocks": blocks, "floors": floors_hist },
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    if counters_ok {
        Ok(EXIT_PASS)
    } else {
        log::error!(
            "counter/event mismatch: tile_changes={} tile_events={} floor_changes={} floor_events={}",
            grid.tile_changes,
            tile_events,
            grid.floor_changes,
            floor_events
        );
        Ok(EXIT_FAIL)
    }
}

fn cmd_world_bench_gen(
    generator: &str,
    seed: u64,
    width: i32,
    height: i32,
    iters: u64,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::maps::generators::{SimplexGenerator, WorldGenerator};
    use mind_core::world::{WorldGrid, WorldParams};

    if width <= 0 || height <= 0 || iters == 0 {
        return Err(anyhow!("--width/--height/--iters must be positive"));
    }
    let content = boot_content()?;
    let params = WorldParams {
        seed_offset: seed,
        width,
        height,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(width, height);
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let start = Instant::now();
        match generator {
            "simplex" => {
                let mut g = SimplexGenerator::new(seed);
                g.generate(&mut grid.tiles, &params, &content);
            }
            "tantros" | "blank" | "serpulo" | "erekir" | "asteroid" => {
                let mut g = make_planet_generator(generator, seed)?;
                g.generate(&mut grid.tiles, &params, &content);
            }
            other => return Err(anyhow!("unknown generator `{other}`")),
        }
        samples.push(start.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let p = |q: f64| -> f64 {
        let index = ((samples.len() as f64 - 1.0) * q).round() as usize;
        samples[index] as f64 / 1_000_000.0
    };
    let report = serde_json::json!({
        "format": 1,
        "generator": generator,
        "seed": seed,
        "width": width,
        "height": height,
        "iters": iters,
        "p50_ms": p(0.50),
        "p95_ms": p(0.95),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "world gen bench: generator={generator} {width}x{height} iters={iters} p50={:.2}ms p95={:.2}ms",
            p(0.50),
            p(0.95)
        );
    }
    Ok(EXIT_PASS)
}

/// Constructs a vanilla planet generator for the harness `world gen` command.
fn make_planet_generator(
    planet: &str,
    seed: u64,
) -> anyhow::Result<Box<dyn mind_core::maps::generators::WorldGenerator>> {
    use mind_core::maps::generators::{BlankPlanetGenerator, WorldGenerator};
    let generator: Box<dyn WorldGenerator> = match planet {
        "blank" => Box::new(BlankPlanetGenerator::new(0)),
        "tantros" => Box::new(mind_core::maps::planet::TantrosPlanetGenerator::new()),
        "asteroid" => Box::new(mind_core::maps::planet::AsteroidGenerator::new(seed as i32)),
        "erekir" => Box::new(mind_core::maps::planet::ErekirPlanetGenerator::new()),
        "serpulo" => Box::new(mind_core::maps::planet::SerpuloPlanetGenerator::new()),
        other => return Err(anyhow!("unknown planet generator `{other}`")),
    };
    Ok(generator)
}

#[allow(clippy::too_many_arguments)]
fn cmd_world_gen(
    generator: &str,
    planet: Option<&str>,
    sector: u32,
    seed: u64,
    width: i32,
    height: i32,
    iters: u64,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::determinism::{Checksum, Hasher};
    use mind_core::maps::generators::{SimplexGenerator, WorldGenerator};
    use mind_core::world::{WorldGrid, WorldParams};

    if width <= 0 || height <= 0 {
        return Err(anyhow!("--width and --height must be positive"));
    }
    if iters == 0 {
        return Err(anyhow!("--iters must be positive"));
    }
    let content = boot_content()?;
    let params = WorldParams {
        seed_offset: seed,
        width,
        height,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(width, height);

    // `--planet` selects a vanilla planet generator; otherwise `--generator`
    // picks the flat/simplex helper (`--generator planet --planet X` also works).
    let planet = planet.or_else(|| (generator == "planet").then_some("serpulo"));
    for _ in 0..iters {
        if let Some(planet) = planet {
            let mut planet_gen: Box<dyn WorldGenerator> = make_planet_generator(planet, seed)?;
            planet_gen.generate(&mut grid.tiles, &params, &content);
        } else {
            match generator {
                "simplex" => {
                    let mut generator_impl = SimplexGenerator::new(seed);
                    generator_impl.generate(&mut grid.tiles, &params, &content);
                }
                "flat" | "blank" => {
                    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
                    for index in 0..grid.tiles.len() {
                        grid.tiles.geti_mut(index).floor = stone;
                    }
                }
                other => return Err(anyhow!("unknown generator `{other}`")),
            }
        }

        // `iters > 1` is a determinism check (same output every pass); the
        // generators overwrite tile data, so no reset is required.
    }

    let mut hasher = Hasher::new();
    hasher.write_u32(width as u32);
    hasher.write_u32(height as u32);
    let mut floors: BTreeMap<String, u64> = BTreeMap::new();
    let mut blocks: BTreeMap<String, u64> = BTreeMap::new();
    let mut overlays: BTreeMap<String, u64> = BTreeMap::new();
    for index in 0..grid.tiles.len() {
        let tile = grid.tiles.geti(index);
        hasher.write_u16(tile.block.raw());
        hasher.write_u16(tile.floor.raw());
        hasher.write_u16(tile.overlay.raw());
        for (map, id) in [
            (&mut floors, tile.floor),
            (&mut blocks, tile.block),
            (&mut overlays, tile.overlay),
        ] {
            *map.entry(
                content
                    .block(id)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
        }
    }
    let checksum = Checksum(hasher.finish().value()).to_hex();
    let generator_label = if planet.is_some() {
        "planet"
    } else {
        generator
    };

    let report = serde_json::json!({
        "format": 1,
        "generator": generator_label,
        "planet": planet,
        "sector": sector,
        "seed": seed,
        "width": width,
        "height": height,
        "checksum": checksum,
        "counts": { "floors": floors, "blocks": blocks, "overlays": overlays },
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    Ok(EXIT_PASS)
}

fn cmd_world_filters(
    seed: u64,
    width: i32,
    height: i32,
    stack: &str,
    order: &str,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::determinism::{Checksum, Hasher, SimRng};
    use mind_core::maps::filters::{FilterRegistry, apply_stack};
    use mind_core::world::WorldGrid;

    if width <= 0 || height <= 0 {
        return Err(anyhow!("--width and --height must be positive"));
    }
    let content = boot_content()?;
    let registry = FilterRegistry::vanilla();

    let mut tags: Vec<String> = stack
        .split(',')
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .collect();
    if order == "reverse" {
        tags.reverse();
    } else if order != "forward" {
        return Err(anyhow!("--order must be `forward` or `reverse`"));
    }

    let mut filters = Vec::with_capacity(tags.len());
    for tag in &tags {
        let payload = serde_json::json!({ "class": tag });
        let filter = registry
            .from_json(&content, &payload)
            .with_context(|| format!("building filter `{tag}`"))?;
        filters.push(filter);
    }

    // Deterministic base grid: stone floor, scattered walls.
    let mut grid = WorldGrid::new(width, height);
    let stone = content
        .block_id("stone")
        .unwrap_or(mind_core::content::BlockId::AIR);
    let sand = content
        .block_id("sand-floor")
        .unwrap_or(mind_core::content::BlockId::AIR);
    let wall = content
        .block_id("stone-wall")
        .unwrap_or(mind_core::content::BlockId::AIR);
    let mut base_rng = SimRng::new(seed);
    let n = (width * height) as usize;
    for index in 0..n {
        let noise_wall = base_rng.chance(mind_core::determinism::RngStream::MapGen, 0.05);
        let noise_sand = base_rng.chance(mind_core::determinism::RngStream::MapGen, 0.3);
        let tile = grid.tiles.geti_mut(index);
        tile.floor = if noise_sand { sand } else { stone };
        if noise_wall {
            tile.block = wall;
        }
    }

    let mut filter_rng = SimRng::new(seed ^ 0x5EED);
    apply_stack(&mut grid.tiles, &mut filters, &content, &mut filter_rng);

    // FNV over the resulting tiles (order contract: flat `x + y*width`).
    let mut hasher = Hasher::new();
    hasher.write_u32(width as u32);
    hasher.write_u32(height as u32);
    let mut floors: BTreeMap<String, u64> = BTreeMap::new();
    let mut blocks: BTreeMap<String, u64> = BTreeMap::new();
    for index in 0..n {
        let tile = grid.tiles.geti(index);
        hasher.write_u16(tile.block.raw());
        hasher.write_u16(tile.floor.raw());
        hasher.write_u16(tile.overlay.raw());
        *floors
            .entry(
                content
                    .block(tile.floor)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
        *blocks
            .entry(
                content
                    .block(tile.block)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
    }
    let checksum = Checksum(hasher.finish().value()).to_hex();

    let report = serde_json::json!({
        "format": 1,
        "generator": "filters",
        "seed": seed,
        "width": width,
        "height": height,
        "stack": tags,
        "order": order,
        "checksum": checksum,
        "counts": { "floors": floors, "blocks": blocks },
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    Ok(EXIT_PASS)
}

fn cmd_maps_list(dir: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::fs::NativeFs;
    use mind_core::maps::Maps;

    if !dir.is_dir() {
        return Err(anyhow!("`{}` is not a directory", dir.display()));
    }
    let fs = NativeFs;
    let mut maps = Maps::new();
    let loaded = maps.load_from_dir(&fs, dir, true);

    let entries: Vec<serde_json::Value> = maps
        .all()
        .iter()
        .map(|map| {
            serde_json::json!({
                "name": map.name(),
                "author": map.author(),
                "description": map.description(),
                "custom": map.custom,
                "width": map.width,
                "height": map.height,
                "version": map.version,
                "build": map.build,
                "file": map.file.display().to_string(),
            })
        })
        .collect();

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_list",
        "dir": dir.display().to_string(),
        "count": loaded,
        "maps": entries,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        for map in maps.all() {
            println!(
                "{} ({}x{}, custom={})",
                map.name(),
                map.width,
                map.height,
                map.custom
            );
        }
        println!("{} maps", maps.len());
    }
    Ok(EXIT_PASS)
}

fn cmd_world_multiblock(
    size: i32,
    block_name: &str,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::world::ops::{WorldCtx, WorldEventLog};
    use mind_core::world::{NoopRenderHooks, WorldGrid};

    if size < 1 {
        return Err(anyhow!("--size must be positive"));
    }
    let content = boot_content()?;
    let block = content
        .block_id(block_name)
        .ok_or_else(|| anyhow!("unknown block `{block_name}`"))?;
    let actual_size = content.block(block).map(|def| def.size).unwrap_or(1);
    if actual_size != size {
        return Err(anyhow!(
            "block `{block_name}` has size {actual_size}, expected {size}"
        ));
    }

    let mut grid = WorldGrid::new(8, 8);
    let mut ecs = bevy_ecs::world::World::new();
    let hooks = BareBuildingHooks;
    let render = NoopRenderHooks;
    let mut log = WorldEventLog::default();

    let mut linked = true;
    let mut first_overlap_cleared = true;
    let mut broke_air = true;

    {
        let mut ctx = WorldCtx {
            grid: &mut grid,
            content: &content,
            ecs: &mut ecs,
            hooks: &hooks,
            render: &render,
            log: &mut log,
        };
        // First multiblock at (1,1).
        ctx.set_block(1, 1, block, 0, 0);
        let first = ctx.grid.tiles.get(1, 1).build;
        let offset = -(size - 1) / 2;
        for dx in 0..size {
            for dy in 0..size {
                let tile = ctx.grid.tiles.get(1 + offset + dx, 1 + offset + dy);
                if tile.build != first || tile.block != block {
                    linked = false;
                }
            }
        }
        // Overlapping second multiblock at (2,2) must clear the first.
        ctx.set_block(2, 2, block, 0, 0);
        if ctx.grid.tiles.get(0, 0).block != mind_core::content::BlockId::AIR {
            first_overlap_cleared = false;
        }
        // Break the (center) block: its whole footprint returns to air.
        ctx.remove_block(2, 2);
        for dx in 0..size {
            for dy in 0..size {
                if ctx.grid.tiles.get(2 + offset + dx, 2 + offset + dy).block
                    != mind_core::content::BlockId::AIR
                {
                    broke_air = false;
                }
            }
        }
    }

    let pass = linked && first_overlap_cleared && broke_air;
    let report = serde_json::json!({
        "format": 1,
        "generator": "multiblock",
        "block": block_name,
        "size": size,
        "linked": linked,
        "first_overlap_cleared": first_overlap_cleared,
        "break_clears_footprint": broke_air,
        "pass": pass,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
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
