// SPDX-License-Identifier: GPL-3.0-only

//! Command implementations.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, anyhow};
use log::LevelFilter;
use mind_core::command::CommandRecord;
use mind_core::config::MindConfig;
use mind_core::content::{
    Blocks, ContentRegistry, MemoryBundle, MemoryUnlockStore, content_counts, create_base_content,
};
use mind_core::scenario::{Scenario, ScenarioPlayer, read_command_log, write_command_log};
use mind_core::sim::{Sim, StateDump};
use mind_core::world::TilePos;

use crate::cli::{AssetsCommand, Cli, Command, ContentCommand};
use crate::paths;
use crate::registry;
use crate::report::{
    BenchReport, ContentBenchReport, ContentIdEntry, ContentIdsReport, ContentLoadReport,
    ContentTypeCount, ContentTypeEntries, RunReport, SimReport, TileCheck,
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
        },
        Command::Assets { command } => match command {
            AssetsCommand::MigrateCheck { repo, manifest } => {
                cmd_assets_migrate_check(repo.as_deref(), manifest.as_deref())
            }
        },
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

/// Boots base content (create + init + postInit + load), the `content` harness
/// path. Assets/bundle are in-memory; headless skips icon/region loading.
fn boot_content() -> anyhow::Result<ContentRegistry> {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)?;
    registry.init()?;
    registry.post_init()?;
    registry.load()?;
    registry.log_content()?;
    Ok(registry)
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
