// SPDX-License-Identifier: GPL-3.0-only

//! `io` command bodies: map listing, save/load round-trips, revision/class-id gates and settings checks (plan 04).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use crate::stats::percentile;
use super::content::boot_content;
use super::*;

/// Plan 04 M5 (§7b): parallel meta-only listing of one map/save directory.
pub(super) fn cmd_io_map_list(dir: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::slot::list_files_meta;

    let fs = NativeFs;
    let candidates = fs
        .ls(dir)?
        .into_iter()
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("msav")
                && !path
                    .file_name()
                    .map(|name| name.to_string_lossy().contains("backup"))
                    .unwrap_or(false)
        })
        .count();
    let entries: Vec<IoMapListEntry> = list_files_meta(&fs, dir)
        .into_iter()
        .map(|(file, meta)| IoMapListEntry {
            file: file.display().to_string(),
            name: if meta.is_map() {
                meta.tags.get("name").cloned().unwrap_or_default()
            } else {
                meta.map_name.clone()
            },
            width: meta.width(),
            height: meta.height(),
            wave: meta.wave,
            build: meta.build,
            format_version: meta.version,
            is_map: meta.is_map(),
            mods: meta.mods.len(),
        })
        .collect();
    let skipped = candidates.saturating_sub(entries.len());
    if !json {
        for entry in &entries {
            println!(
                "{}: {} {}x{} wave={} build={} v={}{}",
                entry.file,
                entry.name,
                entry.width,
                entry.height,
                entry.wave,
                entry.build,
                entry.format_version,
                if entry.is_map { " [map]" } else { "" }
            );
        }
        println!("{} listed, {skipped} skipped (corrupt)", entries.len());
    }
    let report = IoMapListReport {
        dir: dir.display().to_string(),
        listed: entries.len(),
        skipped,
        entries,
    };
    print_report(&report, json)?;
    Ok(EXIT_PASS)
}

/// Plan 04 M4 (§7b): the native v1 map/entities round-trip on the synthetic
/// fixture world (stands in for `serpulo/groundZero` until plan 06 lands).
pub(super) fn cmd_io_roundtrip(
    map: &str,
    width: u16,
    height: u16,
    ticks: u64,
    out: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::fixture::{FixtureContext, FixtureSink, FixtureWorld};
    use mind_core::io::save::versions::v1::base_meta_tags;
    use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};

    if map == "serpulo" || map == "generated" {
        return cmd_io_roundtrip_real(map, width, height, out, json);
    }
    if map != "synthetic" {
        return Err(anyhow!(
            "io roundtrip --map {map}: unknown map; use `synthetic` or `serpulo`"
        ));
    }
    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("mind-io-roundtrip.msav"));
    let fs = NativeFs;
    let registry = boot_content()?;

    // Build + simulate.
    let mut world = FixtureWorld::synthetic(&registry, width, height);
    for _ in 0..ticks {
        world.tick();
    }
    let checksum_before = world.checksum_hex();

    // Save.
    let mut tags = base_meta_tags(width, height, world.wave, "synthetic");
    tags.insert("tick".to_owned(), world.tick.to_string());
    let mut ctx = WriteContext::meta_only(tags);
    ctx.content = Some(&registry);
    ctx.map = Some(&world);
    ctx.entities = Some(&world);
    SaveIo::save(&fs, &out, &ctx, &SaveOptions::new())?;
    let bytes = fs.len(&out).unwrap_or(0);

    // Load into a fresh world.
    let cell = std::cell::RefCell::new(FixtureWorld::new(&registry, 0, 0));
    let mut context = FixtureContext(&cell);
    let mut sink = FixtureSink(&cell);
    let mut load_registry = boot_content()?;
    let mut state = SaveReadState {
        context: Some(&mut context),
        content: Some(&mut load_registry),
        entities: Some(&mut sink),
        ..SaveReadState::default()
    };
    SaveIo::load(&fs, &out, &mut state)?;
    let state_tags = state.tags.clone();
    let state_team_plans = state.team_plans.clone();
    let all_buildings = state.all_buildings.len();
    drop(state);
    let mut loaded = cell.into_inner();
    loaded.apply_meta(&state_tags);
    loaded.apply_team_plans(state_team_plans);
    let checksum_after = loaded.checksum_hex();

    let pass = checksum_before == checksum_after;
    if !pass {
        log::error!("io roundtrip checksum mismatch: {checksum_before} != {checksum_after}");
    }
    let report = IoRoundtripReport {
        map: map.to_owned(),
        width,
        height,
        ticks,
        out: out.display().to_string(),
        bytes,
        buildings: all_buildings,
        checksum_before,
        checksum_after,
        pass,
    };
    print_report(&report, json)?;
    Ok(exit_code(pass))
}

/// FNV-1a checksum over a grid's floor/overlay/block layers (row-major).
pub(super) fn grid_tile_checksum(grid: &mind_core::world::WorldGrid) -> String {
    use mind_core::determinism::Hasher;
    let mut hasher = Hasher::new();
    hasher.write_u32(grid.tiles.width as u32);
    hasher.write_u32(grid.tiles.height as u32);
    for index in 0..grid.tiles.len() {
        let tile = grid.tiles.geti(index);
        hasher.write_u16(tile.block.raw());
        hasher.write_u16(tile.floor.raw());
        hasher.write_u16(tile.overlay.raw());
    }
    hasher.finish().to_hex()
}

/// Plan 06 M2 real-context round-trip (04 §5 M4 swap): generate a real planet
/// map, write it through [`mind_core::world::EcsMapSource`] and read it back
/// through the plan-06 [`mind_core::world::Context`] (no synthetic fixture).
/// Asserts the tile checksum survives save → reset → load.
pub(super) fn cmd_io_roundtrip_real(
    map: &str,
    width: u16,
    height: u16,
    out: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::versions::v1::base_meta_tags;
    use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
    use mind_core::maps::generators::WorldGenerator;
    use mind_core::world::{Context, EcsMapSource, WorldGrid, WorldParams};

    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("mind-io-roundtrip-real.msav"));
    let fs = NativeFs;
    let content = boot_content()?;
    let params = WorldParams {
        seed_offset: 42,
        width: width as i32,
        height: height as i32,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(width as i32, height as i32);
    let mut generator = mind_core::maps::planet::SerpuloPlanetGenerator::new();
    generator.generate(&mut grid.tiles, &params, &content);
    let checksum_before = grid_tile_checksum(&grid);

    // Write through the plan-06 ECS-aware map source (no buildings: the serpulo
    // generator's launch loadout/ruins remain plan-12 seams).
    let ecs = bevy_ecs::world::World::new();
    let source = EcsMapSource::new(&grid, &content, &ecs);
    let mut tags = base_meta_tags(width, height, 0, map);
    tags.insert("name".to_owned(), map.to_owned());
    let mut ctx = WriteContext::meta_only(tags);
    ctx.content = Some(&content);
    ctx.map = Some(&source);
    SaveIo::save(&fs, &out, &ctx, &SaveOptions::new())?;
    let bytes = fs.len(&out).unwrap_or(0);

    // Read into a fresh grid through the real `Context` (04's WorldContext).
    let read_content = boot_content()?;
    let mut load_registry = boot_content()?;
    let mut loaded_grid = WorldGrid::new(0, 0);
    let all_buildings;
    {
        let mut context = Context::new(&mut loaded_grid, &read_content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut load_registry),
            ..SaveReadState::default()
        };
        SaveIo::load(&fs, &out, &mut state)?;
        all_buildings = state.all_buildings.len();
    }
    let checksum_after = grid_tile_checksum(&loaded_grid);

    let pass = checksum_before == checksum_after;
    if !pass {
        log::error!("io roundtrip (real) checksum mismatch: {checksum_before} != {checksum_after}");
    }
    let report = IoRoundtripReport {
        map: map.to_owned(),
        width,
        height,
        ticks: 0,
        out: out.display().to_string(),
        bytes,
        buildings: all_buildings,
        checksum_before,
        checksum_after,
        pass,
    };
    print_report(&report, json)?;
    Ok(exit_code(pass))
}

/// Plan 04 M8 (§7b/§7d): P50/P95 timings for save/load/meta on the synthetic
/// fixture world (the §7d `serpulo/groundZero` baseline needs plan 06 maps).
pub(super) fn cmd_io_bench_save(
    map: &str,
    width: u16,
    height: u16,
    ticks: u64,
    iters: u64,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::fs::{FileSystem, NativeFs};
    use mind_core::io::save::fixture::{FixtureContext, FixtureSink, FixtureWorld};
    use mind_core::io::save::versions::v1::base_meta_tags;
    use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};

    if map != "synthetic" {
        return Err(anyhow!(
            "io bench-save --map {map}: real maps need plan 06 (world/generators); use `--map synthetic`"
        ));
    }
    if iters == 0 {
        return Err(anyhow!("io bench-save --iters must be greater than 0"));
    }
    let fs = NativeFs;
    let path = std::env::temp_dir().join("mind-io-bench.msav");
    let backup = SaveIo::backup_file_for(&path);
    let registry = boot_content()?;

    // Mid-game-ish fixture: build + tick once (outside the timed regions).
    let mut world = FixtureWorld::synthetic(&registry, width, height);
    for _ in 0..ticks {
        world.tick();
    }
    let checksum_before = world.checksum_hex();
    let mut tags = base_meta_tags(width, height, world.wave, "synthetic");
    tags.insert("tick".to_owned(), world.tick.to_string());

    // Save: serialize + deflate + atomic file write (fresh file every run).
    let mut save_samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let _ = fs.delete(&path);
        let _ = fs.delete(&backup);
        let mut ctx = WriteContext::meta_only(tags.clone());
        ctx.content = Some(&registry);
        ctx.map = Some(&world);
        ctx.entities = Some(&world);
        let start = Instant::now();
        SaveIo::save(&fs, &path, &ctx, &SaveOptions::new())?;
        save_samples.push(start.elapsed().as_nanos() as u64);
    }
    let bytes = fs.len(&path).unwrap_or(0);

    // Load: read + inflate + apply regions into a fresh fixture world.
    let mut load_registry = boot_content()?;
    let mut load_samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let cell = std::cell::RefCell::new(FixtureWorld::new(&registry, 0, 0));
        let mut context = FixtureContext(&cell);
        let mut sink = FixtureSink(&cell);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut load_registry),
            entities: Some(&mut sink),
            ..SaveReadState::default()
        };
        let start = Instant::now();
        SaveIo::load(&fs, &path, &mut state)?;
        load_samples.push(start.elapsed().as_nanos() as u64);
    }

    // Meta-only read.
    let mut meta_samples = time_runs(iters as usize, || {
        let _ = SaveIo::get_meta(&fs, &path)?;
        Ok(())
    })?;

    // Load verification (not timed): last read applies meta + plans and must
    // reproduce the save-side checksum.
    let cell = std::cell::RefCell::new(FixtureWorld::new(&registry, 0, 0));
    let mut context = FixtureContext(&cell);
    let mut sink = FixtureSink(&cell);
    let mut state = SaveReadState {
        context: Some(&mut context),
        content: Some(&mut load_registry),
        entities: Some(&mut sink),
        ..SaveReadState::default()
    };
    SaveIo::load(&fs, &path, &mut state)?;
    let state_tags = state.tags.clone();
    let state_team_plans = state.team_plans.clone();
    drop(state);
    let mut loaded = cell.into_inner();
    loaded.apply_meta(&state_tags);
    loaded.apply_team_plans(state_team_plans);
    let pass = checksum_before == loaded.checksum_hex();
    if !pass {
        log::error!("io bench-save load verification failed: checksums differ");
    }

    let report = IoBenchSaveReport {
        map: map.to_owned(),
        width,
        height,
        ticks,
        iters,
        bytes,
        save: io_bench_stat(&mut save_samples),
        load: io_bench_stat(&mut load_samples),
        meta: io_bench_stat(&mut meta_samples),
        pass,
        note: String::from(
            "synthetic 64x64 fixture; the 7d groundZero baseline needs plan 06 real maps",
        ),
    };
    print_report(&report, json)?;
    Ok(exit_code(pass))
}

/// Plan 04 M3 (§7b): revision drift check for every `EntityDefs!` def.
pub(super) fn cmd_io_check_revisions(
    update: bool,
    mind_core_dir: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::entity::registry::entity_defs;
    use mind_core::io::entity::revisions::{RevisionCheck, check_all};
    use mind_core::io::fs::NativeFs;

    let dir = paths::find_mind_core_dir(mind_core_dir)?;
    let root = dir.join("revisions");
    let reports = check_all(&NativeFs, &root, entity_defs(), update)?;
    let mut pass = true;
    let mut def_reports = Vec::new();
    for report in &reports {
        let (status, details) = match &report.outcome {
            RevisionCheck::UpToDate => (String::from("up-to-date"), Vec::new()),
            RevisionCheck::Missing => {
                if update {
                    (String::from("written"), Vec::new())
                } else {
                    pass = false;
                    (
                        String::from("missing"),
                        vec![String::from("no manifests committed")],
                    )
                }
            }
            RevisionCheck::Drift { details, .. } => {
                if update {
                    (String::from("updated"), details.clone())
                } else {
                    pass = false;
                    (String::from("drift"), details.clone())
                }
            }
        };
        if !json {
            println!("{}: {status}", report.name);
            for detail in &details {
                println!("  - {detail}");
            }
        }
        def_reports.push(IoDefRevisionReport {
            name: report.name.clone(),
            status,
            updated_to: report.updated_to,
            details,
        });
    }
    let report = IoCheckRevisionsReport {
        revisions_root: root.display().to_string(),
        update,
        pass,
        defs: def_reports,
    };
    print_report(&report, json)?;
    Ok(exit_code(pass))
}

/// Plan 04 M3: class-ID drift gate (`entity_class_ids.toml` ↔ registry ↔
/// generated `class_ids.rs`).
pub(super) fn cmd_io_check_class_ids(
    update: bool,
    mind_core_dir: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::io::entity::idfile::ClassIdFile;
    use mind_core::io::entity::registry::entity_defs;
    use mind_core::io::fs::{FileSystem, NativeFs};

    let dir = paths::find_mind_core_dir(mind_core_dir)?;
    let toml_path = dir.join("entity_class_ids.toml");
    let rs_path = dir.join("src/io/entity/class_ids.rs");
    let fs = NativeFs;

    let text = String::from_utf8(fs.read(&toml_path)?)
        .map_err(|_| anyhow!("`{}` is not UTF-8", toml_path.display()))?;
    let file = ClassIdFile::parse(&text)?;
    let mut problems = file.problems(entity_defs());

    // The generated constants file must match the TOML exactly (drift gate).
    let expected_rs = file.render_rs();
    let committed_rs = fs
        .read(&rs_path)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    if committed_rs != expected_rs {
        problems.push(String::from(
            "src/io/entity/class_ids.rs is stale (run `io check-class-ids --update`)",
        ));
    }

    let mut updated = false;
    if update && !problems.is_empty() {
        let with_new = file.with_new_defs(entity_defs());
        fs.write(&toml_path, with_new.render_toml().as_bytes())?;
        fs.write(&rs_path, with_new.render_rs().as_bytes())?;
        updated = true;
        // Re-validate after regeneration.
        problems = with_new.problems(entity_defs());
    }

    let pass = problems.is_empty();
    let report = IoCheckClassIdsReport {
        toml: toml_path.display().to_string(),
        generated: rs_path.display().to_string(),
        entries: file.entries.len(),
        update,
        updated,
        problems,
        pass,
    };
    print_report(&report, json)?;
    Ok(exit_code(pass))
}

/// Plan 04 M1 (§7b): settings set → flush → reload equality, corrupt file →
/// defaults, no panic. Uses NativeFs against `--data-dir` (a scratch dir).
pub(super) fn cmd_io_settings(cli: &Cli, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::fs::NativeFs;
    use mind_core::io::settings::{
        KEY_LAST_SECTOR_SAVE, KEY_SAVE_INTERVAL, KEY_UI_SCALE, slot_autosave_key, slot_name_key,
    };
    use mind_core::io::{FileSystem, Paths, SettingsStore};

    let fs = NativeFs;
    let paths = Paths::resolve(cli.data_dir.as_deref());
    fs.mkdirs(&paths.config())?;
    // Deterministic start: the scenario owns this data-dir.
    let _ = fs.delete(&paths.settings_file());

    // Phase 1: set + flush.
    let mut store = SettingsStore::new();
    store.put_string(&slot_name_key("0"), "m1-base");
    store.put_bool(&slot_autosave_key("0"), false);
    store.put_i32(KEY_SAVE_INTERVAL, 7);
    store.put_i32(KEY_UI_SCALE, 150);
    store.put_string(KEY_LAST_SECTOR_SAVE, "sector-serpulo-12");
    store.put_json("controlGroups", &vec![vec![1i32, 2], vec![3]])?;
    store.force_save(&fs, &paths)?;

    // Phase 2: reload → equality.
    let reloaded = SettingsStore::load(&fs, &paths);
    let checks: Vec<(&str, bool)> = vec![
        (
            "slot-name",
            reloaded.get_string(&slot_name_key("0"), "?") == "m1-base",
        ),
        (
            "slot-autosave",
            !reloaded.get_bool(&slot_autosave_key("0"), true),
        ),
        ("saveinterval", reloaded.get_i32(KEY_SAVE_INTERVAL, 2) == 7),
        ("uiscale", reloaded.get_i32(KEY_UI_SCALE, 100) == 150),
        (
            "last-sector-save",
            reloaded.get_string(KEY_LAST_SECTOR_SAVE, "<none>") == "sector-serpulo-12",
        ),
        (
            "json",
            reloaded.get_json::<Vec<Vec<i32>>>("controlGroups")? == Some(vec![vec![1, 2], vec![3]]),
        ),
    ];
    let persisted = checks.iter().all(|(_, ok)| *ok);
    for (name, ok) in &checks {
        if !ok {
            log::error!("settings persistence check failed: {name}");
        }
    }

    // Phase 3: corrupt the file → defaults, no panic.
    fs.write(&paths.settings_file(), b"garbage-not-settings")?;
    let corrupt = SettingsStore::load(&fs, &paths);
    let corrupt_fallback = corrupt.is_empty() && corrupt.get_i32(KEY_SAVE_INTERVAL, 2) == 2;

    // Phase 4: the store recovers (rewrites a valid file).
    let mut recovered = corrupt;
    recovered.put_i32(KEY_SAVE_INTERVAL, 3);
    recovered.force_save(&fs, &paths)?;
    let recovered_ok = SettingsStore::load(&fs, &paths).get_i32(KEY_SAVE_INTERVAL, 2) == 3;

    let pass = persisted && corrupt_fallback && recovered_ok;
    let report = IoSettingsReport {
        data_dir: paths.root().display().to_string(),
        persisted,
        corrupt_fallback,
        recovered: recovered_ok,
        keys_checked: checks.iter().map(|(name, _)| (*name).to_owned()).collect(),
        pass,
    };
    print_report(&report, json)?;
    Ok(exit_code(pass))
}

/// Plan 04 M0: meta-only read of a save file written by the IO engine.
pub(super) fn cmd_io_dump_meta(file: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::{NativeFs, SaveIo};

    let fs = NativeFs;
    let meta = SaveIo::get_meta(&fs, file)
        .with_context(|| format!("reading meta of `{}`", file.display()))?;
    let report = IoDumpMetaReport {
        file: file.display().to_string(),
        format_version: meta.version,
        build: meta.build,
        timestamp: meta.timestamp,
        time_played: meta.time_played,
        map_name: meta.map_name.clone(),
        wave: meta.wave,
        width: meta.width(),
        height: meta.height(),
        is_map: meta.is_map(),
        mods: meta.mods.clone(),
        tags: meta
            .tags
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "meta: format={} build={} map={} wave={} {}x{} is_map={} mods={}",
            report.format_version,
            report.build,
            report.map_name,
            report.wave,
            report.width,
            report.height,
            report.is_map,
            report.mods.len()
        );
    }
    Ok(EXIT_PASS)
}

/// Sorts nanosecond samples and reports the P50/P95/min/max in milliseconds.
pub(super) fn io_bench_stat(samples: &mut [u64]) -> IoBenchStat {
    samples.sort_unstable();
    let millis = |value: u64| value as f64 / 1_000_000.0;
    let last = samples.last().copied().unwrap_or(0);
    IoBenchStat {
        p50_ms: millis(percentile(samples, 50)),
        p95_ms: millis(percentile(samples, 95)),
        min_ms: millis(samples.first().copied().unwrap_or(0)),
        max_ms: millis(last),
    }
}
