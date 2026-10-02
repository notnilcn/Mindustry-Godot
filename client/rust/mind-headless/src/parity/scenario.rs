// SPDX-License-Identifier: GPL-3.0-only

//! `parity/scenario_catalog.json` (format 1): the golden-scenario catalog.
//! `kind = "file"` entries back a committed `scenarios/<name>.json`; every
//! committed scenario file must be catalogued, and the catalog's recorded
//! `expect_checksum` must match the scenario file.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// One catalogued scenario.
#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioEntry {
    /// Registered scenario name (`{system}_{case}`).
    pub name: String,
    /// Owning plan short number.
    pub plan: String,
    /// Phase key.
    pub phase: String,
    /// Tier (`T0`..`T3`).
    #[serde(default)]
    pub tier: String,
    /// `file | embedded | planned`.
    pub kind: String,
    /// Repo-relative scenario file (for `kind = "file"`).
    #[serde(default)]
    pub path: Option<String>,
    /// Expected final checksum recorded in the scenario file.
    #[serde(default)]
    pub expect_checksum: Option<String>,
    /// Producing command (for `kind = "embedded"`).
    #[serde(default)]
    pub command: Option<String>,
}

/// The parsed scenario catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioCatalog {
    /// File format version.
    pub format: u32,
    /// Entries.
    pub entries: Vec<ScenarioEntry>,
}

impl ScenarioCatalog {
    /// Loads the catalog.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))
    }

    /// The set of catalogued scenario names.
    pub fn names(&self) -> BTreeSet<&str> {
        self.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    /// Validates the catalog and every backed scenario file.
    pub fn check(&self, repo: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("scenario catalog: format {} != 1", self.format));
        }

        let mut names = BTreeSet::new();
        for entry in &self.entries {
            if !names.insert(entry.name.as_str()) {
                problems.push(format!("scenario catalog: duplicate `{}`", entry.name));
            }
            if entry.plan.is_empty() || entry.phase.is_empty() {
                problems.push(format!(
                    "scenario catalog: `{}` missing plan/phase",
                    entry.name
                ));
            }
            if !matches!(entry.kind.as_str(), "file" | "embedded" | "planned") {
                problems.push(format!(
                    "scenario catalog: `{}` unknown kind `{}`",
                    entry.name, entry.kind
                ));
            }
            if !entry.tier.is_empty() && !matches!(entry.tier.as_str(), "T0" | "T1" | "T2" | "T3") {
                problems.push(format!(
                    "scenario catalog: `{}` unknown tier `{}`",
                    entry.name, entry.tier
                ));
            }
            match entry.kind.as_str() {
                "file" => {
                    let Some(path) = &entry.path else {
                        problems.push(format!("scenario catalog: `{}` has no path", entry.name));
                        continue;
                    };
                    let full = repo.join(path);
                    if !full.is_file() {
                        problems.push(format!(
                            "scenario catalog: `{}` file {} is missing",
                            entry.name, path
                        ));
                        continue;
                    }
                    if let Err(error) = self.check_scenario_file(&entry.name, &full) {
                        problems.push(format!("scenario catalog: `{}`: {error:#}", entry.name));
                    }
                }
                "embedded" if entry.command.as_deref().unwrap_or_default().is_empty() => {
                    problems.push(format!(
                        "scenario catalog: `{}` embedded entry has no command",
                        entry.name
                    ));
                }
                _ => {}
            }
        }

        // Every committed scenario file must be catalogued.
        let scenarios_dir = repo.join("scenarios");
        if let Ok(read_dir) = std::fs::read_dir(&scenarios_dir) {
            for path in read_dir.flatten().map(|entry| entry.path()) {
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if !names.contains(stem) {
                    problems.push(format!(
                        "scenario catalog: committed scenarios/{stem}.json is not catalogued"
                    ));
                }
            }
        }

        problems.sort();
        problems.dedup();
        problems
    }

    fn check_scenario_file(&self, name: &str, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))?;

        let file_name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if file_name != name {
            anyhow::bail!("scenario file declares name `{file_name}` != catalog `{name}`");
        }
        let checksum = value
            .get("expect")
            .and_then(|expect| expect.get("checksum"))
            .and_then(|v| v.as_str());
        let Some(entry) = self.entries.iter().find(|entry| entry.name == name) else {
            anyhow::bail!("catalog has no entry named `{name}`");
        };
        match (checksum, entry.expect_checksum.as_deref()) {
            (Some(actual), Some(expected)) if actual != expected => {
                anyhow::bail!("expect.checksum `{actual}` != catalog `{expected}`");
            }
            (Some(actual), None) => {
                anyhow::bail!("catalog is missing expect_checksum `{actual}`");
            }
            (None, Some(expected)) => {
                anyhow::bail!("catalog records expect_checksum `{expected}` but the file has none");
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_catalog_is_consistent() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let catalog =
            ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json")).expect("catalog");
        let problems = catalog.check(&repo);
        assert!(problems.is_empty(), "scenario problems: {problems:?}");
        assert!(catalog.names().contains("spine_place_break"));
    }

    #[test]
    fn uncatalogued_file_is_flagged() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let mut catalog =
            ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json")).expect("catalog");
        catalog
            .entries
            .retain(|entry| entry.name != "stdb_offline_boot");
        assert!(
            catalog
                .check(&repo)
                .iter()
                .any(|p| p.contains("stdb_offline_boot"))
        );
    }
}
