// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 16 §7.2 committed render-list/scenario goldens.
//!
//! Runs each `render list` scenario through the compiled `mind-headless` binary
//! and asserts the produced list compares clean against `tests/golden/render/`.

#![allow(clippy::expect_used)]

use std::process::Command;

fn golden(name: &str) -> String {
    format!("{}/tests/golden/render/{name}", env!("CARGO_MANIFEST_DIR"))
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
fn render_scenarios_match_committed_goldens() {
    for scenario in [
        "render_flat_floor",
        "render_block_change",
        "render_layer_order",
        "render_darkness_radius",
        "render_menu_world",
        "render_layers_full",
    ] {
        run(&[
            "render",
            "list",
            scenario,
            "--check",
            &golden(&format!("{scenario}.json")),
            "--json",
        ]);
    }
}

#[test]
fn render_no_sort_emission_order_matches_golden() {
    run(&[
        "render",
        "list",
        "render_layer_order_nosort",
        "--no-sort",
        "--check",
        &golden("render_layer_order_nosort.json"),
        "--json",
    ]);
}
