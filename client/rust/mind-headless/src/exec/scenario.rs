// SPDX-License-Identifier: GPL-3.0-only

//! `list`/`run`/`run-all`/`version` and the in-process suite runner over the scenario catalog (plan 23 §3.2).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::bench::cmd_sim_core_reset_play_cycle;
use super::*;

pub(crate) fn load_scenario(cli: &Cli, name: &str) -> anyhow::Result<Scenario> {
    let fixture = registry::find(name)
        .ok_or_else(|| anyhow!("unknown scenario `{name}`; run `mind-headless list`"))?;
    let dir = paths::find_scenarios_dir(cli.scenarios_dir.as_deref())?;
    let path = dir.join(fixture.file_name());
    Scenario::read(&path).with_context(|| format!("loading scenario `{}`", path.display()))
}

/// Loads `parity/scenario_catalog.json` from the discovered repo root.
pub(super) fn load_scenario_catalog() -> anyhow::Result<ScenarioCatalog> {
    let repo = crate::parity::find_repo(None)?;
    ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))
}

/// `list [--json] [--tier <T>]`: catalogued scenarios (plan 23 §3.2).
pub(super) fn cmd_list(json: bool, tier: Option<&str>) -> anyhow::Result<i32> {
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
pub(super) fn cmd_run_all(cli: &Cli, tier: Option<&str>) -> anyhow::Result<i32> {
    let report = run_catalog_entries(cli, tier, None)?;
    println!(
        "run-all ({}): {}/{} passed",
        tier.unwrap_or("all tiers"),
        report.passed(),
        report.cases.len()
    );
    Ok(exit_code(report.pass()))
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
    Ok(exit_code(pass))
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

pub(super) fn checksums_equal(expected: &str, actual: &str) -> bool {
    expected.eq_ignore_ascii_case(actual)
}

pub(super) fn write_dump(sim: &Sim, path: &Path, all_tiles: bool) -> anyhow::Result<()> {
    let json = sim.dump_json(all_tiles)?;
    // Schema validation: the dump must round-trip through the serde types (§6.3).
    let _decoded: StateDump = serde_json::from_str(&json)
        .with_context(|| format!("dump failed schema round-trip for `{}`", path.display()))?;
    std::fs::write(path, json).with_context(|| format!("writing dump `{}`", path.display()))
}

/// Plan 22 M0 §3.2/§7b: `mind-headless version [--json] [--file <path>]`.
pub(super) fn cmd_version(json: bool, file: Option<&Path>) -> anyhow::Result<i32> {
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
pub(super) fn cmd_run(
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
    Ok(exit_code(pass))
}

/// Reads one hex checksum per non-empty line (`sim_core_*.checksums`).
pub(super) fn read_golden_checksums(path: &Path) -> anyhow::Result<Vec<String>> {
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
pub(super) fn write_simlog(scenario: &Scenario, sim: &mut Sim, path: &Path) -> anyhow::Result<()> {
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
