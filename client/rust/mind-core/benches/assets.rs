// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Asset budgets (plan 03 §7d): `cargo bench -p mind-core --bench assets`.
//!
//! `atlas_lookup`, `bundle_get` and `bundle_format` are the §7d.1 micro-rows;
//! the full cold-pack and runtime-assets-ready budgets are measured by
//! `mind-tools pack --timings` and the in-engine `AssetsReadyEvent`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mind_core::assets::atlas::AtlasIndex;
use mind_core::assets::bundle::{Bundle, parse_properties};

/// Panics with the debug error on failure (benches cannot use `unwrap`).
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("bench setup failed: {error:?}"),
    }
}

/// Builds a 5000-region manifest (roughly the vanilla atlas order of magnitude).
fn manifest(regions: usize) -> String {
    let mut body = String::new();
    for i in 0..regions {
        if i > 0 {
            body.push(',');
        }
        body.push_str(&format!(
            r#"{{"name":"region-{i}","page":0,"x":0,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"}}"#
        ));
    }
    format!(
        r#"{{"format":1,"pages":[{{"index":0,"type":"main","file":"sprites.png","width":4096,"height":4096,"sha256":""}}],"regions":[{body}]}}"#
    )
}

/// Builds a bundle with `keys` entries plus a format key.
fn bundle(keys: usize) -> Bundle {
    let mut text = String::new();
    for i in 0..keys {
        text.push_str(&format!("key-{i}=value {i}\n"));
    }
    text.push_str("greet=Hello {0}\n");
    Bundle::from_layers(vec![parse_properties(&text)])
}

fn asset_benches(c: &mut Criterion) {
    let index = ok(AtlasIndex::from_manifest_json(&manifest(5000)));
    let bundle = bundle(5000);

    let mut group = c.benchmark_group("assets");
    group.bench_function("atlas_lookup", |b| {
        b.iter(|| {
            let hit = index.find(black_box("region-4321")).is_some();
            let miss = index.find(black_box("region-missing")).is_none();
            black_box(hit && miss)
        });
    });
    group.bench_function("bundle_get", |b| {
        b.iter(|| black_box(bundle.get(black_box("key-4321"))));
    });
    group.bench_function("bundle_format", |b| {
        b.iter(|| black_box(bundle.format(black_box("greet"), &["world"])));
    });
    group.finish();
}

criterion_group!(benches, asset_benches);
criterion_main!(benches);
