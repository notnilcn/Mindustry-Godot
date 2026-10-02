// SPDX-License-Identifier: GPL-3.0-only

//! `parity/mcp_catalog.json` (format 1): the in-engine playtest queue.
//! This module documents/validates the catalog; it never drives the editor.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// One MCP catalog entry.
#[derive(Debug, Clone, Deserialize)]
pub struct McpEntry {
    /// Stable entry id.
    pub id: String,
    /// Phase key.
    pub phase: String,
    /// Owning plan short number.
    pub plan: String,
    /// Godot scene path.
    pub scene: String,
    /// Backing headless scenario (if any).
    #[serde(default)]
    pub scenario: Option<String>,
    /// Ordered recipe steps.
    #[serde(default)]
    pub steps: Vec<String>,
    /// Expected assertions.
    #[serde(default)]
    pub asserts: Vec<String>,
    /// Screenshot outputs.
    #[serde(default)]
    pub screenshots: Vec<String>,
}

/// The parsed MCP catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct McpCatalog {
    /// File format version.
    pub format: u32,
    /// Entries.
    pub entries: Vec<McpEntry>,
}

impl McpCatalog {
    /// Loads the catalog.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))
    }

    /// Validates the catalog; `known_scenarios` is the scenario-catalog name set.
    pub fn check(&self, known_scenarios: &BTreeSet<&str>) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("mcp catalog: format {} != 1", self.format));
        }

        let mut ids = BTreeSet::new();
        let mut shots = BTreeSet::new();
        for entry in &self.entries {
            if !ids.insert(entry.id.as_str()) {
                problems.push(format!("mcp catalog: duplicate id `{}`", entry.id));
            }
            if entry.phase.is_empty() || entry.plan.is_empty() {
                problems.push(format!("mcp catalog: `{}` missing phase/plan", entry.id));
            }
            if entry.scene.is_empty() {
                problems.push(format!("mcp catalog: `{}` has an empty scene", entry.id));
            }
            if entry.steps.is_empty() {
                problems.push(format!("mcp catalog: `{}` has no steps", entry.id));
            }
            for screenshot in &entry.screenshots {
                if !shots.insert(screenshot.as_str()) {
                    problems.push(format!(
                        "mcp catalog: screenshot `{screenshot}` used by more than one entry"
                    ));
                }
            }
            if let Some(scenario) = &entry.scenario
                && !known_scenarios.contains(scenario.as_str())
            {
                problems.push(format!(
                    "mcp catalog: `{}` references unknown scenario `{scenario}`",
                    entry.id
                ));
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::scenario::ScenarioCatalog;

    #[test]
    fn committed_mcp_catalog_is_consistent() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let catalog = McpCatalog::load(&repo.join("parity/mcp_catalog.json")).expect("mcp catalog");
        let scenarios =
            ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json")).expect("scenarios");
        let names = scenarios.names();
        let problems = catalog.check(&names);
        assert!(problems.is_empty(), "mcp problems: {problems:?}");
    }

    #[test]
    fn unknown_scenario_is_flagged() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let mut catalog =
            McpCatalog::load(&repo.join("parity/mcp_catalog.json")).expect("mcp catalog");
        catalog.entries[0].scenario = Some(String::from("not_a_scenario"));
        let empty = BTreeSet::new();
        assert!(
            catalog
                .check(&empty)
                .iter()
                .any(|p| p.contains("unknown scenario"))
        );
    }
}
