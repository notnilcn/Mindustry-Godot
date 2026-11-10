// SPDX-License-Identifier: GPL-3.0-only

//! Command implementations: the dispatch table, the shared helpers and the
//! logging/quiet plumbing; each command family's bodies live in
//! `exec/<family>.rs`.
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

mod assets;
mod bench;
mod content;
mod editor;
mod io;
mod maps;
mod meta;
mod mods;
mod scenario;
mod world;

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
use mind_core::world::{NoopRenderHooks, NoopWorldHooks, TilePos, WorldGrid};

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

use assets::{
    cmd_assets_bundle_diff, cmd_assets_fallback_boot, cmd_assets_index, cmd_assets_migrate_check,
    cmd_assets_regions, cmd_assets_sounds_check,
};
use bench::{cmd_bench, cmd_dump, cmd_replay, cmd_sim, cmd_sim_core_reset_play_cycle};
use content::{
    boot_content, cmd_content_audit, cmd_content_bench, cmd_content_dump, cmd_content_ids,
    cmd_content_load, cmd_content_load_order_bad, cmd_content_seed,
};
use editor::{
    cmd_editor_assets, cmd_editor_banned, cmd_editor_bench, cmd_editor_gen_preview,
    cmd_editor_locales, cmd_editor_objectives, cmd_editor_ops, cmd_editor_playtest,
    cmd_editor_resize_shift, cmd_editor_roundtrip, cmd_editor_wave_graph,
};
use io::{
    cmd_io_bench_save, cmd_io_check_class_ids, cmd_io_check_revisions, cmd_io_dump_meta,
    cmd_io_map_list, cmd_io_roundtrip, cmd_io_settings,
};
use maps::{
    cmd_maps_fix, cmd_maps_image_roundtrip, cmd_maps_list, cmd_maps_preview_tiles,
    cmd_maps_registry_shuffle, cmd_maps_roundtrip, cmd_maps_save_load_save,
};
use meta::{cmd_meta_entities, cmd_trace_order};
use mods::{
    cmd_mods_assets, cmd_mods_bench, cmd_mods_content, cmd_mods_error_isolation, cmd_mods_list,
    cmd_mods_overlay, cmd_mods_patch,
};
use scenario::{cmd_list, cmd_run, cmd_run_all, cmd_version, load_scenario_catalog};
use world::{
    cmd_world_bench_gen, cmd_world_filters, cmd_world_gen, cmd_world_multiblock, cmd_world_tile_ops,
};

pub(crate) use scenario::{load_scenario, run_scenario, run_suite};

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;
/// Exit code: usage/IO error.
const EXIT_USAGE: i32 = 2;

/// Maps a pass/fail predicate to the process exit code.
fn exit_code(pass: bool) -> i32 {
    if pass { EXIT_PASS } else { EXIT_FAIL }
}

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

/// Runs `f` `runs` times, returning the elapsed nanoseconds of each run.
fn time_runs(runs: usize, mut f: impl FnMut() -> anyhow::Result<()>) -> anyhow::Result<Vec<u64>> {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        f()?;
        samples.push(start.elapsed().as_nanos() as u64);
    }
    Ok(samples)
}

/// The headless editor fixture: base content, a fresh grid, the ECS world and
/// the no-op hooks that [`WorldEditorGrid`](mind_core::editor::grid::WorldEditorGrid)
/// borrows.
struct EditorWorld {
    content: ContentRegistry,
    grid: WorldGrid,
    ecs: bevy_ecs::world::World,
    hooks: NoopWorldHooks,
    render: NoopRenderHooks,
}

/// Builds the editor fixture with a `width`×`height` grid.
fn editor_world(width: i32, height: i32) -> anyhow::Result<EditorWorld> {
    Ok(EditorWorld {
        content: boot_content()?,
        grid: WorldGrid::new(width, height),
        ecs: bevy_ecs::world::World::new(),
        hooks: NoopWorldHooks,
        render: NoopRenderHooks,
    })
}
