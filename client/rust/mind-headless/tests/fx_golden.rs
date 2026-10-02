// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 17 §7b committed FX-scenario goldens.
//!
//! Runs the `fx` oracle through the compiled `mind-headless` binary and asserts
//! the lifecycle/program/trail traces compare clean against
//! `tests/golden/fx/`.

#![allow(clippy::expect_used)]

use std::process::Command;

fn golden(name: &str) -> String {
    format!("{}/tests/golden/fx/{}", env!("CARGO_MANIFEST_DIR"), name)
}

fn run(args: &[&str]) {
    let exe = env!("CARGO_BIN_EXE_mind-headless");
    let output = Command::new(exe)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("{args:?}: spawn {error}"));
    assert!(
        output.status.success(),
        "{args:?} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fx_scenarios_match_committed_goldens() {
    let lifecycle = golden("fx_lifecycle.json");
    let hit = golden("fx_program_hitBulletSmall.json");
    let explosion = golden("fx_program_explosion.json");
    let trail = golden("fx_trail.json");
    run(&[
        "fx",
        "lifecycle",
        "--ticks",
        "120",
        "--check",
        &lifecycle,
        "--json",
    ]);
    run(&[
        "fx",
        "program",
        "hitBulletSmall",
        "--tick",
        "3",
        "--check",
        &hit,
        "--json",
    ]);
    run(&[
        "fx",
        "program",
        "explosion",
        "--tick",
        "3",
        "--check",
        &explosion,
        "--json",
    ]);
    run(&["fx", "trail", "--check", &trail, "--json"]);
}

#[test]
fn fx_audit_and_smoke_pass() {
    run(&["fx", "audit", "--json"]);
    run(&["fx", "smoke", "--json"]);
}
