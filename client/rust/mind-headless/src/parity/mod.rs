// SPDX-License-Identifier: GPL-3.0-only

//! Plan 23 — the cross-cutting parity/verification harness.
//!
//! Owns the machine-readable registries under `parity/`:
//! `matrix.toml` (upstream-test mapping), `checksum_registry.json`,
//! `scenario_catalog.json`, `golden_manifest.json`, `bench_budgets.json`,
//! `mcp_catalog.json` and `soak.toml`. The subcommands under `parity ...`
//! validate those files and roll them up; they never mutate the registry.

pub mod budgets;
pub mod checksum;
pub mod desync;
pub mod determinism;
pub mod golden;
pub mod matrix;
pub mod mcp;
pub mod minitoml;
pub mod registry;
pub mod report;
pub mod scenario;
pub mod screenshot;
pub mod soak;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::cli::{Cli, ParityCommand};

use self::budgets::Budgets;
use self::golden::GoldenManifest;
use self::mcp::McpCatalog;
use self::registry::ChecksumRegistry;
use self::scenario::ScenarioCatalog;
use self::screenshot::ScreenshotManifest;
use self::soak::Soak;

/// Exit code: all checks passed.
const EXIT_PASS: i32 = 0;
/// Exit code: a check/verification failed.
const EXIT_FAIL: i32 = 1;

/// Marker file that identifies the repo root for parity tooling.
const PARITY_MARKER: &str = "parity/matrix.toml";

/// Resolves the repo root (explicit `--repo`, then `$MIND_REPO`, then walking up
/// from the current directory until `parity/matrix.toml` exists).
pub fn find_repo(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = explicit {
        if dir.join(PARITY_MARKER).is_file() {
            return Ok(dir.to_path_buf());
        }
        return Err(anyhow!(
            "repo root `{}` does not contain {PARITY_MARKER}",
            dir.display()
        ));
    }
    if let Some(dir) = std::env::var_os("MIND_REPO") {
        let dir = PathBuf::from(dir);
        if dir.join(PARITY_MARKER).is_file() {
            return Ok(dir);
        }
    }
    let start = std::env::current_dir().context("resolving current directory")?;
    let mut current = start.as_path();
    loop {
        if current.join(PARITY_MARKER).is_file() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(anyhow!(
                    "could not find `{PARITY_MARKER}` above `{}`; pass --repo",
                    start.display()
                ));
            }
        }
    }
}

/// Loads a `cargo test -- --list` dump, if given.
fn load_test_names(tests: Option<&Path>) -> Result<Option<BTreeSet<String>>> {
    match tests {
        None => Ok(None),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading test list `{}`", path.display()))?;
            Ok(Some(matrix::parse_test_list(&text)))
        }
    }
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Dispatches a `parity` subcommand.
pub fn run(cli: &Cli, command: &ParityCommand) -> Result<i32> {
    match command {
        ParityCommand::Check { json, repo, tests } => {
            let repo = find_repo(repo.as_deref())?;
            let names = load_test_names(tests.as_deref())?;
            let outcome = report::collect(&repo, names.as_ref())?;
            let summary = report::summary(&repo)?;
            if *json {
                print_json(&outcome.to_json(Some(&summary)))?;
            } else {
                print_outcome(&outcome);
            }
            Ok(if outcome.pass() { EXIT_PASS } else { EXIT_FAIL })
        }
        ParityCommand::Run { suite, phase, json } => {
            crate::exec::run_suite(cli, suite, phase.as_deref(), *json)
        }
        ParityCommand::Matrix {
            json,
            repo,
            tests,
            phase,
        } => cmd_matrix(*json, repo.as_deref(), tests.as_deref(), phase.as_deref()),
        ParityCommand::Registry { json, repo } => cmd_registry(*json, repo.as_deref()),
        ParityCommand::Goldens { json, repo } => cmd_goldens(*json, repo.as_deref()),
        ParityCommand::Budgets { json, repo } => cmd_budgets(*json, repo.as_deref()),
        ParityCommand::Mcp { json, repo } => cmd_mcp(*json, repo.as_deref()),
        ParityCommand::Scenarios { json, repo } => cmd_scenarios(*json, repo.as_deref()),
        ParityCommand::Soak {
            profile,
            minutes,
            ticks,
            seed,
            json,
        } => cmd_soak(profile, *minutes, *ticks, *seed, *json, None),
        ParityCommand::DesyncInject { case, seed, json } => cmd_desync(case, *seed, *json),
        ParityCommand::McpParity { suite, json, repo } => {
            cmd_mcp_parity(suite, *json, repo.as_deref())
        }
        ParityCommand::Screenshots {
            json,
            repo,
            diff_a,
            diff_b,
        } => cmd_screenshots(*json, repo.as_deref(), diff_a.as_deref(), diff_b.as_deref()),
        ParityCommand::Report { json, all, repo } => cmd_report(*json, *all, repo.as_deref()),
        ParityCommand::Gate {
            phase,
            json,
            repo,
            out,
        } => cmd_gate(phase, *json, repo.as_deref(), out.as_deref()),
        ParityCommand::BenchGate {
            json,
            baseline,
            canonical,
            repo,
        } => cmd_bench_gate(*json, baseline.as_deref(), *canonical, repo.as_deref()),
        ParityCommand::Checksums {
            suite,
            workers,
            no_cross_process,
            json,
        } => determinism::run_checksums(cli, suite, workers, *no_cross_process, *json),
        ParityCommand::ReplayFuzz {
            seed,
            mutations,
            json,
        } => determinism::replay_fuzz(*seed, mutations, *json),
        ParityCommand::Mirror { write, json, repo } => cmd_mirror(*write, *json, repo.as_deref()),
    }
}

fn print_outcome(outcome: &report::CheckOutcome) {
    for check in &outcome.checks {
        let status = if check.pass() { "PASS" } else { "FAIL" };
        let count = check.count.map(|n| format!(" ({n})")).unwrap_or_default();
        println!("parity {}: {status}{count}", check.name);
        if let Some(note) = &check.note {
            println!("  note: {note}");
        }
        for problem in &check.problems {
            println!("  - {problem}");
        }
    }
    println!(
        "parity: {} check(s), {} failed",
        outcome.checks.len(),
        outcome.failed()
    );
}

fn cmd_matrix(
    json: bool,
    repo: Option<&Path>,
    tests: Option<&Path>,
    phase: Option<&str>,
) -> Result<i32> {
    let repo = find_repo(repo)?;
    let rows = matrix::load(&repo.join("parity/matrix.toml"))?;
    let names = load_test_names(tests)?;
    let scope: Vec<&matrix::Row> = match phase {
        Some(phase) => matrix::rows_through_phase(&rows, phase),
        None => rows.iter().collect(),
    };
    let owned: Vec<matrix::Row> = scope.iter().map(|row| (*row).clone()).collect();
    let problems = matrix::check(&owned, &repo, names.as_ref());
    if json {
        let value = serde_json::json!({
            "format": 1,
            "phase": phase,
            "rows": owned.len(),
            "pass": problems.is_empty(),
            "problems": problems,
        });
        print_json(&value)?;
    } else {
        println!(
            "parity matrix: {} row(s){} -> {}",
            owned.len(),
            phase.map(|p| format!(" through {p}")).unwrap_or_default(),
            if problems.is_empty() { "PASS" } else { "FAIL" }
        );
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn cmd_registry(json: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let registry = ChecksumRegistry::load(&repo.join("parity/checksum_registry.json"))?;
    let problems = registry.check(&repo);
    if json {
        let value = serde_json::json!({
            "format": 1,
            "pass": problems.is_empty(),
            "checksum_version": registry.checksum_version,
            "core_checksum_version": mind_core::constants::CHECKSUM_VERSION,
            "algorithm": registry.algorithm,
            "owner": registry.owner,
            "contributors": registry.contributors.iter().map(|c| serde_json::json!({
                "id": c.id, "owner": c.owner, "status": c.status, "note": c.note
            })).collect::<Vec<_>>(),
            "fold_order": registry.fold_order,
            "problems": problems,
        });
        print_json(&value)?;
    } else {
        println!(
            "parity registry: checksum_version {} (core {}), {} contributor(s) -> {}",
            registry.checksum_version,
            mind_core::constants::CHECKSUM_VERSION,
            registry.contributors.len(),
            if problems.is_empty() { "PASS" } else { "FAIL" }
        );
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn cmd_goldens(json: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let manifest = GoldenManifest::load(&repo.join("parity/golden_manifest.json"))?;
    let results = manifest.verify(&repo);
    let problems = manifest.check(&repo);
    if json {
        let entries: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id, "path": r.path, "exists": r.exists,
                    "pass": r.pass, "expected": r.expected, "actual": r.actual
                })
            })
            .collect();
        let value = serde_json::json!({
            "format": 1,
            "pass": problems.is_empty(),
            "upstream_commit": manifest.upstream.commit,
            "goldens": entries,
            "problems": problems,
        });
        print_json(&value)?;
    } else {
        for result in &results {
            println!(
                "parity goldens: {} {} [{}]",
                if result.pass { "PASS" } else { "FAIL" },
                result.id,
                result.path
            );
        }
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn cmd_budgets(json: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let budgets = Budgets::load(&repo.join("parity/bench_budgets.json"))?;
    let problems = budgets.check(&repo);
    let sources = budgets.source_summary(&repo);
    if json {
        let entries: Vec<serde_json::Value> = budgets
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": entry.id, "plan": entry.plan, "measure": entry.measure,
                    "metric": entry.metric, "budget": entry.budget, "comparison": entry.comparison
                })
            })
            .collect();
        let value = serde_json::json!({
            "format": 1,
            "pass": problems.is_empty(),
            "machine": budgets.machine.id,
            "canonical": { "scenario": budgets.canonical.scenario, "metric": budgets.canonical.metric, "budget": budgets.canonical.budget, "fail_pct": budgets.canonical.fail_pct },
            "sources": sources.iter().map(|(plan, path, present)| serde_json::json!({"plan": plan, "path": path, "present": present})).collect::<Vec<_>>(),
            "entries": entries,
            "problems": problems,
        });
        print_json(&value)?;
    } else {
        println!(
            "parity budgets: {} row(s) across {} plan(s), canonical {}.{} <= {} -> {}",
            budgets.entries.len(),
            budgets
                .entries
                .iter()
                .map(|e| e.plan.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            budgets.canonical.scenario,
            budgets.canonical.metric,
            budgets.canonical.budget,
            if problems.is_empty() { "PASS" } else { "FAIL" }
        );
        for (plan, path, present) in &sources {
            println!(
                "  source plan {plan}: {path} [{}]",
                if *present { "present" } else { "missing" }
            );
        }
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn cmd_mcp(json: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let mcp = McpCatalog::load(&repo.join("parity/mcp_catalog.json"))?;
    let scenarios = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;
    let names = scenarios.names();
    let problems = mcp.check(&names);
    if json {
        let entries: Vec<serde_json::Value> = mcp
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": entry.id, "phase": entry.phase, "plan": entry.plan,
                    "scene": entry.scene, "scenario": entry.scenario,
                    "steps": entry.steps, "screenshots": entry.screenshots
                })
            })
            .collect();
        let value = serde_json::json!({
            "format": 1,
            "pass": problems.is_empty(),
            "entries": entries,
            "problems": problems,
        });
        print_json(&value)?;
    } else {
        println!(
            "parity mcp: {} entries -> {}",
            mcp.entries.len(),
            if problems.is_empty() { "PASS" } else { "FAIL" }
        );
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn cmd_scenarios(json: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let catalog = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;
    let problems = catalog.check(&repo);
    if json {
        let entries: Vec<serde_json::Value> = catalog
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "name": entry.name, "plan": entry.plan, "phase": entry.phase,
                    "tier": entry.tier, "kind": entry.kind, "path": entry.path,
                    "expect_checksum": entry.expect_checksum, "command": entry.command
                })
            })
            .collect();
        let value = serde_json::json!({
            "format": 1,
            "pass": problems.is_empty(),
            "entries": entries,
            "problems": problems,
        });
        print_json(&value)?;
    } else {
        println!(
            "parity scenarios: {} entries -> {}",
            catalog.entries.len(),
            if problems.is_empty() { "PASS" } else { "FAIL" }
        );
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn cmd_mirror(write: bool, json: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let problems = scenario::mirror(&repo, write)?;
    let pass = problems.is_empty();
    if json {
        print_json(&serde_json::json!({
            "format": 1,
            "write": write,
            "pass": pass,
            "problems": problems,
        }))?;
    } else if write {
        println!(
            "parity mirror: materialized client/scenarios/ from scenarios/ -> {}",
            if pass { "PASS" } else { "FAIL" }
        );
    } else {
        println!(
            "parity mirror: client/scenarios/ vs scenarios/ -> {}",
            if pass { "PASS" } else { "FAIL" }
        );
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_soak(
    profile: &str,
    minutes: Option<u64>,
    ticks: Option<u64>,
    seed: u64,
    json: bool,
    repo: Option<&Path>,
) -> Result<i32> {
    let repo = find_repo(repo)?;
    let soak = Soak::load(&repo.join("parity/soak.toml"))?;
    let problems = soak.check();
    if !problems.is_empty() {
        if json {
            print_json(&serde_json::json!({"format": 1, "pass": false, "problems": problems}))?;
        } else {
            for problem in &problems {
                println!("  - {problem}");
            }
        }
        return Ok(EXIT_FAIL);
    }
    let value = soak.run(profile, minutes, ticks, seed)?;
    let pass = value["pass"].as_bool().unwrap_or(false);
    if json {
        print_json(&value)?;
    } else if value["executed"].as_bool().unwrap_or(false) {
        println!(
            "parity soak: profile `{}` ran {} tick(s) in {} ms -> {}",
            value["profile"].as_str().unwrap_or(profile),
            value["ticks"].as_u64().unwrap_or(0),
            value["elapsed_ms"].as_u64().unwrap_or(0),
            if pass { "PASS" } else { "FAIL" },
        );
        println!(
            "  checksum {}; prefix_match {}; rss growth {:.2}% (max {}%); alloc delta {:?} (audit {})",
            value["final_checksum"].as_str().unwrap_or("-"),
            value["prefix_match"].as_bool().unwrap_or(false),
            value["rss_growth_pct"].as_f64().unwrap_or(0.0),
            value["max_rss_growth_pct"].as_f64().unwrap_or(0.0),
            value["alloc_delta"],
            value["alloc_audit"].as_bool().unwrap_or(false),
        );
    } else {
        println!(
            "parity soak: profile `{}` deferred ({})",
            value["profile"].as_str().unwrap_or(profile),
            value["deferred"].as_str().unwrap_or("editor/plan 21"),
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_desync(case: &str, seed: u64, json: bool) -> Result<i32> {
    let value = if case == "all" {
        desync::run_all(seed)?
    } else {
        desync::run(case, seed)?
    };
    let pass = value["pass"].as_bool().unwrap_or(false);
    if json {
        print_json(&value)?;
    } else if case == "all" {
        println!(
            "parity desync-inject: {} case(s) -> {}",
            value["cases"].as_object().map(|c| c.len()).unwrap_or(0),
            if pass { "PASS" } else { "FAIL" },
        );
    } else {
        println!(
            "parity desync-inject: case `{case}` expected {} got {} -> {}",
            value["expected"].as_str().unwrap_or("-"),
            value["observed"].as_str().unwrap_or("-"),
            if pass { "PASS" } else { "FAIL" },
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_mcp_parity(suite: &str, json: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let mcp = McpCatalog::load(&repo.join("parity/mcp_catalog.json"))?;
    let scenarios = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;
    let value = mcp::parity_report(&mcp, &scenarios, suite);
    let pass = value["pass"].as_bool().unwrap_or(false);
    if json {
        print_json(&value)?;
    } else {
        println!(
            "parity mcp-parity: suite {suite}, {} headless-checksum check(s) -> {} (in-engine deferred)",
            value["entries"].as_u64().unwrap_or(0),
            if pass { "PASS" } else { "FAIL" },
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_screenshots(
    json: bool,
    repo: Option<&Path>,
    diff_a: Option<&Path>,
    diff_b: Option<&Path>,
) -> Result<i32> {
    match (diff_a, diff_b) {
        (Some(left), Some(right)) => {
            let diff =
                screenshot::diff_png_files(left, right, screenshot::DiffTolerance::default())?;
            let pass = diff.pass;
            if json {
                print_json(&diff.to_json())?;
            } else {
                println!(
                    "parity screenshots diff: {}x{}, {} changed ({:.4}%), max delta {}, mean {:.3} -> {}",
                    diff.width,
                    diff.height,
                    diff.changed_pixels,
                    diff.changed_fraction * 100.0,
                    diff.max_channel_delta,
                    diff.mean_channel_delta,
                    if pass { "PASS" } else { "FAIL" },
                );
            }
            return Ok(if pass { EXIT_PASS } else { EXIT_FAIL });
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(anyhow!(
                "screenshot diff needs both `--diff-a` and `--diff-b`"
            ));
        }
        (None, None) => {}
    }
    let repo = find_repo(repo)?;
    let manifest = ScreenshotManifest::load(&repo.join("parity/screenshots/manifest.json"))?;
    let results = manifest.verify(&repo);
    let problems = manifest.check(&repo);
    let captured = results.iter().filter(|r| r.pass).count();
    let deferred = results.iter().filter(|r| !r.pass).count();
    if json {
        let entries: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id, "exists": r.exists,
                    "sha256_match": r.sha256_match, "pass": r.pass,
                    "status": "deferred",
                })
            })
            .collect();
        print_json(&serde_json::json!({
            "format": 1,
            "pass": problems.is_empty(),
            "captured": captured,
            "deferred": deferred,
            "capture_deferred": "in-engine screenshot capture needs the Godot editor (single-editor MCP mutex)",
            "baselines": entries,
            "problems": problems,
        }))?;
    } else {
        println!(
            "parity screenshots: {} baseline(s), {} satisfied, {} deferred -> {}",
            results.len(),
            captured,
            deferred,
            if problems.is_empty() { "PASS" } else { "FAIL" },
        );
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn cmd_report(json: bool, all: bool, repo: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    let outcome = report::collect(&repo, None)?;
    let summary = report::summary(&repo)?;
    let systems = if all {
        Some(report::system_rollup(&repo)?)
    } else {
        None
    };
    if json {
        let mut value = outcome.to_json(Some(&summary));
        value["all"] = serde_json::Value::Bool(all);
        if let Some(systems) = &systems {
            value["systems"] = serde_json::to_value(systems)?;
        }
        print_json(&value)?;
    } else {
        print_outcome(&outcome);
        println!("system parity roll-up:");
        for (status, count) in &summary.matrix_status {
            println!("  matrix {status}: {count}");
        }
        for (kind, count) in &summary.scenario_kind {
            println!("  scenario {kind}: {count}");
        }
        for (plan, count) in &summary.budget_plan {
            println!("  budget plan {plan}: {count}");
        }
        for (phase, count) in &summary.mcp_phase {
            println!("  mcp {phase}: {count}");
        }
        if let Some(systems) = &systems {
            println!("systems (--all):");
            for system in systems {
                println!(
                    "  plan {}: {} matrix / {} scenario / {} budget",
                    system.plan, system.matrix_rows, system.scenarios, system.budgets
                );
            }
        }
    }
    Ok(if outcome.pass() { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_gate(phase: &str, json: bool, repo: Option<&Path>, out: Option<&Path>) -> Result<i32> {
    let repo = find_repo(repo)?;
    // Steps 1, 6, 7 run here (matrix/registry/goldens/budgets + ledger proxy).
    // Steps 2-5 (T1 scenario suite, checksum matrix, MCP subset, bench-gate) are
    // owned by the nightly/perf runners and are reported as `deferred`.
    let outcome = report::collect(&repo, None)?;
    let summary = report::summary(&repo)?;
    let pass = outcome.pass();
    let value = serde_json::json!({
        "format": 1,
        "phase": phase,
        "pass": pass,
        "steps": [
            { "name": "matrix_check", "status": if pass { "pass" } else { "fail" } },
            { "name": "t1_scenario_suite", "status": "deferred", "owner": "tools/ci.sh" },
            { "name": "checksum_matrix", "status": "deferred", "owner": "nightly" },
            { "name": "mcp_parity", "status": "deferred", "owner": "nightly windows-gpu" },
            { "name": "bench_gate", "status": "deferred", "owner": "nightly perf" },
            { "name": "goldens_verify", "status": if pass { "pass" } else { "fail" } },
            { "name": "ledger_check", "status": "deferred", "owner": "parity report" }
        ],
        "checks": outcome.to_json(Some(&summary))["checks"],
    });
    if let Some(path) = out {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&value)?))
            .with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json {
        print_json(&value)?;
    } else {
        print_outcome(&outcome);
        println!(
            "parity gate {phase}: runnable steps {}; T1/checksum/MCP/bench are nightly-owned",
            if pass { "PASS" } else { "FAIL" }
        );
        if let Some(path) = out {
            println!("  report: {}", path.display());
        }
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn cmd_bench_gate(
    json: bool,
    baseline: Option<&Path>,
    canonical: bool,
    repo: Option<&Path>,
) -> Result<i32> {
    let repo = find_repo(repo)?;
    let budgets = Budgets::load(&repo.join("parity/bench_budgets.json"))?;
    let mut problems = budgets.check(&repo);
    let baseline_path = baseline
        .map(Path::to_path_buf)
        .unwrap_or_else(|| budgets::Baselines::default_path(&repo));
    let baseline = if baseline_path.is_file() {
        let loaded = budgets::Baselines::load(&baseline_path)?;
        problems.extend(loaded.check_against(&budgets));
        Some(loaded)
    } else {
        problems.push(format!(
            "bench baseline `{}` is missing (plan 23 M4)",
            baseline_path.display()
        ));
        None
    };
    // The canonical rule: the recorded value must be present on the baseline
    // machine before a >10% regression can gate. CI runners record only.
    let canonical_recorded = baseline
        .as_ref()
        .and_then(|baseline| {
            baseline
                .entries
                .iter()
                .find(|entry| entry.id == "canonical")
                .and_then(|entry| entry.value)
        })
        .is_some();
    if canonical && baseline.is_none() {
        problems.push(String::from(
            "bench-gate --canonical requires a committed `bench/baselines.json`",
        ));
    }
    let machine = baseline
        .as_ref()
        .map(|baseline| baseline.machine.id.clone())
        .unwrap_or_default();
    let entries = baseline
        .as_ref()
        .map(|baseline| baseline.entries.len())
        .unwrap_or(0);
    let value = serde_json::json!({
        "format": 1,
        "pass": problems.is_empty(),
        "baseline": baseline_path.display().to_string(),
        "machine": machine,
        "canonical": {
            "scenario": budgets.canonical.scenario,
            "metric": budgets.canonical.metric,
            "budget": budgets.canonical.budget,
            "fail_pct": budgets.canonical.fail_pct,
            "recorded": canonical_recorded
        },
        "entries": entries,
        "problems": problems,
        "note": "recording requires the baseline machine / self-hosted perf runner (plan 23 §7d)",
    });
    if json {
        print_json(&value)?;
    } else {
        println!(
            "parity bench-gate: baseline {entries} row(s), canonical {}.{} (fail >{}%, recorded {}) -> {}",
            budgets.canonical.scenario,
            budgets.canonical.metric,
            budgets.canonical.fail_pct,
            canonical_recorded,
            if problems.is_empty() { "PASS" } else { "FAIL" }
        );
        for problem in &problems {
            println!("  - {problem}");
        }
    }
    Ok(if problems.is_empty() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_repo_resolves_worktree_root() {
        let repo = find_repo(None).expect("repo");
        assert!(repo.join(PARITY_MARKER).is_file());
    }
}
