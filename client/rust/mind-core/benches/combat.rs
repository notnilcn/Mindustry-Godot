// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Combat budgets (plan 10 §7d): `cargo bench -p mind-core --bench combat`.
//!
//! Benchmarks the full combat tick (turrets + defense + bullets) at the
//! recorded `2_000` bullets / `400` turrets profile. The headless
//! `mind-headless combat bench` command records p50/p95/p99 for CI; this is the
//! criterion cross-check.
//!
//! Bench code must not `unwrap`/`expect` (workspace lints).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mind_core::combat::CombatHarness;
use mind_core::world::blocks::defense::turrets;

/// Builds the recorded combat profile: a wall row to shoot, `turrets` firing
/// `test-item` fixtures, then `bullets` live `fuse` projectiles.
fn build_harness(bullets: usize, turrets: usize) -> CombatHarness {
    let mut harness = CombatHarness::new(128, 128, 7);
    if let Some(wall) = harness.content().block_id("copper-wall") {
        for tx in 2..60 {
            let _ = harness.place(tx, 60, wall, 0, true);
        }
    }
    for i in 0..turrets {
        let tx = 2 + (i % 58) as i32;
        let ty = 4 + (i / 58) as i32 * 2;
        let (x, y) = CombatHarness::tile_center(tx, ty);
        if let Some(turret) = harness.spawn_test_turret("test-item", x, y, 1)
            && let Some(copper) = harness.content().item_id("copper")
        {
            for _ in 0..30 {
                turrets::handle_item(&mut harness.build.world, turret, copper);
            }
        }
    }
    for i in 0..bullets {
        let (x, y) = CombatHarness::tile_center(2 + (i % 60) as i32, 2 + (i / 60) as i32 % 60);
        let angle = (i as f32 * 13.0) % 360.0;
        let _ = harness.spawn_bullet("fuse", x, y, angle, 2);
    }
    harness
}

fn combat_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("combat");
    group.sample_size(20);
    group.bench_function("bullets_2000_turrets_400", |b| {
        let mut harness = build_harness(2_000, 400);
        b.iter(|| {
            harness.tick();
            black_box(harness.bullets_live());
        });
    });
    group.finish();
}

criterion_group!(benches, combat_tick);
criterion_main!(benches);
