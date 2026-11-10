// SPDX-License-Identifier: GPL-3.0-only

//! `maps` command bodies: listing, fixer, save/load round-trips, previews and registry shuffle (plans 06/19).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::content::boot_content;
use super::*;

pub(super) fn cmd_maps_list(dir: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::fs::NativeFs;
    use mind_core::maps::Maps;

    if !dir.is_dir() {
        return Err(anyhow!("`{}` is not a directory", dir.display()));
    }
    let fs = NativeFs;
    let mut maps = Maps::new();
    let loaded = maps.load_from_dir(&fs, dir, true);

    let entries: Vec<serde_json::Value> = maps
        .all()
        .iter()
        .map(|map| {
            serde_json::json!({
                "name": map.name(),
                "author": map.author(),
                "description": map.description(),
                "custom": map.custom,
                "width": map.width,
                "height": map.height,
                "version": map.version,
                "build": map.build,
                "file": map.file.display().to_string(),
            })
        })
        .collect();

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_list",
        "dir": dir.display().to_string(),
        "count": loaded,
        "maps": entries,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        for map in maps.all() {
            println!(
                "{} ({}x{}, custom={})",
                map.name(),
                map.width,
                map.height,
                map.custom
            );
        }
        println!("{} maps", maps.len());
    }
    Ok(EXIT_PASS)
}

/// FNV-1a checksum of a preview image's pixels (plan 19 §7b).
pub(super) fn preview_image_checksum(image: &mind_core::io::map::PreviewImage) -> String {
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u32(image.width);
    hasher.write_u32(image.height);
    hasher.write(&image.rgba);
    hasher.finish().to_hex()
}

/// `maps fix` (plan 19 M8 §3.13/§7b): build a deterministic fixture dir, run the
/// MapFixer checks (dry-run then write), and assert the second write is a no-op.
pub(super) fn cmd_maps_fix(dir: Option<&Path>, dry_run: bool, json: bool) -> anyhow::Result<i32> {
    use bevy_ecs::world::World;
    use mind_core::editor::MapEditor;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map};
    use mind_core::io::fs::NativeFs;
    use mind_core::io::json::JsonIo;
    use mind_core::io::json::rules::Rules;
    use mind_core::maps::fix::fix_dir;
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    let fs = NativeFs;
    let mut content = boot_content()?;
    let root = match dir {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir()?.join(path),
        None => std::env::temp_dir().join("mgorch-mapfix"),
    };
    let hidden = root.join("hidden");
    std::fs::create_dir_all(&hidden)?;
    let file = hidden.join("fixme.msav");

    // Deterministic fixture: a 16×16 editor map with fixable rules + wave 5.
    let mut editor = MapEditor::new();
    let mut grid = WorldGrid::new(0, 0);
    let mut ecs = World::new();
    let hooks = NoopWorldHooks;
    let render = NoopRenderHooks;
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.begin_edit_size(&mut world, &content, 16, 16);
    }
    #[allow(clippy::field_reassign_with_default)]
    let rules = {
        let mut rules = Rules::default();
        rules.infinite_resources = true;
        rules.instant_build = true;
        rules.banned_blocks.insert("conveyor".to_owned());
        rules.revealed_blocks.insert("router".to_owned());
        rules
    };
    editor.tags.insert("name".to_owned(), "fixme".to_owned());
    editor
        .tags
        .insert("rules".to_owned(), JsonIo::write(&rules)?);
    editor.tags.insert("wave".to_owned(), "5".to_owned());
    let base = editor_base_tags(16, 16, "fixme");
    save_editor_map(
        &fs,
        &file,
        &grid,
        &content,
        base,
        editor.tags.clone(),
        false,
    )?;

    let dry = fix_dir(&fs, &mut content, &root, true)?;
    let dry_changes: usize = dry.iter().map(|r| r.changes.len()).sum();
    if dry_run {
        let report = serde_json::json!({
            "format": 1,
            "generator": "maps_fix",
            "dry_run": true,
            "reports": dry.iter().map(|r| serde_json::json!({
                "file": r.file.display().to_string(),
                "changed": r.changed,
                "changes": r.changes,
            })).collect::<Vec<_>>(),
            "ok": dry_changes > 0,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(if dry_changes > 0 {
            EXIT_PASS
        } else {
            EXIT_FAIL
        });
    }

    let first = fix_dir(&fs, &mut content, &root, false)?;
    let second = fix_dir(&fs, &mut content, &root, false)?;
    let first_changed = first.iter().filter(|r| r.changed).count();
    let second_changed = second.iter().filter(|r| r.changed).count();
    let ok = dry_changes > 0 && first_changed == 1 && second_changed == 0;
    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_fix",
        "dry_run": false,
        "dry_changes": dry_changes,
        "first_changed": first_changed,
        "second_changed": second_changed,
        "first": first.iter().map(|r| serde_json::json!({
            "file": r.file.display().to_string(),
            "changed": r.changed,
            "changes": r.changes,
        })).collect::<Vec<_>>(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("maps fix: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps save-load-save` (plan 19 M2 §5/§7b): save → load → save byte-stable.
pub(super) fn cmd_maps_save_load_save(map_name: &str, json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::MapEditor;
    use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map};
    use mind_core::io::FileSystem;
    use mind_core::io::StringMap as IndexMap;
    use mind_core::io::fs::NativeFs;
    use mind_core::io::map::MapIo;
    use mind_core::maps::Map;
    use mind_core::world::WorldGrid;

    let mut content = boot_content()?;
    let fs = NativeFs;
    let dir = std::env::temp_dir().join("mgorch-maps");
    std::fs::create_dir_all(&dir)?;
    let file_a = dir.join(format!("{map_name}_a.msav"));
    let file_b = dir.join(format!("{map_name}_b.msav"));

    let mut grid = WorldGrid::new(32, 32);
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let copper = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
    let ice = content.block_id("ice").unwrap_or(stone);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    grid.tiles.get_mut(5, 5).block = copper;
    grid.tiles.get_mut(7, 7).floor = ice;

    let mut tags = IndexMap::new();
    tags.insert("name".to_owned(), map_name.to_owned());
    tags.insert("description".to_owned(), "editor fixture".to_owned());
    tags.insert("author".to_owned(), "lane/f20-19".to_owned());
    tags.insert("rules".to_owned(), r#"{"editor":false}"#.to_owned());
    tags.insert("genfilters".to_owned(), "{}".to_owned());
    tags.insert("locales".to_owned(), r#"{"en":{"a":"A"}}"#.to_owned());
    let base = editor_base_tags(32, 32, map_name);
    save_editor_map(&fs, &file_a, &grid, &content, base, tags, false)?;
    let bytes_a = fs.read(&file_a)?;

    let header = MapIo::create_map(&fs, &file_a, true)?;
    let map_a = Map::from_header(&header, true);
    let mut grid2 = WorldGrid::new(0, 0);
    let mut editor = MapEditor::new();
    editor.begin_edit_map(&mut grid2, &mut content, &fs, &map_a)?;

    let base2 = editor_base_tags(
        grid2.tiles.width as u16,
        grid2.tiles.height as u16,
        map_name,
    );
    save_editor_map(
        &fs,
        &file_b,
        &grid2,
        &content,
        base2,
        editor.tags.clone(),
        false,
    )?;
    let bytes_b = fs.read(&file_b)?;

    let idempotent = bytes_a == bytes_b;
    let rules_a = map_a.tags.get("rules").cloned().unwrap_or_default();
    let header_b = MapIo::create_map(&fs, &file_b, true)?;
    let rules_b = header_b.tags.get("rules").cloned().unwrap_or_default();
    let rules_equal = rules_a == rules_b;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_save_load_save",
        "map": map_name,
        "bytes_a": bytes_a.len(),
        "bytes_b": bytes_b.len(),
        "idempotent": idempotent,
        "rules_equal": rules_equal,
        "rules": rules_a,
        "ok": idempotent && rules_equal,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!(
            "save-load-save: {} ({} B)",
            if idempotent && rules_equal {
                "ok"
            } else {
                "FAIL"
            },
            bytes_a.len()
        );
    }
    Ok(if idempotent && rules_equal {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

/// `maps roundtrip` (plan 06 M4 / plan 19 M7 §3.8): save a live grid through
/// `save_map`, generate its preview PNG/cache, import the copy through
/// `import_map` and assert both preview pixel checksums match.
pub(super) fn cmd_maps_roundtrip(json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::maps_glue::{editor_base_tags, import_map_e2e, save_map_e2e};
    use mind_core::editor::preview::PreviewPipeline;
    use mind_core::io::FileSystem;
    use mind_core::io::StringMap as IndexMap;
    use mind_core::io::fs::{NativeFs, Paths};
    use mind_core::maps::Maps;
    use mind_core::world::WorldGrid;

    let fs = NativeFs;
    let root = std::env::temp_dir().join("mgorch-maps-roundtrip");
    let _ = std::fs::remove_dir_all(&root);
    let paths = Paths::new(&root);
    std::fs::create_dir_all(paths.maps())?;
    std::fs::create_dir_all(root.join("imported"))?;

    let mut content = boot_content()?;
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let core = content.block_id("core-shard");
    let spawn = content.block_id("spawn");

    let mut grid = WorldGrid::new(16, 16);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    if let Some(core) = core {
        grid.tiles.get_mut(4, 4).block = core;
    }
    let mut spawn_count = 0u32;
    if let Some(spawn) = spawn {
        grid.tiles.get_mut(8, 8).overlay = spawn;
        grid.tiles.get_mut(9, 8).overlay = spawn;
        spawn_count = 2;
    }

    let mut tags = IndexMap::new();
    tags.insert("name".to_owned(), "Roundtrip".to_owned());
    tags.insert("author".to_owned(), "lane/f25-maps".to_owned());
    let base = editor_base_tags(16, 16, "Roundtrip");

    let mut maps = Maps::new();
    let mut pipeline = PreviewPipeline::new();
    let (map, image) = save_map_e2e(
        &fs,
        &paths,
        &mut pipeline,
        &mut maps,
        &paths.maps(),
        &grid,
        &mut content,
        base,
        tags,
        false,
    )?;
    let checksum_save = preview_image_checksum(&image);
    let preview_written = fs.exists(&mind_core::maps::preview_file(&paths, &map));
    let cache_written = fs.exists(&mind_core::maps::cache_file(&paths, &map));

    let import_dir = root.join("imported");
    let mut imported_maps = Maps::new();
    let mut imported_pipeline = PreviewPipeline::new();
    let (imported, image2) = import_map_e2e(
        &fs,
        &paths,
        &mut imported_pipeline,
        &mut imported_maps,
        &import_dir,
        &map.file,
        &mut content,
    )?;
    let checksum_import = preview_image_checksum(&image2);

    let map_ok = maps.len() == 1 && imported_maps.len() == 1 && imported.name() == map.name();
    let preview_ok = preview_written && cache_written && checksum_save == checksum_import;
    let ok = map_ok && preview_ok && map.spawns == spawn_count;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_roundtrip",
        "map": map.name(),
        "spawns": map.spawns,
        "preview_checksum_save": checksum_save,
        "preview_checksum_import": checksum_import,
        "preview_written": preview_written,
        "cache_written": cache_written,
        "registry_save": maps.len(),
        "registry_import": imported_maps.len(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("maps roundtrip: {checksum_save} -> {checksum_import} ok={ok}");
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps preview-tiles` (plan 19 M2 §5/§7b): deterministic preview pixels +
/// PNG round-trip.
pub(super) fn cmd_maps_preview_tiles(json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::maps_glue::EditorMapSource;
    use mind_core::io::map::{MapIo, decode_png, encode_png};
    use mind_core::world::WorldGrid;

    let content = boot_content()?;
    let mut grid = WorldGrid::new(8, 8);
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let ore = content.block_id("ore-copper");
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    let mut ore_count = 0usize;
    if let Some(ore) = ore {
        grid.tiles.get_mut(2, 2).overlay = ore;
        grid.tiles.get_mut(5, 3).overlay = ore;
        ore_count = 2;
    }

    let source = EditorMapSource::new(&grid, &content);
    let image = MapIo::preview_from_tiles(&content, &source);
    let pixel_checksum = preview_image_checksum(&image);
    let png = encode_png(&image)?;
    let decoded = decode_png(&png)?;
    let png_ok = decoded == image;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_preview_tiles",
        "width": image.width,
        "height": image.height,
        "pixel_checksum": pixel_checksum,
        "png_bytes": png.len(),
        "png_round_trip_ok": png_ok,
        "ore_count": ore_count,
        "ok": png_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("preview {pixel_checksum} png_round_trip={png_ok}");
    }
    Ok(if png_ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps image-roundtrip` (plan 19 M2 §5/§7b): PNG → `read_image` → `write_image`
/// preserves the pixel checksum.
pub(super) fn cmd_maps_image_roundtrip(json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::maps_glue::{EditorMapSource, GridImageSink};
    use mind_core::io::map::{
        BlockPalette, FnColorMapper, decode_png, encode_png, read_image, write_image,
    };
    use mind_core::world::WorldGrid;

    let content = boot_content()?;
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    let ice = content.block_id("ice").unwrap_or(stone);

    let mut grid = WorldGrid::new(8, 8);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    for x in 0..8 {
        grid.tiles.get_mut(x, 4).floor = ice;
    }

    let palette = BlockPalette::of(&content);
    // The headless fixture builder does not run plan 03's region-derived
    // `map_color` pass, so invert the deterministic palette colors the writer
    // uses instead of plan-06's `ColorMapper` (which only sees explicit colors).
    let pairs: Vec<(u32, BlockId)> = content
        .blocks()
        .iter()
        .map(|def| (palette.map_color(def.id.raw()), def.id))
        .collect();
    let image_in = write_image(&palette, &EditorMapSource::new(&grid, &content));
    let checksum_in = preview_image_checksum(&image_in);
    let png = encode_png(&image_in)?;
    let decoded = decode_png(&png)?;

    let mut grid2 = WorldGrid::new(8, 8);
    for tile in grid2.tiles.array_mut() {
        tile.floor = stone;
    }
    let mapper = FnColorMapper(move |rgba: u32| {
        pairs
            .iter()
            .find(|(color, _)| *color == rgba)
            .map(|(_, block)| *block)
    });
    let mut sink = GridImageSink { grid: &mut grid2 };
    read_image(&palette, &decoded, &mut sink, &mapper)?;
    let image_out = write_image(&palette, &EditorMapSource::new(&grid2, &content));
    let checksum_out = preview_image_checksum(&image_out);
    let ok = checksum_in == checksum_out;

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_image_roundtrip",
        "checksum_in": checksum_in,
        "checksum_out": checksum_out,
        "png_bytes": png.len(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("image round-trip: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `maps registry-shuffle` (plan 06 §7b, verified by 19 M2): registry ordering
/// and `ShuffleMode` selection across seeds never repeats `previous` when more
/// than one candidate exists.
pub(super) fn cmd_maps_registry_shuffle(seeds: u32, json: bool) -> anyhow::Result<i32> {
    use mind_core::io::StringMap as IndexMap;
    use mind_core::maps::{GameMode, Map, Maps, ShuffleMode};
    use mind_core::random::JavaRandom;

    fn named(name: &str, custom: bool, dir: &str) -> Map {
        let mut tags = IndexMap::new();
        tags.insert("name".to_owned(), name.to_owned());
        Map::new(
            std::path::PathBuf::from(format!("{dir}/{name}.msav")),
            16,
            16,
            tags,
            custom,
            1,
            -1,
        )
    }

    let mut maps = Maps::new();
    maps.add(named("alpha", false, "/maps/default"));
    maps.add(named("beta", false, "/maps/default"));
    maps.add(named("custom-one", true, "/maps"));
    maps.add(named("custom-two", true, "/maps"));
    let mut modded = named("mod-map", false, "/mods/x/maps");
    modded.mod_id = Some("x".to_owned());
    maps.add(modded);

    let order: Vec<&str> = maps.all().iter().map(Map::name).collect();
    let custom_before_builtin = order
        .iter()
        .position(|name| *name == "custom-one")
        .zip(order.iter().position(|name| *name == "alpha"))
        .is_some_and(|(custom, builtin)| custom < builtin);

    let mut rng = JavaRandom::new(1);
    let mut previous: Option<usize> = None;
    let mut repeats = 0usize;
    let mut selections = Vec::new();
    for _ in 0..seeds.max(1) {
        let next = ShuffleMode::All.next(GameMode::Survival, previous, maps.all(), &mut rng);
        if let Some(index) = next {
            if Some(index) == previous {
                repeats += 1;
            }
            selections.push(maps.all()[index].name().to_owned());
            previous = Some(index);
        }
    }
    let ok = custom_before_builtin && repeats == 0 && !selections.is_empty();

    let report = serde_json::json!({
        "format": 1,
        "generator": "maps_registry_shuffle",
        "order": order,
        "custom_before_builtin": custom_before_builtin,
        "selections": selections,
        "repeats": repeats,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("registry-shuffle: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}
