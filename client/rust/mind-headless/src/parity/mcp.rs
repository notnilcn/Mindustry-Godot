// SPDX-License-Identifier: GPL-3.0-only

//! `parity/mcp_catalog.json` (format 1): the in-engine playtest queue.
//! This module documents/validates the catalog; it never drives the editor.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::scenario::ScenarioCatalog;

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

/// The headless half of `parity mcp-parity`.
///
/// For the requested suite (`T0`/`T1`/`T2` by scenario tier, `P0`..`P8` by
/// catalog phase, or `all`) it resolves every catalogued MCP scenario against
/// its committed golden. The in-engine capture comparison is reported
/// `deferred` (editor-gated); `pass` reflects the headless half.
pub fn parity_report(
    mcp: &McpCatalog,
    scenarios: &ScenarioCatalog,
    suite: &str,
) -> serde_json::Value {
    let mut entries = Vec::new();
    let mut pass = true;
    for entry in &mcp.entries {
        let scenario = entry
            .scenario
            .as_deref()
            .and_then(|name| scenarios.entries.iter().find(|s| s.name == name));
        let matches = if suite == "all" {
            true
        } else if suite.starts_with('T') {
            scenario.is_some_and(|s| s.tier == suite)
        } else {
            entry.phase == suite
        };
        if !matches {
            continue;
        }
        let (headless_source, golden) = match scenario {
            None if entry.scenario.is_none() => ("catalog-only", None),
            None => ("unknown-scenario", None),
            Some(s) => match s.kind.as_str() {
                "file" => ("golden", s.expect_checksum.clone()),
                "embedded" => ("command", s.command.clone()),
                "planned" => ("planned", None),
                _ => ("unknown-kind", None),
            },
        };
        let headless_ok = match headless_source {
            // A catalog entry with no headless scenario, or one whose scenario
            // is still `planned`, has no golden to resolve yet: it is deferred,
            // not a failure (the in-engine half is editor-gated regardless).
            "catalog-only" | "planned" => true,
            "golden" | "command" => golden.is_some(),
            _ => false,
        };
        if !headless_ok {
            pass = false;
        }
        entries.push(serde_json::json!({
            "id": entry.id,
            "plan": entry.plan,
            "phase": entry.phase,
            "scenario": entry.scenario,
            "golden": golden,
            "headless_source": headless_source,
            "headless_pass": headless_ok,
            "in_engine": "deferred",
            "in_engine_owner": "single-editor MCP mutex (NUD-40/A)",
        }));
    }
    serde_json::json!({
        "format": 1,
        "pass": pass,
        "suite": suite,
        "entries": entries.len(),
        "checks": entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn catalog_with(kind: &str) -> (McpCatalog, ScenarioCatalog) {
        use crate::parity::scenario::ScenarioEntry;
        let mcp = McpCatalog {
            format: 1,
            entries: vec![McpEntry {
                id: String::from("mcp_test"),
                phase: String::from("P0"),
                plan: String::from("00"),
                scene: String::from("res://scenes/spine.tscn"),
                scenario: Some(String::from("test_scenario")),
                steps: vec![String::from("load_scenario")],
                asserts: Vec::new(),
                screenshots: Vec::new(),
            }],
        };
        let scenarios = ScenarioCatalog {
            format: 1,
            entries: vec![ScenarioEntry {
                name: String::from("test_scenario"),
                plan: String::from("00"),
                phase: String::from("P0"),
                tier: String::from("T0"),
                kind: kind.to_owned(),
                path: None,
                expect_checksum: None,
                command: None,
            }],
        };
        (mcp, scenarios)
    }

    #[test]
    fn committed_catalog_headless_parity_is_green() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let mcp = McpCatalog::load(&repo.join("parity/mcp_catalog.json")).expect("mcp catalog");
        let scenarios =
            ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json")).expect("scenarios");
        let report = parity_report(&mcp, &scenarios, "T0");
        assert!(report["pass"].as_bool().unwrap_or(false), "{report}");
        assert!(report["entries"].as_u64().unwrap_or(0) > 0);
    }

    #[test]
    fn planned_scenario_is_deferred_not_failed() {
        let (mcp, scenarios) = catalog_with("planned");
        let report = parity_report(&mcp, &scenarios, "all");
        assert!(report["pass"].as_bool().unwrap_or(false), "{report}");
        assert_eq!(report["checks"][0]["headless_source"], "planned");
        assert_eq!(report["checks"][0]["headless_pass"], true);
    }

    #[test]
    fn embedded_scenario_resolves_through_its_command() {
        let (mcp, mut scenarios) = catalog_with("embedded");
        scenarios.entries[0].command = Some(String::from("logic run test_scenario --json"));
        let report = parity_report(&mcp, &scenarios, "all");
        assert!(report["pass"].as_bool().unwrap_or(false), "{report}");
        assert_eq!(report["checks"][0]["headless_source"], "command");
    }

    #[test]
    fn embedded_scenario_without_a_command_fails() {
        let (mcp, scenarios) = catalog_with("embedded");
        let report = parity_report(&mcp, &scenarios, "all");
        assert!(!report["pass"].as_bool().unwrap_or(false), "{report}");
    }

    #[test]
    fn unknown_scenario_fails_the_headless_half() {
        let (mut mcp, scenarios) = catalog_with("file");
        mcp.entries[0].scenario = Some(String::from("missing_scenario"));
        let report = parity_report(&mcp, &scenarios, "all");
        assert!(!report["pass"].as_bool().unwrap_or(false), "{report}");
        assert_eq!(report["checks"][0]["headless_source"], "unknown-scenario");
    }
}
