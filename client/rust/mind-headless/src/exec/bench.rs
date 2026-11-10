// SPDX-License-Identifier: GPL-3.0-only

//! `sim`/`replay`/`bench`/`dump` command bodies and the simulation benchmark profiles (plans 05/22).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::scenario::{read_golden_checksums, write_dump};
use super::*;

/// Plan 05 §7.2: `sim_core_reset_play_cycle` — groups/clock/entities must return
/// to baseline on every `reset`, and `play` must resume the fixed step.
pub(super) fn cmd_sim_core_reset_play_cycle(
    cli: &Cli,
    cycles: u64,
    json: bool,
) -> anyhow::Result<i32> {
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

pub(super) fn cmd_sim(
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
pub(super) enum ReplayCommand {
    /// P0 `Command` from a JSONL log.
    P0(mind_core::command::Command),
    /// Canonical `SimCommand` from a binary `.simlog`.
    Sim(SimCommand),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn cmd_replay(
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

pub(super) fn cmd_bench(
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
pub(super) fn cmd_bench_sim_core(
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

pub(super) fn cmd_dump(
    cli: &Cli,
    scenario_name: &str,
    out: &Path,
    all_tiles: bool,
) -> anyhow::Result<i32> {
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

pub(super) fn percentile(samples: &[u64], percent: usize) -> u64 {
    debug_assert!(!samples.is_empty());
    let index = (samples.len() * percent / 100).min(samples.len().saturating_sub(1));
    samples.get(index).copied().unwrap_or(0)
}
