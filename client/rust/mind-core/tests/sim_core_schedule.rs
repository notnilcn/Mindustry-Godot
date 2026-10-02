// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 05 M6 integration oracle: the deterministic schedule order must match
//! `tests/golden/sim_core_schedule_order.txt` for menu/paused/playing/editor/
//! client run conditions (plan 05 §7.2 `trace order`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use mind_core::sim::schedule::{RunContext, render_trace};

#[test]
fn trace_order_matches_golden() {
    let live = render_trace(&[
        ("playing", RunContext::playing_headless()),
        ("menu", RunContext::menu()),
        ("paused", RunContext::paused()),
        ("editor", RunContext::editor()),
        ("client", RunContext::client()),
    ]);
    let golden = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/sim_core_schedule_order.txt"
    ))
    .expect("golden schedule order is committed");
    assert_eq!(
        live, golden,
        "schedule order drifted; regenerate with `mind-headless trace order --out`"
    );
}
