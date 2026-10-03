// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 08 M8 committed logistics-scenario goldens.
//!
//! Runs every `logistics_*` block scenario through the compiled
//! `mind-headless` binary and asserts the deterministic FNV-1a checksum
//! against `tests/golden/logistics.json` (plan 08 §6.6/§7.2).

#![allow(clippy::expect_used)]

use std::path::Path;
use std::process::Command;

const SCENARIOS: [&str; 11] = [
    "logistics_smoke",
    "logistics_conveyor_lane",
    "logistics_router_fairness",
    "logistics_bridge_latency",
    "logistics_mass_driver_roundtrip",
    "logistics_unloader_drain",
    "logistics_core_inventory",
    "logistics_payload_move",
    "logistics_payload_load_unload",
    "logistics_payload_fluids",
    "logistics_payload_driver_throw",
];

fn golden() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/logistics.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).expect("parse logistics golden")
}

#[test]
fn logistics_scenario_checksums_match_golden() {
    let golden = golden();
    let exe = env!("CARGO_BIN_EXE_mind-headless");
    for name in SCENARIOS {
        let output = Command::new(exe)
            .args(["blocks", "scenario", name, "--json", "--log-file", "-"])
            .output()
            .unwrap_or_else(|error| panic!("{name}: spawn {error}"));
        assert!(
            output.status.success(),
            "{name} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "{name}: bad json {error}: {}",
                    String::from_utf8_lossy(&output.stdout)
                )
            });
        assert_eq!(
            report["pass"].as_bool(),
            Some(true),
            "{name}: scenario reported failure"
        );
        assert_eq!(
            report["checksum"].as_str(),
            golden[name].as_str(),
            "checksum mismatch for {name}"
        );
    }
}
