// SPDX-License-Identifier: GPL-3.0-only

//! Plan 15 headless input scenarios (`input` subcommand).

use std::path::Path;

use anyhow::{Result, bail};
use mind_core::content::{
    BlockId, ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use mind_core::input::{
    BindingDefault, BindingState, FocusGuards, FocusState, InputLog, InputReplay, KeyBindTable,
    LockId, PlaceMode, PlacementWorld, RawEvent,
};
use mind_core::world::build::{can_replace, valid_break, valid_place_at};
use mind_core::world::{BlockCounter, BlockTable, BuildRules, TilePos, WorldGrid};
use smallvec::SmallVec;

use crate::cli::InputCommand;

/// Runs an `input` subcommand.
pub fn run(command: &InputCommand) -> Result<()> {
    match command {
        InputCommand::Dump { json, out, golden } => dump(*json, out.as_deref(), golden.as_deref()),
        InputCommand::Replay {
            events,
            out,
            json,
            mobile,
        } => replay(events, out.as_deref(), *json, *mobile),
        InputCommand::Scenario {
            name,
            json,
            dump: out,
            golden,
        } => scenario(name, *json, out.as_deref(), golden.as_deref()),
    }
}

fn print_json(value: &serde_json::Value, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    }
}

fn emit(value: &serde_json::Value, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let text = format!("{}\n", serde_json::to_string_pretty(value)?);
    if let Some(path) = out {
        std::fs::write(path, &text)?;
    }
    if let Some(path) = golden {
        let expected = std::fs::read_to_string(path)?;
        if expected != text {
            bail!("input golden mismatch at {}", path.display());
        }
    }
    Ok(())
}

/// `input dump`: the binding registry parity table.
fn dump(json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let mut categories = serde_json::Map::new();
    for category in mind_core::input::Category::ALL {
        categories.insert(
            category.name().to_owned(),
            serde_json::Value::from(KeyBindTable::category(category).count()),
        );
    }
    let binds: Vec<serde_json::Value> = KeyBindTable::all()
        .iter()
        .map(|bind| {
            let default =
                match bind.default {
                    BindingDefault::Key(Some(name)) => serde_json::Value::from(name),
                    BindingDefault::Key(None) => serde_json::Value::Null,
                    BindingDefault::Axis { negative, positive } => serde_json::Value::from(
                        format!("{}/{}", negative.unwrap_or(""), positive.unwrap_or("")),
                    ),
                };
            serde_json::json!({
                "name": bind.name,
                "category": bind.category.map(|c| c.name()),
                "axis": matches!(bind.kind, mind_core::input::KeyKind::Axis),
                "bundle": bind.bundle_key(),
                "default": default,
            })
        })
        .collect();
    let report = serde_json::json!({
        "format": 1,
        "count": KeyBindTable::len(),
        "categories": categories,
        "binds": binds,
    });
    emit(&report, out, golden)?;
    print_json(&report, json);
    Ok(())
}

/// `input replay`: deterministic event-log replay summary (controllers land M2).
fn replay(events: &Path, out: Option<&Path>, json: bool, mobile: bool) -> Result<()> {
    let text = std::fs::read_to_string(events)?;
    let log = InputLog::from_jsonl(&text)?;
    let mut guards = FocusGuards::default();
    let replay = InputReplay::new(log.clone());
    let mut key_events = 0usize;
    let mut mouse_events = 0usize;
    let mut touch_events = 0usize;
    let mut focus_events = 0usize;
    for record in &log.records {
        match &record.ev {
            RawEvent::KeyDown { .. } | RawEvent::KeyUp { .. } => key_events += 1,
            RawEvent::MouseMove { .. } | RawEvent::MouseButton { .. } | RawEvent::Scroll { .. } => {
                mouse_events += 1
            }
            RawEvent::TouchDown { .. }
            | RawEvent::TouchUp { .. }
            | RawEvent::TouchMove { .. }
            | RawEvent::Magnify { .. } => touch_events += 1,
            RawEvent::UiFocus(focus) => {
                focus_events += 1;
                guards.focus = focus.clone();
            }
            _ => {}
        }
    }
    let last_tick = replay.last_tick();
    let report = serde_json::json!({
        "format": 1,
        "mobile": mobile || log.header.mobile,
        "events": log.len(),
        "key_events": key_events,
        "mouse_events": mouse_events,
        "touch_events": touch_events,
        "focus_events": focus_events,
        "last_tick": last_tick,
        "focus_after": guards.focus,
        "checksum": fnv1a(text.as_bytes()),
    });
    emit(&report, out, None)?;
    print_json(&report, json);
    Ok(())
}

/// Runs a named input scenario.
fn scenario(name: &str, json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    match name {
        "input_focus_guards" => focus_guards(json, out, golden),
        "placement_validation_table" => placement_validation_table(json, out, golden),
        "input_place_line_headless" => input_place_line_headless(json, out, golden),
        other => bail!("unknown input scenario `{other}`"),
    }
}

/// Focus/lock guard truth table (M0 §7a).
fn focus_guards(json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let mut cases = Vec::new();

    // locked() gates camera/world mutations, not typed UI shortcuts.
    let mut guards = FocusGuards::default();
    guards.locks.add_lock(LockId::Cutscene);
    cases.push(serde_json::json!({
        "name": "locked_gates_camera_only",
        "locked": guards.locked(),
        "blocks_camera": guards.blocks_camera(),
        "typing": guards.focus.typing(),
    }));
    guards.locks.clear();

    // Dialog blocks placement + shooting.
    guards.focus.has_dialog = true;
    cases.push(serde_json::json!({
        "name": "dialog_blocks_placement",
        "can_place": guards.can_place(),
        "can_shoot": guards.can_shoot(),
        "can_mine": guards.can_mine(),
    }));
    guards.focus.has_dialog = false;

    // Chat/console/scroll block zoom.
    guards.focus.chat_shown = true;
    cases.push(serde_json::json!({
        "name": "chat_blocks_zoom",
        "blocks_zoom": guards.blocks_zoom(),
    }));
    guards.focus.chat_shown = false;
    guards.focus.has_scroll = true;
    cases.push(serde_json::json!({
        "name": "scroll_blocks_zoom",
        "blocks_zoom": guards.blocks_zoom(),
    }));
    guards.focus = FocusState::default();

    // Menu/editor clears placement state.
    guards.focus.is_menu = true;
    cases.push(serde_json::json!({
        "name": "menu_clears_state",
        "can_place": guards.can_place(),
        "clears_placement": guards.focus.clears_placement(),
    }));
    guards.focus = FocusState::default();

    // PlaceMode parity names.
    let modes: Vec<&str> = [
        PlaceMode::None,
        PlaceMode::Breaking,
        PlaceMode::Placing,
        PlaceMode::SchematicSelect,
        PlaceMode::RebuildSelect,
    ]
    .iter()
    .map(|mode| mode.name())
    .collect();

    let report = serde_json::json!({
        "scenario": "input_focus_guards",
        "cases": cases,
        "modes": modes,
        "keybinds": BindingState::new().len(),
        "checksum": fnv1a(serde_json::to_string(&cases)?.as_bytes()),
    });
    emit(&report, out, golden)?;
    print_json(&report, json);
    Ok(())
}

/// Boots base content (same path as `MindSimHost`/the headless harness).
fn boot_content() -> Result<ContentRegistry> {
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .map_err(|error| anyhow::anyhow!("content boot: {error}"))?;
    content.init()?;
    content.post_init()?;
    Ok(content)
}

/// A `WorldGrid` + content-backed [`PlacementWorld`] for the headless oracle.
struct GridPlacementWorld {
    content: ContentRegistry,
    table: BlockTable,
    rules: BuildRules,
    counter: BlockCounter,
    grid: WorldGrid,
    team: u8,
}

impl GridPlacementWorld {
    fn boot(width: i32, height: i32) -> Result<Self> {
        let content = boot_content()?;
        let table = BlockTable::build_default(&content)
            .map_err(|error| anyhow::anyhow!("block table: {error}"))?;
        let floor = content.block_id("stone").unwrap_or(BlockId::AIR);
        let mut grid = WorldGrid::new(width, height);
        grid.fill(floor, BlockId::AIR);
        Ok(Self {
            content,
            table,
            rules: BuildRules::default(),
            counter: BlockCounter::new(),
            grid,
            team: 0,
        })
    }
}

impl PlacementWorld for GridPlacementWorld {
    fn in_bounds(&self, x: i32, y: i32) -> bool {
        self.grid.tiles.in_bounds(x, y)
    }

    fn block_at(&self, x: i32, y: i32) -> BlockId {
        self.grid.tile(x, y).block
    }

    fn floor_deep(&self, _x: i32, _y: i32) -> bool {
        false
    }

    fn always_replace(&self, x: i32, y: i32) -> bool {
        self.block_at(x, y) == BlockId::AIR
    }

    fn can_replace(&self, target: BlockId, other: BlockId) -> bool {
        let (Some(target), Some(other)) = (self.content.block(target), self.content.block(other))
        else {
            return false;
        };
        can_replace(target, other)
    }

    fn valid_place(&self, block: BlockId, x: i32, y: i32, rotation: u8) -> bool {
        valid_place_at(
            &self.content,
            &self.table,
            &self.rules,
            &self.counter,
            &self.grid,
            block,
            self.team,
            rotation,
            x,
            y,
            true,
        )
    }
}

/// `placement_validation_table`: the shared plan-07 `valid_place`/`valid_break`
/// oracle table over a mixed fixture map (golden is the source of truth).
fn placement_validation_table(json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let mut world = GridPlacementWorld::boot(64, 64)?;
    // Mixed fixture: enemy core, wall, and a placed block.
    let core = world.content.block_id("core-shard").unwrap_or(BlockId::AIR);
    let wall = world
        .content
        .block_id("copper-wall")
        .unwrap_or(BlockId::AIR);
    let conveyor = world.content.block_id("conveyor").unwrap_or(BlockId::AIR);
    world.grid.tiles.get_mut(32, 32).block = core;
    world.grid.tiles.get_mut(10, 10).block = wall;
    world.grid.tiles.get_mut(20, 20).block = conveyor;

    let block_names = [
        "conveyor",
        "router",
        "copper-wall",
        "core-shard",
        "mechanical-drill",
        "combustion-generator",
        "solar-panel",
        "battery",
    ];
    let positions: [(i32, i32, u8); 8] = [
        (0, 0, 0),
        (10, 10, 0),
        (10, 11, 0),
        (16, 16, 1),
        (20, 20, 0),
        (20, 21, 2),
        (32, 32, 0),
        (33, 33, 0),
    ];
    let mut rows = Vec::new();
    for name in block_names {
        let Some(block) = world.content.block_id(name) else {
            continue;
        };
        for (x, y, rotation) in positions {
            let place = valid_place_at(
                &world.content,
                &world.table,
                &world.rules,
                &world.counter,
                &world.grid,
                block,
                world.team,
                rotation,
                x,
                y,
                true,
            );
            let break_ok = valid_break(&world.content, &world.grid, &world.rules, world.team, x, y);
            rows.push(serde_json::json!({
                "block": name,
                "x": x,
                "y": y,
                "rot": rotation,
                "valid_place": place,
                "valid_break": break_ok,
            }));
        }
    }
    let report = serde_json::json!({
        "scenario": "placement_validation_table",
        "rows": rows,
        "checksum": fnv1a(serde_json::to_string(&rows)?.as_bytes()),
    });
    emit(&report, out, golden)?;
    print_json(&report, json);
    Ok(())
}

/// `input_place_line_headless`: placement-line planner cases over a flat map.
fn input_place_line_headless(json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let world = GridPlacementWorld::boot(64, 64)?;
    let conveyor = world.content.block_id("conveyor").unwrap_or(BlockId::AIR);
    let mut cases = Vec::new();

    let mut straight: SmallVec<[TilePos; 128]> = SmallVec::new();
    mind_core::input::normalize_line(TilePos::new(10, 10), TilePos::new(20, 10), &mut straight);
    cases.push(points_case("straight", &straight));

    let mut diagonal: SmallVec<[TilePos; 128]> = SmallVec::new();
    mind_core::input::pathfind_line(
        &world,
        true,
        true,
        Some(conveyor),
        TilePos::new(10, 12),
        TilePos::new(20, 17),
        &mut diagonal,
    );
    cases.push(points_case("conveyor_astar", &diagonal));

    let mut no_path: SmallVec<[TilePos; 128]> = SmallVec::new();
    mind_core::input::pathfind_line(
        &world,
        false,
        false,
        Some(conveyor),
        TilePos::new(10, 12),
        TilePos::new(20, 17),
        &mut no_path,
    );
    cases.push(points_case("conveyor_direct", &no_path));

    let mut rectangle: SmallVec<[TilePos; 128]> = SmallVec::new();
    mind_core::input::normalize_rectangle(
        TilePos::new(5, 5),
        TilePos::new(11, 11),
        2,
        &mut rectangle,
    );
    cases.push(points_case("rectangle_2x2", &rectangle));

    let report = serde_json::json!({
        "scenario": "input_place_line_headless",
        "cases": cases,
        "checksum": fnv1a(serde_json::to_string(&cases)?.as_bytes()),
    });
    emit(&report, out, golden)?;
    print_json(&report, json);
    Ok(())
}

fn points_case(name: &str, points: &SmallVec<[TilePos; 128]>) -> serde_json::Value {
    let coords: Vec<(i32, i32)> = points
        .iter()
        .map(|point| (point.x() as i32, point.y() as i32))
        .collect();
    serde_json::json!({
        "name": name,
        "count": coords.len(),
        "points": coords,
    })
}

/// FNV-1a 64-bit (stable report checksum; goldens are the source of truth).
pub(crate) fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
