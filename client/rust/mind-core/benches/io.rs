// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! IO budgets (plan 04 §7d): `cargo bench -p mind-core --bench io`.
//!
//! The synthetic 64×64 `FixtureWorld` stands in for the §7d
//! `serpulo/groundZero` baseline until plan 06 lands real maps; the note in
//! the plan Changelog records byte ranges from the same run. Save/load write
//! to `MockFs` (serialize + deflate + in-memory write) so the numbers isolate
//! the codec; the headless `io bench-save` command measures the same phases
//! over `NativeFs` with P50/P95.
//!
//! Bench code must not `unwrap`/`expect` (workspace lints), so every fallible
//! setup step goes through [`ok`].

use std::cell::RefCell;
use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};

use mind_core::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
use mind_core::io::fs::{FileSystem, MockFs, Paths};
use mind_core::io::save::fixture::{FixtureContext, FixtureSink, FixtureWorld};
use mind_core::io::save::slot::list_files_meta;
use mind_core::io::save::versions::v1::base_meta_tags;
use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
use mind_core::io::settings::SettingsStore;
use mind_core::io::typeio::TypeValue;
use mind_core::io::typeio::codecs::{BuildPlan, write_plan};
use mind_core::io::wire::WireWriter;

/// Panics with the debug error on failure (benches cannot use `unwrap`).
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("bench setup failed: {error:?}"),
    }
}

/// Boots base content exactly like the headless harness (`boot_content`).
fn boot_content() -> ContentRegistry {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = ok(create_base_content(&bundle, &store, true));
    ok(registry.init());
    ok(registry.post_init());
    ok(registry.load());
    registry
}

fn io_benches(c: &mut Criterion) {
    let registry = boot_content();
    let mut world = FixtureWorld::synthetic(&registry, 64, 64);
    for _ in 0..600 {
        world.tick();
    }
    let mut tags = base_meta_tags(64, 64, world.wave, "synthetic");
    tags.insert("tick".to_owned(), world.tick.to_string());

    let fs = MockFs::new();
    let path = PathBuf::from("/bench/io.msav");
    let backup = SaveIo::backup_file_for(&path);
    let mut ctx = WriteContext::meta_only(tags.clone());
    ctx.content = Some(&registry);
    ctx.map = Some(&world);
    ctx.entities = Some(&world);
    ok(SaveIo::save(&fs, &path, &ctx, &SaveOptions::new()));

    let mut group = c.benchmark_group("io");
    group.bench_function("save_synthetic_64x64_600t", |b| {
        b.iter(|| {
            let _ = fs.delete(&path);
            let _ = fs.delete(&backup);
            let mut ctx = WriteContext::meta_only(tags.clone());
            ctx.content = Some(&registry);
            ctx.map = Some(&world);
            ctx.entities = Some(&world);
            ok(SaveIo::save(&fs, &path, &ctx, &SaveOptions::new()));
        });
    });

    let mut load_registry = boot_content();
    group.bench_function("load_synthetic_64x64_600t", |b| {
        b.iter(|| {
            let cell = RefCell::new(FixtureWorld::new(&registry, 0, 0));
            let mut context = FixtureContext(&cell);
            let mut sink = FixtureSink(&cell);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut load_registry),
                entities: Some(&mut sink),
                ..SaveReadState::default()
            };
            ok(SaveIo::load(&fs, &path, &mut state));
        });
    });

    group.bench_function("meta_read", |b| {
        b.iter(|| {
            let meta = ok(SaveIo::get_meta(&fs, &path));
            black_box(meta.wave)
        });
    });

    // Slot listing, 100 saves (`Saves.load` parallel meta reads).
    let list_fs = MockFs::new();
    let list_dir = PathBuf::from("/bench/list");
    for index in 0..100u32 {
        let ctx = WriteContext::meta_only(base_meta_tags(8, 8, 1, "bench-map"));
        ok(SaveIo::save(
            &list_fs,
            &list_dir.join(format!("{index}.msav")),
            &ctx,
            &SaveOptions::new(),
        ));
    }
    group.bench_function("slot_listing_100", |b| {
        b.iter(|| {
            let listed = list_files_meta(&list_fs, &list_dir);
            black_box(listed.len())
        });
    });

    // Settings flush (serialize + atomic write).
    let settings_fs = MockFs::new();
    let settings_paths = Paths::new("/bench/settings-root");
    let mut settings = SettingsStore::new();
    settings.put_i32("bench", 1);
    group.bench_function("settings_flush", |b| {
        b.iter(|| {
            let next = settings.get_i32("bench", 0).wrapping_add(1);
            settings.put_i32("bench", black_box(next));
            ok(settings.force_save(&settings_fs, &settings_paths));
        });
    });

    // TypeIO plan/config encode (steady-state buffer reuse).
    let conveyor = match registry.block_id("conveyor") {
        Some(block) => block,
        None => panic!("conveyor block missing from base content"),
    };
    let plan = BuildPlan::place(3, 4, 1, conveyor, TypeValue::Int(7));
    let mut buffer: Vec<u8> = Vec::with_capacity(256);
    group.bench_function("typeio_plan_encode", |b| {
        b.iter(|| {
            buffer.clear();
            let mut writer = WireWriter::new(&mut buffer);
            ok(write_plan(&mut writer, black_box(&plan)));
            black_box(buffer.len())
        });
    });

    group.finish();
}

criterion_group!(benches, io_benches);
criterion_main!(benches);
