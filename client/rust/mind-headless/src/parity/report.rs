// SPDX-License-Identifier: GPL-3.0-only

//! Aggregated parity checks + report roll-up (`parity check` / `parity report`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;

use super::budgets::Budgets;
use super::golden::GoldenManifest;
use super::matrix;
use super::mcp::McpCatalog;
use super::registry::ChecksumRegistry;
use super::scenario::ScenarioCatalog;
use super::{budgets, matrix as matrix_mod};

/// One named check's result.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// Check name.
    pub name: String,
    /// Problems (empty means pass).
    pub problems: Vec<String>,
    /// Optional informational note.
    pub note: Option<String>,
    /// Optional item count for the roll-up.
    pub count: Option<usize>,
}

impl CheckResult {
    /// Creates a result.
    pub fn new(name: &str, problems: Vec<String>, count: Option<usize>) -> Self {
        Self {
            name: name.to_owned(),
            problems,
            note: None,
            count,
        }
    }

    /// Creates a result with a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Whether this check passed.
    pub fn pass(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The full check set.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    /// Individual checks.
    pub checks: Vec<CheckResult>,
}

impl CheckOutcome {
    /// Whether every check passed.
    pub fn pass(&self) -> bool {
        self.checks.iter().all(CheckResult::pass)
    }

    /// Number of failed checks.
    pub fn failed(&self) -> usize {
        self.checks.iter().filter(|check| !check.pass()).count()
    }

    /// Renders the outcome as JSON.
    pub fn to_json(&self, summary: Option<&Summary>) -> serde_json::Value {
        let checks: Vec<serde_json::Value> = self
            .checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "name": check.name,
                    "pass": check.pass(),
                    "problems": check.problems,
                    "note": check.note,
                    "count": check.count,
                })
            })
            .collect();
        let mut value = serde_json::json!({
            "format": 1,
            "pass": self.pass(),
            "counts": { "checks": self.checks.len(), "failed": self.failed() },
            "checks": checks,
        });
        if let Some(summary) = summary {
            value["summary"] = summary.to_json();
        }
        value
    }
}

/// Roll-up counters for `parity report`.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    /// Matrix rows by status.
    pub matrix_status: BTreeMap<String, usize>,
    /// Matrix rows by phase.
    pub matrix_phase: BTreeMap<String, usize>,
    /// Scenario entries by kind.
    pub scenario_kind: BTreeMap<String, usize>,
    /// Budget entries by plan.
    pub budget_plan: BTreeMap<String, usize>,
    /// MCP entries by phase.
    pub mcp_phase: BTreeMap<String, usize>,
}

impl Summary {
    /// Renders the summary as JSON.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "matrix_status": self.matrix_status,
            "matrix_phase": self.matrix_phase,
            "scenario_kind": self.scenario_kind,
            "budget_plan": self.budget_plan,
            "mcp_phase": self.mcp_phase,
        })
    }
}

/// One per-system roll-up row (plan 23 M7 `parity report --all`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemRollup {
    /// Owning plan short number.
    pub plan: String,
    /// Matrix rows owned by the plan.
    pub matrix_rows: usize,
    /// Catalogued scenarios owned by the plan.
    pub scenarios: usize,
    /// Budget rows owned by the plan.
    pub budgets: usize,
}

/// Derives the whole-program per-system roll-up from the committed registries.
pub fn system_rollup(repo: &Path) -> Result<Vec<SystemRollup>> {
    let rows = matrix_mod::load(&repo.join("parity/matrix.toml"))?;
    let scenarios = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;
    let budgets = budgets::Budgets::load(&repo.join("parity/bench_budgets.json"))?;

    let mut by_plan: BTreeMap<String, SystemRollup> = BTreeMap::new();
    for row in &rows {
        let plan = owner_plan_number(&row.owner);
        let entry = by_plan.entry(plan.clone()).or_insert_with(|| SystemRollup {
            plan,
            matrix_rows: 0,
            scenarios: 0,
            budgets: 0,
        });
        entry.matrix_rows += 1;
    }
    for scenario in &scenarios.entries {
        let entry = by_plan
            .entry(scenario.plan.clone())
            .or_insert_with(|| SystemRollup {
                plan: scenario.plan.clone(),
                matrix_rows: 0,
                scenarios: 0,
                budgets: 0,
            });
        entry.scenarios += 1;
    }
    for budget in &budgets.entries {
        let entry = by_plan
            .entry(budget.plan.clone())
            .or_insert_with(|| SystemRollup {
                plan: budget.plan.clone(),
                matrix_rows: 0,
                scenarios: 0,
                budgets: 0,
            });
        entry.budgets += 1;
    }
    Ok(by_plan.into_values().collect())
}

/// Extracts the `{nn}` plan number from an owner filename like
/// `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (or returns the input unchanged when it
/// is already a short number).
fn owner_plan_number(owner: &str) -> String {
    let digits: String = owner.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        owner.to_owned()
    } else {
        digits
    }
}

/// Collects every structural check against the repo.
pub fn collect(repo: &Path, tests: Option<&BTreeSet<String>>) -> Result<CheckOutcome> {
    let mut checks = Vec::new();

    let matrix_rows = matrix::load(&repo.join("parity/matrix.toml"))?;

    {
        let landed = matrix_rows
            .iter()
            .filter(|row| row.status == "landed")
            .count();
        let note = match tests {
            Some(names) => {
                let resolved = matrix_rows
                    .iter()
                    .filter(|row| row.status == "landed" && names.contains(&row.rust))
                    .count();
                Some(format!(
                    "resolved {resolved}/{landed} landed rows against {} test names",
                    names.len()
                ))
            }
            None => Some(format!(
                "{landed} landed rows; no --tests list supplied (not resolved)"
            )),
        };
        let mut result = CheckResult::new(
            "matrix",
            matrix::check(&matrix_rows, repo, tests),
            Some(matrix_rows.len()),
        );
        if let Some(note) = note {
            result = result.with_note(note);
        }
        checks.push(result);
    }

    {
        let registry = ChecksumRegistry::load(&repo.join("parity/checksum_registry.json"))?;
        checks.push(CheckResult::new(
            "checksum_registry",
            registry.check(repo),
            Some(registry.contributors.len()),
        ));
    }

    let scenarios = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;
    checks.push(CheckResult::new(
        "scenario_catalog",
        scenarios.check(repo),
        Some(scenarios.entries.len()),
    ));

    {
        let manifest = GoldenManifest::load(&repo.join("parity/golden_manifest.json"))?;
        checks.push(CheckResult::new(
            "golden_manifest",
            manifest.check(repo),
            Some(manifest.goldens.len()),
        ));
    }

    {
        let budgets = Budgets::load(&repo.join("parity/bench_budgets.json"))?;
        checks.push(CheckResult::new(
            "bench_budgets",
            budgets.check(repo),
            Some(budgets.entries.len()),
        ));
    }

    {
        let mcp = McpCatalog::load(&repo.join("parity/mcp_catalog.json"))?;
        let names = scenarios.names();
        checks.push(CheckResult::new(
            "mcp_catalog",
            mcp.check(&names),
            Some(mcp.entries.len()),
        ));
    }

    Ok(CheckOutcome { checks })
}

/// Builds the roll-up summary from the committed registries.
pub fn summary(repo: &Path) -> Result<Summary> {
    let mut summary = Summary::default();

    let rows = matrix_mod::load(&repo.join("parity/matrix.toml"))?;
    for row in &rows {
        *summary.matrix_status.entry(row.status.clone()).or_default() += 1;
        *summary.matrix_phase.entry(row.phase.clone()).or_default() += 1;
    }

    let scenarios = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;
    for entry in &scenarios.entries {
        *summary.scenario_kind.entry(entry.kind.clone()).or_default() += 1;
    }

    let budgets = budgets::Budgets::load(&repo.join("parity/bench_budgets.json"))?;
    for entry in &budgets.entries {
        *summary.budget_plan.entry(entry.plan.clone()).or_default() += 1;
    }

    let mcp = McpCatalog::load(&repo.join("parity/mcp_catalog.json"))?;
    for entry in &mcp.entries {
        *summary.mcp_phase.entry(entry.phase.clone()).or_default() += 1;
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_check_set_passes() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let outcome = collect(&repo, None).expect("collect");
        for check in &outcome.checks {
            assert!(
                check.pass(),
                "check `{}` failed: {:?}",
                check.name,
                check.problems
            );
        }
        assert!(outcome.pass());
    }

    #[test]
    fn summary_counts_are_nonzero() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let summary = summary(&repo).expect("summary");
        assert!(summary.matrix_status.values().sum::<usize>() > 0);
        assert!(summary.budget_plan.values().sum::<usize>() >= budgets::REQUIRED_PLANS.len());
    }
}
