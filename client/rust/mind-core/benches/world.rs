// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! World-generation budgets (plan 06 §7d): `cargo bench -p mind-core --bench world`.
//!
//! Full `generate()` per vanilla planet generator at the §7d sizes (256×256 for
//! Serpulo/Erekir/Tantros, 500×500 for Asteroid). Per-phase attribution
//! (noise/`pass`/filters/darkness) is a follow-up; the headless
//! `world bench-gen` command records p50/p95 for CI.
//!
//! Bench code must not `unwrap`/`expect` (workspace lints); every fallible
//! setup step goes through [`ok`].

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mind_core::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
use mind_core::maps::generators::WorldGenerator;
use mind_core::maps::planet::{
    AsteroidGenerator, ErekirPlanetGenerator, SerpuloPlanetGenerator, TantrosPlanetGenerator,
};
use mind_core::world::{Tiles, WorldParams};

/// Panics with the debug error on failure (benches cannot use `unwrap`).
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("bench setup failed: {error:?}"),
    }
}

fn boot() -> ContentRegistry {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = ok(create_base_content(&bundle, &store, true));
    ok(registry.init());
    ok(registry.post_init());
    registry
}

fn generate(name: &str, size: i32, seed: u64, content: &ContentRegistry) -> usize {
    let mut tiles = Tiles::new(size, size);
    let params = WorldParams {
        seed_offset: seed,
        width: size,
        height: size,
        ..WorldParams::default()
    };
    match name {
        "serpulo" => SerpuloPlanetGenerator::new().generate(&mut tiles, &params, content),
        "erekir" => ErekirPlanetGenerator::new().generate(&mut tiles, &params, content),
        "tantros" => TantrosPlanetGenerator::new().generate(&mut tiles, &params, content),
        "asteroid" => AsteroidGenerator::new(seed as i32).generate(&mut tiles, &params, content),
        other => panic!("unknown bench generator `{other}`"),
    }
    tiles.len()
}

fn world_gen(c: &mut Criterion) {
    let content = boot();
    let mut group = c.benchmark_group("world_gen");
    group.sample_size(10);
    for (name, size, seed) in [
        ("serpulo", 256, 42_u64),
        ("erekir", 256, 7),
        ("tantros", 256, 11),
        ("asteroid", 500, 7),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(generate(name, size, seed, &content)));
        });
    }
    group.finish();
}

criterion_group!(benches, world_gen);
criterion_main!(benches);
