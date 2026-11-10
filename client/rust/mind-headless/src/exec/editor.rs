// SPDX-License-Identifier: GPL-3.0-only

//! `editor` command bodies: op-log replay, round-trips, previews, playtest, objectives/wave graph, locales/assets and bench (plan 19).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::content::boot_content;
use super::maps::preview_image_checksum;
use super::*;

/// JSON op-log fixture (plan 19 §6.1): `ops` is a list of `DrawOperation`s, each
/// a list of packed `u64` values (decimal numbers or decimal/hex strings).
#[derive(Debug, serde::Deserialize)]
pub(super) struct EditorOpsFixture {
    #[serde(default = "default_editor_fixture_format")]
    format: u32,
    width: i32,
    height: i32,
    ops: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    checksum_after_apply: String,
    #[serde(default)]
    checksum_after_undo: String,
    #[serde(default)]
    checksum_after_redo: String,
}

pub(super) fn default_editor_fixture_format() -> u32 {
    1
}

/// Parses one packed-op value (number, decimal string or `0x` hex string).
pub(super) fn editor_op_value(value: &serde_json::Value) -> anyhow::Result<u64> {
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| anyhow!("op value `{number}` is not an unsigned integer")),
        serde_json::Value::String(text) => {
            let text = text.trim();
            if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16)
                    .map_err(|error| anyhow!("invalid hex op value `{text}`: {error}"))
            } else {
                text.parse::<u64>()
                    .map_err(|error| anyhow!("invalid op value `{text}`: {error}"))
            }
        }
        other => Err(anyhow!("unsupported op value `{other}`")),
    }
}

/// Canonical world checksum (`ChecksumPart for WorldGrid`, plan 06 §3.2).
pub(super) fn editor_world_checksum(grid: &mind_core::world::WorldGrid) -> String {
    use mind_core::determinism::Checksummer;
    let mut checksummer = Checksummer::new();
    checksummer.part(grid);
    checksummer.finish().to_hex()
}

/// Compares two checksum strings ignoring an optional `0x` prefix and case.
pub(super) fn editor_checksum_matches(actual: &str, expected: &str) -> bool {
    fn normalize(value: &str) -> String {
        value
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X")
            .to_ascii_lowercase()
    }
    normalize(actual) == normalize(expected)
}

/// `editor ops` (plan 19 M0 §5/§7b): replay a packed op-log fixture through the
/// real `WorldGrid` seam — apply → undo-all → redo-all — and assert the three
/// committed world checksums.
pub(super) fn cmd_editor_ops(fixture: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::draw_op::DrawOperation;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::stack::OperationStack;

    let text = std::fs::read_to_string(fixture)
        .with_context(|| format!("reading editor fixture `{}`", fixture.display()))?;
    let data: EditorOpsFixture = serde_json::from_str(&text)
        .with_context(|| format!("parsing editor fixture `{}`", fixture.display()))?;
    if data.format != 1 {
        return Err(anyhow!(
            "unsupported editor fixture format {} (expected 1)",
            data.format
        ));
    }
    if data.width <= 0 || data.height <= 0 {
        return Err(anyhow!(
            "fixture dimensions must be positive (got {}x{})",
            data.width,
            data.height
        ));
    }

    let EditorWorld {
        content,
        mut grid,
        mut ecs,
        hooks,
        render,
    } = editor_world(data.width, data.height)?;
    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
    // `begin_edit_size` base: every tile is stone-floored air.
    for index in 0..grid.tiles.len() {
        grid.tiles.geti_mut(index).floor = stone;
    }

    let mut stack = OperationStack::new();
    let mut op_count = 0usize;
    for packed in &data.ops {
        let mut ops = Vec::with_capacity(packed.len());
        for value in packed {
            ops.push(editor_op_value(value)?);
        }
        op_count += ops.len();
        let mut operation = DrawOperation::from_ops(ops);
        {
            let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            operation.redo(&mut world, &content);
        }
        stack.add(operation);
    }

    let checksum_after_apply = editor_world_checksum(&grid);
    while stack.can_undo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        stack.undo(&mut world, &content);
    }
    let checksum_after_undo = editor_world_checksum(&grid);
    while stack.can_redo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        stack.redo(&mut world, &content);
    }
    let checksum_after_redo = editor_world_checksum(&grid);

    // An apply → undo-all → redo-all cycle must land on the applied state.
    let round_trip_ok = checksum_after_apply == checksum_after_redo;
    let goldens_present = !data.checksum_after_apply.is_empty()
        && !data.checksum_after_undo.is_empty()
        && !data.checksum_after_redo.is_empty();
    let checksums_ok = !goldens_present
        || (editor_checksum_matches(&checksum_after_apply, &data.checksum_after_apply)
            && editor_checksum_matches(&checksum_after_undo, &data.checksum_after_undo)
            && editor_checksum_matches(&checksum_after_redo, &data.checksum_after_redo));

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_ops",
        "fixture": fixture.display().to_string(),
        "width": data.width,
        "height": data.height,
        "operations": data.ops.len(),
        "ops": op_count,
        "checksum_after_apply": checksum_after_apply,
        "checksum_after_undo": checksum_after_undo,
        "checksum_after_redo": checksum_after_redo,
        "expected": {
            "checksum_after_apply": data.checksum_after_apply,
            "checksum_after_undo": data.checksum_after_undo,
            "checksum_after_redo": data.checksum_after_redo,
        },
        "goldens_present": goldens_present,
        "round_trip_ok": round_trip_ok,
        "checksums_ok": checksums_ok,
        "ok": round_trip_ok && checksums_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("apply: {checksum_after_apply}");
        println!("undo:  {checksum_after_undo}");
        println!("redo:  {checksum_after_redo}");
        if !round_trip_ok {
            log::error!("editor ops round-trip mismatch: redo checksum != apply checksum");
        }
        if goldens_present && !checksums_ok {
            log::error!(
                "editor ops checksum mismatch against `{}`",
                fixture.display()
            );
        }
    }
    if round_trip_ok && checksums_ok {
        Ok(EXIT_PASS)
    } else {
        Ok(EXIT_FAIL)
    }
}

/// Normalized world checksum (counters zeroed) so a save/load round-trip is
/// comparable across the load epilogue's counter reset.
pub(super) fn editor_world_checksum_normalized(grid: &mut mind_core::world::WorldGrid) -> String {
    grid.tile_changes = 0;
    grid.floor_changes = 0;
    editor_world_checksum(grid)
}

/// `editor roundtrip` (plan 19 M1 §5/§7b): adopt → draw → undo → save → reload.
pub(super) fn cmd_editor_roundtrip(
    map: Option<&Path>,
    _seed: u64,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map};
    use mind_core::editor::{EditorTool, MapEditor};
    use mind_core::io::fs::NativeFs;
    use mind_core::io::map::MapIo;
    use mind_core::maps::Map;
    use mind_core::world::WorldGrid;

    let EditorWorld {
        mut content,
        mut grid,
        mut ecs,
        hooks,
        render,
    } = editor_world(0, 0)?;
    let fs = NativeFs;
    let mut editor = MapEditor::new();

    if let Some(file) = map {
        let header = MapIo::create_map(&fs, file, true)?;
        let loaded = Map::from_header(&header, true);
        editor.begin_edit_map(&mut grid, &mut content, &fs, &loaded)?;
    } else {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.begin_edit_size(&mut world, &content, 32, 32);
        editor.adopt_world(&mut world);
    }

    // adopt → draw 3 lines + 1 fill.
    let wall = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.draw_block = wall;
        editor.tool = EditorTool::Line;
        mind_core::editor::tool::touched_line(
            &mut editor,
            EditorTool::Line,
            &mut world,
            &content,
            4,
            4,
            20,
            4,
        );
        mind_core::editor::tool::touched_line(
            &mut editor,
            EditorTool::Line,
            &mut world,
            &content,
            4,
            6,
            4,
            18,
        );
        mind_core::editor::tool::touched_line(
            &mut editor,
            EditorTool::Line,
            &mut world,
            &content,
            8,
            8,
            16,
            16,
        );
        editor.flush_op();
    }
    let checksum_after_draw = editor_world_checksum_normalized(&mut grid);
    while editor.can_undo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.undo(&mut world, &content);
    }
    let checksum_after_undo = editor_world_checksum_normalized(&mut grid);
    while editor.can_redo() {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.redo(&mut world, &content);
    }
    let checksum_after_redo = editor_world_checksum_normalized(&mut grid);

    // save → reload.
    let dir = std::env::temp_dir().join("mgorch-editor");
    std::fs::create_dir_all(&dir)?;
    let save_file = dir.join("editor_roundtrip.msav");
    editor
        .tags
        .insert("name".to_owned(), "Editor Roundtrip".to_owned());
    editor.tags.insert("rules".to_owned(), "{}".to_owned());
    editor.tags.insert("genfilters".to_owned(), "{}".to_owned());
    editor.tags.insert("locales".to_owned(), "{}".to_owned());
    let base = editor_base_tags(
        grid.tiles.width as u16,
        grid.tiles.height as u16,
        "Editor Roundtrip",
    );
    save_editor_map(
        &fs,
        &save_file,
        &grid,
        &content,
        base,
        editor.tags.clone(),
        false,
    )?;
    let header = MapIo::create_map(&fs, &save_file, true)?;
    let reloaded = Map::from_header(&header, true);
    let mut grid2 = WorldGrid::new(0, 0);
    editor.begin_edit_map(&mut grid2, &mut content, &fs, &reloaded)?;
    let checksum_loaded = editor_world_checksum_normalized(&mut grid2);

    let round_trip_ok =
        checksum_after_draw == checksum_after_redo && checksum_after_draw == checksum_loaded;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_roundtrip",
        "source": map.map(|p| p.display().to_string()),
        "save": save_file.display().to_string(),
        "checksum_after_draw": checksum_after_draw,
        "checksum_after_undo": checksum_after_undo,
        "checksum_after_redo": checksum_after_redo,
        "checksum_loaded": checksum_loaded,
        "round_trip_ok": round_trip_ok,
        "ok": round_trip_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("draw:   {checksum_after_draw}");
        println!("undo:   {checksum_after_undo}");
        println!("redo:   {checksum_after_redo}");
        println!("loaded: {checksum_loaded}");
        println!("round trip: {}", if round_trip_ok { "ok" } else { "FAIL" });
    }
    Ok(exit_code(round_trip_ok))
}

/// `editor resize-shift` (plan 19 M1 §5/§7b): in-bounds tiles/data survive a
/// resize with a ±10 shift; out-of-bounds tiles become the default stone floor.
pub(super) fn cmd_editor_resize_shift(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::{EditorGrid, MapEditor};

    let EditorWorld {
        content,
        mut grid,
        mut ecs,
        hooks,
        render,
    } = editor_world(0, 0)?;
    let mut editor = MapEditor::new();
    let wall = content
        .block_id("copper-wall")
        .ok_or_else(|| anyhow!("content is missing `copper-wall`"))?;
    let stone = content
        .block_id("stone")
        .ok_or_else(|| anyhow!("content is missing `stone`"))?;

    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.begin_edit_size(&mut world, &content, 100, 100);
    }
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        for x in 20..60 {
            for y in 20..60 {
                world.set_block(x, y, wall, 0, 0);
            }
        }
        world.set_extra_data(30, 30, 0x4321);
    }
    {
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor.resize(&mut world, &content, 80, 80, -10, -10);
    }

    // `MapEditor.resize` (`MapEditor.java:310`) re-centres the old rectangle by
    // `(old - new) / 2 = +10` before the -10 shift, so old (20..59) lands at
    // new (0..39): old (30,30) with the extra data goes to (10,10).
    let ok = grid.tiles.width == 80
        && grid.tiles.height == 80
        && grid.tiles.get(10, 10).block == wall
        && grid.tiles.get(10, 10).extra_data == 0x4321
        && grid.tiles.get(0, 0).block == wall
        && grid.tiles.get(39, 39).block == wall
        && grid.tiles.get(40, 40).block != wall
        && grid.tiles.get(0, 0).floor == stone;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_resize_shift",
        "width": grid.tiles.width,
        "height": grid.tiles.height,
        "preserved_tile": grid.tiles.get(10, 10).block.raw(),
        "preserved_extra": grid.tiles.get(10, 10).extra_data,
        "border_floor": grid.tiles.get(0, 0).floor.raw(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor resize-shift: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(exit_code(ok))
}

/// `editor gen-preview` (plan 19 M4 §5/§7b): a fixed filter stack over a
/// deterministic 64×64 snapshot produces identical preview pixels on every run;
/// a queued mid-generation re-run coalesces to the same result.
pub(super) fn cmd_editor_gen_preview(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::generate::{EditorSnapshot, generate_preview};
    use mind_core::maps::filters::{FilterRegistry, parse_filters};
    use mind_core::world::WorldGrid;

    let content = boot_content()?;
    let stone = content
        .block_id("stone")
        .ok_or_else(|| anyhow!("content is missing `stone`"))?;
    let sand = content.block_id("sand-floor").unwrap_or(stone);
    let ice = content.block_id("ice").unwrap_or(stone);
    let wall = content.block_id("copper-wall").unwrap_or(stone);

    let mut grid = WorldGrid::new(64, 64);
    for tile in grid.tiles.array_mut() {
        tile.floor = stone;
    }
    for x in 0..64 {
        for y in 0..64 {
            if (x + y) % 7 == 0 {
                grid.tiles.get_mut(x, y).floor = sand;
            }
            if (x * 3 + y) % 11 == 0 {
                grid.tiles.get_mut(x, y).floor = ice;
            }
        }
    }
    for x in 20..44 {
        grid.tiles.get_mut(x, 32).block = wall;
    }

    let filter_json = concat!(
        "[",
        "{\"class\":\"noise\",\"seed\":12345,\"scl\":25.0,\"threshold\":0.45,",
        "\"octaves\":3.0,\"falloff\":0.5,\"floor\":\"stone\",\"block\":\"stone-wall\"},",
        "{\"class\":\"scatter\",\"seed\":777,\"chance\":0.2,\"flooronto\":\"sand-floor\",",
        "\"block\":\"copper-wall\"}",
        "]"
    );
    let build = || {
        parse_filters(&content, filter_json, &FilterRegistry::vanilla())
            .unwrap_or_else(|error| panic!("fixed filter stack must parse: {error}"))
    };

    let snapshot = EditorSnapshot::capture(&grid);
    let image_a = generate_preview(&snapshot, &mut build(), &content, 0);
    // A second capture (as if an edit landed mid-generation) and re-run must
    // coalesce to the same preview when the world is unchanged.
    let snapshot_b = EditorSnapshot::capture(&grid);
    let image_b = generate_preview(&snapshot_b, &mut build(), &content, 0);

    let checksum = preview_image_checksum(&image_a);
    let checksum_b = preview_image_checksum(&image_b);
    let deterministic = image_a == image_b;
    let png = mind_core::io::map::encode_png(&image_a)?;
    let decoded = mind_core::io::map::decode_png(&png)?;
    let png_round_trip_ok = decoded == image_a;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_gen_preview",
        "width": image_a.width,
        "height": image_a.height,
        "checksum": checksum,
        "checksum_second_run": checksum_b,
        "png_bytes": png.len(),
        "deterministic": deterministic,
        "png_round_trip_ok": png_round_trip_ok,
        "ok": deterministic && png_round_trip_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor gen-preview: {checksum} deterministic={deterministic}");
    }
    Ok(exit_code(deterministic && png_round_trip_ok))
}

/// `editor playtest` (plan 19 M7 §3.11): `edit_in_game` → `resume_editing` →
/// Shift `playtest` → `try_exit` over the plan-12 `PlaySession`.
pub(super) fn cmd_editor_playtest(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::MapEditor;
    use mind_core::editor::playtest::{EditorPlayState, PlaytestOutcome};
    use mind_core::game::State;
    use mind_core::game::play::PlaySession;
    use mind_core::game::rules::Rules;
    use mind_core::game::rules_event::RulesEpoch;
    use mind_core::maps::Maps;
    use mind_core::world::NoopMapGenHooks;

    let _ = boot_content()?;

    let mut editor = MapEditor::new();
    let mut session = PlaySession::new(Rules::default());
    let mut play = EditorPlayState::new();
    play.rules.waves = true;
    let mut epoch = RulesEpoch::new();

    // 1. `editInGame`: snapshot + hidden editor gamemode + playing phase.
    let events_edit = play.edit_in_game(&mut editor, &mut session);
    let after_edit = session.phase;
    let editor_rules = session.rules.editor;
    let has_snapshot = play.last_saved_rules.is_some();

    // 2. `resumeEditing`: menu phase with the snapshot restored.
    play.resume_editing(&mut editor, &mut session);
    let after_resume = session.phase;
    let restored_waves = play.rules.waves;

    // 3. Shift `playtest`: save() is the gdext facade's job; the core state
    // machine auto-picks `sandbox` for a spawnless map and enters `Playing`.
    let map = Maps::map_for_file(
        std::path::PathBuf::from("/maps/playtest.msav"),
        16,
        16,
        "Playtest",
        true,
    );
    let outcome = play.playtest(
        &mut editor,
        &mut session,
        &map,
        &NoopMapGenHooks,
        &mut epoch,
        true,
    );
    let (mode, events_play) = match outcome {
        PlaytestOutcome::Playing { mode, events } => (mode.name().to_owned(), events),
        PlaytestOutcome::Dialog => ("dialog".to_owned(), Vec::new()),
    };
    let after_playtest = session.phase;

    // 4. `tryExit` always requires the unsaved-changes confirm.
    let exit_confirm = play.try_exit();

    // Deterministic digest of the transition sequence (no wall-clock/ids).
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u8(after_edit.as_u8());
    hasher.write_bool(editor_rules);
    hasher.write_bool(has_snapshot);
    hasher.write_u8(after_resume.as_u8());
    hasher.write_bool(restored_waves);
    hasher.write(mode.as_bytes());
    hasher.write_u32(events_edit.len() as u32);
    hasher.write_u32(events_play.len() as u32);
    hasher.write_bool(exit_confirm);
    hasher.write_u8(after_playtest.as_u8());
    let checksum = hasher.finish().to_hex();

    let ok = after_edit == State::Playing
        && editor_rules
        && has_snapshot
        && after_resume == State::Menu
        && restored_waves
        && mode == "sandbox"
        && after_playtest == State::Playing
        && exit_confirm;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_playtest",
        "phase_after_edit_in_game": after_edit.name(),
        "rules_editor": editor_rules,
        "has_snapshot": has_snapshot,
        "phase_after_resume_editing": after_resume.name(),
        "restored_waves": restored_waves,
        "auto_mode": mode,
        "phase_after_playtest": after_playtest.name(),
        "edit_events": events_edit.len(),
        "play_events": events_play.len(),
        "exit_confirm": exit_confirm,
        "checksum": checksum,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor playtest: {checksum} mode={mode} ok={ok}");
    }
    Ok(exit_code(ok))
}

/// `editor objectives` (plan 19 M5 §5/§7b): parse → serialize → parse the golden
/// objective fixture, preserving `editorPos`/`parents`, and verify every
/// descriptor name appears in the serialized JSON.
pub(super) fn cmd_editor_objectives(fixture: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::objectives::{all_class_tags, class_fields};
    use mind_core::io::json::objectives::MapObjectives;

    let text = std::fs::read_to_string(fixture)
        .with_context(|| format!("reading objectives fixture `{}`", fixture.display()))?;
    let parsed = MapObjectives::from_json(&text)
        .with_context(|| format!("parsing objectives fixture `{}`", fixture.display()))?;
    let serialized = parsed.to_json()?;
    let reparsed = MapObjectives::from_json(&serialized)?;
    let round_trip_ok = parsed == reparsed;

    let value: serde_json::Value = serde_json::from_str(&serialized)?;
    let mut editor_pos_ok = true;
    let mut parents_ok = true;
    let mut descriptor_ok = true;
    for (index, objective) in parsed.iter().enumerate() {
        let object = &value[index];
        if object.get("editorPos").and_then(|v| v.as_i64())
            != Some(objective.common().editor_pos as i64)
        {
            editor_pos_ok = false;
        }
        let expected: Vec<i64> = objective
            .common()
            .parents
            .iter()
            .map(|p| *p as i64)
            .collect();
        let actual: Vec<i64> = object
            .get("parents")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();
        if expected != actual {
            parents_ok = false;
        }
        let keys = object
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>());
        for field in class_fields(objective.class_tag()) {
            if !keys
                .as_ref()
                .is_some_and(|keys| keys.iter().any(|key| key == field.name))
            {
                log::error!(
                    "descriptor `{}` missing from serialized {}",
                    field.name,
                    objective.class_tag()
                );
                descriptor_ok = false;
            }
        }
    }
    let all_classes_covered = all_class_tags()
        .iter()
        .all(|tag| !class_fields(tag).is_empty());

    let ok = round_trip_ok && editor_pos_ok && parents_ok && descriptor_ok && all_classes_covered;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_objectives",
        "fixture": fixture.display().to_string(),
        "count": parsed.len(),
        "round_trip_ok": round_trip_ok,
        "editor_pos_ok": editor_pos_ok,
        "parents_ok": parents_ok,
        "descriptor_ok": descriptor_ok,
        "all_classes_covered": all_classes_covered,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor objectives: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(exit_code(ok))
}

/// `editor wave-graph` (plan 19 M5 §5/§7b): compute the counts/health series for
/// a fixed wave stack and compare its checksum to the committed golden.
pub(super) fn cmd_editor_wave_graph(fixture: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::wave_graph::{WaveGraphData, WaveGraphMode};
    use mind_core::game::spawn_group::SpawnGroup;

    let content = boot_content()?;
    let text = std::fs::read_to_string(fixture)
        .with_context(|| format!("reading wave-graph fixture `{}`", fixture.display()))?;
    let expected: serde_json::Value = serde_json::from_str(&text)?;
    let expected_checksum = expected
        .get("checksum")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();

    let mut dagger = SpawnGroup::new("dagger");
    dagger.unit_amount = 2;
    dagger.unit_scaling = 2.0;
    dagger.max = 40;
    let mut mace = SpawnGroup::new("mace");
    mace.begin = 2;
    mace.unit_amount = 1;
    mace.unit_scaling = 3.0;
    let groups = vec![dagger, mace];

    let resolve = |name: &str| {
        content
            .unit_by_name(name)
            .map(|def| (def.id.raw(), def.health))
    };
    let data = WaveGraphData::compute(&groups, 0, 5, resolve);
    let checksum = wave_graph_checksum(&data);
    let checksums_ok = expected_checksum.is_empty() || checksum == expected_checksum;

    // Structural invariants independent of the committed checksum.
    let structural = data.len() == 6
        && data.units.len() == 2
        && data.max >= 1
        && data.max_total >= 1
        && data.max_health >= 1.0
        && data.max_y(WaveGraphMode::Counts) >= data.max;

    let ok = checksums_ok && structural;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_wave_graph",
        "fixture": fixture.display().to_string(),
        "checksum": checksum,
        "expected_checksum": expected_checksum,
        "units": data.units.iter().map(|u| u.name.clone()).collect::<Vec<_>>(),
        "max": data.max,
        "max_total": data.max_total,
        "max_health": data.max_health,
        "checksums_ok": checksums_ok,
        "structural_ok": structural,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor wave-graph: {checksum} ok={ok}");
    }
    Ok(exit_code(ok))
}

/// FNV-1a checksum of a [`WaveGraphData`] series.
pub(super) fn wave_graph_checksum(data: &mind_core::editor::wave_graph::WaveGraphData) -> String {
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_i32(data.from);
    hasher.write_i32(data.to);
    for unit in &data.units {
        hasher.write_u16(unit.id);
        hasher.write(&unit.health.to_bits().to_le_bytes());
    }
    for row in &data.series {
        for value in row {
            hasher.write_i32(*value);
        }
    }
    for total in &data.totals {
        hasher.write_i32(*total);
    }
    for health in &data.health {
        hasher.write(&health.to_bits().to_le_bytes());
    }
    hasher.write_i32(data.max);
    hasher.write_i32(data.max_total);
    hasher.write(&data.max_health.to_bits().to_le_bytes());
    hasher.finish().to_hex()
}

/// `editor locales` (plan 19 M6 §5): map locales apply to a bundle target and
/// roll back cleanly; the `MapLocaleView` resolves objective text.
pub(super) fn cmd_editor_locales(json: bool) -> anyhow::Result<i32> {
    use mind_core::game::map_objectives::ObjectiveLocale;
    use mind_core::io::StringMap;
    use mind_core::io::json::rules::MapLocales;
    use mind_core::maps::locales::{MapLocaleView, apply_to_all, parse_json, write_json};

    let mut locales = MapLocales::new();
    let mut en = StringMap::new();
    en.insert("foo.name".to_owned(), "Foo".to_owned());
    en.insert("foo.desc".to_owned(), "A thing".to_owned());
    let mut ru = StringMap::new();
    ru.insert("foo.name".to_owned(), "Фу".to_owned());
    locales.0.insert("en".to_owned(), en);
    locales.0.insert("ru".to_owned(), ru);

    let json_tag = write_json(&locales)?;
    let round_trip = parse_json(&json_tag)? == locales;
    let applied = apply_to_all(&mut locales, "en");
    let applied_ok = locales.0["ru"]["foo.desc"] == "A thing";

    // Apply to a live target bundle, then roll back from the snapshot.
    let mut target_before = StringMap::new();
    target_before.insert("foo.name".to_owned(), "Foo".to_owned());
    let mut target = target_before.clone();
    for (key, value) in &locales.0["ru"] {
        target.insert(key.clone(), value.clone());
    }
    let applied_live = target.get("foo.name").map(String::as_str) == Some("Фу");
    target.clone_from(&target_before);
    let rolled_back = target.get("foo.name").map(String::as_str) == Some("Foo");

    let view = MapLocaleView::new(&locales, "ru");
    let view_ok = view.fetch_text("@foo.desc") == "A thing"
        && view.map_locale("foo.name").as_deref() == Some("Фу");

    let ok = round_trip && applied_ok && applied_live && rolled_back && view_ok;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_locales",
        "round_trip_ok": round_trip,
        "applied_keys": applied,
        "applied_ok": applied_ok,
        "apply_live_ok": applied_live,
        "rollback_ok": rolled_back,
        "locale_view_ok": view_ok,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor locales: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(exit_code(ok))
}

/// `editor banned` (plan 19 M6 §5): the banned sets mutate `Rules` JSON.
pub(super) fn cmd_editor_banned(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::banned::{
        BanKind, add, add_all, apply_selection, ban_set, ban_set_mut, filter_pane, remove,
        rules_json,
    };
    use mind_core::io::json::JsonIo;
    use mind_core::io::json::rules::Rules;

    let mut rules = Rules::default();
    {
        let blocks = ban_set_mut(&mut rules, BanKind::Block);
        add(blocks, "conveyor");
        add(blocks, "router");
    }
    {
        let units = ban_set_mut(&mut rules, BanKind::Unit);
        add_all(units, &["dagger".to_owned(), "mace".to_owned()]);
        remove(units, "mace");
    }
    let json_tag = rules_json(&rules)?;
    let parsed: Rules = JsonIo::read(&json_tag)?;
    let json_ok = parsed.banned_blocks.contains("conveyor")
        && parsed.banned_blocks.contains("router")
        && parsed.banned_units.contains("dagger")
        && !parsed.banned_units.contains("mace");

    let pane_selection = ban_set_mut(&mut rules, BanKind::Block);
    add(pane_selection, "router");
    let pane = filter_pane(
        &["conveyor".to_owned(), "router".to_owned()],
        ban_set(&rules, BanKind::Block),
        "",
        true,
    );
    apply_selection(
        ban_set_mut(&mut rules, BanKind::Block),
        &["copper-wall".to_owned()],
    );
    let pane_ok = pane == vec!["conveyor", "router"]
        && ban_set(&rules, BanKind::Block).contains("copper-wall");

    let ok = json_ok && pane_ok;
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_banned",
        "json_ok": json_ok,
        "pane_ok": pane_ok,
        "banned_blocks": parsed.banned_blocks.iter().cloned().collect::<Vec<_>>(),
        "banned_units": parsed.banned_units.iter().cloned().collect::<Vec<_>>(),
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor banned: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(exit_code(ok))
}

/// `editor assets` (plan 19 M6 §5/§3.12): asset zip import/export round-trip.
pub(super) fn cmd_editor_assets(json: bool) -> anyhow::Result<i32> {
    use mind_core::editor::assets::{asset_bytes, export_zip, import_zip};
    use mind_core::mods::assets::{DataAsset, DataAssetType};

    let assets = vec![
        DataAsset::patch("patch.json", r#"{"patch":true}"#),
        DataAsset::bundle("bundle.properties", "foo.name=Foo\n"),
        DataAsset::blob("icon.png", DataAssetType::Image, vec![1, 2, 3, 4], false),
    ];
    let zip = export_zip(&assets)?;
    let imported = import_zip(&zip)?;
    let round_trip = assets.iter().all(|original| {
        imported.iter().any(|candidate| {
            candidate.type_ == original.type_ && asset_bytes(candidate) == asset_bytes(original)
        })
    });
    let png_signature = zip.starts_with(b"PK");
    let ok = round_trip && png_signature && imported.len() == assets.len();
    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_assets",
        "assets": assets.len(),
        "imported": imported.len(),
        "zip_bytes": zip.len(),
        "round_trip_ok": round_trip,
        "ok": ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor assets: {}", if ok { "ok" } else { "FAIL" });
    }
    Ok(exit_code(ok))
}

/// `editor bench` (plan 19 M7 §7d): median timings for op recording, undo, fill
/// and replay over an editor grid.
pub(super) fn cmd_editor_bench(
    suite: &str,
    size: i32,
    runs: u32,
    json: bool,
) -> anyhow::Result<i32> {
    use bevy_ecs::world::World;
    use mind_core::content::BlockId;
    use mind_core::editor::grid::WorldEditorGrid;
    use mind_core::editor::{EditorGrid, EditorTool, MapEditor};
    use mind_core::util::alloc::{alloc_bytes, alloc_count, enabled as alloc_enabled};
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};
    use std::time::Instant;

    let content = boot_content()?;
    let runs = runs.max(1);
    let mut samples: Vec<f64> = Vec::with_capacity(runs as usize);

    for _ in 0..runs {
        let mut editor = MapEditor::new();
        let mut grid = WorldGrid::new(0, 0);
        let mut ecs = World::new();
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        {
            let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            editor.begin_edit_size(&mut world, &content, size, size);
        }
        let wall = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
        let start = Instant::now();
        match suite {
            "recache" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                world.recache_all();
            }
            "line" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                let len = (size - 4).max(1);
                mind_core::editor::tool::touched_line(
                    &mut editor,
                    EditorTool::Line,
                    &mut world,
                    &content,
                    2,
                    2,
                    2 + len,
                    2,
                );
                editor.flush_op();
            }
            "undo" | "mapview" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                let len = (size - 4).max(1);
                mind_core::editor::tool::touched_line(
                    &mut editor,
                    EditorTool::Line,
                    &mut world,
                    &content,
                    2,
                    2,
                    2 + len,
                    2,
                );
                editor.flush_op();
                editor.undo(&mut world, &content);
                editor.redo(&mut world, &content);
            }
            "fill" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                editor.tool = EditorTool::Fill;
                editor.draw_blocks(&mut world, &content, size / 2, size / 2);
                editor.flush_op();
            }
            "replay" => {
                let mut world =
                    WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
                editor.draw_block = wall;
                let len = (size - 4).max(1);
                for _ in 0..20 {
                    mind_core::editor::tool::touched_line(
                        &mut editor,
                        EditorTool::Line,
                        &mut world,
                        &content,
                        2,
                        2,
                        2 + len,
                        2,
                    );
                    editor.flush_op();
                }
            }
            other => return Err(anyhow!("unknown editor bench suite `{other}`")),
        }
        samples.push(start.elapsed().as_secs_f64() * 1e6);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = samples[samples.len() / 2];

    // §7d MapView draw + op recording steady-state allocation audit (plan 19):
    // after warmup, repeated line/flush/undo/redo cycles must not allocate.
    let mut alloc_delta = 0u64;
    let mut bytes_delta = 0u64;
    if suite == "mapview" {
        let wall = content.block_id("copper-wall").unwrap_or(BlockId::AIR);
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        let mut editor = MapEditor::new();
        let mut grid = WorldGrid::new(0, 0);
        let mut ecs = World::new();
        {
            let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            editor.begin_edit_size(&mut world, &content, size, size);
        }
        let len = (size - 4).max(1);
        let step = |editor: &mut MapEditor, world: &mut WorldEditorGrid| {
            editor.draw_block = wall;
            mind_core::editor::tool::touched_line(
                editor,
                EditorTool::Line,
                world,
                &content,
                2,
                2,
                2 + len,
                2,
            );
            editor.flush_op();
            editor.undo(world, &content);
            editor.redo(world, &content);
            // Frame boundary: the sim drains world events each frame; clear the
            // log (retaining capacity) so repeated draws do not reallocate.
            world.clear_events();
        };
        let mut world = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        // Warm up past the undo-stack cap so the op pool is fully seeded with
        // capacity-bearing recycled operations before the measured region.
        for _ in 0..(mind_core::editor::stack::MAX_SIZE + 1) {
            step(&mut editor, &mut world);
        }
        let before = alloc_count();
        let bytes_before = alloc_bytes();
        for _ in 0..runs {
            step(&mut editor, &mut world);
        }
        alloc_delta = alloc_count().saturating_sub(before);
        bytes_delta = alloc_bytes().saturating_sub(bytes_before);
    }
    let alloc_ok = !alloc_enabled() || alloc_delta == 0;

    let report = serde_json::json!({
        "format": 1,
        "generator": "editor_bench",
        "suite": suite,
        "size": size,
        "runs": runs,
        "p50_us": median,
        "p99_us": samples[samples.len() - 1],
        "alloc_audit_enabled": alloc_enabled(),
        "alloc_count": alloc_delta,
        "alloc_bytes": bytes_delta,
        "alloc_limit": 0,
        "alloc_ok": alloc_ok,
        "ok": alloc_ok,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if json {
        println!("{text}");
    } else {
        println!("editor bench {suite}: p50 {median:.1} us");
    }
    Ok(exit_code(alloc_ok))
}
