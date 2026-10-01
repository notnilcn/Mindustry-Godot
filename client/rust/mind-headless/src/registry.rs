// SPDX-License-Identifier: GPL-3.0-only

//! Registered scenario fixtures. Canonical JSON lives in repo-root `scenarios/`.
//!
//! Every plan contributes scenarios named `{system}_{case}` (§7b); this registry is
//! the Rust side of `mind-headless list`/`run`.

/// One registered scenario.
#[derive(Debug, Clone, Copy)]
pub struct ScenarioFixture {
    /// Registered name (`{system}_{case}`).
    pub name: &'static str,
    /// Canonical file name inside `scenarios/`.
    pub file: &'static str,
}

impl ScenarioFixture {
    /// Canonical path name for this fixture.
    pub fn file_name(&self) -> &'static str {
        self.file
    }
}

/// All P0 scenarios (append-only).
pub static SCENARIOS: &[ScenarioFixture] = &[
    ScenarioFixture {
        name: "spine_place_break",
        file: "spine_place_break.json",
    },
    ScenarioFixture {
        name: "spine_determinism",
        file: "spine_determinism.json",
    },
    ScenarioFixture {
        name: "spine_many_commands",
        file: "spine_many_commands.json",
    },
    ScenarioFixture {
        name: "bench_baseline",
        file: "bench_baseline.json",
    },
    ScenarioFixture {
        name: "stdb_offline_boot",
        file: "stdb_offline_boot.json",
    },
    ScenarioFixture {
        name: "stdb_binder_replay",
        file: "stdb_binder_replay.json",
    },
];

/// Registered scenario names.
pub fn names() -> impl Iterator<Item = &'static str> {
    SCENARIOS.iter().map(|fixture| fixture.name)
}

/// Looks up a registered scenario by name.
pub fn find(name: &str) -> Option<&'static ScenarioFixture> {
    SCENARIOS.iter().find(|fixture| fixture.name == name)
}

/// Resolves `bench --scenario` aliases (`spine` profiles map to canonical files).
pub fn bench_alias(name: &str) -> &str {
    match name {
        "spine" => "bench_baseline",
        other => other,
    }
}
