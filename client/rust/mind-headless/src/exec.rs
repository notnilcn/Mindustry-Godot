// SPDX-License-Identifier: GPL-3.0-only

//! Command implementations.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    AssetsCommand, Cli, Command, ContentCommand, EditorCommand, IoCommand, MapsCommand,
    MetaCommand, ModsCommand, TraceCommand, WorldCommand,
};
use crate::mp_scenarios::MpScenario;
use crate::parity::scenario::{ScenarioCatalog, ScenarioEntry};
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

/// When set, scenario runners in `run_suite` suppress their per-scenario
/// reports so the suite can emit one clean `format: 1` JSON document.
static QUIET: AtomicBool = AtomicBool::new(false);

/// Whether per-scenario output is currently suppressed (suite runs only).
pub(crate) fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

fn set_quiet(value: bool) {
    QUIET.store(value, Ordering::Relaxed);
}

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
        Command::Serve {
            config_dir,
            stdb_host,
            db,
            admin_token_file,
            match_config,
            stdb,
            commands,
            boot_timing_json,
            profile_json,
        } => Ok(crate::server::dedicated::run(
            crate::server::dedicated::ServeOptions {
                config_dir: config_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("config")),
                stdb_host: stdb_host.clone().unwrap_or_default(),
                db: db.clone().unwrap_or_default(),
                admin_token_file: admin_token_file.clone(),
                match_config: match_config.clone(),
                offline: stdb.eq_ignore_ascii_case("offline"),
                commands: commands.clone(),
                boot_timing_json: boot_timing_json.clone(),
                profile_json: profile_json.clone(),
            },
        )),
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
            checksum,
        } => cmd_bench(
            &cli,
            *ticks,
            scenario,
            profile.as_deref(),
            *assert_alloc,
            *checksum,
        ),
        Command::Dump {
            scenario,
            out,
            all_tiles,
        } => cmd_dump(&cli, scenario, out, *all_tiles),
        Command::Content { command } => match command {
            ContentCommand::Load { json } => cmd_content_load(*json),
            ContentCommand::Ids { json, out } => cmd_content_ids(*json, out.as_deref()),
            ContentCommand::Seed { out } => cmd_content_seed(out),
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
            MapsCommand::SaveLoadSave { map, json } => cmd_maps_save_load_save(map, *json),
            MapsCommand::Roundtrip { json } => cmd_maps_roundtrip(*json),
            MapsCommand::PreviewTiles { json } => cmd_maps_preview_tiles(*json),
            MapsCommand::ImageRoundtrip { json } => cmd_maps_image_roundtrip(*json),
            MapsCommand::RegistryShuffle { seeds, json } => {
                cmd_maps_registry_shuffle(*seeds, *json)
            }
            MapsCommand::Fix { dir, dry_run, json } => {
                cmd_maps_fix(dir.as_deref(), *dry_run, *json)
            }
        },
        Command::Editor { command } => match command {
            EditorCommand::Ops { fixture, json } => cmd_editor_ops(fixture, *json),
            EditorCommand::Roundtrip { map, seed, json } => {
                cmd_editor_roundtrip(map.as_deref(), *seed, *json)
            }
            EditorCommand::ResizeShift { json } => cmd_editor_resize_shift(*json),
            EditorCommand::GenPreview { json } => cmd_editor_gen_preview(*json),
            EditorCommand::Playtest { json } => cmd_editor_playtest(*json),
            EditorCommand::Objectives { fixture, json } => cmd_editor_objectives(fixture, *json),
            EditorCommand::WaveGraph { fixture, json } => cmd_editor_wave_graph(fixture, *json),
            EditorCommand::Locales { json } => cmd_editor_locales(*json),
            EditorCommand::Banned { json } => cmd_editor_banned(*json),
            EditorCommand::Assets { json } => cmd_editor_assets(*json),
            EditorCommand::Bench {
                suite,
                size,
                runs,
                json,
            } => cmd_editor_bench(suite, *size, *runs, *json),
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
        Command::Input { command } => crate::input_scenarios::run(command),
        Command::Parity { command } => crate::parity::run(&cli, command),
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
            ModsCommand::ErrorIsolation {
                fixture,
                repo,
                json,
            } => cmd_mods_error_isolation(fixture, repo.as_deref(), *json),
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

    if map == "serpulo" || map == "generated" {
        return cmd_io_roundtrip_real(map, width, height, out, json);
    }
    if map != "synthetic" {
        return Err(anyhow!(
            "io roundtrip --map {map}: unknown map; use `synthetic` or `serpulo`"
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

/// FNV-1a checksum over a grid's floor/overlay/block layers (row-major).
fn grid_tile_checksum(grid: &mind_core::world::WorldGrid) -> String {
    use mind_core::determinism::Hasher;
    let mut hasher = Hasher::new();
    hasher.write_u32(grid.tiles.width as u32);
    hasher.write_u32(grid.tiles.height as u32);
    for index in 0..grid.tiles.len() {
        let tile = grid.tiles.geti(index);
        hasher.write_u16(tile.block.raw());
        hasher.write_u16(tile.floor.raw());
        hasher.write_u16(tile.overlay.raw());
    }
    hasher.finish().to_hex()
}

/// Plan 06 M2 real-context round-trip (04 §5 M4 swap): generate a real planet
/// map, write it through [`mind_core::world::EcsMapSource`] and read it back
/// through the plan-06 [`mind_core::world::Context`] (no synthetic fixture).
/// Asserts the tile checksum survives save → reset → load.
fn cmd_io_roundtrip_real(
    map: &str,
    width: u16,
    height: u16,
    out: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::versions::v1::base_meta_tags;
    use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
    use mind_core::maps::generators::WorldGenerator;
    use mind_core::world::{Context, EcsMapSource, WorldGrid, WorldParams};

    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("mind-io-roundtrip-real.msav"));
    let fs = NativeFs;
    let content = boot_content()?;
    let params = WorldParams {
        seed_offset: 42,
        width: width as i32,
        height: height as i32,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(width as i32, height as i32);
    let mut generator = mind_core::maps::planet::SerpuloPlanetGenerator::new();
    generator.generate(&mut grid.tiles, &params, &content);
    let checksum_before = grid_tile_checksum(&grid);

    // Write through the plan-06 ECS-aware map source (no buildings: the serpulo
    // generator's launch loadout/ruins remain plan-12 seams).
    let ecs = bevy_ecs::world::World::new();
    let source = EcsMapSource::new(&grid, &content, &ecs);
    let mut tags = base_meta_tags(width, height, 0, map);
    tags.insert("name".to_owned(), map.to_owned());
    let mut ctx = WriteContext::meta_only(tags);
    ctx.content = Some(&content);
    ctx.map = Some(&source);
    SaveIo::save(&fs, &out, &ctx, &SaveOptions::new())?;
    let bytes = fs.len(&out).unwrap_or(0);

    // Read into a fresh grid through the real `Context` (04's WorldContext).
    let read_content = boot_content()?;
    let mut load_registry = boot_content()?;
    let mut loaded_grid = WorldGrid::new(0, 0);
    let all_buildings;
    {
        let mut context = Context::new(&mut loaded_grid, &read_content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut load_registry),
            ..SaveReadState::default()
        };
        SaveIo::load(&fs, &out, &mut state)?;
        all_buildings = state.all_buildings.len();
    }
    let checksum_after = grid_tile_checksum(&loaded_grid);

    let pass = checksum_before == checksum_after;
    if !pass {
        log::error!("io roundtrip (real) checksum mismatch: {checksum_before} != {checksum_after}");
    }
    let report = IoRoundtripReport {
        map: map.to_owned(),
        width,
        height,
        ticks: 0,
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

/// Total content records across every mappable kind (registry size).
fn script_content_count(registry: &mind_core::content::ContentRegistry) -> usize {
    registry.items().len()
        + registry.blocks().len()
        + registry.liquids().len()
        + registry.statuses().len()
        + registry.units().len()
        + registry.weathers().len()
        + registry.planets().len()
        + registry.sectors().len()
        + registry.teams().len()
}

/// Plan 20 M1/M3 §7b (`mods error-isolation`): one bad content file among good
/// ones must not remove the good records; the failure is reported instead.
fn cmd_mods_error_isolation(fixture: &str, repo: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::{Mods, provider::ModsContentProvider};

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &SettingsStore::new())
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

    let baseline = script_content_count(&registry);
    let result = registry.create_mod_content(&mut provider);
    let errors: Vec<String> = match &result {
        Ok(()) => Vec::new(),
        Err(errors) => errors.iter().map(|error| error.to_string()).collect(),
    };
    let delta = script_content_count(&registry) - baseline;

    let modded_items: Vec<&str> = registry
        .items()
        .iter()
        .filter(|item| item.minfo.is_modded())
        .map(|item| item.name.as_str())
        .collect();
    let modded_blocks: Vec<&str> = registry
        .blocks()
        .iter()
        .filter(|block| block.minfo.is_modded())
        .map(|block| block.name.as_str())
        .collect();
    let good_item = registry.item_id("error-mod-good-item").is_some();
    let bad_block_present = registry.block_id("error-mod-bad-block").is_some();
    // The `error` fixture has two good item files (one with an unknown field
    // that only warns) and one block with an unknown `type` (rejected).
    let pass = good_item && !bad_block_present && !errors.is_empty() && delta == 2;
    if !pass {
        log::error!(
            "mods error-isolation failed: good_item={good_item} bad_block={bad_block_present} delta={delta} errors={errors:?}"
        );
    }
    let report = serde_json::json!({
        "fixture": fixture,
        "goodItem": good_item,
        "badBlockPresent": bad_block_present,
        "errors": errors,
        "moddedItems": modded_items,
        "moddedBlocks": modded_blocks,
        "registryDelta": delta,
        "modHasContentErrors": mods.has_content_errors(),
        "pass": pass,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods error-isolation: {} error(s), registry delta {delta}, goodItem={good_item}, badBlock={bad_block_present}: {}",
            report["errors"].as_array().map_or(0, Vec::len),
            if pass { "PASS" } else { "FAIL" }
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
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

pub(crate) fn load_scenario(cli: &Cli, name: &str) -> anyhow::Result<Scenario> {
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

/// One executed suite case.
pub(crate) struct SuiteCase {
    pub name: String,
    pub pass: bool,
    pub duration_ms: u128,
}

/// The executed cases of a filtered catalog run.
pub(crate) struct SuiteReport {
    pub cases: Vec<SuiteCase>,
}

impl SuiteReport {
    pub fn passed(&self) -> usize {
        self.cases.iter().filter(|case| case.pass).count()
    }

    pub fn pass(&self) -> bool {
        self.cases.iter().all(|case| case.pass)
    }
}

/// Runs the file-backed scenarios matching an optional tier/phase filter.
pub(crate) fn run_catalog_entries(
    cli: &Cli,
    tier: Option<&str>,
    phase: Option<&str>,
) -> anyhow::Result<SuiteReport> {
    let catalog = load_scenario_catalog()?;
    let entries: Vec<&ScenarioEntry> = catalog
        .entries
        .iter()
        .filter(|entry| entry.kind == "file")
        .filter(|entry| tier.is_none_or(|tier| entry.tier == tier))
        .filter(|entry| phase.is_none_or(|phase| entry.phase.as_str() <= phase))
        .collect();
    if entries.is_empty() {
        return Err(anyhow!(
            "no file-backed scenarios match tier `{}`",
            tier.unwrap_or("all")
        ));
    }
    let mut cases = Vec::new();
    for entry in &entries {
        let name = entry.name.as_str();
        let started = Instant::now();
        let code = if let Some(kind) = StdbScenario::from_name(name) {
            crate::stdb_scenarios::run(cli, kind, None, false)?
        } else if name == "sim_core_reset_play_cycle" {
            cmd_sim_core_reset_play_cycle(cli, 20, false)?
        } else {
            cmd_run(cli, name, None, false, None, None, 20, 0, None, None)?
        };
        cases.push(SuiteCase {
            name: name.to_owned(),
            pass: code == EXIT_PASS,
            duration_ms: started.elapsed().as_millis(),
        });
    }
    Ok(SuiteReport { cases })
}

/// `run-all [--tier <T>]`: run every file-backed scenario in the tier
/// (plan 23 §3.2). Embedded/planned entries are catalogued but not run.
fn cmd_run_all(cli: &Cli, tier: Option<&str>) -> anyhow::Result<i32> {
    let report = run_catalog_entries(cli, tier, None)?;
    println!(
        "run-all ({}): {}/{} passed",
        tier.unwrap_or("all tiers"),
        report.passed(),
        report.cases.len()
    );
    Ok(if report.pass() { EXIT_PASS } else { EXIT_FAIL })
}

/// `parity run --suite <smoke|gate|full> [--phase Pn]`: the in-process suite
/// runner (plan 23 §3.2). Emits a `format: 1` report whose `results` match §3.2.
pub(crate) fn run_suite(
    cli: &Cli,
    suite: &str,
    phase: Option<&str>,
    json: bool,
) -> anyhow::Result<i32> {
    let tier = match suite {
        "smoke" => "T0",
        "gate" => "T1",
        "full" => "T2",
        other => {
            return Err(anyhow!("unknown suite `{other}`; expected smoke|gate|full"));
        }
    };
    set_quiet(true);
    let result = run_catalog_entries(cli, Some(tier), phase);
    set_quiet(false);
    let report = result?;
    let pass = report.pass();
    let passed = report.passed();
    let total = report.cases.len();
    if json {
        let results: Vec<serde_json::Value> = report
            .cases
            .iter()
            .map(|case| {
                serde_json::json!({
                    "scenario": case.name,
                    "status": if case.pass { "pass" } else { "fail" },
                    "duration_ms": case.duration_ms,
                    "error": if case.pass { serde_json::Value::Null } else { serde_json::Value::from("non-zero exit") },
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "format": 1,
                "suite": suite,
                "tier": tier,
                "phase": phase,
                "results": results,
                "counts": { "pass": passed, "fail": total - passed, "total": total },
                "pass": pass,
            }))?
        );
    } else {
        println!(
            "parity run --suite {suite} ({tier}{}): {passed}/{total} passed -> {}",
            phase.map(|p| format!(" <= {p}")).unwrap_or_default(),
            if pass { "PASS" } else { "FAIL" }
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Runs a scenario to completion; optionally collects per-tick checksums.
pub(crate) fn run_scenario(
    scenario: &Scenario,
    collect_ticks: bool,
) -> anyhow::Result<(Sim, Vec<String>)> {
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
    if is_quiet() {
        return Ok(());
    }
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
    // Plan 21 §7b: `mp_*` scenarios are network-free relay/lifecycle proofs.
    if let Some(kind) = MpScenario::from_name(name) {
        if emit_commands.is_some() || emit_simlog.is_some() {
            log::warn!("--emit-commands/--emit-simlog are ignored for `{name}` (mp scenario)");
        }
        return crate::mp_scenarios::run(cli, kind, dump, json);
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
pub(crate) fn sample_checksums(per_tick: &[String], every: u64) -> Vec<String> {
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
    checksum: bool,
) -> anyhow::Result<i32> {
    if ticks == 0 {
        return Err(anyhow!("--ticks must be greater than zero"));
    }
    // Plan 05 M8 §7.4: `bench sim_core --profile {empty,mid,stress}`.
    if scenario_name == "sim_core" || profile.is_some() {
        return cmd_bench_sim_core(ticks, profile.unwrap_or("mid"), assert_alloc, checksum);
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
    // Plan 21 §7d: network-free `mp_*` bench mirrors (no sim scenario file).
    if let Some((p50_ns, p99_ns, checksum)) = crate::mp_scenarios::bench(scenario_name, ticks) {
        let report = BenchReport {
            scenario: scenario_name.to_owned(),
            ticks,
            p50_ns,
            p99_ns,
            p50_us: p50_ns.div_ceil(1_000),
            p99_us: p99_ns.div_ceil(1_000),
            checksum,
            baseline_status: String::from("no-baseline"),
        };
        println!("{}", serde_json::to_string(&report)?);
        return Ok(EXIT_PASS);
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
    if checksum {
        println!("bench checksum: {}", report.checksum);
    }
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
fn cmd_bench_sim_core(
    ticks: u64,
    profile: &str,
    assert_alloc: Option<u64>,
    checksum: bool,
) -> anyhow::Result<i32> {
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
    if checksum {
        println!("bench checksum: {}", report.checksum);
    }
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

/// Plan-21 §3.12.4: generate the server module's `content_seed.rs`.
fn cmd_content_seed(out: &Path) -> anyhow::Result<i32> {
    let registry = boot_content()?;
    let mut names: BTreeMap<String, u8> = BTreeMap::new();
    for (index, type_) in registry::LIVE_CONTENT_TYPES.iter().enumerate() {
        for entry in registry.entries(*type_) {
            if let Some(name) = entry.name {
                names.entry(name.to_owned()).or_insert(index as u8);
            }
        }
    }
    let mut text = String::new();
    text.push_str("// SPDX-License-Identifier: GPL-3.0-only\n\n");
    text.push_str("//! GENERATED by `mind-headless content seed` (plan 21 §3.12.4).\n");
    text.push_str("//! Regenerate via `server/gen_content_seed.sh`; never hand-edit.\n\n");
    text.push_str("/// Vanilla content as `(content_type, name)` pairs (append-only ABI).\n");
    text.push_str("pub const VANILLA_CONTENT: &[(u8, &str)] = &[\n");
    for (name, content_type) in &names {
        text.push_str(&format!("    ({content_type}, {name:?}),\n"));
    }
    text.push_str("];\n");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, &text).with_context(|| format!("writing `{}`", out.display()))?;
    log::info!("wrote {} content entries to {}", names.len(), out.display());
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

/// JSON op-log fixture (plan 19 §6.1): `ops` is a list of `DrawOperation`s, each
/// a list of packed `u64` values (decimal numbers or decimal/hex strings).
#[derive(Debug, serde::Deserialize)]
struct EditorOpsFixture {
    #[serde(default = "default_editor_fixture_format")]
    format: u32,
    width: i32,
    height: i32,
    ops: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    checksum_after_apply: String,
    #[serde(default)]
    checksum_after_undo: String,
    #[serde(default)]
    checksum_after_redo: String,
}

fn default_editor_fixture_format() -> u32 {
    1
}

/// Parses one packed-op value (number, decimal string or `0x` hex string).
fn editor_op_value(value: &serde_json::Value) -> anyhow::Result<u64> {
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| anyhow!("op value `{number}` is not an unsigned integer")),
        serde_json::Value::String(text) => {
            let text = text.trim();
            if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16)
                    .map_err(|error| anyhow!("invalid hex op value `{text}`: {error}"))
            } else {
                text.parse::<u64>()
                    .map_err(|error| anyhow!("invalid op value `{text}`: {error}"))
            }
        }
        other => Err(anyhow!("unsupported op value `{other}`")),
    }
}

/// Canonical world checksum (`ChecksumPart for WorldGrid`, plan 06 §3.2).
fn editor_world_checksum(grid: &mind_core::world::WorldGrid) -> String {
    use mind_core::determinism::Checksummer;
    let mut checksummer = Checksummer::new();
    checksummer.part(grid);
    checksummer.finish().to_hex()
}

/// Compares two checksum strings ignoring an optional `0x` prefix and case.
fn editor_checksum_matches(actual: &str, expected: &str) -> bool {
    fn normalize(value: &str) -> String {
        value
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X")
            .to_ascii_lowercase()
    }
    normalize(actual) == normalize(expected)
}

/// `editor ops` (plan 19 M0 §5/§7b): replay a packed op-log fixture through the
/// real `WorldGrid` seam — apply → undo-all → redo-all — and assert the three
/// committed world checksums.
fn cmd_editor_ops(fixture: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::draw_op::DrawOperation;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::stack::OperationStack;
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    let text = std::fs::read_to_string(fixture)
        .with_context(|| format!("reading editor fixture `{}`", fixture.display()))?;
    let data: EditorOpsFixture = serde_json::from_str(&text)
        .with_context(|| format!("parsing editor fixture `{}`", fixture.display()))?;
    if data.format != 1 {
        return Err(anyhow!(
            "unsupported editor fixture format {} (expected 1)",
            data.format
        ));
    }
    if data.width <= 0 || data.height <= 0 {
        return Err(anyhow!(
            "fixture dimensions must be positive (got {}x{})",
            data.width,
            data.height
        ));
    }

    let content = boot_content()?;
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);

    let mut grid = WorldGrid::new(data.width, data.height);
    // `begin_edit_size` base: every tile is stone-floored air.
    for index in 0..grid.tiles.len() {
        grid.tiles.geti_mut(index).floor = stone;
    }
    let mut ecs = bevy_ecs::world::World::new();
    let hooks = NoopWorldHooks;
    let render = NoopRenderHooks;

    let mut stack = OperationStack::new();
    let mut op_count = 0usize;
    for packed in &data.ops {
        let mut ops = Vec::with_capacity(packed.len());
        for value in packed {
            ops.push(editor_op_value(value)?);
        }
        op_count += ops.len();
        let mut operation = DrawOperation::from_ops(ops);
        {
            let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            operation.redo(&mut world, &content);
        }
        stack.add(operation);
    }

    let checksum_after_apply = editor_world_checksum(&grid);
    while stack.can_undo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        stack.undo(&mut world, &content);
    }
    let checksum_after_undo = editor_world_checksum(&grid);
    while stack.can_redo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        stack.redo(&mut world, &content);
    }
    let checksum_after_redo = editor_world_checksum(&grid);

    // An apply → undo-all → redo-all cycle must land on the applied state.
    let round_trip_ok = checksum_after_apply == checksum_after_redo;
    let goldens_present = !data.checksum_after_apply.is_empty()
        && !data.checksum_after_undo.is_empty()
        && !data.checksum_after_redo.is_empty();
    let checksums_ok = !goldens_present
        || (editor_checksum_matches(&checksum_after_apply, &data.checksum_after_apply)
            && editor_checksum_matches(&checksum_after_undo, &data.checksum_after_undo)
            && editor_checksum_matches(&checksum_after_redo, &data.checksum_after_redo));

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_ops",
        "fixture": fixture.display().to_string(),
        "width": data.width,
        "height": data.height,
        "operations": data.ops.len(),
        "ops": op_count,
        "checksum_after_apply": checksum_after_apply,
        "checksum_after_undo": checksum_after_undo,
        "checksum_after_redo": checksum_after_redo,
        "expected": {
            "checksum_after_apply": data.checksum_after_apply,
            "checksum_after_undo": data.checksum_after_undo,
            "checksum_after_redo": data.checksum_after_redo,
        },
        "goldens_present": goldens_present,
        "round_trip_ok": round_trip_ok,
        "checksums_ok": checksums_ok,
        "ok": round_trip_ok && checksums_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("apply: {checksum_after_apply}");
        println!("undo:  {checksum_after_undo}");
        println!("redo:  {checksum_after_redo}");
        if !round_trip_ok {
            log::error!("editor ops round-trip mismatch: redo checksum != apply checksum");
        }
        if goldens_present && !checksums_ok {
            log::error!(
                "editor ops checksum mismatch against `{}`",
                fixture.display()
            );
        }
    }
    if round_trip_ok && checksums_ok {
        Ok(EXIT_PASS)
    } else {
        Ok(EXIT_FAIL)
    }
}

/// Normalized world checksum (counters zeroed) so a save/load round-trip is
/// comparable across the load epilogue's counter reset.
fn editor_world_checksum_normalized(grid: &mut mind_core::world::WorldGrid) -> String {
    grid.tile_changes = 0;
    grid.floor_changes = 0;
    editor_world_checksum(grid)
}

/// FNV-1a checksum of a preview image's pixels (plan 19 §7b).
fn preview_image_checksum(image: &mind_core::io::map::PreviewImage) -> String {
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u32(image.width);
    hasher.write_u32(image.height);
    hasher.write(&image.rgba);
    hasher.finish().to_hex()
}

/// `editor roundtrip` (plan 19 M1 §5/§7b): adopt → draw → undo → save → reload.
fn cmd_editor_roundtrip(map: Option<&Path>, _seed: u64, json: bool) -> anyhow::Result<i32> {
    use bevy_ecs::world::World;
    use mind_core::content::BlockId;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map};
    use mind_core::editor::{EditorTool, MapEditor};
    use mind_core::io::fs::NativeFs;
    use mind_core::io::map::MapIo;
    use mind_core::maps::Map;
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    let mut content = boot_content()?;
    let fs = NativeFs;
    let mut editor = MapEditor::new();
    let mut grid = WorldGrid::new(0, 0);
    let mut ecs = World::new();
    let hooks = NoopWorldHooks;
    let render = NoopRenderHooks;

    if let Some(file) = map {
        let header = MapIo::create_map(&fs, file, true)?;
        let loaded = Map::from_header(&header, true);
        editor.begin_edit_map(&mut grid, &mut content, &fs, &loaded)?;
    } else {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.begin_edit_size(&mut world, &content, 32, 32);
        editor.adopt_world(&mut world);
    }

    // adopt → draw 3 lines + 1 fill.
    let wall = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.draw_block = wall;
        editor.tool = EditorTool::Line;
        mind_core::editor::tool::touched_line(
            &mut editor,
            EditorTool::Line,
            &mut world,
            &content,
            4,
            4,
            20,
            4,
        );
        mind_core::editor::tool::touched_line(
            &mut editor,
            EditorTool::Line,
            &mut world,
            &content,
            4,
            6,
            4,
            18,
        );
        mind_core::editor::tool::touched_line(
            &mut editor,
            EditorTool::Line,
            &mut world,
            &content,
            8,
            8,
            16,
            16,
        );
        editor.flush_op();
    }
    let checksum_after_draw = editor_world_checksum_normalized(&mut grid);
    while editor.can_undo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.undo(&mut world, &content);
    }
    let checksum_after_undo = editor_world_checksum_normalized(&mut grid);
    while editor.can_redo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.redo(&mut world, &content);
    }
    let checksum_after_redo = editor_world_checksum_normalized(&mut grid);

    // save → reload.
    let dir = std::env::temp_dir().join("mgorch-editor");
    std::fs::create_dir_all(&dir)?;
    let save_file = dir.join("editor_roundtrip.msav");
    editor
        .tags
        .insert("name".to_owned(), "Editor Roundtrip".to_owned());
    editor.tags.insert("rules".to_owned(), "{}".to_owned());
    editor.tags.insert("genfilters".to_owned(), "{}".to_owned());
    editor.tags.insert("locales".to_owned(), "{}".to_owned());
    let base = editor_base_tags(
        grid.tiles.width as u16,
        grid.tiles.height as u16,
        "Editor Roundtrip",
    );
    save_editor_map(
        &fs,
        &save_file,
        &grid,
        &content,
        base,
        editor.tags.clone(),
        false,
    )?;
    let header = MapIo::create_map(&fs, &save_file, true)?;
    let reloaded = Map::from_header(&header, true);
    let mut grid2 = WorldGrid::new(0, 0);
    editor.begin_edit_map(&mut grid2, &mut content, &fs, &reloaded)?;
    let checksum_loaded = editor_world_checksum_normalized(&mut grid2);

    let round_trip_ok =
        checksum_after_draw == checksum_after_redo && checksum_after_draw == checksum_loaded;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_roundtrip",
        "source": map.map(|p| p.display().to_string()),
        "save": save_file.display().to_string(),
        "checksum_after_draw": checksum_after_draw,
        "checksum_after_undo": checksum_after_undo,
        "checksum_after_redo": checksum_after_redo,
        "checksum_loaded": checksum_loaded,
        "round_trip_ok": round_trip_ok,
        "ok": round_trip_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("draw:   {checksum_after_draw}");
        println!("undo:   {checksum_after_undo}");
        println!("redo:   {checksum_after_redo}");
        println!("loaded: {checksum_loaded}");
        println!("round trip: {}", if round_trip_ok { "ok" } else { "FAIL" });
    }
    Ok(if round_trip_ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `editor resize-shift` (plan 19 M1 §5/§7b): in-bounds tiles/data survive a
/// resize with a ±10 shift; out-of-bounds tiles become the default stone floor.
fn cmd_editor_resize_shift(json: bool) -> anyhow::Result<i32> {
    use bevy_ecs::world::World;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::{EditorGrid, MapEditor};
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    let content = boot_content()?;
    let mut editor = MapEditor::new();
    let mut grid = WorldGrid::new(0, 0);
    let mut ecs = World::new();
    let hooks = NoopWorldHooks;
    let render = NoopRenderHooks;
    let wall = content
        .block_id("copper-wall")
        .ok_or_else(|| anyhow!("content is missing `copper-wall`"))?;
    let stone = content
        .block_id("stone")
        .ok_or_else(|| anyhow!("content is missing `stone`"))?;

    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.begin_edit_size(&mut world, &content, 100, 100);
    }
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        for x in 20..60 {
            for y in 20..60 {
                world.set_block(x, y, wall, 0, 0);
            }
        }
        world.set_extra_data(30, 30, 0x4321);
    }
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.resize(&mut world, &content, 80, 80, -10, -10);
    }

    let ok = grid.tiles.width == 80
        && grid.tiles.height == 80
        && grid.tiles.get(10, 10).block == wall
        && grid.tiles.get(20, 20).extra_data == 0x4321
        && grid.tiles.get(0, 0).floor == stone;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_resize_shift",
        "width": grid.tiles.width,
        "height": grid.tiles.height,
        "preserved_tile": grid.tiles.get(10, 10).block.raw(),
        "preserved_extra": grid.tiles.get(20, 20).extra_data,
        "border_floor": grid.tiles.get(0, 0).floor.raw(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor resize-shift: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `editor gen-preview` (plan 19 M4 §5/§7b): a fixed filter stack over a
/// deterministic 64×64 snapshot produces identical preview pixels on every run;
/// a queued mid-generation re-run coalesces to the same result.
fn cmd_editor_gen_preview(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::generate::{EditorSnapshot, generate_preview};
    use mind_core::maps::filters::{FilterRegistry, parse_filters};
    use mind_core::world::WorldGrid;

    let content = boot_content()?;
    let stone = content
        .block_id("stone")
        .ok_or_else(|| anyhow!("content is missing `stone`"))?;
    let sand = content.block_id("sand-floor").unwrap_or(stone);
    let ice = content.block_id("ice").unwrap_or(stone);
    let wall = content.block_id("copper-wall").unwrap_or(stone);

    let mut grid = WorldGrid::new(64, 64);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    for x in 0..64 {
        for y in 0..64 {
            if (x + y) % 7 == 0 {
                grid.tiles.get_mut(x, y).floor = sand;
            }
            if (x * 3 + y) % 11 == 0 {
                grid.tiles.get_mut(x, y).floor = ice;
            }
        }
    }
    for x in 20..44 {
        grid.tiles.get_mut(x, 32).block = wall;
    }

    let filter_json = concat!(
        "[",
        "{\"class\":\"noise\",\"seed\":12345,\"scl\":25.0,\"threshold\":0.45,",
        "\"octaves\":3.0,\"falloff\":0.5,\"floor\":\"stone\",\"block\":\"stone-wall\"},",
        "{\"class\":\"scatter\",\"seed\":777,\"chance\":0.2,\"flooronto\":\"sand-floor\",",
        "\"block\":\"copper-wall\"}",
        "]"
    );
    let build = || {
        parse_filters(&content, filter_json, &FilterRegistry::vanilla())
            .unwrap_or_else(|error| panic!("fixed filter stack must parse: {error}"))
    };

    let snapshot = EditorSnapshot::capture(&grid);
    let image_a = generate_preview(&snapshot, &mut build(), &content, 0);
    // A second capture (as if an edit landed mid-generation) and re-run must
    // coalesce to the same preview when the world is unchanged.
    let snapshot_b = EditorSnapshot::capture(&grid);
    let image_b = generate_preview(&snapshot_b, &mut build(), &content, 0);

    let checksum = preview_image_checksum(&image_a);
    let checksum_b = preview_image_checksum(&image_b);
    let deterministic = image_a == image_b;
    let png = mind_core::io::map::encode_png(&image_a)?;
    let decoded = mind_core::io::map::decode_png(&png)?;
    let png_round_trip_ok = decoded == image_a;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_gen_preview",
        "width": image_a.width,
        "height": image_a.height,
        "checksum": checksum,
        "checksum_second_run": checksum_b,
        "png_bytes": png.len(),
        "deterministic": deterministic,
        "png_round_trip_ok": png_round_trip_ok,
        "ok": deterministic && png_round_trip_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor gen-preview: {checksum} deterministic={deterministic}");
    }
    Ok(if deterministic && png_round_trip_ok {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

/// `editor playtest` (plan 19 M7 §3.11): `edit_in_game` → `resume_editing` →
/// Shift `playtest` → `try_exit` over the plan-12 `PlaySession`.
fn cmd_editor_playtest(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::MapEditor;
    use mind_core::editor::playtest::{EditorPlayState, PlaytestOutcome};
    use mind_core::game::State;
    use mind_core::game::play::PlaySession;
    use mind_core::game::rules::Rules;
    use mind_core::game::rules_event::RulesEpoch;
    use mind_core::maps::Maps;
    use mind_core::world::NoopMapGenHooks;

    let _ = boot_content()?;

    let mut editor = MapEditor::new();
    let mut session = PlaySession::new(Rules::default());
    let mut play = EditorPlayState::new();
    play.rules.waves = true;
    let mut epoch = RulesEpoch::new();

    // 1. `editInGame`: snapshot + hidden editor gamemode + playing phase.
    let events_edit = play.edit_in_game(&mut editor, &mut session);
    let after_edit = session.phase;
    let editor_rules = session.rules.editor;
    let has_snapshot = play.last_saved_rules.is_some();

    // 2. `resumeEditing`: menu phase with the snapshot restored.
    play.resume_editing(&mut editor, &mut session);
    let after_resume = session.phase;
    let restored_waves = play.rules.waves;

    // 3. Shift `playtest`: save() is the gdext facade's job; the core state
    // machine auto-picks `sandbox` for a spawnless map and enters `Playing`.
    let map = Maps::map_for_file(
        std::path::PathBuf::from("/maps/playtest.msav"),
        16,
        16,
        "Playtest",
        true,
    );
    let outcome = play.playtest(
        &mut editor,
        &mut session,
        &map,
        &NoopMapGenHooks,
        &mut epoch,
        true,
    );
    let (mode, events_play) = match outcome {
        PlaytestOutcome::Playing { mode, events } => (mode.name().to_owned(), events),
        PlaytestOutcome::Dialog => ("dialog".to_owned(), Vec::new()),
    };
    let after_playtest = session.phase;

    // 4. `tryExit` always requires the unsaved-changes confirm.
    let exit_confirm = play.try_exit();

    // Deterministic digest of the transition sequence (no wall-clock/ids).
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u8(after_edit.as_u8());
    hasher.write_bool(editor_rules);
    hasher.write_bool(has_snapshot);
    hasher.write_u8(after_resume.as_u8());
    hasher.write_bool(restored_waves);
    hasher.write(mode.as_bytes());
    hasher.write_u32(events_edit.len() as u32);
    hasher.write_u32(events_play.len() as u32);
    hasher.write_bool(exit_confirm);
    hasher.write_u8(after_playtest.as_u8());
    let checksum = hasher.finish().to_hex();

    let ok = after_edit == State::Playing
        && editor_rules
        && has_snapshot
        && after_resume == State::Menu
        && restored_waves
        && mode == "sandbox"
        && after_playtest == State::Playing
        && exit_confirm;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_playtest",
        "phase_after_edit_in_game": after_edit.name(),
        "rules_editor": editor_rules,
        "has_snapshot": has_snapshot,
        "phase_after_resume_editing": after_resume.name(),
        "restored_waves": restored_waves,
        "auto_mode": mode,
        "phase_after_playtest": after_playtest.name(),
        "edit_events": events_edit.len(),
        "play_events": events_play.len(),
        "exit_confirm": exit_confirm,
        "checksum": checksum,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor playtest: {checksum} mode={mode} ok={ok}");
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `editor objectives` (plan 19 M5 §5/§7b): parse → serialize → parse the golden
/// objective fixture, preserving `editorPos`/`parents`, and verify every
/// descriptor name appears in the serialized JSON.
fn cmd_editor_objectives(fixture: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::objectives::{all_class_tags, class_fields};
    use mind_core::io::json::objectives::MapObjectives;

    let text = std::fs::read_to_string(fixture)
        .with_context(|| format!("reading objectives fixture `{}`", fixture.display()))?;
    let parsed = MapObjectives::from_json(&text)
        .with_context(|| format!("parsing objectives fixture `{}`", fixture.display()))?;
    let serialized = parsed.to_json()?;
    let reparsed = MapObjectives::from_json(&serialized)?;
    let round_trip_ok = parsed == reparsed;

    let value: serde_json::Value = serde_json::from_str(&serialized)?;
    let mut editor_pos_ok = true;
    let mut parents_ok = true;
    let mut descriptor_ok = true;
    for (index, objective) in parsed.iter().enumerate() {
        let object = &value[index];
        if object.get("editorPos").and_then(|v| v.as_i64())
            != Some(objective.common().editor_pos as i64)
        {
            editor_pos_ok = false;
        }
        let expected: Vec<i64> = objective
            .common()
            .parents
            .iter()
            .map(|p| *p as i64)
            .collect();
        let actual: Vec<i64> = object
            .get("parents")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();
        if expected != actual {
            parents_ok = false;
        }
        let keys = object
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>());
        for field in class_fields(objective.class_tag()) {
            if !keys
                .as_ref()
                .is_some_and(|keys| keys.iter().any(|key| key == field.name))
            {
                log::error!(
                    "descriptor `{}` missing from serialized {}",
                    field.name,
                    objective.class_tag()
                );
                descriptor_ok = false;
            }
        }
    }
    let all_classes_covered = all_class_tags()
        .iter()
        .all(|tag| !class_fields(tag).is_empty());

    let ok = round_trip_ok && editor_pos_ok && parents_ok && descriptor_ok && all_classes_covered;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_objectives",
        "fixture": fixture.display().to_string(),
        "count": parsed.len(),
        "round_trip_ok": round_trip_ok,
        "editor_pos_ok": editor_pos_ok,
        "parents_ok": parents_ok,
        "descriptor_ok": descriptor_ok,
        "all_classes_covered": all_classes_covered,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor objectives: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `editor wave-graph` (plan 19 M5 §5/§7b): compute the counts/health series for
/// a fixed wave stack and compare its checksum to the committed golden.
fn cmd_editor_wave_graph(fixture: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::wave_graph::{WaveGraphData, WaveGraphMode};
    use mind_core::game::spawn_group::SpawnGroup;

    let content = boot_content()?;
    let text = std::fs::read_to_string(fixture)
        .with_context(|| format!("reading wave-graph fixture `{}`", fixture.display()))?;
    let expected: serde_json::Value = serde_json::from_str(&text)?;
    let expected_checksum = expected
        .get("checksum")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();

    let mut dagger = SpawnGroup::new("dagger");
    dagger.unit_amount = 2;
    dagger.unit_scaling = 2.0;
    dagger.max = 40;
    let mut mace = SpawnGroup::new("mace");
    mace.begin = 2;
    mace.unit_amount = 1;
    mace.unit_scaling = 3.0;
    let groups = vec![dagger, mace];

    let resolve = |name: &str| {
        content
            .unit_by_name(name)
            .map(|def| (def.id.raw(), def.health))
    };
    let data = WaveGraphData::compute(&groups, 0, 5, resolve);
    let checksum = wave_graph_checksum(&data);
    let checksums_ok = expected_checksum.is_empty() || checksum == expected_checksum;

    // Structural invariants independent of the committed checksum.
    let structural = data.len() == 6
        && data.units.len() == 2
        && data.max >= 1
        && data.max_total >= 1
        && data.max_health >= 1.0
        && data.max_y(WaveGraphMode::Counts) >= data.max;

    let ok = checksums_ok && structural;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_wave_graph",
        "fixture": fixture.display().to_string(),
        "checksum": checksum,
        "expected_checksum": expected_checksum,
        "units": data.units.iter().map(|u| u.name.clone()).collect::<Vec<_>>(),
        "max": data.max,
        "max_total": data.max_total,
        "max_health": data.max_health,
        "checksums_ok": checksums_ok,
        "structural_ok": structural,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor wave-graph: {checksum} ok={ok}");
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// FNV-1a checksum of a [`WaveGraphData`] series.
fn wave_graph_checksum(data: &mind_core::editor::wave_graph::WaveGraphData) -> String {
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_i32(data.from);
    hasher.write_i32(data.to);
    for unit in &data.units {
        hasher.write_u16(unit.id);
        hasher.write(&unit.health.to_bits().to_le_bytes());
    }
    for row in &data.series {
        for value in row {
            hasher.write_i32(*value);
        }
    }
    for total in &data.totals {
        hasher.write_i32(*total);
    }
    for health in &data.health {
        hasher.write(&health.to_bits().to_le_bytes());
    }
    hasher.write_i32(data.max);
    hasher.write_i32(data.max_total);
    hasher.write(&data.max_health.to_bits().to_le_bytes());
    hasher.finish().to_hex()
}

/// `editor locales` (plan 19 M6 §5): map locales apply to a bundle target and
/// roll back cleanly; the `MapLocaleView` resolves objective text.
fn cmd_editor_locales(json: bool) -> anyhow::Result<i32> {
    use mind_core::game::map_objectives::ObjectiveLocale;
    use mind_core::io::StringMap;
    use mind_core::io::json::rules::MapLocales;
    use mind_core::maps::locales::{MapLocaleView, apply_to_all, parse_json, write_json};

    let mut locales = MapLocales::new();
    let mut en = StringMap::new();
    en.insert("foo.name".to_owned(), "Foo".to_owned());
    en.insert("foo.desc".to_owned(), "A thing".to_owned());
    let mut ru = StringMap::new();
    ru.insert("foo.name".to_owned(), "Фу".to_owned());
    locales.0.insert("en".to_owned(), en);
    locales.0.insert("ru".to_owned(), ru);

    let json_tag = write_json(&locales)?;
    let round_trip = parse_json(&json_tag)? == locales;
    let applied = apply_to_all(&mut locales, "en");
    let applied_ok = locales.0["ru"]["foo.desc"] == "A thing";

    // Apply to a live target bundle, then roll back from the snapshot.
    let mut target_before = StringMap::new();
    target_before.insert("foo.name".to_owned(), "Foo".to_owned());
    let mut target = target_before.clone();
    for (key, value) in &locales.0["ru"] {
        target.insert(key.clone(), value.clone());
    }
    let applied_live = target.get("foo.name").map(String::as_str) == Some("Фу");
    target.clone_from(&target_before);
    let rolled_back = target.get("foo.name").map(String::as_str) == Some("Foo");

    let view = MapLocaleView::new(&locales, "ru");
    let view_ok = view.fetch_text("@foo.desc") == "A thing"
        && view.map_locale("foo.name").as_deref() == Some("Фу");

    let ok = round_trip && applied_ok && applied_live && rolled_back && view_ok;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_locales",
        "round_trip_ok": round_trip,
        "applied_keys": applied,
        "applied_ok": applied_ok,
        "apply_live_ok": applied_live,
        "rollback_ok": rolled_back,
        "locale_view_ok": view_ok,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor locales: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `editor banned` (plan 19 M6 §5): the banned sets mutate `Rules` JSON.
fn cmd_editor_banned(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::banned::{
        BanKind, add, add_all, apply_selection, ban_set, ban_set_mut, filter_pane, remove,
        rules_json,
    };
    use mind_core::io::json::JsonIo;
    use mind_core::io::json::rules::Rules;

    let mut rules = Rules::default();
    {
        let blocks = ban_set_mut(&mut rules, BanKind::Block);
        add(blocks, "conveyor");
        add(blocks, "router");
    }
    {
        let units = ban_set_mut(&mut rules, BanKind::Unit);
        add_all(units, &["dagger".to_owned(), "mace".to_owned()]);
        remove(units, "mace");
    }
    let json_tag = rules_json(&rules)?;
    let parsed: Rules = JsonIo::read(&json_tag)?;
    let json_ok = parsed.banned_blocks.contains("conveyor")
        && parsed.banned_blocks.contains("router")
        && parsed.banned_units.contains("dagger")
        && !parsed.banned_units.contains("mace");

    let pane_selection = ban_set_mut(&mut rules, BanKind::Block);
    add(pane_selection, "router");
    let pane = filter_pane(
        &["conveyor".to_owned(), "router".to_owned()],
        ban_set(&rules, BanKind::Block),
        "",
        true,
    );
    apply_selection(
        ban_set_mut(&mut rules, BanKind::Block),
        &["copper-wall".to_owned()],
    );
    let pane_ok = pane == vec!["conveyor", "router"]
        && ban_set(&rules, BanKind::Block).contains("copper-wall");

    let ok = json_ok && pane_ok;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_banned",
        "json_ok": json_ok,
        "pane_ok": pane_ok,
        "banned_blocks": parsed.banned_blocks.iter().cloned().collect::<Vec<_>>(),
        "banned_units": parsed.banned_units.iter().cloned().collect::<Vec<_>>(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor banned: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `editor assets` (plan 19 M6 §5/§3.12): asset zip import/export round-trip.
fn cmd_editor_assets(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::assets::{asset_bytes, export_zip, import_zip};
    use mind_core::mods::assets::{DataAsset, DataAssetType};

    let assets = vec![
        DataAsset::patch("patch.json", r#"{"patch":true}"#),
        DataAsset::bundle("bundle.properties", "foo.name=Foo\n"),
        DataAsset::blob("icon.png", DataAssetType::Image, vec![1, 2, 3, 4], false),
    ];
    let zip = export_zip(&assets)?;
    let imported = import_zip(&zip)?;
    let round_trip = assets.iter().all(|original| {
        imported.iter().any(|candidate| {
            candidate.type_ == original.type_ && asset_bytes(candidate) == asset_bytes(original)
        })
    });
    let png_signature = zip.starts_with(b"PK");
    let ok = round_trip && png_signature && imported.len() == assets.len();
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_assets",
        "assets": assets.len(),
        "imported": imported.len(),
        "zip_bytes": zip.len(),
        "round_trip_ok": round_trip,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor assets: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `editor bench` (plan 19 M7 §7d): median timings for op recording, undo, fill
/// and replay over an editor grid.
fn cmd_editor_bench(suite: &str, size: i32, runs: u32, json: bool) -> anyhow::Result<i32> {
    use bevy_ecs::world::World;
    use mind_core::content::BlockId;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::{EditorGrid, EditorTool, MapEditor};
    use mind_core::util::alloc::{alloc_bytes, alloc_count, enabled as alloc_enabled};
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};
    use std::time::Instant;

    let content = boot_content()?;
    let runs = runs.max(1);
    let mut samples: Vec<f64> = Vec::with_capacity(runs as usize);

    for _ in 0..runs {
        let mut editor = MapEditor::new();
        let mut grid = WorldGrid::new(0, 0);
        let mut ecs = World::new();
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        {
            let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            editor.begin_edit_size(&mut world, &content, size, size);
        }
        let wall = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
        let start = Instant::now();
        match suite {
            "recache" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                world.recache_all();
            }
            "line" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                let len = (size - 4).max(1);
                mind_core::editor::tool::touched_line(
                    &mut editor,
                    EditorTool::Line,
                    &mut world,
                    &content,
                    2,
                    2,
                    2 + len,
                    2,
                );
                editor.flush_op();
            }
            "undo" | "mapview" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                let len = (size - 4).max(1);
                mind_core::editor::tool::touched_line(
                    &mut editor,
                    EditorTool::Line,
                    &mut world,
                    &content,
                    2,
                    2,
                    2 + len,
                    2,
                );
                editor.flush_op();
                editor.undo(&mut world, &content);
                editor.redo(&mut world, &content);
            }
            "fill" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                editor.tool = EditorTool::Fill;
                editor.draw_blocks(&mut world, &content, size / 2, size / 2);
                editor.flush_op();
            }
            "replay" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                let len = (size - 4).max(1);
                for _ in 0..20 {
                    mind_core::editor::tool::touched_line(
                        &mut editor,
                        EditorTool::Line,
                        &mut world,
                        &content,
                        2,
                        2,
                        2 + len,
                        2,
                    );
                    editor.flush_op();
                }
            }
            other => return Err(anyhow!("unknown editor bench suite `{other}`")),
        }
        samples.push(start.elapsed().as_secs_f64() * 1e6);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = samples[samples.len() / 2];

    // §7d MapView draw + op recording steady-state allocation audit (plan 19):
    // after warmup, repeated line/flush/undo/redo cycles must not allocate.
    let mut alloc_delta = 0u64;
    let mut bytes_delta = 0u64;
    if suite == "mapview" {
        let wall = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        let mut editor = MapEditor::new();
        let mut grid = WorldGrid::new(0, 0);
        let mut ecs = World::new();
        {
            let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            editor.begin_edit_size(&mut world, &content, size, size);
        }
        let len = (size - 4).max(1);
        let step = |editor: &mut MapEditor, world: &mut WorldEditorGrid| {
            editor.draw_block = wall;
            mind_core::editor::tool::touched_line(
                editor,
                EditorTool::Line,
                world,
                &content,
                2,
                2,
                2 + len,
                2,
            );
            editor.flush_op();
            editor.undo(world, &content);
            editor.redo(world, &content);
        };
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        for _ in 0..20 {
            step(&mut editor, &mut world);
        }
        let before = alloc_count();
        let bytes_before = alloc_bytes();
        for _ in 0..runs {
            step(&mut editor, &mut world);
        }
        alloc_delta = alloc_count().saturating_sub(before);
        bytes_delta = alloc_bytes().saturating_sub(bytes_before);
    }
    let alloc_ok = !alloc_enabled() || alloc_delta == 0;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_bench",
        "suite": suite,
        "size": size,
        "runs": runs,
        "p50_us": median,
        "p99_us": samples[samples.len() - 1],
        "alloc_audit_enabled": alloc_enabled(),
        "alloc_count": alloc_delta,
        "alloc_bytes": bytes_delta,
        "alloc_limit": 0,
        "alloc_ok": alloc_ok,
        "ok": alloc_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor bench {suite}: p50 {median:.1} us");
    }
    Ok(EXIT_PASS)
}

/// `maps fix` (plan 19 M8 §3.13/§7b): build a deterministic fixture dir, run the
/// MapFixer checks (dry-run then write), and assert the second write is a no-op.
fn cmd_maps_fix(dir: Option<&Path>, dry_run: bool, json: bool) -> anyhow::Result<i32> {
    use bevy_ecs::world::World;
    use mind_core::editor::MapEditor;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map};
    use mind_core::io::fs::NativeFs;
    use mind_core::io::json::JsonIo;
    use mind_core::io::json::rules::Rules;
    use mind_core::maps::fix::fix_dir;
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    let fs = NativeFs;
    let mut content = boot_content()?;
    let root = match dir {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir()?.join(path),
        None => std::env::temp_dir().join("mgorch-mapfix"),
    };
    let hidden = root.join("hidden");
    std::fs::create_dir_all(&hidden)?;
    let file = hidden.join("fixme.msav");

    // Deterministic fixture: a 16×16 editor map with fixable rules + wave 5.
    let mut editor = MapEditor::new();
    let mut grid = WorldGrid::new(0, 0);
    let mut ecs = World::new();
    let hooks = NoopWorldHooks;
    let render = NoopRenderHooks;
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.begin_edit_size(&mut world, &content, 16, 16);
    }
    #[allow(clippy::field_reassign_with_default)]
    let rules = {
        let mut rules = Rules::default();
        rules.infinite_resources = true;
        rules.instant_build = true;
        rules.banned_blocks.insert("conveyor".to_owned());
        rules.revealed_blocks.insert("router".to_owned());
        rules
    };
    editor.tags.insert("name".to_owned(), "fixme".to_owned());
    editor
        .tags
        .insert("rules".to_owned(), JsonIo::write(&rules)?);
    editor.tags.insert("wave".to_owned(), "5".to_owned());
    let base = editor_base_tags(16, 16, "fixme");
    save_editor_map(
        &fs,
        &file,
        &grid,
        &content,
        base,
        editor.tags.clone(),
        false,
    )?;

    let dry = fix_dir(&fs, &mut content, &root, true)?;
    let dry_changes: usize = dry.iter().map(|r| r.changes.len()).sum();
    if dry_run {
        let report = serde_json::json!({
            "format": 1,
            "generator": "maps_fix",
            "dry_run": true,
            "reports": dry.iter().map(|r| serde_json::json!({
                "file": r.file.display().to_string(),
                "changed": r.changed,
                "changes": r.changes,
            })).collect::<Vec<_>>(),
            "ok": dry_changes > 0,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(if dry_changes > 0 {
            EXIT_PASS
        } else {
            EXIT_FAIL
        });
    }

    let first = fix_dir(&fs, &mut content, &root, false)?;
    let second = fix_dir(&fs, &mut content, &root, false)?;
    let first_changed = first.iter().filter(|r| r.changed).count();
    let second_changed = second.iter().filter(|r| r.changed).count();
    let ok = dry_changes > 0 && first_changed == 1 && second_changed == 0;
    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_fix",
        "dry_run": false,
        "dry_changes": dry_changes,
        "first_changed": first_changed,
        "second_changed": second_changed,
        "first": first.iter().map(|r| serde_json::json!({
            "file": r.file.display().to_string(),
            "changed": r.changed,
            "changes": r.changes,
        })).collect::<Vec<_>>(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("maps fix: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps save-load-save` (plan 19 M2 §5/§7b): save → load → save byte-stable.
fn cmd_maps_save_load_save(map_name: &str, json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::MapEditor;
    use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map};
    use mind_core::io::FileSystem;
    use mind_core::io::StringMap as IndexMap;
    use mind_core::io::fs::NativeFs;
    use mind_core::io::map::MapIo;
    use mind_core::maps::Map;
    use mind_core::world::WorldGrid;

    let mut content = boot_content()?;
    let fs = NativeFs;
    let dir = std::env::temp_dir().join("mgorch-maps");
    std::fs::create_dir_all(&dir)?;
    let file_a = dir.join(format!("{map_name}_a.msav"));
    let file_b = dir.join(format!("{map_name}_b.msav"));

    let mut grid = WorldGrid::new(32, 32);
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let copper = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
    let ice = content.block_id("ice").unwrap_or(stone);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    grid.tiles.get_mut(5, 5).block = copper;
    grid.tiles.get_mut(7, 7).floor = ice;

    let mut tags = IndexMap::new();
    tags.insert("name".to_owned(), map_name.to_owned());
    tags.insert("description".to_owned(), "editor fixture".to_owned());
    tags.insert("author".to_owned(), "lane/f20-19".to_owned());
    tags.insert("rules".to_owned(), r#"{"editor":false}"#.to_owned());
    tags.insert("genfilters".to_owned(), "{}".to_owned());
    tags.insert("locales".to_owned(), r#"{"en":{"a":"A"}}"#.to_owned());
    let base = editor_base_tags(32, 32, map_name);
    save_editor_map(&fs, &file_a, &grid, &content, base, tags, false)?;
    let bytes_a = fs.read(&file_a)?;

    let header = MapIo::create_map(&fs, &file_a, true)?;
    let map_a = Map::from_header(&header, true);
    let mut grid2 = WorldGrid::new(0, 0);
    let mut editor = MapEditor::new();
    editor.begin_edit_map(&mut grid2, &mut content, &fs, &map_a)?;

    let base2 = editor_base_tags(
        grid2.tiles.width as u16,
        grid2.tiles.height as u16,
        map_name,
    );
    save_editor_map(
        &fs,
        &file_b,
        &grid2,
        &content,
        base2,
        editor.tags.clone(),
        false,
    )?;
    let bytes_b = fs.read(&file_b)?;

    let idempotent = bytes_a == bytes_b;
    let rules_a = map_a.tags.get("rules").cloned().unwrap_or_default();
    let header_b = MapIo::create_map(&fs, &file_b, true)?;
    let rules_b = header_b.tags.get("rules").cloned().unwrap_or_default();
    let rules_equal = rules_a == rules_b;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_save_load_save",
        "map": map_name,
        "bytes_a": bytes_a.len(),
        "bytes_b": bytes_b.len(),
        "idempotent": idempotent,
        "rules_equal": rules_equal,
        "rules": rules_a,
        "ok": idempotent && rules_equal,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!(
            "save-load-save: {} ({} B)",
            if idempotent && rules_equal {
                "ok"
            } else {
                "FAIL"
            },
            bytes_a.len()
        );
    }
    Ok(if idempotent && rules_equal {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

/// `maps roundtrip` (plan 06 M4 / plan 19 M7 §3.8): save a live grid through
/// `save_map`, generate its preview PNG/cache, import the copy through
/// `import_map` and assert both preview pixel checksums match.
fn cmd_maps_roundtrip(json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::maps_glue::{editor_base_tags, import_map_e2e, save_map_e2e};
    use mind_core::editor::preview::PreviewPipeline;
    use mind_core::io::FileSystem;
    use mind_core::io::StringMap as IndexMap;
    use mind_core::io::fs::{NativeFs, Paths};
    use mind_core::maps::Maps;
    use mind_core::world::WorldGrid;

    let fs = NativeFs;
    let root = std::env::temp_dir().join("mgorch-maps-roundtrip");
    let _ = std::fs::remove_dir_all(&root);
    let paths = Paths::new(&root);
    std::fs::create_dir_all(paths.maps())?;
    std::fs::create_dir_all(root.join("imported"))?;

    let mut content = boot_content()?;
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let core = content.block_id("core-shard");
    let spawn = content.block_id("spawn");

    let mut grid = WorldGrid::new(16, 16);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    if let Some(core) = core {
        grid.tiles.get_mut(4, 4).block = core;
    }
    let mut spawn_count = 0u32;
    if let Some(spawn) = spawn {
        grid.tiles.get_mut(8, 8).overlay = spawn;
        grid.tiles.get_mut(9, 8).overlay = spawn;
        spawn_count = 2;
    }

    let mut tags = IndexMap::new();
    tags.insert("name".to_owned(), "Roundtrip".to_owned());
    tags.insert("author".to_owned(), "lane/f25-maps".to_owned());
    let base = editor_base_tags(16, 16, "Roundtrip");
    let file = paths.maps().join("roundtrip.msav");

    let mut maps = Maps::new();
    let mut pipeline = PreviewPipeline::new();
    let (map, image) = save_map_e2e(
        &fs,
        &paths,
        &mut pipeline,
        &mut maps,
        &file,
        &grid,
        &mut content,
        base,
        tags,
        false,
    )?;
    let checksum_save = preview_image_checksum(&image);
    let preview_written = fs.exists(&mind_core::maps::preview_file(&paths, &map));
    let cache_written = fs.exists(&mind_core::maps::cache_file(&paths, &map));

    let import_dir = root.join("imported");
    let mut imported_maps = Maps::new();
    let mut imported_pipeline = PreviewPipeline::new();
    let (imported, image2) = import_map_e2e(
        &fs,
        &paths,
        &mut imported_pipeline,
        &mut imported_maps,
        &import_dir,
        &file,
        &mut content,
    )?;
    let checksum_import = preview_image_checksum(&image2);

    let map_ok = maps.len() == 1 && imported_maps.len() == 1 && imported.name() == map.name();
    let preview_ok = preview_written && cache_written && checksum_save == checksum_import;
    let ok = map_ok && preview_ok && map.spawns == spawn_count;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_roundtrip",
        "map": map.name(),
        "spawns": map.spawns,
        "preview_checksum_save": checksum_save,
        "preview_checksum_import": checksum_import,
        "preview_written": preview_written,
        "cache_written": cache_written,
        "registry_save": maps.len(),
        "registry_import": imported_maps.len(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("maps roundtrip: {checksum_save} -> {checksum_import} ok={ok}");
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps preview-tiles` (plan 19 M2 §5/§7b): deterministic preview pixels +
/// PNG round-trip.
fn cmd_maps_preview_tiles(json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::maps_glue::EditorMapSource;
    use mind_core::io::map::{MapIo, decode_png, encode_png};
    use mind_core::world::WorldGrid;

    let content = boot_content()?;
    let mut grid = WorldGrid::new(8, 8);
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let ore = content.block_id("ore-copper");
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    let mut ore_count = 0usize;
    if let Some(ore) = ore {
        grid.tiles.get_mut(2, 2).overlay = ore;
        grid.tiles.get_mut(5, 3).overlay = ore;
        ore_count = 2;
    }

    let source = EditorMapSource::new(&grid, &content);
    let image = MapIo::preview_from_tiles(&content, &source);
    let pixel_checksum = preview_image_checksum(&image);
    let png = encode_png(&image)?;
    let decoded = decode_png(&png)?;
    let png_ok = decoded == image;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_preview_tiles",
        "width": image.width,
        "height": image.height,
        "pixel_checksum": pixel_checksum,
        "png_bytes": png.len(),
        "png_round_trip_ok": png_ok,
        "ore_count": ore_count,
        "ok": png_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("preview {pixel_checksum} png_round_trip={png_ok}");
    }
    Ok(if png_ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps image-roundtrip` (plan 19 M2 §5/§7b): PNG → `read_image` → `write_image`
/// preserves the pixel checksum.
fn cmd_maps_image_roundtrip(json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::maps_glue::{EditorMapSource, GridImageSink};
    use mind_core::io::map::{
        BlockPalette, FnColorMapper, decode_png, encode_png, read_image, write_image,
    };
    use mind_core::world::WorldGrid;

    let content = boot_content()?;
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let ice = content.block_id("ice").unwrap_or(stone);

    let mut grid = WorldGrid::new(8, 8);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    for x in 0..8 {
        grid.tiles.get_mut(x, 4).floor = ice;
    }

    let palette = BlockPalette::of(&content);
    // The headless fixture builder does not run plan 03's region-derived
    // `map_color` pass, so invert the deterministic palette colors the writer
    // uses instead of plan-06's `ColorMapper` (which only sees explicit colors).
    let pairs: Vec<(u32, BlockId)> = content
        .blocks()
        .iter()
        .map(|def| (palette.map_color(def.id.raw()), def.id))
        .collect();
    let image_in = write_image(&palette, &EditorMapSource::new(&grid, &content));
    let checksum_in = preview_image_checksum(&image_in);
    let png = encode_png(&image_in)?;
    let decoded = decode_png(&png)?;

    let mut grid2 = WorldGrid::new(8, 8);
    for tile in grid2.tiles.array_mut() {
        tile.floor = stone;
    }
    let mapper = FnColorMapper(move |rgba: u32| {
        pairs
            .iter()
            .find(|(color, _)| *color == rgba)
            .map(|(_, block)| *block)
    });
    let mut sink = GridImageSink { grid: &mut grid2 };
    read_image(&palette, &decoded, &mut sink, &mapper)?;
    let image_out = write_image(&palette, &EditorMapSource::new(&grid2, &content));
    let checksum_out = preview_image_checksum(&image_out);
    let ok = checksum_in == checksum_out;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_image_roundtrip",
        "checksum_in": checksum_in,
        "checksum_out": checksum_out,
        "png_bytes": png.len(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("image round-trip: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps registry-shuffle` (plan 06 §7b, verified by 19 M2): registry ordering
/// and `ShuffleMode` selection across seeds never repeats `previous` when more
/// than one candidate exists.
fn cmd_maps_registry_shuffle(seeds: u32, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::StringMap as IndexMap;
    use mind_core::maps::{GameMode, Map, Maps, ShuffleMode};
    use mind_core::random::JavaRandom;

    fn named(name: &str, custom: bool, dir: &str) -> Map {
        let mut tags = IndexMap::new();
        tags.insert("name".to_owned(), name.to_owned());
        Map::new(
            std::path::PathBuf::from(format!("{dir}/{name}.msav")),
            16,
            16,
            tags,
            custom,
            1,
            -1,
        )
    }

    let mut maps = Maps::new();
    maps.add(named("alpha", false, "/maps/default"));
    maps.add(named("beta", false, "/maps/default"));
    maps.add(named("custom-one", true, "/maps"));
    maps.add(named("custom-two", true, "/maps"));
    let mut modded = named("mod-map", false, "/mods/x/maps");
    modded.mod_id = Some("x".to_owned());
    maps.add(modded);

    let order: Vec<&str> = maps.all().iter().map(Map::name).collect();
    let custom_before_builtin = order
        .iter()
        .position(|name| *name == "custom-one")
        .zip(order.iter().position(|name| *name == "alpha"))
        .is_some_and(|(custom, builtin)| custom < builtin);

    let mut rng = JavaRandom::new(1);
    let mut previous: Option<usize> = None;
    let mut repeats = 0usize;
    let mut selections = Vec::new();
    for _ in 0..seeds.max(1) {
        let next = ShuffleMode::All.next(GameMode::Survival, previous, maps.all(), &mut rng);
        if let Some(index) = next {
            if Some(index) == previous {
                repeats += 1;
            }
            selections.push(maps.all()[index].name().to_owned());
            previous = Some(index);
        }
    }
    let ok = custom_before_builtin && repeats == 0 && !selections.is_empty();

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_registry_shuffle",
        "order": order,
        "custom_before_builtin": custom_before_builtin,
        "selections": selections,
        "repeats": repeats,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("registry-shuffle: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
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
