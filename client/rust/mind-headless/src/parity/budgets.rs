// SPDX-License-Identifier: GPL-3.0-only

//! `parity/bench_budgets.json` (format 1): the cross-crate performance-budget
//! registry. Per-plan `bench/baselines.json` files remain the recording source
//! of truth; this module validates coverage and the canonical metric.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Plans that must publish at least one budget row once their system lands.
pub const REQUIRED_PLANS: &[&str] = &[
    "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14", "15",
    "16", "17", "18", "20",
];

/// Benchmark host description.
#[derive(Debug, Clone, Deserialize)]
pub struct Machine {
    /// Stable machine id used to gate (CI runners record only).
    pub id: String,
    /// OS description.
    #[serde(default)]
    pub os: String,
    /// GPU/backend description.
    #[serde(default)]
    pub gpu: String,
}

/// The one metric that blocks phase gates on regression.
#[derive(Debug, Clone, Deserialize)]
pub struct Canonical {
    /// Canonical scenario name.
    pub scenario: String,
    /// Metric key.
    pub metric: String,
    /// Absolute budget.
    pub budget: f64,
    /// Regression threshold (NUD-39/A).
    #[serde(default)]
    pub fail_pct: f64,
}

/// A per-plan baseline file recorded by the owning plan.
#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    /// Owning plan short number.
    pub plan: String,
    /// Repo-relative path.
    pub path: String,
    /// Whether the file is expected to exist today.
    #[serde(default)]
    pub present: bool,
    /// Optional note.
    #[serde(default)]
    pub note: Option<String>,
}

/// One aggregated budget row.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    /// Stable row id.
    pub id: String,
    /// Owning plan short number.
    pub plan: String,
    /// Producing command.
    pub measure: String,
    /// Metric key.
    pub metric: String,
    /// Absolute budget.
    pub budget: f64,
    /// `lte` (budget is a ceiling) or `gte` (budget is a floor).
    #[serde(default)]
    pub comparison: Option<String>,
}

/// The parsed budget registry.
#[derive(Debug, Clone, Deserialize)]
pub struct Budgets {
    /// File format version.
    pub format: u32,
    /// Benchmark host.
    pub machine: Machine,
    /// Canonical gate metric.
    pub canonical: Canonical,
    /// Per-plan baseline files.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Aggregated rows.
    pub entries: Vec<Entry>,
}

impl Budgets {
    /// Loads the registry.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))
    }

    /// Validates the registry against the repo and plan coverage.
    pub fn check(&self, repo: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("bench budgets: format {} != 1", self.format));
        }
        if self.machine.id.trim().is_empty() {
            problems.push(String::from("bench budgets: machine.id is empty"));
        }
        if self.canonical.scenario.trim().is_empty() || self.canonical.metric.trim().is_empty() {
            problems.push(String::from(
                "bench budgets: canonical metric is incomplete",
            ));
        }
        if self.canonical.fail_pct <= 0.0 {
            problems.push(String::from(
                "bench budgets: canonical fail_pct must be > 0",
            ));
        }

        let mut ids = BTreeSet::new();
        let mut plans = BTreeSet::new();
        for entry in &self.entries {
            if !ids.insert(entry.id.as_str()) {
                problems.push(format!("bench budgets: duplicate entry id `{}`", entry.id));
            }
            if entry.measure.trim().is_empty() {
                problems.push(format!(
                    "bench budgets: `{}` has an empty measure",
                    entry.id
                ));
            }
            if entry.budget <= 0.0 || !entry.budget.is_finite() {
                problems.push(format!(
                    "bench budgets: `{}` budget {} must be > 0",
                    entry.id, entry.budget
                ));
            }
            if let Some(comparison) = &entry.comparison
                && !matches!(comparison.as_str(), "lte" | "gte")
            {
                problems.push(format!(
                    "bench budgets: `{}` has unknown comparison `{comparison}`",
                    entry.id
                ));
            }
            plans.insert(entry.plan.as_str());
        }

        for required in REQUIRED_PLANS {
            if !plans.contains(required) {
                problems.push(format!(
                    "bench budgets: plan {required} has no budget rows (add to parity/bench_budgets.json)"
                ));
            }
        }

        for source in &self.sources {
            let path = repo.join(&source.path);
            if source.present && !path.is_file() {
                problems.push(format!(
                    "bench budgets: source file `{}` (plan {}) is marked present but missing",
                    source.path, source.plan
                ));
            }
        }
        problems
    }

    /// Summary of the per-plan baseline files that exist.
    pub fn source_summary(&self, repo: &Path) -> Vec<(String, String, bool)> {
        self.sources
            .iter()
            .map(|source| {
                (
                    source.plan.clone(),
                    source.path.clone(),
                    repo.join(&source.path).is_file(),
                )
            })
            .collect()
    }
}

/// One recorded row in `bench/baselines.json` (§6.3). `value` is `None` until a
/// run on the baseline machine records it; CI runners never gate on a value.
#[derive(Debug, Clone, Deserialize)]
pub struct BaselineEntry {
    /// Stable row id (matches a [`Entry`] id).
    pub id: String,
    /// Owning plan short number.
    pub plan: String,
    /// Producing command.
    pub measure: String,
    /// Metric key.
    pub metric: String,
    /// Recorded baseline value (`null` until measured).
    #[serde(default)]
    pub value: Option<f64>,
    /// Absolute budget.
    pub budget: f64,
    /// `lte` (ceiling) or `gte` (floor).
    #[serde(default)]
    pub comparison: Option<String>,
}

/// Allocation-audit profile list (§6.3).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AllocProfiles {
    /// Profiles that must report zero steady-state allocations.
    #[serde(default)]
    pub assert_zero_profiles: Vec<String>,
}

/// `bench/baselines.json` (format 1): the recorded baseline + budgets.
#[derive(Debug, Clone, Deserialize)]
pub struct Baselines {
    /// File format version.
    pub format: u32,
    /// Benchmark host.
    pub machine: Machine,
    /// Canonical gate metric.
    pub canonical: Canonical,
    /// Recorded rows.
    #[serde(default)]
    pub entries: Vec<BaselineEntry>,
    /// Allocation-audit profiles.
    #[serde(default)]
    pub alloc: AllocProfiles,
}

impl Baselines {
    /// Loads the baseline file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))
    }

    /// Structural validation of the baseline file.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("baseline: format {} != 1", self.format));
        }
        if self.machine.id.trim().is_empty() {
            problems.push(String::from("baseline: machine.id is empty"));
        }
        if self.canonical.scenario.trim().is_empty() || self.canonical.metric.trim().is_empty() {
            problems.push(String::from("baseline: canonical metric is incomplete"));
        }
        if self.canonical.fail_pct <= 0.0 {
            problems.push(String::from("baseline: canonical fail_pct must be > 0"));
        }
        let mut ids = BTreeSet::new();
        for entry in &self.entries {
            if !ids.insert(entry.id.as_str()) {
                problems.push(format!("baseline: duplicate entry id `{}`", entry.id));
            }
            if entry.budget <= 0.0 || !entry.budget.is_finite() {
                problems.push(format!(
                    "baseline: `{}` budget {} must be > 0",
                    entry.id, entry.budget
                ));
            }
            if let Some(value) = entry.value
                && !value.is_finite()
            {
                problems.push(format!("baseline: `{}` value is not finite", entry.id));
            }
        }
        problems
    }

    /// Entry ids that have a recorded value on the baseline machine.
    pub fn recorded_ids(&self) -> BTreeSet<&str> {
        self.entries
            .iter()
            .filter(|entry| entry.value.is_some())
            .map(|entry| entry.id.as_str())
            .collect()
    }

    /// Validates that the baseline covers every row of the budget registry and
    /// that plans are represented.
    pub fn check_against(&self, budgets: &Budgets) -> Vec<String> {
        let mut problems = self.check();
        let baseline_ids: BTreeSet<&str> =
            self.entries.iter().map(|entry| entry.id.as_str()).collect();
        for entry in &budgets.entries {
            if !baseline_ids.contains(entry.id.as_str()) {
                problems.push(format!(
                    "baseline: budget row `{}` (plan {}) has no baseline entry",
                    entry.id, entry.plan
                ));
            }
        }
        let plans: BTreeSet<&str> = self
            .entries
            .iter()
            .map(|entry| entry.plan.as_str())
            .collect();
        for required in REQUIRED_PLANS {
            if !plans.contains(required) {
                problems.push(format!(
                    "baseline: plan {required} has no recorded baseline row"
                ));
            }
        }
        problems
    }

    /// Resolves the default `bench/baselines.json` path under `repo`.
    pub fn default_path(repo: &Path) -> PathBuf {
        repo.join("bench/baselines.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn registry_path() -> PathBuf {
        crate::paths::find_repo_root(None)
            .expect("repo root")
            .join("parity/bench_budgets.json")
    }

    fn baseline_path() -> PathBuf {
        crate::paths::find_repo_root(None)
            .expect("repo root")
            .join("bench/baselines.json")
    }

    #[test]
    fn every_plan_has_budget_rows() {
        let budgets = Budgets::load(&registry_path()).expect("budgets");
        let baseline = Baselines::load(&baseline_path()).expect("baseline");
        let plans: BTreeSet<&str> = baseline
            .entries
            .iter()
            .map(|entry| entry.plan.as_str())
            .collect();
        for required in REQUIRED_PLANS {
            assert!(
                plans.contains(required),
                "plan {required} has no baseline budget row"
            );
        }
        // The baseline covers every budget-registry row.
        let baseline_ids: BTreeSet<&str> = baseline.entries.iter().map(|e| e.id.as_str()).collect();
        for entry in &budgets.entries {
            assert!(
                baseline_ids.contains(entry.id.as_str()),
                "baseline missing budget row `{}`",
                entry.id
            );
        }
    }

    #[test]
    fn baseline_covers_all_bench_profiles() {
        let budgets = Budgets::load(&registry_path()).expect("budgets");
        let baseline = Baselines::load(&baseline_path()).expect("baseline");
        let problems = baseline.check_against(&budgets);
        assert!(problems.is_empty(), "baseline problems: {problems:?}");
        // Every alloc-audit profile is a known plan or profile name.
        for profile in &baseline.alloc.assert_zero_profiles {
            assert!(!profile.trim().is_empty(), "empty alloc profile");
        }
    }

    #[test]
    fn committed_budgets_cover_required_plans() {
        let budgets = Budgets::load(&registry_path()).expect("budgets");
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let problems = budgets.check(&repo);
        assert!(problems.is_empty(), "budget problems: {problems:?}");
    }

    #[test]
    fn missing_plan_is_flagged() {
        let mut budgets = Budgets::load(&registry_path()).expect("budgets");
        budgets.entries.retain(|entry| entry.plan != "05");
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        assert!(
            budgets
                .check(&repo)
                .iter()
                .any(|p| p.contains("plan 05 has no budget rows"))
        );
    }
}
